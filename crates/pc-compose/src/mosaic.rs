//! Sum disjoint subpixel coverage once, before compositing the partition over its backdrop.
use crate::Buffer;
use photocraft_doc::{Fill, Layer, LayerContent};
use photocraft_geom::{Affine, Rect};
pub fn render(layer: &Layer, rect: Rect, scale: f64) -> Option<Buffer> {
    if !photocraft_doc::vector_design::mosaic::active(layer) {
        return None;
    }
    let LayerContent::Group(group) = &layer.content else {
        return None;
    };
    let mut result = Buffer::transparent(rect);
    for child in group.children.iter().filter(|l| l.visible) {
        let LayerContent::Shape(shape) = &child.content else {
            return None;
        };
        let Some(Fill::Solid(color)) = &shape.fill else {
            return None;
        };
        let rgb = color.to_rgb();
        let color = [rgb[0], rgb[1], rgb[2], color.alpha];
        let path = shape.path.transform(&Affine::scale(scale));
        for (pixel, coverage) in result.px.iter_mut().zip(photocraft_vector::path_coverage(&path, rect)) {
            let alpha = coverage * color[3];
            for c in 0..3 {
                pixel[c] += color[c] * alpha;
            }
            pixel[3] += alpha;
        }
    }
    for pixel in &mut result.px {
        if pixel[3] > 0. {
            for c in 0..3 {
                pixel[c] /= pixel[3];
            }
        }
        pixel[3] = pixel[3].clamp(0., 1.);
    }
    Some(result)
}
pub fn surface(layer: &Layer, rect: Rect, scale: f64, format: photocraft_color::PixelFormat) -> Option<photocraft_raster::Surface> {
    if u64::from(rect.width()) * u64::from(rect.height()) > crate::scaled_view::MAX_VIEW_PIXELS {
        return None;
    }
    let buffer = render(layer, rect, scale)?;
    let mut surface = photocraft_raster::Surface::new(format);
    let mut row = Vec::with_capacity(rect.width() as usize * format.channels());
    for y in rect.y0..rect.y1 {
        row.clear();
        for x in rect.x0..rect.x1 {
            let p = buffer.get(x, y);
            row.extend(photocraft_raster::from_rgba(&format, p));
        }
        surface.write_region(Rect::new(rect.x0, y, rect.x1, y + 1), &row);
    }
    surface.prune();
    Some(surface)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use photocraft_doc::{Color, Path, ShapeLayer, Subpath};
    fn partition() -> Layer {
        let shapes = [(0.0, 3.5, Color::rgb(1., 0., 0.)), (3.5, 8., Color::rgb(0., 0., 1.))]
            .into_iter()
            .map(|(a, b, c)| {
                Layer::new(
                    "paint",
                    LayerContent::Shape(ShapeLayer {
                        path: Path::new(vec![Subpath::polygon(&[(a, 0.), (b, 0.), (b, 8.), (a, 8.)])]),
                        fill: Some(Fill::Solid(c)),
                        ..Default::default()
                    }),
                )
            })
            .collect();
        let mut group = Layer::group("partition", shapes);
        assert!(photocraft_doc::vector_design::mosaic::mark(&mut group).is_ok());
        group
    }
    #[test]
    fn complementary_coverages_preserve_alpha_and_colour_at_every_scale() {
        let group = partition();
        for scale in [1., 2., 4., 8.] {
            let rect = Rect::new(0, 0, (8. * scale) as i32, (8. * scale) as i32);
            let buffer = render(&group, rect, scale).expect("marked partition");
            assert!(buffer.px.iter().all(|p| (p[3] - 1.).abs() <= 1. / 255.));
            if scale == 1. {
                assert_eq!(buffer.get(3, 4), [0.5, 0., 0.5, 1.]);
            }
        }
    }
    #[test]
    fn geometry_changes_disable_partition_but_recolouring_keeps_it() {
        let mut group = partition();
        if let Some(children) = group.children_mut()
            && let LayerContent::Shape(shape) = &mut children[0].content
        {
            shape.fill = Some(Fill::Solid(Color::WHITE));
        }
        assert!(photocraft_doc::vector_design::mosaic::active(&group));
        if let Some(children) = group.children_mut()
            && let LayerContent::Shape(shape) = &mut children[0].content
        {
            shape.path.subpaths[0].knots[0].anchor.x += 0.1;
        }
        assert!(!photocraft_doc::vector_design::mosaic::active(&group));
    }
}
