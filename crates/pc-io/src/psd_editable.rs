//! Bounded Local Image editability beside interoperable PSD pixels/blocks.
//! A digest binds private values to the standard record; external edits invalidate them.
use photocraft_doc::{Layer, LayerContent, PixelFormat, Rect, ShapeLayer};
use photocraft_psd::{LayerRecord, TaggedBlock};
use serde_json::{Value, json};

pub(crate) const KEY: [u8; 4] = *b"liED";
const MAGIC: &[u8; 8] = b"LIEDIT01";
const MAX_BYTES: usize = 1 << 20;

fn fingerprint(record: &LayerRecord) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(format!("{:?}{:?}{:?}{:?}", record.rect, record.mask, record.blending_ranges, record.blend_mode).as_bytes());
    hash.update(&[record.opacity, record.clipping, record.flags.0]);
    for channel in &record.channels {
        hash.update(&channel.id.to_be_bytes());
        hash.update(format!("{:?}", channel.compression).as_bytes());
        hash.update(&(channel.data.len() as u64).to_be_bytes());
        hash.update(&channel.data);
    }
    for block in record.blocks.iter().filter(|block| block.key != KEY) {
        hash.update(&block.key);
        hash.update(&(block.data.len() as u64).to_be_bytes());
        hash.update(&block.data);
    }
    hash.finalize().to_hex().to_string()
}

pub(crate) fn attach(layer: &Layer, record: &mut LayerRecord) -> Result<(), String> {
    record.blocks.retain(|block| block.key != KEY);
    let content = match &layer.content {
        LayerContent::Text(text) if layer.vector.on_path.is_none() => {
            json!({"kind":"text", "runs":text.char_runs(), "paragraphs":text.paragraph_runs()})
        }
        LayerContent::Shape(shape)
            if layer.vector_mask.is_some() && layer.vector.appearance.is_none() && layer.vector.live_shape.is_none() && layer.vector.corners.is_empty() =>
        {
            json!({"kind":"shape", "path":shape.path, "fill":shape.fill, "stroke":shape.stroke, "live":shape.live, "mask":layer.vector_mask})
        }
        _ => return Ok(()),
    };
    let bytes = serde_json::to_vec(&json!({"version":1, "record":fingerprint(record), "content":content})).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("native editable PSD record exceeds 1 MiB".into());
    }
    let mut payload = MAGIC.to_vec();
    payload.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    payload.extend_from_slice(&bytes);
    if payload.len() % 2 == 1 {
        payload.push(0);
    }
    record.blocks.push(TaggedBlock::new(KEY, payload));
    Ok(())
}

const MAX_GEOMETRY_POINTS: usize = 262_144;

/// Admit untrusted native geometry before calling either the flattener or stroker.
/// This digest binds edits, but is not authentication of private JSON contents.
fn shape_geometry_cost(shape: &ShapeLayer, mask: &photocraft_doc::VectorMask) -> Result<usize, String> {
    use photocraft_doc::{LineCap, LineJoin, StrokeAlign};
    let bad = || "native PSD shape exceeds the geometry/stroker work budget".to_string();
    photocraft_vector::appearance::validate(&photocraft_vector::appearance::legacy_appearance(shape))?;
    if !mask.density.is_finite() || !(0.0..=1.0).contains(&mask.density) || !mask.feather.is_finite() || !(0.0..=1000.).contains(&mask.feather) {
        return Err(bad());
    }
    let metrics = |path: &photocraft_doc::Path, tolerance: f64| -> Result<(usize, usize, f64), String> {
        if path.subpaths.len() > 16384 {
            return Err(bad());
        }
        let mut knots = 0usize;
        let mut points = path.subpaths.len();
        let mut length = 0.0;
        for subpath in &path.subpaths {
            knots = knots.checked_add(subpath.knots.len()).ok_or_else(bad)?;
            if knots > 16384 {
                return Err(bad());
            }
            if subpath
                .knots
                .iter()
                .flat_map(|knot| [knot.anchor, knot.in_ctrl, knot.out_ctrl])
                .any(|point| !point.x.is_finite() || !point.y.is_finite() || point.x.abs() > 1_000_000. || point.y.abs() > 1_000_000.)
            {
                return Err(bad());
            }
            for segment in subpath.segments() {
                points = points.checked_add(photocraft_vector::segment_pieces(&segment, tolerance)).ok_or_else(bad)?;
                if points > MAX_GEOMETRY_POINTS {
                    return Err(bad());
                }
                // A Bezier's control-polygon length bounds its arc length without
                // flattening/allocating it, so the dash work estimate is conservative.
                length += segment.windows(2).map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y)).sum::<f64>();
            }
        }
        Ok((points, knots, length))
    };
    let tolerance = photocraft_vector::DEFAULT_TOLERANCE * 0.1;
    let (points, knots, length) = metrics(&shape.path, tolerance)?;
    let (mask_points, _, _) = metrics(&mask.path, tolerance)?;
    let mut estimate = points as f64 + mask_points as f64;
    if let Some(stroke) = &shape.stroke {
        // Kurbo's offset tolerance scales with width. Reject under-resolved widths
        // instead of allowing arbitrarily small tolerances in its subdivision loop.
        if stroke.width > 0.0 && stroke.width < 0.001 {
            return Err(bad());
        }
        let mut style = photocraft_vector::stroke_style(stroke);
        if shape.path.subpaths.iter().all(|subpath| subpath.closed) && stroke.align != StrokeAlign::Center {
            style.width *= 2.0;
        }
        let period: f64 = style.dashes.iter().sum();
        let progress = 1e-6f64.max(length * f64::EPSILON * 64.0);
        if !style.dashes.is_empty() && (period <= 0.0 || !period.is_finite() || style.dashes.iter().any(|dash| *dash > 0.0 && *dash < progress)) {
            return Err(bad());
        }
        let pieces = if period > 0.0 {
            (length / period + knots as f64 + shape.path.subpaths.len() as f64).ceil() * style.dashes.len() as f64
        } else {
            shape.path.subpaths.len() as f64
        };
        if !pieces.is_finite() || pieces > 100_000.0 {
            return Err(bad());
        }
        if style.width > 0.0 && stroke.opacity > 0.0 {
            let stroke_tolerance = photocraft_vector::DEFAULT_TOLERANCE.min(style.width * 1e-5);
            let (stroke_points, _, _) = metrics(&shape.path, stroke_tolerance * 0.1)?;
            // segment_pieces has a 1e-7 floor; account for tighter stroke tolerances.
            let stroke_points = stroke_points as f64 * (1e-7 / (stroke_tolerance * 0.1)).sqrt().max(1.0);
            let radius = style.width / 2.0;
            let step = (2.0 * (1.0 - stroke_tolerance / radius).clamp(-1.0, 1.0).acos()).min(std::f64::consts::FRAC_PI_2);
            let circle = (std::f64::consts::TAU / step).ceil().max(8.0);
            estimate += stroke_points * 16.0
                + knots as f64 * if style.join == LineJoin::Round { circle } else { 8.0 }
                + pieces * if style.cap == LineCap::Round { 2.0 * circle } else { 8.0 };
        }
    }
    if !estimate.is_finite() || estimate > MAX_GEOMETRY_POINTS as f64 {
        return Err(bad());
    }
    // Reserve space for flattened points, stroke outlines, and rasterizer edges.
    (estimate.ceil() as usize).checked_mul(128).ok_or_else(bad)
}

pub(crate) fn restore(layer: &mut Layer, record: &LayerRecord, format: PixelFormat, canvas: Rect, remaining: &mut usize) -> Result<(), String> {
    let Some(block) = record.block(&KEY) else { return Ok(()) };
    let bad = || "invalid native editable PSD record; standard layer retained".to_string();
    if !block.data.starts_with(MAGIC) || block.data.len() < 12 || block.data.len() > MAX_BYTES + 15 {
        return Err(bad());
    }
    let len = u32::from_be_bytes([block.data[8], block.data[9], block.data[10], block.data[11]]) as usize;
    let end = 12usize.checked_add(len).ok_or_else(bad)?;
    let bytes = block.data.get(12..end).ok_or_else(bad)?;
    if block.data.len() - end > 3 || block.data[end..].iter().any(|byte| *byte != 0) {
        return Err(bad());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| bad())?;
    if value["version"].as_u64() != Some(1) {
        return Err(bad());
    }
    if value["record"].as_str() != Some(fingerprint(record).as_str()) {
        return Err("stale native editable PSD record ignored after external layer edits".into());
    }
    let content = &value["content"];
    match content["kind"].as_str() {
        Some("text") => {
            let LayerContent::Text(text) = &mut layer.content else { return Err(bad()) };
            let runs: Vec<photocraft_doc::text::TextRun> = serde_json::from_value(content["runs"].clone()).map_err(|_| bad())?;
            let paragraphs: Vec<photocraft_doc::text::ParagraphRun> = serde_json::from_value(content["paragraphs"].clone()).map_err(|_| bad())?;
            if runs.len() > 16384
                || paragraphs.len() > 16384
                || runs.iter().try_fold(0usize, |n, run| n.checked_add(run.len)) != Some(text.text.len())
                || paragraphs.iter().try_fold(0usize, |n, run| n.checked_add(run.len)) != Some(text.text.len())
            {
                return Err(bad());
            }
            text.runs = runs;
            text.paragraphs = paragraphs;
            text.sync_summary();
        }
        Some("shape") => {
            let mut shape = ShapeLayer {
                path: serde_json::from_value(content["path"].clone()).map_err(|_| bad())?,
                fill: serde_json::from_value(content["fill"].clone()).map_err(|_| bad())?,
                stroke: serde_json::from_value(content["stroke"].clone()).map_err(|_| bad())?,
                live: serde_json::from_value(content["live"].clone()).map_err(|_| bad())?,
                ..Default::default()
            };
            let mask: Option<photocraft_doc::VectorMask> = serde_json::from_value(content["mask"].clone()).map_err(|_| bad())?;
            let mask = mask.ok_or_else(bad)?;
            let geometry_cost = shape_geometry_cost(&shape, &mask)?;
            if geometry_cost > *remaining {
                return Err("native PSD shape exceeds the shared geometry memory budget".into());
            }
            *remaining -= geometry_cost;
            // Keep the original canvas as the gradient frame. The renderer clips to
            // compiled ink/stroke bounds; a small shape on a large canvas is cheap.
            let compiled = photocraft_vector::CompiledShape::new(&shape, photocraft_vector::DEFAULT_TOLERANCE, canvas);
            let ink = compiled.bounds().map_or(Rect::EMPTY, |bounds| bounds.intersect(&canvas));
            let pixels = (ink.width() as usize).checked_mul(ink.height() as usize).ok_or_else(bad)?;
            let cost = if ink.is_empty() {
                0
            } else {
                (ink.width() as usize)
                    .checked_add(128)
                    .and_then(|w| (ink.height() as usize).checked_add(128).and_then(|h| w.checked_mul(h)))
                    .and_then(|n| n.checked_mul(format.channels()))
                    .and_then(|n| n.checked_mul(format.sample.bytes()))
                    .and_then(|n| n.checked_mul(3))
                    .ok_or_else(bad)?
            };
            if pixels > 16 * 1024 * 1024 || cost > *remaining {
                return Err("native PSD shape exceeds the shared import rendering budget".into());
            }
            *remaining -= cost;
            shape.cache = Some(compiled.render(format, ink));
            layer.content = LayerContent::Shape(shape);
            layer.vector_mask = Some(mask);
        }
        _ => return Err(bad()),
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use photocraft_doc::{Color, Fill, GradientStyle, Path, ShapeStroke, Subpath, Surface, VectorMask};

    fn shape_layer() -> Layer {
        let shape = ShapeLayer { path: photocraft_vector::shapes::rect(0., 0., 16., 16.), fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
        let mut layer = Layer::new("small native shape", LayerContent::Shape(shape));
        layer.vector_mask = Some(VectorMask::new(photocraft_vector::shapes::rect(0., 0., 12., 12.)));
        layer
    }

    #[test]
    fn tiny_shape_private_roundtrip_on_24mp_canvas_charges_ink_and_keeps_gradient_frame() {
        let canvas = Rect::new(0, 0, 6000, 4000);
        let format = PixelFormat::RGBA8;
        let mut layer = shape_layer();
        let LayerContent::Shape(shape) = &mut layer.content else { panic!("shape") };
        let mut gradient = Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 0., 100., GradientStyle::Linear, false);
        if let Fill::Gradient { align, .. } = &mut gradient {
            *align = false;
        }
        shape.fill = Some(gradient);
        let expected = photocraft_vector::render_shape(shape, format, canvas);
        let incorrect_frame = photocraft_vector::render_shape(shape, format, Rect::new(0, 0, 16, 16));
        let ink = expected.content_bounds();
        assert_ne!(expected.to_interleaved(ink), incorrect_frame.to_interleaved(ink));
        let mut record = LayerRecord::default();
        attach(&layer, &mut record).unwrap();
        for _ in 0..2 {
            let mut restored = Layer::new("fallback", LayerContent::Raster(Surface::new(format)));
            // Far below a full 24MP raster. The serialized private record is tiny too.
            let mut remaining = 2 * 1024 * 1024;
            restore(&mut restored, &record, format, canvas, &mut remaining).unwrap();
            assert!(remaining > 1024 * 1024);
            let LayerContent::Shape(shape) = &restored.content else { panic!("editability lost") };
            let cache = shape.cache.as_ref().unwrap();
            assert_eq!(cache.content_bounds(), ink);
            assert_eq!(cache.to_interleaved(ink), expected.to_interleaved(ink));
            assert_eq!(restored.vector_mask, layer.vector_mask);
            attach(&restored, &mut record).unwrap();
            assert!(record.block(&KEY).unwrap().data.len() < 8192);
        }
    }

    #[test]
    fn private_dash_payloads_are_rejected_before_stroker_or_allocation() {
        let layer = shape_layer();
        let mut record = LayerRecord::default();
        attach(&layer, &mut record).unwrap();
        let original = record.clone();
        for (width, dashes, extent) in [
            (1.0, vec![1e-30, 1e-30], 1.0),     // subtraction would not advance the dash iterator
            (1.0, vec![0.0, 1e-30], 1.0),       // dot fallback also needs positive progress
            (1.0, vec![1e-6, 1e-6], 1.0),       // finite progression but excessive total dash work
            (1.0, vec![1.0, 1.0], 1_000_000.0), // long path / normal dashes exceeds work budget
            (1e-30, vec![], 1.0),               // tiny stroke tolerance must not drive subdivision
            (1.0, vec![-1.0, 1.0], 1.0),        // malformed negative pattern
        ] {
            record = original.clone();
            let block = record.blocks.iter_mut().find(|block| block.key == KEY).unwrap();
            let len = u32::from_be_bytes(block.data[8..12].try_into().unwrap()) as usize;
            let mut data: Value = serde_json::from_slice(&block.data[12..12 + len]).unwrap();
            data["content"]["path"] = serde_json::to_value(Path::new(vec![Subpath::polyline(&[(0., 0.), (extent, 0.)])])).unwrap();
            data["content"]["stroke"] = serde_json::to_value(ShapeStroke { width, dashes, ..Default::default() }).unwrap();
            // Keep the original matching standard-record digest: it is not authentication.
            let data = serde_json::to_vec(&data).unwrap();
            block.data = MAGIC.to_vec();
            block.data.extend_from_slice(&(data.len() as u32).to_be_bytes());
            block.data.extend_from_slice(&data);
            let mut fallback = Layer::new("fallback", LayerContent::Raster(Surface::new(PixelFormat::RGBA8)));
            let mut remaining = 2 * 1024 * 1024;
            assert!(restore(&mut fallback, &record, PixelFormat::RGBA8, Rect::new(0, 0, 64, 64), &mut remaining).is_err());
            assert_eq!(remaining, 2 * 1024 * 1024, "rejected before geometry compilation/render admission");
            assert!(matches!(fallback.content, LayerContent::Raster(_)));
        }
    }

    #[test]
    fn private_shape_geometry_and_paint_budgets_are_cumulative() {
        let mut layer = shape_layer();
        let mask = layer.vector_mask.as_ref().unwrap();
        let LayerContent::Shape(shape) = &mut layer.content else { panic!("shape") };
        shape.stroke = Some(ShapeStroke { dashes: vec![2., 1.], ..Default::default() });
        assert!(shape_geometry_cost(shape, mask).is_ok(), "ordinary dashed stroke remains supported");
        shape.path = Path::new((0..16384).map(|_| Subpath::polygon(&[(0., 0.), (1., 1.)])).collect());
        assert!(shape_geometry_cost(shape, mask).is_err());
        shape.path = photocraft_vector::shapes::rect(0., 0., 16., 16.);
        shape.fill = Some(Fill::gradient(vec![(0.5, Color::BLACK); 4097], 0., 100., GradientStyle::Linear, false));
        assert!(shape_geometry_cost(shape, mask).is_err());
    }
}
