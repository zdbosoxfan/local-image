//! AI denoise at the source stage, before Develop geometry/tone/detail. Model loading, decoding,
//! inference and result writing run in Enhance workers. Strength mixes the cached source result.
use super::store::{self, Kind};
use super::{CANCELLED, Job, JobCtl, JobKind, Outcome};
use crate::{Session, media::SourceLevel};
use li_raster::RgbFrame;
use li_raster::resample::Filter;
use li_seg::denoise::Variant;
use lightcraft_catalog::{PhotoId, Source};
use lightcraft_develop::DenoiseModel;
use lightcraft_preview::Hasher128;
use std::sync::{Arc, Mutex, Weak};

pub const PREVIEW_EDGE: usize = 2560;
pub const INFERENCE_PREVIEW_EDGE: usize = 1024;
#[cfg(not(target_arch = "wasm32"))]
type GpuSlot = Option<(std::path::PathBuf, Variant, (u64, Option<std::time::SystemTime>), lightcraft_gpu::denoise::Denoiser)>;
#[cfg(not(target_arch = "wasm32"))]
static GPU_MODEL: Mutex<GpuSlot> = Mutex::new(None);

/// Session-test scopes release model tensors before releasing the compute
/// context. The cache mutex also waits for any inference using those tensors.
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) fn release_gpu_test_cache() {
    let model = GPU_MODEL.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    drop(model);
}
pub type DenoiseFn = Arc<dyn Fn(&[[f32; 3]], usize, usize, &(dyn Fn(usize, usize) -> bool + Sync)) -> Result<Vec<[f32; 3]>, String> + Send + Sync>;
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct DenoiseDone {
    pub key: String,
    pub source: String,
    pub model: String,
    pub family: DenoiseModel,
    pub preview: bool,
    /// [`source_options`] of the decoding it was made from.
    pub raw: u64,
}
pub fn model_tag() -> String {
    format!("{}@df957efa", li_seg::denoise::DENOISE_ID)
}
pub fn source_hash(p: &lightcraft_catalog::Photo) -> String {
    Hasher128::new().str(&crate::media::content_key(p)).finish().to_string()
}
pub fn result_key(source: &str, model: &str) -> String {
    store::key_of(&["denoise-v2", source, model])
}
pub fn preview_key(key: &str) -> String {
    format!("{key}-{PREVIEW_EDGE}")
}
/// What a result depends on besides the photo's content and the model: the raw decoding of the
/// source it was made from (camera profile, highlight recovery, demosaic; 0 for the defaults).
pub fn source_options(d: &lightcraft_develop::DevelopSettings) -> u64 {
    crate::files::RawOptions { capture_radius: false, ..crate::files::RawOptions::of(d) }.key()
}
/// Whether `a` was made under `d`'s raw decoding (a result made under other settings isn't
/// applied: AI Denoise has to run again).
pub fn matches_decoding(a: &lightcraft_develop::DenoiseRef, d: &lightcraft_develop::DevelopSettings) -> bool {
    a.raw == source_options(d)
}
pub fn can_denoise(s: &Session, id: PhotoId) -> Result<(), String> {
    let p = s.catalog.photo(id).ok_or("no such photo")?;
    if p.preview_only.is_some() {
        return Err("AI Denoise needs decoded pixels; this raw is preview-only.".into());
    }
    Ok(())
}

/// GPU/NPU-only inference router. Device failure is reported; never silently start a CPU job.
pub fn infer(
    dir: &std::path::Path,
    kind: Variant,
    pixels: &[f32],
    w: usize,
    h: usize,
    gpu: bool,
    progress: &(dyn Fn(usize, usize) -> bool + Sync),
) -> Result<Vec<[f32; 3]>, String> {
    infer_for(kind.spec().group.ok_or("model has no function")?, dir, kind, pixels, w, h, gpu, progress)
}
fn infer_for(
    group: li_seg::Group,
    dir: &std::path::Path,
    kind: Variant,
    pixels: &[f32],
    w: usize,
    h: usize,
    gpu: bool,
    progress: &(dyn Fn(usize, usize) -> bool + Sync),
) -> Result<Vec<[f32; 3]>, String> {
    let custom = li_seg::custom(dir, group);
    infer_selected(dir, kind, custom.as_ref(), pixels, w, h, gpu, progress)
}
fn infer_selected(
    dir: &std::path::Path,
    kind: Variant,
    custom: Option<&li_seg::Custom>,
    pixels: &[f32],
    w: usize,
    h: usize,
    gpu: bool,
    progress: &(dyn Fn(usize, usize) -> bool + Sync),
) -> Result<Vec<[f32; 3]>, String> {
    if !gpu {
        return Err("AI Denoise requires a GPU; CPU inference is disabled.".into());
    }
    let (path, kind, tile) = if let Some(c) = custom {
        (c.path(dir), if c.class == 1 { Variant::Linear } else { Variant::Rgb }, c.size)
    } else {
        if li_seg::verified_denoise_bytes(dir, kind.spec()).is_none() {
            return Err("The AI Denoise model is not installed or failed verification. Download it in Settings › Local AI.".into());
        }
        (li_seg::companion_path(dir, kind.file()), kind, kind.tile())
    };
    let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let stamp = (metadata.len(), metadata.modified().ok());
    #[cfg(not(target_arch = "wasm32"))]
    if gpu && lightcraft_gpu::enabled() {
        let mut cache = GPU_MODEL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache.as_ref().is_none_or(|(p, k, st, d)| p != &path || *k != kind || *st != stamp || !d.is_current()) {
            *cache = None;
            let shape = li_seg::restoration_graph::Shape { c: kind.channels(), h: tile, w: tile };
            let graph = li_seg::restoration_graph::Graph::load(&path, shape)
                .map_err(|e| format!("This AI Denoise model can't run: {e:#}. Choose another model, or Use default in Settings › Local AI."))?;
            match lightcraft_gpu::denoise::Denoiser::from_graph(graph, kind) {
                Ok(d) => *cache = Some((path.clone(), kind, stamp, d)),
                Err(e) => return Err(format!("AI Denoise GPU unavailable: {e}")),
            }
        }
        if let Some((_, _, _, d)) = cache.as_ref() {
            match d.denoise(pixels, w, h, progress) {
                Ok(out) => return Ok(out),
                Err(e) if e == CANCELLED => return Err(e),
                Err(e) => {
                    *cache = None;
                    return Err(format!("AI Denoise GPU unavailable: {e}"));
                }
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = gpu;
    Err("AI Denoise requires an available GPU/NPU. Retry after other GPU jobs finish.".into())
}
impl Session {
    pub fn denoiser(&self) -> Result<(DenoiseFn, String), String> {
        if let Some(d) = &self.enhance.denoiser {
            return Ok((d.clone(), "custom".into()));
        }
        let dir = self.quick_seg_dir.clone().ok_or("The AI Denoise model is not installed.")?;
        if !li_seg::companion_path(&dir, Variant::Linear.file()).is_file() {
            return Err("The AI Denoise model is not installed.".into());
        }
        Ok((Arc::new(move |px, w, h, progress| infer(&dir, Variant::Linear, px.as_flattened(), w, h, true, progress)), model_tag()))
    }
    pub fn start_denoise(&mut self, id: PhotoId, wait: bool) -> Result<Arc<Job>, String> {
        self.start_denoise_with(id, false, None, wait)
    }
    pub fn start_denoise_with(
        &mut self,
        id: PhotoId,
        preview: bool,
        destination: Option<std::path::PathBuf>,
        wait: bool,
    ) -> Result<Arc<Job>, String> {
        can_denoise(self, id)?;
        if self.enhance.running_for(id).any(|j| matches!(j.kind, JobKind::Denoise | JobKind::DenoiseExport)) {
            return Err("AI Denoise is already running on this photo.".into());
        }
        let p = self.catalog.photo(id).ok_or("no such photo")?.clone();
        let raw_key = source_options(&p.develop);
        let raw_opts = if p.develops_raw() { crate::files::RawOptions::of(&p.develop) } else { Default::default() };
        let family = match p.develop.enhance.denoise_model {
            DenoiseModel::Auto => {
                if crate::media::source_info(&p).raw {
                    DenoiseModel::Raw
                } else {
                    DenoiseModel::Rgb
                }
            }
            m => m,
        };
        let override_fn = self.enhance.denoiser.clone();
        let dir = self.quick_seg_dir.clone();
        let group = if family == DenoiseModel::Raw { li_seg::Group::RawDenoise } else { li_seg::Group::RgbDenoise };
        let custom = dir.as_ref().and_then(|d| li_seg::custom(d, group));
        if custom.is_none()
            && override_fn.is_none()
            && dir.as_ref().is_none_or(|d| {
                !li_seg::model_path(d, if family == DenoiseModel::Raw { Variant::Linear.spec() } else { Variant::Rgb.spec() }).is_file()
            })
        {
            return Err("The AI Denoise model is not installed. Download it in Settings › Local AI.".into());
        }
        if preview && destination.is_some() {
            return Err("exports require full-resolution denoising".into());
        }
        let src = self.media.source_ref(&p, if preview { SourceLevel::Preview } else { SourceLevel::Full });
        let to = self.ai_store_root();
        let exporting = destination.is_some();
        let work: super::Work = Box::new(move |ctl| {
            ctl.report(0.01, "Loading the photo");
            ctl.check()?;
            let progress = |n: usize, total: usize| {
                ctl.report(0.05 + 0.85 * n as f32 / total.max(1) as f32, "Denoising");
                !ctl.is_cancelled()
            };
            let mut variant =
                if family == DenoiseModel::Raw || custom.as_ref().is_some_and(|c| c.class == 1) { Variant::Linear } else { Variant::Rgb };
            let (original, mut full) = if override_fn.is_none() && custom.is_none() && !preview && family == DenoiseModel::Raw {
                match &p.source {
                    Source::File { path } => {
                        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                        if lightcraft_raw::probe(&bytes).is_some() {
                            let raw = crate::files::prepare_raw(&bytes, &raw_opts)?;
                            if raw.cfa.as_ref().is_some_and(|c| c.is_bayer()) {
                                variant = Variant::Bayer;
                                let dir = dir.as_deref().ok_or("no models folder")?;
                                let (img, source) = denoise_bayer(
                                    &raw,
                                    &bytes,
                                    || src.load_source(),
                                    |packed, w, h| infer_selected(dir, Variant::Bayer, None, packed, w, h, true, &progress),
                                )?;
                                (Some(source), img)
                            } else {
                                let img = src.load()?;
                                let data = infer_selected(
                                    dir.as_deref().ok_or("no models folder")?,
                                    variant,
                                    custom.as_ref(),
                                    img.data.as_flattened(),
                                    img.width,
                                    img.height,
                                    true,
                                    &progress,
                                )?;
                                (Some(img.clone()), RgbFrame { width: img.width, height: img.height, data })
                            }
                        } else {
                            let img = src.load()?;
                            let data = infer_selected(
                                dir.as_deref().ok_or("no models folder")?,
                                variant,
                                custom.as_ref(),
                                img.data.as_flattened(),
                                img.width,
                                img.height,
                                true,
                                &progress,
                            )?;
                            (Some(img.clone()), RgbFrame { width: img.width, height: img.height, data })
                        }
                    }
                    _ => {
                        let img = src.load()?;
                        let data = infer_selected(
                            dir.as_deref().ok_or("no models folder")?,
                            variant,
                            custom.as_ref(),
                            img.data.as_flattened(),
                            img.width,
                            img.height,
                            true,
                            &progress,
                        )?;
                        (Some(img.clone()), RgbFrame { width: img.width, height: img.height, data })
                    }
                }
            } else {
                let img = src.load()?;
                let img = if preview {
                    let (w, h) = fit(img.width, img.height, INFERENCE_PREVIEW_EDGE);
                    Arc::new(img.resize(w, h, Filter::Box))
                } else {
                    img
                };
                let input = if variant == Variant::Rgb { to_display(&img) } else { img.data.clone() };
                let mut data = if let Some(f) = &override_fn {
                    f(&input, img.width, img.height, &progress)?
                } else {
                    infer_selected(
                        dir.as_deref().ok_or("no models folder")?,
                        variant,
                        custom.as_ref(),
                        input.as_flattened(),
                        img.width,
                        img.height,
                        true,
                        &progress,
                    )?
                };
                if variant == Variant::Rgb {
                    from_display(&mut data);
                }
                (Some(img.clone()), RgbFrame { width: img.width, height: img.height, data })
            };
            if full.data.len() != full.width * full.height || !full.data.iter().flatten().all(|v| v.is_finite()) {
                return Err("the denoiser returned invalid pixels".into());
            }
            ctl.check()?;
            if let Some(path) = destination {
                // Export the selected strength; original sources are never rewritten.
                let original = original.map(Ok).unwrap_or_else(|| src.load())?;
                let t = if p.develop.enhance.ai.is_none() && p.develop.enhance.denoise == 0. {
                    1.
                } else {
                    (p.develop.enhance.denoise / 100.).clamp(0., 1.) as f32
                };
                if original.width != full.width || original.height != full.height {
                    return Err("denoise source size mismatch".into());
                }
                for (a, b) in full.data.iter_mut().zip(&original.data) {
                    for c in 0..3 {
                        a[c] = b[c] + t * (a[c] - b[c]);
                    }
                }
                export_new(&path, &full, &p, ctl)?;
                return Ok(Outcome::DenoiseExport(path.to_string_lossy().into()));
            }
            ctl.report(0.92, "Saving");
            let source = source_hash(&p);
            let tag = if override_fn.is_some() {
                "custom".into()
            } else if let Some(c) = &custom {
                let bytes = std::fs::read(c.path(dir.as_deref().ok_or("no models folder")?)).map_err(|e| e.to_string())?;
                format!("custom:{}:{variant:?}", li_seg::denoise::sha256_hex(&bytes))
            } else {
                format!("{}:{variant:?}", if family == DenoiseModel::Raw { model_tag() } else { "denoise-nind-v1".into() })
            };
            let tag = if raw_key == 0 { tag } else { format!("{tag}:{raw_key:016x}") };
            let model = if preview { format!("{tag}:preview") } else { tag };
            let key = result_key(&source, &model);
            store::put_rgb(Kind::Denoise, &key, &full, to.as_deref())?;
            let (pw, ph) = fit(full.width, full.height, PREVIEW_EDGE);
            if (pw, ph) != (full.width, full.height) {
                store::put_rgb(Kind::Denoise, &preview_key(&key), &full.resize(pw, ph, Filter::Mitchell), to.as_deref())?;
            }
            forget_levels(&key);
            ctl.report(1., "Done");
            Ok(Outcome::Denoise(DenoiseDone { key, source, model, family, preview, raw: raw_key }))
        });
        Ok(self.enhance.spawn(
            id,
            if exporting { JobKind::DenoiseExport } else { JobKind::Denoise },
            if exporting { "Denoise to new file" } else { "AI Denoise" },
            wait,
            work,
        ))
    }
}

/// Force the input's CFA phase to RGGB by translation, retaining spatial phase in the output.
/// Black subtraction and per-position white normalization have already happened; WB is absent.
pub fn pack_bayer(n: &lightcraft_raw::Normalized) -> Result<(Vec<f32>, usize, usize, [usize; 2]), String> {
    let c = n.cfa.as_ref().filter(|c| c.is_bayer()).ok_or("not Bayer")?;
    if n.cpp != 1 || n.data.len() != n.width * n.height || n.width < 2 || n.height < 2 {
        return Err("invalid Bayer input".into());
    }
    let pos = c.pattern.iter().position(|&v| v == 0).ok_or("no red photosites")?;
    let offset = [pos % 2, pos / 2];
    let (w, h) = (n.width.div_ceil(2), n.height.div_ceil(2));
    let mut out = vec![0.; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            for (k, [dx, dy]) in [[0, 0], [1, 0], [0, 1], [1, 1]].into_iter().enumerate() {
                // Repeat the final same-color photosite at odd dimensions, keeping the CFA phase.
                let same_phase = |q: usize, n: usize| if q < n { q } else { q - (q - n + 1).div_ceil(2) * 2 };
                let sx = same_phase(2 * x + offset[0] + dx, n.width);
                let sy = same_phase(2 * y + offset[1] + dy, n.height);
                out[(y * w + x) * 4 + k] = n.data[sy * n.width + sx].clamp(0., 1.);
            }
        }
    }
    Ok((out, w, h, offset))
}
pub fn unpack_camera(data: Vec<[f32; 3]>, w: usize, h: usize, ow: usize, oh: usize, offset: [usize; 2]) -> RgbFrame {
    let mut out = RgbFrame::new(ow, oh);
    for y in 0..oh {
        for x in 0..ow {
            out.data[y * ow + x] = data[y.saturating_sub(offset[1]).min(h - 1) * w + x.saturating_sub(offset[0]).min(w - 1)];
        }
    }
    out
}
/// Apply the source camera transform to camera RGB, without changing crop or orientation. The
/// camera look's nonlinear hue/saturation table stays with the pipeline, as for the source.
pub fn camera_rgb_to_working(raw: &lightcraft_raw::RawImage, bytes: &[u8], rgb: &mut RgbFrame) {
    let t = lightcraft_raw::color::camera_transform(raw, lightcraft_raw::color::as_shot_white_xy(raw));
    let lens = crate::files::embedded_lens(&raw.info());
    let look = crate::camera_preview::fit_preview(raw, bytes, &t, lens.as_ref());
    let matrix = look.as_ref().map_or(t.matrix, |p| p.matrix.mul(&t.matrix));
    let gain = 2f32.powf(t.baseline_exposure as f32);
    rgb.map_in_place(|p| matrix.apply_f32(std::array::from_fn(|c| p[c] * t.wb[c])).map(|v| v * gain));
}
/// How far around clipped photosites the source decode replaces the model, and the soft edge.
const CLIP_GROW: usize = 4;
const CLIP_FEATHER: usize = 2;
/// AI Denoise of a Bayer raw's mosaic, prepared by [`crate::files::prepare_raw`]. `infer` turns
/// the packed RGGB planes (`w × h`, four values per pixel) into camera RGB at twice their size.
/// The result matches `source` (the photo's normal decode, loaded once the model has run): the
/// same working space, crop and orientation. Where the sensor clipped it is `source` itself,
/// whose highlight recovery the model can't do (and clipped photosites carry no noise).
fn denoise_bayer(
    raw: &lightcraft_raw::RawImage,
    bytes: &[u8],
    source: impl FnOnce() -> Result<crate::media::DecodedSource, String>,
    infer: impl FnOnce(&[f32], usize, usize) -> Result<Vec<[f32; 3]>, String>,
) -> Result<(RgbFrame, Arc<RgbFrame>), String> {
    let n = raw.normalized().map_err(|e| e.to_string())?;
    let (packed, w, h, offset) = pack_bayer(&n)?;
    let out = infer(&packed, w, h)?;
    drop(packed);
    if out.len() != w * h * 4 {
        return Err("the denoiser returned invalid pixels".into());
    }
    let camera = unpack_camera(out, w * 2, h * 2, n.width, n.height, offset);
    drop(n);
    let mut img = camera_source(raw, bytes, camera);
    let source = source()?;
    if (source.image.width, source.image.height) != (img.width, img.height) {
        return Err("denoise source size mismatch".into());
    }
    if let Some(clip) = source.info.as_ref().and_then(|i| i.clip_confidence.as_ref()) {
        keep_clipped(&mut img, &source.image, clip);
    }
    Ok((img, source.image))
}
/// `img` takes `source`'s pixels where the sensor clipped, [`CLIP_GROW`] pixels around them and a
/// [`CLIP_FEATHER`] edge.
fn keep_clipped(img: &mut RgbFrame, source: &RgbFrame, clip: &lightcraft_pipeline::ClipConfidence) {
    if (clip.width, clip.height) != (img.width, img.height) || !clip.data.iter().any(|v| *v > 0.0) {
        return;
    }
    let clipped =
        li_raster::GrayFrame { width: clip.width, height: clip.height, data: clip.data.iter().map(|v| f32::from(u8::from(*v > 0.0))).collect() };
    let mut weight = clipped.max_filter(CLIP_GROW).data;
    li_raster::blur::box_blur_n(&mut weight, clip.width, clip.height, 1, CLIP_FEATHER);
    for ((o, s), t) in img.data.iter_mut().zip(&source.data).zip(&weight) {
        let t = t.clamp(0.0, 1.0);
        if t > 0.0 {
            for c in 0..3 {
                o[c] += (s[c] - o[c]) * t;
            }
        }
    }
}
fn camera_source(raw: &lightcraft_raw::RawImage, bytes: &[u8], mut rgb: RgbFrame) -> RgbFrame {
    lightcraft_raw::opcodes::apply_list3(&raw.opcodes.list3, &mut rgb);
    camera_rgb_to_working(raw, bytes, &mut rgb);
    let c = raw.crop.clipped(rgb.width, rgb.height);
    if c.width > 0 && c.height > 0 {
        rgb = rgb.into_crop(c.x, c.y, c.width, c.height);
    }
    rgb.into_oriented(raw.orientation)
}
fn to_display(img: &RgbFrame) -> Vec<[f32; 3]> {
    let m = li_color::SRGB.from_xyz().mul(&li_color::REC2020.to_xyz());
    img.data.iter().map(|p| m.apply_f32(*p).map(|v| li_color::transfer::linear_to_srgb(v).clamp(0., 1.))).collect()
}
fn from_display(data: &mut [[f32; 3]]) {
    let m = li_color::REC2020.from_xyz().mul(&li_color::SRGB.to_xyz());
    for p in data {
        *p = m.apply_f32(p.map(li_color::transfer::srgb_to_linear));
    }
}
fn export_new(path: &std::path::Path, img: &RgbFrame, p: &lightcraft_catalog::Photo, ctl: &JobCtl) -> Result<(), String> {
    use lightcraft_codecs::encode::{EncodeImage, EncodeMeta, Samples, TiffCompression};
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    ctl.report(0.93, "Saving");
    let mut metadata = crate::export::export_metadata(p, &crate::export::ExportOptions::default()).unwrap_or_default();
    metadata.width = Some(img.width as u32);
    metadata.height = Some(img.height as u32);
    metadata.orientation = Some(lightcraft_meta::Orientation::Normal);
    let bytes = if ext == "tif" || ext == "tiff" {
        let display = to_display(img);
        let icc = lightcraft_codecs::icc::write_named(lightcraft_codecs::NamedSpace::Srgb);
        let samples: Vec<u16> = display.iter().flatten().map(|v| (v * 65535. + 0.5) as u16).collect();
        let exif = lightcraft_meta::write_exif(&metadata);
        lightcraft_codecs::encode::encode_tiff(
            &EncodeImage::new(img.width as u32, img.height as u32, 3, Samples::U16(&samples)),
            TiffCompression::Deflate,
            &EncodeMeta { icc: Some(&icc), exif: Some(&exif), ..Default::default() },
        )
        .map_err(|e| e.to_string())?
    } else if ext == "dng" {
        // These pixels are already in working RGB. Camera identification must not cause this
        // app (or another raw editor) to assign the original camera's DCP a second time.
        metadata.make = Some("Local Image".into());
        metadata.model = Some("Denoised linear Rec.2020".into());
        let color = lightcraft_raw::ColorData {
            illuminant: [21, 0],
            color_matrix: [Some(li_color::REC2020.from_xyz()), None],
            forward_matrix: [Some(li_color::bradford(li_color::D65, li_color::D50).mul(&li_color::REC2020.to_xyz())), None],
            as_shot_neutral: Some([1.; 3]),
            ..Default::default()
        };
        let raw = lightcraft_raw::RawImage {
            format: lightcraft_raw::RawFormat::Dng,
            width: img.width,
            height: img.height,
            cpp: 3,
            data: lightcraft_raw::RawData::F32(img.data.as_flattened().to_vec()),
            cfa: None,
            bits: 32,
            black: lightcraft_raw::BlackLevel::uniform(0.),
            white: vec![1.; 3],
            active_area: lightcraft_raw::Rect::new(0, 0, img.width, img.height),
            crop: lightcraft_raw::Rect::new(0, 0, img.width, img.height),
            orientation: lightcraft_geom::Orientation::Normal,
            color,
            wb_multipliers: None,
            linearized: false,
            vendor_lens: None,
            opcodes: Default::default(),
            metadata,
        };
        lightcraft_raw::write_dng(&raw, &lightcraft_raw::DngWriteOptions::default()).map_err(|e| e.to_string())?
    } else {
        return Err("choose a new .dng or .tiff file".into());
    };
    ctl.check()?;
    li_fs::write_new(path, &bytes).map_err(|e| e.to_string())?;
    ctl.report(1., "Done");
    Ok(())
}

fn fit(w: usize, h: usize, edge: usize) -> (usize, usize) {
    let long = w.max(h);
    if long <= edge {
        return (w, h);
    }
    let k = edge as f64 / long as f64;
    (((w as f64 * k).round() as usize).max(1), ((h as f64 * k).round() as usize).max(1))
}

/// Whether a stored result exists for `key` (cached; no file system access once known).
pub fn result_exists(key: &str) -> bool {
    store::exists(Kind::Denoise, key)
}

struct Mixed {
    src: Weak<RgbFrame>,
    key: String,
    strength: u32,
    out: Arc<RgbFrame>,
}

/// Recently mixed sources (so a view's stage cache sees the same buffer while the amount holds).
static MIXED: Mutex<Vec<Mixed>> = Mutex::new(Vec::new());
/// Recently read stored results (store key → pixels).
static LEVELS: Mutex<Vec<(String, Arc<RgbFrame>)>> = Mutex::new(Vec::new());

fn forget_levels(key: &str) {
    LEVELS.lock().unwrap_or_else(|e| e.into_inner()).retain(|(k, _)| !k.starts_with(key));
    MIXED.lock().unwrap_or_else(|e| e.into_inner()).retain(|m| m.key != key);
}

/// The stored result for `key` at a level fitting `long` px (the preview-size copy when it is
/// enough), read once and kept for a while.
fn level(key: &str, long: usize) -> Option<Arc<RgbFrame>> {
    let pk = preview_key(key);
    let candidates = if long <= PREVIEW_EDGE { vec![pk, key.to_owned()] } else { vec![key.to_owned()] };
    for k in candidates {
        if let Some(img) = LEVELS.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(c, _)| *c == k).map(|e| e.1.clone()) {
            return Some(img);
        }
        if let Some(img) = store::get_rgb(Kind::Denoise, &k) {
            let img = Arc::new(img);
            let mut l = LEVELS.lock().unwrap_or_else(|e| e.into_inner());
            l.insert(0, (k, img.clone()));
            l.truncate(2);
            return Some(img);
        }
    }
    None
}

/// `src` (a decoded source of the photo, any level) with the Denoise result `key` mixed in by
/// `amount` (0..100). `None` when there is nothing to mix (amount 0, or no stored result: render
/// from `src` itself).
pub fn denoised_source(src: &Arc<RgbFrame>, key: &str, amount: f64) -> Option<Arc<RgbFrame>> {
    let t = (amount / 100.0).clamp(0.0, 1.0) as f32;
    if t <= 0.0 || !result_exists(key) {
        return None;
    }
    let strength = t.to_bits();
    {
        let mut m = MIXED.lock().unwrap_or_else(|e| e.into_inner());
        m.retain(|e| e.src.strong_count() > 0);
        if let Some(e) = m.iter().find(|e| e.key == key && e.strength == strength && e.src.upgrade().is_some_and(|s| Arc::ptr_eq(&s, src))) {
            return Some(e.out.clone());
        }
    }
    let den = level(key, src.width.max(src.height))?;
    let den = if (den.width, den.height) == (src.width, src.height) { den } else { Arc::new(den.resize(src.width, src.height, Filter::Mitchell)) };
    let mut out = (**src).clone();
    for (o, d) in out.data.iter_mut().zip(&den.data) {
        for c in 0..3 {
            o[c] += (d[c] - o[c]) * t;
        }
    }
    let out = Arc::new(out);
    let mut m = MIXED.lock().unwrap_or_else(|e| e.into_inner());
    m.insert(0, Mixed { src: Arc::downgrade(src), key: key.to_owned(), strength, out: out.clone() });
    m.truncate(3);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_denoise_camera_source_keeps_post_demosaic_opcodes_crop_and_orientation() {
        use lightcraft_raw::opcodes::{Area, Opcode, OpcodeLists};
        let raw = lightcraft_raw::RawImage {
            format: lightcraft_raw::RawFormat::Dng,
            width: 8,
            height: 6,
            cpp: 1,
            data: lightcraft_raw::RawData::F32((0..48).map(|i| 0.1 + i as f32 * 0.01).collect()),
            cfa: lightcraft_raw::Cfa::bayer("RGGB"),
            bits: 32,
            black: lightcraft_raw::BlackLevel::uniform(0.),
            white: vec![1.],
            active_area: lightcraft_raw::Rect::new(0, 0, 8, 6),
            crop: lightcraft_raw::Rect::new(1, 1, 4, 3),
            orientation: lightcraft_geom::Orientation::Rotate90,
            color: Default::default(),
            wb_multipliers: None,
            linearized: false,
            vendor_lens: None,
            opcodes: OpcodeLists {
                list3: vec![Opcode::ScalePerRow {
                    area: Area { top: 0, left: 0, bottom: 6, right: 8, plane: 0, planes: 3, row_pitch: 1, col_pitch: 1 },
                    scales: vec![0.5, 1., 0.75, 1.25, 0.5, 1.],
                }],
                ..Default::default()
            },
            metadata: Default::default(),
        };
        // The established raw developer is the oracle for corrections applied after the
        // demosaic-equivalent model head, before the crop and final orientation.
        let camera = lightcraft_raw::demosaic(&raw.normalized().unwrap(), lightcraft_raw::Method::Bilinear);
        let actual = camera_source(&raw, &[], camera);
        let mut expected = raw.develop(lightcraft_raw::Method::Bilinear).unwrap();
        camera_rgb_to_working(&raw, &[], &mut expected);
        let expected = expected.into_oriented(raw.orientation);
        assert_eq!((actual.width, actual.height), (3, 4));
        assert_eq!(actual.data, expected.data);
    }

    /// A grey RGGB scene under as-shot multipliers (2.4, 1, 1.6) with a block at the white level
    /// (a blown window), cropped oddly and turned a quarter.
    fn clipped_bayer_dng() -> Vec<u8> {
        use lightcraft_raw::*;
        let (w, h) = (96usize, 64usize);
        let cfa = Cfa::bayer("RGGB").unwrap();
        let (black, white) = (256.0, 16000.0);
        let data = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                if (40..72).contains(&x) && (16..48).contains(&y) {
                    return white as u16;
                }
                let grey = 0.25 + 0.1 * x as f32 / w as f32;
                let v = grey / [2.4, 1.0, 1.6][cfa.color_at(x, y) as usize];
                (black + v * (white - black)) as u16
            })
            .collect();
        let raw = RawImage {
            format: RawFormat::Dng,
            width: w,
            height: h,
            cpp: 1,
            data: RawData::U16(data),
            cfa: Some(cfa),
            bits: 14,
            black: BlackLevel::uniform(black),
            white: vec![white],
            active_area: Rect::new(0, 0, w, h),
            crop: Rect::new(3, 1, 89, 61),
            orientation: lightcraft_geom::Orientation::Rotate90,
            color: ColorData {
                illuminant: [21, 0],
                color_matrix: [Some(li_color::Mat3([[0.9, 0.2, -0.15], [-0.3, 1.25, 0.08], [0.02, -0.12, 0.85]])), None],
                as_shot_neutral: Some([1.0 / 2.4, 1.0, 1.0 / 1.6]),
                ..Default::default()
            },
            wb_multipliers: None,
            linearized: false,
            vendor_lens: None,
            opcodes: Default::default(),
            metadata: Default::default(),
        };
        write_dng(&raw, &Default::default()).unwrap()
    }

    /// A stand-in Bayer model: each 2×2 block becomes [R, mean G, B] scaled by `gain`.
    fn bayer_stub(gain: f32) -> impl FnOnce(&[f32], usize, usize) -> Result<Vec<[f32; 3]>, String> {
        move |packed, w, h| {
            let mut out = vec![[0.0; 3]; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    let p = &packed[(y * w + x) * 4..][..4];
                    let rgb = [p[0], (p[1] + p[2]) / 2.0, p[3]].map(|v| v * gain);
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        out[(2 * y + dy) * 2 * w + 2 * x + dx] = rgb;
                    }
                }
            }
            Ok(out)
        }
    }

    #[test]
    fn ai_denoise_bayer_keeps_the_source_highlight_recovery_where_the_sensor_clipped() {
        let bytes = clipped_bayer_dng();
        let opts = crate::files::RawOptions::default();
        let (reference, info) = crate::files::load_bytes_with(&bytes, usize::MAX, &opts).unwrap();
        let clip = info.clip_confidence.clone().expect("the raw has a clip map");
        assert!(clip.data.iter().any(|v| *v >= 1.0), "the block is clipped");
        let raw = crate::files::prepare_raw(&bytes, &opts).unwrap();
        let reference = Arc::new(reference);
        let decoded = crate::media::DecodedSource::new(reference.clone(), Some(info));
        let (img, original) = denoise_bayer(&raw, &bytes, || Ok(decoded), bayer_stub(0.5)).unwrap();
        assert!(Arc::ptr_eq(&original, &reference), "the normal decode is handed back for export");
        assert_eq!((img.width, img.height), (reference.width, reference.height));
        let near_clip =
            li_raster::GrayFrame { width: clip.width, height: clip.height, data: clip.data.clone() }.max_filter(CLIP_GROW + CLIP_FEATHER + 2);
        let (mut inside, mut outside) = (0, 0);
        for (i, (d, r)) in img.data.iter().zip(reference.data.iter()).enumerate() {
            if clip.data[i] >= 1.0 {
                inside += 1;
                // the source's reconstruction, not the camera's white turned magenta by white balance
                for c in [0, 2] {
                    let (got, want) = (d[c] / d[1], r[c] / r[1]);
                    assert!((got / want - 1.0).abs() < 0.01, "pixel {i}: ratio {got} vs {want} ({d:?} vs {r:?})");
                }
            } else if near_clip.data[i] == 0.0 {
                outside += 1;
                for c in 0..3 {
                    assert!((d[c] - 0.5 * r[c]).abs() < 0.02 * r[c].abs() + 1e-4, "pixel {i}: the model's result is kept: {d:?} vs {r:?}");
                }
            }
        }
        assert!(inside > 100 && outside > 1000, "{inside} clipped, {outside} away from the clip");
    }

    #[test]
    fn ai_denoise_result_is_not_applied_after_the_raw_decoding_changes() {
        let mut d = lightcraft_develop::DevelopSettings::default();
        let key = result_key("0123", "raw-decoding-test");
        store::put_rgb(Kind::Denoise, &key, &RgbFrame { width: 4, height: 4, data: vec![[0.5; 3]; 16] }, None).unwrap();
        let source = lightcraft_preview::Hash128(0x5eed);
        d.enhance.denoise = 100.0;
        d.enhance.ai = Some(lightcraft_develop::DenoiseRef {
            key: lightcraft_develop::AiKey::parse(&key).unwrap(),
            source: lightcraft_develop::AiKey(source.0),
            model: DenoiseModel::Raw,
            preview: false,
            raw: source_options(&d),
        });
        let src = Arc::new(RgbFrame { width: 4, height: 4, data: vec![[0.1; 3]; 16] });
        let mixed = |d: &lightcraft_develop::DevelopSettings| super::super::for_render(&src, &Arc::new(d.clone()), Some(source)).0.data[0][0];
        assert_eq!(mixed(&d), 0.5, "made under these settings: applied");
        let mut highlights = d.clone();
        highlights.raw.highlights = lightcraft_develop::HighlightMode::Clip;
        let mut profile = d.clone();
        profile.camera_profile = "matrix".into();
        for changed in [&highlights, &profile] {
            assert!(!matches_decoding(&changed.enhance.ai.unwrap(), changed));
            assert_eq!(mixed(changed), 0.1, "made under other raw settings: not applied");
        }
        let mut capture = d.clone();
        capture.raw.capture.enabled = !capture.raw.capture.enabled;
        assert_eq!(mixed(&capture), 0.5, "capture sharpening doesn't change the decoded source");
        store::delete(Kind::Denoise, &key);
    }

    /// A stand-in denoiser: every pixel becomes the image mean (noise gone, brightness kept).
    fn flatten() -> DenoiseFn {
        Arc::new(|px, _, _, progress| {
            let n = px.len().max(1) as f32;
            let m = px.iter().fold([0.0f32; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]).map(|v| v / n);
            if !progress(1, 1) {
                return Err(CANCELLED.into());
            }
            Ok(vec![m; px.len()])
        })
    }

    fn demo_raw(s: &mut Session) -> PhotoId {
        s.visible_cloned().into_iter().find(|id| can_denoise(s, *id).is_ok()).expect("a scene-referred demo photo")
    }

    #[test]
    fn denoise_runs_stores_and_mixes_by_amount() {
        let mut s = Session::with_demo();
        let id = demo_raw(&mut s);
        assert!(s.denoiser().is_err(), "no model installed");
        s.enhance.denoiser = Some(flatten());
        let job = s.start_denoise(id, true).expect("starts");
        assert!(job.finished());
        let done = match s.enhance.take_finished().pop().map(|(_, r)| r) {
            Some(Ok(Outcome::Denoise(d))) => d,
            other => panic!("{other:?}"),
        };
        let p = s.catalog.photo(id).unwrap().clone();
        assert_eq!(done.source, source_hash(&p));
        assert_eq!(done.key, result_key(&done.source, "custom"));
        assert!(result_exists(&done.key));
        // a preview-level source gets the result scaled to its size, mixed by the amount
        let src = s.source_now(id, SourceLevel::Thumb).unwrap();
        let flat = denoised_source(&src, &done.key, 100.0).expect("mixed");
        assert_eq!((flat.width, flat.height), (src.width, src.height));
        let spread = |img: &RgbFrame| {
            let (lo, hi) = img.data.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
            hi - lo
        };
        assert!(spread(&flat) < spread(&src) * 0.2, "{} vs {}", spread(&flat), spread(&src));
        let half = denoised_source(&src, &done.key, 50.0).unwrap();
        let i = src.data.len() / 3;
        assert!((half.data[i][1] - (src.data[i][1] + flat.data[i][1]) / 2.0).abs() < 1e-4);
        // the same request gives the same buffer (stage caches stay warm)
        assert!(Arc::ptr_eq(&half, &denoised_source(&src, &done.key, 50.0).unwrap()));
        assert!(denoised_source(&src, &done.key, 0.0).is_none());
        // the result gone: render from the source itself
        store::delete(Kind::Denoise, &done.key);
        assert!(denoised_source(&src, &done.key, 50.0).is_none());
    }

    #[test]
    fn denoise_is_refused_where_it_cannot_run_and_can_be_cancelled() {
        let mut s = Session::with_demo();
        let id = demo_raw(&mut s);
        let slow: DenoiseFn = Arc::new(|px, _, _, progress| {
            for i in 0..1000 {
                if !progress(i, 1000) {
                    return Err(CANCELLED.into());
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Ok(px.to_vec())
        });
        s.enhance.denoiser = Some(slow);
        let job = s.start_denoise(id, false).unwrap();
        assert!(s.start_denoise(id, false).is_err(), "one at a time");
        s.enhance.cancel(Some(job.id));
        while !job.finished() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(s.enhance.take_finished().pop().map(|(_, r)| r), Some(Err(CANCELLED.to_string())));
        assert!(s.start_denoise(PhotoId(u64::MAX), true).is_err());
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    #[test]
    fn ai_denoise_auto_routes_a_rendered_file_through_display_rgb() {
        let path = std::env::temp_dir().join(format!("li-denoise-rgb-{}.png", std::process::id()));
        let pixels = vec![128u8; 16 * 16 * 3];
        let bytes = lightcraft_codecs::encode_png(
            &lightcraft_codecs::EncodeImage::new(16, 16, 3, lightcraft_codecs::Samples::U8(&pixels)),
            &Default::default(),
        )
        .unwrap();
        std::fs::write(&path, bytes).unwrap();
        let mut s = Session::new().with_fs();
        s.execute("library.import", &serde_json::json!({"paths":[path.to_string_lossy()]})).unwrap();
        let id = s.active().unwrap();
        s.enhance.denoiser = Some(Arc::new(|p, _, _, _| {
            for pixel in p {
                for v in pixel {
                    assert!((*v - 128. / 255.).abs() < 1e-5, "display contract {pixel:?}");
                }
            }
            Ok(p.to_vec())
        }));
        s.start_denoise(id, true).unwrap();
        let done = s.enhance.take_finished().pop().unwrap().1.unwrap();
        assert!(matches!(done, Outcome::Denoise(DenoiseDone { family: DenoiseModel::Rgb, .. })));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn ai_denoise_packing_uses_sensor_values_in_all_four_phases() {
        for name in ["RGGB", "GRBG", "GBRG", "BGGR"] {
            let cfa = lightcraft_raw::Cfa::bayer(name).unwrap();
            let (w, h) = (7, 5);
            let data = (0..w * h).map(|i| [0.1, 0.2, 0.3][cfa.color_at(i % w, i / w) as usize]).collect();
            let n = lightcraft_raw::Normalized { width: w, height: h, cpp: 1, data, cfa: Some(cfa) };
            let (p, pw, ph, _) = pack_bayer(&n).unwrap();
            assert_eq!((pw, ph), (4, 3));
            for p in p.as_chunks::<4>().0 {
                assert_eq!(*p, [0.1, 0.2, 0.2, 0.3], "{name}: no WB or misplaced red/blue");
            }
        }
    }
    #[test]
    fn ai_denoise_result_is_foreign_after_settings_copy_and_preview_cannot_export() {
        let mut s = Session::with_demo();
        let id = s.active().unwrap();
        s.enhance.denoiser = Some(Arc::new(|p, _, _, _| Ok(vec![[0.1, 0.2, 0.3]; p.len()])));
        let job = s.start_denoise_with(id, true, None, true).unwrap();
        assert!(job.finished());
        assert!(s.enhance_poll().errors.is_empty());
        let d = s.develop_of(id).unwrap();
        assert!(d.enhance.ai.unwrap().preview);
        assert!(s.export_job(id, 0, 0, lightcraft_pipeline::OutputSpace::Srgb, lightcraft_pipeline::OutputDepth::U8).is_err());
        let src = Arc::new(RgbFrame::new(32, 24));
        let (out, _) = super::super::session::for_render(&src, &d, Some(lightcraft_preview::Hash128(987)));
        assert!(Arc::ptr_eq(&out, &src));
        s.start_denoise(id, true).unwrap();
        s.enhance_poll();
        assert!(!s.develop_of(id).unwrap().enhance.ai.unwrap().preview);
    }
    #[test]
    fn ai_denoise_exports_new_tiff_and_dng_with_neutral_working_wb() {
        let s = Session::with_demo();
        let p = s.catalog.photo(s.active().unwrap()).unwrap();
        let img = RgbFrame { width: 8, height: 6, data: vec![[0.2; 3]; 48] };
        let ctl = JobCtl::default();
        let dir = std::env::temp_dir().join(format!("li-denoise-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for ext in ["dng", "tiff"] {
            let path = dir.join(format!("new.{ext}"));
            let _ = std::fs::remove_file(&path);
            export_new(&path, &img, p, &ctl).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            assert!(export_new(&path, &img, p, &ctl).is_err(), "never overwrite an existing file");
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            if ext == "dng" {
                let raw = lightcraft_raw::decode(&bytes).unwrap();
                assert_eq!(raw.cfa, None);
                assert_eq!(raw.cpp, 3);
                assert_eq!(raw.color.as_shot_neutral, Some([1.; 3]));
                let t = lightcraft_raw::color::camera_transform(&raw, lightcraft_raw::color::as_shot_white_xy(&raw));
                let pixel = t.matrix.apply_f32(std::array::from_fn(|c| raw.data.get(c) * t.wb[c]));
                for v in pixel {
                    assert!((v - 0.2).abs() < 2e-4, "{pixel:?}");
                }
                let (loaded, _) = crate::files::load_bytes(&bytes, usize::MAX).unwrap();
                for p in loaded.data {
                    for v in p {
                        assert!((v - 0.2).abs() < 2e-4, "source reload {v}");
                    }
                }
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod gpu_only_tests {
    #[test]
    fn denoise_cpu_request_refused_before_model_loading() {
        let error = super::infer_selected(std::path::Path::new("not-installed"), li_seg::denoise::Variant::Rgb, None, &[], 1, 1, false, &|_, _| true)
            .unwrap_err();
        assert!(error.contains("CPU inference is disabled"));
    }
}
