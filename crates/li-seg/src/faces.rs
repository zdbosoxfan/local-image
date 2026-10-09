//! Opt-in face inference on the CPU, with no process-wide state or persistence. The caller owns
//! the models and decides whether face analysis is enabled. Coordinates are pixels in the full,
//! oriented input; the engine can normalise them when storing MWG regions.
//!
//! The YuNet decoding equations and ArcFace alignment reference are described in Smart Sort
//! §5.2, following OpenCV's Apache-2.0 `modules/objdetect/src/face_detect.cpp`
//! (52100328d82d0502534323e9524a701baa3a1e2a) and `face_recognize.cpp`
//! (13c571a801ad5c67a752e5cd58a8a7e7725f99d2). This is an independent Rust implementation
//! of those equations, not copied upstream code. SFace's pinned ONNX graph includes its own
//! pixel normalisation: feed RGB 0..255, not an additional (x - 127.5) / 128 transform.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, ensure};
use tract_onnx::prelude::*;

mod geometry;
pub use geometry::{Affine, Letterbox, REFERENCE_LANDMARKS, letterbox, similarity_transform, warp_affine_112};
mod cluster;
pub use cluster::{Assignment, Cluster, FaceNode, NamedPerson, NodeKey, assign_cluster, assign_person, chinese_whispers, cosine, normalise_embedding};

/// Download metadata only; registration in the model manager belongs to Phase 3b.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceModelFile {
    pub file: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub licence: &'static str,
}

pub const YUNET: FaceModelFile = FaceModelFile {
    file: "face_detection_yunet_2023mar.onnx",
    url: "https://huggingface.co/opencv/face_detection_yunet/resolve/3cc26e7f1014a5ee5d74a42acee58bafc9d0a310/face_detection_yunet_2023mar.onnx",
    bytes: 232589,
    sha256: "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4",
    licence: "MIT",
};

pub const SFACE: FaceModelFile = FaceModelFile {
    file: "face_recognition_sface_2021dec.onnx",
    url: "https://huggingface.co/opencv/face_recognition_sface/resolve/3d7082438a6e4551e840c9b2bb60b71e8da4b524/face_recognition_sface_2021dec.onnx",
    bytes: 38696353,
    sha256: "0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79",
    licence: "Apache-2.0",
};

pub const DETECT_LONG_EDGE: usize = 1024;
pub const SCORE_MIN: f32 = 0.9;
pub const SHORT_SIDE_MIN: f32 = 40.0;
pub const NMS_IOU: f32 = 0.3;
pub const NMS_TOP_K: usize = 5000;
pub const CLUSTER_COSINE: f32 = 0.50;
pub const ASSIGN_COSINE: f32 = 0.45;
pub const ASSIGN_MARGIN: f32 = 0.05;
pub const CLUSTER_PASSES: usize = 30;

/// Interleaved RGB. Float samples use 0..1 (finite HDR samples are clamped to that range).
#[derive(Clone, Copy, Debug)]
pub enum RgbPixels<'a> {
    U8(&'a [u8]),
    U16(&'a [u16]),
    F32(&'a [f32]),
}

#[derive(Clone, Copy, Debug)]
pub struct FaceImage<'a> {
    pub pixels: RgbPixels<'a>,
    pub width: usize,
    pub height: usize,
}

impl FaceImage<'_> {
    pub fn validate(self) -> Result<()> {
        ensure!(self.width > 0 && self.height > 0, "Face input must have nonzero dimensions");
        let count = self.width.checked_mul(self.height).and_then(|n| n.checked_mul(3)).context("Face input dimensions overflow")?;
        let len = match self.pixels {
            RgbPixels::U8(p) => p.len(),
            RgbPixels::U16(p) => p.len(),
            RgbPixels::F32(p) => {
                ensure!(p.iter().all(|v| v.is_finite()), "Face input contains nonfinite samples");
                p.len()
            }
        };
        ensure!(len == count, "Face input length does not match its RGB dimensions");
        Ok(())
    }

    /// One sample in the model's 0..255 range; validation is done once at the API boundary.
    fn sample(self, x: usize, y: usize, channel: usize) -> f32 {
        let i = (y * self.width + x) * 3 + channel;
        match self.pixels {
            RgbPixels::U8(p) => f32::from(p[i]),
            RgbPixels::U16(p) => f32::from(p[i]) * (255.0 / 65535.0),
            RgbPixels::F32(p) => p[i].clamp(0.0, 1.0) * 255.0,
        }
    }
}

/// Detection before embedding. Rectangles and landmarks are in the same pixel coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
    pub landmarks: [[f32; 2]; 5],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    /// x, y, width, height in original-image pixels, clipped to its bounds.
    pub rect: [f32; 4],
    pub landmarks: [[f32; 2]; 5],
    pub score: f32,
    pub embedding: [f32; 128],
}

/// Flat row-major tensors at strides 8, 16 and 32, in that order.
#[derive(Clone, Debug, Default)]
pub struct YunetOutputs {
    pub cls: [Vec<f32>; 3],
    pub obj: [Vec<f32>; 3],
    pub bbox: [Vec<f32>; 3],
    pub kps: [Vec<f32>; 3],
}

/// Decode all finite candidates at or above `score_min`. Bad tensor lengths are errors rather
/// than silently discarding a model whose output layout is incompatible.
pub fn decode_yunet(outputs: &YunetOutputs, pad_w: usize, pad_h: usize, score_min: f32) -> Result<Vec<FaceBox>> {
    ensure!(pad_w > 0 && pad_h > 0 && pad_w.is_multiple_of(32) && pad_h.is_multiple_of(32), "YuNet input must be padded to multiples of 32");
    ensure!(score_min.is_finite() && (0.0..=1.0).contains(&score_min), "Invalid YuNet score threshold");
    let mut faces = Vec::new();
    for (level, stride) in [8, 16, 32].into_iter().enumerate() {
        let cols = pad_w / stride;
        let count = cols.checked_mul(pad_h / stride).context("YuNet output dimensions overflow")?;
        ensure!(
            outputs.cls[level].len() == count
                && outputs.obj[level].len() == count
                && count.checked_mul(4) == Some(outputs.bbox[level].len())
                && count.checked_mul(10) == Some(outputs.kps[level].len()),
            "Invalid YuNet output lengths at stride {stride}"
        );
        for idx in 0..count {
            let cls = outputs.cls[level][idx];
            let obj = outputs.obj[level][idx];
            if !cls.is_finite() || !obj.is_finite() {
                continue;
            }
            let score = (cls.clamp(0.0, 1.0) * obj.clamp(0.0, 1.0)).sqrt();
            if score < score_min {
                continue;
            }
            let b = &outputs.bbox[level][4 * idx..4 * idx + 4];
            let k = &outputs.kps[level][10 * idx..10 * idx + 10];
            if !b.iter().chain(k).all(|v| v.is_finite()) {
                continue;
            }
            let s = stride as f32;
            let c = (idx % cols) as f32;
            let r = (idx / cols) as f32;
            let w = b[2].exp() * s;
            let h = b[3].exp() * s;
            let x = (c + b[0]) * s - w / 2.0;
            let y = (r + b[1]) * s - h / 2.0;
            let landmarks = std::array::from_fn(|n| [(k[2 * n] + c) * s, (k[2 * n + 1] + r) * s]);
            if [x, y, w, h].iter().chain(landmarks.iter().flatten()).all(|v| v.is_finite()) && w > 0.0 && h > 0.0 {
                faces.push(FaceBox { x, y, w, h, score, landmarks });
            }
        }
    }
    Ok(faces)
}

pub fn intersection_over_union(a: &FaceBox, b: &FaceBox) -> f32 {
    let w = ((a.x + a.w).min(b.x + b.w) - a.x.max(b.x)).max(0.0) as f64;
    let h = ((a.y + a.h).min(b.y + b.h) - a.y.max(b.y)).max(0.0) as f64;
    let intersection = w * h;
    let union = f64::from(a.w) * f64::from(a.h) + f64::from(b.w) * f64::from(b.h) - intersection;
    if union > 0.0 { (intersection / union) as f32 } else { 0.0 }
}

/// Stable score order preserves stride/cell order at equal scores. `top_k` bounds the
/// candidates *before* suppression, matching OpenCV, rather than the number of survivors.
pub fn nms(mut faces: Vec<FaceBox>, iou: f32, top_k: usize) -> Vec<FaceBox> {
    faces.retain(|f| f.score.is_finite() && [f.x, f.y, f.w, f.h].iter().all(|v| v.is_finite()) && f.w > 0.0 && f.h > 0.0);
    faces.sort_by(|a, b| b.score.total_cmp(&a.score));
    if top_k > 0 {
        faces.truncate(top_k);
    }
    let mut keep = Vec::new();
    for face in faces {
        if keep.iter().all(|other| intersection_over_union(&face, other) <= iou) {
            keep.push(face);
        }
    }
    keep
}

type Runner = dyn Fn(Tensor) -> TractResult<TVec<TValue>> + Send + Sync;
type DetectorPlans = BTreeMap<(usize, usize), Arc<Runner>>;
const OUTPUT_NAMES: [&str; 12] = ["cls_8", "cls_16", "cls_32", "obj_8", "obj_16", "obj_32", "bbox_8", "bbox_16", "bbox_32", "kps_8", "kps_16", "kps_32"];

/// Cheap to share via Arc. Each inference run has its own tract state; only plan creation is
/// locked. The per-instance cache retains at most four shapes (no global model cache).
pub struct FaceModels {
    detector: InferenceModel,
    detector_plans: Mutex<DetectorPlans>,
    embedder: Arc<Runner>,
}

impl std::fmt::Debug for FaceModels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FaceModels").finish_non_exhaustive()
    }
}

impl FaceModels {
    /// Load `<dir>/segmentation/<file>`, the app's model layout. A directory containing the
    /// two files directly is also accepted for coordinator/reference harnesses.
    /// Download size/hash verification is the model manager's responsibility.
    pub fn load(dir: &Path) -> Result<Self> {
        let dir = if dir.join(YUNET.file).is_file() { dir.to_path_buf() } else { dir.join("segmentation") };
        Self::load_files(&dir.join(YUNET.file), &dir.join(SFACE.file))
    }

    pub fn load_files(detector: &Path, embedder: &Path) -> Result<Self> {
        let mut detector =
            tract_onnx::onnx().with_ignore_output_shapes(true).with_ignore_value_info(true).model_for_path(detector).context("Could not load YuNet")?;
        ensure!(detector.input_outlets()?.len() == 1 && detector.output_outlets()?.len() == 12, "YuNet must have one input and twelve outputs");
        // Resolve by ONNX output name, never rely on the file's output order.
        detector.select_outputs_by_name(OUTPUT_NAMES).context("Missing YuNet output names")?;
        let embedder = tract_onnx::onnx().model_for_path(embedder).context("Could not load SFace")?;
        ensure!(embedder.input_outlets()?.len() == 1 && embedder.output_outlets()?.len() == 1, "SFace must have one input and one output");
        let plan = embedder.with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, 112, 112)))?.into_optimized()?.into_runnable()?;
        let models = Self { detector, detector_plans: Mutex::new(BTreeMap::new()), embedder: Arc::new(move |input| plan.run(tvec!(input.into()))) };
        // Fail at load time for an incompatible detector, and warm the common square plan.
        models.detector_plan(1024, 1024)?;
        Ok(models)
    }

    fn detector_plan(&self, width: usize, height: usize) -> Result<Arc<Runner>> {
        let mut plans = self.detector_plans.lock().map_err(|_| anyhow::anyhow!("YuNet plan cache lock poisoned"))?;
        if let Some(plan) = plans.get(&(width, height)) {
            return Ok(Arc::clone(plan));
        }
        let plan = self
            .detector
            .clone()
            .with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, height, width)))?
            .into_optimized()?
            .into_runnable()?;
        let run: Arc<Runner> = Arc::new(move |input| plan.run(tvec!(input.into())));
        if plans.len() >= 4 {
            // Eviction is deterministic; outstanding runs retain their own Arc.
            if let Some(key) = plans.keys().next().copied() {
                plans.remove(&key);
            }
        }
        plans.insert((width, height), Arc::clone(&run));
        Ok(run)
    }

    /// Detect, filter on the 1024 input, suppress overlaps, and embed from the original
    /// resolution. Errors contain no face/embedding data and are propagated to the caller.
    pub fn faces(&self, image: FaceImage<'_>) -> Result<Vec<Face>> {
        let boxes = self.detect(image)?;
        boxes
            .into_iter()
            .map(|b| {
                let embedding = self.embed(image, &b)?;
                Ok(Face { rect: [b.x, b.y, b.w, b.h], landmarks: b.landmarks, score: b.score, embedding })
            })
            .collect()
    }

    pub fn detect(&self, image: FaceImage<'_>) -> Result<Vec<FaceBox>> {
        let (input, layout) = letterbox(image)?;
        let tensor = Tensor::from_shape(&[1, 3, layout.pad_h, layout.pad_w], &input)?;
        let values = self.detector_plan(layout.pad_w, layout.pad_h)?(tensor)?;
        let outputs = outputs_from_values(&values, layout.pad_w, layout.pad_h)?;
        let mut candidates = decode_yunet(&outputs, layout.pad_w, layout.pad_h, SCORE_MIN)?;
        for b in &mut candidates {
            let right = (b.x + b.w).clamp(0.0, layout.resized_w as f32);
            let bottom = (b.y + b.h).clamp(0.0, layout.resized_h as f32);
            b.x = b.x.clamp(0.0, layout.resized_w as f32);
            b.y = b.y.clamp(0.0, layout.resized_h as f32);
            b.w = right - b.x;
            b.h = bottom - b.y;
        }
        candidates.retain(|b| b.w.min(b.h) >= SHORT_SIDE_MIN);
        let mut boxes = nms(candidates, NMS_IOU, NMS_TOP_K);
        for b in &mut boxes {
            b.x = (b.x / layout.scale).min(image.width as f32);
            b.y = (b.y / layout.scale).min(image.height as f32);
            b.w = (b.w / layout.scale).min(image.width as f32 - b.x);
            b.h = (b.h / layout.scale).min(image.height as f32 - b.y);
            b.landmarks = b.landmarks.map(|p| layout.to_original(p));
        }
        Ok(boxes)
    }

    pub fn embed(&self, image: FaceImage<'_>, face: &FaceBox) -> Result<[f32; 128]> {
        let matrix = similarity_transform(&face.landmarks)?;
        let rgb = warp_affine_112(image, &matrix)?;
        let mut nchw = vec![0.0f32; 3 * 112 * 112];
        for (i, pixel) in rgb.as_chunks::<3>().0.iter().enumerate() {
            for c in 0..3 {
                nchw[c * 112 * 112 + i] = pixel[c];
            }
        }
        let out = (self.embedder)(Tensor::from_shape(&[1, 3, 112, 112], &nchw)?)?;
        ensure!(out.len() == 1 && out[0].shape() == [1, 128], "Invalid SFace output shape (expected 1×128)");
        let view = out[0].to_plain_array_view::<f32>().context("SFace output must be f32")?;
        let mut embedding = [0.0f32; 128];
        for (dst, src) in embedding.iter_mut().zip(view.iter()) {
            *dst = *src;
        }
        normalise_embedding(&embedding).context("Invalid SFace embedding")
    }
}

fn outputs_from_values(values: &[TValue], width: usize, height: usize) -> Result<YunetOutputs> {
    ensure!(values.len() == 12, "Invalid YuNet output count");
    let mut out = YunetOutputs::default();
    for (i, stride) in [8, 16, 32].into_iter().enumerate() {
        let count = (width / stride) * (height / stride);
        for (offset, channels, dst) in [(0, 1, &mut out.cls[i]), (3, 1, &mut out.obj[i]), (6, 4, &mut out.bbox[i]), (9, 10, &mut out.kps[i])] {
            let value = &values[offset + i];
            ensure!(value.shape() == [1, count, channels], "Invalid YuNet shape for {}", OUTPUT_NAMES[offset + i]);
            *dst = value.to_plain_array_view::<f32>().context("YuNet output must be f32")?.iter().copied().collect();
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
