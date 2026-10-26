//! Gradient mixing and bounded linear approximations for paint tools and vector writers.

use photocraft_doc::GradientMethod;

use crate::{midpoint_remap, sample_opacity};

/// Straight RGBA mixed in display RGB or OKLab; alpha remains linear.
fn mix(method: GradientMethod, a: [f32; 4], b: [f32; 4], u: f32) -> [f32; 4] {
    let mut out = std::array::from_fn(|ch| a[ch] + (b[ch] - a[ch]) * u);
    if method == GradientMethod::Perceptual && u > 0.0 && u < 1.0 {
        use li_color::{srgb_perceptual, transfer};
        let decode = |c: [f32; 4]| srgb_perceptual::linear_srgb_to_oklab_f64(std::array::from_fn(|ch| transfer::srgb_to_linear_f64(f64::from(c[ch]))));
        let (p, q) = (decode(a), decode(b));
        let lab = std::array::from_fn(|ch| p[ch] + (q[ch] - p[ch]) * f64::from(u));
        let rgb = srgb_perceptual::oklab_to_linear_srgb_f64(lab).map(|v| {
            let v = transfer::linear_to_srgb_f64(v);
            if v.is_finite() { v.clamp(0.0, 1.0) as f32 } else { 0.0 }
        });
        out[..3].copy_from_slice(&rgb);
    }
    out
}

/// Colour stops with midpoints at `t`, sorted by location. An exact hard stop takes its
/// left colour; immediately after it takes the last coincident colour.
pub(crate) fn sample_color(method: GradientMethod, stops: &[(f32, [f32; 4])], mids: &[f32], t: f32) -> [f32; 4] {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else { return [0.0; 4] };
    if t <= first.0 {
        return first.1;
    }
    let at = stops.partition_point(|s| s.0 < t);
    let (Some(a), Some(b)) = (at.checked_sub(1).and_then(|i| stops.get(i)), stops.get(at)) else { return last.1 };
    let u = if b.0 > a.0 { (t - a.0) / (b.0 - a.0) } else { 0.0 };
    mix(method, a.1, b.1, midpoint_remap(u, mids.get(at - 1).copied().unwrap_or(0.5)))
}

/// Colour alpha multiplied by independent opacity, after colour mixing.
pub fn sample_stops(method: GradientMethod, stops: &[(f32, [f32; 4])], mids: &[f32], opacity: &[(f32, f32)], t: f32) -> [f32; 4] {
    let mut c = sample_color(method, stops, mids, t);
    if !opacity.is_empty() {
        c[3] *= sample_opacity(opacity, t);
    }
    c
}

/// Exact limits at a breakpoint, without an epsilon that can round across a hard stop
/// when a writer reflects or reverses its parameter.
pub fn sample_limits(method: GradientMethod, stops: &[(f32, [f32; 4])], mids: &[f32], opacity: &[(f32, f32)], t: f32) -> ([f32; 4], [f32; 4]) {
    let left = sample_stops(method, stops, mids, opacity, t);
    let mut right = sample_color(method, stops, mids, t);
    let at = stops.partition_point(|s| s.0 <= t);
    if let Some((position, color)) = at.checked_sub(1).and_then(|i| stops.get(i))
        && *position == t
    {
        right = *color;
    }
    let mut alpha = sample_opacity(opacity, t);
    let at = opacity.partition_point(|s| s.0 <= t);
    if let Some((position, value)) = at.checked_sub(1).and_then(|i| opacity.get(i))
        && *position == t
    {
        alpha = *value;
    }
    right[3] *= alpha;
    (left, right)
}

/// Maximum samples in an expanded gradient, including both sides of hard stops.
pub const MAX_EXPANDED_STOPS: usize = 4096;
const TOLERANCE: f32 = 0.4 / 255.0;
const MAX_DEPTH: u32 = 16;
const LIMIT: &str = "gradient needs too many stops to preserve its appearance; simplify the ramp";

/// Refines sorted ramp breakpoints until linear RGBA interpolation follows `sample` within
/// 0.4/255 per channel. Coincident positions retain their side (-1/1) and order. Failure is
/// explicit if the bounded sample/depth budget cannot meet the tolerance.
pub fn refine_samples(samples: Vec<(f32, i8)>, sample: impl Fn(f32, i8) -> [f32; 4]) -> Result<Vec<(f32, i8)>, &'static str> {
    if samples.len() > MAX_EXPANDED_STOPS {
        return Err(LIMIT);
    }
    if samples.iter().any(|(t, side)| !t.is_finite() || !(0.0..=1.0).contains(t) || !(-1..=1).contains(side)) || samples.windows(2).any(|p| p[0].0 > p[1].0) {
        return Err("gradient positions must be finite, ordered and within 0..1");
    }
    let mut out = Vec::new();
    if let Some(first) = samples.first() {
        out.push(*first);
    }
    for pair in samples.windows(2) {
        subdivide(&sample, pair[0], pair[1], 0, &mut out)?;
    }
    Ok(out)
}

fn subdivide(sample: &impl Fn(f32, i8) -> [f32; 4], a: (f32, i8), b: (f32, i8), depth: u32, out: &mut Vec<(f32, i8)>) -> Result<(), &'static str> {
    if out.len() >= MAX_EXPANDED_STOPS {
        return Err(LIMIT);
    }
    let (c0, c1) = (sample(a.0, a.1), sample(b.0, b.1));
    if c0.iter().chain(&c1).any(|v| !v.is_finite()) {
        return Err("gradient colours must be finite");
    }
    let mut error = 0.0_f32;
    if a.0 < b.0 {
        for u in [0.25, 0.5, 0.75] {
            let c = sample(a.0 + (b.0 - a.0) * u, 0);
            if c.iter().any(|v| !v.is_finite()) {
                return Err("gradient colours must be finite");
            }
            for ch in 0..4 {
                error = error.max((c0[ch] + (c1[ch] - c0[ch]) * u - c[ch]).abs());
            }
        }
    }
    if error > TOLERANCE {
        let mid = (a.0 + b.0) / 2.0;
        if depth >= MAX_DEPTH || mid <= a.0 || mid >= b.0 {
            return Err(LIMIT);
        }
        subdivide(sample, a, (mid, 0), depth + 1, out)?;
        subdivide(sample, (mid, 0), b, depth + 1, out)?;
    } else {
        out.push(b);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
