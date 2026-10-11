//! The quality plan specifies geometry IoU at 8x supersampled coverage. Colour is
//! compared separately at the requested output resolution, including all opacities.
//! Opaque paint isolates geometry from the 50% paint-opacity threshold, which is
//! unrelated to shape boundaries. Both renderer inputs retain their actual geometry.
use super::*;
pub(super) fn coverage_iou(doc: &Document, svg: &str) -> f64 {
    const SCALE: u32 = 8;
    let parsed = xml(svg);
    let mut edits = Vec::new();
    for node in parsed.descendants().filter(|n| n.is_element()) {
        for a in node.attributes() {
            let value = match a.name() {
                "fill" | "stroke" | "stop-color" if a.value() != "none" => Some("#000000".to_owned()),
                "opacity" | "fill-opacity" | "stroke-opacity" | "stop-opacity" => Some("1".to_owned()),
                "width" if node == parsed.root_element() => Some((doc.size.width * SCALE).to_string()),
                "height" if node == parsed.root_element() => Some((doc.size.height * SCALE).to_string()),
                _ => None,
            };
            if let Some(value) = value {
                edits.push((a.range_value(), value));
            }
        }
    }
    edits.sort_by_key(|a| std::cmp::Reverse(a.0.start));
    let mut opaque = svg.to_owned();
    for (range, value) in edits {
        opaque.replace_range(range, &value);
    }
    let actual = render(&opaque);
    let mut reference = doc.clone();
    reference.size = Size::new(doc.size.width * SCALE, doc.size.height * SCALE);
    let format = reference.pixel_format();
    let bounds = reference.bounds();
    let transform = photocraft_doc::Affine::scale(f64::from(SCALE));
    fn walk(layers: &mut [Layer], transform: &photocraft_doc::Affine, format: photocraft_doc::PixelFormat, bounds: photocraft_doc::Rect, dpi: f32) {
        for layer in layers {
            layer.opacity = 1.;
            layer.fill_opacity = 1.;
            if let Some(mask) = &mut layer.vector_mask {
                mask.path = mask.path.transform(transform);
            }
            match &mut layer.content {
                LayerContent::Shape(shape) => {
                    shape.path = shape.path.transform(transform);
                    if shape.fill.is_some() {
                        shape.fill = Some(Fill::Solid(Color::BLACK));
                    }
                    if let Some(stroke) = &mut shape.stroke {
                        stroke.paint = Fill::Solid(Color::BLACK);
                        stroke.opacity = 1.;
                        stroke.width *= 8.;
                    }
                    shape.cache = Some(photocraft_vector::render_shape(shape, format, bounds));
                }
                LayerContent::Text(text) => {
                    text.transform = transform.mul(&text.transform);
                    text.color = Color::BLACK;
                    for run in &mut text.runs {
                        run.style.color = Color::BLACK;
                    }
                    photocraft_text::TextEngine::new().render_layer(text, dpi, format);
                }
                LayerContent::Group(group) => walk(&mut group.children, transform, format, bounds, dpi),
                LayerContent::Fill(fill) => *fill = Fill::Solid(Color::BLACK),
                _ => panic!("coverage oracle supports vector fixtures"),
            }
        }
    }
    walk(&mut reference.layers, &transform, format, bounds, reference.resolution_dpi);
    let expected = photocraft_compose::flatten(&reference).to_rgba8();
    assert_eq!(actual.len() * 4, expected.pixels.len());
    // Integrate fractional covered areas: intersect=min(a,b), union=max(a,b).
    // Thresholding discards subpixel coverage and can turn a tiny AA difference
    // along a half-covered edge into an entire row/column of geometry errors.
    let (intersection, union) = actual
        .iter()
        .zip(expected.pixels.as_chunks::<4>().0.iter())
        .fold((0_u64, 0_u64), |(i, u), (a, b)| (i + u64::from(a[3].min(b[3])), u + u64::from(a[3].max(b[3]))));
    if union == 0 { 1.0 } else { intersection as f64 / union as f64 }
}
