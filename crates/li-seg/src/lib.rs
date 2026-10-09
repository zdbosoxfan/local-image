//! Local segmentation on the CPU: one small ONNX model per function (IS-Net for the subject,
//! PP-MobileSeg for the sky, Depth Anything V2 Small for depth), run with `tract` (pure Rust, no
//! GPU, no external runtime), or a custom `.onnx` file the user picks for a function. Powers
//! Select › Subject and Sky, Remove Background (Quick), the Object Selection tool's click mode,
//! and the Library's Subject / Sky / Background / Depth masks — small, quick models that need no
//! AI server.
//!
//! Pre- and post-processing follow the rembg conventions the models were trained for (and
//! OmaPhoto's port of Compositor): premultiplied resize to the model size, normalisation by the
//! image's peak value then mean 0.5 / std 1 (IS-Net) or ImageNet mean/std (U²-Net style), the
//! first output map, min–max stretch, bilinear upscale.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use tract_onnx::prelude::*;
use tract_onnx::tract_hir::infer::Factoid;

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
    /// The function Settings lists it under; `None`: listed on its own.
    pub group: Option<Group>,
    /// Plain-language description: what it does, the speed / quality trade-off.
    pub about: &'static str,
}

/// A function the on-device models serve (Settings › Local AI lists one row per function).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    /// The salient subject: Select Subject, Remove Background (Quick), Object Selection.
    Subject,
    Sky,
    Depth,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Subject, Group::Sky, Group::Depth];

    pub fn label(self) -> &'static str {
        match self {
            Group::Subject => "Subject & Background",
            Group::Sky => "Sky",
            Group::Depth => "Depth",
        }
    }

    /// A short stable key (custom-model file names, the custom-model list).
    pub fn key(self) -> &'static str {
        match self {
            Group::Subject => "subject",
            Group::Sky => "sky",
            Group::Depth => "depth",
        }
    }

    /// What the function does and where the app uses it.
    pub fn about(self) -> &'static str {
        match self {
            Group::Subject => {
                "Finds the main subject of a photo. Used by Select › Subject, Remove Background (Quick), the Object Selection tool's click mode, and the Library's Subject and Background masks."
            }
            Group::Sky => "Finds the sky. Used by Select › Sky and the Library's sky masks.",
            Group::Depth => {
                "Estimates how far away each part of a photo is. Used by the Library's depth masks (e.g. to darken or blur the background, or pick the foreground by distance)."
            }
        }
    }

    /// The official model of this function.
    pub fn official(self) -> &'static ModelSpec {
        let id = match self {
            Group::Subject => "isnet",
            Group::Sky => "sky-mobileseg",
            Group::Depth => "depth-anything-v2-small",
        };
        MODELS.iter().find(|m| m.id == id).expect("the official model is in MODELS")
    }

    /// The model the app uses for this function: the custom one when set, else the official one
    /// when it is installed.
    pub fn in_use(self, models_dir: &Path) -> Option<InUse> {
        active(self, models_dir).map(|a| a.in_use)
    }
}

/// Which model serves a function right now.
#[derive(Clone, Debug, PartialEq)]
pub enum InUse {
    Official(&'static ModelSpec),
    Custom(Custom),
}

/// How a subject model wants its input normalised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Norm {
    /// IS-Net style: mean 0.5, std 1.
    IsNet,
    /// U²-Net / ImageNet style: ImageNet mean and std.
    ImageNet,
}

impl Norm {
    pub const ALL: [Norm; 2] = [Norm::IsNet, Norm::ImageNet];

    pub fn label(self) -> &'static str {
        match self {
            Norm::IsNet => "IS-Net style",
            Norm::ImageNet => "U²-Net / ImageNet style",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Norm::IsNet => "isnet",
            Norm::ImageNet => "imagenet",
        }
    }

    fn from_key(k: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|n| n.key() == k)
    }

    fn mean_std(self) -> ([f32; 3], [f32; 3]) {
        match self {
            Norm::IsNet => ([0.5; 3], [1.0; 3]),
            Norm::ImageNet => ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225]),
        }
    }
}

/// What a model finds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Task {
    /// The salient subject: one foreground map.
    Subject,
    /// The sky. `classes` = 1: one logit map (sigmoid); otherwise ADE20K-style class logits where
    /// `class` is the sky and `margin` how far it must lead every other class.
    Sky { classes: usize, class: usize, margin: f32 },
    /// Monocular relative depth (Depth Anything V2): one map of relative inverse depth
    /// (larger = nearer), ImageNet-normalised input.
    Depth,
}

pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "isnet",
        label: "IS-Net general",
        file: "isnet-general-use.onnx",
        bytes: 178648008,
        sha256: "60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a",
        url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-general-use.onnx",
        size: 1024,
        isnet: true,
        licence: "Apache-2.0 (DIS / IS-Net, Qin et al. 2022)",
        task: Task::Subject,
        group: Some(Group::Subject),
        about: "Finds the subject with the finest edges (hair, fur, thin details). The best quality for Select Subject and Remove Background; takes a second or two per photo.",
    },
    // Sky: PaddleSeg's PP-MobileSeg-Base trained on ADE20K (sky = class 2), ONNX opset 13.
    ModelSpec {
        id: "sky-mobileseg",
        label: "PP-MobileSeg",
        file: "pp_mobileseg_base_ade20k_512.onnx",
        bytes: 23711565,
        sha256: "63c15451d3907472410de9417cabf2f121b62006d50b980fb2f774ceddaeec7a",
        url: "https://raw.githubusercontent.com/kisakutanaka/SkySegmentation/4f1715a9517e065d2867a724b4dc6aad914f0aca/models/pp_mobileseg_base_ade20k_512.onnx",
        size: 512,
        isnet: false,
        licence: "Apache-2.0 (PaddleSeg PP-MobileSeg; trained on ADE20K)",
        task: Task::Sky { classes: 150, class: 2, margin: 2.0 },
        group: Some(Group::Sky),
        about: "Finds the sky precisely, including around trees, buildings and the horizon. A scene-parsing model that knows 150 kinds of things, so tricky skies work well. Takes a second or two per photo.",
    },
    // Depth: Depth Anything V2 Small (Yang et al. 2024; the Small weights are Apache-2.0, the
    // larger ones are not), the TorchScript ONNX export of fabio-sim/Depth-Anything-ONNX
    // (Apache-2.0) release v2.0.0 — plain opset-17 operators that tract runs (that release's
    // other export uses ONNX local functions, which it doesn't).
    ModelSpec {
        id: "depth-anything-v2-small",
        label: "Depth Anything V2 Small",
        file: "depth_anything_v2_vits_dynamic.onnx",
        bytes: 99092268,
        sha256: "46c4e8eeda3a27f34701831b6a2ec7753d7b38779b215acb5633424703deed8f",
        url: "https://github.com/fabio-sim/Depth-Anything-ONNX/releases/download/v2.0.0/depth_anything_v2_vits_dynamic.onnx",
        size: 518,
        isnet: false,
        licence: "Apache-2.0 (Depth Anything V2 Small, Yang et al. 2024)",
        task: Task::Depth,
        group: Some(Group::Depth),
        about: "Clean edges between near and far, good on most photos. Takes a few seconds per photo.",
    },
];

/// Files of models the app used to offer (U²-Net small and full, TinySkyNet, MiDaS small): no
/// longer used, left on disk until the user removes them. (file name, label)
pub const LEGACY: &[(&str, &str)] = &[
    ("u2netp.onnx", "U²-Net small"),
    ("u2net.onnx", "U²-Net"),
    ("tinyskynet_skyseg_256.onnx", "TinySkyNet"),
    ("midas_v21_small_256.onnx", "MiDaS v2.1 small"),
];

pub fn spec(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

type Runner = dyn Fn(Tensor) -> TractResult<TVec<TValue>> + Send + Sync;

/// How to run a model: official or custom.
#[derive(Clone, Debug)]
struct Config {
    id: String,
    size: usize,
    norm: Norm,
    task: Task,
}

impl Config {
    fn of_spec(spec: &ModelSpec) -> Self {
        Self { id: spec.id.to_owned(), size: spec.size, norm: if spec.isnet { Norm::IsNet } else { Norm::ImageNet }, task: spec.task }
    }
}

/// A loaded model, ready to run (cheap to clone).
#[derive(Clone)]
pub struct Segmenter {
    cfg: Config,
    run: Arc<Runner>,
}

impl std::fmt::Debug for Segmenter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Segmenter").field("model", &self.cfg.id).finish()
    }
}

/// The map's height and width when it is a single map: `[n, 1, h, w]`, `[n, h, w]` or `[h, w]`.
fn single_map(shape: &[usize]) -> Option<(usize, usize)> {
    match shape {
        [_, 1, h, w] | [_, h, w] | [h, w] => Some((*h, *w)),
        _ => None,
    }
}

impl Segmenter {
    pub fn load(spec: &ModelSpec, path: &Path) -> Result<Self> {
        Self::load_cfg(Config::of_spec(spec), path)
    }

    fn load_cfg(cfg: Config, path: &Path) -> Result<Self> {
        let s = cfg.size;
        // the dynamic-shape depth export declares symbolic intermediate shapes that conflict
        // with the fixed input: let tract infer them
        let depth = cfg.task == Task::Depth;
        let plan = tract_onnx::onnx()
            .with_ignore_value_info(depth)
            .with_ignore_output_shapes(depth)
            .model_for_path(path)
            .with_context(|| format!("Could not read the segmentation model {}", path.display()))?
            .with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, s, s)))?
            .into_optimized()?
            .into_runnable()?;
        Ok(Self { cfg, run: Arc::new(move |t: Tensor| plan.run(tvec!(t.into()))) })
    }

    /// The model's id (an official id, or `custom:<file>`).
    pub fn id(&self) -> &str {
        &self.cfg.id
    }

    /// The shape of the model's first output for a blank input.
    fn output_shape(&self) -> Result<Vec<usize>> {
        let s = self.cfg.size;
        let out = (self.run)(tract_ndarray::Array4::<f32>::zeros((1, 3, s, s)).into())?;
        Ok(out[0].shape().to_vec())
    }

    /// The model input for an RGBA8 image: premultiplied resize to the model size, normalised.
    fn input(&self, rgba: &[u8], w: usize, h: usize, peak_scaled: bool) -> Result<tract_ndarray::Array4<f32>> {
        if rgba.len() != w * h * 4 || w == 0 || h == 0 {
            bail!("image buffer size mismatch");
        }
        let s = self.cfg.size;
        let small = resize_premultiplied(rgba, w, h, s, s);
        let peak = if peak_scaled { small.iter().flat_map(|p| [p[0], p[1], p[2]]).fold(1e-6f32, f32::max) } else { 1.0 };
        let (mean, std) = self.cfg.norm.mean_std();
        Ok(tract_ndarray::Array4::from_shape_fn((1, 3, s, s), |(_, c, y, x)| (small[y * s + x][c] / peak - mean[c]) / std[c]))
    }

    /// Foreground probability (0–1) for an RGBA8 image, at the image's size. `None` when the model
    /// sees no clear subject.
    pub fn predict(&self, rgba: &[u8], w: usize, h: usize) -> Result<Option<Vec<f32>>> {
        let input = self.input(rgba, w, h, true)?;
        let out = (self.run)(input.into())?;
        let map = out[0].to_plain_array_view::<f32>()?;
        let Some((oh, ow)) = single_map(map.shape()) else { bail!("The model's output {:?} isn't a single map", map.shape()) };
        let vals: Vec<f32> = map.iter().copied().collect();
        if vals.len() < oh * ow || oh * ow == 0 {
            bail!("unexpected model output");
        }
        let vals = &vals[..oh * ow];
        let (lo, hi) = vals.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        if hi < 0.5 {
            return Ok(None);
        }
        let span = (hi - lo).max(1e-6);
        let norm: Vec<f32> = vals.iter().map(|v| (v - lo) / span).collect();
        Ok(Some(upscale(&norm, ow, oh, w, h)))
    }

    /// Sky probability (0–1) for an RGBA8 image, at the image's size (a sky model). Raw
    /// probabilities, not stretched: a photo without sky stays near zero.
    pub fn predict_sky(&self, rgba: &[u8], w: usize, h: usize) -> Result<Vec<f32>> {
        let Task::Sky { classes, class, margin } = self.cfg.task else { bail!("not a sky model") };
        let input = self.input(rgba, w, h, false)?;
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
        if self.cfg.task != Task::Depth {
            bail!("not a depth model");
        }
        let input = self.input(rgba, w, h, false)?;
        let out = (self.run)(input.into())?;
        let map = out[0].to_plain_array_view::<f32>()?;
        let Some((oh, ow)) = single_map(map.shape()) else { bail!("unexpected depth model output {:?}", map.shape()) };
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

// ------------------------------------------------------------------------------ custom models

/// A model the user picked for a function (an `.onnx` file copied into
/// `<models>/segmentation/custom/`), used instead of the official one.
#[derive(Clone, Debug, PartialEq)]
pub struct Custom {
    pub group: Group,
    /// The copy's file name inside the custom folder.
    pub file: String,
    /// The picked file's name, for display.
    pub name: String,
    /// The square input size the model runs at.
    pub size: usize,
    /// Subject models: how the input is normalised.
    pub norm: Norm,
    /// Sky models: the number of output channels (1: a sigmoid map; otherwise class logits).
    pub classes: usize,
    /// Sky models with class logits: the sky's class index.
    pub class: usize,
}

impl Custom {
    /// Where the copy lives.
    pub fn path(&self, models_dir: &Path) -> PathBuf {
        custom_dir(models_dir).join(&self.file)
    }

    fn config(&self) -> Config {
        let task = match self.group {
            Group::Subject => Task::Subject,
            Group::Sky => Task::Sky { classes: self.classes, class: self.class, margin: if self.classes == 1 { 0.0 } else { 2.0 } },
            Group::Depth => Task::Depth,
        };
        Config { id: format!("custom:{}", self.file), size: self.size, norm: self.norm, task }
    }
}

/// What the user chose besides the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CustomOptions {
    /// Subject models: the input normalisation.
    pub norm: Norm,
    /// Sky models with class logits: the sky's class index (ADE20K: 2).
    pub sky_class: usize,
}

impl Default for CustomOptions {
    fn default() -> Self {
        Self { norm: Norm::IsNet, sky_class: 2 }
    }
}

/// `<models>/segmentation/custom/`.
pub fn custom_dir(models_dir: &Path) -> PathBuf {
    models_dir.join("segmentation").join("custom")
}

fn custom_list(models_dir: &Path) -> PathBuf {
    custom_dir(models_dir).join("custom.txt")
}

/// The custom model set for `group`, when its file is still there.
pub fn custom(models_dir: &Path, group: Group) -> Option<Custom> {
    read_customs(models_dir).into_iter().find(|c| c.group == group).filter(|c| c.path(models_dir).is_file())
}

/// One line per function: `key TAB file TAB name TAB size TAB norm TAB classes TAB class`.
fn read_customs(models_dir: &Path) -> Vec<Custom> {
    let Ok(text) = std::fs::read_to_string(custom_list(models_dir)) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let [key, file, name, size, norm, classes, class] = f.as_slice() else { return None };
            Some(Custom {
                group: Group::ALL.into_iter().find(|g| g.key() == *key)?,
                file: (*file).to_owned(),
                name: (*name).to_owned(),
                size: size.parse().ok().filter(|s| *s > 0)?,
                norm: Norm::from_key(norm)?,
                classes: classes.parse().ok().filter(|c| *c > 0)?,
                class: class.parse().ok()?,
            })
        })
        .collect()
}

fn write_customs(models_dir: &Path, customs: &[Custom]) -> Result<()> {
    let text: String =
        customs.iter().map(|c| format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\n", c.group.key(), c.file, c.name, c.size, c.norm.key(), c.classes, c.class)).collect();
    let path = custom_list(models_dir);
    if text.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    std::fs::create_dir_all(custom_dir(models_dir))?;
    std::fs::write(&path, text).with_context(|| format!("Could not save {}", path.display()))
}

/// A file name that is safe inside the custom folder.
fn safe_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' }).collect();
    s.trim_matches('.').to_owned()
}

/// The model's square input size when the file fixes it (`None` when it is dynamic).
pub fn input_size(path: &Path) -> Result<Option<usize>> {
    let model = tract_onnx::onnx().model_for_path(path).with_context(|| format!("Could not read {} as an ONNX model", path.display()))?;
    let fact = model.input_fact(0).context("The model has no input")?;
    let dim = |i: usize| fact.shape.dim(i).and_then(|d| d.concretize()).and_then(|d| d.to_i64().ok());
    if fact.shape.rank().concretize().is_some_and(|r| r != 4) {
        bail!("The model's input must be an image (batch × 3 × height × width)");
    }
    if let Some(c) = dim(1)
        && c != 3
    {
        bail!("The model's input has {c} channels; an RGB image (3) is needed");
    }
    match (dim(2), dim(3)) {
        (Some(h), Some(w)) if h == w && h > 0 => Ok(Some(h as usize)),
        (Some(h), Some(w)) => bail!("The model's input is {h}×{w}; a square input (or a flexible size) is needed"),
        _ => Ok(None),
    }
}

/// A small synthetic photo to test a model on: a sky-blue gradient over a green ground, with a
/// red block in the middle.
fn synthetic(w: usize, h: usize) -> Vec<u8> {
    let mut img = vec![255u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let c = if x > w / 3 && x < 2 * w / 3 && y > h / 2 && y < h * 9 / 10 {
                [200, 40, 40]
            } else if y < h / 2 {
                [90, 150, 230]
            } else {
                [70, 120, 60]
            };
            img[i..i + 3].copy_from_slice(&c);
        }
    }
    img
}

/// Checks that `path` can serve `group` as configured by `custom` (loads it and runs it once on a
/// synthetic image). Errors say what doesn't fit.
fn validate(custom: &Custom, path: &Path) -> Result<()> {
    let seg = Segmenter::load_cfg(custom.config(), path)?;
    let (w, h) = (96, 64);
    let img = synthetic(w, h);
    match custom.group {
        Group::Subject => {
            seg.predict(&img, w, h).context("Not a subject model: it must output one foreground map")?;
        }
        Group::Sky => {
            seg.predict_sky(&img, w, h).context("Not a sky model")?;
        }
        Group::Depth => {
            seg.predict_depth(&img, w, h).context("Not a depth model: it must output one depth map")?;
        }
    }
    Ok(())
}

/// Adds the `.onnx` file at `src` as the custom model of `group`: reads its input size (the
/// official model's size when it is flexible), tests it on a synthetic image, copies it into the
/// custom folder and remembers it. A custom model already set is replaced; `dispose` puts its old
/// file away (the Trash). Nothing changes when the model doesn't fit.
pub fn add_custom(models_dir: &Path, group: Group, src: &Path, opts: CustomOptions, dispose: &dyn Fn(&Path) -> std::io::Result<()>) -> Result<Custom> {
    if !src.is_file() {
        bail!("{} isn't a file", src.display());
    }
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !name.to_lowercase().ends_with(".onnx") {
        bail!("Choose an ONNX model (a file ending in .onnx)");
    }
    let size = input_size(src)?.unwrap_or(group.official().size);
    let mut c = Custom { group, file: format!("{}-{}", group.key(), safe_name(&name)), name, size, norm: opts.norm, classes: 1, class: 0 };
    if group == Group::Sky {
        // a sky model gives one sigmoid map, or one logit map per class
        let probe = Segmenter::load_cfg(Config { task: Task::Sky { classes: 1, class: 0, margin: 0.0 }, ..c.config() }, src)?;
        let shape = probe.output_shape()?;
        let classes = match shape.as_slice() {
            [_, ch, _, _] if *ch >= 1 => *ch,
            other => bail!("Not a sky model: its output {other:?} should be 1 map or one map per class"),
        };
        if classes > 1 && opts.sky_class >= classes {
            bail!("The sky class {} is out of range: this model has {classes} classes (0 to {})", opts.sky_class, classes - 1);
        }
        c.classes = classes;
        c.class = if classes > 1 { opts.sky_class } else { 0 };
    }
    validate(&c, src)?;
    std::fs::create_dir_all(custom_dir(models_dir))?;
    let mut all = read_customs(models_dir);
    let old = all.iter().find(|o| o.group == group).cloned();
    let dest = c.path(models_dir);
    if src != dest {
        std::fs::copy(src, &dest).with_context(|| format!("Could not copy the model into {}", dest.display()))?;
    }
    if let Some(old) = old
        && old.file != c.file
    {
        let _ = dispose(&old.path(models_dir));
    }
    all.retain(|o| o.group != group);
    all.push(c.clone());
    write_customs(models_dir, &all)?;
    forget_group(group);
    Ok(c)
}

/// Goes back to the official model of `group`: forgets the custom one and `dispose`s of its copy
/// (the Trash). Returns the bytes of the file that was put away (0 when there was none).
pub fn clear_custom(models_dir: &Path, group: Group, dispose: &dyn Fn(&Path) -> std::io::Result<()>) -> Result<u64> {
    let mut all = read_customs(models_dir);
    let Some(old) = all.iter().find(|o| o.group == group).cloned() else { return Ok(0) };
    let path = old.path(models_dir);
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if path.is_file() {
        dispose(&path).with_context(|| format!("Could not remove {}", path.display()))?;
    }
    all.retain(|o| o.group != group);
    write_customs(models_dir, &all)?;
    forget_group(group);
    Ok(bytes)
}

// -------------------------------------------------------------------------------- old models

/// Files of the models the app no longer offers that are still on disk, with their sizes.
pub fn legacy_installed(models_dir: &Path) -> Vec<(PathBuf, u64)> {
    LEGACY
        .iter()
        .map(|(file, _)| models_dir.join("segmentation").join(file))
        .filter_map(|p| std::fs::metadata(&p).ok().filter(|m| m.is_file()).map(|m| (p, m.len())))
        .collect()
}

/// Puts the old models' files away with `dispose` (the Trash). Returns the bytes freed.
pub fn remove_legacy_with(models_dir: &Path, dispose: &dyn Fn(&Path) -> std::io::Result<()>) -> Result<u64> {
    let mut freed = 0;
    for (p, bytes) in legacy_installed(models_dir) {
        dispose(&p).with_context(|| format!("Could not remove {}", p.display()))?;
        freed += bytes;
    }
    Ok(freed)
}

// ------------------------------------------------------------------------- which model runs

struct Active {
    /// Cache key: changes when the model or its settings do.
    key: String,
    path: PathBuf,
    cfg: Config,
    in_use: InUse,
}

fn active(group: Group, models_dir: &Path) -> Option<Active> {
    if let Some(c) = custom(models_dir, group) {
        let len = std::fs::metadata(c.path(models_dir)).map(|m| m.len()).unwrap_or(0);
        let key = format!("custom:{}:{}:{}:{}:{}:{len}", c.file, c.size, c.norm.key(), c.classes, c.class);
        return Some(Active { key, path: c.path(models_dir), cfg: c.config(), in_use: InUse::Custom(c) });
    }
    let spec = group.official();
    let path = model_path(models_dir, spec);
    path.is_file().then(|| Active { key: spec.id.to_owned(), path, cfg: Config::of_spec(spec), in_use: InUse::Official(spec) })
}

/// Whether a model is ready for `group` (a custom one, or the installed official one).
pub fn available(group: Group, models_dir: &Path) -> bool {
    active(group, models_dir).is_some()
}

/// A process-wide cache of the loaded depth model.
pub fn shared_depth(models_dir: &Path) -> Option<Segmenter> {
    cached(active(Group::Depth, models_dir)?, &DEPTH)
}

/// A process-wide cache of the loaded subject model (loading takes a moment).
pub fn shared(models_dir: &Path) -> Option<Segmenter> {
    cached(active(Group::Subject, models_dir)?, &SUBJECT)
}

/// A process-wide cache of the loaded sky model.
pub fn shared_sky(models_dir: &Path) -> Option<Segmenter> {
    cached(active(Group::Sky, models_dir)?, &SKY)
}

/// Size of the installed copy of `spec`, when it is installed.
pub fn installed_bytes(models_dir: &Path, spec: &ModelSpec) -> Option<u64> {
    std::fs::metadata(model_path(models_dir, spec)).ok().filter(|m| m.is_file()).map(|m| m.len())
}

/// Removes an installed model, deleting its file. See [`remove_with`].
pub fn remove(models_dir: &Path, spec: &ModelSpec) -> Result<u64> {
    remove_with(models_dir, spec, &|p| std::fs::remove_file(p))
}

/// Removes an installed model: `dispose` gets its file (to move it to the Trash, or delete it),
/// and the loaded copy is forgotten so nothing keeps using it. Returns the bytes freed (0 when it
/// wasn't installed).
pub fn remove_with(models_dir: &Path, spec: &ModelSpec, dispose: &dyn Fn(&Path) -> std::io::Result<()>) -> Result<u64> {
    let path = model_path(models_dir, spec);
    let Some(bytes) = installed_bytes(models_dir, spec) else {
        forget(spec);
        return Ok(0);
    };
    dispose(&path).with_context(|| format!("Could not remove {}", path.display()))?;
    forget(spec);
    Ok(bytes)
}

/// Drops the process-wide loaded copy of `spec` (after it is removed).
fn forget(spec: &ModelSpec) {
    for slot in [&SUBJECT, &SKY, &DEPTH] {
        let mut g = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if g.as_ref().is_some_and(|(key, _)| key == spec.id) {
            *g = None;
        }
    }
}

/// Drops the loaded copy of whatever serves `group`.
fn forget_group(group: Group) {
    let mut g = slot(group).lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    *g = None;
}

type Slot = std::sync::Mutex<Option<(String, Segmenter)>>;
static SUBJECT: Slot = std::sync::Mutex::new(None);
static SKY: Slot = std::sync::Mutex::new(None);
static DEPTH: Slot = std::sync::Mutex::new(None);

fn slot(group: Group) -> &'static Slot {
    match group {
        Group::Subject => &SUBJECT,
        Group::Sky => &SKY,
        Group::Depth => &DEPTH,
    }
}

fn cached(a: Active, slot: &Slot) -> Option<Segmenter> {
    let mut g = slot.lock().ok()?;
    if let Some((key, s)) = g.as_ref()
        && *key == a.key
    {
        return Some(s.clone());
    }
    let s = Segmenter::load_cfg(a.cfg, &a.path).ok()?;
    *g = Some((a.key, s.clone()));
    Some(s)
}

#[cfg(test)]
mod tests;
