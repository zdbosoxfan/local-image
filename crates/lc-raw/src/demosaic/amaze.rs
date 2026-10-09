//! AMaZE (Aliasing Minimization and Zipper Elimination), complete kernel with parallel tile bands.
//! Port of darktable `src/iop/demosaicing/amaze.cc` at
//! 733bd69f32cac7ff5e41025115942772add1f088, cross-checked with RawTherapee
//! 5f486d3678b34c74ba0c63571c17babe20935019. Copyright (c) 2008-2010 Emil
//! Martinec; speed optimizations by Ingo Weyrich; (C) 2011-2024 darktable
//! developers. GPL-3.0-or-later. See docs/PORTS.md and the project notices.
//! A single safe indexed scratch buffer retains upstream’s overlapping plane lifetimes.
//! In particular, boundary values must survive when vcdalt becomes Dgrb.
#![allow(unused_parens, unused_mut, non_snake_case)]
use super::Mosaic;
use crate::Rgb32f;
use rayon::prelude::*;
// Float offsets reproduce upstream's 128-byte cache padding and shared lifetimes.
// DELHV/PMWT, VCD/RBM, CDDIFF/DELP, DELM/RBINT, VCD_ALT/Dgrb and DGINTV/Dgrb2
// are the same storage. Nyquist flags use separate byte-equivalent integer arrays.
const GREEN: usize = 0;
const DELHV: usize = 25632;
const DIR0: usize = 51264;
const DIR1: usize = 76896;
const VCD: usize = 102528;
const RBP: usize = 115360;
const HCD: usize = 128160;
const VCD_ALT: usize = 153792;
const HCD_ALT: usize = 179424;
const CDDIFF: usize = 205056;
const DELM: usize = 217888;
const HVWT: usize = 230720;
const DGINTV: usize = 243552;
const DGINTH: usize = 269184;
const DGRB_SQ_M: usize = 294816;
const DGRB_SQ_P: usize = 307648;
const CFA: usize = 320480;
const NYQUIST_TEST: usize = 349344;
#[inline]
fn fc(m: &Mosaic, y: i32, x: i32) -> i32 {
    m.cfa.color_at((x & 1) as usize, (y & 1) as usize) as i32
}
#[inline]
fn interpolatef(a: f32, b: f32, c: f32) -> f32 {
    a * (b - c) + c
}
#[inline]
fn square(v: f32) -> f32 {
    v * v
}
#[inline]
fn lim(x: f32, a: f32, b: f32) -> f32 {
    a.max(x.min(b))
}
#[inline]
fn ulim(x: f32, a: f32, b: f32) -> f32 {
    lim(x, a.min(b), a.max(b))
}
#[inline]
fn xmul2f(x: f32) -> f32 {
    if x.to_bits() & 0x7fff_ffff == 0 { x } else { f32::from_bits(x.to_bits().wrapping_add(1 << 23)) }
}
#[inline]
fn xdiv2f(x: f32) -> f32 {
    xdiv(x, 1)
}
#[inline]
fn xdiv(x: f32, n: i32) -> f32 {
    if x.to_bits() & 0x7fff_ffff == 0 { x } else { f32::from_bits(x.to_bits().wrapping_sub((n as u32) << 23)) }
}
#[inline]
fn clampnan(x: f32, a: f32, b: f32) -> f32 {
    if x.is_nan() {
        (a + b) * 0.5
    } else if x.is_infinite() {
        lim(x, a, b)
    } else {
        x
    }
}
pub(crate) fn amaze(m: &Mosaic) -> Rgb32f {
    // Upstream's 16-pixel mirror padding requires a 33-pixel minimum extent.
    // Extend tiny crops with parity-preserving reflection, then crop the result.
    if m.w < 33 || m.h < 33 {
        let (w, h) = (m.w.max(34), m.h.max(34));
        let data: Vec<f32> = (0..w * h).map(|i| m.get((i % w) as isize, (i / w) as isize)).collect();
        let padded = amaze(&Mosaic { w, h, data: &data, cfa: m.cfa });
        return Rgb32f::from_fn(m.w, m.h, |x, y| padded.get(x, y));
    }
    let width = m.w as i32;
    let height = m.h as i32;
    let input = m.data;
    let clip_pt = 1.0f32;
    let mut out = vec![[0.0f32; 3]; m.w * m.h];
    let clip_pt8: f32 = (0.8f32 * clip_pt);
    let ts: i32 = 160;
    let _tsh: i32 = (ts / 2);
    let mut ex: i32;
    let mut ey: i32;
    if (fc(m, 0, 0) == 1) {
        if (fc(m, 0, 1) == 0) {
            ey = 0;
            ex = 1;
        } else {
            ey = 1;
            ex = 0;
        }
    } else {
        if (fc(m, 0, 0) == 0) {
            ey = 0;
            ex = 0;
        } else {
            ey = 1;
            ex = 1;
        }
    }
    let v1: i32 = ts;
    let v2: i32 = (2 * ts);
    let v3: i32 = (3 * ts);
    let p1: i32 = (-(ts) + 1);
    let p2: i32 = ((-(2) * ts) + 2);
    let p3: i32 = ((-(3) * ts) + 3);
    let m1: i32 = (ts + 1);
    let m2: i32 = ((2 * ts) + 2);
    let m3: i32 = ((3 * ts) + 3);
    let eps: f32 = 1e-5f32;
    let epssq: f32 = 1e-10f32;
    let arthresh: f32 = 0.75f32;
    let gaussodd: [f32; 4] = [0.14659727707323927f32, 0.103592713382435f32, 0.0732036125103057f32, 0.0365543548389495f32];
    let nyqthresh: f32 = 0.5f32;
    let gaussgrad: [f32; 6] = [
        (nyqthresh * 0.07384411893421103f32),
        (nyqthresh * 0.06207511968171489f32),
        (nyqthresh * 0.0521818194747806f32),
        (nyqthresh * 0.03687419286733595f32),
        (nyqthresh * 0.03099732204057846f32),
        (nyqthresh * 0.018413194161458882f32),
    ];
    let gausseven: [f32; 2] = [0.13719494435797422f32, 0.05640252782101291f32];
    let gquinc: [f32; 4] = [0.169917f32, 0.108947f32, 0.069855f32, 0.0287182f32];
    // Keep the exact 160-pixel tiles / 32-pixel overlap. Only their central
    // 128 rows are written. Every Rayon job has the original aliased scratch
    // planes and reuses them across its horizontal tiles, never across workers.
    let new_scratch = || (vec![0.0f32; 15 * 160 * 160], vec![0i32; 160 * 80], vec![0i32; 160 * 80]);
    let process_band = |(scratch, nyquist, nyquist2): &mut (Vec<f32>, Vec<i32>, Vec<i32>), (band, out): (usize, &mut [[f32; 3]])| {
        let top = band as i32 * 128 - 16;
        let mut left = -16;
        while left < width {
            nyquist.fill(0);
            let bottom: i32 = (top + ts).min(height + 16);
            let right: i32 = (left + ts).min(width + 16);
            let rr1: i32 = (bottom - top);
            let cc1: i32 = (right - left);
            let rrmin: i32 = (if (top < 0) { 16 } else { 0 });
            let ccmin: i32 = (if (left < 0) { 16 } else { 0 });
            let rrmax: i32 = (if (bottom > height) { (height - top) } else { rr1 });
            let ccmax: i32 = (if (right > width) { (width - left) } else { cc1 });
            if (rrmin > 0) {
                {
                    let mut rr: i32 = 0;
                    while (rr < 16) {
                        {
                            let mut cc: i32 = ccmin;
                            let mut row: i32 = ((32 - rr) + top);
                            while (cc < ccmax) {
                                {
                                    scratch[CFA + (((rr * ts) + cc) as usize)] = input[((row * width) + (cc + left)) as usize];
                                    scratch[GREEN + (((rr * ts) + cc) as usize)] = scratch[CFA + (((rr * ts) + cc) as usize)];
                                }
                                cc += 1;
                            }
                        }
                        rr += 1;
                    }
                }
            }
            {
                let mut rr: i32 = rrmin;
                while (rr < rrmax) {
                    {
                        let row: i32 = (rr + top);
                        {
                            let mut cc: i32 = ccmin;
                            while (cc < ccmax) {
                                {
                                    let indx1: i32 = ((rr * ts) + cc);
                                    scratch[CFA + ((indx1) as usize)] = input[((row * width) + (cc + left)) as usize];
                                    scratch[GREEN + ((indx1) as usize)] = scratch[CFA + ((indx1) as usize)];
                                }
                                cc += 1;
                            }
                        }
                    }
                    rr += 1;
                }
            }
            if (rrmax < rr1) {
                {
                    let mut rr: i32 = 0;
                    while (rr < 16) {
                        {
                            let mut cc: i32 = ccmin;
                            while (cc < ccmax) {
                                {
                                    scratch[CFA + ((((rrmax + rr) * ts) + cc) as usize)] =
                                        input[((((height - rr) - 2) * width) + (left + cc)) as usize];
                                    scratch[GREEN + ((((rrmax + rr) * ts) + cc) as usize)] = scratch[CFA + ((((rrmax + rr) * ts) + cc) as usize)];
                                }
                                cc += 1;
                            }
                        }
                        rr += 1;
                    }
                }
            }
            if (ccmin > 0) {
                {
                    let mut rr: i32 = rrmin;
                    while (rr < rrmax) {
                        {
                            let mut cc: i32 = 0;
                            let mut row: i32 = (rr + top);
                            while (cc < 16) {
                                {
                                    scratch[CFA + (((rr * ts) + cc) as usize)] = input[((row * width) + ((32 - cc) + left)) as usize];
                                    scratch[GREEN + (((rr * ts) + cc) as usize)] = scratch[CFA + (((rr * ts) + cc) as usize)];
                                }
                                cc += 1;
                            }
                        }
                        rr += 1;
                    }
                }
            }
            if (ccmax < cc1) {
                {
                    let mut rr: i32 = rrmin;
                    while (rr < rrmax) {
                        {
                            let mut cc: i32 = 0;
                            while (cc < 16) {
                                {
                                    scratch[CFA + ((((rr * ts) + ccmax) + cc) as usize)] =
                                        input[(((top + rr) * width) + ((width - cc) - 2)) as usize];
                                    scratch[GREEN + ((((rr * ts) + ccmax) + cc) as usize)] = scratch[CFA + ((((rr * ts) + ccmax) + cc) as usize)];
                                }
                                cc += 1;
                            }
                        }
                        rr += 1;
                    }
                }
            }
            if ((rrmin > 0) && (ccmin > 0)) {
                {
                    let mut rr: i32 = 0;
                    while (rr < 16) {
                        {
                            let mut cc: i32 = 0;
                            while (cc < 16) {
                                {
                                    scratch[CFA + (((rr * ts) + cc) as usize)] = input[(((32 - rr) * width) + (32 - cc)) as usize];
                                    scratch[GREEN + (((rr * ts) + cc) as usize)] = scratch[CFA + (((rr * ts) + cc) as usize)];
                                }
                                cc += 1;
                            }
                        }
                        rr += 1;
                    }
                }
            }
            if ((rrmax < rr1) && (ccmax < cc1)) {
                {
                    let mut rr: i32 = 0;
                    while (rr < 16) {
                        {
                            let mut cc: i32 = 0;
                            while (cc < 16) {
                                {
                                    scratch[CFA + (((((rrmax + rr) * ts) + ccmax) + cc) as usize)] =
                                        input[((((height - rr) - 2) * width) + ((width - cc) - 2)) as usize];
                                    scratch[GREEN + (((((rrmax + rr) * ts) + ccmax) + cc) as usize)] =
                                        scratch[CFA + (((((rrmax + rr) * ts) + ccmax) + cc) as usize)];
                                }
                                cc += 1;
                            }
                        }
                        rr += 1;
                    }
                }
            }
            if ((rrmin > 0) && (ccmax < cc1)) {
                {
                    let mut rr: i32 = 0;
                    while (rr < 16) {
                        {
                            let mut cc: i32 = 0;
                            while (cc < 16) {
                                {
                                    scratch[CFA + ((((rr * ts) + ccmax) + cc) as usize)] = input[(((32 - rr) * width) + ((width - cc) - 2)) as usize];
                                    scratch[GREEN + ((((rr * ts) + ccmax) + cc) as usize)] = scratch[CFA + ((((rr * ts) + ccmax) + cc) as usize)];
                                }
                                cc += 1;
                            }
                        }
                        rr += 1;
                    }
                }
            }
            if ((rrmax < rr1) && (ccmin > 0)) {
                {
                    let mut rr: i32 = 0;
                    while (rr < 16) {
                        {
                            let mut cc: i32 = 0;
                            while (cc < 16) {
                                {
                                    scratch[CFA + ((((rrmax + rr) * ts) + cc) as usize)] =
                                        input[((((height - rr) - 2) * width) + (32 - cc)) as usize];
                                    scratch[GREEN + ((((rrmax + rr) * ts) + cc) as usize)] = scratch[CFA + ((((rrmax + rr) * ts) + cc) as usize)];
                                }
                                cc += 1;
                            }
                        }
                        rr += 1;
                    }
                }
            }
            {
                let mut rr: i32 = 2;
                while (rr < (rr1 - 2)) {
                    {
                        let mut cc: i32 = 2;
                        let mut indx: i32 = ((rr * ts) + cc);
                        while (cc < (cc1 - 2)) {
                            {
                                let delh: f32 = (scratch[CFA + ((indx + 1) as usize)] - scratch[CFA + ((indx - 1) as usize)]).abs();
                                let delv: f32 = (scratch[CFA + ((indx + v1) as usize)] - scratch[CFA + ((indx - v1) as usize)]).abs();
                                scratch[DIR0 + ((indx) as usize)] = (((eps
                                    + (scratch[CFA + ((indx + v2) as usize)] - scratch[CFA + ((indx) as usize)]).abs())
                                    + (scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx - v2) as usize)]).abs())
                                    + delv);
                                scratch[DIR1 + ((indx) as usize)] = (((eps
                                    + (scratch[CFA + ((indx + 2) as usize)] - scratch[CFA + ((indx) as usize)]).abs())
                                    + (scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx - 2) as usize)]).abs())
                                    + delh);
                                scratch[DELHV + ((indx) as usize)] = (square(delh) + square(delv));
                            }
                            cc += 1;
                            indx += 1;
                        }
                    }
                    rr += 1;
                }
            }
            {
                let mut rr: i32 = 4;
                while (rr < (rr1 - 4)) {
                    {
                        let mut fcswitch: bool = ((fc(m, rr, 4) & 1) != 0);
                        {
                            let mut cc: i32 = 4;
                            let mut indx: i32 = ((rr * ts) + cc);
                            while (cc < (cc1 - 4)) {
                                {
                                    let cru: f32 = ((scratch[CFA + ((indx - v1) as usize)]
                                        * (scratch[DIR0 + ((indx - v2) as usize)] + scratch[DIR0 + ((indx) as usize)]))
                                        / ((scratch[DIR0 + ((indx - v2) as usize)] * (eps + scratch[CFA + ((indx) as usize)]))
                                            + (scratch[DIR0 + ((indx) as usize)] * (eps + scratch[CFA + ((indx - v2) as usize)]))));
                                    let crd: f32 = ((scratch[CFA + ((indx + v1) as usize)]
                                        * (scratch[DIR0 + ((indx + v2) as usize)] + scratch[DIR0 + ((indx) as usize)]))
                                        / ((scratch[DIR0 + ((indx + v2) as usize)] * (eps + scratch[CFA + ((indx) as usize)]))
                                            + (scratch[DIR0 + ((indx) as usize)] * (eps + scratch[CFA + ((indx + v2) as usize)]))));
                                    let crl: f32 = ((scratch[CFA + ((indx - 1) as usize)]
                                        * (scratch[DIR1 + ((indx - 2) as usize)] + scratch[DIR1 + ((indx) as usize)]))
                                        / ((scratch[DIR1 + ((indx - 2) as usize)] * (eps + scratch[CFA + ((indx) as usize)]))
                                            + (scratch[DIR1 + ((indx) as usize)] * (eps + scratch[CFA + ((indx - 2) as usize)]))));
                                    let crr: f32 = ((scratch[CFA + ((indx + 1) as usize)]
                                        * (scratch[DIR1 + ((indx + 2) as usize)] + scratch[DIR1 + ((indx) as usize)]))
                                        / ((scratch[DIR1 + ((indx + 2) as usize)] * (eps + scratch[CFA + ((indx) as usize)]))
                                            + (scratch[DIR1 + ((indx) as usize)] * (eps + scratch[CFA + ((indx + 2) as usize)]))));
                                    let guha: f32 = (scratch[CFA + ((indx - v1) as usize)]
                                        + xdiv2f(scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx - v2) as usize)]));
                                    let gdha: f32 = (scratch[CFA + ((indx + v1) as usize)]
                                        + xdiv2f(scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx + v2) as usize)]));
                                    let glha: f32 = (scratch[CFA + ((indx - 1) as usize)]
                                        + xdiv2f(scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx - 2) as usize)]));
                                    let grha: f32 = (scratch[CFA + ((indx + 1) as usize)]
                                        + xdiv2f(scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx + 2) as usize)]));
                                    let mut guar: f32;
                                    let mut gdar: f32;
                                    let mut glar: f32;
                                    let mut grar: f32;
                                    if ((1.0f32 - cru).abs() < arthresh) {
                                        guar = (scratch[CFA + ((indx) as usize)] * cru);
                                    } else {
                                        guar = guha;
                                    }
                                    if ((1.0f32 - crd).abs() < arthresh) {
                                        gdar = (scratch[CFA + ((indx) as usize)] * crd);
                                    } else {
                                        gdar = gdha;
                                    }
                                    if ((1.0f32 - crl).abs() < arthresh) {
                                        glar = (scratch[CFA + ((indx) as usize)] * crl);
                                    } else {
                                        glar = glha;
                                    }
                                    if ((1.0f32 - crr).abs() < arthresh) {
                                        grar = (scratch[CFA + ((indx) as usize)] * crr);
                                    } else {
                                        grar = grha;
                                    }
                                    let hwt: f32 = (scratch[DIR1 + ((indx - 1) as usize)]
                                        / (scratch[DIR1 + ((indx - 1) as usize)] + scratch[DIR1 + ((indx + 1) as usize)]));
                                    let vwt: f32 = (scratch[DIR0 + ((indx - v1) as usize)]
                                        / (scratch[DIR0 + ((indx + v1) as usize)] + scratch[DIR0 + ((indx - v1) as usize)]));
                                    let Gintvha: f32 = ((vwt * gdha) + ((1.0f32 - vwt) * guha));
                                    let Ginthha: f32 = ((hwt * grha) + ((1.0f32 - hwt) * glha));
                                    if fcswitch {
                                        scratch[VCD + ((indx) as usize)] =
                                            (scratch[CFA + ((indx) as usize)] - ((vwt * gdar) + ((1.0f32 - vwt) * guar)));
                                        scratch[HCD + ((indx) as usize)] =
                                            (scratch[CFA + ((indx) as usize)] - ((hwt * grar) + ((1.0f32 - hwt) * glar)));
                                        scratch[VCD_ALT + ((indx) as usize)] = (scratch[CFA + ((indx) as usize)] - Gintvha);
                                        scratch[HCD_ALT + ((indx) as usize)] = (scratch[CFA + ((indx) as usize)] - Ginthha);
                                    } else {
                                        scratch[VCD + ((indx) as usize)] =
                                            (((vwt * gdar) + ((1.0f32 - vwt) * guar)) - scratch[CFA + ((indx) as usize)]);
                                        scratch[HCD + ((indx) as usize)] =
                                            (((hwt * grar) + ((1.0f32 - hwt) * glar)) - scratch[CFA + ((indx) as usize)]);
                                        scratch[VCD_ALT + ((indx) as usize)] = (Gintvha - scratch[CFA + ((indx) as usize)]);
                                        scratch[HCD_ALT + ((indx) as usize)] = (Ginthha - scratch[CFA + ((indx) as usize)]);
                                    }
                                    fcswitch = !(fcswitch);
                                    if (((scratch[CFA + ((indx) as usize)] > clip_pt8) || (Gintvha > clip_pt8)) || (Ginthha > clip_pt8)) {
                                        guar = guha;
                                        gdar = gdha;
                                        glar = glha;
                                        grar = grha;
                                        scratch[VCD + ((indx) as usize)] = scratch[VCD_ALT + ((indx) as usize)];
                                        scratch[HCD + ((indx) as usize)] = scratch[HCD_ALT + ((indx) as usize)];
                                    }
                                    scratch[DGINTV + ((indx) as usize)] = (square(guha - gdha)).min(square(guar - gdar));
                                    scratch[DGINTH + ((indx) as usize)] = (square(glha - grha)).min(square(glar - grar));
                                }
                                cc += 1;
                                indx += 1;
                            }
                        }
                    }
                    rr += 1;
                }
            }
            {
                let mut rr: i32 = 4;
                while (rr < (rr1 - 4)) {
                    {
                        {
                            let mut cc: i32 = 4;
                            let mut indx: i32 = ((rr * ts) + cc);
                            let mut c: i32 = (fc(m, rr, cc) & 1);
                            while (cc < (cc1 - 4)) {
                                {
                                    let hcdvar: f32 = ((3.0f32
                                        * ((square(scratch[HCD + ((indx - 2) as usize)]) + square(scratch[HCD + ((indx) as usize)]))
                                            + square(scratch[HCD + ((indx + 2) as usize)])))
                                        - square(
                                            ((scratch[HCD + ((indx - 2) as usize)] + scratch[HCD + ((indx) as usize)])
                                                + scratch[HCD + ((indx + 2) as usize)]),
                                        ));
                                    let hcdaltvar: f32 = ((3.0f32
                                        * ((square(scratch[HCD_ALT + ((indx - 2) as usize)]) + square(scratch[HCD_ALT + ((indx) as usize)]))
                                            + square(scratch[HCD_ALT + ((indx + 2) as usize)])))
                                        - square(
                                            ((scratch[HCD_ALT + ((indx - 2) as usize)] + scratch[HCD_ALT + ((indx) as usize)])
                                                + scratch[HCD_ALT + ((indx + 2) as usize)]),
                                        ));
                                    let vcdvar: f32 = ((3.0f32
                                        * ((square(scratch[VCD + ((indx - v2) as usize)]) + square(scratch[VCD + ((indx) as usize)]))
                                            + square(scratch[VCD + ((indx + v2) as usize)])))
                                        - square(
                                            ((scratch[VCD + ((indx - v2) as usize)] + scratch[VCD + ((indx) as usize)])
                                                + scratch[VCD + ((indx + v2) as usize)]),
                                        ));
                                    let vcdaltvar: f32 = ((3.0f32
                                        * ((square(scratch[VCD_ALT + ((indx - v2) as usize)]) + square(scratch[VCD_ALT + ((indx) as usize)]))
                                            + square(scratch[VCD_ALT + ((indx + v2) as usize)])))
                                        - square(
                                            ((scratch[VCD_ALT + ((indx - v2) as usize)] + scratch[VCD_ALT + ((indx) as usize)])
                                                + scratch[VCD_ALT + ((indx + v2) as usize)]),
                                        ));
                                    if (hcdaltvar < hcdvar) {
                                        scratch[HCD + ((indx) as usize)] = scratch[HCD_ALT + ((indx) as usize)];
                                    }
                                    if (vcdaltvar < vcdvar) {
                                        scratch[VCD + ((indx) as usize)] = scratch[VCD_ALT + ((indx) as usize)];
                                    }
                                    let mut Gintv: f32;
                                    let mut Ginth: f32;
                                    if (c != 0) {
                                        Ginth = (-(scratch[HCD + ((indx) as usize)]) + scratch[CFA + ((indx) as usize)]);
                                        Gintv = (-(scratch[VCD + ((indx) as usize)]) + scratch[CFA + ((indx) as usize)]);
                                        if (scratch[HCD + ((indx) as usize)] > (0 as f32)) {
                                            if ((3.0f32 * scratch[HCD + ((indx) as usize)]) > (Ginth + scratch[CFA + ((indx) as usize)])) {
                                                scratch[HCD + ((indx) as usize)] =
                                                    (-(ulim(Ginth, scratch[CFA + ((indx - 1) as usize)], scratch[CFA + ((indx + 1) as usize)]))
                                                        + scratch[CFA + ((indx) as usize)]);
                                            } else {
                                                let hwt: f32 = (1.0f32
                                                    - ((3.0f32 * scratch[HCD + ((indx) as usize)])
                                                        / ((eps + Ginth) + scratch[CFA + ((indx) as usize)])));
                                                scratch[HCD + ((indx) as usize)] = ((hwt * scratch[HCD + ((indx) as usize)])
                                                    + ((1.0f32 - hwt)
                                                        * (-(ulim(
                                                            Ginth,
                                                            scratch[CFA + ((indx - 1) as usize)],
                                                            scratch[CFA + ((indx + 1) as usize)],
                                                        )) + scratch[CFA + ((indx) as usize)])));
                                            }
                                        }
                                        if (scratch[VCD + ((indx) as usize)] > (0 as f32)) {
                                            if ((3.0f32 * scratch[VCD + ((indx) as usize)]) > (Gintv + scratch[CFA + ((indx) as usize)])) {
                                                scratch[VCD + ((indx) as usize)] =
                                                    (-(ulim(Gintv, scratch[CFA + ((indx - v1) as usize)], scratch[CFA + ((indx + v1) as usize)]))
                                                        + scratch[CFA + ((indx) as usize)]);
                                            } else {
                                                let vwt: f32 = (1.0f32
                                                    - ((3.0f32 * scratch[VCD + ((indx) as usize)])
                                                        / ((eps + Gintv) + scratch[CFA + ((indx) as usize)])));
                                                scratch[VCD + ((indx) as usize)] = ((vwt * scratch[VCD + ((indx) as usize)])
                                                    + ((1.0f32 - vwt)
                                                        * (-(ulim(
                                                            Gintv,
                                                            scratch[CFA + ((indx - v1) as usize)],
                                                            scratch[CFA + ((indx + v1) as usize)],
                                                        )) + scratch[CFA + ((indx) as usize)])));
                                            }
                                        }
                                        if (Ginth > clip_pt) {
                                            scratch[HCD + ((indx) as usize)] =
                                                (-(ulim(Ginth, scratch[CFA + ((indx - 1) as usize)], scratch[CFA + ((indx + 1) as usize)]))
                                                    + scratch[CFA + ((indx) as usize)]);
                                        }
                                        if (Gintv > clip_pt) {
                                            scratch[VCD + ((indx) as usize)] =
                                                (-(ulim(Gintv, scratch[CFA + ((indx - v1) as usize)], scratch[CFA + ((indx + v1) as usize)]))
                                                    + scratch[CFA + ((indx) as usize)]);
                                        }
                                    } else {
                                        Ginth = (scratch[HCD + ((indx) as usize)] + scratch[CFA + ((indx) as usize)]);
                                        Gintv = (scratch[VCD + ((indx) as usize)] + scratch[CFA + ((indx) as usize)]);
                                        if (scratch[HCD + ((indx) as usize)] < (0 as f32)) {
                                            if ((3.0f32 * scratch[HCD + ((indx) as usize)]) < -(Ginth + scratch[CFA + ((indx) as usize)])) {
                                                scratch[HCD + ((indx) as usize)] =
                                                    (ulim(Ginth, scratch[CFA + ((indx - 1) as usize)], scratch[CFA + ((indx + 1) as usize)])
                                                        - scratch[CFA + ((indx) as usize)]);
                                            } else {
                                                let mut hwt: f32 = (1.0f32
                                                    + ((3.0f32 * scratch[HCD + ((indx) as usize)])
                                                        / ((eps + Ginth) + scratch[CFA + ((indx) as usize)])));
                                                scratch[HCD + ((indx) as usize)] = ((hwt * scratch[HCD + ((indx) as usize)])
                                                    + ((1.0f32 - hwt)
                                                        * (ulim(Ginth, scratch[CFA + ((indx - 1) as usize)], scratch[CFA + ((indx + 1) as usize)])
                                                            - scratch[CFA + ((indx) as usize)])));
                                            }
                                        }
                                        if (scratch[VCD + ((indx) as usize)] < (0 as f32)) {
                                            if ((3.0f32 * scratch[VCD + ((indx) as usize)]) < -(Gintv + scratch[CFA + ((indx) as usize)])) {
                                                scratch[VCD + ((indx) as usize)] =
                                                    (ulim(Gintv, scratch[CFA + ((indx - v1) as usize)], scratch[CFA + ((indx + v1) as usize)])
                                                        - scratch[CFA + ((indx) as usize)]);
                                            } else {
                                                let vwt: f32 = (1.0f32
                                                    + ((3.0f32 * scratch[VCD + ((indx) as usize)])
                                                        / ((eps + Gintv) + scratch[CFA + ((indx) as usize)])));
                                                scratch[VCD + ((indx) as usize)] = ((vwt * scratch[VCD + ((indx) as usize)])
                                                    + ((1.0f32 - vwt)
                                                        * (ulim(
                                                            Gintv,
                                                            scratch[CFA + ((indx - v1) as usize)],
                                                            scratch[CFA + ((indx + v1) as usize)],
                                                        ) - scratch[CFA + ((indx) as usize)])));
                                            }
                                        }
                                        if (Ginth > clip_pt) {
                                            scratch[HCD + ((indx) as usize)] =
                                                (ulim(Ginth, scratch[CFA + ((indx - 1) as usize)], scratch[CFA + ((indx + 1) as usize)])
                                                    - scratch[CFA + ((indx) as usize)]);
                                        }
                                        if (Gintv > clip_pt) {
                                            scratch[VCD + ((indx) as usize)] =
                                                (ulim(Gintv, scratch[CFA + ((indx - v1) as usize)], scratch[CFA + ((indx + v1) as usize)])
                                                    - scratch[CFA + ((indx) as usize)]);
                                        }
                                        scratch[CDDIFF + ((indx) as usize)] =
                                            square(scratch[VCD + ((indx) as usize)] - scratch[HCD + ((indx) as usize)]);
                                    }
                                    c = (!(c != 0) as i32);
                                }
                                cc += 1;
                                indx += 1;
                            }
                        }
                    }
                    rr += 1;
                }
            }
            {
                let mut rr: i32 = 6;
                while (rr < (rr1 - 6)) {
                    {
                        {
                            let mut cc: i32 = (6 + (fc(m, rr, 2) & 1));
                            let mut indx: i32 = ((rr * ts) + cc);
                            while (cc < (cc1 - 6)) {
                                {
                                    let uave: f32 = (((scratch[VCD + ((indx) as usize)] + scratch[VCD + ((indx - v1) as usize)])
                                        + scratch[VCD + ((indx - v2) as usize)])
                                        + scratch[VCD + ((indx - v3) as usize)]);
                                    let dave: f32 = (((scratch[VCD + ((indx) as usize)] + scratch[VCD + ((indx + v1) as usize)])
                                        + scratch[VCD + ((indx + v2) as usize)])
                                        + scratch[VCD + ((indx + v3) as usize)]);
                                    let lave: f32 = (((scratch[HCD + ((indx) as usize)] + scratch[HCD + ((indx - 1) as usize)])
                                        + scratch[HCD + ((indx - 2) as usize)])
                                        + scratch[HCD + ((indx - 3) as usize)]);
                                    let rave: f32 = (((scratch[HCD + ((indx) as usize)] + scratch[HCD + ((indx + 1) as usize)])
                                        + scratch[HCD + ((indx + 2) as usize)])
                                        + scratch[HCD + ((indx + 3) as usize)]);
                                    let mut Dgrbvvaru: f32 = (((square(scratch[VCD + ((indx) as usize)] - uave)
                                        + square(scratch[VCD + ((indx - v1) as usize)] - uave))
                                        + square(scratch[VCD + ((indx - v2) as usize)] - uave))
                                        + square(scratch[VCD + ((indx - v3) as usize)] - uave));
                                    let mut Dgrbvvard: f32 = (((square(scratch[VCD + ((indx) as usize)] - dave)
                                        + square(scratch[VCD + ((indx + v1) as usize)] - dave))
                                        + square(scratch[VCD + ((indx + v2) as usize)] - dave))
                                        + square(scratch[VCD + ((indx + v3) as usize)] - dave));
                                    let mut Dgrbhvarl: f32 = (((square(scratch[HCD + ((indx) as usize)] - lave)
                                        + square(scratch[HCD + ((indx - 1) as usize)] - lave))
                                        + square(scratch[HCD + ((indx - 2) as usize)] - lave))
                                        + square(scratch[HCD + ((indx - 3) as usize)] - lave));
                                    let mut Dgrbhvarr: f32 = (((square(scratch[HCD + ((indx) as usize)] - rave)
                                        + square(scratch[HCD + ((indx + 1) as usize)] - rave))
                                        + square(scratch[HCD + ((indx + 2) as usize)] - rave))
                                        + square(scratch[HCD + ((indx + 3) as usize)] - rave));
                                    let hwt: f32 = (scratch[DIR1 + ((indx - 1) as usize)]
                                        / (scratch[DIR1 + ((indx - 1) as usize)] + scratch[DIR1 + ((indx + 1) as usize)]));
                                    let vwt: f32 = (scratch[DIR0 + ((indx - v1) as usize)]
                                        / (scratch[DIR0 + ((indx + v1) as usize)] + scratch[DIR0 + ((indx - v1) as usize)]));
                                    let vcdvar: f32 = ((epssq + (vwt * Dgrbvvard)) + ((1.0f32 - vwt) * Dgrbvvaru));
                                    let hcdvar: f32 = ((epssq + (hwt * Dgrbhvarr)) + ((1.0f32 - hwt) * Dgrbhvarl));
                                    Dgrbvvaru = ((scratch[DGINTV + ((indx) as usize)] + scratch[DGINTV + ((indx - v1) as usize)])
                                        + scratch[DGINTV + ((indx - v2) as usize)]);
                                    Dgrbvvard = ((scratch[DGINTV + ((indx) as usize)] + scratch[DGINTV + ((indx + v1) as usize)])
                                        + scratch[DGINTV + ((indx + v2) as usize)]);
                                    Dgrbhvarl = ((scratch[DGINTH + ((indx) as usize)] + scratch[DGINTH + ((indx - 1) as usize)])
                                        + scratch[DGINTH + ((indx - 2) as usize)]);
                                    Dgrbhvarr = ((scratch[DGINTH + ((indx) as usize)] + scratch[DGINTH + ((indx + 1) as usize)])
                                        + scratch[DGINTH + ((indx + 2) as usize)]);
                                    let mut vcdvar1: f32 = ((epssq + (vwt * Dgrbvvard)) + ((1.0f32 - vwt) * Dgrbvvaru));
                                    let mut hcdvar1: f32 = ((epssq + (hwt * Dgrbhvarr)) + ((1.0f32 - hwt) * Dgrbhvarl));
                                    let varwt: f32 = (hcdvar / (vcdvar + hcdvar));
                                    let diffwt: f32 = (hcdvar1 / (vcdvar1 + hcdvar1));
                                    if ((((0.5f32 - varwt) * (0.5f32 - diffwt)) > (0 as f32)) && ((0.5f32 - diffwt).abs() < (0.5f32 - varwt).abs())) {
                                        scratch[HVWT + ((indx >> 1) as usize)] = varwt;
                                    } else {
                                        scratch[HVWT + ((indx >> 1) as usize)] = diffwt;
                                    }
                                }
                                cc += 2;
                                indx += 2;
                            }
                        }
                    }
                    rr += 1;
                }
            }
            {
                let mut rr: i32 = 6;
                while (rr < (rr1 - 6)) {
                    {
                        let mut cc: i32 = (6 + (fc(m, rr, 2) & 1));
                        let mut indx: i32 = ((rr * ts) + cc);
                        {
                            while (cc < (cc1 - 6)) {
                                {
                                    scratch[NYQUIST_TEST + ((indx >> 1) as usize)] = (((((gaussodd[0_usize]
                                        * scratch[CDDIFF + ((indx) as usize)])
                                        + (gaussodd[1_usize]
                                            * (((scratch[CDDIFF + ((indx - m1) as usize)] + scratch[CDDIFF + ((indx + p1) as usize)])
                                                + scratch[CDDIFF + ((indx - p1) as usize)])
                                                + scratch[CDDIFF + ((indx + m1) as usize)])))
                                        + (gaussodd[2_usize]
                                            * (((scratch[CDDIFF + ((indx - v2) as usize)] + scratch[CDDIFF + ((indx - 2) as usize)])
                                                + scratch[CDDIFF + ((indx + 2) as usize)])
                                                + scratch[CDDIFF + ((indx + v2) as usize)])))
                                        + (gaussodd[3_usize]
                                            * (((scratch[CDDIFF + ((indx - m2) as usize)] + scratch[CDDIFF + ((indx + p2) as usize)])
                                                + scratch[CDDIFF + ((indx - p2) as usize)])
                                                + scratch[CDDIFF + ((indx + m2) as usize)])))
                                        - ((((((gaussgrad[0_usize] * scratch[DELHV + ((indx) as usize)])
                                            + (gaussgrad[1_usize]
                                                * (((scratch[DELHV + ((indx - v1) as usize)] + scratch[DELHV + ((indx + 1) as usize)])
                                                    + scratch[DELHV + ((indx - 1) as usize)])
                                                    + scratch[DELHV + ((indx + v1) as usize)])))
                                            + (gaussgrad[2_usize]
                                                * (((scratch[DELHV + ((indx - m1) as usize)] + scratch[DELHV + ((indx + p1) as usize)])
                                                    + scratch[DELHV + ((indx - p1) as usize)])
                                                    + scratch[DELHV + ((indx + m1) as usize)])))
                                            + (gaussgrad[3_usize]
                                                * (((scratch[DELHV + ((indx - v2) as usize)] + scratch[DELHV + ((indx - 2) as usize)])
                                                    + scratch[DELHV + ((indx + 2) as usize)])
                                                    + scratch[DELHV + ((indx + v2) as usize)])))
                                            + (gaussgrad[4_usize]
                                                * (((((((scratch[DELHV + (((indx - v2) - 1) as usize)]
                                                    + scratch[DELHV + (((indx - v2) + 1) as usize)])
                                                    + scratch[DELHV + (((indx - ts) - 2) as usize)])
                                                    + scratch[DELHV + (((indx - ts) + 2) as usize)])
                                                    + scratch[DELHV + (((indx + ts) - 2) as usize)])
                                                    + scratch[DELHV + (((indx + ts) + 2) as usize)])
                                                    + scratch[DELHV + (((indx + v2) - 1) as usize)])
                                                    + scratch[DELHV + (((indx + v2) + 1) as usize)])))
                                            + (gaussgrad[5_usize]
                                                * (((scratch[DELHV + ((indx - m2) as usize)] + scratch[DELHV + ((indx + p2) as usize)])
                                                    + scratch[DELHV + ((indx - p2) as usize)])
                                                    + scratch[DELHV + ((indx + m2) as usize)]))));
                                }
                                cc += 2;
                                indx += 2;
                            }
                        }
                    }
                    rr += 1;
                }
            }
            let mut nystartrow: i32 = 0;
            let mut nyendrow: i32 = 0;
            let mut nystartcol: i32 = (ts + 1);
            let mut nyendcol: i32 = 0;
            {
                let mut rr: i32 = 6;
                while (rr < (rr1 - 6)) {
                    {
                        {
                            let mut cc: i32 = (6 + (fc(m, rr, 2) & 1));
                            let mut indx: i32 = ((rr * ts) + cc);
                            while (cc < (cc1 - 6)) {
                                {
                                    if (scratch[NYQUIST_TEST + ((indx >> 1) as usize)] > 0.0f32) {
                                        nyquist[(indx >> 1) as usize] = 1;
                                        nystartrow = (if (nystartrow != 0) { nystartrow } else { rr });
                                        nyendrow = rr;
                                        nystartcol = (if (nystartcol > cc) { cc } else { nystartcol });
                                        nyendcol = (if (nyendcol < cc) { cc } else { nyendcol });
                                    }
                                }
                                cc += 2;
                                indx += 2;
                            }
                        }
                    }
                    rr += 1;
                }
            }
            let mut doNyquist: bool = ((nystartrow != nyendrow) && (nystartcol != nyendcol));
            if doNyquist {
                nyendrow += 1;
                nyendcol += 1;
                nystartcol -= (nystartcol & 1);
                nystartrow = (8).max(nystartrow);
                nyendrow = (rr1 - 8).min(nyendrow);
                nystartcol = (8).max(nystartcol);
                nyendcol = (cc1 - 8).min(nyendcol);
                nyquist2.fill(0);
                {
                    let mut rr: i32 = nystartrow;
                    while (rr < nyendrow) {
                        {
                            {
                                let mut indx: i32 = (((rr * ts) + nystartcol) + (fc(m, rr, 2) & 1));
                                while (indx < ((rr * ts) + nyendcol)) {
                                    {
                                        let mut nyquisttemp: i32 = (((((((nyquist[((indx - v2) >> 1) as usize]
                                            + nyquist[((indx - m1) >> 1) as usize])
                                            + nyquist[((indx + p1) >> 1) as usize])
                                            + nyquist[((indx - 2) >> 1) as usize])
                                            + nyquist[((indx + 2) >> 1) as usize])
                                            + nyquist[((indx - p1) >> 1) as usize])
                                            + nyquist[((indx + m1) >> 1) as usize])
                                            + nyquist[((indx + v2) >> 1) as usize]);
                                        nyquist2[(indx >> 1) as usize] =
                                            (if (nyquisttemp > 4) { 1 } else { (if (nyquisttemp < 4) { 0 } else { nyquist[(indx >> 1) as usize] }) });
                                    }
                                    indx += 2;
                                }
                            }
                        }
                        rr += 1;
                    }
                }
                {
                    let mut rr: i32 = nystartrow;
                    while (rr < nyendrow) {
                        {
                            let mut indx: i32 = (((rr * ts) + nystartcol) + (fc(m, rr, 2) & 1));
                            while (indx < ((rr * ts) + nyendcol)) {
                                {
                                    if (nyquist2[(indx >> 1) as usize] != 0) {
                                        let mut sumcfa: f32 = 0.0f32;
                                        let mut sumh: f32 = 0.0f32;
                                        let mut sumv: f32 = 0.0f32;
                                        let mut sumsqh: f32 = 0.0f32;
                                        let mut sumsqv: f32 = 0.0f32;
                                        let mut areawt: f32 = 0.0f32;
                                        {
                                            let mut i: i32 = -(6);
                                            while (i < 7) {
                                                {
                                                    let mut indx1: i32 = ((indx + (i * ts)) - 6);
                                                    {
                                                        let mut j: i32 = -(6);
                                                        while (j < 7) {
                                                            {
                                                                if (nyquist2[(indx1 >> 1) as usize] != 0) {
                                                                    let mut cfatemp: f32 = scratch[CFA + ((indx1) as usize)];
                                                                    sumcfa += cfatemp;
                                                                    sumh += (scratch[CFA + ((indx1 - 1) as usize)]
                                                                        + scratch[CFA + ((indx1 + 1) as usize)]);
                                                                    sumv += (scratch[CFA + ((indx1 - v1) as usize)]
                                                                        + scratch[CFA + ((indx1 + v1) as usize)]);
                                                                    sumsqh += (square(cfatemp - scratch[CFA + ((indx1 - 1) as usize)])
                                                                        + square(cfatemp - scratch[CFA + ((indx1 + 1) as usize)]));
                                                                    sumsqv += (square(cfatemp - scratch[CFA + ((indx1 - v1) as usize)])
                                                                        + square(cfatemp - scratch[CFA + ((indx1 + v1) as usize)]));
                                                                    areawt += 1_f32;
                                                                }
                                                            }
                                                            j += 2;
                                                            indx1 += 2;
                                                        }
                                                    }
                                                }
                                                i += 2;
                                            }
                                        }
                                        sumh = (sumcfa - xdiv2f(sumh));
                                        sumv = (sumcfa - xdiv2f(sumv));
                                        areawt = xdiv2f(areawt);
                                        let hcdvar: f32 = (epssq + ((areawt * sumsqh) - (sumh * sumh)).abs());
                                        let vcdvar: f32 = (epssq + ((areawt * sumsqv) - (sumv * sumv)).abs());
                                        scratch[HVWT + ((indx >> 1) as usize)] = (hcdvar / (vcdvar + hcdvar));
                                    }
                                }
                                indx += 2;
                            }
                        }
                        rr += 1;
                    }
                }
            }
            {
                let mut rr: i32 = 8;
                while (rr < (rr1 - 8)) {
                    {
                        let mut indx: i32 = (((rr * ts) + 8) + (fc(m, rr, 2) & 1));
                        while (indx < (((rr * ts) + cc1) - 8)) {
                            {
                                let hvwtalt: f32 = xdiv(
                                    (((scratch[HVWT + (((indx - m1) >> 1) as usize)] + scratch[HVWT + (((indx + p1) >> 1) as usize)])
                                        + scratch[HVWT + (((indx - p1) >> 1) as usize)])
                                        + scratch[HVWT + (((indx + m1) >> 1) as usize)]),
                                    2,
                                );
                                scratch[HVWT + ((indx >> 1) as usize)] =
                                    (if ((0.5f32 - scratch[HVWT + ((indx >> 1) as usize)]).abs() < (0.5f32 - hvwtalt).abs()) {
                                        hvwtalt
                                    } else {
                                        scratch[HVWT + ((indx >> 1) as usize)]
                                    });
                                scratch[VCD_ALT + ((indx >> 1) as usize)] = interpolatef(
                                    scratch[HVWT + ((indx >> 1) as usize)],
                                    scratch[VCD + ((indx) as usize)],
                                    scratch[HCD + ((indx) as usize)],
                                );
                                scratch[GREEN + ((indx) as usize)] = (scratch[CFA + ((indx) as usize)] + scratch[VCD_ALT + ((indx >> 1) as usize)]);
                                scratch[(DGINTV + 2 * ((indx >> 1) as usize))] = (if (nyquist2[(indx >> 1) as usize] != 0) {
                                    square(
                                        (scratch[GREEN + ((indx) as usize)]
                                            - xdiv2f(scratch[GREEN + ((indx - 1) as usize)] + scratch[GREEN + ((indx + 1) as usize)])),
                                    )
                                } else {
                                    0.0f32
                                });
                                scratch[DGINTV + 2 * ((indx >> 1) as usize) + 1] = (if (nyquist2[(indx >> 1) as usize] != 0) {
                                    square(
                                        (scratch[GREEN + ((indx) as usize)]
                                            - xdiv2f(scratch[GREEN + ((indx - v1) as usize)] + scratch[GREEN + ((indx + v1) as usize)])),
                                    )
                                } else {
                                    0.0f32
                                });
                            }
                            indx += 2;
                        }
                    }
                    rr += 1;
                }
            }
            if doNyquist {
                {
                    let mut rr: i32 = nystartrow;
                    while (rr < nyendrow) {
                        {
                            let mut indx: i32 = (((rr * ts) + nystartcol) + (fc(m, rr, 2) & 1));
                            while (indx < ((rr * ts) + nyendcol)) {
                                {
                                    if (nyquist2[(indx >> 1) as usize] != 0) {
                                        let gvarh: f32 = (epssq
                                            + ((((gquinc[0_usize] * scratch[(DGINTV + 2 * ((indx >> 1) as usize))])
                                                + (gquinc[1_usize]
                                                    * (((scratch[(DGINTV + 2 * (((indx - m1) >> 1) as usize))]
                                                        + scratch[(DGINTV + 2 * (((indx + p1) >> 1) as usize))])
                                                        + scratch[(DGINTV + 2 * (((indx - p1) >> 1) as usize))])
                                                        + scratch[(DGINTV + 2 * (((indx + m1) >> 1) as usize))])))
                                                + (gquinc[2_usize]
                                                    * (((scratch[(DGINTV + 2 * (((indx - v2) >> 1) as usize))]
                                                        + scratch[(DGINTV + 2 * (((indx - 2) >> 1) as usize))])
                                                        + scratch[(DGINTV + 2 * (((indx + 2) >> 1) as usize))])
                                                        + scratch[(DGINTV + 2 * (((indx + v2) >> 1) as usize))])))
                                                + (gquinc[3_usize]
                                                    * (((scratch[(DGINTV + 2 * (((indx - m2) >> 1) as usize))]
                                                        + scratch[(DGINTV + 2 * (((indx + p2) >> 1) as usize))])
                                                        + scratch[(DGINTV + 2 * (((indx - p2) >> 1) as usize))])
                                                        + scratch[(DGINTV + 2 * (((indx + m2) >> 1) as usize))]))));
                                        let gvarv: f32 = (epssq
                                            + ((((gquinc[0_usize] * scratch[DGINTV + 2 * ((indx >> 1) as usize) + 1])
                                                + (gquinc[1_usize]
                                                    * (((scratch[DGINTV + 2 * (((indx - m1) >> 1) as usize) + 1]
                                                        + scratch[DGINTV + 2 * (((indx + p1) >> 1) as usize) + 1])
                                                        + scratch[DGINTV + 2 * (((indx - p1) >> 1) as usize) + 1])
                                                        + scratch[DGINTV + 2 * (((indx + m1) >> 1) as usize) + 1])))
                                                + (gquinc[2_usize]
                                                    * (((scratch[DGINTV + 2 * (((indx - v2) >> 1) as usize) + 1]
                                                        + scratch[DGINTV + 2 * (((indx - 2) >> 1) as usize) + 1])
                                                        + scratch[DGINTV + 2 * (((indx + 2) >> 1) as usize) + 1])
                                                        + scratch[DGINTV + 2 * (((indx + v2) >> 1) as usize) + 1])))
                                                + (gquinc[3_usize]
                                                    * (((scratch[DGINTV + 2 * (((indx - m2) >> 1) as usize) + 1]
                                                        + scratch[DGINTV + 2 * (((indx + p2) >> 1) as usize) + 1])
                                                        + scratch[DGINTV + 2 * (((indx - p2) >> 1) as usize) + 1])
                                                        + scratch[DGINTV + 2 * (((indx + m2) >> 1) as usize) + 1]))));
                                        scratch[VCD_ALT + ((indx >> 1) as usize)] = (((scratch[HCD + ((indx) as usize)] * gvarv)
                                            + (scratch[VCD + ((indx) as usize)] * gvarh))
                                            / (gvarv + gvarh));
                                        scratch[GREEN + ((indx) as usize)] =
                                            (scratch[CFA + ((indx) as usize)] + scratch[VCD_ALT + ((indx >> 1) as usize)]);
                                    }
                                }
                                indx += 2;
                            }
                        }
                        rr += 1;
                    }
                }
            }
            {
                let mut rr: i32 = 6;
                while (rr < (rr1 - 6)) {
                    {
                        if ((fc(m, rr, 2) & 1) == 0) {
                            {
                                let mut cc: i32 = 6;
                                let mut indx: i32 = ((rr * ts) + cc);
                                while (cc < (cc1 - 6)) {
                                    {
                                        scratch[CDDIFF + ((indx >> 1) as usize)] =
                                            (scratch[CFA + ((indx + p1) as usize)] - scratch[CFA + ((indx - p1) as usize)]).abs();
                                        scratch[DELM + ((indx >> 1) as usize)] =
                                            (scratch[CFA + ((indx + m1) as usize)] - scratch[CFA + ((indx - m1) as usize)]).abs();
                                        scratch[DGRB_SQ_P + ((indx >> 1) as usize)] =
                                            (square(scratch[CFA + ((indx + 1) as usize)] - scratch[CFA + (((indx + 1) - p1) as usize)])
                                                + square(scratch[CFA + ((indx + 1) as usize)] - scratch[CFA + (((indx + 1) + p1) as usize)]));
                                        scratch[DGRB_SQ_M + ((indx >> 1) as usize)] =
                                            (square(scratch[CFA + ((indx + 1) as usize)] - scratch[CFA + (((indx + 1) - m1) as usize)])
                                                + square(scratch[CFA + ((indx + 1) as usize)] - scratch[CFA + (((indx + 1) + m1) as usize)]));
                                    }
                                    cc += 2;
                                    indx += 2;
                                }
                            }
                        } else {
                            {
                                let mut cc: i32 = 6;
                                let mut indx: i32 = ((rr * ts) + cc);
                                while (cc < (cc1 - 6)) {
                                    {
                                        scratch[DGRB_SQ_P + ((indx >> 1) as usize)] =
                                            (square(scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx - p1) as usize)])
                                                + square(scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx + p1) as usize)]));
                                        scratch[DGRB_SQ_M + ((indx >> 1) as usize)] =
                                            (square(scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx - m1) as usize)])
                                                + square(scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx + m1) as usize)]));
                                        scratch[CDDIFF + ((indx >> 1) as usize)] =
                                            (scratch[CFA + (((indx + 1) + p1) as usize)] - scratch[CFA + (((indx + 1) - p1) as usize)]).abs();
                                        scratch[DELM + ((indx >> 1) as usize)] =
                                            (scratch[CFA + (((indx + 1) + m1) as usize)] - scratch[CFA + (((indx + 1) - m1) as usize)]).abs();
                                    }
                                    cc += 2;
                                    indx += 2;
                                }
                            }
                        }
                    }
                    rr += 1;
                }
            }
            {
                let mut rr: i32 = 8;
                while (rr < (rr1 - 8)) {
                    {
                        {
                            let mut cc: i32 = (8 + (fc(m, rr, 2) & 1));
                            let mut indx: i32 = ((rr * ts) + cc);
                            let mut indx1: i32 = (indx >> 1);
                            while (cc < (cc1 - 8)) {
                                {
                                    let mut crse: f32 = (xmul2f(scratch[CFA + ((indx + m1) as usize)])
                                        / ((eps + scratch[CFA + ((indx) as usize)]) + scratch[CFA + ((indx + m2) as usize)]));
                                    let mut crnw: f32 = (xmul2f(scratch[CFA + ((indx - m1) as usize)])
                                        / ((eps + scratch[CFA + ((indx) as usize)]) + scratch[CFA + ((indx - m2) as usize)]));
                                    let mut crne: f32 = (xmul2f(scratch[CFA + ((indx + p1) as usize)])
                                        / ((eps + scratch[CFA + ((indx) as usize)]) + scratch[CFA + ((indx + p2) as usize)]));
                                    let mut crsw: f32 = (xmul2f(scratch[CFA + ((indx - p1) as usize)])
                                        / ((eps + scratch[CFA + ((indx) as usize)]) + scratch[CFA + ((indx - p2) as usize)]));
                                    let mut rbse: f32;
                                    let mut rbnw: f32;
                                    let mut rbne: f32;
                                    let mut rbsw: f32;
                                    if ((1.0f32 - crse).abs() < arthresh) {
                                        rbse = (scratch[CFA + ((indx) as usize)] * crse);
                                    } else {
                                        rbse = (scratch[CFA + ((indx + m1) as usize)]
                                            + xdiv2f(scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx + m2) as usize)]));
                                    }
                                    if ((1.0f32 - crnw).abs() < arthresh) {
                                        rbnw = (scratch[CFA + ((indx) as usize)] * crnw);
                                    } else {
                                        rbnw = (scratch[CFA + ((indx - m1) as usize)]
                                            + xdiv2f(scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx - m2) as usize)]));
                                    }
                                    if ((1.0f32 - crne).abs() < arthresh) {
                                        rbne = (scratch[CFA + ((indx) as usize)] * crne);
                                    } else {
                                        rbne = (scratch[CFA + ((indx + p1) as usize)]
                                            + xdiv2f(scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx + p2) as usize)]));
                                    }
                                    if ((1.0f32 - crsw).abs() < arthresh) {
                                        rbsw = (scratch[CFA + ((indx) as usize)] * crsw);
                                    } else {
                                        rbsw = (scratch[CFA + ((indx - p1) as usize)]
                                            + xdiv2f(scratch[CFA + ((indx) as usize)] - scratch[CFA + ((indx - p2) as usize)]));
                                    }
                                    let wtse: f32 = (((eps + scratch[DELM + ((indx1) as usize)]) + scratch[DELM + (((indx + m1) >> 1) as usize)])
                                        + scratch[DELM + (((indx + m2) >> 1) as usize)]);
                                    let wtnw: f32 = (((eps + scratch[DELM + ((indx1) as usize)]) + scratch[DELM + (((indx - m1) >> 1) as usize)])
                                        + scratch[DELM + (((indx - m2) >> 1) as usize)]);
                                    let wtne: f32 = (((eps + scratch[CDDIFF + ((indx1) as usize)])
                                        + scratch[CDDIFF + (((indx + p1) >> 1) as usize)])
                                        + scratch[CDDIFF + (((indx + p2) >> 1) as usize)]);
                                    let wtsw: f32 = (((eps + scratch[CDDIFF + ((indx1) as usize)])
                                        + scratch[CDDIFF + (((indx - p1) >> 1) as usize)])
                                        + scratch[CDDIFF + (((indx - p2) >> 1) as usize)]);
                                    scratch[VCD + ((indx1) as usize)] = (((wtse * rbnw) + (wtnw * rbse)) / (wtse + wtnw));
                                    scratch[RBP + ((indx1) as usize)] = (((wtne * rbsw) + (wtsw * rbne)) / (wtne + wtsw));
                                    let rbvarm: f32 = (epssq
                                        + ((gausseven[0_usize]
                                            * (((scratch[DGRB_SQ_M + (((indx - v1) >> 1) as usize)]
                                                + scratch[DGRB_SQ_M + (((indx - 1) >> 1) as usize)])
                                                + scratch[DGRB_SQ_M + (((indx + 1) >> 1) as usize)])
                                                + scratch[DGRB_SQ_M + (((indx + v1) >> 1) as usize)]))
                                            + (gausseven[1_usize]
                                                * (((((((scratch[DGRB_SQ_M + ((((indx - v2) - 1) >> 1) as usize)]
                                                    + scratch[DGRB_SQ_M + ((((indx - v2) + 1) >> 1) as usize)])
                                                    + scratch[DGRB_SQ_M + ((((indx - 2) - v1) >> 1) as usize)])
                                                    + scratch[DGRB_SQ_M + ((((indx + 2) - v1) >> 1) as usize)])
                                                    + scratch[DGRB_SQ_M + ((((indx - 2) + v1) >> 1) as usize)])
                                                    + scratch[DGRB_SQ_M + ((((indx + 2) + v1) >> 1) as usize)])
                                                    + scratch[DGRB_SQ_M + ((((indx + v2) - 1) >> 1) as usize)])
                                                    + scratch[DGRB_SQ_M + ((((indx + v2) + 1) >> 1) as usize)]))));
                                    scratch[DELHV + ((indx1) as usize)] = (rbvarm
                                        / ((epssq
                                            + ((gausseven[0_usize]
                                                * (((scratch[DGRB_SQ_P + (((indx - v1) >> 1) as usize)]
                                                    + scratch[DGRB_SQ_P + (((indx - 1) >> 1) as usize)])
                                                    + scratch[DGRB_SQ_P + (((indx + 1) >> 1) as usize)])
                                                    + scratch[DGRB_SQ_P + (((indx + v1) >> 1) as usize)]))
                                                + (gausseven[1_usize]
                                                    * (((((((scratch[DGRB_SQ_P + ((((indx - v2) - 1) >> 1) as usize)]
                                                        + scratch[DGRB_SQ_P + ((((indx - v2) + 1) >> 1) as usize)])
                                                        + scratch[DGRB_SQ_P + ((((indx - 2) - v1) >> 1) as usize)])
                                                        + scratch[DGRB_SQ_P + ((((indx + 2) - v1) >> 1) as usize)])
                                                        + scratch[DGRB_SQ_P + ((((indx - 2) + v1) >> 1) as usize)])
                                                        + scratch[DGRB_SQ_P + ((((indx + 2) + v1) >> 1) as usize)])
                                                        + scratch[DGRB_SQ_P + ((((indx + v2) - 1) >> 1) as usize)])
                                                        + scratch[DGRB_SQ_P + ((((indx + v2) + 1) >> 1) as usize)]))))
                                            + rbvarm));
                                    if (scratch[RBP + ((indx1) as usize)] < scratch[CFA + ((indx) as usize)]) {
                                        if (xmul2f(scratch[RBP + ((indx1) as usize)]) < scratch[CFA + ((indx) as usize)]) {
                                            scratch[RBP + ((indx1) as usize)] = ulim(
                                                scratch[RBP + ((indx1) as usize)],
                                                scratch[CFA + ((indx - p1) as usize)],
                                                scratch[CFA + ((indx + p1) as usize)],
                                            );
                                        } else {
                                            let pwt: f32 = (xmul2f(scratch[CFA + ((indx) as usize)] - scratch[RBP + ((indx1) as usize)])
                                                / ((eps + scratch[RBP + ((indx1) as usize)]) + scratch[CFA + ((indx) as usize)]));
                                            scratch[RBP + ((indx1) as usize)] = ((pwt * scratch[RBP + ((indx1) as usize)])
                                                + ((1.0f32 - pwt)
                                                    * ulim(
                                                        scratch[RBP + ((indx1) as usize)],
                                                        scratch[CFA + ((indx - p1) as usize)],
                                                        scratch[CFA + ((indx + p1) as usize)],
                                                    )));
                                        }
                                    }
                                    if (scratch[VCD + ((indx1) as usize)] < scratch[CFA + ((indx) as usize)]) {
                                        if (xmul2f(scratch[VCD + ((indx1) as usize)]) < scratch[CFA + ((indx) as usize)]) {
                                            scratch[VCD + ((indx1) as usize)] = ulim(
                                                scratch[VCD + ((indx1) as usize)],
                                                scratch[CFA + ((indx - m1) as usize)],
                                                scratch[CFA + ((indx + m1) as usize)],
                                            );
                                        } else {
                                            let mwt: f32 = (xmul2f(scratch[CFA + ((indx) as usize)] - scratch[VCD + ((indx1) as usize)])
                                                / ((eps + scratch[VCD + ((indx1) as usize)]) + scratch[CFA + ((indx) as usize)]));
                                            scratch[VCD + ((indx1) as usize)] = ((mwt * scratch[VCD + ((indx1) as usize)])
                                                + ((1.0f32 - mwt)
                                                    * ulim(
                                                        scratch[VCD + ((indx1) as usize)],
                                                        scratch[CFA + ((indx - m1) as usize)],
                                                        scratch[CFA + ((indx + m1) as usize)],
                                                    )));
                                        }
                                    }
                                    if (scratch[RBP + ((indx1) as usize)] > clip_pt) {
                                        scratch[RBP + ((indx1) as usize)] = ulim(
                                            scratch[RBP + ((indx1) as usize)],
                                            scratch[CFA + ((indx - p1) as usize)],
                                            scratch[CFA + ((indx + p1) as usize)],
                                        );
                                    }
                                    if (scratch[VCD + ((indx1) as usize)] > clip_pt) {
                                        scratch[VCD + ((indx1) as usize)] = ulim(
                                            scratch[VCD + ((indx1) as usize)],
                                            scratch[CFA + ((indx - m1) as usize)],
                                            scratch[CFA + ((indx + m1) as usize)],
                                        );
                                    }
                                }
                                cc += 2;
                                indx += 2;
                                indx1 += 1;
                            }
                        }
                    }
                    rr += 1;
                }
            }
            {
                let mut rr: i32 = 10;
                while (rr < (rr1 - 10)) {
                    {
                        let mut cc: i32 = (10 + (fc(m, rr, 2) & 1));
                        let mut indx: i32 = ((rr * ts) + cc);
                        let mut indx1: i32 = (indx >> 1);
                        while (cc < (cc1 - 10)) {
                            {
                                let pmwtalt: f32 = xdiv(
                                    (((scratch[DELHV + (((indx - m1) >> 1) as usize)] + scratch[DELHV + (((indx + p1) >> 1) as usize)])
                                        + scratch[DELHV + (((indx - p1) >> 1) as usize)])
                                        + scratch[DELHV + (((indx + m1) >> 1) as usize)]),
                                    2,
                                );
                                if ((0.5f32 - scratch[DELHV + ((indx1) as usize)]).abs() < (0.5f32 - pmwtalt).abs()) {
                                    scratch[DELHV + ((indx1) as usize)] = pmwtalt;
                                }
                                scratch[DELM + ((indx1) as usize)] = xdiv2f(
                                    ((scratch[CFA + ((indx) as usize)]
                                        + (scratch[VCD + ((indx1) as usize)] * (1.0f32 - scratch[DELHV + ((indx1) as usize)])))
                                        + (scratch[RBP + ((indx1) as usize)] * scratch[DELHV + ((indx1) as usize)])),
                                );
                            }
                            cc += 2;
                            indx += 2;
                            indx1 += 1;
                        }
                    }
                    rr += 1;
                }
            }
            {
                let mut rr: i32 = 12;
                while (rr < (rr1 - 12)) {
                    {
                        let mut cc: i32 = (12 + (fc(m, rr, 2) & 1));
                        let mut indx: i32 = ((rr * ts) + cc);
                        let mut indx1: i32 = (indx >> 1);
                        while (cc < (cc1 - 12)) {
                            {
                                if ((0.5f32 - scratch[DELHV + ((indx >> 1) as usize)]).abs()
                                    < (0.5f32 - scratch[HVWT + ((indx >> 1) as usize)]).abs())
                                {
                                    cc += 2;
                                    indx += 2;
                                    indx1 += 1;
                                    continue;
                                }
                                let cru: f32 = ((scratch[CFA + ((indx - v1) as usize)] * 2.0f32)
                                    / ((eps + scratch[DELM + ((indx1) as usize)]) + scratch[DELM + ((indx1 - v1) as usize)]));
                                let crd: f32 = ((scratch[CFA + ((indx + v1) as usize)] * 2.0f32)
                                    / ((eps + scratch[DELM + ((indx1) as usize)]) + scratch[DELM + ((indx1 + v1) as usize)]));
                                let crl: f32 = ((scratch[CFA + ((indx - 1) as usize)] * 2.0f32)
                                    / ((eps + scratch[DELM + ((indx1) as usize)]) + scratch[DELM + ((indx1 - 1) as usize)]));
                                let crr: f32 = ((scratch[CFA + ((indx + 1) as usize)] * 2.0f32)
                                    / ((eps + scratch[DELM + ((indx1) as usize)]) + scratch[DELM + ((indx1 + 1) as usize)]));
                                let mut gu: f32;
                                let mut gd: f32;
                                let mut gl: f32;
                                let mut gr: f32;
                                if ((1.0f32 - cru).abs() < arthresh) {
                                    gu = (scratch[DELM + ((indx1) as usize)] * cru);
                                } else {
                                    gu = (scratch[CFA + ((indx - v1) as usize)]
                                        + xdiv2f(scratch[DELM + ((indx1) as usize)] - scratch[DELM + ((indx1 - v1) as usize)]));
                                }
                                if ((1.0f32 - crd).abs() < arthresh) {
                                    gd = (scratch[DELM + ((indx1) as usize)] * crd);
                                } else {
                                    gd = (scratch[CFA + ((indx + v1) as usize)]
                                        + xdiv2f(scratch[DELM + ((indx1) as usize)] - scratch[DELM + ((indx1 + v1) as usize)]));
                                }
                                if ((1.0f32 - crl).abs() < arthresh) {
                                    gl = (scratch[DELM + ((indx1) as usize)] * crl);
                                } else {
                                    gl = (scratch[CFA + ((indx - 1) as usize)]
                                        + xdiv2f(scratch[DELM + ((indx1) as usize)] - scratch[DELM + ((indx1 - 1) as usize)]));
                                }
                                if ((1.0f32 - crr).abs() < arthresh) {
                                    gr = (scratch[DELM + ((indx1) as usize)] * crr);
                                } else {
                                    gr = (scratch[CFA + ((indx + 1) as usize)]
                                        + xdiv2f(scratch[DELM + ((indx1) as usize)] - scratch[DELM + ((indx1 + 1) as usize)]));
                                }
                                let mut Gintv: f32 = (((scratch[DIR0 + ((indx - v1) as usize)] * gd)
                                    + (scratch[DIR0 + ((indx + v1) as usize)] * gu))
                                    / (scratch[DIR0 + ((indx + v1) as usize)] + scratch[DIR0 + ((indx - v1) as usize)]));
                                let mut Ginth: f32 = (((scratch[DIR1 + ((indx - 1) as usize)] * gr) + (scratch[DIR1 + ((indx + 1) as usize)] * gl))
                                    / (scratch[DIR1 + ((indx - 1) as usize)] + scratch[DIR1 + ((indx + 1) as usize)]));
                                if (Gintv < scratch[DELM + ((indx1) as usize)]) {
                                    if ((2_f32 * Gintv) < scratch[DELM + ((indx1) as usize)]) {
                                        Gintv = ulim(Gintv, scratch[CFA + ((indx - v1) as usize)], scratch[CFA + ((indx + v1) as usize)]);
                                    } else {
                                        let mut vwt: f32 = ((2.0f32 * (scratch[DELM + ((indx1) as usize)] - Gintv))
                                            / ((eps + Gintv) + scratch[DELM + ((indx1) as usize)]));
                                        Gintv = ((vwt * Gintv)
                                            + ((1.0f32 - vwt)
                                                * ulim(Gintv, scratch[CFA + ((indx - v1) as usize)], scratch[CFA + ((indx + v1) as usize)])));
                                    }
                                }
                                if (Ginth < scratch[DELM + ((indx1) as usize)]) {
                                    if ((2_f32 * Ginth) < scratch[DELM + ((indx1) as usize)]) {
                                        Ginth = ulim(Ginth, scratch[CFA + ((indx - 1) as usize)], scratch[CFA + ((indx + 1) as usize)]);
                                    } else {
                                        let hwt: f32 = ((2.0f32 * (scratch[DELM + ((indx1) as usize)] - Ginth))
                                            / ((eps + Ginth) + scratch[DELM + ((indx1) as usize)]));
                                        Ginth = ((hwt * Ginth)
                                            + ((1.0f32 - hwt)
                                                * ulim(Ginth, scratch[CFA + ((indx - 1) as usize)], scratch[CFA + ((indx + 1) as usize)])));
                                    }
                                }
                                if (Ginth > clip_pt) {
                                    Ginth = ulim(Ginth, scratch[CFA + ((indx - 1) as usize)], scratch[CFA + ((indx + 1) as usize)]);
                                }
                                if (Gintv > clip_pt) {
                                    Gintv = ulim(Gintv, scratch[CFA + ((indx - v1) as usize)], scratch[CFA + ((indx + v1) as usize)]);
                                }
                                scratch[GREEN + ((indx) as usize)] =
                                    ((Ginth * (1.0f32 - scratch[HVWT + ((indx1) as usize)])) + (Gintv * scratch[HVWT + ((indx1) as usize)]));
                                scratch[VCD_ALT + ((indx >> 1) as usize)] = (scratch[GREEN + ((indx) as usize)] - scratch[CFA + ((indx) as usize)]);
                            }
                            cc += 2;
                            indx += 2;
                            indx1 += 1;
                        }
                    }
                    rr += 1;
                }
            }
            {
                let mut rr: i32 = (13 - ey);
                while (rr < (rr1 - 12)) {
                    {
                        let mut indx1: i32 = ((((rr * ts) + 13) - ex) >> 1);
                        while (indx1 < ((((rr * ts) + cc1) - 12) >> 1)) {
                            {
                                scratch[VCD_ALT + 12800 + ((indx1) as usize)] = scratch[VCD_ALT + ((indx1) as usize)];
                                scratch[VCD_ALT + ((indx1) as usize)] = (0 as f32);
                            }
                            indx1 += 1;
                        }
                    }
                    rr += 2;
                }
            }
            {
                let mut rr: i32 = 14;
                while (rr < (rr1 - 14)) {
                    {
                        let mut cc: i32 = (14 + (fc(m, rr, 2) & 1));
                        let mut indx: i32 = ((rr * ts) + cc);
                        let mut c: i32 = (1 - (fc(m, rr, cc) / 2));
                        while (cc < (cc1 - 14)) {
                            {
                                let wtnw: f32 = (1.0f32
                                    / (((eps
                                        + (scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - m1) >> 1) as usize)]
                                            - scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + m1) >> 1) as usize)])
                                            .abs())
                                        + (scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - m1) >> 1) as usize)]
                                            - scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - m3) >> 1) as usize)])
                                            .abs())
                                        + (scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + m1) >> 1) as usize)]
                                            - scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - m3) >> 1) as usize)])
                                            .abs()));
                                let wtne: f32 = (1.0f32
                                    / (((eps
                                        + (scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + p1) >> 1) as usize)]
                                            - scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - p1) >> 1) as usize)])
                                            .abs())
                                        + (scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + p1) >> 1) as usize)]
                                            - scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + p3) >> 1) as usize)])
                                            .abs())
                                        + (scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - p1) >> 1) as usize)]
                                            - scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + p3) >> 1) as usize)])
                                            .abs()));
                                let wtsw: f32 = (1.0f32
                                    / (((eps
                                        + (scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - p1) >> 1) as usize)]
                                            - scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + p1) >> 1) as usize)])
                                            .abs())
                                        + (scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - p1) >> 1) as usize)]
                                            - scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + m3) >> 1) as usize)])
                                            .abs())
                                        + (scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + p1) >> 1) as usize)]
                                            - scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - p3) >> 1) as usize)])
                                            .abs()));
                                let wtse: f32 = (1.0f32
                                    / (((eps
                                        + (scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + m1) >> 1) as usize)]
                                            - scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - m1) >> 1) as usize)])
                                            .abs())
                                        + (scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + m1) >> 1) as usize)]
                                            - scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - p3) >> 1) as usize)])
                                            .abs())
                                        + (scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - m1) >> 1) as usize)]
                                            - scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + m3) >> 1) as usize)])
                                            .abs()));
                                scratch[VCD_ALT + ((c) as usize) * 12800 + ((indx >> 1) as usize)] = (((((wtnw
                                    * ((((1.325f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - m1) >> 1) as usize)])
                                        - (0.175f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - m3) >> 1) as usize)]))
                                        - (0.075f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + ((((indx - m1) - 2) >> 1) as usize)]))
                                        - (0.075f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + ((((indx - m1) - v2) >> 1) as usize)])))
                                    + (wtne
                                        * ((((1.325f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + p1) >> 1) as usize)])
                                            - (0.175f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + p3) >> 1) as usize)]))
                                            - (0.075f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + ((((indx + p1) + 2) >> 1) as usize)]))
                                            - (0.075f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + ((((indx + p1) + v2) >> 1) as usize)]))))
                                    + (wtsw
                                        * ((((1.325f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - p1) >> 1) as usize)])
                                            - (0.175f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx - p3) >> 1) as usize)]))
                                            - (0.075f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + ((((indx - p1) - 2) >> 1) as usize)]))
                                            - (0.075f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + ((((indx - p1) - v2) >> 1) as usize)]))))
                                    + (wtse
                                        * ((((1.325f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + m1) >> 1) as usize)])
                                            - (0.175f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + (((indx + m3) >> 1) as usize)]))
                                            - (0.075f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + ((((indx + m1) + 2) >> 1) as usize)]))
                                            - (0.075f32 * scratch[VCD_ALT + ((c) as usize) * 12800 + ((((indx + m1) + v2) >> 1) as usize)]))))
                                    / (((wtnw + wtne) + wtsw) + wtse));
                            }
                            cc += 2;
                            indx += 2;
                        }
                    }
                    rr += 1;
                }
            }
            {
                let mut rr: i32 = 16;
                while (rr < (rr1 - 16)) {
                    {
                        let mut row: i32 = (rr + top);
                        let mut col: i32 = (left + 16);
                        let mut indx: i32 = ((rr * ts) + 16);
                        if ((fc(m, rr, 2) & 1) == 1) {
                            {
                                while (indx < ((((rr * ts) + cc1) - 16) - (cc1 & 1))) {
                                    {
                                        if ((col < width) && (row < height)) {
                                            let temp: f32 = (1.0f32
                                                / ((((scratch[HVWT + (((indx - v1) >> 1) as usize)] + 2.0f32)
                                                    - scratch[HVWT + (((indx + 1) >> 1) as usize)])
                                                    - scratch[HVWT + (((indx - 1) >> 1) as usize)])
                                                    + scratch[HVWT + (((indx + v1) >> 1) as usize)]));
                                            out[((row - (top + 16)) * width + col) as usize][0] = clampnan(
                                                (scratch[GREEN + ((indx) as usize)]
                                                    - (((((scratch[HVWT + (((indx - v1) >> 1) as usize)]
                                                        * scratch[VCD_ALT + (((indx - v1) >> 1) as usize)])
                                                        + ((1.0f32 - scratch[HVWT + (((indx + 1) >> 1) as usize)])
                                                            * scratch[VCD_ALT + (((indx + 1) >> 1) as usize)]))
                                                        + ((1.0f32 - scratch[HVWT + (((indx - 1) >> 1) as usize)])
                                                            * scratch[VCD_ALT + (((indx - 1) >> 1) as usize)]))
                                                        + (scratch[HVWT + (((indx + v1) >> 1) as usize)]
                                                            * scratch[VCD_ALT + (((indx + v1) >> 1) as usize)]))
                                                        * temp)),
                                                0.0f32,
                                                1.0f32,
                                            );
                                            out[((row - (top + 16)) * width + col) as usize][2] = clampnan(
                                                (scratch[GREEN + ((indx) as usize)]
                                                    - (((((scratch[HVWT + (((indx - v1) >> 1) as usize)]
                                                        * scratch[VCD_ALT + 12800 + (((indx - v1) >> 1) as usize)])
                                                        + ((1.0f32 - scratch[HVWT + (((indx + 1) >> 1) as usize)])
                                                            * scratch[VCD_ALT + 12800 + (((indx + 1) >> 1) as usize)]))
                                                        + ((1.0f32 - scratch[HVWT + (((indx - 1) >> 1) as usize)])
                                                            * scratch[VCD_ALT + 12800 + (((indx - 1) >> 1) as usize)]))
                                                        + (scratch[HVWT + (((indx + v1) >> 1) as usize)]
                                                            * scratch[VCD_ALT + 12800 + (((indx + v1) >> 1) as usize)]))
                                                        * temp)),
                                                0.0f32,
                                                1.0f32,
                                            );
                                        }
                                        indx += 1;
                                        col += 1;
                                        if ((col < width) && (row < height)) {
                                            out[((row - (top + 16)) * width + col) as usize][0] = clampnan(
                                                (scratch[GREEN + ((indx) as usize)] - scratch[VCD_ALT + ((indx >> 1) as usize)]),
                                                0.0f32,
                                                1.0f32,
                                            );
                                            out[((row - (top + 16)) * width + col) as usize][2] = clampnan(
                                                (scratch[GREEN + ((indx) as usize)] - scratch[VCD_ALT + 12800 + ((indx >> 1) as usize)]),
                                                0.0f32,
                                                1.0f32,
                                            );
                                        }
                                    }
                                    indx += 1;
                                    col += 1;
                                }
                            }
                            if ((cc1 & 1) != 0) && ((col < width) && (row < height)) {
                                let temp: f32 = (1.0f32
                                    / ((((scratch[HVWT + (((indx - v1) >> 1) as usize)] + 2.0f32) - scratch[HVWT + (((indx + 1) >> 1) as usize)])
                                        - scratch[HVWT + (((indx - 1) >> 1) as usize)])
                                        + scratch[HVWT + (((indx + v1) >> 1) as usize)]));
                                out[((row - (top + 16)) * width + col) as usize][0] = clampnan(
                                    (scratch[GREEN + ((indx) as usize)]
                                        - (((((scratch[HVWT + (((indx - v1) >> 1) as usize)] * scratch[VCD_ALT + (((indx - v1) >> 1) as usize)])
                                            + ((1.0f32 - scratch[HVWT + (((indx + 1) >> 1) as usize)])
                                                * scratch[VCD_ALT + (((indx + 1) >> 1) as usize)]))
                                            + ((1.0f32 - scratch[HVWT + (((indx - 1) >> 1) as usize)])
                                                * scratch[VCD_ALT + (((indx - 1) >> 1) as usize)]))
                                            + (scratch[HVWT + (((indx + v1) >> 1) as usize)] * scratch[VCD_ALT + (((indx + v1) >> 1) as usize)]))
                                            * temp)),
                                    0.0f32,
                                    1.0f32,
                                );
                                out[((row - (top + 16)) * width + col) as usize][2] = clampnan(
                                    (scratch[GREEN + ((indx) as usize)]
                                        - (((((scratch[HVWT + (((indx - v1) >> 1) as usize)]
                                            * scratch[VCD_ALT + 12800 + (((indx - v1) >> 1) as usize)])
                                            + ((1.0f32 - scratch[HVWT + (((indx + 1) >> 1) as usize)])
                                                * scratch[VCD_ALT + 12800 + (((indx + 1) >> 1) as usize)]))
                                            + ((1.0f32 - scratch[HVWT + (((indx - 1) >> 1) as usize)])
                                                * scratch[VCD_ALT + 12800 + (((indx - 1) >> 1) as usize)]))
                                            + (scratch[HVWT + (((indx + v1) >> 1) as usize)]
                                                * scratch[VCD_ALT + 12800 + (((indx + v1) >> 1) as usize)]))
                                            * temp)),
                                    0.0f32,
                                    1.0f32,
                                );
                            }
                        } else {
                            {
                                while (indx < ((((rr * ts) + cc1) - 16) - (cc1 & 1))) {
                                    {
                                        if ((col < width) && (row < height)) {
                                            out[((row - (top + 16)) * width + col) as usize][0] = clampnan(
                                                (scratch[GREEN + ((indx) as usize)] - scratch[VCD_ALT + ((indx >> 1) as usize)]),
                                                0.0f32,
                                                1.0f32,
                                            );
                                            out[((row - (top + 16)) * width + col) as usize][2] = clampnan(
                                                (scratch[GREEN + ((indx) as usize)] - scratch[VCD_ALT + 12800 + ((indx >> 1) as usize)]),
                                                0.0f32,
                                                1.0f32,
                                            );
                                        }
                                        indx += 1;
                                        col += 1;
                                        if ((col < width) && (row < height)) {
                                            let temp: f32 = (1.0f32
                                                / ((((scratch[HVWT + (((indx - v1) >> 1) as usize)] + 2.0f32)
                                                    - scratch[HVWT + (((indx + 1) >> 1) as usize)])
                                                    - scratch[HVWT + (((indx - 1) >> 1) as usize)])
                                                    + scratch[HVWT + (((indx + v1) >> 1) as usize)]));
                                            out[((row - (top + 16)) * width + col) as usize][0] = clampnan(
                                                (scratch[GREEN + ((indx) as usize)]
                                                    - (((((scratch[HVWT + (((indx - v1) >> 1) as usize)]
                                                        * scratch[VCD_ALT + (((indx - v1) >> 1) as usize)])
                                                        + ((1.0f32 - scratch[HVWT + (((indx + 1) >> 1) as usize)])
                                                            * scratch[VCD_ALT + (((indx + 1) >> 1) as usize)]))
                                                        + ((1.0f32 - scratch[HVWT + (((indx - 1) >> 1) as usize)])
                                                            * scratch[VCD_ALT + (((indx - 1) >> 1) as usize)]))
                                                        + (scratch[HVWT + (((indx + v1) >> 1) as usize)]
                                                            * scratch[VCD_ALT + (((indx + v1) >> 1) as usize)]))
                                                        * temp)),
                                                0.0f32,
                                                1.0f32,
                                            );
                                            out[((row - (top + 16)) * width + col) as usize][2] = clampnan(
                                                (scratch[GREEN + ((indx) as usize)]
                                                    - (((((scratch[HVWT + (((indx - v1) >> 1) as usize)]
                                                        * scratch[VCD_ALT + 12800 + (((indx - v1) >> 1) as usize)])
                                                        + ((1.0f32 - scratch[HVWT + (((indx + 1) >> 1) as usize)])
                                                            * scratch[VCD_ALT + 12800 + (((indx + 1) >> 1) as usize)]))
                                                        + ((1.0f32 - scratch[HVWT + (((indx - 1) >> 1) as usize)])
                                                            * scratch[VCD_ALT + 12800 + (((indx - 1) >> 1) as usize)]))
                                                        + (scratch[HVWT + (((indx + v1) >> 1) as usize)]
                                                            * scratch[VCD_ALT + 12800 + (((indx + v1) >> 1) as usize)]))
                                                        * temp)),
                                                0.0f32,
                                                1.0f32,
                                            );
                                        }
                                    }
                                    indx += 1;
                                    col += 1;
                                }
                            }
                            if ((cc1 & 1) != 0) && ((col < width) && (row < height)) {
                                out[((row - (top + 16)) * width + col) as usize][0] =
                                    clampnan((scratch[GREEN + ((indx) as usize)] - scratch[VCD_ALT + ((indx >> 1) as usize)]), 0.0f32, 1.0f32);
                                out[((row - (top + 16)) * width + col) as usize][2] = clampnan(
                                    (scratch[GREEN + ((indx) as usize)] - scratch[VCD_ALT + 12800 + ((indx >> 1) as usize)]),
                                    0.0f32,
                                    1.0f32,
                                );
                            }
                        }
                    }
                    rr += 1;
                }
            }
            {
                let mut rr: i32 = 16;
                while (rr < (rr1 - 16)) {
                    {
                        let row: i32 = (rr + top);
                        {
                            let mut cc: i32 = 16;
                            while (cc < (cc1 - 16)) {
                                {
                                    let col: i32 = (cc + left);
                                    let indx: i32 = ((rr * ts) + cc);
                                    if ((col < width) && (row < height)) {
                                        out[((row - (top + 16)) * width + col) as usize][1] =
                                            clampnan(scratch[GREEN + ((indx) as usize)], 0.0f32, 1.0f32);
                                    }
                                }
                                cc += 1;
                            }
                        }
                    }
                    rr += 1;
                }
            }
            left += ts - 32;
        }
    };
    if m.w < 128 {
        // Truncated first tiles retain scratch boundary values from the previous
        // band in the faithful scalar port. Keep that order for narrow crops.
        let mut scratch = new_scratch();
        for band in out.chunks_mut(m.w * 128).enumerate() {
            process_band(&mut scratch, band);
        }
    } else {
        out.par_chunks_mut(m.w * 128).enumerate().for_each_init(new_scratch, process_band);
    }
    Rgb32f { width: m.w, height: m.h, data: out }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upstream_vectors() {
        let (w, h) = (255, 251);
        for pat in ["RGGB", "BGGR", "GRBG", "GBRG"] {
            let cfa = crate::Cfa::bayer(pat).unwrap();
            let data: Vec<f32> =
                (0u32..(w * h) as u32).map(|i| ((i.wrapping_mul(1664525).wrapping_add(1013904223) >> 8) & 65535) as f32 / 65536.0).collect();
            let out = amaze(&Mosaic { w, h, data: &data, cfa: &cfa });
            let actual: Vec<f32> = out
                .data
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    let (x, y) = (i % w, i / w);
                    x % 17 < 2 || y % 17 < 2 || (126..=130).contains(&x) || (126..=130).contains(&y)
                })
                .flat_map(|(_, p)| p.iter().copied())
                .collect();
            assert_eq!(actual.len(), 49455);
            crate::test_vectors::compare(&format!("amaze/{pat}.f32"), &actual, 2e-6);
            // RT clamps negative output; darktable and our scene-linear port preserve it.
            crate::test_vectors::compare(&format!("amaze/{pat}-rt.f32"), &actual, 0.19);
            let clamped: Vec<_> = actual.iter().map(|v| v.max(0.0)).collect();
            crate::test_vectors::compare(&format!("amaze/{pat}-rt.f32"), &clamped, 2e-6);
        }
    }
}
