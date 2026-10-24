//! The LightCraft develop pipeline (CPU reference implementation).
//!
//! Input: a scene-referred, linear Rec.2020 source image (already EXIF-oriented) at any resolution
//! (full size or a proxy), plus [`DevelopSettings`]. Output: a display-encoded sRGB image at the
//! requested size, and its histogram.
//!
//! Stage order:
//! 1. geometry — user orientation, lens corrections (distortion, CA, vignetting), perspective, crop +
//!    straighten, flips; one resample at output resolution; then defringe
//! 2. white balance/camera calibration, reconstruction input, retouching and noise reduction
//! 3. primary scene tools (`primary`): exposed log-EV Highlights/Shadows/Whites/Blacks,
//!    local-Laplacian Clarity, EIGF Texture/Structure, UCS22 colour tools and Skin Tone;
//!    Background and imported Lightroom local adjustments, with a fixed geometric tone proxy
//! 4. established scene/finish tools: dehaze, local exposure, faithful EIGF tone equalizer,
//!    contrast tone map, curves and Point Color (OkLCh), vignette, grain and sharpening
//! 5. adjustment layers — each visible `Mask::tools` runs its complete tool set on the finished
//!    result below it, in stored list order, and blends once by coverage × amount × opacity;
//!    inputs/outputs are unclipped, display-referred linear Rec.2020; spatial fields are rebuilt
//!    from this layer's input, and a layer never repeats the camera tone map or RAW reconstruction
//! 6. display: output gamut mapping, proofing and transfer encoding once (sRGB by default)
//!
//! Spatial parameters are specified relative to the image's long edge, so a 400 px preview and a
//! 60 MP export look alike.
//!
//! The primary stage temporarily exposes its input for perceptual tone/colour calculations,
//! then removes that gain so the established finish applies exposure once. With
//! [`render_cached`] each stage's output is reused while its inputs are unchanged ([`StageCache`]);
//! primary edits reuse geometry and WB/NR, and finish-only edits reuse the primary result.
//! With adjustment layers, completed Background/layer prefixes are cached independently;
//! editing k re-evaluates k..top, while a Background edit invalidates the entire stack.
//! No-layer documents retain their original render path and golden hashes.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(test)]
mod test_vectors;
#[cfg(test)]
mod tests_lenscorr_merge;

pub mod auto;
pub mod balance;
pub mod blur;
pub mod capture;
pub mod colorcal;
pub mod colorequal;
pub mod colorops;
pub mod cull;
pub mod detail;
pub mod dust;
pub mod eigf;
pub mod finish;
pub mod geometry;
pub mod layers;
pub mod lcp;
pub mod lensdb;
pub mod llf;
pub mod local;
pub mod lut;
pub mod masks;
pub mod metrics;
pub mod negative;
pub mod optics;
pub mod output;
pub mod patches;
pub mod primary;
pub mod profiles;
pub mod redeye;
pub mod source_color;
pub mod spots;
pub mod tone;
pub mod tone2;
pub mod toneeq;
pub mod transform;
pub mod ucs;
pub mod upright;
pub mod visualize;

pub use output::{DeepImage, DeepSamples, OutputDepth, OutputSpace, OutputTrc, Proof, ProofIntent, ProofProfile};
pub use visualize::{MaskView, Overlay};

use lightcraft_develop::{DevelopSettings, Treatment};
use std::any::Any;
use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use li_raster::{GrayFrame, Histogram, RgbFrame, Rgba8, par_rows};

pub use tone::ToneMap;

/// A sensor-space confidence plane, in EXIF-oriented source coordinates.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ClipConfidence {
    pub width: usize,
    pub height: usize,
    #[serde(serialize_with = "serialize_clip_runs", deserialize_with = "deserialize_clip_runs")]
    pub data: Vec<f32>,
}
// Lossless runs keep the mostly-unclipped sensor plane small in smart-preview headers.
fn serialize_clip_runs<S: serde::Serializer>(data: &[f32], serializer: S) -> Result<S::Ok, S::Error> {
    use serde::Serialize;
    let mut runs: Vec<(usize, f32)> = Vec::new();
    for &v in data {
        if let Some((n, prev)) = runs.last_mut()
            && prev.to_bits() == v.to_bits()
        {
            *n += 1;
        } else {
            runs.push((1, v));
        }
    }
    runs.serialize(serializer)
}
fn deserialize_clip_runs<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<f32>, D::Error> {
    use serde::Deserialize;
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Data {
        Runs(Vec<(usize, f32)>),
        Plain(Vec<f32>),
    }
    match Data::deserialize(deserializer)? {
        Data::Plain(v) => Ok(v),
        Data::Runs(runs) => {
            let size = runs
                .iter()
                .try_fold(0usize, |n, (k, _)| n.checked_add(*k))
                .filter(|n| *n <= 100_000_000)
                .ok_or_else(|| serde::de::Error::custom("clip-confidence plane is too large"))?;
            let mut data = Vec::with_capacity(size);
            for (n, v) in runs {
                data.extend(std::iter::repeat_n(v, n));
            }
            Ok(data)
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CameraWbPreset {
    pub name: String,
    pub temp: f64,
    pub tint: f64,
}

/// Facts about the source the settings are interpreted against.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SourceInfo {
    /// Sensor fraction of clipped channels, before reconstruction and camera conversion.
    pub clip_confidence: Option<Arc<ClipConfidence>>,
    pub raw_clip_level: Option<f32>,
    pub camera_noise: Option<detail::nr::NoiseCalibration>,
    pub camera_wb_presets: Vec<CameraWbPreset>,
    /// Embedded DNG/vendor correction relative to the EXIF-oriented source.
    pub lens: Option<lightcraft_develop::EmbeddedLens>,
    /// Original Sony curves from the single lc-meta reader; converted only if no embedded map exists.
    pub sony_lens: Option<lightcraft_develop::SonyLensData>,
    pub source_orientation: lightcraft_geom::Orientation,
    /// Linear camera -> working basis for calibrated per-plane CA.
    pub lens_camera_to_working: Option<li_color::Mat3>,
    /// Nonlinear fitted look is evaluated after lens geometry.
    pub source_hue_sat: Option<Arc<li_color::profile::HsvTable>>,
    /// Ordered built-in DNG lens operations already applied in camera RGB.
    pub embedded_lens_baked: bool,
    pub embedded_ca_baked: bool,
    /// Raw sources use absolute Kelvin white balance; rendered sources use a relative scale around
    /// their as-shot white.
    pub raw: bool,
    pub as_shot_temp: f64,
    pub as_shot_tint: f64,
    /// No measured camera illuminant: WB adjustments are relative to the camera's rendered look.
    pub relative_wb: bool,
    pub camera_color: Option<li_color::camera::CameraWhite>,
    /// DNG/DCP tables evaluated after WB for the chosen illuminant (CPU stage inside GPU renders).
    pub camera_profile: Option<Arc<li_color::profile::ProfileLook>>,
    /// Baseline exposure already present in the decoded source, undone before the base table.
    pub baseline_gain: f32,
    pub camera_tone: Option<tone::CameraTone>,
    /// The lens database's correction for the photo's lens (set by the engine when the settings
    /// ask for it; relative to the EXIF-oriented source).
    #[serde(skip)]
    pub lens_db: Option<lensdb::LensCorrection>,
    /// User-imported Adobe LCP, interpolated for the shot.
    #[serde(skip)]
    pub lcp: Option<lcp::LcpCorrection>,
    /// Sensor pixels per source pixel (a binned or downscaled raw preview is > 1), for tools
    /// sized in sensor pixels (capture sharpening).
    pub sensor_scale: f32,
    /// Capture sharpening radius measured from the raw data (sensor pixels), when it was asked for.
    pub capture_radius: Option<f32>,
    /// Capture sharpening's contrast threshold for this sensor and ISO (0..1).
    pub capture_threshold: f32,
    /// Camera look: the camera's own curve (fitted to the embedded JPEG, else the
    /// maker's base curve). Set by raw decodes.
    pub look_curve: Option<tone::CameraTone>,
    /// A DNG profile's tone curve (`ProfileToneCurve`), available as a Camera fallback.
    /// Soft Film always keeps its own curve. Set by raw decodes.
    pub profile_curve: Option<tone::CameraTone>,
}

impl SourceInfo {
    /// Prefer the decoded embedded model; otherwise consume the metadata-only Sony handoff.
    pub fn embedded_lens(&self, w: f64, h: f64) -> Option<lightcraft_develop::EmbeddedLens> {
        self.lens.or_else(|| self.sony_lens.as_ref().and_then(|data| optics::sony_embedded_lens(data, w, h)))
    }
}

impl Default for SourceInfo {
    fn default() -> Self {
        Self {
            raw: false,
            clip_confidence: None,
            raw_clip_level: None,
            camera_noise: None,
            camera_wb_presets: vec![],
            as_shot_temp: 6500.0,
            as_shot_tint: 0.0,
            lens: None,
            lens_camera_to_working: None,
            source_hue_sat: None,
            embedded_lens_baked: false,
            embedded_ca_baked: false,
            sony_lens: None,
            source_orientation: lightcraft_geom::Orientation::Normal,
            relative_wb: false,
            camera_color: None,
            camera_profile: None,
            baseline_gain: 1.0,
            camera_tone: None,
            lens_db: None,
            lcp: None,
            sensor_scale: 1.0,
            capture_radius: None,
            capture_threshold: 0.4,
            look_curve: None,
            profile_curve: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Quality {
    /// Interactive drags: skips the most expensive refinements.
    Draft,
    #[default]
    Full,
}

#[derive(Clone, Debug)]
pub struct RenderRequest {
    /// Output must fit in `max_w × max_h` (aspect preserved).
    pub max_w: usize,
    pub max_h: usize,
    pub quality: Quality,
    /// Show the crop frame's content only (true) or the whole uncropped image (crop tool active).
    pub apply_crop: bool,
    /// A diagnostic view drawn over the result (e.g. Point Color's "visualize range").
    pub overlay: Overlay,
    /// Colour space of the result (exports; previews stay sRGB).
    pub space: OutputSpace,
    /// Sample format: 8-bit only, or also a 16-bit / linear float [`Rendered::deep`] (exports).
    pub depth: OutputDepth,
    /// Soft proofing (CPU only; see [`Proof`]).
    pub proof: Option<Proof>,
}

impl RenderRequest {
    pub fn fit(max_w: usize, max_h: usize) -> Self {
        Self {
            max_w,
            max_h,
            quality: Quality::Full,
            apply_crop: true,
            overlay: Overlay::None,
            space: OutputSpace::Srgb,
            depth: OutputDepth::U8,
            proof: None,
        }
    }
}

pub struct Rendered {
    /// The result, 8-bit (for deep renders: `deep` reduced to 8 bits, display-encoded).
    pub image: Rgba8,
    pub histogram: Histogram,
    /// The high-bit-depth result when [`RenderRequest::depth`] asks for one.
    pub deep: Option<DeepImage>,
}

/// Everything the per-pixel stage needs, precomputed at output resolution.
///
/// The image and planes are computed *before exposure* (so they can be reused while exposure is
/// dragged): the per-pixel stage multiplies the image by `gain` and shifts log planes by `ev`.
/// Spatial filters on log luminance are shift-equivariant, so this equals filtering after exposure.
#[derive(Clone)]
pub(crate) struct Prepared {
    pub img: Arc<RgbFrame>,
    /// Log2 luminance relative to middle grey (before exposure: add `ev`).
    pub log_l: Arc<GrayFrame>,
    pub base: Arc<GrayFrame>,
    pub clarity_blur: Option<Arc<GrayFrame>>,
    pub texture_blur: Option<Arc<GrayFrame>>,
    /// Blurred chromaticity (`rgb / Y`) for local Moiré / Noise.
    pub chroma_blur: Option<Arc<RgbFrame>>,
    pub masks: Vec<masks::Evaluated>,
    /// Develop layers' noise reduction: (evaluated mask index, the image denoised again with the
    /// layer's settings), see [`layers`].
    pub layer_nr: Vec<(usize, Arc<RgbFrame>)>,
    /// Output pixels per unit of the source long edge.
    pub px_per_long: f64,
    /// The tone equalizer's mask (see [`toneeq`]) when the tool is on.
    pub tone_eq: Option<Arc<GrayFrame>>,
    /// The image and log luminance range and brush masks select on when they are not `img`:
    /// the primary stage's input, on which that stage evaluated the same masks.
    pub selection: Option<(Arc<RgbFrame>, Arc<GrayFrame>)>,
    /// sharpening: `log_l` blurred once and twice, and the blur radius (output px).
    pub sharp: Option<(Arc<GrayFrame>, Arc<GrayFrame>, f32)>,
    pub layer_sharp: Vec<(usize, Arc<GrayFrame>, Arc<GrayFrame>, f32)>,
    /// dehaze: the refined dark channel and the airlight (before exposure).
    pub haze: Option<Arc<detail::Haze>>,
}

/// Output size for a source of `src_w × src_h` under `s`, fitting `max_w × max_h`.
pub fn output_size(src_w: usize, src_h: usize, s: &DevelopSettings, req: &RenderRequest) -> (usize, usize) {
    geometry::Frame::new(src_w, src_h, s, req.apply_crop).fit(req.max_w, req.max_h)
}

/// Size of the cropped output of a `src_w × src_h` source under `s` at the source's own resolution
/// (what a full-size export produces).
pub fn native_output_size(src_w: usize, src_h: usize, s: &DevelopSettings) -> (f64, f64) {
    geometry::Frame::new(src_w, src_h, s, true).native_size()
}

/// The geometric frame `render` uses for `src` (lens data, automatic CA estimate included).
pub fn frame_for(src: &RgbFrame, info: &SourceInfo, s: &DevelopSettings, apply_crop: bool) -> geometry::Frame {
    let embedded = info.embedded_lens(src.width as f64, src.height as f64);
    // Built-in ordered DNG corrections have already run before colour conversion.
    // Suppress all automatic fallbacks, while preserving manual Optics controls.
    let settings = if info.embedded_lens_baked && s.optics.lens_profile {
        let mut disabled = s.clone();
        disabled.optics.lens_profile = false;
        Cow::Owned(disabled)
    } else {
        Cow::Borrowed(s)
    };
    let mut frame =
        geometry::Frame::with_lcp(src.width, src.height, &settings, apply_crop, embedded.as_ref(), info.lens_db.as_ref(), info.lcp.as_ref());
    let profile_ca = frame.warp.as_ref().is_some_and(|w| w.profile_ca());
    if let Some(warp) = frame.warp.as_mut().filter(|_| profile_ca)
        && let Some(forward) = info.lens_camera_to_working
        && let Some(undo) = forward.inverse()
    {
        warp.camera_basis = Some([undo, forward]);
    }
    // The estimator sees the uncorrected source. Applying it on top of a
    // calibrated CA profile would correct the same displacement twice.
    if s.optics.remove_ca && s.section_enabled("optics") && !profile_ca && !info.embedded_ca_baked {
        frame.add_lateral_ca(optics::estimate_lateral_ca(src));
    }
    frame
}

/// Intermediate results of recent renders of one view, reused by [`render_cached`].
///
/// Each stage is keyed by exactly the inputs it depends on: the resampled source by the source
/// buffer, framing and output size; the white-balanced, retouched, denoised image by those plus
/// white balance, spots and noise reduction; each spatial plane by that plus its own radius. So a
/// tone or colour slider drag (or exposure) re-runs only the per-pixel stage, a clarity drag skips
/// resampling and noise reduction, and so on. Holds the last few output sizes (a draft-size and a
/// full-size render of the same view both stay warm).
pub struct StageCache {
    entries: Mutex<Vec<CacheEntry>>,
    capacity: usize,
    stack_renders: Mutex<(usize, usize)>,
    /// Another renderer's per-view state (the GPU renderer keeps its device-resident stages here).
    ext: Mutex<Option<Arc<dyn Any + Send + Sync>>>,
    /// The last source, parameters key, result, and retained clipping metadata.
    pre: Mutex<Option<CapturedSource>>,
}

type CapturedSource = (Arc<RgbFrame>, u64, Arc<RgbFrame>, Option<Arc<ClipConfidence>>);

impl Default for StageCache {
    fn default() -> Self {
        StageCache { entries: Mutex::new(Vec::new()), capacity: 2, stack_renders: Mutex::new((0, 0)), ext: Mutex::new(None), pre: Mutex::new(None) }
    }
}

impl StageCache {
    /// Background finishes and layer evaluations performed by the latest cached CPU render.
    pub fn stack_renders(&self) -> (usize, usize) {
        *self.stack_renders.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Drop everything (e.g. when memory is needed).
    pub fn clear(&self) {
        self.lock().clear();
        *self.ext.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.pre.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Per-view state of type `T` kept alongside this cache (created on first use). Lets another
    /// renderer of the same view (e.g. `lightcraft-gpu`) keep its own stage cache with this one.
    pub fn extension<T: Any + Send + Sync + Default>(&self) -> Arc<T> {
        let mut g = self.ext.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = g.as_ref()
            && let Ok(t) = e.clone().downcast::<T>()
        {
            return t;
        }
        let t = Arc::new(T::default());
        *g = Some(t.clone());
        t
    }

    /// The per-view state of type `T` if there is one (unlike [`Self::extension`], never creates it).
    pub fn peek_extension<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        self.ext.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|e| e.clone().downcast::<T>().ok())
    }

    /// Bytes of the intermediate images held (each buffer counted once; the sources they were
    /// made from belong to their owner and are not counted).
    pub fn bytes(&self) -> usize {
        fn size<T>(i: &li_raster::Frame<T>) -> usize {
            i.data.len() * std::mem::size_of::<T>()
        }
        let mut seen: Vec<usize> = Vec::new();
        let mut total = 0;
        let mut add = |p: usize, n: usize| {
            if !seen.contains(&p) {
                seen.push(p);
                total += n;
            }
        };
        for e in self.lock().iter() {
            add(Arc::as_ptr(&e.sampled) as usize, size(&e.sampled));
            if let Some((_, l)) = &e.lin {
                add(Arc::as_ptr(l) as usize, size(l));
            }
            if let Some((_, p)) = &e.primary {
                add(Arc::as_ptr(p) as usize, size(p));
            }
            for image in e.stack.outputs.iter().filter_map(|p| p.1.as_ref()) {
                add(Arc::as_ptr(image) as usize, size(image));
            }
            let pl = &e.planes;
            let planes = pl.log_l.iter().chain(pl.base.iter().map(|x| &x.1)).chain(pl.clarity.iter().map(|x| &x.1));
            for p in planes.chain(pl.texture.iter().map(|x| &x.1)) {
                add(Arc::as_ptr(p) as usize, size(p));
            }
            for p in pl.sharp.iter().flat_map(|x| [&x.1, &x.2]).chain(pl.layer_sharp.iter().flat_map(|x| [&x.1, &x.2])) {
                add(Arc::as_ptr(p) as usize, size(p));
            }
            if let Some(h) = &pl.haze {
                add(Arc::as_ptr(h) as usize, (h.positive.data.len() + h.negative.data.len()) * 4);
            }
            for (_, l) in &pl.layer_nr {
                add(Arc::as_ptr(l) as usize, size(l));
            }
        }
        if let Some((_, _, out, _)) = self.pre.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            total += size(out);
        }
        total
    }

    /// Number of cached output sizes.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<CacheEntry>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn get(&self, src: &Arc<RgbFrame>, geo: u64) -> Option<CacheEntry> {
        self.lock().iter().find(|e| e.geo == geo && Arc::ptr_eq(&e.src, src)).cloned()
    }

    fn put(&self, e: CacheEntry) {
        let mut v = self.lock();
        v.retain(|o| !(o.geo == e.geo && Arc::ptr_eq(&o.src, &e.src)));
        v.push(e);
        while v.len() > self.capacity {
            v.remove(0);
        }
    }
}

#[derive(Clone)]
struct CacheEntry {
    src: Arc<RgbFrame>,
    geo: u64,
    sampled: Arc<RgbFrame>,
    lin: Option<(u64, Arc<RgbFrame>)>,
    planes: local::Planes,
    primary: primary::Cached,
    stack: layers::Cache,
}

fn hash_of(parts: impl std::hash::Hash) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    BuildHasherDefault::<DefaultHasher>::default().hash_one(parts)
}

/// What a render resolves to before any pixel work: the effective settings (profile and Upright
/// applied), the geometric frame, the output size and the stage-cache keys. Shared by the CPU
/// renderer and the GPU renderer (`lightcraft-gpu`), so both key their caches identically.
#[derive(Clone)]
pub struct Plan<'a> {
    pub settings: Cow<'a, DevelopSettings>,
    pub frame: geometry::Frame,
    pub w: usize,
    pub h: usize,
    /// Output pixels per unit of the oriented source's long edge.
    pub px_per_long: f64,
    /// Long edge of the source buffer (px).
    pub src_long: usize,
    /// Key of the resampled source (together with the source buffer's identity).
    pub geo: u64,
    /// Key of the white-balanced, retouched, denoised image (and of the planes computed from it).
    pub lin_key: u64,
    /// Red eye / pet eye corrections with their detected pupils, in output pixels.
    pub eyes: Vec<redeye::EyeK>,
}

/// Resolve `s` against `src` for `req` (see [`Plan`]).
pub fn plan<'a>(src: &RgbFrame, info: &SourceInfo, s: &'a DevelopSettings, req: &RenderRequest) -> Plan<'a> {
    let mut settings: Cow<'a, DevelopSettings> = match profiles::effective(s) {
        Cow::Borrowed(b) => upright::resolve(src, info, b),
        Cow::Owned(o) => Cow::Owned(upright::resolve(src, info, &o).into_owned()),
    };
    visualize::adjust_settings(req.overlay, &mut settings);
    let s = &*settings;
    let frame = frame_for(src, info, s, req.apply_crop);
    let (w, h) = frame.fit(req.max_w, req.max_h);
    let px_per_long = frame.px_per_long(w);
    let src_long = src.width.max(src.height);
    let geo = hash_of((format!("{frame:?}"), w, h));
    let (wb_t, wb_tint) = local::effective_wb(info, s);
    let eyes = redeye::resolve(src, &s.red_eye, s.orientation, &frame, w, h, px_per_long);
    let d = &s.detail;
    let lin_key = hash_of((
        geo,
        [wb_t, wb_tint, info.as_shot_temp, info.as_shot_tint].map(f64::to_bits),
        format!("{:?}", s.spots),
        format!("{eyes:?}"),
        // defringe runs in this stage
        format!("{:?}", s.optics),
        // the negative conversion runs in this stage (only while it is on: its sliders change
        // nothing while it is off)
        format!("{:?}", negative::params(s)),
        [d.nr_luminance, d.nr_detail, d.nr_color, d.nr_color_detail, d.nr_color_smoothness, d.nr_contrast].map(f64::to_bits),
        (src_long, s.section_enabled("detail")),
        // colour calibration runs with the white balance
        format!("{:?}", colorcal::of(info, s)),
        format!("{:?}", info.camera_color),
        (
            info.camera_profile.as_ref().map(|p| p.hash64()),
            info.source_hue_sat.as_ref().map(|p| Arc::as_ptr(p) as usize),
            info.camera_noise.map(|n| (n.a.map(f32::to_bits), n.b.map(f32::to_bits), n.camera_to_working.map(|r| r.map(f32::to_bits)))),
        ),
        (info.baseline_gain.to_bits(), info.sensor_scale.to_bits()),
    ));
    Plan { settings, frame, w, h, px_per_long, src_long, geo, lin_key, eyes }
}

/// Whether scene-linear pixel tools still require the host inside a GPU render.
/// Nonlinear calibration and stored AI patches have native kernels.
pub fn lin_needs_cpu(s: &DevelopSettings) -> bool {
    let o = &s.optics;
    let defringe = s.section_enabled("optics") && (o.defringe_purple_amount > 0.0 || o.defringe_green_amount > 0.0);
    defringe || s.spots.iter().any(|spot| !spot.is_ai()) || negative::converts(s)
}

/// Tone Equalizer and its overlay have native device preparation and finish kernels.
pub fn tools_need_cpu(_s: &DevelopSettings, _req: &RenderRequest) -> bool {
    false
}

/// Develop layer stacks have native device-resident evaluation (device limits still permit fallback).
pub fn layers_need_cpu(_s: &DevelopSettings) -> bool {
    false
}

/// The white-balanced, negative-converted, defringed, retouched image (before noise reduction):
/// the CPU part of the scene-linear stage, in place.
///
/// The negative conversion comes right after white balance: like darktable's negadoctor it reads
/// the scan's scene-linear transmittance (white-balanced, in the working space), and everything
/// after it (defringe's purple/green hues, retouching, noise reduction, tone and colour) has to see
/// the positive image.
pub fn lin_cpu(img: &mut RgbFrame, info: &SourceInfo, p: &Plan<'_>) {
    let s = &*p.settings;
    if let Some(table) = info.source_hue_sat.as_ref()
        && let Some(look) = source_color::HueSat::new(table)
    {
        img.map_in_place(|rgb| look.apply(rgb));
    }
    // AI removals first: their patches are in the source's own light (before white balance)
    patches::apply_spots(img, &s.spots, &p.frame);
    local::white_balance(img, info, s);
    negative::apply(img, s);
    optics::defringe(img, s, p.px_per_long / optics::DEFRINGE_REF_LONG);
    spots::apply(img, &s.spots, &p.frame, p.px_per_long);
    redeye::apply(img, &p.eyes);
}

/// Capture sharpening of a source with `info` under `s` (see [`capture`]): `None` when it is off,
/// the source isn't raw, or the blur at this source's scale is too small to matter.
pub fn capture_params(info: &SourceInfo, s: &DevelopSettings) -> Option<capture::CaptureParams> {
    let c = &s.raw.capture;
    if !c.enabled || !info.raw || !s.section_enabled("raw") {
        return None;
    }
    let radius = if c.radius > 0.0 { c.radius as f32 } else { info.capture_radius.unwrap_or(CAPTURE_DEFAULT_RADIUS) };
    let sigma = radius / info.sensor_scale.max(1e-3);
    if sigma <= 0.25 {
        return None;
    }
    Some(capture::CaptureParams {
        sigma,
        threshold: if c.threshold > 0.0 { (c.threshold / 100.0) as f32 } else { info.capture_threshold },
        corner_boost: (c.corner_boost / 100.0) as f32,
        center: 0.0,
        iterations: capture::iterations_of(c.iterations.clamp(1.0, 25.0) as u32),
        clip: None,
    })
}

/// The radius darktable falls back to when the raw data can't tell.
const CAPTURE_DEFAULT_RADIUS: f32 = 0.5;

/// The capture-sharpened source (`None`: nothing to sharpen; render `src`), reused from `cache`
/// while the source and the parameters are unchanged. Both renderers call it first.
pub fn presource(src: &Arc<RgbFrame>, info: &SourceInfo, s: &DevelopSettings, cache: Option<&StageCache>) -> Option<Arc<RgbFrame>> {
    capture_params(info, s).map(|p| presource_with(src, info, &p, cache))
}

fn presource_with(src: &Arc<RgbFrame>, info: &SourceInfo, p: &capture::CaptureParams, cache: Option<&StageCache>) -> Arc<RgbFrame> {
    let key = hash_of((format!("{p:?}"), info.clip_confidence.as_ref().map(|c| Arc::as_ptr(c) as usize)));
    if let Some(c) = cache
        && let Some((s0, k, out, _)) = c.pre.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && Arc::ptr_eq(s0, src)
        && *k == key
    {
        return out.clone();
    }
    let mut img = (**src).clone();
    timed("capture sharpening", || capture::sharpen_with_confidence(&mut img, p, info.clip_confidence.as_deref()));
    let out = Arc::new(img);
    if let Some(c) = cache {
        *c.pre.lock().unwrap_or_else(|e| e.into_inner()) = Some((src.clone(), key, out.clone(), info.clip_confidence.clone()));
    }
    out
}

/// Render `src` with settings `s`. Legacy infallible adapter: an unsupported proof
/// yields an empty render. Custom ICC callers should use [`render_checked`] for the error.
pub fn render(src: &RgbFrame, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest) -> Rendered {
    render_checked(src, info, s, req).unwrap_or_else(|_| empty_render())
}

/// Render with explicit CMS linking errors (printer and monitor profiles are never substituted).
pub fn render_checked(src: &RgbFrame, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest) -> Result<Rendered, photocraft_cms::CmsError> {
    if let Some(proof) = &req.proof {
        proof.try_params(req.space)?;
    }
    Ok(render_impl(Src::Borrowed(src), info, s, req, None, primary::DEFAULT_HS_METHOD))
}

/// [`render`], reusing the intermediate results in `cache`.
pub fn render_cached(src: &Arc<RgbFrame>, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest, cache: &StageCache) -> Rendered {
    render_cached_checked(src, info, s, req, cache).unwrap_or_else(|_| empty_render())
}

/// [`render_checked`] with the app's intermediate stage cache.
pub fn render_cached_checked(
    src: &Arc<RgbFrame>,
    info: &SourceInfo,
    s: &DevelopSettings,
    req: &RenderRequest,
    cache: &StageCache,
) -> Result<Rendered, photocraft_cms::CmsError> {
    if let Some(proof) = &req.proof {
        proof.try_params(req.space)?;
    }
    Ok(render_impl(Src::Shared(src), info, s, req, Some(cache), primary::DEFAULT_HS_METHOD))
}

fn empty_render() -> Rendered {
    let image = Rgba8::new(0, 0);
    let histogram = Histogram::of_srgb8(&image);
    Rendered { image, histogram, deep: None }
}

/// Internal evaluation harness entry point; never persisted in photo settings.
pub fn render_hs_candidate(src: &RgbFrame, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest, method: primary::HsMethod) -> Rendered {
    render_impl(Src::Borrowed(src), info, s, req, None, method)
}

enum Src<'a> {
    Borrowed(&'a RgbFrame),
    Shared(&'a Arc<RgbFrame>),
}

fn render_impl(
    src: Src<'_>,
    info: &SourceInfo,
    s: &DevelopSettings,
    req: &RenderRequest,
    cache: Option<&StageCache>,
    hs_method: primary::HsMethod,
) -> Rendered {
    // `Instant::now()` panics on wasm32-unknown-unknown: only read the clock when profiling.
    let lap = |what: &str, t: &mut Option<std::time::Instant>| {
        if let Some(t) = t {
            log::debug!("  {what}: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
            *t = std::time::Instant::now();
        }
    };
    let mut t = profiling().then(std::time::Instant::now);
    // capture sharpening works on the source itself (sensor-scale blur), before everything else
    let pre: Option<Arc<RgbFrame>> = capture_params(info, s).map(|p| match &src {
        Src::Shared(a) => presource_with(a, info, &p, cache),
        Src::Borrowed(r) => {
            let mut img = (*r).clone();
            capture::sharpen_with_confidence(&mut img, &p, info.clip_confidence.as_deref());
            Arc::new(img)
        }
    });
    let src = match &pre {
        Some(a) => Src::Shared(a),
        None => src,
    };
    let src_img: &RgbFrame = match &src {
        Src::Borrowed(r) => r,
        Src::Shared(a) => a,
    };
    let mut plan = plan(src_img, info, s, req);
    let stack_settings = layers::active(&plan.settings).then(|| plan.settings.clone().into_owned());
    if stack_settings.is_some() {
        plan.settings = Cow::Owned(layers::background(&plan.settings));
    }
    let stack_base_key = layers::background_key(&plan, info, req.quality, hs_method);
    let Plan { w, h, px_per_long, src_long, geo, lin_key, .. } = plan;
    let frame = plan.frame.clone();
    let s = &*plan.settings;

    let shared = match (&src, cache) {
        (Src::Shared(a), Some(c)) => Some((*a, c)),
        _ => None,
    };
    let cached = shared.and_then(|(a, c)| c.get(a, geo));
    let sampled = match &cached {
        Some(e) => e.sampled.clone(),
        None => Arc::new(frame.sample(src_img, w, h)),
    };
    lap("sample", &mut t);

    let lin = match cached.as_ref().and_then(|e| e.lin.clone()).filter(|(k, _)| *k == lin_key) {
        Some((_, img)) => img,
        None => {
            // Without a cache the resampled buffer is ours: work on it in place.
            let mut img = if shared.is_some() { (*sampled).clone() } else { Arc::unwrap_or_clone(sampled.clone()) };
            lin_cpu(&mut img, info, &plan);
            local::denoise_source(&mut img, s, src_long, px_per_long, info);
            Arc::new(img)
        }
    };
    lap("wb/spots/nr", &mut t);
    let primary_key = primary::key_for(&plan, info, hs_method);
    let primary = if primary::active(&plan.settings) {
        match cached.as_ref().and_then(|e| e.primary.clone()).filter(|(key, _)| *key == primary_key) {
            Some(p) => Some(p),
            None => {
                let proxy = if primary::needs_proxy(&plan.settings) { primary::proxy_for(src_img, info, &plan) } else { RgbFrame::new(0, 0) };
                Some((primary_key, Arc::new(primary::process(&lin, &proxy, info, &plan, hs_method))))
            }
        }
    } else {
        None
    };
    let prepared_lin = primary.as_ref().map_or_else(|| lin.clone(), |p| p.1.clone());
    if primary.is_some() {
        plan.settings = Cow::Owned(visualize::remaining_for_overlay(req.overlay, &plan.settings));
    }
    let s = &*plan.settings;
    let planes_key = if primary.is_some() { primary_key } else { lin_key };
    let mut stack_cache = cached.as_ref().map(|e| e.stack.clone()).unwrap_or_default();
    let mut planes = match cached.map(|e| e.planes) {
        Some(p) if p.key == planes_key => p,
        _ => local::Planes { key: planes_key, ..Default::default() },
    };
    let selection = primary.is_some().then(|| lin.clone());
    let prep = local::prepare(prepared_lin, selection, s, info, &frame, px_per_long, src_long, req.quality, &mut planes);
    lap("prepare", &mut t);
    let result = if let Some(original) = &stack_settings {
        let layers::Stacked { image, background_renders: background, layer_renders: layers, masks, diagnostic } =
            layers::stack(&prep, &plan, original, info, req, hs_method, stack_base_key, &mut stack_cache);
        if let Some(c) = cache {
            *c.stack_renders.lock().unwrap_or_else(|e| e.into_inner()) = (background, layers);
        }
        let mut p = prep.clone();
        p.img = image;
        p.masks = masks;
        p.masks.extend(prep.masks.clone());
        plan.settings = Cow::Borrowed(original);
        finish_stack(&p, &plan, info, req, diagnostic.as_ref())
    } else {
        if let Some(c) = cache {
            *c.stack_renders.lock().unwrap_or_else(|e| e.into_inner()) = (0, 0);
        }
        stack_cache.outputs.clear();
        finish_prepared(&prep, &plan, info, req)
    };
    if let Some((a, c)) = shared {
        c.put(CacheEntry { src: a.clone(), geo, sampled, lin: Some((lin_key, lin)), planes, primary, stack: stack_cache });
    }
    result
}

/// A CPU finish stage inside the GPU renderer, for the faithful EIGF tone equalizer.
/// Geometry, white balance, and noise reduction have already run on the device.
pub fn finish_scene_stage(mut img: Arc<RgbFrame>, plan: &Plan<'_>, info: &SourceInfo, req: &RenderRequest) -> Rendered {
    if layers::active(&plan.settings) {
        let original = &*plan.settings;
        let mut background = Plan { settings: Cow::Owned(layers::background(original)), ..plan.clone() };
        let key = layers::background_key(&background, info, req.quality, primary::DEFAULT_HS_METHOD);
        let mut selection = None;
        if primary::active(&background.settings) {
            let proxy = if primary::needs_proxy(&background.settings) { (*img).clone() } else { RgbFrame::new(0, 0) };
            let developed = Arc::new(primary::process(&img, &proxy, info, &background, primary::DEFAULT_HS_METHOD));
            selection = Some(std::mem::replace(&mut img, developed));
            background.settings = Cow::Owned(visualize::remaining_for_overlay(req.overlay, &background.settings));
        }
        let mut prep = local::prepare(
            img,
            selection,
            &background.settings,
            info,
            &background.frame,
            background.px_per_long,
            background.src_long,
            req.quality,
            &mut local::Planes::default(),
        );
        let stacked = layers::stack(&prep, &background, original, info, req, primary::DEFAULT_HS_METHOD, key, &mut layers::Cache::default());
        prep.img = stacked.image;
        let background_masks = std::mem::take(&mut prep.masks);
        prep.masks = stacked.masks;
        prep.masks.extend(background_masks);
        return finish_stack(&prep, plan, info, req, stacked.diagnostic.as_ref());
    }
    let prep =
        local::prepare(img, None, &plan.settings, info, &plan.frame, plan.px_per_long, plan.src_long, req.quality, &mut local::Planes::default());
    finish_prepared(&prep, plan, info, req)
}

fn finish_stack(prep: &Prepared, plan: &Plan<'_>, info: &SourceInfo, req: &RenderRequest, diagnostic: Option<&GrayFrame>) -> Rendered {
    let neutral = DevelopSettings::default();
    let deep = (req.depth != OutputDepth::U8)
        .then(|| finish::finish_deep_mode(prep, &neutral, &plan.frame, info, req.space, req.depth, req.proof.clone(), finish::FinishMode::Output));
    let mut image = deep.as_ref().map_or_else(
        || finish::finish_mode(prep, &neutral, &plan.frame, info, req.space, req.proof.clone(), finish::FinishMode::Output),
        DeepImage::to_rgba8,
    );
    let histogram = Histogram::of_srgb8(&image);
    if req.depth == OutputDepth::U8 {
        let mask = if diagnostic.is_some() {
            None
        } else if let Some(m) = req.overlay.mask(&plan.settings)
            && masks::scalar_alpha(m).is_none()
            && !prep.masks.iter().any(|mask| mask.id == m.id)
        {
            let log = prep.img.map(local::log_lum);
            Some(masks::evaluate_one_tone(m, &plan.frame, plan.w, plan.h, &prep.img, &log, 0.0, Some(&ToneMap::working_identity())))
        } else {
            overlay_alpha(req.overlay, plan, prep)
        };
        visualize::apply(&mut image, req.overlay, plan, diagnostic.or(mask.as_ref()));
    }
    Rendered { image, histogram, deep }
}

fn finish_prepared(prep: &Prepared, plan: &Plan<'_>, info: &SourceInfo, req: &RenderRequest) -> Rendered {
    let s = &*plan.settings;
    let frame = &plan.frame;
    if req.depth != OutputDepth::U8 {
        let deep = finish::finish_deep(prep, s, frame, info, req.space, req.depth, req.proof.clone());
        let image = deep.to_rgba8();
        let histogram = Histogram::of_srgb8(&image);
        return Rendered { image, histogram, deep: Some(deep) };
    }
    let image = finish::finish(prep, s, frame, info, req.space, req.proof.clone());
    let histogram = Histogram::of_srgb8(&image);
    let mut image = image;
    let mask = overlay_alpha(req.overlay, plan, prep);
    visualize::apply(&mut image, req.overlay, plan, mask.as_ref());
    Rendered { image, histogram, deep: None }
}

/// The colour a colour-range mask would sample at normalized image point `p` (OkLab of the
/// exposed scene colours, as [`MaskShape::ColorRange`](lightcraft_develop::MaskShape) compares
/// them), averaged over 3 × 3 pixels of a render fitting `req`. `None` outside the image.
pub fn color_range_sample(
    src: &RgbFrame,
    info: &SourceInfo,
    s: &DevelopSettings,
    req: &RenderRequest,
    p: lightcraft_geom::Point,
) -> Option<[f64; 3]> {
    let plan = plan(src, info, s, req);
    let mut img = plan.frame.sample(src, plan.w, plan.h);
    lin_cpu(&mut img, info, &plan);
    let o = plan.frame.norm_to_out(plan.w, plan.h).apply(p);
    let (cx, cy) = (o.x.floor() as i64, o.y.floor() as i64);
    if cx < 0 || cy < 0 || cx >= plan.w as i64 || cy >= plan.h as i64 {
        return None;
    }
    let gain = (s.light.exposure as f32).exp2();
    let tone = masks::selection_tone(s, info);
    let mut acc = [0f64; 3];
    let mut n = 0.0;
    for y in (cy - 1).max(0)..=(cy + 1).min(plan.h as i64 - 1) {
        for x in (cx - 1).max(0)..=(cx + 1).min(plan.w as i64 - 1) {
            let c = img.data[y as usize * plan.w + x as usize].map(|v| v * gain);
            let lab = li_color::perceptual::oklab_from_2020(tone2::tone_px(&tone, &tone.method(), c));
            for k in 0..3 {
                acc[k] += lab[k] as f64;
            }
            n += 1.0;
        }
    }
    Some(acc.map(|v| v / n))
}

/// Scene-linear UCS22 reference sampled before Skin Tone, averaged over 3×3 pixels.
pub fn skin_reference_sample(
    src: &RgbFrame,
    info: &SourceInfo,
    s: &DevelopSettings,
    req: &RenderRequest,
    p: lightcraft_geom::Point,
) -> Option<[f64; 3]> {
    let mut settings = s.clone();
    settings.skin_tone = Default::default();
    for m in &mut settings.masks {
        m.tools.skin_tone = None;
    }
    let plan = plan(src, info, &settings, req);
    let mut img = plan.frame.sample(src, plan.w, plan.h);
    lin_cpu(&mut img, info, &plan);
    if primary::active(&plan.settings) {
        let proxy = if primary::needs_proxy(&plan.settings) { primary::proxy_for(src, info, &plan) } else { RgbFrame::new(0, 0) };
        img = primary::process(&img, &proxy, info, &plan, primary::DEFAULT_HS_METHOD);
    }
    let q = plan.frame.norm_to_out(plan.w, plan.h).apply(p);
    if q.x < 0. || q.y < 0. || q.x >= plan.w as f64 || q.y >= plan.h as f64 {
        return None;
    }
    let (cx, cy) = (q.x.floor() as usize, q.y.floor() as usize);
    let mut rgb = [0.0f32; 3];
    let mut n = 0.;
    for y in cy.saturating_sub(1)..=(cy + 1).min(plan.h - 1) {
        for x in cx.saturating_sub(1)..=(cx + 1).min(plan.w - 1) {
            let c = img.get(x, y);
            for k in 0..3 {
                rgb[k] += c[k];
            }
            n += 1.;
        }
    }
    let g = (settings.light.exposure as f32).exp2();
    let c = ucs::xyy_to_jch(ucs::xyz_to_xyy(ucs::mul(&ucs::mats().rgb_to_xyz, rgb.map(|v| v * g / n))), ucs::y_to_l_star(1.));
    Some([c[0] as f64, c[1] as f64, c[2].to_degrees().rem_euclid(360.) as f64])
}

/// The alpha plane a mask overlay shows: the one the render evaluated, or (for a hidden mask) a
/// fresh evaluation. Full coverage is drawn scalarly and needs no diagnostic plane either.
fn overlay_alpha(o: Overlay, plan: &Plan<'_>, prep: &Prepared) -> Option<GrayFrame> {
    if let Some(skin) = o.skin(&plan.settings).filter(|s| s.reference.is_some()) {
        return Some(visualize::skin_alpha(&prep.img, skin, (plan.settings.light.exposure as f32).exp2()));
    }
    if o == Overlay::SharpenMask {
        // At very small preview scales the Gaussian is effectively identity, but
        // Alt Masking still shows the edge mask rather than the photograph.
        let (b2, sigma) = prep.sharp.as_ref().map_or((&prep.log_l, 0.0), |(_, b2, sg)| (b2, *sg));
        let k = detail::SharpK::new(0.0, (plan.settings.detail.sharpen_masking / 100.0) as f32, sigma);
        return Some(detail::sharp_mask_plane(b2, &k));
    }

    if o == Overlay::ToneEqMask {
        let m = prep.tone_eq.as_ref()?;
        return Some(m.map(toneeq::preview_grey));
    }
    let m = o.mask(&plan.settings)?;
    if masks::scalar_alpha(m).is_some() {
        return None;
    }
    if let Some(e) = prep.masks.iter().find(|e| e.id == m.id) {
        return Some(match &e.alpha {
            masks::Alpha::Spatial(p) => p.clone(),
            masks::Alpha::Scalar(_) => return None,
        });
    }
    let ev = plan.settings.light.exposure as f32;
    let (img, log) = prep.selection.as_ref().map_or((&prep.img, &prep.log_l), |(img, log)| (img, log));
    Some(masks::evaluate_one(m, &plan.frame, plan.w, plan.h, img, log, ev))
}

/// Convenience: render a before/after pair side by side is up to the UI; this renders "before"
/// (default look, keeping the crop so framing matches).
pub fn before_settings(s: &DevelopSettings) -> DevelopSettings {
    // (the photo's look: "before" is its unedited rendition)
    let mut b = DevelopSettings { crop: s.crop, orientation: s.orientation, look: s.look, ..DevelopSettings::default() };
    b.wb = lightcraft_develop::WhiteBalance { mode: lightcraft_develop::WbMode::AsShot, ..b.wb };
    b
}

pub fn is_bw(s: &DevelopSettings) -> bool {
    s.treatment == Treatment::Bw || s.profile.id == "lc.mono" || s.profile.id.starts_with("lc.bw.")
}

/// `LIGHTCRAFT_PROFILE` is set: print per-stage timings to stderr.
pub fn profiling() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LIGHTCRAFT_PROFILE").is_some())
}

/// Run `f`, printing its duration under `LIGHTCRAFT_PROFILE`.
pub(crate) fn timed<R>(what: &str, f: impl FnOnce() -> R) -> R {
    if !profiling() {
        return f();
    }
    let t = std::time::Instant::now();
    let r = f();
    log::debug!("    {what}: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    r
}

pub(crate) fn for_rows<T: Send>(data: &mut [T], w: usize, f: impl Fn(usize, &mut [T]) + Sync + Send) {
    par_rows(data, w, f)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_detail;
#[cfg(test)]
mod tests_detail_refvec;
#[cfg(test)]
mod tests_geometry;
#[cfg(test)]
mod tests_layers;
#[cfg(test)]
mod tests_local;
#[cfg(test)]
mod tests_sliders;
#[cfg(test)]
mod tests_toolset;

mod base_curve_data;
pub mod basecurves;

#[cfg(test)]
#[test]
fn develop_c_full_overlay_keeps_scalar_alpha_even_when_hidden() {
    use lightcraft_develop::{Mask, MaskComponent, MaskShape};
    let src = RgbFrame::from_fn(32, 16, |_, _| [0.1; 3]);
    let info = SourceInfo::default();
    let mut s = DevelopSettings {
        masks: vec![Mask {
            id: 1,
            opacity: 50.0,
            components: vec![MaskComponent { name: None, op: Default::default(), invert: false, shape: MaskShape::Full }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let req = RenderRequest {
        overlay: Overlay::Mask { id: 1, view: MaskView::WhiteOnBlack, color: [255, 0, 0], opacity: 100 },
        ..RenderRequest::fit(32, 16)
    };
    let plan = plan(&src, &info, &s, &req);
    let img = Arc::new(plan.frame.sample(&src, plan.w, plan.h));
    let prep = local::prepare(img, None, &s, &info, &plan.frame, plan.px_per_long, plan.src_long, req.quality, &mut local::Planes::default());
    assert!(overlay_alpha(req.overlay, &plan, &prep).is_none(), "Full diagnostics must not expand scalar coverage into a plane");
    assert!(render(&src, &info, &s, &req).image.data.iter().all(|p| *p == [128, 128, 128, 255]));
    s.masks[0].visible = false;
    assert!(render(&src, &info, &s, &req).image.data.iter().all(|p| *p == [128, 128, 128, 255]));
}

#[cfg(test)]
mod sony_dji_source_tests {
    use super::*;
    #[test]
    fn sony_lens_boundary_retains_signed_curves_and_old_source_settings_load() {
        let data = lightcraft_develop::SonyLensData {
            distortion: vec![-100, 0, 120],
            ca_red: vec![-8, 0, 9],
            ca_blue: vec![2, 1, -4],
            vignetting: vec![0, 16, 90],
        };
        let source = SourceInfo { sony_lens: Some(data), ..Default::default() };
        let json = serde_json::to_value(&source).unwrap();
        let back: SourceInfo = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(source.sony_lens, back.sony_lens);
        let mut old = json;
        old.as_object_mut().unwrap().remove("sony_lens");
        let old: SourceInfo = serde_json::from_value(old).unwrap();
        assert!(old.sony_lens.is_none());
    }
}
