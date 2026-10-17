//! Sequential Develop adjustment layers in unclipped, display-referred linear Rec.2020.
//!
//! Background runs its complete scene/finish tools first. Visible `Mask::tools` then run in
//! stored list order (bottom to top), each on the finished result below it:
//! `out = mix(input, Tools(input), coverage * amount * opacity)`. The output transform,
//! proofing and quantization run once, after the stack. Imported `Mask::adjust` stays in the
//! Background's established stages; it is not converted into a stacked tool set.
//!
//! Spatial tools build their fields from this layer's input, not from the source. Exposure is
//! a linear gain; contrast is log slope about 0.18 without a second camera tone map. WB is a
//! chromatic adaptation relative to the Background white. RAW reconstruction is Background-only.
//! Filled coverage uses a scalar, including the exposure-only fast path. Cached prefixes are
//! keyed by the Background and all earlier layers; editing k invalidates k..top only.
//!
//! `LayerK` below describes the historical stage blends used by low-level finish helpers;
//! render entry points remove `tools` from Background and use the sequential evaluator.

use std::sync::Arc;

use li_raster::RgbFrame;
use lightcraft_develop::{DevelopSettings, LayerTools, Mask};

use crate::SourceInfo;
use crate::colorops::ColorOps;
use crate::finish::Vig;
use crate::tone::ToneMap;

/// The masks that are evaluated (visible, with components), in order: their position here is
/// their index in the evaluated list (`Prepared::masks`).
fn evaluated(s: &DevelopSettings) -> impl Iterator<Item = &Mask> {
    s.masks.iter().filter(|m| m.visible && !m.components.is_empty())
}

/// Whether any evaluated mask holds layer tools.
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
            let v = view(&m.tools, info, s);
            let key = crate::hash_of((
                [d.nr_luminance, d.nr_detail, d.nr_color, d.nr_color_detail, d.nr_color_smoothness, d.nr_contrast].map(f64::to_bits),
                format!("{:?}", info.camera_noise),
                format!("{:?}", crate::local::wb_matrix_for(info, &v)),
            ));
            Some((i, key, v))
        })
        .collect()
}

/// The linear image `img` (already denoised with the photo's settings) denoised once more with
/// each layer's noise reduction, reusing `cache` (key → image) and leaving only the images still
/// in use in it. Each entry: (evaluated mask index, image).
pub(crate) fn nr_images(
    img: &Arc<RgbFrame>,
    s: &DevelopSettings,
    info: &SourceInfo,
    src_long: usize,
    px_per_long: f64,
    cache: &mut Vec<(u64, Arc<RgbFrame>)>,
) -> Vec<(usize, Arc<RgbFrame>)> {
    let layers = nr_layers(s, info);
    let mut keep = Vec::new();
    let mut out = Vec::new();
    for (i, key, v) in layers {
        let im = match cache.iter().chain(keep.iter()).find(|(k, _)| *k == key) {
            Some((_, im)) => im.clone(),
            None => {
                let mut c = (**img).clone();
                crate::local::denoise_source(&mut c, &v, src_long, px_per_long, info);

                Arc::new(c)
            }
        };
        if !keep.iter().any(|(k, _): &(u64, Arc<RgbFrame>)| *k == key) {
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

/// Background keeps imported local adjustments in their historical stages. Neutral adjustment
/// masks do not invalidate its caches when an adjustment layer's mask changes.
pub fn background(s: &DevelopSettings) -> DevelopSettings {
    let mut b = s.clone();
    b.masks.retain(|m| {
        let mut a = m.adjust;
        a.amount = 100.0;
        a.color_hue = 0.0;
        a != lightcraft_develop::LocalAdjustments::default()
    });
    for m in &mut b.masks {
        m.tools = LayerTools::default();
    }
    b
}

/// A layer's complete, neutral-by-default tools and white adaptation. No image-level tools.
pub fn settings(m: &Mask, s: &DevelopSettings, info: &SourceInfo) -> (DevelopSettings, Option<[[f32; 3]; 3]>) {
    let white = crate::local::effective_wb(info, s);
    let mut d = m.tools.view(white);
    d.disabled_sections = s.disabled_sections.clone();
    let wb = m.tools.wb.and_then(|w| crate::local::wb_change(white, (w.temp, w.tint)));
    (d, wb)
}

/// +1 EV is exactly twice the developed linear RGB, including values above display white.
/// The contrast slope uses the established slider response without a bounded base curve.
pub fn contrast(c: [f32; 3], amount: f32) -> [f32; 3] {
    if amount == 0.0 {
        return c;
    }
    let y = li_color::luminance_2020(c).max(1e-9);
    let slope = crate::tone2::contrast_slope(amount as f64);
    let gain = (y / crate::tone::GREY).powf(slope - 1.0);
    c.map(|v| v * gain)
}

/// Whites and Blacks use developed-image anchors, rather than RAW's over-white range.
/// Middle grey is 0.18; Whites ramps through +0.5..+2.5 EV, Blacks through -6..-3 EV.
/// Exposure shifts selection, but its gain itself is still applied only by finish.
pub fn anchors(c: [f32; 3], exposure: f32, whites: f32, blacks: f32) -> [f32; 3] {
    let light = crate::primary::log_light(c.map(|v| v * exposure.exp2()));
    let ev = 0.8 * whites * crate::primary::smooth(0.5, 2.5, light) + 0.8 * blacks * (1.0 - crate::primary::smooth(-6.0, -3.0, light));
    c.map(|v| v * ev.exp2())
}

/// Skip all spatial preparation for layers setting exposure alone.
pub fn exposure_only(m: &Mask) -> Option<f32> {
    let mut t = m.tools.clone();
    let light = t.light.take()?;
    let mut neutral = light;
    neutral.exposure = 0.0;
    (t.is_empty() && neutral == Default::default()).then(|| (light.exposure as f32).exp2())
}

/// Shared rolling cache keys for CPU and device-resident stack prefixes.
pub fn background_key(plan: &crate::Plan<'_>, info: &SourceInfo, quality: crate::Quality, method: crate::primary::HsMethod) -> u64 {
    crate::hash_of((crate::primary::key_for(plan, info, method), format!("{:?}", plan.settings), quality == crate::Quality::Draft))
}
pub fn key(input: u64, m: &Mask) -> u64 {
    crate::hash_of((input, format!("{:?}", (&m.tools, &m.components, m.invert, m.refine)), crate::masks::mask_scale(m).to_bits()))
}

/// Maximum retained stack pixels per output size. The current image is always retained,
/// even when a single full-resolution image exceeds this bound. No new compute pool is created.
pub const STACK_CACHE_BYTES: usize = 512 << 20;

/// Prefix cache (at most two output sizes live in StageCache). Pixel work uses the application's
/// existing bounded rayon pool; no thread-per-layer pool or simultaneous layer renders.
#[derive(Clone, Default)]
pub(crate) struct Cache {
    pub outputs: Vec<(u64, Option<Arc<RgbFrame>>)>,
}

pub(crate) struct Stacked {
    pub image: Arc<RgbFrame>,
    pub background_renders: usize,
    pub layer_renders: usize,
    pub masks: Vec<crate::masks::Evaluated>,
    pub diagnostic: Option<li_raster::GrayFrame>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn stack(
    prep: &crate::Prepared,
    plan: &crate::Plan<'_>,
    original: &DevelopSettings,
    info: &SourceInfo,
    req: &crate::RenderRequest,
    method: crate::primary::HsMethod,
    base_key: u64,
    cache: &mut Cache,
) -> Stacked {
    let mut background_renders = 0;
    let mut layer_renders = 0;
    let mut input = match cache.outputs.first().filter(|p| p.0 == base_key) {
        Some((_, Some(image))) => image.clone(),
        _ => {
            background_renders += 1;
            Arc::new(crate::finish::working(prep, &plan.settings, plan, info, false, None))
        }
    };
    let mut keep = vec![(base_key, Some(input.clone()))];
    let mut ik = base_key;
    let mut masks = Vec::new();
    let mut diagnostic = if req.overlay == crate::Overlay::SkinToneRange(None) {
        req.overlay
            .skin(original)
            .filter(|s| s.reference.is_some())
            .map(|skin| crate::visualize::skin_alpha(&prep.img, skin, (original.light.exposure as f32).exp2()))
    } else {
        None
    };
    for m in evaluated(original).filter(|m| !m.tools.is_empty()) {
        let k = key(ik, m);
        if req.overlay == crate::Overlay::SkinToneRange(Some(m.id)) {
            diagnostic = req.overlay.skin(original).filter(|s| s.reference.is_some()).map(|skin| crate::visualize::skin_alpha(&input, skin, 1.0));
        }
        if let Some((_, Some(out))) = cache.outputs.get(keep.len()).filter(|p| p.0 == k) {
            if req.overlay.mask(original).is_some_and(|mask| mask.id == m.id) {
                let log = input.map(crate::local::log_lum);
                let alpha = crate::masks::scalar_alpha(m).map(crate::masks::Alpha::Scalar).unwrap_or_else(|| {
                    crate::masks::Alpha::Spatial(crate::masks::evaluate_one_tone(
                        m,
                        &plan.frame,
                        plan.w,
                        plan.h,
                        &input,
                        &log,
                        0.0,
                        Some(&crate::ToneMap::working_identity()),
                    ))
                });
                masks.push(crate::masks::Evaluated { id: m.id, alpha, adjust: m.adjust });
            }
            input = out.clone();
        } else {
            layer_renders += 1;
            let log = if crate::masks::scalar_alpha(m).is_none() { input.map(crate::local::log_lum) } else { li_raster::GrayFrame::new(0, 0) };
            let alpha = crate::masks::scalar_alpha(m).map(crate::masks::Alpha::Scalar).unwrap_or_else(|| {
                crate::masks::Alpha::Spatial(crate::masks::evaluate_one_tone(
                    m,
                    &plan.frame,
                    plan.w,
                    plan.h,
                    &input,
                    &log,
                    0.0,
                    Some(&crate::ToneMap::working_identity()),
                ))
            });
            let mut out = (*input).clone();
            if matches!(alpha, crate::masks::Alpha::Scalar(0.0)) {
                // No covered pixels: no tool field or pixel evaluation is needed.
            } else if let Some(gain) = exposure_only(m) {
                crate::for_rows(&mut out.data, plan.w, |y, row| {
                    for (x, c) in row.iter_mut().enumerate() {
                        let a = alpha.at(y * plan.w + x);
                        if a != 0.0 {
                            *c = c.map(|v| v * (1.0 + (gain - 1.0) * a));
                        }
                    }
                });
            } else {
                let (mut d, wb) = settings(m, original, info);
                let mut tool_input = (*input).clone();
                if let Some(mat) = wb {
                    tool_input.map_in_place(|c| crate::ucs::mul(&mat, c));
                }
                crate::local::denoise_source(&mut tool_input, &d, plan.src_long, plan.px_per_long, info);
                if d.light.whites != 0.0 || d.light.blacks != 0.0 {
                    let light = d.light;
                    tool_input.map_in_place(|c| anchors(c, light.exposure as f32, light.whites as f32 / 100.0, light.blacks as f32 / 100.0));
                    d.light.whites = 0.0;
                    d.light.blacks = 0.0;
                }
                let layer_info = SourceInfo { sensor_scale: info.sensor_scale, ..Default::default() };
                let mut lp = crate::Plan {
                    settings: std::borrow::Cow::Owned(d.clone()),
                    frame: plan.frame.clone(),
                    w: plan.w,
                    h: plan.h,
                    px_per_long: plan.px_per_long,
                    src_long: plan.src_long,
                    geo: plan.geo,
                    lin_key: ik,
                    eyes: Vec::new(),
                };
                if crate::primary::active(&d) {
                    // The proxy is this layer's developed input, never the original RAW.
                    let scale = ((crate::primary::PROXY_PIXELS as f64 / input.len().max(1) as f64).sqrt()).min(1.0);
                    let proxy = if crate::primary::needs_proxy(&d) {
                        tool_input.resize(
                            (plan.w as f64 * scale).round().max(1.0) as usize,
                            (plan.h as f64 * scale).round().max(1.0) as usize,
                            li_raster::resample::Filter::Bilinear,
                        )
                    } else {
                        RgbFrame::new(0, 0)
                    };
                    tool_input = crate::primary::process(&tool_input, &proxy, &layer_info, &lp, method);
                    d = crate::primary::remaining(&d);
                    lp.settings = std::borrow::Cow::Owned(d.clone());
                }
                let p = crate::local::prepare(
                    Arc::new(tool_input),
                    &d,
                    &layer_info,
                    &plan.frame,
                    plan.px_per_long,
                    plan.src_long,
                    req.quality,
                    &mut crate::local::Planes::default(),
                );
                let q = crate::finish::working(&p, &d, &lp, &layer_info, true, Some(&alpha));
                crate::for_rows(&mut out.data, plan.w, |y, row| {
                    for (x, c) in row.iter_mut().enumerate() {
                        let i = y * plan.w + x;
                        let a = alpha.at(i);
                        if a != 0.0 {
                            *c = mix3(*c, q.data[i], a);
                        }
                    }
                });
            }
            if req.overlay.mask(original).is_some_and(|mask| mask.id == m.id) {
                masks.push(crate::masks::Evaluated { id: m.id, alpha, adjust: m.adjust });
            }
            input = Arc::new(out);
        }
        keep.push((k, Some(input.clone())));
        // Keep all preview prefixes, but avoid N full-resolution images on small machines.
        // Evicted keys retain their positions; a surviving suffix can still be reused exactly.
        let bytes = input.data.len() * 12;
        let capacity = (STACK_CACHE_BYTES / bytes.max(1)).max(1);
        let retained = keep.iter().filter(|p| p.1.is_some()).count();
        let mut evict = retained.saturating_sub(capacity);
        for (_, image) in &mut keep {
            if evict > 0 && image.is_some() {
                *image = None;
                evict -= 1;
            }
        }
        ik = k;
    }
    cache.outputs = keep;
    Stacked { image: input, background_renders, layer_renders, masks, diagnostic }
}
