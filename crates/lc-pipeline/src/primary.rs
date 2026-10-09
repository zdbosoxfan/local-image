//! Primary Develop tools. Scene-linear, unclipped Rec.2020, after WB/reconstruction.
//! LI Tone uses vkdt's local-Laplacian construction (BSD-2-Clause) with a smooth
//! EV remap, ten levels of intensity sampling, and a pyramid reaching 1×1.
//! B uses darktable's faithful EIGF at multiple scales. Both apply ratio-preserving gains.
use crate::{Plan, SourceInfo, eigf, llf, ucs};
use lightcraft_develop::{ClarityMode, DevelopSettings, SkinTone};
use lightcraft_raster::resample::{Filter, resize};
use lightcraft_raster::{Plane, Rgb32f};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HsMethod {
    LiTone,
    Eigf,
}
/// Owner's A/B choice: changing the default is one line; never persisted or exposed in UI.
pub const DEFAULT_HS_METHOD: HsMethod = HsMethod::LiTone;
pub const PROXY_PIXELS: usize = 2_000_000;

/// Independent stage guards. Neutral controls never build a plane or run a pixel pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stages {
    pub tone: bool,
    pub clarity: bool,
    pub texture: bool,
    pub structure: bool,
    pub equalizer: bool,
    pub balance: bool,
    pub skin: bool,
}
impl Stages {
    pub fn of(s: &DevelopSettings) -> Self {
        Self {
            tone: s.light.highlights != 0. || s.light.shadows != 0. || s.light.whites != 0. || s.light.blacks != 0.,
            clarity: s.section_enabled("effects") && s.effects.clarity != 0.,
            texture: s.section_enabled("effects") && s.effects.texture != 0.,
            structure: s.section_enabled("effects") && s.effects.structure != 0.,
            equalizer: !s.mixer.is_neutral() || crate::is_bw(s),
            balance: s.color.vibrance != 0. || s.color.saturation != 0. || !s.grading.is_neutral(),
            skin: s.section_enabled("skinTone") && s.skin_tone.reference.is_some() && (s.skin_tone.uniformity != 0. || s.skin_tone.lightness != 0.),
        }
    }
    pub fn any(self) -> bool {
        self.tone || self.clarity || self.texture || self.structure || self.equalizer || self.balance || self.skin
    }
    pub fn proxy(self) -> bool {
        self.tone || self.clarity
    }
}

pub fn needs_proxy(s: &DevelopSettings) -> bool {
    Stages::of(s).proxy()
        || s.masks.iter().any(|m| {
            m.visible && !m.components.is_empty() && {
                let mut d = m.tools.view((6500., 0.));
                d.light.highlights += m.adjust.highlights;
                d.light.shadows += m.adjust.shadows;
                d.effects.clarity += m.adjust.clarity;
                d.disabled_sections = s.disabled_sections.clone();
                Stages::of(&d).proxy()
            }
        })
}

pub fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn norm(c: [f32; 3]) -> f32 {
    ((c[0] * c[0] + c[1] * c[1] + c[2] * c[2]) / 3.).sqrt()
}
pub fn log_light(c: [f32; 3]) -> f32 {
    let n = norm(c).max(1e-7);
    let mx = c[0].max(c[1]).max(c[2]);
    let t = smooth(0., 3., (n / 0.18).log2());
    ((n + (mx - n) * t).max(1e-7) / 0.18).log2()
}
/// The slider remap has bounded slope, continuous first derivative and preserves ordering.
pub fn tone_gain(l: f32, hl: f32, sh: f32, wh: f32, bl: f32) -> f32 {
    1.6 * hl * smooth(0., 4., l) + 1.8 * sh * (1. - smooth(-6., 0., l)) + 0.8 * wh * smooth(3., 7., l) + 0.8 * bl * (1. - smooth(-10., -4., l))
}

fn plane_from_fn(w: usize, h: usize, f: impl Fn(usize, usize) -> f32 + Sync + Send) -> Plane {
    let mut out = Plane::new(w, h);
    crate::for_rows(&mut out.data, w, |y, row| {
        for (x, v) in row.iter_mut().enumerate() {
            *v = f(x, y);
        }
    });
    out
}

// vkdt reduce.comp's exact 1,4,6,4,1 stencil path. Unlike darktable's padded pyramid,
// clamp-to-edge lets rectangular/tiny levels safely reach 1px, with bounded memory.
fn reduce(p: &Plane) -> Plane {
    const W: [f32; 5] = [1. / 16., 4. / 16., 6. / 16., 4. / 16., 1. / 16.];
    plane_from_fn(p.width.div_ceil(2), p.height.div_ceil(2), |x, y| {
        let mut s = 0.;
        for j in 0..5 {
            for i in 0..5 {
                let xx = (2 * x + i).saturating_sub(2).min(p.width - 1);
                let yy = (2 * y + j).saturating_sub(2).min(p.height - 1);
                s += p.get(xx, yy) * W[i] * W[j];
            }
        }
        s
    })
}
fn expand(p: &Plane, w: usize, h: usize) -> Plane {
    // vkdt assemble.comp's 5-tap polyphase stencil path.
    plane_from_fn(w, h, |x, y| {
        let even = [(-1, 0.125), (0, 0.75), (1, 0.125)];
        let odd = [(0, 0.5), (1, 0.5)];
        let xs: &[(isize, f32)] = if x % 2 == 0 { &even } else { &odd };
        let ys: &[(isize, f32)] = if y % 2 == 0 { &even } else { &odd };
        let mut s = 0.;
        for &(j, wy) in ys {
            for &(i, wx) in xs {
                s += wx
                    * wy
                    * p.get(
                        (x as isize / 2 + i).clamp(0, p.width as isize - 1) as usize,
                        (y as isize / 2 + j).clamp(0, p.height as isize - 1) as usize,
                    );
            }
        }
        s
    })
}
fn pyramid(p: Plane) -> Vec<Plane> {
    let mut v = vec![p];
    while v.last().is_some_and(|p| p.width > 1 || p.height > 1) {
        let p = &v[v.len() - 1];
        v.push(reduce(p));
    }
    v
}
/// Log-EV extension of vkdt llap: interpolated remapped Laplacians, remapped residual.
/// Detail's slope is one near gamma; large-scale beta is the derivative of the slider curve.
pub fn li_tone(l: &Plane, hl: f32, sh: f32, wh: f32, bl: f32) -> Plane {
    if hl == 0. && sh == 0. && wh == 0. && bl == 0. {
        return Plane::new(l.width, l.height);
    }
    let lo = l.data.iter().copied().fold(f32::INFINITY, f32::min).min(-6.);
    let hi = l.data.iter().copied().fold(f32::NEG_INFINITY, f32::max).max(4.);
    let range = (hi - lo).max(1.);
    let ng = 10;
    let orig = pyramid(l.clone());
    let mut acc: Vec<_> = orig.iter().map(|p| Plane::new(p.width, p.height)).collect();
    let step = range / (ng - 1) as f32;
    let sigma = step.max(1.0);
    for k in 0..ng {
        let g = lo + k as f32 * step;
        let remap = |x: f32| {
            let d = x - g;
            let keep = (-1.5 * d * d / (sigma * sigma)).exp();
            x + tone_gain(g, hl, sh, wh, bl) * keep + tone_gain(x, hl, sh, wh, bl) * (1. - keep)
        };
        let remapped = pyramid(l.map(remap));
        for lev in 0..orig.len() {
            let p = &orig[lev];
            let ex = if lev + 1 < orig.len() { Some(expand(&remapped[lev + 1], p.width, p.height)) } else { None };
            for i in 0..p.len() {
                let t = ((p.data[i] - lo) / step).clamp(0., (ng - 1) as f32);
                let a = t.floor().min((ng - 2) as f32) as usize;
                let f = t - a as f32;
                let weight = if k == a {
                    1. - f
                } else if k == a + 1 {
                    f
                } else {
                    0.
                };
                let lap = remapped[lev].data[i] - ex.as_ref().map_or(0., |q| q.data[i]);
                acc[lev].data[i] += weight * lap;
            }
        }
    }
    let mut out = acc.pop().unwrap_or_else(|| l.clone());
    for p in acc.into_iter().rev() {
        let ex = expand(&out, p.width, p.height);
        out = p.zip_map(&ex, |a, b| a + b);
    }
    out.zip_map(l, |a, b| (a - b).clamp(-3., 3.))
}

pub fn eigf_tone(l: &Plane, hl: f32, sh: f32, wh: f32, bl: f32) -> Plane {
    if hl == 0. && sh == 0. && wh == 0. && bl == 0. {
        return Plane::new(l.width, l.height);
    }
    let lum = l.map(|v| (v * std::f32::consts::LN_2).exp() * 0.18);
    let mut out = Plane::new(l.width, l.height);
    for (fraction, weight) in [(0.005, 0.25), (0.015, 0.5), (0.045, 0.25)] {
        let mut p = eigf::Params::new((l.width.max(l.height) as f32 * fraction).max(1.), 0.08);
        p.iterations = 3;
        p.quantization = 0.5;
        let base = eigf::filter(&lum, p);
        for (o, &v) in out.data.iter_mut().zip(&base.data) {
            *o += weight * tone_gain((v / 0.18).max(1e-7).log2(), hl, sh, wh, bl);
        }
    }
    out
}

/// Cross fast guided upsampling of a gain field, guide = full-resolution log EV.
pub fn upsample_gain(proxy_l: &Plane, gain: &Plane, full_l: &Plane) -> Plane {
    let sigma = (proxy_l.width.max(proxy_l.height) as f32 * 0.002).max(1.);
    let mean = |p: &Plane| lightcraft_raster::blur::gaussian(p, sigma);
    let ml = mean(proxy_l);
    let mg = mean(gain);
    let var = mean(&proxy_l.map(|v| v * v));
    let cov = mean(&proxy_l.zip_map(gain, |a, b| a * b));
    let mut a = Plane::new(proxy_l.width, proxy_l.height);
    let mut b = a.clone();
    for i in 0..a.len() {
        a.data[i] = (cov.data[i] - ml.data[i] * mg.data[i]) / ((var.data[i] - ml.data[i] * ml.data[i]).max(0.) + 0.05);
        b.data[i] = mg.data[i] - a.data[i] * ml.data[i];
    }
    let up = |p: &Plane| resize(&mean(p), full_l.width, full_l.height, Filter::Bilinear);
    let (a, b) = (up(&a), up(&b));
    Plane::from_fn(full_l.width, full_l.height, |x, y| a.get(x, y) * full_l.get(x, y) + b.get(x, y))
}

/// A fixed-size proxy of the same geometric frame, independent of requested export/preview.
pub fn proxy_for(src: &Rgb32f, info: &SourceInfo, plan: &Plan<'_>) -> Rgb32f {
    let p = proxy_plan(src, plan);
    let mut img = p.frame.sample(src, p.w, p.h);
    crate::lin_cpu(&mut img, info, &p);
    crate::local::denoise(&mut img, &p.settings, p.src_long, p.px_per_long, info.sensor_scale);
    img
}

/// The CPU and GPU proxy resolve precisely the same frame, WB, retouching and NR parameters.
pub fn proxy_plan<'a>(src: &Rgb32f, plan: &Plan<'a>) -> Plan<'a> {
    let (nw, nh) = plan.frame.native_size();
    let scale = ((PROXY_PIXELS as f64 / (nw * nh).max(1.)).sqrt()).min(1.);
    let (w, h) = plan.frame.fit((nw * scale).round().max(1.) as usize, (nh * scale).round().max(1.) as usize);
    // Reuse the resolved frame/settings: applying a profile or Upright again, or
    // restoring a crop suppressed by the render request, would change this proxy.
    let px_per_long = plan.frame.px_per_long(w);
    let mut p = Plan {
        settings: plan.settings.clone(),
        frame: plan.frame.clone(),
        w,
        h,
        px_per_long,
        src_long: plan.src_long,
        geo: 0,
        lin_key: 0,
        eyes: Vec::new(),
    };
    p.eyes = crate::redeye::resolve(src, &p.settings.red_eye, p.settings.orientation, &p.frame, w, h, px_per_long);
    p
}

fn layer_settings(s: &DevelopSettings, m: &lightcraft_develop::Mask) -> DevelopSettings {
    let mut d = m.tools.view((6500., 0.));
    d.light.highlights += m.adjust.highlights;
    d.light.shadows += m.adjust.shadows;
    d.effects.clarity += m.adjust.clarity;
    d.effects.texture += m.adjust.texture;
    d.disabled_sections = s.disabled_sections.clone();
    d
}
pub fn layers_active(s: &DevelopSettings) -> bool {
    s.masks.iter().any(|m| {
        m.visible
            && !m.components.is_empty()
            && (Stages::of(&layer_settings(s, m)).any() || m.tools.wb.is_some() || m.adjust.temp != 0. || m.adjust.tint != 0.)
    })
}
pub fn needs_clip(s: &DevelopSettings) -> bool {
    s.light.highlights < 0. || s.masks.iter().any(|m| m.visible && !m.components.is_empty() && layer_settings(s, m).light.highlights < 0.)
}
pub fn active(s: &DevelopSettings) -> bool {
    Stages::of(s).any() || layers_active(s)
}
/// Remove only tools consumed in this scene stage; the rest retain their established stages.
pub fn remaining(s: &DevelopSettings) -> DevelopSettings {
    let mut d = s.clone();
    clear(&mut d);
    for m in &mut d.masks {
        m.adjust.temp = 0.;
        m.adjust.tint = 0.;
        m.adjust.highlights = 0.;
        m.adjust.shadows = 0.;
        m.adjust.clarity = 0.;
        m.adjust.texture = 0.;
        if let Some(l) = &mut m.tools.light {
            l.highlights = 0.;
            l.shadows = 0.;
            l.whites = 0.;
            l.blacks = 0.;
        }
        if let Some(e) = &mut m.tools.effects {
            e.clarity = 0.;
            e.texture = 0.;
            e.structure = 0.;
        }
        m.tools.light = m.tools.light.filter(|l| l.exposure != 0. || l.contrast != 0.);
        m.tools.effects = m.tools.effects.filter(|e| e.dehaze != 0.);
        m.tools.wb = None;
        m.tools.color = None;
        m.tools.mixer = None;
        m.tools.grading = None;
        m.tools.bw_mix = None;
        m.tools.treatment = None;
        m.tools.skin_tone = None;
    }
    d
}
fn clear(d: &mut DevelopSettings) {
    d.light.highlights = 0.;
    d.light.shadows = 0.;
    d.light.whites = 0.;
    d.light.blacks = 0.;
    d.effects.clarity = 0.;
    d.effects.texture = 0.;
    d.effects.structure = 0.;
    d.color = Default::default();
    d.mixer = Default::default();
    d.grading = Default::default();
    d.bw_mix = Default::default();
    if crate::is_bw(d) {
        d.point_colors.clear();
    }
    d.treatment = lightcraft_develop::Treatment::Color;
    // Built-in monochrome profiles have already expanded their settings and have no LUT.
    if d.profile.id == "lc.mono" || d.profile.id.starts_with("lc.bw.") {
        d.profile.id = "lc.neutral".into();
    }
    d.skin_tone = Default::default();
}

/// Fraction clipped, transformed with the same geometry. No scene-RGB clipping heuristic.
fn clips(info: &SourceInfo, plan: &Plan<'_>) -> Option<Plane> {
    let c = info.clip_confidence.as_ref()?;
    if c.width == 0 || c.height == 0 || c.data.len() != c.width.checked_mul(c.height)? {
        return None;
    }
    let rgb = Rgb32f { width: c.width, height: c.height, data: c.data.iter().map(|v| [*v; 3]).collect() };
    Some(plan.frame.sample(&rgb, plan.w, plan.h).map(|c| c[0].clamp(0., 1.)))
}

/// Apply global and layer tools at their stage, using the shared mask alpha/opacity.
pub fn process(img: &Rgb32f, proxy: &Rgb32f, info: &SourceInfo, plan: &Plan<'_>, method: HsMethod) -> Rgb32f {
    let s = &*plan.settings;
    let mut out = img.clone();
    let gain = (s.light.exposure as f32).exp2();
    if gain != 1. {
        out.map_in_place(|p| p.map(|v| v * gain));
    }
    let proxy = if gain == 1. { std::borrow::Cow::Borrowed(proxy) } else { std::borrow::Cow::Owned(proxy.map(|p| p.map(|v| v * gain))) };
    let masks = if layers_active(s) {
        let log = img.map(crate::local::log_lum);
        crate::masks::evaluate_with_tone(
            &s.masks,
            &plan.frame,
            plan.w,
            plan.h,
            img,
            &log,
            s.light.exposure as f32,
            &crate::tone2::tone_map(s, info, s.light.contrast, 0., 0.),
        )
    } else {
        Vec::new()
    };
    let clip = needs_clip(s).then(|| clips(info, plan)).flatten();
    apply_tone_detail(
        &mut out,
        &proxy,
        s,
        clip.as_ref(),
        method,
        plan.px_per_long,
        crate::detail::out_per_orig(plan.px_per_long, plan.src_long, info.sensor_scale),
    );
    apply_colour(&mut out, s, plan.px_per_long);
    for (m, alpha) in s.masks.iter().filter(|m| m.visible && !m.components.is_empty()).zip(&masks) {
        let mut d = m.tools.view(crate::local::effective_wb(info, s));
        d.light.highlights += m.adjust.highlights;
        d.light.shadows += m.adjust.shadows;
        d.effects.texture += m.adjust.texture;
        d.effects.clarity += m.adjust.clarity;
        d.disabled_sections = s.disabled_sections.clone();
        let flags = Stages::of(&d);
        let from = crate::local::effective_wb(info, s);
        let to = m.tools.wb.map_or((from.0 * (m.adjust.temp / 100. * 0.6).exp(), from.1 + m.adjust.tint * 0.4), |w| (w.temp, w.tint));
        let wb = (m.tools.wb.is_some() || m.adjust.temp != 0. || m.adjust.tint != 0.).then(|| crate::local::wb_change(from, to)).flatten();
        if wb.is_some() || flags.tone || flags.clarity || flags.texture || flags.structure {
            let mut q = out.clone();
            if let Some(mat) = wb {
                q.map_in_place(|c| ucs::mul(&mat, c));
            }
            apply_tone_detail(
                &mut q,
                &proxy,
                &d,
                clip.as_ref(),
                method,
                plan.px_per_long,
                crate::detail::out_per_orig(plan.px_per_long, plan.src_long, info.sensor_scale),
            );
            for i in 0..out.len() {
                out.data[i] = crate::layers::mix3(out.data[i], q.data[i], alpha.alpha.data[i]);
            }
        }
        if flags.equalizer || flags.balance || flags.skin {
            let mut q = out.clone();
            apply_colour(&mut q, &d, plan.px_per_long);
            for i in 0..out.len() {
                out.data[i] = crate::layers::mix3(out.data[i], q.data[i], alpha.alpha.data[i]);
            }
        }
    }
    if gain != 1. {
        out.map_in_place(|p| p.map(|v| v / gain));
    }
    out
}

fn apply_tone_detail(img: &mut Rgb32f, proxy: &Rgb32f, s: &DevelopSettings, clip: Option<&Plane>, method: HsMethod, ppl: f64, band_scale: f32) {
    let stages = Stages::of(s);
    if !stages.tone && !stages.clarity && !stages.texture && !stages.structure {
        return;
    }
    let l = img.map(log_light);
    let pl = proxy.map(log_light);
    let (hl, sh, wh, bl) =
        (s.light.highlights as f32 / 100., s.light.shadows as f32 / 100., s.light.whites as f32 / 100., s.light.blacks as f32 / 100.);
    if hl != 0. || sh != 0. || wh != 0. || bl != 0. {
        let field = |h| match method {
            HsMethod::LiTone => li_tone(&pl, h, sh, wh, bl),
            HsMethod::Eigf => eigf_tone(&pl, h, sh, wh, bl),
        };
        let g = upsample_gain(&pl, &field(hl), &l);
        let nohl = (hl < 0. && clip.is_some_and(|p| p.data.iter().any(|v| *v > 0.66))).then(|| upsample_gain(&pl, &field(0.), &l));
        for (i, c) in img.data.iter_mut().enumerate() {
            let mut ev = g.data[i];
            if let (Some(cp), Some(base)) = (clip, nohl.as_ref()) {
                ev = base.data[i] + (ev - base.data[i]) * (1. - smooth(0.66, 1., cp.data[i]));
            }
            *c = c.map(|v| v * ev.exp2());
        }
    }
    if !stages.clarity && !stages.texture && !stages.structure {
        return;
    }
    let cl = s.effects.clarity as f32 / 100.;
    let tx = s.effects.texture as f32 / 100.;
    let st = s.effects.structure as f32 / 100.;
    let mut field = Plane::new(img.width, img.height);
    if cl != 0. {
        // darktable's local Laplacian detail remap, only medium-scale bands. Log mapped into 0..1.
        let lo = -12.;
        let range = 24.;
        let mapped = pl.map(|v| ((v - lo) / range).clamp(0., 1.));
        let levels = llf::num_levels(mapped.width, mapped.height);
        let center = (ppl as f32 * 0.015 * (proxy.width as f32 / img.width as f32)).max(1.).log2();
        let weights = (0..levels).map(|i| (-((i as f32 - center) / 1.5).powi(2)).exp()).collect();
        let p = llf::Params {
            remap: llf::Remap::Darktable { sigma: 1.5 / range, shadows: 1., highlights: 1., clarity: 1. },
            num_gamma: 12,
            level_weights: Some(weights),
            remap_residual: false,
        };
        // Weighted identity is subtracted too; weights suppress the baseline Laplacians as well.
        let identity = llf::Params { remap: llf::Remap::Darktable { sigma: 1.5 / range, shadows: 1., highlights: 1., clarity: 0. }, ..p.clone() };
        let detail = llf::local_laplacian_with(&mapped, &p).zip_map(&llf::local_laplacian_with(&mapped, &identity), |a, b| (a - b) * range);
        field = upsample_gain(&pl, &detail, &l);
        for (v, &ll) in field.data.iter_mut().zip(&l.data) {
            *v *= cl * (-(ll / 3.5).powi(2)).exp();
        }
    }
    if tx != 0. || st != 0. {
        // Shadow noise bias before log/EIGF; pixel bands use the actual output-to-sensor pixel scale.
        let biased = img.map(|c| norm(c).max(0.) + 0.004);
        let band = |a: f32, b: f32| {
            let blur = |radius: f32| {
                let sigma = (radius * band_scale).max(0.01);
                if sigma < 1. { cross_eigf(&biased, &biased, sigma, 0.15) } else { eigf::filter(&biased, eigf::Params::new(sigma, 0.15)) }
            };
            let p = blur(a);
            let q = blur(b);
            p.zip_map(&q, |a, b| (a.max(1e-7) / b.max(1e-7)).log2().clamp(-0.5, 0.5))
        };
        if tx != 0. {
            for (v, b) in field.data.iter_mut().zip(band(4., 16.).data) {
                *v += tx * b;
            }
        }
        if st != 0. {
            for (v, b) in field.data.iter_mut().zip(band(2., 4.).data) {
                *v += st * b;
            }
        }
    }
    for (i, c) in img.data.iter_mut().enumerate() {
        let ev = field.data[i];
        *c = c.map(|v| v * ev.exp2());
        if cl != 0. && s.effects.clarity_mode != ClarityMode::Neutral {
            let lw = ucs::y_to_l_star(1.);
            let mut h = ucs::rgb_to_hsb(*c, lw);
            let k = match s.effects.clarity_mode {
                ClarityMode::Natural => 1. - 0.15 * cl.abs() * smooth(2., 6., l.data[i]) * h[1].clamp(0., 1.),
                ClarityMode::Punch => 1. + 0.15 * ev.abs(),
                ClarityMode::Neutral => 1.,
            };
            h[1] *= k;
            ucs::gamut_map_hsb(&mut h, lw);
            *c = ucs::hsb_to_rgb(h, lw);
        }
    }
}
fn apply_colour(img: &mut Rgb32f, s: &DevelopSettings, ppl: f64) {
    let stages = Stages::of(s);
    if stages.equalizer {
        crate::colorequal::apply(img, s, ppl);
    }
    if stages.balance {
        let k = crate::balance::Balance::new(s);
        img.map_in_place(|c| k.apply(c));
    }
    if stages.skin {
        skin(img, &s.skin_tone, ppl);
    }
}

/// Skin uniformity edits only low-frequency chroma; original pore/texture residual survives.
pub fn skin(img: &mut Rgb32f, s: &SkinTone, ppl: f64) {
    let Some(reference) = s.reference else {
        return;
    };
    if s.uniformity == 0. && s.lightness == 0. {
        return;
    }
    let lw = ucs::y_to_l_star(1.);
    let refc = [reference[0] as f32, reference[1] as f32, (reference[2] as f32).to_radians()];
    let jch: Vec<_> = img.data.iter().map(|c| ucs::xyy_to_jch(ucs::xyz_to_xyy(ucs::mul(&ucs::mats().rgb_to_xyz, *c)), lw)).collect();
    let planes: [Plane; 3] = std::array::from_fn(|k| Plane {
        width: img.width,
        height: img.height,
        data: jch
            .iter()
            .map(|c| {
                if k == 0 {
                    c[0]
                } else if k == 1 {
                    c[1] * c[2].cos()
                } else {
                    c[1] * c[2].sin()
                }
            })
            .collect(),
    });
    let guide = img.map(|c| norm(c).max(1e-6));
    let low: [Plane; 3] = std::array::from_fn(|k| cross_eigf(&guide, &planes[k], (ppl as f32 * 0.0075).max(1.), 0.05));
    let wrap = |v: f32| (v + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    for i in 0..img.len() {
        let c = jch[i];
        let dh = wrap(c[2] - refc[2]).abs();
        let wh = (s.hue_range as f32).to_radians();
        let wc = 0.04 + s.chroma_range as f32 / 100. * 0.4;
        let wl = 0.05 + s.lightness_range as f32 / 100. * 0.5;
        let mut w = (1. - smooth(wh * 0.5, wh, dh))
            * (1. - smooth(wc * 0.5, wc, (c[1] - refc[1]).abs()))
            * (1. - smooth(wl * 0.5, wl, (c[0] - refc[0]).abs()))
            * smooth(0.005, 0.03, c[1]);
        // Redder colours than the selected skin: soft lip protection, independent of image lightness.
        if s.protect_lips {
            w *= 1. - smooth(0.12, 0.3, -wrap(c[2] - refc[2]));
        }
        if w <= 0. {
            continue;
        }
        let u = w * s.uniformity as f32 / 100.;
        let v = w * s.lightness as f32 / 100. * 0.25;
        let a = planes[1].data[i] + u * (refc[1] * refc[2].cos() - low[1].data[i]);
        let b = planes[2].data[i] + u * (refc[1] * refc[2].sin() - low[2].data[i]);
        let new = [(c[0] + v * (refc[0] - low[0].data[i])).max(0.), (a * a + b * b).sqrt(), b.atan2(a)];
        let mut h = ucs::jch_to_hsb(new);
        ucs::gamut_map_hsb(&mut h, lw);
        img.data[i] = ucs::hsb_to_rgb(h, lw);
    }
}
/// EIGF covariance blend extended to signed colour planes (no floor on the guided output).
pub fn cross_eigf(guide: &Plane, p: &Plane, sigma: f32, eps: f32) -> Plane {
    let mean = |p: &Plane| Plane { width: p.width, height: p.height, data: eigf::mean(&p.data, p.width, p.height, 1, sigma) };
    let mg = mean(guide);
    let mp = mean(p);
    let gg = mean(&guide.map(|v| v * v));
    let gp = mean(&guide.zip_map(p, |a, b| a * b));
    Plane::from_fn(p.width, p.height, |x, y| {
        let g = guide.get(x, y);
        let m = mg.get(x, y);
        let ng = (m * g).max(1e-6);
        let a = (gp.get(x, y) - m * mp.get(x, y)) / ((gg.get(x, y) - m * m).max(0.) + eps * ng);
        a * g + mp.get(x, y) - a * m
    })
}

pub fn key(plan: &Plan<'_>, info: &SourceInfo) -> u64 {
    key_for(plan, info, DEFAULT_HS_METHOD)
}
pub fn key_for(plan: &Plan<'_>, info: &SourceInfo, method: HsMethod) -> u64 {
    let s = &*plan.settings;
    crate::hash_of((
        method,
        plan.lin_key,
        format!(
            "{:?}",
            (
                &s.light,
                &s.color,
                &s.mixer,
                &s.grading,
                &s.skin_tone,
                &s.bw_mix,
                crate::is_bw(s),
                (s.effects.clarity, s.effects.texture, s.effects.structure, s.effects.clarity_mode),
                (s.look, &s.look_options),
                &s.disabled_sections,
                &s.masks,
            )
        ),
        info.raw_clip_level.map(f32::to_bits),
        info.raw,
        format!("{:?}", (info.camera_tone, info.look_curve, info.profile_curve)),
        info.clip_confidence.as_ref().map(|c| Arc::as_ptr(c) as usize),
    ))
}
pub type Cached = Option<(u64, Arc<Rgb32f>)>;
