//! Bilateral filtering follows Tomasi–Manduchi (ICCV 1998), a bounded 5×5 kernel.
//! Palette assignment reuses the pinned vtracer Oklab implementation already in T1.
//! Resampling weights bilinear taps by colour proximity to the nearest source sample;
//! alpha is interpolated separately, and invisible RGB never contributes to visible colour.
use super::{Options, Stats, check};
use crate::{Error, Fitter, Image, Params, Preset, colorfit::oklab::Oklab, vc::Color};
use std::collections::BTreeMap;

fn lab(c: [u8; 3]) -> Oklab {
    Oklab::from_color(&Color::new(c[0], c[1], c[2]))
}
fn rgb(p: &[u8]) -> [u8; 3] {
    [p[0], p[1], p[2]]
}
fn difference(a: &[u8], b: &[u8]) -> f64 {
    (0..3).map(|i| (f64::from(a[i]) - f64::from(b[i])).powi(2)).sum()
}

pub(super) struct Prepared {
    pub pixels: Vec<u8>,
    pub continuous: Vec<u8>,
}

pub(super) fn prepare(image: Image<'_>, params: &Params, opts: &Options, scale: u32, stats: &mut Stats, cancel: &impl Fn() -> bool) -> Result<Prepared, Error> {
    let (w, h) = (image.width as usize, image.height as usize);
    let mut pixels = image.rgba.to_vec();
    // Apply visibility before preprocessing, so ignored/hidden RGB cannot pollute the palette.
    if opts.prefilter || opts.aa_palette || opts.small_holes || scale > 1 {
        for row in pixels.chunks_exact_mut(w * 4) {
            check(cancel)?;
            for p in row.as_chunks_mut::<4>().0 {
                if !params.use_alpha {
                    p[3] = 255;
                }
                if params.ignore_color == Some(rgb(p)) {
                    p[3] = 0;
                }
            }
        }
    }
    if opts.prefilter {
        pixels = bilateral(&pixels, w, h, opts.filter_sigma, stats, cancel)?;
    }
    // Hole processing precedes palette quantization, retaining the original contrast evidence.
    if opts.small_holes && opts.hole_area > 0 {
        holes(&mut pixels, w, h, opts.hole_area, stats, cancel)?;
    }
    let aa_enabled = opts.aa_palette && !matches!(params.preset, Preset::BlackWhite | Preset::Silhouette);
    let continuous = if aa_enabled { pixels.clone() } else { Vec::new() };
    if aa_enabled {
        palette(&mut pixels, w, h, params, stats, cancel)?;
    }
    let pixels = if scale > 1 {
        if params.fitter == Fitter::Pixel {
            let factor = scale as usize;
            let mut output = vec![0; w * h * factor * factor * 4];
            for y in 0..h * factor {
                check(cancel)?;
                for x in 0..w * factor {
                    let i = (y * w * factor + x) * 4;
                    let j = ((y / factor) * w + x / factor) * 4;
                    output[i..i + 4].copy_from_slice(&pixels[j..j + 4]);
                }
            }
            output
        } else {
            upscale(&pixels, w, h, scale as usize, cancel)?
        }
    } else {
        pixels
    };
    Ok(Prepared { pixels, continuous })
}
fn bilateral(input: &[u8], w: usize, h: usize, sigma: f64, stats: &mut Stats, cancel: &impl Fn() -> bool) -> Result<Vec<u8>, Error> {
    let mut out = input.to_vec();
    for y in 0..h {
        check(cancel)?;
        for x in 0..w {
            let i = (y * w + x) * 4;
            let p = &input[i..i + 4];
            if p[3] < 128 {
                continue;
            }
            let mut sum = [0.; 3];
            let mut weight = 0.;
            for yy in y.saturating_sub(2)..=(y + 2).min(h - 1) {
                for xx in x.saturating_sub(2)..=(x + 2).min(w - 1) {
                    let j = (yy * w + xx) * 4;
                    let q = &input[j..j + 4];
                    if q[3] != p[3] {
                        continue;
                    }
                    let distance = x.abs_diff(xx).pow(2) + y.abs_diff(yy).pow(2);
                    let a = (-(distance as f64) / 4. - difference(p, q) / (2. * sigma * sigma)).exp();
                    for c in 0..3 {
                        sum[c] += a * f64::from(q[c]);
                    }
                    weight += a;
                }
            }
            for c in 0..3 {
                out[i + c] = (sum[c] / weight).round() as u8;
            }
            stats.filtered_pixels += usize::from(out[i..i + 3] != input[i..i + 3]);
        }
    }
    Ok(out)
}

fn palette(pixels: &mut [u8], w: usize, h: usize, params: &Params, stats: &mut Stats, cancel: &impl Fn() -> bool) -> Result<(), Error> {
    // At most 32768 bins, independent of input colour count. No unbounded RGB histogram.
    let mut histogram: BTreeMap<[u8; 3], (usize, [u64; 3])> = BTreeMap::new();
    for y in 0..h {
        check(cancel)?;
        for x in 0..w {
            let i = (y * w + x) * 4;
            let p = &pixels[i..i + 4];
            if p[3] != 255 {
                continue;
            }
            let mut stable = true;
            for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let q = &pixels[(yy * w + xx) * 4..][..4];
                    if q[3] != 255 || (0..3).any(|c| p[c].abs_diff(q[c]) > 12) {
                        stable = false;
                    }
                }
            }
            if stable {
                let entry = histogram.entry([p[0] >> 3, p[1] >> 3, p[2] >> 3]).or_default();
                entry.0 += 1;
                for (sum, &v) in entry.1.iter_mut().zip(p) {
                    *sum += u64::from(v);
                }
            }
        }
    }
    let mut colors: Vec<_> = histogram.into_iter().map(|(key, (n, sum))| (n, key, sum.map(|s| (s / n as u64) as u8))).collect();
    colors.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut chosen = params.palette.clone();
    if chosen.is_empty() {
        for (_, _, c) in colors {
            let target = lab(c);
            if chosen.iter().all(|&a| lab(a).distance_squared(&target) > 0.025_f64.powi(2)) {
                chosen.push(c);
            }
            if chosen.len() == params.max_colors {
                break;
            }
        }
    }
    // Tiny/no-core images retain T1's palette rather than inventing a background colour.
    if chosen.is_empty() {
        return Ok(());
    }
    let labs: Vec<_> = chosen.iter().copied().map(lab).collect();
    for row in pixels.chunks_exact_mut(w * 4) {
        check(cancel)?;
        for p in row.as_chunks_mut::<4>().0 {
            if p[3] < 128 {
                continue;
            }
            let target = lab(rgb(p));
            let index =
                labs.iter().enumerate().min_by(|(_, a), (_, b)| a.distance_squared(&target).total_cmp(&b.distance_squared(&target))).map_or(0, |(i, _)| i);
            stats.palette_pixels += usize::from(p[..3] != chosen[index]);
            p[..3].copy_from_slice(&chosen[index]);
        }
    }
    stats.palette = chosen;
    Ok(())
}

fn upscale(input: &[u8], w: usize, h: usize, scale: usize, cancel: &impl Fn() -> bool) -> Result<Vec<u8>, Error> {
    let (ow, oh) = (w * scale, h * scale);
    let mut out = vec![0; ow * oh * 4];
    for y in 0..oh {
        check(cancel)?;
        let sy = ((y as f64 + 0.5) / scale as f64 - 0.5).clamp(0., (h - 1) as f64);
        for x in 0..ow {
            let sx = ((x as f64 + 0.5) / scale as f64 - 0.5).clamp(0., (w - 1) as f64);
            let near = &input[(sy.round() as usize * w + sx.round() as usize) * 4..][..4];
            let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
            let (fx, fy) = (sx.fract(), sy.fract());
            let mut sum = [0.; 3];
            let (mut weight, mut alpha) = (0., 0.);
            for (xx, yy, spatial) in [
                (x0, y0, (1. - fx) * (1. - fy)),
                ((x0 + 1).min(w - 1), y0, fx * (1. - fy)),
                (x0, (y0 + 1).min(h - 1), (1. - fx) * fy),
                ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1), fx * fy),
            ] {
                let p = &input[(yy * w + xx) * 4..][..4];
                alpha += spatial * f64::from(p[3]);
                let range = (-difference(near, p) / (2. * 24_f64.powi(2))).exp();
                let a = spatial * range * f64::from(p[3]) / 255.;
                for c in 0..3 {
                    sum[c] += a * f64::from(p[c]);
                }
                weight += a;
            }
            let i = (y * ow + x) * 4;
            for c in 0..3 {
                out[i + c] = if weight > 1e-12 { (sum[c] / weight).round() as u8 } else { near[c] };
            }
            out[i + 3] = alpha.round() as u8;
        }
    }
    Ok(out)
}

fn holes(pixels: &mut [u8], w: usize, h: usize, max_area: usize, stats: &mut Stats, cancel: &impl Fn() -> bool) -> Result<(), Error> {
    let mut visited = vec![false; w * h];
    let mut queue = Vec::new();
    for start in 0..w * h {
        if start % w == 0 {
            check(cancel)?;
        }
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let value: [u8; 4] = [pixels[start * 4], pixels[start * 4 + 1], pixels[start * 4 + 2], pixels[start * 4 + 3]];
        if value[3] != 255 {
            continue;
        } // Transparent counters are always preserved.
        queue.clear();
        queue.push(start);
        let (mut head, mut edge, mut boundary, mut uniform) = (0, false, None, true);
        while head < queue.len() {
            if head % 4096 == 0 {
                check(cancel)?;
            }
            let i = queue[head];
            head += 1;
            let (x, y) = (i % w, i / w);
            edge |= x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            for next in
                [x.checked_sub(1).map(|xx| y * w + xx), (x + 1 < w).then_some(i + 1), y.checked_sub(1).map(|yy| yy * w + x), (y + 1 < h).then_some(i + w)]
                    .into_iter()
                    .flatten()
            {
                let q = &pixels[next * 4..][..4];
                if q == value {
                    if !visited[next] {
                        visited[next] = true;
                        queue.push(next);
                    }
                } else if let Some(b) = boundary {
                    uniform &= b == [q[0], q[1], q[2], q[3]];
                } else {
                    boundary = Some([q[0], q[1], q[2], q[3]]);
                }
            }
        }
        if !edge
            && uniform
            && queue.len() <= max_area
            && let Some(b) = boundary
            && b[3] == 255
            && (0..3).all(|c| b[c].abs_diff(value[c]) <= 12)
        {
            for &i in &queue {
                pixels[i * 4..i * 4 + 4].copy_from_slice(&b);
            }
            stats.filled_holes += 1;
        }
    }
    Ok(())
}
