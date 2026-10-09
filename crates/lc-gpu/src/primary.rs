//! Device-resident primary stages. No image readback or host image upload is permitted here.
//! CPU metadata (matrices, RBF/gamut LUTs, Deriche coefficients) is shared with the reference.
use crate::ctx::{Buf, groups1, groups2};
use crate::render::{Cx, gaussian};
use lightcraft_develop::{ClarityMode, DevelopSettings};
use lightcraft_pipeline::{
    Plan, SourceInfo,
    primary::{HsMethod, Stages},
    ucs,
};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Clone, Default)]
pub(crate) struct Cache {
    slots: HashMap<String, (u64, Arc<Buf>)>,
}
impl Cache {
    fn get(&mut self, slot: impl Into<String>, key: u64, build: impl FnOnce() -> Buf) -> Arc<Buf> {
        let slot = slot.into();
        if let Some((k, b)) = self.slots.get(&slot)
            && *k == key
        {
            return b.clone();
        }
        let b = Arc::new(build());
        self.slots.insert(slot, (key, b.clone()));
        b
    }
    pub(crate) fn proxy(&mut self, key: u64, build: impl FnOnce() -> Buf) -> Arc<Buf> {
        self.get("proxy", key, build)
    }
    pub(crate) fn clip(&mut self, geo: u64, ptr: usize, build: impl FnOnce() -> Buf) -> Arc<Buf> {
        self.get("clip", key((geo, ptr)), build)
    }
    fn find(&self, slot: &str, key: u64) -> Option<Arc<Buf>> {
        self.slots.get(slot).filter(|v| v.0 == key).map(|v| v.1.clone())
    }
    fn put(&mut self, slot: String, key: u64, buf: Buf) -> Arc<Buf> {
        let buf = Arc::new(buf);
        self.slots.insert(slot, (key, buf.clone()));
        buf
    }
    pub(crate) fn buffers(&self) -> impl Iterator<Item = &Arc<Buf>> {
        self.slots.values().map(|v| &v.1)
    }
}
fn key(v: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}
fn f(x: f32) -> u32 {
    x.to_bits()
}
fn run(cx: &mut Cx<'_>, name: &str, n: usize, extra: &[u32], input: [Option<&Buf>; 5], len: usize) -> Buf {
    let out = cx.gpu.buffer(len);
    into(cx, name, n, extra, input, &out);
    out
}
fn into(cx: &mut Cx<'_>, name: &str, n: usize, extra: &[u32], input: [Option<&Buf>; 5], out: &Buf) {
    let mut p = vec![n as u32];
    p.extend_from_slice(extra);
    cx.run(name, &p, &[input[0], input[1], input[2], input[3], input[4], Some(out)], groups1(n));
}
fn one(cx: &mut Cx<'_>, name: &str, n: usize, extra: &[u32], a: &Buf, len: usize) -> Buf {
    run(cx, name, n, extra, [Some(a), None, None, None, None], len)
}
fn bounds(cx: &mut Cx<'_>, a: &Buf, n: usize, nc: usize, lo: f32, hi: f32) -> Buf {
    let groups = groups1(n);
    let mut count = (groups[0] * groups[1]) as usize;
    let mut out = one(cx, "p_bounds", n, &[nc as u32, f(lo), f(hi)], a, count * 8);
    while count > 1 {
        let next = groups1(count);
        let len = (next[0] * next[1]) as usize;
        out = one(cx, "p_bounds_join", count, &[nc as u32, f(lo), f(hi)], &out, len * 8);
        count = len;
    }
    out
}
/// Faithful Deriche kernel, channel clamps stay on the GPU when reduced from moments.
fn deriche(cx: &mut Cx<'_>, a: &Buf, w: usize, h: usize, nc: usize, sigma: f32, limit: Option<&Buf>, bound: f32) -> Buf {
    let alpha = 1.695 / sigma.max(0.01);
    let em = (-alpha).exp();
    let em2 = (-2. * alpha).exp();
    let b1 = -2. * em;
    let b2 = em2;
    let k = (1. - em) * (1. - em) / (1. + 2. * alpha * em - em2);
    let (a0, a1, a2, a3) = (k, k * (alpha - 1.) * em, k * (alpha + 1.) * em, -k * em2);
    let cp = (a0 + a1) / (1. + b1 + b2);
    let cn = (a2 + a3) / (1. + b1 + b2);
    let fallback = limit.is_none().then(|| cx.gpu.upload(&[-bound, -bound, -bound, -bound, bound, bound, bound, bound].map(f)));
    let limit = limit.or(fallback.as_ref());
    let coefficients = [a0, a1, a2, a3, b1, b2, cp, cn].map(f);
    let mut p = vec![w as u32, h as u32, nc as u32, 0];
    p.extend(coefficients);
    let mut params = vec![(w * nc) as u32];
    params.extend_from_slice(&p);
    let tmp = cx.gpu.buffer(w * h * nc);
    cx.run("p_deriche", &params, &[Some(a), limit, None, None, None, Some(&tmp)], groups2(w * nc, 1, [64, 1]));
    let transposed = transpose(cx, &tmp, w, h, nc);
    params[0] = (h * nc) as u32;
    params[1] = h as u32;
    params[2] = w as u32;
    let out = cx.gpu.buffer(w * h * nc);
    cx.run("p_deriche", &params, &[Some(&transposed), limit, None, None, None, Some(&out)], groups2(h * nc, 1, [64, 1]));
    transpose(cx, &out, h, w, nc)
}
fn transpose(cx: &mut Cx<'_>, src: &Buf, w: usize, h: usize, nc: usize) -> Buf {
    let out = cx.gpu.buffer(w * h * nc);
    cx.run(
        "p_transpose",
        &[(w * h) as u32, w as u32, h as u32, nc as u32],
        &[Some(src), None, None, None, None, Some(&out)],
        groups2(w, h, [16, 16]),
    );
    out
}
fn interpolate(cx: &mut Cx<'_>, a: &Buf, dim: (usize, usize), out: (usize, usize), nc: usize) -> Buf {
    if dim == out {
        return cx.copy(a);
    }
    one(cx, "p_interp", out.0 * out.1, &[dim.0 as u32, dim.1 as u32, out.0 as u32, out.1 as u32, nc as u32], a, out.0 * out.1 * nc)
}
fn extract(cx: &mut Cx<'_>, a: &Buf, n: usize, nc: usize, channels: usize, off: usize) -> Buf {
    one(cx, "p_extract", n, &[nc as u32, channels as u32, off as u32], a, n * channels)
}
fn eigf(cx: &mut Cx<'_>, input: &Buf, w: usize, h: usize, sigma: f32, eps: f32, iterations: usize, quant: f32) -> Buf {
    let scale = sigma.clamp(1., 4.);
    let dim = ((w as f32 / scale) as usize, (h as f32 / scale) as usize);
    let dim = (dim.0.max(1), dim.1.max(1));
    let dn = dim.0 * dim.1;
    let nc = if quant == 0. { 2 } else { 4 };
    let mut out = cx.copy(input);
    for _ in 0..iterations {
        let ds = interpolate(cx, &out, (w, h), dim, 1);
        let mask = (quant != 0.).then(|| one(cx, "p_quant", w * h, &[f(quant)], &out, w * h));
        let guide = interpolate(cx, mask.as_ref().unwrap_or(&out), (w, h), dim, 1);
        let moments = run(cx, "p_moments", dn, &[nc as u32], [Some(&guide), Some(&ds), None, None, None], dn * nc);
        let limit = bounds(cx, &moments, dn, nc, 1e7, 0.);
        let av = deriche(cx, &moments, dim.0, dim.1, nc, (sigma / scale).max(1.), Some(&limit), f32::MAX);
        let av = one(cx, "p_variance", dn, &[nc as u32], &av, dn * nc);
        let av = interpolate(cx, &av, dim, (w, h), nc);
        out = run(cx, "p_eigf_apply", w * h, &[nc as u32, f(eps)], [Some(&out), Some(&av), mask.as_ref(), None, None], w * h);
    }
    out
}
fn cross(cx: &mut Cx<'_>, guide: &Buf, input: &Buf, w: usize, h: usize, sigma: f32, eps: f32) -> Buf {
    let m = run(cx, "p_cross_pre", w * h, &[], [Some(guide), Some(input), None, None, None], w * h * 4);
    let m = deriche(cx, &m, w, h, 4, sigma, None, f32::MAX);
    run(cx, "p_cross_apply", w * h, &[f(eps)], [Some(guide), Some(&m), None, None, None], w * h)
}
fn gain_ab(cx: &mut Cx<'_>, pl: &Buf, field: &Buf, w: usize, h: usize) -> Buf {
    let sigma = (w.max(h) as f32 * 0.002).max(1.);
    let m = run(cx, "p_cross_pre", w * h, &[], [Some(pl), Some(field), None, None, None], w * h * 4);
    let m = gaussian(cx, &m, w, h, 4, sigma);
    let ab = one(cx, "p_gain_ab", w * h, &[], &m, w * h * 2);
    gaussian(cx, &ab, w, h, 2, sigma)
}
fn up_gain(cx: &mut Cx<'_>, l: &Buf, ab: &Buf, (w, h): (usize, usize), (pw, ph): (usize, usize)) -> Buf {
    let resized = (pw > w || ph > h).then(|| crate::render::resize(cx, ab, (pw, ph), (w, h), 2, lightcraft_raster::resample::Filter::Bilinear));
    let (ab, pw, ph) = resized.as_ref().map_or((ab, pw, ph), |b| (b, w, h));
    run(cx, "p_up_gain", w * h, &[w as u32, h as u32, pw as u32, ph as u32], [Some(l), Some(ab), None, None, None], w * h)
}
fn pyramid(cx: &mut Cx<'_>, first: Arc<Buf>, w: usize, h: usize, nc: usize) -> Vec<(usize, usize, Arc<Buf>)> {
    let mut v = vec![(w, h, first)];
    let (mut w, mut h) = (w, h);
    while w > 1 || h > 1 {
        let nw = w.div_ceil(2);
        let nh = h.div_ceil(2);
        let b = one(cx, "p_ll_reduce", nw * nh, &[w as u32, h as u32, nc as u32], &v[v.len() - 1].2, nw * nh * nc);
        v.push((nw, nh, Arc::new(b)));
        w = nw;
        h = nh;
    }
    v
}
fn li_tone(cx: &mut Cx<'_>, pl: &Arc<Buf>, w: usize, h: usize, sliders: [f32; 4], cache: &mut Cache, pk: u64, scope: &str) -> Buf {
    let slot = format!("{scope}/ll-basis");
    let basis = if let Some(b) = cache.find(&slot, pk) {
        b
    } else {
        let limit = cache.get(format!("{scope}/ll-bounds"), pk, || bounds(cx, pl, w * h, 1, 1e30, -1e30));
        let mut orig = vec![(w, h, pl.clone())];
        let (mut pw, mut ph) = (w, h);
        let mut lev = 1;
        while pw > 1 || ph > 1 {
            let nw = pw.div_ceil(2);
            let nh = ph.div_ceil(2);
            let prev = &orig[lev - 1].2;
            let next =
                cache.get(format!("{scope}/ll-original/{lev}"), pk, || one(cx, "p_ll_reduce", nw * nh, &[pw as u32, ph as u32, 1], prev, nw * nh));
            orig.push((nw, nh, next));
            pw = nw;
            ph = nh;
            lev += 1;
        }
        let mut acc: Vec<_> = orig.iter().map(|(w, h, _)| cx.zeroed(w * h * 4)).collect();
        for gam in 0..10 {
            let remap = run(cx, "p_ll_remap", w * h, &[gam], [Some(pl), Some(&limit), None, None, None], w * h * 4);
            let pyr = pyramid(cx, Arc::new(remap), w, h, 4);
            for (lev, (pw, ph, original)) in orig.iter().enumerate() {
                into(
                    cx,
                    "p_ll_acc",
                    pw * ph,
                    &[*pw as u32, *ph as u32, gam, (lev + 1 < orig.len()) as u32],
                    [Some(&pyr[lev].2), pyr.get(lev + 1).map(|v| &*v.2), Some(original), Some(&limit), None],
                    &acc[lev],
                );
            }
        }
        let mut out = acc.pop().unwrap_or_else(|| cx.zeroed(w * h * 4));
        for ((pw, ph, _), fine) in orig.iter().zip(acc).rev() {
            out = run(cx, "p_ll_assemble", pw * ph, &[*pw as u32, *ph as u32], [Some(&fine), Some(&out), None, None, None], pw * ph * 4);
        }
        let basis = run(cx, "p_ll_gain", w * h, &[], [Some(&out), Some(pl), None, None, None], w * h * 4);
        cache.put(slot, pk, basis)
    };
    one(cx, "p_ll_mix", w * h, &sliders.map(f), &basis, w * h)
}

fn clarity(cx: &mut Cx<'_>, pl: &Buf, w: usize, h: usize, ppl: f64, full_w: usize) -> Buf {
    if w < 4 || h < 4 {
        return cx.zeroed(w * h);
    }
    let nl = lightcraft_pipeline::llf::num_levels(w, h).min(30);
    let last = nl - 1;
    let pad = 1usize << last;
    let pw = w + 2 * pad;
    let ph = h + 2 * pad;
    let first = one(cx, "p_dt_pad", pw * ph, &[w as u32, h as u32, pad as u32], pl, pw * ph);
    let mut orig = vec![(pw, ph, first)];
    for lev in 1..=last {
        let (lw, lh, prev) = &orig[lev - 1];
        let nw = lw.div_ceil(2);
        let nh = lh.div_ceil(2);
        let next = one(cx, "p_dt_reduce", nw * nh, &[*lw as u32, *lh as u32, 1], prev, nw * nh);
        orig.push((nw, nh, next));
    }
    let center = (ppl as f32 * 0.015 * (w as f32 / full_w as f32)).max(1.).log2();
    let acc: Vec<_> = orig.iter().take(last).map(|(w, h, _)| cx.zeroed(w * h * 2)).collect();
    for gam in 0..12 {
        let first = one(cx, "p_dt_remap", pw * ph, &[gam], &orig[0].2, pw * ph * 2);
        let mut remap = vec![first];
        for lev in 1..=last {
            let (lw, lh, _) = &orig[lev - 1];
            let (nw, nh, _) = &orig[lev];
            let next = one(cx, "p_dt_reduce", nw * nh, &[*lw as u32, *lh as u32, 2], &remap[lev - 1], nw * nh * 2);
            remap.push(next);
        }
        for lev in (0..last).rev() {
            let (lw, lh, guide) = &orig[lev];
            let wt = (-((lev as f32 - center) / 1.5).powi(2)).exp();
            into(
                cx,
                "p_dt_acc",
                lw * lh,
                &[*lw as u32, *lh as u32, gam, f(wt)],
                [Some(&remap[lev]), Some(&remap[lev + 1]), Some(guide), None, None],
                &acc[lev],
            );
        }
    }
    let (lw, lh, top) = &orig[last];
    let mut out = one(cx, "p_dt_top", lw * lh, &[], top, lw * lh * 2);
    for lev in (0..last).rev() {
        let (lw, lh, _) = &orig[lev];
        out = run(cx, "p_dt_assemble", lw * lh, &[*lw as u32, *lh as u32], [Some(&acc[lev]), Some(&out), None, None, None], lw * lh * 2);
    }
    one(cx, "p_dt_crop", w * h, &[w as u32, pad as u32], &out, w * h)
}
fn lut(cx: &mut Cx<'_>, s: &DevelopSettings) -> Buf {
    use lightcraft_pipeline::colorequal::{hues, rbf};
    use std::f32::consts::PI;
    let bands = s.mixer.bands();
    let mut data = ucs::gamut_lut().to_vec();
    data.extend(rbf(bands.map(|b| b.hue as f32 / 100. * 0.5), *hues(), PI, false));
    data.extend(rbf(bands.map(|b| 1. + b.sat as f32 / 200.), *hues(), PI, true));
    data.extend(rbf(bands.map(|b| 1. + b.lum as f32 / 200.), *hues(), PI, true));
    data.extend(rbf(s.bw_mix.bands().map(|b| b as f32 / 100.), *hues(), PI, false));
    data.extend((-4096..=4096).map(|i| (1. / (1. + (-60. * 0.5 * i as f64 / 4096.).exp())) as f32));
    cx.gpu.upload(&data.into_iter().map(f).collect::<Vec<_>>())
}
#[cfg(test)]
fn uv_guided(cx: &mut Cx<'_>, uv: &Buf, target: &Buf, w: usize, h: usize, sigma: f32, eps: f32, brightness: bool) -> Buf {
    uv_guided_cached(cx, uv, target, w, h, sigma, eps, brightness, &mut Cache::default(), 0, "test")
}
#[allow(clippy::too_many_arguments)]
fn uv_guided_cached(
    cx: &mut Cx<'_>,
    uv: &Buf,
    target: &Buf,
    w: usize,
    h: usize,
    sigma: f32,
    eps: f32,
    brightness: bool,
    cache: &mut Cache,
    ik: u64,
    scope: &str,
) -> Buf {
    let scaling = (sigma - 1.5).floor().clamp(1., 4.);
    let gs = (sigma / scaling).max(0.2);
    let (dw, dh) = ((w as f32 / scaling) as usize, (h as f32 / scaling) as usize);
    let (dw, dh) = (dw.max(1), dh.max(1));
    let n = dw * dh;
    let guide = cache.get(format!("{scope}/guide"), ik, || interpolate(cx, uv, (w, h), (dw, dh), 2));
    let target = interpolate(cx, target, (w, h), (dw, dh), 2);
    let cov = cache.get(format!("{scope}/cov"), ik, || {
        let cov = one(cx, "p_uv_cov", n, &[0], &guide, n * 4);
        deriche(cx, &cov, dw, dh, 4, gs, None, 1e9)
    });
    let corr = run(cx, "p_uv_cov", n, &[1], [Some(&guide), Some(&target), None, None, None], n * 4);
    let mean = cache.get(format!("{scope}/mean"), ik, || deriche(cx, &guide, dw, dh, 2, gs, None, 1e9));
    let mt = deriche(cx, &target, dw, dh, 2, gs, None, 1e9);
    let b = brightness.then(|| {
        let single = extract(cx, &target, n, 2, 1, 1);
        deriche(cx, &single, dw, dh, 1, 0.1 * gs, None, 1e9)
    });

    let corr = deriche(cx, &corr, dw, dh, 4, gs, None, 1e9);
    let aa = run(cx, "p_uv_ab", n, &[f(eps), brightness as u32, 0, 0], [Some(&mean), Some(&mt), Some(&cov), Some(&corr), b.as_ref()], n * 4);
    let bb = run(cx, "p_uv_ab", n, &[f(eps), brightness as u32, 0, 1], [Some(&mean), Some(&mt), Some(&cov), Some(&corr), b.as_ref()], n * 2);
    let aa = deriche(cx, &aa, dw, dh, 4, gs, None, 1e9);
    let bb = deriche(cx, &bb, dw, dh, 2, gs, None, 1e9);
    let aa = interpolate(cx, &aa, (dw, dh), (w, h), 4);
    let bb = interpolate(cx, &bb, (dw, dh), (w, h), 2);
    run(cx, "p_uv_apply", w * h, &[], [Some(uv), Some(&aa), Some(&bb), None, None], w * h * 2)
}
fn equalizer(cx: &mut Cx<'_>, img: &Buf, s: &DevelopSettings, w: usize, h: usize, ppl: f64, cache: &mut Cache, ik: u64, scope: &str) -> Buf {
    let n = w * h;
    let scale = (ppl as f32 / 6000.).max(1e-3);
    let tab = lut(cx, s);
    let uv = cache.get(format!("{scope}/equal-raw-uv"), ik, || one(cx, "p_equal_pre", n, &[2], img, n * 2));
    let l = cache.get(format!("{scope}/equal-l"), ik, || one(cx, "p_equal_pre", n, &[1], img, n));
    let sat = cache.get(format!("{scope}/equal-sat"), ik, || {
        let p = one(cx, "p_equal_pre", n, &[0], img, n);
        deriche(cx, &p, w, h, 1, scale.max(0.5), None, 1e9)
    });
    let uv_slot = format!("{scope}/equal-uv");
    let uv = if let Some(b) = cache.find(&uv_slot, ik) {
        b
    } else {
        let pre = uv_guided_cached(cx, &uv, &uv, w, h, (0.75 * scale).max(0.2), 1e-5, false, cache, ik, &format!("{scope}/pre"));
        let out = run(cx, "p_equal_uv", n, &[], [Some(&uv), Some(&pre), Some(&sat), None, Some(&tab)], n * 2);
        cache.put(uv_slot, ik, out)
    };
    let hsb = cache.get(format!("{scope}/equal-hsb"), ik, || run(cx, "p_equal_hsb", n, &[], [Some(&uv), Some(&l), None, None, None], n * 4));
    let max_b = s.mixer.bands().iter().map(|b| (b.lum as f32 / 200.).abs()).fold(0., f32::max);
    let corr = run(cx, "p_equal_corr", n, &[0, w as u32, h as u32, f(max_b), f(scale)], [Some(&hsb), Some(&sat), None, None, Some(&tab)], n * 2);
    let dh = run(cx, "p_equal_corr", n, &[1, w as u32, h as u32, f(max_b), f(scale)], [Some(&hsb), Some(&sat), None, None, Some(&tab)], n);
    let grad = run(cx, "p_equal_corr", n, &[2, w as u32, h as u32, f(max_b), f(scale)], [Some(&hsb), Some(&sat), None, None, Some(&tab)], n);
    let grad = deriche(cx, &grad, w, h, 1, scale.max(0.5), None, 1e9);
    let corr = uv_guided_cached(cx, &uv, &corr, w, h, (0.5 * scale).max(0.2), 1e-6, true, cache, ik, &format!("{scope}/correction"));
    let extra = cx.gpu.buffer(n * 2);
    cx.copy_into(&dh, &extra, 0);
    cx.copy_into(&grad, &extra, n);
    run(
        cx,
        "p_equal_finish",
        n,
        &[f(max_b), lightcraft_pipeline::is_bw(s) as u32],
        [Some(&hsb), Some(&corr), Some(&sat), Some(&extra), Some(&tab)],
        n * 3,
    )
}
fn balance(cx: &mut Cx<'_>, img: &Buf, s: &DevelopSettings, n: usize, gamut: &Buf) -> Buf {
    let k = lightcraft_pipeline::balance::Balance::new(s);
    let mut p = vec![k.vibrance, k.chroma, k.saturation];
    p.extend(k.chroma_masks);
    p.extend(k.saturation_masks);
    p.extend(k.brilliance_masks);
    p.extend([k.brilliance, k.hue_angle]);
    p.extend(k.global);
    p.extend(k.shadows);
    p.extend(k.highlights);
    p.extend(k.midtones);
    // two reserved words keep the fixed parameter offsets explicit.
    p.extend([k.midtones_y, k.white, k.grey, k.contrast, 0., 0., k.shadows_weight, k.highlights_weight, k.midtones_weight, k.mask_grey]);
    run(cx, "p_balance", n, &p.into_iter().map(f).collect::<Vec<_>>(), [Some(img), None, None, None, Some(gamut)], n * 3)
}
fn skin(cx: &mut Cx<'_>, img: &Buf, s: &DevelopSettings, w: usize, h: usize, ppl: f64, cache: &mut Cache, ik: u64, scope: &str, gamut: &Buf) -> Buf {
    let n = w * h;
    let pre = cache.get(format!("{scope}/skin-pre"), ik, || one(cx, "p_skin_pre", n, &[0], img, n * 4));
    let extra = cache.get(format!("{scope}/skin-extra"), ik, || one(cx, "p_skin_pre", n, &[1], img, n * 2));
    let low = cache.get(format!("{scope}/skin-low"), ik, || {
        let guide = extract(cx, &extra, n, 2, 1, 0);
        let sigma = (ppl as f32 * 0.0075).max(1.);
        let moments = one(cx, "p_moments", n, &[2], &guide, n * 2);
        let means = deriche(cx, &moments, w, h, 2, sigma, None, f32::MAX);
        let all = cx.gpu.buffer(n * 3);
        for ch in 0..3 {
            let moments = run(cx, "p_skin_moments", n, &[ch as u32], [Some(&pre), Some(&guide), None, None, None], n * 2);
            let mean = deriche(cx, &moments, w, h, 2, sigma, None, f32::MAX);
            let p = run(cx, "p_skin_low", n, &[], [Some(&guide), Some(&means), Some(&mean), None, None], n);
            cx.copy_into(&p, &all, ch * n);
        }
        all
    });
    let sk = &s.skin_tone;
    let r = sk.reference.unwrap_or_default();
    let p = [
        f(r[0] as f32),
        f(r[1] as f32),
        f((r[2] as f32).to_radians()),
        f(sk.uniformity as f32 / 100.),
        f(sk.lightness as f32 / 100.),
        f((sk.hue_range as f32).to_radians()),
        f(0.04 + sk.chroma_range as f32 / 100. * 0.4),
        f(0.05 + sk.lightness_range as f32 / 100. * 0.5),
        sk.protect_lips as u32,
    ];
    run(cx, "p_skin_finish", n, &p, [Some(img), Some(&pre), Some(&low), Some(&extra), Some(gamut)], n * 3)
}

#[allow(clippy::too_many_arguments)]
fn tone_ab(
    cx: &mut Cx<'_>,
    pl: &Arc<Buf>,
    w: usize,
    h: usize,
    sl: [f32; 4],
    method: HsMethod,
    cache: &mut Cache,
    pk: u64,
    scope: &str,
    name: &str,
) -> Arc<Buf> {
    let slot = format!("{scope}/tone-ab/{name}");
    let fk = key((pk, method, sl.map(f)));
    if let Some(b) = cache.find(&slot, fk) {
        return b;
    }
    if sl == [0.; 4] {
        let zero = cx.zeroed(w * h * 2);
        return cache.put(slot, fk, zero);
    }
    let field = match method {
        HsMethod::LiTone => li_tone(cx, pl, w, h, sl, cache, pk, scope),
        HsMethod::Eigf => {
            let lum = cache.get(format!("{scope}/eigf-lum"), pk, || one(cx, "p_lum", w * h, &[], pl, w * h));
            let filtered: Vec<_> = [0.005, 0.015, 0.045]
                .into_iter()
                .enumerate()
                .map(|(i, frac)| {
                    cache.get(format!("{scope}/eigf-base/{i}"), pk, || eigf(cx, &lum, w, h, (w.max(h) as f32 * frac).max(1.), 0.08, 3, 0.5))
                })
                .collect();
            run(cx, "p_eigf_gain", w * h, &sl.map(f), [Some(&filtered[0]), Some(&filtered[1]), Some(&filtered[2]), None, None], w * h)
        }
    };
    let ab = gain_ab(cx, pl, &field, w, h);
    cache.put(slot, fk, ab)
}

#[allow(clippy::too_many_arguments)]
fn tone_detail(
    cx: &mut Cx<'_>,
    mut img: Arc<Buf>,
    s: &DevelopSettings,
    plan: &Plan<'_>,
    info: &SourceInfo,
    proxy: Option<(&Arc<Buf>, usize, usize)>,
    clip: Option<&Buf>,
    method: HsMethod,
    cache: &mut Cache,
    mut ik: u64,
    pk: u64,
    scope: &str,
) -> (Arc<Buf>, u64) {
    let flags = Stages::of(s);
    if !flags.tone && !flags.clarity && !flags.texture && !flags.structure {
        return (img, ik);
    }
    let (w, h, n) = (plan.w, plan.h, plan.w * plan.h);
    // The full guide is captured before H/S, just as in the CPU reference.
    let l = cache.get(format!("{scope}/log"), ik, || one(cx, "p_log", n, &[], &img, n));
    let pl = proxy.map(|(p, pw, ph)| {
        let l = cache.get("proxy-log", pk, || one(cx, "p_log", pw * ph, &[], p, pw * ph));
        (l, pw, ph)
    });
    let sliders = [s.light.highlights, s.light.shadows, s.light.whites, s.light.blacks].map(|x| x as f32 / 100.);
    if flags.tone
        && let Some((pl, pw, ph)) = &pl
    {
        let tone_key = key((pk, method, sliders.map(f)));
        let ab = tone_ab(cx, pl, *pw, *ph, sliders, method, cache, pk, scope, "main");
        let gain = up_gain(cx, &l, &ab, (w, h), (*pw, *ph));
        let nohl = (sliders[0] < 0. && clip.is_some()).then(|| {
            let mut sl = sliders;
            sl[0] = 0.;
            let ab = tone_ab(cx, pl, *pw, *ph, sl, method, cache, pk, scope, "nohl");
            up_gain(cx, &l, &ab, (w, h), (*pw, *ph))
        });
        let outkey = key((ik, tone_key, info.clip_confidence.as_ref().map(|c| Arc::as_ptr(c) as usize)));
        img = cache.get(format!("{scope}/tone-image"), outkey, || {
            run(cx, "p_gain_apply", n, &[nohl.is_some() as u32], [Some(&img), Some(&gain), nohl.as_ref(), clip, None], n * 3)
        });
        ik = outkey;
    }
    if !flags.clarity && !flags.texture && !flags.structure {
        return (img, ik);
    }
    let cl = if flags.clarity
        && let Some((pl, pw, ph)) = &pl
    {
        let ck = key((pk, plan.px_per_long.to_bits(), w));
        let ab = cache.get(format!("{scope}/clarity-ab"), ck, || {
            let detail = clarity(cx, pl, *pw, *ph, plan.px_per_long, w);
            gain_ab(cx, pl, &detail, *pw, *ph)
        });
        Some(up_gain(cx, &l, &ab, (w, h), (*pw, *ph)))
    } else {
        None
    };
    let scale = lightcraft_pipeline::detail::out_per_orig(plan.px_per_long, plan.src_long, info.sensor_scale);
    let band_key = key((ik, f(scale)));
    let bands = if flags.texture || flags.structure {
        let biased = cache.get(format!("{scope}/biased"), ik, || one(cx, "p_biased", n, &[], &img, n));
        let mut filtered = |radius: f32, cx: &mut Cx<'_>| {
            cache.get(format!("{scope}/band/{}", radius as u32), band_key, || {
                let sigma = (radius * scale).max(0.01);
                if sigma < 1. { cross(cx, &biased, &biased, w, h, sigma, 0.15) } else { eigf(cx, &biased, w, h, sigma, 0.15, 2, 0.) }
            })
        };
        let middle = filtered(4., cx);
        let all = cx.gpu.buffer(n * 2);
        if flags.texture {
            let wide = filtered(16., cx);
            let band = run(cx, "p_band", n, &[], [Some(&middle), Some(&wide), None, None, None], n);
            cx.copy_into(&band, &all, 0);
        }
        if flags.structure {
            let fine = filtered(2., cx);
            let band = run(cx, "p_band", n, &[], [Some(&fine), Some(&middle), None, None, None], n);
            cx.copy_into(&band, &all, n);
        }
        Some(all)
    } else {
        None
    };
    let mode = match s.effects.clarity_mode {
        ClarityMode::Natural => 0,
        ClarityMode::Punch => 1,
        ClarityMode::Neutral => 2,
    };
    let p = [
        f(s.effects.clarity as f32 / 100.),
        f(s.effects.texture as f32 / 100.),
        f(s.effects.structure as f32 / 100.),
        flags.clarity as u32,
        flags.texture as u32,
        flags.structure as u32,
        mode,
    ];
    let outkey = key((ik, p));
    let gamut = cache.get("gamut", 0, || cx.gpu.upload(&ucs::gamut_lut().map(f)));
    let out = cache.get(format!("{scope}/detail-image"), outkey, || {
        run(cx, "p_detail_apply", n, &p, [Some(&img), cl.as_ref(), bands.as_ref(), Some(&l), Some(&gamut)], n * 3)
    });
    (out, outkey)
}
#[allow(clippy::too_many_arguments)]
fn colour(cx: &mut Cx<'_>, mut img: Arc<Buf>, s: &DevelopSettings, plan: &Plan<'_>, cache: &mut Cache, mut ik: u64, scope: &str) -> (Arc<Buf>, u64) {
    let flags = Stages::of(s);
    let n = plan.w * plan.h;
    if flags.equalizer {
        let ok = key((ik, format!("{:?}", (&s.mixer, &s.bw_mix, lightcraft_pipeline::is_bw(s)))));
        // Intermediate UV/HSB fields are keyed only by their input, not mixer slider values.
        let slot = format!("{scope}/equal-image");
        img = if let Some(out) = cache.find(&slot, ok) {
            out
        } else {
            let out = equalizer(cx, &img, s, plan.w, plan.h, plan.px_per_long, cache, ik, scope);
            cache.put(slot, ok, out)
        };
        ik = ok;
    }
    if flags.balance || flags.skin {
        let gamut = cache.get("gamut", 0, || cx.gpu.upload(&ucs::gamut_lut().map(f)));
        if flags.balance {
            let ok = key((ik, format!("{:?}", (&s.color, &s.grading))));
            img = cache.get(format!("{scope}/balance-image"), ok, || balance(cx, &img, s, n, &gamut));
            ik = ok;
        }
        if flags.skin {
            let ok = key((ik, format!("{:?}", s.skin_tone)));
            let out = skin(cx, &img, s, plan.w, plan.h, plan.px_per_long, cache, ik, scope, &gamut);
            img = Arc::new(out);
            ik = ok;
        }
    }
    (img, ik)
}

/// All primary tools, including layer tools, stay on the device from WB to finish.
#[allow(clippy::too_many_arguments)]
pub(crate) fn process(
    cx: &mut Cx<'_>,
    lin: Arc<Buf>,
    proxy: Option<(Arc<Buf>, usize, usize)>,
    clip: Option<&Buf>,
    alpha: Option<&Buf>,
    info: &SourceInfo,
    plan: &Plan<'_>,
    method: HsMethod,
    cache: &mut Cache,
) -> Arc<Buf> {
    let s = &*plan.settings;
    let n = plan.w * plan.h;
    let gain = (s.light.exposure as f32).exp2();
    let base_key = key((plan.lin_key, f(gain)));
    // Selection masks are evaluated on pre-primary pixels using the photo tone map.
    // A contrast/look change therefore invalidates later layer fields even when their
    // component geometry and primary sliders have not changed.
    let alpha_key = key((
        base_key,
        s.light.contrast.to_bits(),
        format!(
            "{:?}",
            (s.look, &s.look_options, lightcraft_pipeline::negative::converts(s), info.raw, info.camera_tone, info.look_curve, info.profile_curve)
        ),
    ));
    let mut img = if gain == 1. { lin } else { cache.get("exposed", base_key, || one(cx, "p_scale", n, &[f(gain)], &lin, n * 3)) };
    let proxy = proxy.map(|(b, w, h)| {
        let exposed = if gain == 1. { b } else { cache.get("proxy-exposed", base_key, || one(cx, "p_scale", w * h, &[f(gain)], &b, w * h * 3)) };
        (exposed, w, h)
    });
    let pp = proxy.as_ref().map(|(p, w, h)| (p, *w, *h));
    let (next, ik) = tone_detail(cx, img, s, plan, info, pp, clip, method, cache, base_key, base_key, "global");
    img = next;
    let (next, mut ik) = colour(cx, img, s, plan, cache, ik, "global");
    img = next;
    for (mi, m) in s.masks.iter().filter(|m| m.visible && !m.components.is_empty()).enumerate() {
        let scope = format!("layer/{mi}");
        let mut d = m.tools.view(lightcraft_pipeline::local::effective_wb(info, s));
        d.light.highlights += m.adjust.highlights;
        d.light.shadows += m.adjust.shadows;
        d.effects.texture += m.adjust.texture;
        d.effects.clarity += m.adjust.clarity;
        d.disabled_sections = s.disabled_sections.clone();
        let from = lightcraft_pipeline::local::effective_wb(info, s);
        let to = m.tools.wb.map_or((from.0 * (m.adjust.temp / 100. * 0.6).exp(), from.1 + m.adjust.tint * 0.4), |w| (w.temp, w.tint));
        let wb =
            (m.tools.wb.is_some() || m.adjust.temp != 0. || m.adjust.tint != 0.).then(|| lightcraft_pipeline::local::wb_change(from, to)).flatten();
        let flags = Stages::of(&d);
        let mut q = img.clone();
        let mut qkey = ik;
        if let Some(mat) = wb {
            let mut p = vec![1];
            p.extend(mat.iter().flatten().copied().map(f));
            let out = cx.gpu.buffer(n * 3);
            crate::render::map(cx, "wb_k", n, &p, [Some(&q), None, None], &out);
            q = Arc::new(out);
            qkey = key((ik, mat.map(|r| r.map(f))));
        }
        if wb.is_some() || flags.tone || flags.clarity || flags.texture || flags.structure {
            let (q, k) = tone_detail(cx, q, &d, plan, info, pp, clip, method, cache, qkey, base_key, &scope);
            if let Some(alpha) = alpha {
                img = Arc::new(run(cx, "p_mix", n, &[(mi * n) as u32], [Some(&img), Some(&q), Some(alpha), None, None], n * 3));
                ik = key((
                    ik,
                    k,
                    alpha_key,
                    format!("{:?}", (&m.components, m.invert, m.refine)),
                    lightcraft_pipeline::masks::mask_scale(m).to_bits(),
                ));
            }
        }
        if flags.equalizer || flags.balance || flags.skin {
            let (q, k) = colour(cx, img.clone(), &d, plan, cache, ik, &scope);
            if let Some(alpha) = alpha {
                img = Arc::new(run(cx, "p_mix", n, &[(mi * n) as u32], [Some(&img), Some(&q), Some(alpha), None, None], n * 3));
                ik = key((
                    ik,
                    k,
                    alpha_key,
                    format!("{:?}", (&m.components, m.invert, m.refine)),
                    lightcraft_pipeline::masks::mask_scale(m).to_bits(),
                ));
            }
        }
    }
    if gain == 1. { img } else { Arc::new(one(cx, "p_scale", n, &[f(1. / gain)], &img, n * 3)) }
}

pub(crate) fn clip_plane(cx: &mut Cx<'_>, rgb: &Buf, n: usize) -> Buf {
    one(cx, "p_clip", n, &[], rgb, n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_pipeline::{RenderRequest, primary};
    use lightcraft_raster::{Plane, Rgb32f};
    fn device() -> Option<&'static crate::ctx::Gpu> {
        static DEVICE: std::sync::OnceLock<Result<crate::ctx::Gpu, String>> = std::sync::OnceLock::new();
        match DEVICE.get_or_init(crate::ctx::Gpu::numerical_test_device) {
            Ok(g) => Some(g),
            Err(e) => {
                eprintln!("skipped: no CPU Vulkan numerical-test adapter ({e})");
                None
            }
        }
    }
    fn error(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(a, b)| (a - b).abs()).fold(0., f32::max)
    }
    #[test]
    fn native_filter_and_pyramid_numerics() {
        let Some(g) = device() else { return };
        let _scope = crate::ctx::RenderScope::new(g);
        let mut cx = Cx::new(g);
        let (w, h) = (37, 29);
        let p = Plane::from_fn(w, h, |x, y| {
            0.002 + 0.013 * x as f32 + 0.0007 * y as f32 + if x > 18 { 0.7 } else { 0. } + 0.002 * (x as f32 * 0.7 + y as f32 * 0.3).sin()
        });
        let b = g.upload(&p.data.iter().copied().map(f).collect::<Vec<_>>());
        for sigma in [0.2, 1.7, 6.3] {
            let out = deriche(&mut cx, &b, w, h, 1, sigma, None, f32::MAX);
            let out: Vec<f32> = cx.read(&out, w * h);
            let cpu = lightcraft_pipeline::eigf::mean(&p.data, w, h, 1, sigma);
            assert!(error(&out, &cpu) < 3e-6, "Deriche {sigma}: {}", error(&out, &cpu));
        }
        for quant in [0., 0.5] {
            let out = eigf(&mut cx, &b, w, h, 6.3, 0.08, 3, quant);
            let out: Vec<f32> = cx.read(&out, w * h);
            let mut par = lightcraft_pipeline::eigf::Params::new(6.3, 0.08);
            par.iterations = 3;
            par.quantization = quant;
            let cpu = lightcraft_pipeline::eigf::filter(&p, par);
            assert!(error(&out, &cpu.data) < 3e-6, "EIGF {quant}: {}", error(&out, &cpu.data));
        }
        let log = p.map(|v| (v / 0.18).log2());
        let lb = Arc::new(g.upload(&log.data.iter().copied().map(f).collect::<Vec<_>>()));
        let sl = [-1., 1., 0.3, -0.2];
        let out = li_tone(&mut cx, &lb, w, h, sl, &mut Cache::default(), 0, "test");
        let out: Vec<f32> = cx.read(&out, w * h);
        let cpu = primary::li_tone(&log, sl[0], sl[1], sl[2], sl[3]);
        assert!(error(&out, &cpu.data) < 3e-5, "LI Tone {}", error(&out, &cpu.data));
        let mut cache = Cache::default();
        let first = li_tone(&mut cx, &lb, w, h, sl, &mut cache, 0, "drag");
        drop(first);
        let before = cx.primary_dispatches();
        let second = li_tone(&mut cx, &lb, w, h, [-0.3, 0.4, -0.5, 0.6], &mut cache, 0, "drag");
        assert_eq!(cx.primary_dispatches() - before, 1, "a cached LI Tone drag only combines its four response fields");
        let second: Vec<f32> = cx.read(&second, w * h);
        let cpu = primary::li_tone(&log, -0.3, 0.4, -0.5, 0.6);
        assert!(error(&second, &cpu.data) < 3e-5);
        let uv: Vec<f32> = (0..w * h)
            .flat_map(|i| [0.03 * (0.13 * (i % w) as f32).sin() + if i % w > 18 { 0.01 } else { 0. }, 0.04 * (0.17 * (i / w) as f32).cos()])
            .collect();
        let ub = g.upload(&uv.iter().copied().map(f).collect::<Vec<_>>());
        let out = uv_guided(&mut cx, &ub, &ub, w, h, 7.3, 1e-5, false);
        let out: Vec<f32> = cx.read(&out, w * h * 2);
        let cpu = lightcraft_pipeline::colorequal::guided_uv(&uv, &uv, w, h, 7.3, 1e-5, false);
        assert!(error(&out, &cpu) < 5e-6, "UV guided {}", error(&out, &cpu));
    }
    #[test]
    fn native_primary_render_matches_unchanged_cpu() {
        let Some(g) = device() else { return };
        let _scope = crate::ctx::RenderScope::new(g);
        let src = Arc::new(Rgb32f::from_fn(53, 37, |x, y| {
            let t = (x as f32 / 52. * 8. - 6.).exp2();
            [t * (1. + 0.1 * (y as f32).sin()), t * 0.6, t * 0.3]
        }));
        let info = SourceInfo::default();
        let req = RenderRequest::fit(53, 37);
        let stages = crate::render::GpuStages::default();
        let mut cases = vec![("default", DevelopSettings::default())];
        let mut s = DevelopSettings::default();
        s.light.highlights = -70.;
        s.light.shadows = 50.;
        cases.push(("tone", s));
        for mode in [ClarityMode::Natural, ClarityMode::Punch, ClarityMode::Neutral] {
            let mut s = DevelopSettings::default();
            s.effects.clarity = 50.;
            s.effects.texture = -30.;
            s.effects.structure = 40.;
            s.effects.clarity_mode = mode;
            cases.push(("detail", s));
        }
        let mut s = DevelopSettings::default();
        s.color.vibrance = 70.;
        s.color.saturation = 40.;
        s.grading.shadows = lightcraft_develop::Wheel { hue: 210., sat: 30., lum: 15. };
        cases.push(("balance", s));
        let mut s = DevelopSettings::default();
        s.mixer.orange.hue = 30.;
        s.mixer.blue.sat = -60.;
        s.mixer.orange.lum = 20.;
        cases.push(("equalizer", s.clone()));
        s.treatment = lightcraft_develop::Treatment::Bw;
        cases.push(("BW", s));
        let mut s = DevelopSettings::default();
        s.skin_tone.reference = Some([0.5, 0.13, 45.]);
        s.skin_tone.uniformity = 80.;
        s.skin_tone.lightness = 25.;
        cases.push(("skin", s));
        for method in [HsMethod::LiTone, HsMethod::Eigf] {
            for (name, s) in &cases {
                let cpu = lightcraft_pipeline::render_hs_candidate(&src, &info, s, &req, method);
                let out = crate::render::render(g, &src, &info, s, &req, Some(&stages), None, method);
                assert!(out.is_some(), "native render {name}");
                let Some(out) = out else { return };
                let differences: Vec<_> =
                    out.image.data.iter().zip(&cpu.image.data).flat_map(|(a, b)| (0..3).map(move |c| a[c].abs_diff(b[c]) as f32)).collect();
                let max = differences.iter().copied().fold(0., f32::max);
                let mean = differences.iter().sum::<f32>() / differences.len() as f32;
                eprintln!("{name} {method:?}: max {max}, mean {mean}");
                if *name == "default" {
                    assert_eq!(stages.primary_dispatches(), 0);
                }
                assert!(max <= 3. && mean < 0.5, "{name} {method:?}: max {max}, mean {mean}");
            }
        }
    }
    #[test]
    fn native_capture_lens_and_primary_caches_match_cpu() {
        use lightcraft_pipeline::lensdb::{Distortion, LensCorrection, Tca};
        let Some(g) = device() else { return };
        let _scope = crate::ctx::RenderScope::new(g);
        let errors = crate::ctx::ErrorScopes::push(g);
        let src = Arc::new(lightcraft_scenes::demo_library()[2].render(137, 89));
        let info = SourceInfo {
            raw: true,
            capture_radius: Some(0.8),
            lens_db: Some(LensCorrection {
                distortion: Distortion::Poly3 { k1: -0.2 },
                tca: Tca::Linear { kr: 1.01, kb: 0.99 },
                vignetting: Some([-0.3, 0.05, 0.]),
                diag_norm: 2.,
                center: [0.1, -0.15],
            }),
            ..Default::default()
        };
        let mut s = DevelopSettings::default();
        s.raw.capture.enabled = true;
        s.raw.capture.radius = 0.8;
        s.raw.capture.threshold = 25.;
        s.raw.capture.iterations = 2.;
        s.lens_db.enabled = true;
        s.light.highlights = -60.;
        s.light.shadows = 35.;
        s.effects.clarity = 20.;
        s.effects.texture = 15.;
        s.effects.structure = 25.;
        s.color.vibrance = 20.;
        s.mixer.orange.lum = 10.;
        let mut req = RenderRequest::fit(137, 89);
        let cache = crate::render::GpuStages::default();
        for step in 0..8 {
            match step {
                1 => s.light.highlights = -40.,
                2 => s.raw.capture.radius = 1.2,
                3 => s.lens_db.distortion = 200.,
                4 => req = RenderRequest::fit(73, 59),
                5 => s.raw.capture.enabled = false,
                6 => s.raw.capture.enabled = true,
                7 => s.red_eye.push(lightcraft_develop::RedEye {
                    center: lightcraft_geom::Point::new(0.55, 0.48),
                    rx: 0.1,
                    ry: 0.1,
                    ..Default::default()
                }),
                _ => {}
            }
            let method = if step == 6 { HsMethod::Eigf } else { HsMethod::LiTone };
            let cached = check_render(g, &src, &info, &s, &req, method, &cache);
            let fresh = check_render(g, &src, &info, &s, &req, method, &crate::render::GpuStages::default());
            assert_eq!(cached, fresh, "capture/lens/primary cache edit {step}");
        }
        assert!(errors.pop().is_none());
        assert!(!crate::ctx::failed());
    }

    fn check_render(
        g: &crate::ctx::Gpu,
        src: &Arc<Rgb32f>,
        info: &SourceInfo,
        s: &DevelopSettings,
        req: &RenderRequest,
        method: HsMethod,
        cache: &crate::render::GpuStages,
    ) -> lightcraft_raster::Rgba8 {
        let cpu = lightcraft_pipeline::render_hs_candidate(src, info, s, req, method);
        let out = crate::render::render(g, src, info, s, req, Some(cache), None, method);
        assert!(out.is_some());
        let Some(out) = out else { return cpu.image };
        let mut max = 0u8;
        let mut sum = 0usize;
        for (a, b) in out.image.data.iter().zip(&cpu.image.data) {
            for c in 0..3 {
                let d = a[c].abs_diff(b[c]);
                max = max.max(d);
                sum += d as usize;
            }
        }
        let mean = sum as f32 / (cpu.image.len() * 3) as f32;
        assert!(max <= 3 && mean < 0.5, "{method:?} {}x{}: max {max}, mean {mean}", req.max_w, req.max_h);
        out.image
    }
    #[test]
    fn native_preview_clipping_layers_and_cache_edits() {
        use lightcraft_develop::{Effects, LayerTools, Mask, MaskComponent, MaskOp, MaskShape};
        use lightcraft_geom::Point;
        let Some(g) = device() else { return };
        let _scope = crate::ctx::RenderScope::new(g);
        let src = Arc::new(Rgb32f::from_fn(311, 207, |x, y| {
            let l = (x as f32 / 310. * 10. - 7.).exp2();
            [l, l * (0.6 + 0.1 * (y as f32 * 0.3).cos()), l * 0.3]
        }));
        let info = SourceInfo {
            raw: true,
            raw_clip_level: Some(0.99),
            clip_confidence: Some(Arc::new(lightcraft_pipeline::ClipConfidence {
                width: 31,
                height: 21,
                data: (0..31 * 21).map(|i| if i % 31 > 26 { 1. } else { 0. }).collect(),
            })),
            ..Default::default()
        };
        let req = RenderRequest::fit(193, 127);
        let stages = crate::render::GpuStages::default();
        let mut s = DevelopSettings::default();
        s.light.exposure = 0.3;
        s.light.highlights = -80.;
        s.light.shadows = 65.;
        s.light.whites = 40.;
        s.light.blacks = -20.;
        s.effects.clarity = 35.;
        for method in [HsMethod::LiTone, HsMethod::Eigf, HsMethod::LiTone] {
            check_render(g, &src, &info, &s, &req, method, &stages);
        }
        let map = lightcraft_pipeline::tone2::tone_map(&s, &info, s.light.contrast, 0., 0.);
        let c = src.get(150, 100).map(|v| v * (s.light.exposure as f32).exp2());
        let lab = lightcraft_color::perceptual::oklab_from_2020(lightcraft_pipeline::tone2::tone_px(&map, &map.method(), c));
        s.masks.push(Mask {
            refine: 40.,
            opacity: 65.,
            adjust: lightcraft_develop::LocalAdjustments { temp: 15., tint: -20., amount: 150., ..Default::default() },
            components: vec![MaskComponent {
                name: None,
                op: MaskOp::Add,
                invert: false,
                shape: MaskShape::Linear { start: Point::new(0., 0.), end: Point::new(1., 0.) },
            }],
            tools: LayerTools {
                effects: Some(Effects { clarity: -20., structure: 50., ..Default::default() }),
                color: Some(lightcraft_develop::ColorAdj { vibrance: 30., ..Default::default() }),
                ..Default::default()
            },
            ..Default::default()
        });
        s.masks.push(Mask {
            tools: LayerTools { effects: Some(Effects { texture: 35., ..Default::default() }), ..Default::default() },
            components: s.masks[0].components.clone(),
            ..Default::default()
        });
        s.masks[0].components.push(MaskComponent {
            name: None,
            op: MaskOp::Intersect,
            invert: false,
            shape: MaskShape::ColorRange { samples: vec![lab.map(f64::from)], refine: 80. },
        });
        for edit in 0..4 {
            s.light.contrast = edit as f64 * 15.;
            s.masks[0].invert = edit % 2 == 0;
            s.masks[0].refine = edit as f64 * 20.;
            s.light.highlights = -20. - edit as f64 * 20.;
            let warm = check_render(g, &src, &info, &s, &req, HsMethod::LiTone, &stages);
            let fresh = check_render(g, &src, &info, &s, &req, HsMethod::LiTone, &crate::render::GpuStages::default());
            assert_eq!(warm, fresh);
        }
    }
    #[test]
    fn native_skin_changes_skin_and_zero_controls_dispatch_nothing() {
        let Some(g) = device() else { return };
        let _scope = crate::ctx::RenderScope::new(g);
        let lw = ucs::y_to_l_star(1.);
        let src = Arc::new(Rgb32f::from_fn(91, 65, |x, y| {
            let j = [
                0.5 + 0.02 * (y as f32 * 0.07).sin(),
                0.045 + 0.008 * (x as f32 * 0.1).sin() + 0.0005 * (x as f32 * 1.8).sin(),
                0.55 + 0.1 * (x as f32 * 0.03).cos(),
            ];
            ucs::mul(&ucs::mats().xyz_to_rgb, ucs::xyy_to_xyz(ucs::jch_to_xyy(j, lw)))
        }));
        let info = SourceInfo::default();
        let req = RenderRequest::fit(91, 65);
        let stages = crate::render::GpuStages::default();
        let mut s = DevelopSettings::default();
        s.skin_tone.reference = Some([0.5, 0.045, 0.55f64.to_degrees()]);
        s.skin_tone.uniformity = 80.;
        s.skin_tone.lightness = 40.;
        let edited = check_render(g, &src, &info, &s, &req, HsMethod::LiTone, &stages);
        assert!(stages.primary_dispatches() > 0);
        s.skin_tone.uniformity = 0.;
        s.skin_tone.lightness = 0.;
        s.effects.clarity_mode = ClarityMode::Punch;
        s.grading.shadows.hue = 200.;
        let identity = check_render(g, &src, &info, &s, &req, HsMethod::LiTone, &stages);
        assert_ne!(edited, identity);
        assert_eq!(stages.primary_dispatches(), 0);
        let setters: [fn(&mut DevelopSettings, f64); 13] = [
            |s, v| s.light.highlights = v,
            |s, v| s.light.shadows = v,
            |s, v| s.light.whites = v,
            |s, v| s.light.blacks = v,
            |s, v| s.effects.clarity = v,
            |s, v| s.effects.texture = v,
            |s, v| s.effects.structure = v,
            |s, v| s.color.vibrance = v,
            |s, v| s.color.saturation = v,
            |s, v| s.mixer.orange.hue = v,
            |s, v| s.grading.shadows.sat = v,
            |s, v| s.skin_tone.uniformity = v,
            |s, v| s.skin_tone.lightness = v,
        ];
        for set in setters {
            set(&mut s, 20.);
            check_render(g, &src, &info, &s, &req, HsMethod::LiTone, &stages);
            assert!(stages.primary_dispatches() > 0);
            set(&mut s, 0.);
            assert_eq!(check_render(g, &src, &info, &s, &req, HsMethod::LiTone, &stages), identity);
            assert_eq!(stages.primary_dispatches(), 0);
        }
    }
    #[test]
    fn native_colour_and_skin_fit_four_channel_bindings() {
        let Some(g) = device() else { return };
        let _scope = crate::ctx::RenderScope::new(g);
        let _limit = crate::ctx::LimitOverride::new(100_000);
        let src = Arc::new(Rgb32f::from_fn(91, 65, |x, y| [0.12 + x as f32 * 0.001, 0.07 + y as f32 * 0.001, 0.04]));
        let mut s = DevelopSettings::default();
        s.mixer.orange.hue = 35.;
        s.mixer.orange.lum = 20.;
        s.skin_tone.reference = Some([0.5, 0.045, 32.]);
        s.skin_tone.uniformity = 70.;
        check_render(g, &src, &SourceInfo::default(), &s, &RenderRequest::fit(91, 65), HsMethod::LiTone, &crate::render::GpuStages::default());
        assert!(crate::ctx::take_failure().is_none(), "primary intermediates must fit the four-channel binding limit");
    }
}
