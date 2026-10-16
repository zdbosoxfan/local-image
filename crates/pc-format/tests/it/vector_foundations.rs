//! Frozen basic-document loading and additive vector metadata/storage gates.
use photocraft_doc::vector_design::{appearance::*, drawing::*, live::*, structure::*, text::*};
use photocraft_doc::*;
use photocraft_format::*;

#[test]
fn frozen_basic_documents_load() {
    let bytes = include_bytes!("../fixtures/vector-basic.pcraft");
    let m = read_manifest(bytes).unwrap();
    assert_eq!(m.format_version, FORMAT_VERSION);
    let d = load_from_bytes(bytes).unwrap();
    assert!(d.symbols.is_empty());
    assert_eq!(d.design_units, DesignUnits::Pixels);
    assert_eq!(d.layers.len(), 2);
    assert!(d.layers.iter().all(|l| l.vector.is_empty()));
    let LayerContent::Shape(sh) = &d.layers[0].content else { panic!("shape") };
    assert_eq!(sh.path.subpaths[0].knots.len(), 4);
    assert!(sh.stroke.is_some());
    assert!(sh.live.is_some());
    assert_eq!(d.layers[0].vector_mask.as_ref().unwrap().path, sh.path);
    assert_eq!(d.paths[0].path, sh.path);
    assert_eq!(d.work_path.as_ref(), Some(&sh.path));
    assert_eq!(d.clipping_path.as_ref().unwrap().name, "Saved outline");
    let back = load_from_bytes(&save_to_bytes(&d, &Default::default()).unwrap()).unwrap();
    assert_eq!(back, d);
}

fn extended_document() -> Document {
    let mut d = load_from_bytes(include_bytes!("../fixtures/vector-basic.pcraft")).unwrap();
    let p = d.paths[0].path.clone();
    d.design_units = DesignUnits::Millimeters;
    d.layers[0].vector = VectorData {
        corners: vec![KnotCorner { subpath: 0, knot: 1, corner: Corner { radius: 2., kind: CornerKind::Chamfer } }],
        live_shape: Some(DesignShape { rect: [2., 2., 16., 14.], angle: 0.25, parameters: ShapeParameters::Star { points: 5, inner: 0.5, corner: 2. } }),
        appearance: Some(Appearance {
            items: vec![
                AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::BLACK))),
                AppearanceItem::Stroke(StrokeItem {
                    profile: Some(WidthProfile { points: vec![WidthPoint { t: 0.5, left: 2., right: 3. }] }),
                    brush: Some(BrushRef { id: "brush-1".into() }),
                    start_arrow: Some(Arrowhead::Triangle),
                    ..Default::default()
                }),
            ],
        }),
        constraints: Some(Constraints { right: true, fixed_width: true, ..Default::default() }),
        clip: Some(p.clone()),
        ..Default::default()
    };
    d.layers[1].vector.on_path = Some(PathText { path: p, start: 0.1, end: 0.9, flip: true, align: PathTextAlign::Center });
    let mut group = Layer::group("Live", vec![d.layers[0].duplicate()]);
    group.vector.live = Some(LiveGroup::Blend(BlendSpec { spacing: BlendSpacing::Steps(5), ..Default::default() }));
    d.layers.push(group);
    let mut pixel = Layer::raster("Master pixels", d.pixel_format());
    pixel.surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(0, 0, 1, 1), &[0.1, 0.2, 0.3, 1.]);
    d.symbols.push(SymbolDef { id: SymbolId(1), name: "Master".into(), layers: vec![d.layers[0].duplicate(), pixel] });
    d
}
#[test]
fn vector_foundations_native_roundtrip_and_id_remapping() {
    let d = extended_document();
    let bytes = save_to_bytes(&d, &Default::default()).unwrap();
    assert_eq!(load_from_bytes(&bytes).unwrap(), d);
    let remap = load_from_bytes_with(&bytes, &LoadOptions { preserve_ids: false, ..Default::default() }).unwrap();
    assert_ne!(remap.symbols[0].layers[0].id, d.symbols[0].layers[0].id);
    assert_eq!(remap.symbols[0].id, SymbolId(1));
    assert_eq!(remap.layers[0].vector, d.layers[0].vector);
}
#[test]
fn vector_foundations_directory_storage_keeps_symbol_tiles() {
    let d = extended_document();
    let dir = crate::common::temp_dir("vector-symbols");
    let mut writer = PcraftWriter::new();
    writer.save_dir(&d, &dir, &Default::default()).unwrap();
    writer.save_dir(&d, &dir, &Default::default()).unwrap();
    assert_eq!(load_path(&dir).unwrap(), d);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn vector_foundations_empty_fields_are_absent_in_output() {
    let d = load_from_bytes(include_bytes!("../fixtures/vector-basic.pcraft")).unwrap();
    let m = read_manifest(&save_to_bytes(&d, &Default::default()).unwrap()).unwrap();
    let json = serde_json::to_value(m).unwrap();
    assert!(json["document"].get("symbols").is_none());
    assert!(json["document"].get("design_units").is_none());
    assert!(json["document"]["layers"][0].get("vector").is_none());
    assert_eq!(FORMAT_VERSION, 3); // Independent group isolation is guarded by v3.
}
#[test]
fn vector_foundations_symbol_depth_uses_the_existing_limit() {
    let mut d = extended_document();
    let mut layer = Layer::raster("Deep", d.pixel_format());
    for _ in 0..=MAX_GROUP_DEPTH {
        layer = Layer::group("Group", vec![layer]);
    }
    d.symbols[0].layers = vec![layer];
    assert!(matches!(save_to_bytes(&d, &Default::default()), Err(FormatError::LimitExceeded(_))));
}

#[test]
fn vector_integration_repeat_envelope_styles_swatches_frame_native_roundtrip() {
    use photocraft_doc::vector_design::{styles, swatches::*};
    let mut doc = extended_document();
    for spec in [
        LiveGroup::Repeat(RepeatSpec::Radial { count: 9, center: [12., 12.], angle: 0.3 }),
        LiveGroup::Repeat(RepeatSpec::Grid { rows: 2, cols: 3, spacing: [4., 5.] }),
        LiveGroup::Repeat(RepeatSpec::Mirror { origin: [12., 12.], angle: 0.5 }),
        LiveGroup::Envelope(EnvelopeSpec::Warp { style: "arc".into(), bend: 0.2, horizontal: 0.1, vertical: -0.1 }),
        LiveGroup::Envelope(EnvelopeSpec::Mesh { rows: 2, cols: 2, points: vec![[0., 0.], [1., 0.], [0., 1.], [1., 1.]] }),
        LiveGroup::Envelope(EnvelopeSpec::TopObject { path: doc.paths[0].path.clone() }),
    ] {
        let mut layer = Layer::group("Editable live geometry", vec![doc.layers[0].duplicate()]);
        layer.vector.live = Some(spec);
        doc.layers.push(layer);
    }
    doc.layers[1].vector.frame_compose = Some(FrameCompose { enabled: true, balanced: true, optical_margins: true, grid_spacing: 12., grid_origin: 0.25 });
    doc.graphic_styles = styles::starter_library();
    let style = &doc.graphic_styles[0];
    doc.layers[0].vector.graphic_style =
        Some(styles::StyleLink { style: style.id, applied: style.attributes.clone(), character_runs: Vec::new(), paragraph_runs: Vec::new() });
    doc.swatches.swatches.push(Swatch {
        id: SwatchId(1),
        name: "Native global ink".into(),
        paint: Fill::Solid(Color::BLACK),
        global: true,
        spot: false,
        group: None,
        stops: Vec::new(),
    });
    doc.swatches.links.push(PaintLink {
        target: PaintTarget { symbol: None, layer: doc.layers[0].id, paint: PaintSlot::Appearance(0), stop: None },
        reference: SwatchRef { swatch: SwatchId(1), tint: 100. },
    });
    let bytes = save_to_bytes(&doc, &Default::default()).unwrap();
    assert_eq!(load_from_bytes(&bytes).unwrap(), doc);
    let remapped = load_from_bytes_with(&bytes, &LoadOptions { preserve_ids: false, ..Default::default() }).unwrap();
    assert_eq!(remapped.swatches.links[0].target.layer, remapped.layers[0].id);
    assert_eq!(remapped.graphic_styles, doc.graphic_styles);
    for (a, b) in remapped.layers.iter().zip(&doc.layers) {
        assert_eq!(a.vector.live, b.vector.live);
        assert_eq!(a.vector.frame_compose, b.vector.frame_compose);
        assert_eq!(a.vector.graphic_style, b.vector.graphic_style);
    }
}
#[test]
fn vector_integration_native_rejects_duplicate_names_bad_frame_and_mesh() {
    let mut doc = extended_document();
    doc.graphic_styles = photocraft_doc::vector_design::styles::starter_library();
    doc.graphic_styles[1].name = doc.graphic_styles[0].name.clone();
    assert!(save_to_bytes(&doc, &Default::default()).is_err());
    doc.graphic_styles.clear();
    doc.layers[1].vector.frame_compose = Some(FrameCompose { grid_spacing: f32::NAN, ..Default::default() });
    assert!(save_to_bytes(&doc, &Default::default()).is_err());
    doc.layers[1].vector.frame_compose = None;
    doc.layers[0].vector.appearance = Some(Appearance {
        items: vec![AppearanceItem::Mesh(MeshFillItem {
            paint: GradientMesh { rows: 1, cols: 1, points: Vec::new(), dither: false, seed: 0 },
            opacity: 1.,
            blend: BlendMode::Normal,
            visible: true,
        })],
    });
    assert!(save_to_bytes(&doc, &Default::default()).is_err());
}
