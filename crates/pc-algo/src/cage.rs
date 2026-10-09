//! Cage transform (Edit › Transform › Cage): the pixels inside a cage polygon follow the cage's
//! vertices to their targets through Green or mean value coordinates
//! ([`photocraft_geom::cage::CageMap`]); everything outside the cage stays put, and the part of
//! the cage the content leaves becomes transparent (GIMP's and Krita's cage tools behave alike).
//!
//! The inside is lifted with an anti-aliased polygon mask, warped forward through the map on a
//! fine triangle mesh ([`crate::warp::warp_mesh_surface`]), and composited back over the rest.

use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_geom::cage::CageMap;
use photocraft_raster::Surface;

use crate::transform::Interp;
use crate::warp::warp_mesh_surface;

/// Sub-rows per pixel row for [`polygon_coverage`].
const SUB: usize = 4;

/// Anti-aliased even-odd coverage of `poly` over `r` (row-major, `0..=1`): four sub-scanlines
/// per pixel row, each with exact horizontal span coverage.
pub fn polygon_coverage(poly: &[[f64; 2]], r: Rect) -> Vec<f32> {
    let (w, h) = (r.width() as usize, r.height() as usize);
    let mut cov = vec![0.0f32; w * h];
    if poly.len() < 3 || w == 0 || h == 0 {
        return cov;
    }
    let n = poly.len();
    let mut xs: Vec<f64> = Vec::with_capacity(16);
    let mut row = vec![0.0f64; w + 1];
    for yy in 0..h {
        row.fill(0.0);
        for s in 0..SUB {
            let y = f64::from(r.y0) + yy as f64 + (s as f64 + 0.5) / SUB as f64;
            xs.clear();
            for i in 0..n {
                let (a, b) = (poly[i], poly[(i + 1) % n]);
                if (a[1] > y) != (b[1] > y) {
                    xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
                }
            }
            xs.sort_by(f64::total_cmp);
            for span in xs.as_chunks::<2>().0 {
                let (x0, x1) = (span[0] - f64::from(r.x0), span[1] - f64::from(r.x0));
                let (x0, x1) = (x0.clamp(0.0, w as f64), x1.clamp(0.0, w as f64));
                if x1 <= x0 {
                    continue;
                }
                let (i0, i1) = (x0.floor() as usize, x1.floor() as usize);
                if i0 == i1 {
                    row[i0.min(w - 1)] += x1 - x0;
                    continue;
                }
                row[i0] += (i0 + 1) as f64 - x0;
                for v in &mut row[i0 + 1..i1] {
                    *v += 1.0;
                }
                if i1 < w {
                    row[i1] += x1 - i1 as f64;
                }
            }
        }
        for (c, v) in cov[yy * w..(yy + 1) * w].iter_mut().zip(&row) {
            *c = (v / SUB as f64).clamp(0.0, 1.0) as f32;
        }
    }
    cov
}

/// The cage's source box (rounded out), within `limit`.
fn cage_rect(map: &CageMap, limit: Rect) -> Rect {
    let b = map.source_bounds();
    Rect::new(b[0].floor() as i32 - 1, b[1].floor() as i32 - 1, b[2].ceil() as i32 + 1, b[3].ceil() as i32 + 1).intersect(&limit)
}

/// Normal-blends straight-alpha `top` over `dst` (same format, with alpha).
fn over(dst: &mut Surface, top: &Surface) {
    let b = top.content_bounds();
    if b.is_empty() {
        return;
    }
    let n = dst.format().channels();
    let a = n - 1;
    let t = top.read_region(b);
    let mut d = dst.read_region(b);
    for (dp, tp) in d.chunks_exact_mut(n).zip(t.chunks_exact(n)) {
        let (ta, da) = (tp[a], dp[a]);
        let oa = ta + da * (1.0 - ta);
        if oa > 0.0 {
            for c in 0..a {
                dp[c] = (tp[c] * ta + dp[c] * da * (1.0 - ta)) / oa;
            }
        }
        dp[a] = oa;
    }
    dst.write_region(b, &d);
}

/// Deforms the pixels of `src` inside the cage. The result has `src`'s format with alpha added.
pub fn cage_warp_surface(src: &Surface, map: &CageMap, interp: Interp) -> Surface {
    let fmt = src.format();
    let with_alpha = PixelFormat::new(fmt.mode, fmt.sample, true);
    let mut rest = if fmt.alpha { src.clone() } else { src.convert(with_alpha) };
    let r = cage_rect(map, src.content_bounds());
    if map.is_identity() || r.is_empty() {
        return rest;
    }
    let cov = polygon_coverage(map.source(), r);
    let n = with_alpha.channels();
    let px = rest.read_region(r);
    let (mut lp, mut rp) = (px.clone(), px);
    for (k, (l, q)) in cov.iter().zip(lp.chunks_exact_mut(n).zip(rp.chunks_exact_mut(n))) {
        l[n - 1] *= k;
        q[n - 1] *= 1.0 - k;
    }
    let mut lifted = Surface::new(with_alpha);
    lifted.write_region(r, &lp);
    lifted.prune();
    rest.write_region(r, &rp);
    let moved = warp_mesh_surface(&lifted, r, &|x, y| map.map(x, y), interp);
    over(&mut rest, &moved);
    rest.prune();
    rest
}

/// [`cage_warp_surface`] for a single-channel surface whose outside reads as its default (a
/// layer mask): the caged values move, the area they leave reads as the default.
pub fn cage_warp_gray(s: &Surface, map: &CageMap, interp: Interp) -> Surface {
    let fmt = s.format();
    if fmt.alpha || fmt.channels() != 1 {
        return cage_warp_surface(s, map, interp);
    }
    let default = s.default_pixel().first().copied().unwrap_or(0.0);
    let r = cage_rect(map, Rect::new(-(1 << 24), -(1 << 24), 1 << 24, 1 << 24));
    let mut out = s.clone();
    if map.is_identity() || r.is_empty() {
        return out;
    }
    let cov = polygon_coverage(map.source(), r);
    let v = s.read_region(r);
    let mut lifted = Surface::new(PixelFormat::new(fmt.mode, fmt.sample, true));
    let lp: Vec<f32> = v.iter().zip(&cov).flat_map(|(g, k)| [*g, *k]).collect();
    lifted.write_region(r, &lp);
    lifted.prune();
    let rp: Vec<f32> = v.iter().zip(&cov).map(|(g, k)| g * (1.0 - k) + default * k).collect();
    out.write_region(r, &rp);
    let moved = warp_mesh_surface(&lifted, r, &|x, y| map.map(x, y), interp);
    let b = moved.content_bounds();
    if !b.is_empty() {
        let m = moved.read_region(b);
        let under = out.read_region(b);
        let flat: Vec<f32> = m.as_chunks::<2>().0.iter().zip(&under).map(|(p, u)| p[0] * p[1] + u * (1.0 - p[1])).collect();
        out.write_region(b, &flat);
    }
    out.prune();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_geom::cage::CageCoords;

    fn layer() -> Surface {
        let mut s = Surface::new(PixelFormat::RGBA8);
        s.fill_rect(Rect::new(0, 0, 100, 100), &[0.1, 0.1, 0.1, 1.0]);
        s.fill_rect(Rect::new(40, 40, 50, 50), &[1.0, 0.0, 0.0, 1.0]);
        s
    }

    #[test]
    fn coverage_is_exact_for_a_box_and_antialiased_on_a_diagonal() {
        let c = polygon_coverage(&[[1.0, 1.0], [3.5, 1.0], [3.5, 3.0], [1.0, 3.0]], Rect::new(0, 0, 4, 4));
        assert_eq!(c[4 + 1], 1.0);
        assert_eq!(c[4 + 3], 0.5);
        assert_eq!(c[0], 0.0);
        let t = polygon_coverage(&[[0.0, 0.0], [8.0, 0.0], [0.0, 8.0]], Rect::new(0, 0, 8, 8));
        let sum: f32 = t.iter().sum();
        assert!((sum - 32.0).abs() < 0.2, "{sum}");
    }

    #[test]
    fn dragging_the_cage_moves_inside_content_and_leaves_the_outside() {
        let s = layer();
        let cage = vec![[30.0, 30.0], [60.0, 30.0], [60.0, 60.0], [30.0, 60.0]];
        for coords in [CageCoords::Green, CageCoords::MeanValue] {
            let target: Vec<[f64; 2]> = cage.iter().map(|p| [p[0] + 20.0, p[1] + 10.0]).collect();
            let m = CageMap::new(&cage, &target, coords).unwrap();
            let o = cage_warp_surface(&s, &m, Interp::Bilinear);
            // The red square moved by (20, 10).
            let p = o.pixel(65, 55);
            assert!(p[0] > 0.95 && p[1] < 0.05 && p[3] > 0.99, "{coords:?} {p:?}");
            assert!(o.pixel(45, 45)[3] < 0.05, "the vacated cage is transparent");
            // Outside the cage nothing changed.
            assert_eq!(o.pixel(10, 10), s.pixel(10, 10));
            assert_eq!(o.pixel(90, 20), s.pixel(90, 20));
        }
        // An identity cage is a no-op.
        let id = CageMap::new(&cage, &cage, CageCoords::Green).unwrap();
        assert!(cage_warp_surface(&s, &id, Interp::Bicubic) == s);
    }

    #[test]
    fn masks_follow_with_their_default_behind() {
        let mut m = Surface::with_default(PixelFormat::GRAY8, &[1.0]);
        m.fill_rect(Rect::new(40, 40, 50, 50), &[0.0]);
        let cage = vec![[30.0, 30.0], [60.0, 30.0], [60.0, 60.0], [30.0, 60.0]];
        let target: Vec<[f64; 2]> = cage.iter().map(|p| [p[0] + 20.0, p[1]]).collect();
        let map = CageMap::new(&cage, &target, CageCoords::MeanValue).unwrap();
        let o = cage_warp_gray(&m, &map, Interp::Bilinear);
        assert!(o.pixel(65, 45)[0] < 0.05, "{:?}", o.pixel(65, 45));
        assert!(o.pixel(45, 45)[0] > 0.95, "{:?}", o.pixel(45, 45));
        assert_eq!(o.pixel(5, 5)[0], 1.0);
    }
}
