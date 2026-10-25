//! Custom models: an `.onnx` file the user picks for a function, checked on a synthetic photo,
//! copied into `<models>/segmentation/custom/` and listed in `custom.txt`.

use super::*;

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
    /// How the input is normalised.
    pub norm: Norm,
    /// Sky output channels; restoration models persist three RGB channels here.
    pub classes: usize,
    /// Sky class index; depth output: 0 = inverse depth, 1 = metric depth; restoration
    /// contract: 0 = display sRGB, 1 = linear Rec.2020.
    pub class: usize,
    /// Subject and sky maps: what the output values are.
    pub activation: Activation,
}

impl Custom {
    /// Where the copy lives.
    pub fn path(&self, models_dir: &Path) -> PathBuf {
        custom_dir(models_dir).join(&self.file)
    }

    pub(crate) fn config(&self) -> Config {
        let task = match self.group {
            Group::Subject => Task::Subject,
            Group::Sky => Task::Sky { classes: self.classes, class: self.class, margin: if self.classes == 1 { 0.0 } else { 2.0 } },
            Group::Depth => Task::Depth,
            Group::Tagging => Task::ImageText,
            Group::Faces => Task::FaceEmbed,
            Group::RawDenoise | Group::RgbDenoise | Group::Remove => self.group.official().task,
        };
        Config {
            id: format!("custom:{}", self.file),
            size: self.size,
            norm: self.norm,
            task,
            post: Post::MinMax,
            activation: self.activation,
            metric: self.group == Group::Depth && self.class == 1,
            keeps_aspect: false,
        }
    }
}

/// What the user chose besides the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CustomOptions {
    /// RGB restoration: use linear Rec.2020 instead of display sRGB. Raw always uses linear.
    pub denoise_linear: bool,
    /// The input normalisation (subject, sky and depth models).
    pub norm: Norm,
    /// Sky models with class logits: the sky's class index (ADE20K: 2).
    pub sky_class: usize,
    /// Depth models: the output is metric depth (larger = farther) instead of inverse depth.
    pub depth_metric: bool,
}

impl Default for CustomOptions {
    fn default() -> Self {
        Self { norm: Norm::IsNet, sky_class: 2, denoise_linear: false, depth_metric: false }
    }
}

impl CustomOptions {
    /// The options a new custom model of `group` starts from.
    pub fn for_group(group: Group) -> Self {
        Self { norm: Norm::default_for(group), ..Self::default() }
    }
}

/// `<models>/segmentation/custom/`.
pub fn custom_dir(models_dir: &Path) -> PathBuf {
    models_dir.join("segmentation").join("custom")
}

pub(crate) fn custom_list(models_dir: &Path) -> PathBuf {
    custom_dir(models_dir).join("custom.txt")
}

/// The custom model set for `group`, when its file is still there.
pub fn custom(models_dir: &Path, group: Group) -> Option<Custom> {
    if !group.supports_custom() {
        return None;
    }
    read_customs(models_dir).into_iter().find(|c| c.group == group).filter(|c| c.path(models_dir).is_file())
}

/// One line per function: `key TAB file TAB name TAB size TAB norm TAB classes TAB class TAB
/// activation` (lines without the activation are read as [`Activation::legacy`]).
fn read_customs(models_dir: &Path) -> Vec<Custom> {
    let Ok(text) = std::fs::read_to_string(custom_list(models_dir)) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let ([key, file, name, size, norm, classes, class], activation) = (f.get(..7)?, f.get(7)) else { return None };
            if f.len() > 8 {
                return None;
            }
            let group = Group::ALL.into_iter().find(|g| g.supports_custom() && g.key() == *key)?;
            let custom = Custom {
                group,
                file: (*file).to_owned(),
                name: (*name).to_owned(),
                size: size.parse().ok().filter(|s| *s > 0)?,
                norm: Norm::from_key(norm)?,
                classes: classes.parse().ok().filter(|c| *c > 0)?,
                class: class.parse().ok()?,
                activation: match activation {
                    Some(a) => Activation::from_key(a)?,
                    None => Activation::legacy(group),
                },
            };
            (custom.group != Group::Sky || custom.class < custom.classes).then_some(custom)
        })
        .collect()
}

fn write_customs(models_dir: &Path, customs: &[Custom]) -> Result<()> {
    let text: String = customs
        .iter()
        .map(|c| format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n", c.group.key(), c.file, c.name, c.size, c.norm.key(), c.classes, c.class, c.activation.key()))
        .collect();
    let path = custom_list(models_dir);
    if text.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    std::fs::create_dir_all(custom_dir(models_dir))?;
    li_fs::write_atomic(&path, text.as_bytes()).with_context(|| format!("Could not save {}", path.display()))
}

/// A file name that is safe inside the custom folder.
fn safe_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' }).collect();
    s.trim_matches('.').to_owned()
}

/// Why a user-picked custom model cannot be used.
#[derive(Debug, Clone, PartialEq)]
pub enum CustomModelError {
    /// This function requires a bundle and has no custom-model flow.
    UnsupportedFunction,
    /// The path isn't a regular file.
    NotAFile { path: PathBuf },
    /// The file name doesn't end in `.onnx`.
    NotOnnx,
    /// The model has no input.
    NoInput,
    /// The model's input is not a 4D image tensor.
    NotAnImageInput,
    /// The model's input has a channel count other than 3.
    WrongChannels { channels: i64 },
    /// The model's input is not square.
    NonSquareInput { height: i64, width: i64 },
    /// The sky model's first output shape isn't a class map.
    NotSkyModelOutput { shape: Vec<usize> },
    /// The requested sky class index is out of range.
    SkyClassOutOfRange { class: usize, classes: usize },
    /// The subject model doesn't output a single foreground map.
    NotSubjectModel,
    /// The depth model doesn't output a single depth map.
    NotDepthModel,
    /// The GPU denoiser can't run this restoration model; why.
    UnsupportedRestoration { reason: String },
    /// A tract load/optimise failure.
    Other(String),
}

impl fmt::Display for CustomModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CustomModelError::UnsupportedFunction => f.write_str("Custom models are not supported for this function"),
            CustomModelError::NotAFile { path } => write!(f, "{} isn't a file", path.display()),
            CustomModelError::NotOnnx => f.write_str("Choose an ONNX model (a file ending in .onnx)"),
            CustomModelError::NoInput => f.write_str("The model has no input"),
            CustomModelError::NotAnImageInput => f.write_str("The model's input must be an image (batch × 3 × height × width)"),
            CustomModelError::WrongChannels { channels } => write!(f, "The model's input has {channels} channels; an RGB image (3) is needed"),
            CustomModelError::NonSquareInput { height, width } => {
                write!(f, "The model's input is {height}×{width}; a square input (or a flexible size) is needed")
            }
            CustomModelError::NotSkyModelOutput { shape } => write!(f, "Not a sky model: its output {shape:?} should be 1 map or one map per class"),
            CustomModelError::SkyClassOutOfRange { class, classes } => {
                write!(f, "The sky class {class} is out of range: this model has {classes} classes (0 to {})", classes - 1)
            }
            CustomModelError::NotSubjectModel => f.write_str("Not a subject model: it must output one foreground map"),
            CustomModelError::NotDepthModel => f.write_str("Not a depth model: it must output one depth map"),
            CustomModelError::UnsupportedRestoration { reason } => write!(f, "{RESTORATION_UNSUPPORTED} ({reason})"),
            CustomModelError::Other(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for CustomModelError {}

/// The model's square input size when the file fixes it (`None` when it is dynamic).
pub fn input_size(path: &Path) -> Result<Option<usize>, CustomModelError> {
    let model = tract_onnx::onnx().model_for_path(path).map_err(|_| CustomModelError::Other(format!("Could not read {} as an ONNX model", path.display())))?;
    let fact = model.input_fact(0).map_err(|_| CustomModelError::NoInput)?;
    let dim = |i: usize| fact.shape.dim(i).and_then(|d| d.concretize()).and_then(|d| d.to_i64().ok());
    if fact.shape.rank().concretize().is_some_and(|r| r != 4) {
        return Err(CustomModelError::NotAnImageInput);
    }
    if let Some(c) = dim(1)
        && c != 3
    {
        return Err(CustomModelError::WrongChannels { channels: c });
    }
    match (dim(2), dim(3)) {
        (Some(h), Some(w)) if h == w && h > 0 => Ok(Some(h as usize)),
        (Some(h), Some(w)) => Err(CustomModelError::NonSquareInput { height: h, width: w }),
        _ => Ok(None),
    }
}

/// A small synthetic photo to test a model on: a sky-blue gradient over a green ground, with a
/// red block in the middle.
pub(crate) fn synthetic(w: usize, h: usize) -> Vec<u8> {
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

/// Why a restoration model can't be used, before the specific reason.
pub const RESTORATION_UNSUPPORTED: &str = "Local Image's GPU denoiser can't run this model. It runs convolutions (not grouped or dilated), ReLU, LeakyReLU, sigmoid, max pooling, two-input channel concat and DepthToSpace, with float32 weights";

fn tract_error(e: anyhow::Error) -> CustomModelError {
    CustomModelError::Other(format!("{e:#}"))
}

/// The first output of a mask model run on the synthetic photo and on its inverse: its shape, and
/// what its values are.
fn probe_outputs(cfg: Config, path: &Path, peak_scaled: bool) -> Result<(Vec<usize>, Activation), CustomModelError> {
    let seg = Segmenter::load_cfg(cfg, path).map_err(tract_error)?;
    let s = seg.cfg.size;
    let photo = synthetic(s, s);
    let inverse: Vec<u8> = photo.as_chunks::<4>().0.iter().flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], p[3]]).collect();
    let (mut shape, mut values) = (Vec::new(), Vec::new());
    for img in [photo, inverse] {
        let input = seg.input(&img, s, s, peak_scaled).map_err(tract_error)?;
        let out = (seg.run)(input.into(), &never).map_err(tract_error)?;
        let map = out.first().ok_or_else(|| CustomModelError::Other("The model has no output".into()))?.to_plain_array_view::<f32>().map_err(tract_error)?;
        shape = map.shape().to_vec();
        values.extend(map.iter().copied());
    }
    Ok((shape, Activation::of_outputs(values)))
}

/// Checks that `path` can serve `custom.group` as `opts` configure it: a mask model runs on the
/// synthetic photo (its sky classes and output activation are set in `custom`), a restoration
/// model on a flat tile. Errors say what doesn't fit.
fn validate(custom: &mut Custom, path: &Path, opts: &CustomOptions) -> Result<(), CustomModelError> {
    if matches!(custom.group, Group::RawDenoise | Group::RgbDenoise) {
        let kind = if custom.class == 1 { denoise::Variant::Linear } else { denoise::Variant::Rgb };
        // the GPU graph is what runs the model; tract checks its numbers on the CPU
        restoration_graph::Graph::load(path, restoration_graph::Shape { c: kind.channels(), h: custom.size, w: custom.size })
            .map_err(|e| CustomModelError::UnsupportedRestoration { reason: format!("{e:#}") })?;
        let model = denoise::Denoiser::load_at(path, kind, custom.size).map_err(tract_error)?;
        let out = model.run_tile(&vec![0.1; 3 * custom.size * custom.size]).map_err(tract_error)?;
        if out.len() != 3 * custom.size * custom.size || !out.iter().all(|v| v.is_finite()) {
            return Err(CustomModelError::Other("Restoration output must be three finite channels at the input size".into()));
        }
        return Ok(());
    }
    if matches!(custom.group, Group::Tagging | Group::Faces | Group::Remove) {
        return Err(CustomModelError::UnsupportedFunction);
    }
    let cfg =
        Config { task: if custom.group == Group::Sky { Task::Sky { classes: 1, class: 0, margin: 0.0 } } else { custom.config().task }, ..custom.config() };
    let (shape, activation) = probe_outputs(cfg, path, custom.group == Group::Subject)?;
    match custom.group {
        Group::Sky => {
            let classes = match shape.as_slice() {
                [_, ch, _, _] if *ch >= 1 => *ch,
                other => return Err(CustomModelError::NotSkyModelOutput { shape: other.to_vec() }),
            };
            if classes > 1 && opts.sky_class >= classes {
                return Err(CustomModelError::SkyClassOutOfRange { class: opts.sky_class, classes });
            }
            custom.classes = classes;
            custom.class = if classes > 1 { opts.sky_class } else { 0 };
        }
        Group::Subject if single_map(&shape).is_none() => return Err(CustomModelError::NotSubjectModel),
        Group::Depth if single_map(&shape).is_none() => return Err(CustomModelError::NotDepthModel),
        _ => {}
    }
    custom.activation = activation;
    Ok(())
}

/// Adds the `.onnx` file at `src` as the custom model of `group`: reads its input size (the
/// official model's size when it is flexible), tests it on a synthetic image, copies it into the
/// custom folder and remembers it. A custom model already set is replaced; `dispose` puts its old
/// file away (the Trash). Nothing changes when the model doesn't fit.
pub fn add_custom(models_dir: &Path, group: Group, src: &Path, opts: CustomOptions, dispose: &dyn Fn(&Path) -> std::io::Result<()>) -> Result<Custom> {
    add_custom_with(models_dir, group, src, opts, dispose, &write_customs)
}

fn identical_files(a: &Path, b: &Path) -> std::io::Result<bool> {
    use std::io::Read;
    if std::fs::metadata(a)?.len() != std::fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (std::io::BufReader::new(std::fs::File::open(a)?), std::io::BufReader::new(std::fs::File::open(b)?));
    let (mut aa, mut bb) = ([0u8; 8192], [0u8; 8192]);
    loop {
        let n = a.read(&mut aa)?;
        b.read_exact(&mut bb[..n])?;
        if aa[..n] != bb[..n] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

pub(crate) fn add_custom_with(
    models_dir: &Path,
    group: Group,
    src: &Path,
    opts: CustomOptions,
    dispose: &dyn Fn(&Path) -> std::io::Result<()>,
    publish: &dyn Fn(&Path, &[Custom]) -> Result<()>,
) -> Result<Custom> {
    if !group.supports_custom() {
        return Err(CustomModelError::UnsupportedFunction.into());
    }
    if !src.is_file() {
        return Err(CustomModelError::NotAFile { path: src.to_path_buf() }.into());
    }
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !name.to_lowercase().ends_with(".onnx") {
        return Err(CustomModelError::NotOnnx.into());
    }
    let size = input_size(src)?.unwrap_or(group.official().size);
    let mut c = Custom {
        group,
        file: format!("{}-{}", group.key(), safe_name(&name)),
        name,
        size,
        norm: opts.norm,
        classes: 1,
        class: 0,
        activation: Activation::Probability,
    };
    if matches!(group, Group::RawDenoise | Group::RgbDenoise) {
        c.classes = 3;
        c.class = usize::from(group == Group::RawDenoise || opts.denoise_linear);
    }
    if group == Group::Depth {
        c.class = usize::from(opts.depth_metric);
    }
    let inlined = external_data::inlined(src).map_err(tract_error)?;
    std::fs::create_dir_all(custom_dir(models_dir))?;
    let mut all = read_customs(models_dir);
    let old = all.iter().find(|o| o.group == group).cloned();
    let original_dest = c.path(models_dir);
    // A replacement with the same basename must not overwrite the registered file
    // before its new metadata has been published.
    let same_copy = inlined.is_none()
        && src != original_dest
        && original_dest.is_file()
        && old.as_ref().is_some_and(|old| old.path(models_dir) == original_dest)
        && identical_files(src, &original_dest)?;
    if src != original_dest && original_dest.exists() && !same_copy {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        loop {
            c.file = format!("{}-{}-{}-{}", group.key(), std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed), safe_name(&c.name));
            if !c.path(models_dir).exists() {
                break;
            }
        }
    }
    let dest = c.path(models_dir);
    let copied = src != dest && !same_copy;
    if copied {
        match &inlined {
            Some(bytes) => li_fs::write_atomic(&dest, bytes).map(drop),
            None => std::fs::copy(src, &dest).map(drop),
        }
        .with_context(|| format!("Could not copy the model into {}", dest.display()))?;
    }
    // the copy is what runs, so it is what gets tested
    if let Err(error) = validate(&mut c, &dest, &opts) {
        if copied {
            let _ = std::fs::remove_file(&dest);
        }
        return Err(error.into());
    }
    all.retain(|o| o.group != group);
    all.push(c.clone());
    if let Err(error) = publish(models_dir, &all) {
        if copied {
            let _ = std::fs::remove_file(&dest);
        }
        return Err(error);
    }
    if let Some(old) = old
        && old.file != c.file
    {
        let _ = dispose(&old.path(models_dir));
    }
    forget_group(group);
    Ok(c)
}

/// Changes how the custom model of `group` is run (its input normalisation, a sky model's class,
/// a depth model's output) without importing it again.
pub fn set_custom_options(models_dir: &Path, group: Group, opts: CustomOptions) -> Result<Custom> {
    let mut all = read_customs(models_dir);
    let c = all.iter_mut().find(|c| c.group == group).context("This function has no custom model")?;
    c.norm = opts.norm;
    match group {
        Group::Sky if c.classes > 1 => {
            if opts.sky_class >= c.classes {
                return Err(CustomModelError::SkyClassOutOfRange { class: opts.sky_class, classes: c.classes }.into());
            }
            c.class = opts.sky_class;
        }
        Group::Depth => c.class = usize::from(opts.depth_metric),
        _ => {}
    }
    let changed = c.clone();
    write_customs(models_dir, &all)?;
    forget_group(group);
    Ok(changed)
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

#[cfg(test)]
#[test]
fn custom_inventory_parity_write_fault_and_cleanup() {
    let dir = std::env::temp_dir().join(format!("custom-inventory-{}", std::process::id()));
    let model = Custom {
        group: Group::Subject,
        file: "fixture.onnx".into(),
        name: "Fixture".into(),
        size: 32,
        norm: Norm::IsNet,
        classes: 1,
        class: 0,
        activation: Activation::Logits,
    };
    write_customs(&dir, &[model]).unwrap();
    let path = custom_list(&dir);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes, b"subject\tfixture.onnx\tFixture\t32\tisnet\t1\t0\tlogits\n");
    let changed = Custom { file: "new.onnx".into(), name: "New".into(), size: 64, norm: Norm::ImageNet, ..read_customs(&dir)[0].clone() };
    {
        let _fault = li_fs::fail_writes_after(2);
        assert!(write_customs(&dir, &[changed]).is_err());
    }
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(custom_dir(&dir)).unwrap().count(), 1);
    assert_eq!(read_customs(&dir)[0].file, "fixture.onnx");
    write_customs(&dir, &[]).unwrap();
    assert!(!path.exists());
    std::fs::remove_dir_all(dir).unwrap();
}
