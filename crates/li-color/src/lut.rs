//! Shared red-fastest 3D LUT interpolation. Tetrahedral interpolation uses f64 weights and
//! accumulation, rounding once to f32; trilinear remains available for stored editor choices.

/// Map a finite increasing input domain into LUT coordinates. Invalid persisted domains
/// keep unit coordinates; clamping belongs to the sampler so all callers share the boundary rule.
#[inline]
pub fn normalize_domain(rgb: [f32; 3], min: [f32; 3], max: [f32; 3]) -> [f32; 3] {
    if !(0..3).all(|i| min[i].is_finite() && max[i].is_finite() && max[i] > min[i] && (max[i] - min[i]).is_finite()) {
        return rgb;
    }
    std::array::from_fn(|i| (rgb[i] - min[i]) / (max[i] - min[i]))
}

/// Locate the containing cell and fractional coordinate on a normalized axis.
#[inline]
fn locate(n: usize, v: f32) -> (usize, f32) {
    let p = v.clamp(0.0, 1.0) * (n - 1) as f32;
    let i = (p.floor() as usize).min(n - 2);
    (i, p - i as f32)
}

/// The four corners and barycentric weights of the containing tetrahedron.
#[inline]
fn tetrahedron(f: [f32; 3]) -> ([[usize; 3]; 4], [f64; 4]) {
    let [r, g, b] = f.map(f64::from);
    let (a, c, x, y, z) = if r >= g {
        if g >= b {
            ([1, 0, 0], [1, 1, 0], r, g, b)
        } else if r >= b {
            ([1, 0, 0], [1, 0, 1], r, b, g)
        } else {
            ([0, 0, 1], [1, 0, 1], b, r, g)
        }
    } else if r >= b {
        ([0, 1, 0], [1, 1, 0], g, r, b)
    } else if g >= b {
        ([0, 1, 0], [0, 1, 1], g, b, r)
    } else {
        ([0, 0, 1], [0, 1, 1], b, g, r)
    };
    ([[0, 0, 0], a, c, [1, 1, 1]], [1.0 - x, x - y, y - z, z])
}

#[inline]
fn weighted(v: [f32; 4], w: [f64; 4]) -> f32 {
    (f64::from(v[0]) * w[0] + f64::from(v[1]) * w[1] + f64::from(v[2]) * w[2] + f64::from(v[3]) * w[3]) as f32
}

/// Tetrahedral evaluation in one cell of an interleaved table. Strides permit ICC's
/// blue-fastest order, non-cubic grids, arbitrary output counts and degenerate dimensions.
#[inline]
pub fn tetrahedral_strided(data: &[f32], base: usize, strides: [usize; 3], frac: [f32; 3], out: &mut [f32]) {
    let (corners, weights) = tetrahedron(frac);
    let offsets = corners.map(|c| base + c[0] * strides[0] + c[1] * strides[1] + c[2] * strides[2]);
    for (k, v) in out.iter_mut().enumerate() {
        *v = weighted(offsets.map(|i| data[i + k]), weights);
    }
}

/// Sample RGB from a validated table via a corner accessor (absolute grid coordinates).
/// Degenerate tables leave the input unchanged.
#[inline]
pub fn sample(n: usize, rgb: [f32; 3], tetrahedral: bool, at: impl Fn(usize, usize, usize) -> [f32; 3]) -> [f32; 3] {
    if n < 2 {
        return rgb;
    }
    let loc = rgb.map(|v| locate(n, v));
    let f = loc.map(|x| x.1);
    let corner = |c: [usize; 3]| at(loc[0].0 + c[0], loc[1].0 + c[1], loc[2].0 + c[2]);
    if tetrahedral {
        let (coords, w) = tetrahedron(f);
        let v = coords.map(corner);
        std::array::from_fn(|k| weighted(v.map(|c| c[k]), w))
    } else {
        let lerp = |a: [f32; 3], b: [f32; 3], t: f32| std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t);
        let c00 = lerp(corner([0, 0, 0]), corner([1, 0, 0]), f[0]);
        let c10 = lerp(corner([0, 1, 0]), corner([1, 1, 0]), f[0]);
        let c01 = lerp(corner([0, 0, 1]), corner([1, 0, 1]), f[0]);
        let c11 = lerp(corner([0, 1, 1]), corner([1, 1, 1]), f[0]);
        lerp(lerp(c00, c10, f[1]), lerp(c01, c11, f[1]), f[2])
    }
}

/// Sample red-fastest interleaved RGB data; short and overflowing tables are no-ops.
#[inline]
pub fn sample_rgb(data: &[f32], n: usize, rgb: [f32; 3], tetrahedral: bool) -> [f32; 3] {
    if n < 2 || n.checked_pow(3).and_then(|x| x.checked_mul(3)).is_none_or(|len| data.len() < len) {
        return rgb;
    }
    sample(n, rgb, tetrahedral, |r, g, b| {
        let i = ((b * n + g) * n + r) * 3;
        [data[i], data[i + 1], data[i + 2]]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layouts_and_boundaries_agree_exactly() {
        let data: Vec<f32> = (0..24).map(|i| (i as f32 * 0.71).sin()).collect();
        for rgb in [[0.0; 3], [1.0; 3], [0.25, 0.5, 0.75], [0.75, 0.5, 0.25], [0.5; 3], [1.0, 0.0, 0.0]] {
            let mut strided = [0.0; 3];
            tetrahedral_strided(&data, 0, [3, 6, 12], rgb, &mut strided);
            assert_eq!(sample_rgb(&data, 2, rgb, true).map(f32::to_bits), strided.map(f32::to_bits));
        }
        assert_eq!(sample_rgb(&[], usize::MAX, [0.2; 3], true), [0.2; 3]);
    }

    #[test]
    fn tetrahedral_improves_correlated_nonlinear_lut() {
        let n = 9;
        let f = |c: [f32; 3]| [c[0].max(c[1]).max(c[2]).powi(2); 3];
        let lut = crate::lutfile::LutFile::from_fn("test", n, f);
        let (mut tet, mut tri) = (0.0f64, 0.0f64);
        for i in 0..1000 {
            let c = [((i * 67) % 997) as f32 / 996.0, ((i * 193) % 991) as f32 / 990.0, ((i * 43) % 983) as f32 / 982.0];
            let expected = f(c)[0];
            tet += f64::from((sample_rgb(&lut.data, n, c, true)[0] - expected).abs());
            tri += f64::from((sample_rgb(&lut.data, n, c, false)[0] - expected).abs());
        }
        eprintln!("correlated max² LUT MAE: tetra={} trilinear={}", tet / 1000.0, tri / 1000.0);
        assert!(tet < tri);
    }
    #[test]
    fn f64_accumulation_reduces_rounding_error() {
        let (mut weighted_error, mut difference_error, mut precise_error) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..20000u32 {
            let f = [((i * 67) % 997) as f32 / 996.0, ((i * 193) % 991) as f32 / 990.0, ((i * 43) % 983) as f32 / 982.0];
            let (_, w) = tetrahedron(f);
            let v = std::array::from_fn::<_, 4, _>(|k| ((i as f32 * 0.071 + k as f32 * 1.7).sin() * 8.0) + 0.01);
            let exact = f64::from(v[0]) * w[0] + f64::from(v[1]) * w[1] + f64::from(v[2]) * w[2] + f64::from(v[3]) * w[3];
            let old_weighted: f32 = v.iter().zip(w).map(|(&v, w)| v * w as f32).sum();
            let x = (w[1] + w[2] + w[3]) as f32;
            let y = (w[2] + w[3]) as f32;
            let z = w[3] as f32;
            let old_difference = v[0] + (v[1] - v[0]) * x + (v[2] - v[1]) * y + (v[3] - v[2]) * z;
            weighted_error += (f64::from(old_weighted) - exact).abs();
            difference_error += (f64::from(old_difference) - exact).abs();
            precise_error += (f64::from(weighted(v, w)) - exact).abs();
        }
        eprintln!(
            "tetra accumulation MAE: weighted={} difference={} shared_f64={}",
            weighted_error / 20000.0,
            difference_error / 20000.0,
            precise_error / 20000.0
        );
        assert!(precise_error < weighted_error && precise_error < difference_error);
    }

    #[test]
    fn interpolation_policy_is_distinct_from_accumulation_precision() {
        // Trilinear is exact for xy; tetrahedral is piecewise affine, so cannot be
        // universally more accurate for every unknown function between LUT nodes.
        let data = crate::lutfile::LutFile::from_fn("xy", 2, |c| [c[0] * c[1]; 3]).data;
        let c = [0.25, 0.75, 0.5];
        assert_eq!(sample_rgb(&data, 2, c, false), [0.1875; 3]);
        assert_eq!(sample_rgb(&data, 2, c, true), [0.25; 3]);
    }
}
