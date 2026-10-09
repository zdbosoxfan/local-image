//! Develop layers (Capture One-style): every mask can hold any develop tool that isn't
//! image-level ([`LayerTools`]), faded by its opacity.
//!
//! Evaluated the darktable way, not once per layer: each tool still runs once at its own stage of
//! the pipeline, and at that stage every layer that sets it blends in its own instance,
//! `out = mix(in, tool_L(in), alpha_L)` with `alpha_L` the mask's alpha × amount × opacity. So the
//! layer's exposure still comes before the tone map and its curve after the photo's curve, and N
//! layers cost one render plus the per-pixel work of the tools they set (nothing for the others).
//!
//! Where each layer tool runs (see `finish::finish_with`):
//! * noise reduction: the linear image denoised once more with the layer's settings
//!   ([`nr_images`]), blended at the start of the per-pixel stage;
//! * dehaze: right after the photo's dehaze (scene linear);
//! * white balance (a change from the photo's white to the layer's) and exposure: with the
//!   masks' local exposure, before highlights/shadows;
//! * highlights/shadows, clarity, texture, sharpening: the local tone stage (log luminance, the
//!   photo's edge-aware planes);
//! * contrast, whites, blacks: the tone map, built with the photo's values plus the layer's;
//! * vibrance, saturation, colour mixer, Point Color, B&W, colour grading: after the photo's;
//! * vignette, tone curves (encoded values), grain: after the photo's.
//!
//! The GPU renderer has no kernels for these: settings with layer tools render on the CPU
//! ([`active`], `lightcraft_gpu::render`).

use std::sync::Arc;

use lightcraft_develop::{DevelopSettings, LayerTools, Mask};
use lightcraft_raster::Rgb32f;

use crate::SourceInfo;
use crate::colorops::ColorOps;
use crate::finish::Vig;
use crate::tone::ToneMap;

/// The masks that are evaluated (visible, with components), in order: their position here is
/// their index in the evaluated list (`Prepared::masks`).
fn evaluated(s: &DevelopSettings) -> impl Iterator<Item = &Mask> {
    s.masks.iter().filter(|m| m.visible && !m.components.is_empty())
}

/// Whether any evaluated mask holds layer tools (such settings render on the CPU).
pub fn active(s: &DevelopSettings) -> bool {
    evaluated(s).any(|m| !m.tools.is_empty())
}

/// The layer's local-tone stage: highlights, shadows, clarity, texture, sharpening (and masking),
/// in the units of the photo's (see `finish::FinishParams`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LocalTone {
    pub hl: f32,
    pub sh: f32,
    pub clar: f32,
    pub tex: f32,
    pub sharpen_mask: f32,
    /// Pixel-scale sharpening (see [`crate::detail`]): Amount / 100 and Detail (0..1), on the
    /// layer's sharpening planes (its own radius).
    pub sharp_amount: f32,
    pub sharpen_detail: f32,
}

impl LocalTone {
    fn is_neutral(&self) -> bool {
        self.hl == 0.0 && self.sh == 0.0 && self.clar == 0.0 && self.tex == 0.0 && self.sharp_amount == 0.0
    }
}

/// One layer's tools, resolved for the per-pixel stage. `None` / zero: the layer doesn't change
/// that tool (and it isn't evaluated for it).
pub struct LayerK {
    /// Index of the layer's mask among the evaluated masks.
    pub mask: usize,
    /// White balance change from the photo's white to the layer's (linear Rec.2020).
    pub wb: Option<[[f32; 3]; 3]>,
    /// Exposure gain.
    pub gain: Option<f32>,
    pub dehaze: f32,
    pub local: Option<LocalTone>,
    /// The tone map with the photo's contrast / whites / blacks plus the layer's.
    pub tone: Option<ToneMap>,
    pub ops: Option<ColorOps>,
    pub vig: Option<Vig>,
    /// Tone curves on encoded values and Refine Saturation (0..1).
    pub curves: Option<(crate::finish::CurveTables, f32)>,
    /// Grain: amount, cell size (px), roughness, seed.
    pub grain: Option<(f32, f32, f32, u32)>,
}

impl LayerK {
    /// Whether the layer changes the scene-linear colour before the local tone stage.
    pub fn scene(&self) -> bool {
        self.wb.is_some() || self.gain.is_some() || self.dehaze != 0.0
    }
}

/// The view of a layer's tools (see [`LayerTools::view`]) against the photo's white balance.
fn view(t: &LayerTools, info: &SourceInfo, s: &DevelopSettings) -> DevelopSettings {
    t.view(crate::local::effective_wb(info, s))
}

/// The layers of `s` that change something, resolved (for a `px_per_long` render).
pub fn resolve(s: &DevelopSettings, info: &SourceInfo, px_per_long: f64) -> Vec<LayerK> {
    let effects = s.section_enabled("effects");
    let mut out = Vec::new();
    for (mask, m) in evaluated(s).enumerate() {
        let t = &m.tools;
        if t.is_empty() {
            continue;
        }
        let v = view(t, info, s);
        let wb = t.wb.and_then(|w| crate::local::wb_change(crate::local::effective_wb(info, s), (w.temp, w.tint)));
        let gain = t.light.filter(|l| l.exposure != 0.0).map(|l| 2f32.powf(l.exposure as f32));
        let e = if effects { t.effects.unwrap_or_default() } else { Default::default() };
        let local = LocalTone {
            hl: (v.light.highlights / 100.0) as f32,
            sh: (v.light.shadows / 100.0) as f32,
            clar: (e.clarity / 100.0) as f32,
            tex: (e.texture / 100.0) as f32,
            sharpen_mask: (v.detail.sharpen_masking / 100.0) as f32,
            sharp_amount: (v.detail.sharpen_amount / 100.0) as f32,
            sharpen_detail: (v.detail.sharpen_detail / 100.0) as f32,
        };
        let tone = t.light.filter(|l| l.contrast != 0.0 || l.whites != 0.0 || l.blacks != 0.0).map(|l| {
            let c = |g: f64, d: f64| (g + d).clamp(-100.0, 100.0);
            crate::finish::tone_map(s, info, c(s.light.contrast, l.contrast), c(s.light.whites, l.whites), c(s.light.blacks, l.blacks))
        });
        let colour =
            t.color.is_some() || t.mixer.is_some() || t.point_colors.is_some() || t.treatment.is_some() || t.bw_mix.is_some() || t.grading.is_some();
        let ops = colour.then(|| ColorOps::new(&v)).filter(|o| !o.is_identity());
        let curves = t.curve.as_ref().and_then(|c| crate::finish::curve_luts(c).map(|l| (l, (c.refine_saturation / 100.0).clamp(0.0, 1.0) as f32)));
        let vig = if effects && t.vignette.is_some() { crate::finish::vignette(&v) } else { None };
        let grain = if effects && t.grain.is_some() { crate::finish::grain(&v, px_per_long) } else { None };
        let k = LayerK {
            mask,
            wb,
            gain,
            dehaze: (e.dehaze / 100.0) as f32,
            local: (!local.is_neutral()).then_some(local),
            tone,
            ops,
            vig,
            curves,
            grain,
        };
        let any = k.scene() || k.local.is_some() || k.tone.is_some() || k.ops.is_some() || k.vig.is_some() || k.curves.is_some() || k.grain.is_some();
        if any {
            out.push(k);
        }
    }
    out
}

/// Which spatial planes the layers of `s` need: (edge-aware base, clarity, texture, dark channel
/// or dehaze plane, pixel-scale sharpening planes).
pub fn planes_needed(s: &DevelopSettings) -> (bool, bool, bool, bool, bool) {
    let effects = s.section_enabled("effects");
    let (mut base, mut clarity, mut texture, mut dark, mut sharp) = (false, false, false, false, false);
    for m in evaluated(s) {
        let t = &m.tools;
        if let Some(l) = t.light {
            base |= l.highlights != 0.0 || l.shadows != 0.0;
        }
        if let Some(e) = t.effects.filter(|_| effects) {
            clarity |= e.clarity != 0.0;
            texture |= e.texture != 0.0;
            dark |= e.dehaze != 0.0;
        }
        if let Some(d) = t.detail {
            sharp |= d.sharpen_amount != 0.0;
        }
    }
    (base, clarity, texture, dark, sharp)
}

/// Noise-reduction parameters of each layer that sets them: (evaluated mask index, cache key,
/// the layer's view).
fn nr_layers(s: &DevelopSettings, info: &SourceInfo) -> Vec<(usize, u64, DevelopSettings)> {
    evaluated(s)
        .enumerate()
        .filter_map(|(i, m)| {
            let d = m.tools.detail.filter(|d| d.nr_luminance > 0.0 || d.nr_color > 0.0)?;
            let key =
                crate::hash_of([d.nr_luminance, d.nr_detail, d.nr_color, d.nr_color_detail, d.nr_color_smoothness, d.nr_contrast].map(f64::to_bits));
            let v = view(&m.tools, info, s);
            Some((i, key, v))
        })
        .collect()
}

/// The linear image `img` (already denoised with the photo's settings) denoised once more with
/// each layer's noise reduction, reusing `cache` (key → image) and leaving only the images still
/// in use in it. Each entry: (evaluated mask index, image).
pub(crate) fn nr_images(
    img: &Arc<Rgb32f>,
    s: &DevelopSettings,
    info: &SourceInfo,
    src_long: usize,
    px_per_long: f64,
    cache: &mut Vec<(u64, Arc<Rgb32f>)>,
) -> Vec<(usize, Arc<Rgb32f>)> {
    let layers = nr_layers(s, info);
    let mut keep = Vec::new();
    let mut out = Vec::new();
    for (i, key, v) in layers {
        let im = match cache.iter().chain(keep.iter()).find(|(k, _)| *k == key) {
            Some((_, im)) => im.clone(),
            None => {
                let mut c = (**img).clone();
                crate::local::denoise(&mut c, &v, src_long, px_per_long, info.sensor_scale);

                Arc::new(c)
            }
        };
        if !keep.iter().any(|(k, _): &(u64, Arc<Rgb32f>)| *k == key) {
            keep.push((key, im.clone()));
        }
        out.push((i, im));
    }
    *cache = keep;
    out
}

/// `a + (b − a)·t` per channel.
#[inline]
pub fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
