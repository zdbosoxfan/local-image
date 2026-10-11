//! Independent geometry oracles; no dependency on the constructive geometry engine.
//! EDT follows Felzenszwalb & Huttenlocher, Distance Transforms of Sampled Functions,
//! Theory of Computing 8 (2012), Algorithm 1 (https://cs.brown.edu/people/pfelzens/papers/dt-final.pdf).
use kurbo::{BezPath, ParamCurve, ParamCurveNearest, Shape};
use photocraft_doc::{FillRule, Path};
use photocraft_geom::Rect;

/// Exact Green integral of simple, nonintersecting nested cubic contours.
/// Resolve component booleans before calling: arbitrary intersecting contours require topology,
/// and lie outside this oracle's preconditions. Open fills close by a line.
pub fn area(path: &Path, rule: FillRule) -> Result<f64, String> {
    if !super::is_finite(path) || path.inverted || path.components().len() > 1 {
        return Err("area requires a finite, resolved winding region".into());
    }
    let contours: Vec<BezPath> = path
        .subpaths
        .iter()
        .filter_map(|s| {
            let first = s.knots.first()?;
            let mut bp = BezPath::new();
            bp.move_to((first.anchor.x, first.anchor.y));
            for [_, a, b, c] in s.segments() {
                bp.curve_to((a.x, a.y), (b.x, b.y), (c.x, c.y));
            }
            bp.close_path();
            Some(bp)
        })
        .collect();
    let inside = |w: i32| match rule {
        FillRule::NonZero => w != 0,
        FillRule::EvenOdd => w % 2 != 0,
    };
    let mut total = 0.0;
    for (i, contour) in contours.iter().enumerate() {
        let bounds = contour.bounding_box();
        let signed = (kurbo::Affine::translate((-bounds.x0, -bounds.y0)) * contour).area();
        if signed == 0.0 {
            continue;
        }
        // A start anchor may lie on another contour at a tangency. Probe a curve
        // interior away from all other boundaries, preserving nested-hole winding.
        let p = contour.segments().flat_map(|seg| [0.2113248654, 0.5, 0.7886751346].map(|t| seg.eval(t))).max_by(|a, b| {
            let distance = |p| {
                contours
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .flat_map(|(_, c)| c.segments())
                    .map(|seg| seg.nearest(p, 1e-10).distance_sq)
                    .fold(f64::INFINITY, f64::min)
            };
            distance(*a).total_cmp(&distance(*b))
        });
        let Some(p) = p else { continue };
        let outside: i32 = contours.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, c)| c.winding(p)).sum();
        let winding = if signed > 0.0 { 1 } else { -1 };
        let coefficient = i32::from(inside(outside + winding)) - i32::from(inside(outside));
        total += signed.abs() * f64::from(coefficient);
    }
    Ok(total)
}

/// Native independent coverage oracle, with coordinates in document pixels.
pub fn coverage(path: &Path, rect: Rect) -> Vec<f32> {
    photocraft_vector::path_coverage(path, rect)
}

/// Exact squared Euclidean distance to the nearest true pixel center (in grid units).
/// An empty feature set produces infinity. No chamfer approximation or geometry engine.
pub fn euclidean_distance_transform(mask: &[bool], width: usize, height: usize) -> Result<Vec<f64>, String> {
    if width == 0 || height == 0 || width.checked_mul(height) != Some(mask.len()) {
        return Err("invalid distance-transform dimensions".into());
    }
    let mut tmp = vec![f64::INFINITY; mask.len()];
    for y in 0..height {
        let row: Vec<f64> = mask[y * width..(y + 1) * width].iter().map(|&v| if v { 0.0 } else { f64::INFINITY }).collect();
        tmp[y * width..(y + 1) * width].copy_from_slice(&edt_1d(&row));
    }
    let mut out = vec![f64::INFINITY; mask.len()];
    for x in 0..width {
        let col: Vec<f64> = (0..height).map(|y| tmp[y * width + x]).collect();
        for (y, d) in edt_1d(&col).into_iter().enumerate() {
            out[y * width + x] = d;
        }
    }
    Ok(out)
}

fn edt_1d(f: &[f64]) -> Vec<f64> {
    let sites: Vec<usize> = f.iter().enumerate().filter_map(|(i, d)| d.is_finite().then_some(i)).collect();
    let Some(&first) = sites.first() else { return vec![f64::INFINITY; f.len()] };
    let mut v = vec![first];
    let mut z = vec![f64::NEG_INFINITY, f64::INFINITY];
    for &q in &sites[1..] {
        let mut k = v.len() - 1;
        let mut s;
        loop {
            let p = v[k];
            s = ((f[q] + (q as f64).powi(2)) - (f[p] + (p as f64).powi(2))) / (2.0 * (q - p) as f64);
            if s > z[k] || k == 0 {
                break;
            }
            v.pop();
            z.pop();
            k -= 1;
        }
        z[k + 1] = s;
        v.push(q);
        z.push(f64::INFINITY);
    }
    let mut k = 0;
    (0..f.len())
        .map(|q| {
            while z[k + 1] < q as f64 {
                k += 1;
            }
            (q as f64 - v[k] as f64).powi(2) + f[v[k]]
        })
        .collect()
}

/// Signed EDT threshold for a round offset. Rasterize at 8× and pass scale=8.
/// Pixel-center distances have an uncertainty of half a grid cell at the boundary.
pub fn offset_mask(mask: &[f32], width: usize, height: usize, delta: f64, scale: f64) -> Result<Vec<f32>, String> {
    if !delta.is_finite() || !scale.is_finite() || scale <= 0.0 || mask.iter().any(|v| !v.is_finite()) {
        return Err("invalid offset-mask inputs".into());
    }
    let inside: Vec<bool> = mask.iter().map(|&v| v >= 0.5).collect();
    let features: Vec<bool> = inside.iter().map(|&v| if delta >= 0.0 { v } else { !v }).collect();
    let distances = euclidean_distance_transform(&features, width, height)?;
    Ok(inside
        .iter()
        .zip(distances)
        .map(|(&filled, d)| {
            let near = (d.sqrt() - 0.5).max(0.0) / scale <= delta.abs();
            f32::from(if delta >= 0.0 { filled || near } else { filled && !near })
        })
        .collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    #[test]
    fn edt_matches_brute_force_and_empty_features() {
        for bits in 0..256u16 {
            let mask: Vec<bool> = (0..8).map(|i| bits & (1 << i) != 0).collect();
            let got = euclidean_distance_transform(&mask, 4, 2).unwrap();
            for (i, &d) in got.iter().enumerate() {
                let expected = mask
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| **v)
                    .map(|(j, _)| (i % 4).abs_diff(j % 4).pow(2) as f64 + (i / 4).abs_diff(j / 4).pow(2) as f64)
                    .fold(f64::INFINITY, f64::min);
                assert_eq!(d, expected);
            }
        }
        assert!(euclidean_distance_transform(&[], 0, 1).is_err());
    }
    #[test]
    fn green_area_tangent_contours_are_not_nested() {
        let mut p = photocraft_vector::shapes::ellipse(16., 16., 48., 48.);
        let circle = area(&p, FillRule::NonZero).unwrap();
        p.subpaths.extend(photocraft_vector::shapes::rect(64., 8., 16., 64.).subpaths.into_iter().map(|s| s.with_op(photocraft_doc::PathOp::Join)));
        assert!((area(&p, FillRule::NonZero).unwrap() - circle - 1024.).abs() < 1e-8);
        // Reverse all contours; the filled area is unchanged.
        for s in &mut p.subpaths {
            s.knots.reverse();
            for k in &mut s.knots {
                std::mem::swap(&mut k.in_ctrl, &mut k.out_ctrl);
            }
        }
        assert!((area(&p, FillRule::NonZero).unwrap() - circle - 1024.).abs() < 1e-8);
    }
    #[test]
    fn green_area_respects_winding_holes_and_evenodd() {
        let mut p = photocraft_vector::shapes::rect(0.0, 0.0, 100.0, 100.0);
        p.subpaths.extend(photocraft_vector::shapes::rect(25.0, 25.0, 50.0, 50.0).subpaths.into_iter().map(|s| s.with_op(photocraft_doc::PathOp::Join)));
        assert_eq!(area(&p, FillRule::NonZero).unwrap(), 10000.0);
        assert_eq!(area(&p, FillRule::EvenOdd).unwrap(), 7500.0);
        p.subpaths[1].knots.reverse();
        assert_eq!(area(&p, FillRule::NonZero).unwrap(), 7500.0);
    }
}
