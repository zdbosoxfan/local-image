//! pc-text's native layout/outline adapter, following type_extra_cmds and package 12's
//! quadratic elevation. Original Local Image code; no second text shaper.
use super::*;
use photocraft_doc::{Knot, Path, PathOp, ShapeLayer, Subpath, TextLayer};
use photocraft_geom::Point;
use photocraft_text::{
    layout::TextLayout,
    render::{self, PathEl},
};

pub(super) fn layout(doc: &Document, t: &TextLayer) -> Result<TextLayout> {
    if t.text.len() > 65_536 || t.runs.len() > 4096 || t.paragraphs.len() > 4096 || t.transform.m.iter().any(|v| !v.is_finite() || v.abs() > 1_000_000.) {
        return Err(error("styled text exceeds layout budget"));
    }
    let mut validation = t.clone();
    validation.warp = None;
    crate::vector_packages::path_text::on_path::validate_text(&validation).map_err(|e| error(e.0))?;
    if let photocraft_doc::text::TextShape::Box { x, y, width, height } = t.shape
        && (![x, y, width, height].iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.) || width <= 0. || height <= 0.)
    {
        return Err(error("invalid styled text frame"));
    }
    Ok(photocraft_text::shared().lock().unwrap_or_else(|e| e.into_inner()).layout(t, doc.resolution_dpi))
}
pub(super) fn shape(t: &TextLayer, layout: &TextLayout) -> Result<ShapeLayer> {
    let warp = render::layout_warp(layout, t.warp.as_ref());
    let mut outlines = render::outlines(layout, &t.transform, warp.as_ref());
    for d in &layout.decorations {
        let mut points = [[d.x0, d.y0], [d.x1, d.y0], [d.x1, d.y1], [d.x0, d.y1]]
            .map(|[x, y]| {
                let (x, y) = warp.as_ref().map_or((f64::from(x), f64::from(y)), |w| w.apply(f64::from(x), f64::from(y)));
                let p = t.transform.apply(Point::new(x, y));
                [p.x, p.y]
            })
            .into_iter();
        let mut elements = Vec::new();
        if let Some(p) = points.next() {
            elements.push(PathEl::MoveTo(p));
        }
        elements.extend(points.map(PathEl::LineTo));
        elements.push(PathEl::Close);
        outlines.push(elements);
    }
    from_outlines(t, outlines)
}
pub(super) fn from_outlines(t: &TextLayer, outlines: Vec<Vec<PathEl>>) -> Result<ShapeLayer> {
    if outlines.iter().map(Vec::len).sum::<usize>() > 1_000_000 {
        return Err(error("styled text outline budget"));
    }
    let mut path = Path::default();
    let point = |[x, y]: [f64; 2]| Point::new(x, y);
    for elements in outlines {
        let mut knots: Vec<Knot> = Vec::new();
        let finish = |knots: &mut Vec<Knot>, path: &mut Path| {
            if knots.len() > 1
                && knots.first().zip(knots.last()).is_some_and(|(a, b)| a.anchor == b.anchor)
                && let Some(last) = knots.pop()
            {
                knots[0].in_ctrl = last.in_ctrl;
            }
            if knots.len() > 1 {
                path.subpaths.push(Subpath {
                    closed: true,
                    knots: std::mem::take(knots),
                    op: if path.subpaths.is_empty() { PathOp::Combine } else { PathOp::Join },
                });
            }
            knots.clear();
        };
        for el in elements {
            match el {
                PathEl::MoveTo([x, y]) => {
                    finish(&mut knots, &mut path);
                    knots.push(Knot::corner(x, y));
                }
                PathEl::LineTo([x, y]) => knots.push(Knot::corner(x, y)),
                PathEl::QuadTo(c, p) => {
                    let Some(last) = knots.last_mut() else { continue };
                    last.out_ctrl = Point::new(last.anchor.x + (c[0] - last.anchor.x) * 2. / 3., last.anchor.y + (c[1] - last.anchor.y) * 2. / 3.);
                    knots.push(Knot {
                        anchor: point(p),
                        in_ctrl: Point::new(p[0] + (c[0] - p[0]) * 2. / 3., p[1] + (c[1] - p[1]) * 2. / 3.),
                        out_ctrl: point(p),
                        smooth: false,
                    });
                }
                PathEl::CurveTo(a, b, p) => {
                    if let Some(last) = knots.last_mut() {
                        last.out_ctrl = point(a);
                    }
                    knots.push(Knot { anchor: point(p), in_ctrl: point(b), out_ctrl: point(p), smooth: false });
                }
                PathEl::Close => finish(&mut knots, &mut path),
            }
        }
        finish(&mut knots, &mut path);
    }
    crate::vector_api::validate_path(&path)?;
    Ok(ShapeLayer { path, fill: Some(Fill::Solid(t.color)), ..Default::default() })
}
pub(super) fn bytes(doc: &Document, t: &TextLayer, l: &TextLayout) -> u128 {
    let warp = render::layout_warp(l, t.warp.as_ref());
    let r = render::ink_rect_warped(l, &t.transform, warp.as_ref());
    if r.is_empty() {
        return 0;
    }
    let tile = photocraft_geom::TILE_SIZE;
    let nx = i64::from((r.x1 - 1).div_euclid(tile)) - i64::from(r.x0.div_euclid(tile)) + 1;
    let ny = i64::from((r.y1 - 1).div_euclid(tile)) - i64::from(r.y0.div_euclid(tile)) + 1;
    nx as u128 * ny as u128 * tile as u128 * tile as u128 * doc.pixel_format().bytes_per_pixel() as u128
}
pub(super) fn render(doc: &Document, t: &TextLayer, l: &TextLayout) -> photocraft_raster::Surface {
    let warp = render::layout_warp(l, t.warp.as_ref());
    let r = render::rasterize_warped(l, &t.transform, doc.pixel_format(), t.antialias, warp.as_ref());
    // Keep only the bounded canvas cache; no full-canvas staging allocation.
    let mut result = photocraft_raster::Surface::new(doc.pixel_format());
    let clip = r.rect.intersect(&doc.bounds());
    for y in clip.y0..clip.y1 {
        for x in clip.x0..clip.x1 {
            let px = r.surface.pixel(x, y);
            if px.last().is_some_and(|a| *a != 0.) {
                result.write_pixel(x, y, &px);
            }
        }
    }
    result
}
