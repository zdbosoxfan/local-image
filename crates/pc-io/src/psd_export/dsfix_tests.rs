use super::*;
#[test]
fn dsfix_shape_cache_block_and_record_share_masked_pixels() {
    let mut doc = Document::new("cache reuse", photocraft_geom::Size::new(32, 32), ColorMode::Rgb, SampleType::U8);
    let mut shape = photocraft_doc::ShapeLayer {
        path: photocraft_vector::shapes::rect(0., 0., 16., 16.),
        fill: Some(photocraft_doc::Fill::Solid(photocraft_doc::Color::BLACK)),
        ..Default::default()
    };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    let mut layer = Layer::new("shape", LayerContent::Shape(shape));
    layer.vector.appearance = Some(photocraft_doc::vector_design::appearance::Appearance {
        items: vec![photocraft_doc::vector_design::appearance::AppearanceItem::Fill(photocraft_doc::vector_design::appearance::FillItem::new(
            photocraft_doc::Fill::Solid(photocraft_doc::Color::BLACK),
        ))],
    });
    layer.vector_mask = Some(photocraft_doc::VectorMask::new(photocraft_vector::shapes::rect(0., 0., 8., 16.)));
    let cache = fallback::bounded_cache(&layer, doc.bounds()).unwrap();
    let block = fallback::cache_block(&cache, fallback::CacheKind::Shape, false, photocraft_compose::paint_bounds(&layer));
    doc.layers.push(layer);
    let psd = document_to_psd(&doc);
    let record = &psd.layer_info.as_ref().unwrap().layers[0];
    assert_eq!(record.blocks.iter().find(|b| b.key == fallback::CACHE_KEY).unwrap().data, block);
    // Saved save-fix 745a5797 deliberately rasterizes an independent shape
    // mask: PSD's one vector-mask slot cannot represent both geometries.
    assert!(!record.blocks.iter().any(|b| fallback::proxy_key(&b.key)), "masked shape must remain a clipped raster fallback");
    assert_eq!(record.rect.width(), 8);
    assert_eq!(record.rect.height(), 16);
    let alpha = record.channels.iter().find(|c| c.id == -1).unwrap().decode(8, 16, 8, Version::Psd).unwrap();
    assert!(alpha.iter().all(|a| *a == 255));
}
