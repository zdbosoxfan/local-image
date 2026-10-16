//! Cancellable binary admission. Bradley–Roth comparison follows T1's
//! frontend/binary.rs (vtracer 928ed0a6f654408e28fb741b6133d4c456bd0160).
// Copyright (c) 2024 TSANG, Hao Fung, vtracer contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
use super::Control;
use crate::{Error, Image, Params};

pub(super) fn foreground(image: Image<'_>, p: &Params, c: &mut Control<'_, impl Fn() -> bool>) -> Result<Vec<bool>, Error> {
    let (w, h) = (image.width as usize, image.height as usize);
    let n = w * h;
    let mut gray = Vec::with_capacity(n);
    let mut admitted = Vec::with_capacity(n);
    for (i, px) in image.rgba.as_chunks::<4>().0.iter().enumerate() {
        if i.is_multiple_of(1024) {
            c.work(1024)?;
        }
        let on = (!p.use_alpha || px[3] >= 128) && p.ignore_color != Some([px[0], px[1], px[2]]);
        admitted.push(on);
        gray.push(if on { (u32::from(px[0]) + u32::from(px[1]) + u32::from(px[2])) / 3 } else { 255 });
    }
    if !p.adaptive {
        for i in 0..n {
            if i.is_multiple_of(1024) {
                c.work(1024)?;
            }
            admitted[i] &= gray[i] < u32::from(p.threshold);
        }
        return Ok(admitted);
    }
    let stride = w + 1;
    let mut sat = vec![0u64; stride * (h + 1)];
    for y in 0..h {
        let mut row = 0;
        for x in 0..w {
            if (y * w + x).is_multiple_of(1024) {
                c.work(1024)?;
            }
            row += u64::from(gray[y * w + x]);
            sat[(y + 1) * stride + x + 1] = sat[y * stride + x + 1] + row;
        }
    }
    let side = if p.adaptive_window == 0 { (w.min(h) / 8).max(1) } else { p.adaptive_window as usize };
    let half = side / 2;
    let factor = 1. - p.adaptive_sensitivity / 100.;
    for y in 0..h {
        let (y0, y1) = (y.saturating_sub(half), (y + half + 1).min(h));
        for x in 0..w {
            let i = y * w + x;
            if i.is_multiple_of(1024) {
                c.work(1024)?;
            }
            let (x0, x1) = (x.saturating_sub(half), (x + half + 1).min(w));
            let sum = sat[y1 * stride + x1] + sat[y0 * stride + x0] - sat[y1 * stride + x0] - sat[y0 * stride + x1];
            admitted[i] &= (gray[i] as f64) <= sum as f64 / ((x1 - x0) * (y1 - y0)) as f64 * factor;
        }
    }
    Ok(admitted)
}
