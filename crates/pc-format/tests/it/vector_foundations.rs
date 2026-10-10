//! Frozen v1 loading and additive vector metadata/storage gates.
use photocraft_doc::vector_design::{appearance::*, drawing::*, live::*, structure::*, text::*};
use photocraft_doc::*;
use photocraft_format::*;

#[test]
fn old_v1_documents_load() {
    let bytes = include_bytes!("../fixtures/vector-v1.pcraft");
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
    let mut d = load_from_bytes(include_bytes!("../fixtures/vector-v1.pcraft")).unwrap();
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
fn vector_foundations_empty_fields_are_absent_in_v1_output() {
    let d = load_from_bytes(include_bytes!("../fixtures/vector-v1.pcraft")).unwrap();
    let m = read_manifest(&save_to_bytes(&d, &Default::default()).unwrap()).unwrap();
    let json = serde_json::to_value(m).unwrap();
    assert!(json["document"].get("symbols").is_none());
    assert!(json["document"].get("design_units").is_none());
    assert!(json["document"]["layers"][0].get("vector").is_none());
    assert_eq!(FORMAT_VERSION, 2); // Symbol content is atomically activated in package 15
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
