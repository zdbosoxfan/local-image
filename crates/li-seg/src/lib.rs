//! Local segmentation on the CPU: salient-object models (U²-Net, U²-Net-P, IS-Net) and sky models
//! (PP-MobileSeg ADE20K, TinySkyNet) in ONNX form, run with `tract` (pure Rust, no GPU, no external
//! runtime). Powers Select › Subject and Sky, Remove Background (Quick), the Object Selection
//! tool's click mode, and the Library's Subject / Sky / Background masks — small, quick models
//! that need no AI server.
//!
//! Pre- and post-processing follow the rembg conventions the models were trained for (and
//! OmaPhoto's port of Compositor): premultiplied resize to the model size, normalisation by the
//! image's peak value then ImageNet mean/std (U²-Net) or mean 0.5 / std 1 (IS-Net), the first
//! output map, min–max stretch, bilinear upscale.

use std::path::Path;
use std::sync::Arc;

pub mod denoise;

use anyhow::{Context, Result, bail};
use tract_onnx::prelude::*;

/// A downloadable model: (file name, bytes, sha256, url, input size, label).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub url: &'static str,
    pub size: usize,
    pub isnet: bool,
    pub licence: &'static str,
    /// What the model finds.
    pub task: Task,
}

/// What a model finds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Task {
    /// The salient subject (U²-Net family): one foreground map.
    Subject,
    /// The sky. `classes` = 1: one logit map (sigmoid); otherwise ADE20K-style class logits where
    /// `class` is the sky and `margin` how far it must lead every other class.
    Sky { classes: usize, class: usize, margin: f32 },
    /// Monocular relative depth (Depth Anything V2, MiDaS): one map of relative inverse depth
    /// (larger = nearer), ImageNet-normalised input.
    Depth,
    /// AI Denoise ([`denoise`]): the download is a model package (a zip); `inner` is the model's
    /// path in it, with its own size and SHA-256 (checked when it is unpacked).
    Denoise { inner: &'static str, inner_bytes: u64, inner_sha256: &'static str },
}

pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "u2netp",
        label: "U²-Net small (fast, 5 MB)",
        file: "u2netp.onnx",
        bytes: 4574861,
        sha256: "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8",
        url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx",
        size: 320,
        isnet: false,
        licence: "Apache-2.0 (U²-Net, Qin et al. 2020)",
        task: Task::Subject,
    },
    ModelSpec {
        id: "u2net",
        label: "U²-Net (176 MB)",
        file: "u2net.onnx",
        bytes: 175997641,
        sha256: "8d10d2f3bb75ae3b6d527c77944fc5e7dcd94b29809d47a739a7a728a912b491",
        url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2net.onnx",
        size: 320,
        isnet: false,
        licence: "Apache-2.0 (U²-Net, Qin et al. 2020)",
        task: Task::Subject,
    },
    ModelSpec {
        id: "isnet",
        label: "IS-Net general (best edges, 179 MB)",
        file: "isnet-general-use.onnx",
        bytes: 178648008,
        sha256: "60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a",
        url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-general-use.onnx",
        size: 1024,
        isnet: true,
        licence: "Apache-2.0 (DIS / IS-Net, Qin et al. 2022)",
        task: Task::Subject,
    },
    // Sky: PaddleSeg's PP-MobileSeg-Base trained on ADE20K (sky = class 2), ONNX opset 13.
    ModelSpec {
        id: "sky-mobileseg",
        label: "Sky · PP-MobileSeg (24 MB)",
        file: "pp_mobileseg_base_ade20k_512.onnx",
        bytes: 23711565,
        sha256: "63c15451d3907472410de9417cabf2f121b62006d50b980fb2f774ceddaeec7a",
        url: "https://raw.githubusercontent.com/kisakutanaka/SkySegmentation/4f1715a9517e065d2867a724b4dc6aad914f0aca/models/pp_mobileseg_base_ade20k_512.onnx",
        size: 512,
        isnet: false,
        licence: "Apache-2.0 (PaddleSeg PP-MobileSeg; trained on ADE20K)",
        task: Task::Sky { classes: 150, class: 2, margin: 2.0 },
    },
    // Sky, tiny: a 49K-parameter UNet distilled from SkySeg (Open Images), for a quick first guess.
    ModelSpec {
        id: "sky-tiny",
        label: "Sky · TinySkyNet (0.2 MB, quick preview)",
        file: "tinyskynet_skyseg_256.onnx",
        bytes: 203485,
        sha256: "bdf304a00ff84b424ed39823ae1eb003707799d62cf2725317ea8073919aba7c",
        url: "https://raw.githubusercontent.com/kisakutanaka/SkySegmentation/4f1715a9517e065d2867a724b4dc6aad914f0aca/models/tinyskynet_skyseg_256.onnx",
        size: 256,
        isnet: false,
        licence: "MIT (TinySkyNet-SkySeg)",
        task: Task::Sky { classes: 1, class: 0, margin: 0.0 },
    },
    // Depth: Depth Anything V2 Small (Yang et al. 2024; the Small weights are Apache-2.0, the
    // larger ones are not), the TorchScript ONNX export of fabio-sim/Depth-Anything-ONNX
    // (Apache-2.0) release v2.0.0 — plain opset-17 operators that tract runs (that release's
    // other export uses ONNX local functions, which it doesn't).
    ModelSpec {
        id: "depth-anything-v2-small",
        label: "Depth · Depth Anything V2 Small (99 MB)",
        file: "depth_anything_v2_vits_dynamic.onnx",
        bytes: 99092268,
        sha256: "46c4e8eeda3a27f34701831b6a2ec7753d7b38779b215acb5633424703deed8f",
        url: "https://github.com/fabio-sim/Depth-Anything-ONNX/releases/download/v2.0.0/depth_anything_v2_vits_dynamic.onnx",
        size: 518,
        isnet: false,
        licence: "Apache-2.0 (Depth Anything V2 Small, Yang et al. 2024)",
        task: Task::Depth,
    },
    // Depth, smaller and older: MiDaS v2.1 small (Ranftl et al.), MIT, from the MiDaS release.
    ModelSpec {
        id: "depth-midas-small",
        label: "Depth · MiDaS v2.1 small (67 MB)",
        file: "midas_v21_small_256.onnx",
        bytes: 66764249,
        sha256: "2d8c6cb8f415229daf1eb041024208e2608c9f98e17c81cc7c6ecb449c56fd58",
        url: "https://github.com/isl-org/MiDaS/releases/download/v2_1/model-small.onnx",
        size: 256,
        isnet: false,
        licence: "MIT (MiDaS v2.1 small, Ranftl et al. 2020)",
        task: Task::Depth,
    },
    // AI Denoise: darktable-ai's RawNIND UtNet2 package (release-5.6.0, GPL-3.0); its linear
    // Rec.2020 variant is unpacked from it (`denoise::install_package`).
    ModelSpec {
        id: denoise::DENOISE_ID,
        label: "AI Denoise · RawNIND UtNet2 (31 MB)",
        file: "rawdenoise-nind-linear.onnx",
        bytes: 57700134,
        sha256: "d71b5f1e727c85a359e6f74dca9e2016c9d8fc3e2f7ac3e9b347d80ceca969af",
        url: "https://github.com/darktable-org/darktable-ai/releases/download/release-5.6.0/rawdenoise-nind.dtmodel",
        size: denoise::TILE,
        isnet: false,
        licence: "GPL-3.0 (RawNIND UtNet2, Brummer & De Vleeschouwer 2025; darktable-ai)",
        task: Task::Denoise {
            inner: "rawdenoise-nind/model_linear.onnx",
            inner_bytes: 31053823,
            inner_sha256: "df957efadcc152c007d5d3b0917bdff9e41c0d4a0efe56584ef30b36393cd181",
        },
    },
];

pub fn spec(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

type Runner = dyn Fn(Tensor) -> TractResult<TVec<TValue>> + Send + Sync;

/// A loaded model, ready to run (cheap to clone).
#[derive(Clone)]
pub struct Segmenter {
    spec: ModelSpec,
    run: Arc<Runner>,
}

impl std::fmt::Debug for Segmenter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Segmenter").field("model", &self.spec.id).finish()
    }
}

impl Segmenter {
    pub fn load(spec: &ModelSpec, path: &Path) -> Result<Self> {
        let s = spec.size;
        // the dynamic-shape depth export declares symbolic intermediate shapes that conflict
        // with the fixed input: let tract infer them
        let depth = spec.task == Task::Depth;
        let plan = tract_onnx::onnx()
            .with_ignore_value_info(depth)
            .with_ignore_output_shapes(depth)
            .model_for_path(path)
            .with_context(|| format!("Could not read the segmentation model {}", path.display()))?
            .with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, s, s)))?
            .into_optimized()?
            .into_runnable()?;
        Ok(Self { spec: *spec, run: Arc::new(move |t: Tensor| plan.run(tvec!(t.into()))) })
    }

    pub fn spec(&self) -> &ModelSpec {
        &self.spec
    }

    /// Foreground probability (0–1) for an RGBA8 image, at the image's size. `None` when the model
    /// sees no clear subject.
    pub fn predict(&self, rgba: &[u8], w: usize, h: usize) -> Result<Option<Vec<f32>>> {
        if rgba.len() != w * h * 4 || w == 0 || h == 0 {
            bail!("image buffer size mismatch");
        }
        let s = self.spec.size;
        let small = resize_premultiplied(rgba, w, h, s, s);
        let peak = small.iter().flat_map(|p| [p[0], p[1], p[2]]).fold(1e-6f32, f32::max);
        let (mean, std) = if self.spec.isnet { ([0.5f32; 3], [1.0f32; 3]) } else { ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225]) };
        let input = tract_ndarray::Array4::from_shape_fn((1, 3, s, s), |(_, c, y, x)| (small[y * s + x][c] / peak - mean[c]) / std[c]);
        let out = (self.run)(input.into())?;
        let map = out[0].to_plain_array_view::<f32>()?;
        let vals: Vec<f32> = map.iter().copied().collect();
        if vals.len() < s * s {
            bail!("unexpected model output");
        }
        let vals = &vals[..s * s];
        let (lo, hi) = vals.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        if hi < 0.5 {
            return Ok(None);
        }
        let span = (hi - lo).max(1e-6);
        let norm: Vec<f32> = vals.iter().map(|v| (v - lo) / span).collect();
        Ok(Some(upscale(&norm, s, s, w, h)))
    }

    /// Sky probability (0–1) for an RGBA8 image, at the image's size (a sky model). Raw
    /// probabilities, not stretched: a photo without sky stays near zero.
    pub fn predict_sky(&self, rgba: &[u8], w: usize, h: usize) -> Result<Vec<f32>> {
        let Task::Sky { classes, class, margin } = self.spec.task else { bail!("not a sky model") };
        if rgba.len() != w * h * 4 || w == 0 || h == 0 {
            bail!("image buffer size mismatch");
        }
        let s = self.spec.size;
        let small = resize_premultiplied(rgba, w, h, s, s);
        let (mean, std) = ([0.485f32, 0.456, 0.406], [0.229f32, 0.224, 0.225]);
        let input = tract_ndarray::Array4::from_shape_fn((1, 3, s, s), |(_, c, y, x)| (small[y * s + x][c] - mean[c]) / std[c]);
        let out = (self.run)(input.into())?;
        let map = out[0].to_plain_array_view::<f32>()?;
        let shape = map.shape().to_vec();
        let (oh, ow) = match shape.as_slice() {
            [_, c, oh, ow] if *c == classes => (*oh, *ow),
            other => bail!("unexpected sky model output {other:?}"),
        };
        let vals: Vec<f32> = map.iter().copied().collect();
        let plane = oh * ow;
        let sigmoid = |x: f32| 1.0 / (1.0 + (-x).exp());
        let prob: Vec<f32> = if classes == 1 {
            vals[..plane].iter().map(|v| sigmoid(*v)).collect()
        } else {
            (0..plane)
                .map(|i| {
                    let sky = vals[class * plane + i];
                    let other = (0..classes).filter(|c| *c != class).map(|c| vals[c * plane + i]).fold(f32::MIN, f32::max);
                    sigmoid(sky - other - margin)
                })
                .collect()
        };
        Ok(upscale(&prob, ow, oh, w, h))
    }

    /// Relative nearness (0 = farthest, 1 = nearest in the photo) for an RGBA8 image, at the
    /// image's size (a depth model). The model sees the photo squeezed to its square input; its
    /// relative inverse depth is min–max stretched (it has no absolute scale).
    pub fn predict_depth(&self, rgba: &[u8], w: usize, h: usize) -> Result<Vec<f32>> {
        if self.spec.task != Task::Depth {
            bail!("not a depth model");
        }
        if rgba.len() != w * h * 4 || w == 0 || h == 0 {
            bail!("image buffer size mismatch");
        }
        let s = self.spec.size;
        let small = resize_premultiplied(rgba, w, h, s, s);
        let (mean, std) = ([0.485f32, 0.456, 0.406], [0.229f32, 0.224, 0.225]);
        let input = tract_ndarray::Array4::from_shape_fn((1, 3, s, s), |(_, c, y, x)| (small[y * s + x][c] - mean[c]) / std[c]);
        let out = (self.run)(input.into())?;
        let map = out[0].to_plain_array_view::<f32>()?;
        let shape = map.shape().to_vec();
        let (oh, ow) = match shape.as_slice() {
            [_, oh, ow] | [_, 1, oh, ow] => (*oh, *ow),
            other => bail!("unexpected depth model output {other:?}"),
        };
        let vals: Vec<f32> = map.iter().copied().collect();
        let vals = &vals[..oh * ow];
        let (lo, hi) = vals.iter().filter(|v| v.is_finite()).fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        if hi <= lo {
            return Ok(vec![0.5; w * h]);
        }
        let norm: Vec<f32> = vals.iter().map(|v| if v.is_finite() { (v - lo) / (hi - lo) } else { 0.0 }).collect();
        Ok(upscale(&norm, ow, oh, w, h))
    }
}

/// Area-filtered, premultiplied (transparent reads as black) resize to `tw × th`, values 0–1.
fn resize_premultiplied(rgba: &[u8], w: usize, h: usize, tw: usize, th: usize) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0f32; 3]; tw * th];
    for ty in 0..th {
        let (y0, y1) = (ty * h / th, (((ty + 1) * h).div_ceil(th)).min(h).max(ty * h / th + 1));
        for tx in 0..tw {
            let (x0, x1) = (tx * w / tw, (((tx + 1) * w).div_ceil(tw)).min(w).max(tx * w / tw + 1));
            let mut acc = [0.0f32; 3];
            let mut n = 0.0f32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * w + x) * 4;
                    let a = rgba[i + 3] as f32 / 255.0;
                    for c in 0..3 {
                        acc[c] += rgba[i + c] as f32 / 255.0 * a;
                    }
                    n += 1.0;
                }
            }
            out[ty * tw + tx] = acc.map(|v| v / n.max(1.0));
        }
    }
    out
}

/// Bilinear upscale of a `sw × sh` map to `w × h`.
pub fn upscale(src: &[f32], sw: usize, sh: usize, w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * sh as f32 / h as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy.fract());
        let y1 = (y0 + 1).min(sh - 1);
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * sw as f32 / w as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx.fract());
            let x1 = (x0 + 1).min(sw - 1);
            let a = src[y0 * sw + x0] * (1.0 - tx) + src[y0 * sw + x1] * tx;
            let b = src[y1 * sw + x0] * (1.0 - tx) + src[y1 * sw + x1] * tx;
            out[y * w + x] = a * (1.0 - ty) + b * ty;
        }
    }
    out
}

/// The object under a click: the 8-connected region of the probability map (≥ 0.5) containing
/// `(cx, cy)`; `None` when the click is on background.
pub fn object_at(prob: &[f32], w: usize, h: usize, cx: usize, cy: usize) -> Option<Vec<f32>> {
    if cx >= w || cy >= h || prob[cy * w + cx] < 0.5 {
        return None;
    }
    let mut keep = vec![false; w * h];
    let mut stack = vec![(cx, cy)];
    keep[cy * w + cx] = true;
    while let Some((x, y)) = stack.pop() {
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if !keep[j] && prob[j] >= 0.5 {
                    keep[j] = true;
                    stack.push((nx as usize, ny as usize));
                }
            }
        }
    }
    // Keep the soft edge of the region: soft values touching it stay, the rest go.
    Some((0..w * h).map(|i| if keep[i] { prob[i] } else { 0.0 }).collect())
}

/// Where segmentation models live: `<models>/segmentation/<file>`.
pub fn model_path(models_dir: &Path, spec: &ModelSpec) -> std::path::PathBuf {
    models_dir.join("segmentation").join(spec.file)
}

/// The best installed subject model (IS-Net, then U²-Net, then U²-Net-P).
pub fn best_installed(models_dir: &Path) -> Option<(&'static ModelSpec, std::path::PathBuf)> {
    ["isnet", "u2net", "u2netp"].iter().filter_map(|id| spec(id)).map(|s| (s, model_path(models_dir, s))).find(|(_, p)| p.is_file())
}

/// The best installed sky model (PP-MobileSeg, then TinySkyNet).
pub fn best_sky(models_dir: &Path) -> Option<(&'static ModelSpec, std::path::PathBuf)> {
    ["sky-mobileseg", "sky-tiny"].iter().filter_map(|id| spec(id)).map(|s| (s, model_path(models_dir, s))).find(|(_, p)| p.is_file())
}

/// The best installed depth model (Depth Anything V2 Small, then MiDaS small).
pub fn best_depth(models_dir: &Path) -> Option<(&'static ModelSpec, std::path::PathBuf)> {
    ["depth-anything-v2-small", "depth-midas-small"].iter().filter_map(|id| spec(id)).map(|s| (s, model_path(models_dir, s))).find(|(_, p)| p.is_file())
}

/// A process-wide cache of the loaded depth model.
pub fn shared_depth(models_dir: &Path) -> Option<Segmenter> {
    cached(best_depth(models_dir)?, &DEPTH)
}

/// A process-wide cache of the loaded subject model (loading takes a moment).
pub fn shared(models_dir: &Path) -> Option<Segmenter> {
    cached(best_installed(models_dir)?, &SUBJECT)
}

/// A process-wide cache of the loaded sky model.
pub fn shared_sky(models_dir: &Path) -> Option<Segmenter> {
    cached(best_sky(models_dir)?, &SKY)
}

type Slot = std::sync::Mutex<Option<(String, Segmenter)>>;
static SUBJECT: Slot = std::sync::Mutex::new(None);
static SKY: Slot = std::sync::Mutex::new(None);
static DEPTH: Slot = std::sync::Mutex::new(None);

fn cached((spec, path): (&'static ModelSpec, std::path::PathBuf), slot: &Slot) -> Option<Segmenter> {
    let mut g = slot.lock().ok()?;
    if let Some((id, s)) = g.as_ref()
        && id == spec.id
    {
        return Some(s.clone());
    }
    let s = Segmenter::load(spec, &path).ok()?;
    *g = Some((spec.id.to_owned(), s.clone()));
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_flood_keeps_the_clicked_region_only() {
        let (w, h) = (10, 4);
        let mut p = vec![0.0f32; w * h];
        for y in 0..4 {
            p[y * w + 1] = 1.0;
            p[y * w + 2] = 0.9;
            p[y * w + 7] = 1.0;
        }
        let o = object_at(&p, w, h, 1, 1).unwrap();
        assert!(o[1] > 0.0 && o[2] > 0.0 && o[7] == 0.0);
        assert!(object_at(&p, w, h, 5, 1).is_none());
    }

    /// Runs the small model on a synthetic subject when it's available (downloaded by CI or the
    /// developer to `LI_SEG_TEST_MODELS`).
    #[test]
    fn small_model_finds_a_centred_subject() {
        let Some(dir) = std::env::var_os("LI_SEG_TEST_MODELS") else { return };
        let spec = spec("u2netp").unwrap();
        let path = Path::new(&dir).join(spec.file);
        if !path.is_file() {
            return;
        }
        let seg = Segmenter::load(spec, &path).unwrap();
        let (w, h) = (256, 192);
        let mut img = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let inside = ((x as f32 - 128.0) / 60.0).powi(2) + ((y as f32 - 96.0) / 70.0).powi(2) < 1.0;
                let c = if inside { [220, 60, 40] } else { [70 + (x % 9) as u8, 120, 160] };
                img[i..i + 3].copy_from_slice(&c);
                img[i + 3] = 255;
            }
        }
        let p = seg.predict(&img, w, h).unwrap().expect("a subject");
        assert!(p[96 * w + 128] > 0.5, "centre {}", p[96 * w + 128]);
        assert!(p[5 * w + 5] < 0.5, "corner {}", p[5 * w + 5]);
    }

    /// The sky model on a synthetic landscape when available (`LI_SEG_TEST_MODELS` holding the
    /// PP-MobileSeg or TinySkyNet file).
    #[test]
    fn sky_model_finds_the_sky() {
        let Some(dir) = std::env::var_os("LI_SEG_TEST_MODELS") else { return };
        for id in ["sky-mobileseg", "sky-tiny"] {
            let spec = spec(id).unwrap();
            let path = Path::new(&dir).join(spec.file);
            if !path.is_file() {
                continue;
            }
            let seg = Segmenter::load(spec, &path).unwrap();
            let (w, h) = (320, 240);
            let mut img = vec![0u8; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) * 4;
                    let c = if y < 110 { [90 + (y / 3) as u8, 150 + (y / 4) as u8, 230] } else { [70 + (x % 13) as u8, 110 + (y % 7) as u8, 50] };
                    img[i..i + 3].copy_from_slice(&c);
                    img[i + 3] = 255;
                }
            }
            let p = seg.predict_sky(&img, w, h).unwrap();
            assert!(p[30 * w + 160] > 0.5, "{id}: sky {}", p[30 * w + 160]);
            assert!(p[200 * w + 160] < 0.5, "{id}: ground {}", p[200 * w + 160]);
        }
    }

    #[test]
    fn subject_and_sky_models_are_told_apart() {
        assert!(MODELS.iter().filter(|m| matches!(m.task, Task::Sky { .. })).count() >= 2);
        assert!(best_installed(Path::new("/nonexistent")).is_none() && best_sky(Path::new("/nonexistent")).is_none());
        assert!(best_depth(Path::new("/nonexistent")).is_none());
        // the subject / sky choosers never pick a depth model
        for id in ["depth-anything-v2-small", "depth-midas-small"] {
            assert_eq!(spec(id).map(|s| s.task), Some(Task::Depth), "{id}");
        }
    }

    /// A photo-like scene with depth cues: a checkered ground plane receding to the horizon
    /// (in perspective) under a plain sky, and a box standing on the near ground.
    fn ground_scene(w: usize, h: usize) -> Vec<u8> {
        let horizon = h as f32 * 0.4;
        let mut img = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let c = if (y as f32) < horizon {
                    [150 + (y * 60 / h) as u8, 185, 235]
                } else {
                    // ground point at distance z ∝ 1 / (y − horizon)
                    let dy = (y as f32 - horizon + 0.5).max(0.5);
                    let z = 40.0 / dy;
                    let gx = (x as f32 - w as f32 / 2.0) * z / 40.0;
                    let check = ((gx.floor() as i64 + (z * 2.0).floor() as i64) & 1) == 0;
                    let fog = (z / 4.0).min(1.0);
                    let base = if check { [120.0, 100.0, 70.0] } else { [70.0, 60.0, 45.0] };
                    std::array::from_fn(|k| (base[k] * (1.0 - fog) + [150.0, 175.0, 210.0][k] * fog) as u8)
                };
                let in_box = x > w * 2 / 5 && x < w * 3 / 5 && y > h * 3 / 5 && y < h * 9 / 10;
                let c = if in_box { [200, 40, 40] } else { c };
                img[i..i + 3].copy_from_slice(&c);
                img[i + 3] = 255;
            }
        }
        img
    }

    /// The depth models run under tract and see the near ground as nearer than the horizon
    /// (`LI_SEG_TEST_MODELS` holding the Depth Anything V2 Small and/or MiDaS small file).
    #[test]
    fn depth_models_find_the_near_ground() {
        let Some(dir) = std::env::var_os("LI_SEG_TEST_MODELS") else { return };
        for id in ["depth-anything-v2-small", "depth-midas-small"] {
            let spec = spec(id).unwrap();
            let path = Path::new(&dir).join(spec.file);
            if !path.is_file() {
                continue;
            }
            let t = std::time::Instant::now();
            let seg = Segmenter::load(spec, &path).unwrap();
            let loaded = t.elapsed();
            let (w, h) = (384, 288);
            let img = ground_scene(w, h);
            let t = std::time::Instant::now();
            let d = seg.predict_depth(&img, w, h).unwrap();
            eprintln!("{id}: load {loaded:?}, predict {:?}", t.elapsed());
            assert_eq!(d.len(), w * h);
            assert!(d.iter().all(|v| (0.0..=1.0).contains(v)));
            let at = |x: usize, y: usize| d[y * w + x];
            let (near, far) = (at(w / 8, h - 6), at(w / 8, (h as f32 * 0.42) as usize));
            assert!(near > far + 0.2, "{id}: near ground {near}, horizon {far}");
            assert!(at(w / 2, h * 3 / 4) > far, "{id}: the box is nearer than the horizon");
            assert!(seg.predict(&img, w, h).is_ok() || seg.predict_sky(&img, w, h).is_err());
        }
    }
}
