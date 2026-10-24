//! Colour-managed canvas display: the document's composite shown through document profile →
//! monitor profile, on the GPU canvas (folded into the display 3D LUT) and the CPU canvas
//! (an 8-bit transform of the composite).
//!
//! * The composite is in [`composite_profile`] (the document profile for RGB, its gray curve as
//!   RGB for gray documents, sRGB for CMYK — read through the document's CMYK profile, see
//!   `photocraft_color::convert::CmykSpace` — and Lab).
//! * Linear composites (EXR/HDR, linear profiles) are stored in the 8-bit canvas texture
//!   sRGB-encoded ([`CanvasDisplay::encode_srgb`]) so shadows keep their precision; the display
//!   source profile is then the same primaries with the sRGB curve.
//! * The monitor profile comes from Edit › Color Settings › Monitor Profile: `auto` (the
//!   compositor-tagged P3 on macOS, sRGB on managed Wayland outputs, or the detected ICC
//!   on X11/Windows, else sRGB), a built-in id or
//!   an `.icc` path.
//! * When the source and the monitor match (sRGB documents on an sRGB display, the common
//!   case) the transform is the identity and nothing is applied on either path.
//!
//! Results are cached per (document profile, mode, monitor profile); the rendering intent is
//! relative colorimetric with black point compensation, as for Photoshop's monitor display.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use li_color::curve::Curve;
use li_color::space::OutputSpace;
use photocraft_cms::{Builtin, ColorSpace, Intent, Profile, Transform};
use photocraft_compose::Buffer;
use photocraft_doc::Document;
use photocraft_raster::Rgba8Image;

use crate::color_cmds::{ColorState, composite_profile, mode_space, profile_from_bytes, resolve_profile};
use crate::{EngineError, Result};

/// Display intent and black point compensation of the monitor transform.
pub const DISPLAY_INTENT: Intent = Intent::RelativeColorimetric;
pub const DISPLAY_BPC: bool = true;

/// Actual window backend, supplied by the native host rather than guessed from environment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputEnvironment {
    #[default]
    AppManaged,
    MacOs,
    Wayland,
}

/// Destination of automatic display rendering. An explicit profile always belongs to the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMode {
    AppManaged,
    CompositorSrgb,
    CompositorP3,
}

pub fn display_mode(environment: OutputEnvironment, managed_output: bool, explicit: bool) -> DisplayMode {
    if explicit {
        return DisplayMode::AppManaged;
    }
    match environment {
        OutputEnvironment::MacOs => DisplayMode::CompositorP3,
        OutputEnvironment::Wayland if managed_output => DisplayMode::CompositorSrgb,
        _ => DisplayMode::AppManaged,
    }
}

/// How the canvas shows one document.
#[derive(Debug)]
pub struct CanvasDisplay {
    /// The canvas texture stores `srgb_encode(value)` instead of the composite value (linear
    /// composites; see the module docs).
    pub encode_srgb: bool,
    /// Profile of the canvas texture values (the composite profile, or its sRGB-curve twin
    /// when `encode_srgb`).
    pub source: Arc<Profile>,
    /// The monitor profile.
    pub monitor: Arc<Profile>,
    /// Texture values → monitor; `None` when that is the identity (within half a 10-bit step).
    pub transform: Option<Arc<Transform>>,
    /// Changes whenever any of the above changes (for UI caches).
    pub key: u64,
    /// Changes whenever what the GPU canvas texture stores changes (`source`, `encode_srgb`),
    /// but not with the monitor: the GPU canvas shares its texture between displays and only
    /// the display LUT is per display.
    pub texture_key: u64,
}

impl CanvasDisplay {
    /// Nothing to do: values go to the screen unchanged.
    pub fn is_identity(&self) -> bool {
        !self.encode_srgb && self.transform.is_none()
    }

    /// CPU canvas: a straight-alpha composite → RGBA8 monitor values.
    pub fn to_rgba8(&self, buf: &Buffer) -> Rgba8Image {
        let mut img = Rgba8Image::new(buf.rect.width(), buf.rect.height());
        use rayon::prelude::*;
        const CHUNK: usize = 4096;
        let width = img.width as usize;
        img.pixels.par_chunks_mut(CHUNK * 4).zip(buf.px.par_chunks(CHUNK)).enumerate().for_each(|(chunk, (dst, src))| {
            let input: Vec<f32> =
                src.iter().flat_map(|p| p[..3].iter().map(|v| if self.encode_srgb { li_color::convert::linear_to_srgb(*v) } else { *v })).collect();
            let mut output = vec![0.0; input.len()];
            let output = if let Some(t) = &self.transform {
                t.convert_f32(&input, 3, &mut output, 3, false);
                &output
            } else {
                &input
            };
            for (j, ((dst, src), color)) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src).zip(output.as_chunks::<3>().0).enumerate() {
                let i = chunk * CHUNK + j;
                for c in 0..3 {
                    dst[c] = quantize_display(color[c], (i % width).wrapping_add(buf.rect.x0 as usize), (i / width).wrapping_add(buf.rect.y0 as usize));
                }
                dst[3] = (src[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        });
        img
    }

    /// Library's 8-bit preview, encoded in `source` (including its existing soft proof) → display.
    /// Float evaluation preserves fractional destination codes until the display-only dither.
    pub fn rgb8_to_display(&self, width: usize, bytes: &[u8]) -> Vec<u8> {
        if self.is_identity() {
            return bytes.to_vec();
        }
        let mut dst = bytes.to_vec();
        if let Some(t) = &self.transform {
            let mut out = [0.0f32; 3];
            for (i, (src, dst)) in bytes.as_chunks::<4>().0.iter().zip(dst.as_chunks_mut::<4>().0).enumerate() {
                t.eval(&[f32::from(src[0]) / 255.0, f32::from(src[1]) / 255.0, f32::from(src[2]) / 255.0], &mut out);
                for c in 0..3 {
                    dst[c] = quantize_display(out[c], i % width, i / width);
                }
            }
        }
        dst
    }

    /// GPU canvas: a CPU composite as the texture stores it (sRGB-encoded for linear
    /// composites; the display LUT does the rest). Values above 1.0 are encoded too (the sRGB
    /// curve extended), for the float canvas texture of 32-bit documents.
    pub fn texture_buffer<'a>(&self, buf: &'a Buffer) -> Cow<'a, Buffer> {
        if !self.encode_srgb {
            return Cow::Borrowed(buf);
        }
        let mut b = buf.clone();
        for p in &mut b.px {
            for v in &mut p[..3] {
                // Capped at the largest half float; NaN is dropped by the texel conversion.
                *v = li_color::convert::linear_to_srgb(v.clamp(0.0, 65504.0));
            }
        }
        Cow::Owned(b)
    }
}

/// Bayer's 8×8 ordered thresholds, centred on zero. The same integer construction is used
/// in the GPU final pass. No frame seed: stationary pixels and tests are deterministic.
pub fn display_noise(x: usize, y: usize) -> f32 {
    let mut rank = 0;
    for bit in 0..3 {
        let xb = (x >> bit) & 1;
        let yb = (y >> bit) & 1;
        rank = rank * 4 + ((xb ^ yb) * 2 + yb);
    }
    (rank as f32 + 0.5) / 64.0 - 0.5
}

/// Display-only quantisation. Exact byte codes remain exact; never used for exports.
pub fn quantize_display(value: f32, x: usize, y: usize) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5 + display_noise(x, y)).floor().clamp(0.0, 255.0) as u8
}

/// Is `p` an RGB matrix/TRC profile with linear curves?
fn is_linear_rgb(p: &Profile) -> bool {
    p.color_space == ColorSpace::Rgb && p.is_matrix_shaper() && p.trc.as_ref().is_some_and(|t| t.iter().all(Curve::is_identity))
}

/// The same primaries with the sRGB curve (what an sRGB-encoded texture of a linear composite is in).
fn srgb_curve_twin(p: &Profile) -> Profile {
    let c = li_color::curve::srgb_trc();
    let mut q = p.clone();
    q.trc = Some([c.clone(), c.clone(), c]);
    q.description = format!("{} (sRGB-encoded)", p.description);
    q.with_encoded_bytes()
}

/// Does `t` map every lattice point (the neutral axis only for gray documents) to itself
/// within half a 10-bit step?
fn is_identity(t: &Transform, neutral_only: bool) -> bool {
    const N: usize = 9;
    let s = (N - 1) as f32;
    let mut out = [0.0f32; 16];
    let mut check = |v: [f32; 3]| {
        t.eval(&v, &mut out);
        (0..3).all(|i| (out[i] - v[i]).abs() <= 0.5 / 1023.0)
    };
    if neutral_only {
        return (0..=32).all(|i| check([i as f32 / 32.0; 3]));
    }
    (0..N).all(|b| (0..N).all(|g| (0..N).all(|r| check([r as f32 / s, g as f32 / s, b as f32 / s]))))
}

fn hash_of(v: impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// Output spaces a Library/Develop view can be rendered in, narrowest gamut first.
const VIEW_SPACES: [OutputSpace; 5] = [OutputSpace::Srgb, OutputSpace::DisplayP3, OutputSpace::AdobeRgb, OutputSpace::Rec2020, OutputSpace::ProPhoto];

/// How far (in linear channel values) a monitor primary may lie outside a view space and still
/// count as inside it: measured profiles of standard panels scatter slightly around the standard
/// primaries, and clipping that sliver is invisible.
const VIEW_GAMUT_SLACK: f64 = 0.02;

/// The narrowest built-in output space whose gamut holds `monitor`'s. Library and Develop render
/// their views in it, gamut-mapped exactly as an export to that space, so colours the monitor can
/// show are never clipped to sRGB on the way. sRGB when the monitor's primaries can't be read.
pub fn view_space(monitor: &Profile) -> OutputSpace {
    let Ok(to_lab) = Transform::new(monitor, Builtin::LabD50.profile(), Intent::RelativeColorimetric, false) else {
        return OutputSpace::Srgb;
    };
    let primaries = [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|rgb| {
        let mut lab = [0.0f32; 3];
        to_lab.eval(&rgb, &mut lab);
        // the Lab built-in's ICC encoding: L*/100, (a* + 128)/255, (b* + 128)/255
        let lab = [f64::from(lab[0]) * 100.0, f64::from(lab[1]) * 255.0 - 128.0, f64::from(lab[2]) * 255.0 - 128.0];
        li_color::cms_math::lab_to_xyz(lab, li_color::cms_math::D50)
    });
    VIEW_SPACES
        .into_iter()
        .find(|space| {
            space.named().to_xyz_d50().inverse().is_some_and(|from_xyz| primaries.iter().all(|p| from_xyz.apply(*p).iter().all(|v| *v >= -VIEW_GAMUT_SLACK)))
        })
        .unwrap_or(OutputSpace::ProPhoto)
}

/// How Library/Develop pixels rendered in `space` reach `monitor` (relative colorimetric with
/// black point compensation; nothing at all when `space` is the monitor's own).
pub fn rendered_display(space: OutputSpace, monitor: Arc<Profile>) -> Result<Arc<CanvasDisplay>> {
    let source = Arc::new(Builtin::from_named(space.named()).profile().clone());
    let transform = if source.content_hash() == monitor.content_hash() {
        None
    } else {
        let t = Transform::new(&source, &monitor, DISPLAY_INTENT, DISPLAY_BPC).map_err(|e| EngineError::Other(format!("colour management: {e}")))?;
        (!is_identity(&t, false)).then(|| Arc::new(t))
    };
    let key = if space == OutputSpace::Srgb { monitor.content_hash() } else { hash_of((space, monitor.content_hash())) };
    Ok(Arc::new(CanvasDisplay { encode_srgb: false, texture_key: source.content_hash(), key, source, monitor, transform }))
}

/// What the platform reported about the displays' profiles (Monitor Profile = `auto`; #569).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum MonitorDetection {
    /// No platform reader (web, Windows until its safe hook is implemented, tests).
    #[default]
    Unsupported,
    /// Being read (the first time; a re-read keeps the previous displays meanwhile).
    Pending,
    /// The reader ran but returned no displays.
    Failed { reason: String },
    /// [`ColorState::displays`] holds what it read.
    Found,
    /// [`ColorState::displays`] holds an earlier reading: the last re-read failed.
    Retained { reason: String },
}

/// A display as the platform reports it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    /// Platform display id (macOS: `CGDirectDisplayID`).
    pub id: u32,
    /// E.g. "Built-in Retina Display".
    pub name: String,
    /// `[x, y, width, height]` in OS points; origin at the top-left of the primary display,
    /// y down (the coordinates window positions use).
    pub frame: [f64; 4],
    /// The OS's name for the display's profile (System Settings), when known.
    pub profile_name: Option<String>,
    /// The display's ICC profile; `None` when the OS gave none.
    #[serde(skip)]
    pub icc: Option<Arc<Vec<u8>>>,
    /// A native Wayland compositor has a managed standard-space output.
    pub compositor_managed: bool,
}

/// The display showing most of a window whose frame (OS points, as [`Display::frame`]) is
/// `rect`: the one it overlaps most, as AppKit's `NSWindow.screen`. `None` when it overlaps none.
pub fn display_at(displays: &[Display], rect: [f64; 4]) -> Option<u32> {
    if !rect.iter().all(|v| v.is_finite()) {
        return None;
    }
    let area = |d: &Display| {
        let w = (rect[0] + rect[2]).min(d.frame[0] + d.frame[2]) - rect[0].max(d.frame[0]);
        let h = (rect[1] + rect[3]).min(d.frame[1] + d.frame[3]) - rect[1].max(d.frame[1]);
        if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
    };
    displays.iter().map(|d| (d.id, area(d))).filter(|(_, a)| *a > 0.0).max_by(|a, b| a.1.total_cmp(&b.1)).map(|(id, _)| id)
}

/// The monitor profile the canvas is actually shown in on one display, and why
/// (`edit.colorSettings`'s `monitorStatus`, Help › System Info).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorStatus {
    /// Color Settings › Monitor Profile as set: `auto`, a built-in id or an `.icc` path.
    pub requested: String,
    /// `auto` (the display's own profile), `manual` (the chosen profile) or `fallback` (sRGB,
    /// because the requested profile isn't available or usable: see `reason`).
    pub source: &'static str,
    /// The display this is for (`auto` reads its profile), when known.
    pub display: Option<String>,
    /// Description of the profile in use (from the profile itself).
    pub profile: String,
    /// The OS's name for it (`auto`), when known.
    pub profile_name: Option<String>,
    /// Its content hash, to tell profiles with the same name apart.
    pub fingerprint: String,
    pub detection: MonitorDetection,
    pub reason: Option<String>,
}

impl MonitorStatus {
    /// One line for Help › System Info and the Color Settings dialog: the profile (by the OS's
    /// name for it in `auto`), the source and any reason. The display is said by the caller.
    pub fn summary(&self) -> String {
        let name = match (&self.profile_name, self.source) {
            (Some(n), "auto") => n.as_str(),
            _ => self.profile.as_str(),
        };
        if self.source == "compositor" {
            let detected = self.profile_name.as_deref().unwrap_or("unknown");
            return format!("{} (compositor; detected panel profile: {detected})", self.profile);
        }
        match &self.reason {
            Some(r) => format!("{name} ({}: {r})", self.source),
            None => format!("{name} ({})", self.source),
        }
    }
}

/// A resolved monitor profile and what it was resolved from (setting, display, profile bytes,
/// detection state).
struct MonitorEntry {
    mode: DisplayMode,
    spec: String,
    display: Option<u32>,
    icc: Option<Arc<Vec<u8>>>,
    detection: MonitorDetection,
    profile: Arc<Profile>,
    status: MonitorStatus,
}

/// Caches of [`ColorState`] for the display (monitor profiles per display, per-document displays).
#[derive(Default)]
pub struct DisplayCaches {
    monitor: Mutex<HashMap<Option<u32>, MonitorEntry>>,
    preview: Mutex<HashMap<u64, Arc<CanvasDisplay>>>,
    canvas: Mutex<HashMap<(ColorSpace, u64, u64), Arc<CanvasDisplay>>>,
}

impl ColorState {
    /// The shared display transform for Library/Develop's sRGB output. Soft proofing has
    /// already run in the Develop pipeline; applying only the monitor stage avoids proofing twice.
    pub fn srgb_preview_display(&self) -> Result<Arc<CanvasDisplay>> {
        let monitor = self.monitor();
        let key = monitor.content_hash();
        let mut cache = self.display.preview.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(d) = cache.get(&key) {
            return Ok(d.clone());
        }
        let d = rendered_display(OutputSpace::Srgb, monitor)?;
        if cache.len() >= 16 {
            cache.clear();
        }
        cache.insert(key, d.clone());
        Ok(d)
    }
    /// The monitor profile of the main window's display ([`ColorState::main_display`]); see
    /// [`ColorState::monitor_for`].
    pub fn monitor(&self) -> Arc<Profile> {
        self.monitor_for(self.main_display)
    }

    /// The monitor profile for `display`: Color Settings › Monitor Profile (`auto`: that
    /// display's profile as the platform reported it, the primary display's when `display` is
    /// unknown, else sRGB). Profiles that are missing, unreadable, not RGB or unusable as a
    /// display destination fall back to sRGB, and [`ColorState::monitor_status_for`] says so.
    pub fn monitor_for(&self, display: Option<u32>) -> Arc<Profile> {
        self.resolved_monitor(display).0
    }

    /// What [`ColorState::monitor`] resolved to, and why.
    pub fn monitor_status(&self) -> MonitorStatus {
        self.monitor_status_for(self.main_display)
    }

    /// What [`ColorState::monitor_for`] resolved to, and why.
    pub fn monitor_status_for(&self, display: Option<u32>) -> MonitorStatus {
        self.resolved_monitor(display).1
    }

    /// Record the platform's reading of the displays (`auto`). An error keeps displays read
    /// before (a failed re-read shouldn't undo a good one): the detection is then
    /// [`MonitorDetection::Retained`], and the reason is also returned for the log.
    pub fn set_displays(&mut self, r: std::result::Result<Vec<Display>, String>) -> Option<String> {
        let reason = match r {
            Ok(d) if !d.is_empty() => {
                self.displays = d;
                self.monitor_detection = MonitorDetection::Found;
                return None;
            }
            Ok(_) => "the platform reported no displays".to_string(),
            Err(reason) => reason,
        };
        if self.displays.is_empty() {
            self.monitor_detection = MonitorDetection::Failed { reason };
            None
        } else {
            self.monitor_detection = MonitorDetection::Retained { reason: reason.clone() };
            Some(reason)
        }
    }

    /// Every display with what its monitor profile resolves to (Help › System Info).
    pub fn display_statuses(&self) -> Vec<serde_json::Value> {
        self.displays
            .iter()
            .map(|d| {
                serde_json::json!({
                    "id": d.id, "name": d.name, "frame": d.frame, "profileName": d.profile_name,
                    "main": self.main_display == Some(d.id), "status": self.monitor_status_for(Some(d.id)),
                })
            })
            .collect()
    }

    /// The display `id`, else the primary one (the first reported).
    fn display_or_primary(&self, id: Option<u32>) -> Option<&Display> {
        id.and_then(|id| self.displays.iter().find(|d| d.id == id)).or(self.displays.first())
    }

    fn resolved_monitor(&self, display: Option<u32>) -> (Arc<Profile>, MonitorStatus) {
        let spec = self.settings.monitor_profile.as_str();
        let auto = spec.is_empty() || spec == "auto";
        let shown = self.display_or_primary(display);
        let mode = display_mode(self.output_environment, shown.is_some_and(|d| d.compositor_managed), !auto);
        let icc = if auto { shown.and_then(|d| d.icc.clone()) } else { None };
        let mut cache = self.display.monitor.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = cache.get(&display)
            && e.mode == mode
            && e.spec == spec
            && e.display == shown.map(|d| d.id)
            && e.detection == self.monitor_detection
            && match (&e.icc, &icc) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
        {
            return (e.profile.clone(), e.status.clone());
        }
        let found = if mode != DisplayMode::AppManaged {
            Ok(Arc::new(
                match mode {
                    DisplayMode::CompositorP3 => Builtin::DisplayP3,
                    _ => Builtin::Srgb,
                }
                .profile()
                .clone(),
            ))
        } else if auto && self.output_environment == OutputEnvironment::Wayland {
            // An unconfirmed Wayland compositor/output gets standard sRGB. Applying a colord
            // ICC here could double-correct a managed surface; an explicit selection overrides it.
            Err("native Wayland output management is unknown; using tagged sRGB".into())
        } else if auto {
            match (shown, &icc, &self.monitor_detection) {
                (_, Some(b), _) => profile_from_bytes(b).map_err(|e| format!("the display profile can't be read: {e}")),
                (Some(d), None, _) => Err(format!("{} has no ICC profile", d.name)),
                (None, None, MonitorDetection::Pending) => Err("the display profile hasn't been read yet".into()),
                (None, None, MonitorDetection::Failed { reason }) => Err(reason.clone()),
                (None, None, _) => Err("this platform doesn't report display profiles".into()),
            }
        } else {
            resolve_profile(spec, None, Some(photocraft_color::ColorMode::Rgb)).map_err(|e| e.to_string())
        };
        let usable = found.and_then(|p| {
            if p.color_space != ColorSpace::Rgb {
                return Err(format!("`{}` is a {:?} profile, not RGB", p.description, p.color_space));
            }
            // The canvas transforms end at this profile: one that can't be a destination would
            // otherwise leave the canvas silently unmanaged.
            Transform::new(Builtin::Srgb.profile(), &p, DISPLAY_INTENT, DISPLAY_BPC)
                .map_err(|e| format!("`{}` can't be used as a display profile: {e}", p.description))?;
            Ok(p)
        });
        let (p, source, reason) = match usable {
            Ok(p) => (
                p,
                if mode != DisplayMode::AppManaged {
                    "compositor"
                } else if auto {
                    "auto"
                } else {
                    "manual"
                },
                (mode != DisplayMode::AppManaged).then(|| "the OS converts the tagged surface to the panel profile".into()),
            ),
            Err(r) => (Arc::new(Builtin::Srgb.profile().clone()), "fallback", Some(r)),
        };
        let status = MonitorStatus {
            requested: if spec.is_empty() { "auto".into() } else { spec.to_string() },
            source,
            display: shown.map(|d| d.name.clone()),
            profile: p.description.clone(),
            profile_name: if auto { shown.and_then(|d| d.profile_name.clone()) } else { None },
            fingerprint: format!("{:016x}", p.content_hash()),
            detection: self.monitor_detection.clone(),
            reason,
        };
        if cache.len() > 16 {
            cache.clear();
        }
        let entry = MonitorEntry {
            mode,
            spec: spec.to_string(),
            display: shown.map(|d| d.id),
            icc,
            detection: self.monitor_detection.clone(),
            profile: p.clone(),
            status: status.clone(),
        };
        cache.insert(display, entry);
        (p, status)
    }

    /// How the canvas shows `doc` on the main window's display.
    pub fn canvas_display(&self, doc: &Document) -> Result<Arc<CanvasDisplay>> {
        self.canvas_display_for(doc, self.main_display)
    }

    /// How the canvas shows `doc` on `display` (cached per document profile, mode and monitor
    /// profile).
    pub fn canvas_display_for(&self, doc: &Document, display: Option<u32>) -> Result<Arc<CanvasDisplay>> {
        let space = mode_space(doc.mode);
        let doc_hash = doc.icc_profile.as_ref().and_then(|b| profile_from_bytes(b).ok()).filter(|p| p.color_space == space).map_or(0, |p| p.content_hash());
        let monitor = self.monitor_for(display);
        let key = (space, doc_hash, monitor.content_hash());
        if let Some(d) = self.display.canvas.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return Ok(d.clone());
        }
        let composite = composite_profile(doc);
        let encode_srgb = is_linear_rgb(&composite);
        let source = if encode_srgb { Arc::new(srgb_curve_twin(&composite)) } else { composite };
        let transform = if source.content_hash() == monitor.content_hash() {
            None
        } else {
            let t = Transform::new(&source, &monitor, DISPLAY_INTENT, DISPLAY_BPC).map_err(|e| EngineError::Other(format!("colour management: {e}")))?;
            (!is_identity(&t, space == ColorSpace::Gray)).then(|| Arc::new(t))
        };
        let d = Arc::new(CanvasDisplay {
            encode_srgb,
            key: hash_of((source.content_hash(), monitor.content_hash(), encode_srgb, transform.is_some())),
            texture_key: hash_of((source.content_hash(), encode_srgb)),
            source,
            monitor,
            transform,
        });
        let mut c = self.display.canvas.lock().unwrap_or_else(|e| e.into_inner());
        if c.len() > 32 {
            c.clear();
        }
        c.insert(key, d.clone());
        Ok(d)
    }

    /// Changes whenever the canvas display of `doc` on the main window's display changes.
    pub fn display_signature(&self, doc: &Document) -> u64 {
        self.display_signature_for(doc, self.main_display)
    }

    /// Changes whenever the canvas display of `doc` on `display` changes: profiles, monitor,
    /// Proof Colors, Gamut Warning and 32-bit preview settings (for the UI's LUT cache).
    pub fn display_signature_for(&self, doc: &Document, display: Option<u32>) -> u64 {
        let cm = self.canvas_display_for(doc, display).map(|d| d.key).unwrap_or(0);
        // Per frame: read the proof state in place (the default state allocates a profile).
        let proof = self.proof_ref(doc.id).filter(|pv| pv.enabled || pv.gamut_warning).map(|pv| {
            let s = &pv.setup;
            (pv.enabled, pv.gamut_warning, s.profile.content_hash(), s.intent, s.bpc, s.simulate_paper, s.kind, pv.gamut_threshold.to_bits())
        });
        let hdr =
            crate::proof_sim::hdr_active(self, doc).then(|| self.hdr.get(&doc.id).map(|h| (h.highlight_compression, h.exposure.to_bits(), h.gamma.to_bits())));
        hash_of((cm, proof, hdr))
    }
}

#[cfg(test)]
#[path = "display_color_tests.rs"]
mod tests;

#[test]
fn dsfix_parallel_display_keeps_dither_phase_alpha_and_fractional_values() {
    let source = Arc::new(Builtin::Srgb.profile().clone());
    let display = CanvasDisplay { encode_srgb: false, source: source.clone(), monitor: source, transform: None, key: 0, texture_key: 0 };
    let rect = photocraft_geom::Rect::new(3, 5, 136, 72);
    let buf = Buffer::filled(rect, [0.5, 0.25, 0.75, 0.3]);
    let image = display.to_rgba8(&buf);
    for (i, p) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
        let (x, y) = (i % 133 + 3, i / 133 + 5);
        assert_eq!(*p, [quantize_display(0.5, x, y), quantize_display(0.25, x, y), quantize_display(0.75, x, y), 77]);
    }
}
