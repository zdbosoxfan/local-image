//! Low-gradient palette masks reuse T1 contours and fitters unchanged. At AA edges,
//! colour-mixture coverage locates the 50% boundary instead of rounding it to a pixel.
//! This is a local scalar-field correction, not a replacement for the T1 fitter.
use super::{check, preprocess::Prepared};
use crate::{ColorPath, Error, Fitter, Image, Params, Trace, vc::BinaryImage};
use kurbo::Point;
use photocraft_doc::{Path, PathOp};

fn mixture(pixel: &[u8], target: [u8; 3], palette: &[[u8; 3]]) -> f64 {
    if pixel[3] == 0 {
        return 0.;
    }
    let p = [f64::from(pixel[0]), f64::from(pixel[1]), f64::from(pixel[2])];
    let target = target.map(f64::from);
    let mut best = (f64::INFINITY, 0.);
    for other in palette {
        let q = other.map(f64::from);
        let v = [target[0] - q[0], target[1] - q[1], target[2] - q[2]];
        let len: f64 = v.iter().map(|v| v * v).sum();
        if len < 1e-10 {
            continue;
        }
        let t = ((0..3).map(|i| (p[i] - q[i]) * v[i]).sum::<f64>() / len).clamp(0., 1.);
        let residual: f64 = (0..3).map(|i| (p[i] - q[i] - t * v[i]).powi(2)).sum();
        if residual < best.0 {
            best = (residual, t);
        }
    }
    if palette.len() == 1 {
        best.1 = 1.;
    }
    best.1 * f64::from(pixel[3]) / 255.
}
fn sample(field: &[f64], w: usize, h: usize, p: Point) -> f64 {
    // Pixel samples are at centres; outside the source rectangle is unpainted.
    let (x, y) = (p.x - 0.5, p.y - 0.5);
    let (ix, iy) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x.fract().rem_euclid(1.), y.fract().rem_euclid(1.));
    let get = |xx: i64, yy: i64| if xx >= 0 && yy >= 0 && xx < w as i64 && yy < h as i64 { field[yy as usize * w + xx as usize] } else { 0. };
    get(ix, iy) * (1. - fx) * (1. - fy) + get(ix + 1, iy) * fx * (1. - fy) + get(ix, iy + 1) * (1. - fx) * fy + get(ix + 1, iy + 1) * fx * fy
}

pub(super) fn trace(
    source: Image<'_>,
    prepared: &Prepared,
    params: &Params,
    palette: &[[u8; 3]],
    scale: u32,
    cancel: &impl Fn() -> bool,
) -> Result<Trace, Error> {
    let (w, h) = ((source.width * scale) as usize, (source.height * scale) as usize);
    let (sw, sh) = (source.width as usize, source.height as usize);
    let mut budget = crate::BOUNDARY_BUDGET;
    let mut layers = Vec::new();
    // Most populous stable colour is first, preserving the usual background-under-logo order.
    for (color_index, &color) in palette.iter().enumerate() {
        check(cancel)?;
        let is_background = color_index == 0
            && (0..h).all(|y| {
                [0, w - 1].into_iter().all(|x| {
                    let p = &prepared.pixels[(y * w + x) * 4..][..4];
                    p[3] == 255 && p[..3] == color
                })
            })
            && (0..w).all(|x| {
                [0, h - 1].into_iter().all(|y| {
                    let p = &prepared.pixels[(y * w + x) * 4..][..4];
                    p[3] == 255 && p[..3] == color
                })
            });
        if is_background {
            let mut bez = kurbo::BezPath::new();
            bez.move_to((0., 0.));
            bez.line_to((w as f64, 0.));
            bez.line_to((w as f64, h as f64));
            bez.line_to((0., h as f64));
            bez.close_path();
            layers.push(ColorPath { color: [color[0], color[1], color[2], 255], path: crate::adapter::bezpath_to_path(&bez)? });
            continue;
        }
        let mut mask = BinaryImage::new_w_h(w, h);
        let mut count = 0;
        for y in 0..h {
            check(cancel)?;
            for x in 0..w {
                let p = &prepared.pixels[(y * w + x) * 4..][..4];
                if p[3] >= 128 && p[..3] == color {
                    mask.set_pixel(x, y, true);
                    count += 1;
                }
            }
        }
        if count == 0 {
            continue;
        }
        let mut field = Vec::with_capacity(sw * sh);
        for row in prepared.continuous.chunks_exact(sw * 4) {
            check(cancel)?;
            for p in row.as_chunks::<4>().0 {
                field.push(mixture(p, color, palette));
            }
        }
        let mut working = Vec::with_capacity(w * h);
        for y in 0..h {
            check(cancel)?;
            for x in 0..w {
                let p = Point::new((x as f64 + 0.5) / f64::from(scale), (y as f64 + 0.5) / f64::from(scale));
                working.push(sample(&field, sw, sh, p));
            }
        }
        let loops = if params.fitter == Fitter::Pixel {
            crate::boundary::contours(&mask, &mut budget, cancel)?
        } else {
            super::contour::contours(&working, w, h, &mut budget, cancel)?
        };
        let mut path = Path::default();
        for contour in loops {
            check(cancel)?;
            // PixelArt explicitly keeps exact lattice corners even with the palette toggle.
            let points = contour.clone();
            let fit = match params.fitter {
                Fitter::Potrace => {
                    // Potrace calc_lon requires unit lattice steps. Fractional AA contours
                    // instead use the already-locked optimal kurbo simplifier locally.
                    let opts = kurbo::simplify::SimplifyOptions::default().opt_level(kurbo::simplify::SimplifyOptLevel::Optimize).angle_thresh(0.75);
                    kurbo::simplify::simplify_bezpath(crate::boundary::polygon(&points), params.detail, &opts)
                }
                Fitter::Spline => crate::spline::fit(&points, params.detail),
                Fitter::Pixel => crate::boundary::polygon(&points),
            };
            let mut part = crate::adapter::bezpath_to_path(&fit)?;
            for sub in &mut part.subpaths {
                if sub.closed && sub.knots.len() > 2 {
                    let last = sub.knots[sub.knots.len() - 1];
                    let first = sub.knots[0];
                    if (last.anchor.x - first.anchor.x).hypot(last.anchor.y - first.anchor.y) < 1e-9 {
                        sub.knots.pop();
                        sub.knots[0].in_ctrl = last.in_ctrl;
                    }
                }
                let n = sub.knots.len();
                for i in 0..n {
                    let j = (i + 1) % n;
                    let a = sub.knots[i];
                    let b = sub.knots[j];
                    let direction = kurbo::Vec2::new(b.anchor.x - a.anchor.x, b.anchor.y - a.anchor.y);
                    let out = kurbo::Vec2::new(a.out_ctrl.x - a.anchor.x, a.out_ctrl.y - a.anchor.y);
                    let into = kurbo::Vec2::new(b.in_ctrl.x - b.anchor.x, b.in_ctrl.y - b.anchor.y);
                    if out.dot(direction) < 0. && out.hypot() < params.detail {
                        sub.knots[i].out_ctrl = a.anchor;
                    }
                    if into.dot(direction) > 0. && into.hypot() < params.detail {
                        sub.knots[j].in_ctrl = b.anchor;
                    }
                }
            }
            if super::topology::crosses(&part, cancel)? {
                part = crate::adapter::bezpath_to_path(&crate::boundary::polygon(&crate::boundary::remove_collinear(&points)))?;
            }
            for sub in &mut part.subpaths {
                sub.op = if path.subpaths.is_empty() { PathOp::Combine } else { PathOp::Join };
            }
            path.subpaths.extend(part.subpaths);
        }
        if super::topology::crosses(&path, cancel)? {
            path = Path::default();
            for contour in super::contour::contours(&working, w, h, &mut budget, cancel)? {
                let mut part = crate::adapter::bezpath_to_path(&crate::boundary::polygon(&crate::boundary::remove_collinear(&contour)))?;
                for sub in &mut part.subpaths {
                    sub.op = if path.subpaths.is_empty() { PathOp::Combine } else { PathOp::Join };
                }
                path.subpaths.extend(part.subpaths);
            }
        }
        if !path.is_empty() {
            layers.push(ColorPath { color: [color[0], color[1], color[2], 255], path });
        }
    }
    let nodes = layers.iter().flat_map(|l| &l.path.subpaths).map(|s| s.knots.len()).sum();
    Ok(Trace { width: w as u32, height: h as u32, strokes: Vec::new(), layers, nodes })
}
