#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::vector_design::{
    appearance::{Appearance, WidthPoint, WidthProfile},
    drawing::{Corner, CornerKind, DesignShape, KnotCorner, ShapeParameters},
};
use photocraft_doc::{Color, ColorMode, SampleType, ShapeLayer, ShapeStroke, Size, Subpath};
pub(super) fn fixture(n: usize) -> (Session, Vec<LayerId>) {
    let mut doc = Document::new("Transforms", Size::new(256, 256), ColorMode::Rgb, SampleType::U8);
    let mut ids = Vec::new();
    for i in 0..n {
        let x = 20. + i as f64 * 50.;
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, 20.), (x + 20., 20.), (x + 20., 40.), (x, 40.)])]),
            fill: Some(Fill::Solid(Color::BLACK)),
            stroke: Some(ShapeStroke { width: 4., ..Default::default() }),
            ..Default::default()
        };
        crate::vector_cmds::refresh_shape(&doc, &mut sh);
        let layer = Layer::new("Square", LayerContent::Shape(sh));
        ids.push(layer.id);
        doc.layers.push(layer);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    s.active_mut().unwrap().active_layer = ids.last().copied();
    s.active_mut().unwrap().selected_layers = ids.clone();
    (s, ids)
}
fn request(s: &Session, ids: &[LayerId], matrix: [f64; 6]) -> Commit {
    Commit {
        capture: Capture::begin(s, ids).unwrap(),
        operation: Operation::Affine { matrix },
        options: Options::default(),
        copy: false,
        conversion: ConversionChoice::Ask,
    }
}
fn shape(doc: &Document, id: LayerId) -> &ShapeLayer {
    let LayerContent::Shape(sh) = &doc.layer(id).unwrap().content else { panic!("not shape") };
    sh
}
#[test]
fn transform34_engine_rotate_clicked_node_exact_preview_atomic_undo() {
    let (mut s, ids) = fixture(2);
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let c = request(&s, &ids, rotate(Point::new(20., 20.), 90.).unwrap());
    let prepared = prepare(&s, &c).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert_eq!(shape(&prepared.document, ids[0]).path.subpaths[0].knots[0].anchor, Point::new(20., 20.));
    let p = shape(&prepared.document, ids[0]).path.subpaths[0].knots[1].anchor;
    assert!((p.x - 20.).abs() < 1e-9 && (p.y - 0.).abs() < 1e-9);
    commit(&mut s, &c).unwrap();
    assert_eq!(*s.active().unwrap().doc, prepared.document);
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.active().unwrap().revision, revision + 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
}
#[test]
fn transform34_engine_symbol_conversion_choice_atomic_and_retains_original_with_copy() {
    use photocraft_doc::{SymbolDef, SymbolId, SymbolInstance};
    let (mut s, ids) = fixture(1);
    let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let layers = std::mem::take(&mut doc.layers);
    doc.symbols.push(SymbolDef { id: SymbolId(100), name: "Square".into(), layers });
    let instance = Layer::new("Instance", LayerContent::Symbol(SymbolInstance { symbol: SymbolId(100), transform: Affine::IDENTITY, cache: None }));
    let id = instance.id;
    doc.layers.push(instance);
    let mut c = request(&s, &[id], Affine::IDENTITY.m);
    c.operation =
        Operation::Distort { source: [20., 20., 20., 20.], quad: [[25., 20.], [35., 20.], [40., 40.], [20., 40.]], perspective: false, tolerance: 0.25 };
    let before = s.active().unwrap().doc.clone();
    assert_eq!(conversion_targets(&before, &[id]), vec![id]);
    assert!(commit(&mut s, &c).is_err());
    c.conversion = ConversionChoice::Envelope;
    assert!(commit(&mut s, &c).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    c.conversion = ConversionChoice::Curves;
    c.copy = true;
    let copied = commit(&mut s, &c).unwrap();
    let doc = &s.active().unwrap().doc;
    assert!(matches!(doc.layer(id).unwrap().content, LayerContent::Symbol(_)));
    let LayerContent::Group(g) = &doc.layer(copied[0]).unwrap().content else { panic!() };
    assert_ne!(g.children[0].id, ids[0]);
    let LayerContent::Shape(sh) = &g.children[0].content else { panic!() };
    assert_eq!(sh.path.subpaths[0].knots[0].anchor, Point::new(25., 20.));
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn transform34_conversion_styled_and_path_text_preserve_paints_decorations_atomic_copy() {
    use photocraft_doc::{
        TextLayer,
        text::{CharStyle, TextRun},
        vector_design::text::{PathText, PathTextAlign},
    };
    for attached in [false, true] {
        let (mut s, ids) = fixture(1);
        let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
        let l = doc.layer_mut(ids[0]).unwrap();
        l.content = LayerContent::Text(TextLayer {
            text: "AB".into(),
            font_family: "Inter".into(),
            size_pt: 20.,
            runs: vec![
                TextRun { len: 1, style: CharStyle { font_family: "Inter".into(), size_pt: 20., color: Color::BLACK, underline: true, ..Default::default() } },
                TextRun { len: 1, style: CharStyle { font_family: "Inter".into(), size_pt: 20., color: Color::WHITE, ..Default::default() } },
            ],
            transform: if attached { Affine::IDENTITY } else { Affine::translate(20., 100.) },
            ..Default::default()
        });
        if attached {
            l.vector.on_path = Some(PathText {
                path: Path::new(vec![Subpath::polyline(&[(20., 100.), (220., 100.)])]),
                start: 0.,
                end: 1.,
                flip: false,
                align: PathTextAlign::Baseline,
            });
        }
        let before = s.active().unwrap().doc.clone();
        let mut c = request(&s, &ids, Affine::IDENTITY.m);
        c.operation =
            Operation::Distort { source: [0., 0., 256., 256.], quad: [[10., 5.], [266., 5.], [266., 261.], [10., 261.]], perspective: false, tolerance: 0.25 };
        c.conversion = ConversionChoice::Curves;
        c.copy = true;
        let copied = commit(&mut s, &c).unwrap();
        let doc = &s.active().unwrap().doc;
        assert_eq!(doc.layer(ids[0]).unwrap(), before.layer(ids[0]).unwrap());
        let converted = doc.layer(copied[0]).unwrap();
        assert!(converted.vector.on_path.is_none());
        let LayerContent::Group(g) = &converted.content else { panic!() };
        assert_eq!(g.children.len(), 2);
        let black = g.children.iter().find(|l| matches!(&l.content,LayerContent::Shape(sh) if sh.fill == Some(Fill::Solid(Color::BLACK)))).unwrap();
        let LayerContent::Shape(sh) = &black.content else { panic!() };
        assert!(sh.path.subpaths.len() >= 3, "A outer/inner contours and underline must survive");
        let underline =
            sh.path.subpaths.iter().find(|sp| sp.knots.len() == 4 && sp.knots.iter().all(|k| k.anchor == k.in_ctrl && k.anchor == k.out_ctrl)).unwrap();
        assert!(underline.knots.iter().all(|k| k.anchor.y >= 100.));
        assert_eq!(s.journal.len(), 1);
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
    }
}
#[test]
fn transform34_engine_reshape_copy_again_remaps_node_targets() {
    let (mut s, ids) = fixture(1);
    let mut c = request(&s, &ids, Affine::IDENTITY.m);
    c.operation = Operation::Reshape { nodes: vec![Node { layer: ids[0], subpath: 0, knot: 0 }], delta: [0., 5.], radius: 10. };
    c.copy = true;
    let first = commit(&mut s, &c).unwrap();
    let repeated = again_request(&s, &first).unwrap();
    let second = commit(&mut s, &repeated).unwrap();
    let doc = &s.active().unwrap().doc;
    assert_eq!(shape(doc, ids[0]).path.subpaths[0].knots[0].anchor.y, 20.);
    assert_eq!(shape(doc, first[0]).path.subpaths[0].knots[0].anchor.y, 25.);
    assert_eq!(shape(doc, second[0]).path.subpaths[0].knots[0].anchor.y, 30.);
    assert_eq!(doc.layer_count(), 3);
}
#[test]
fn transform34_engine_group_clip_linked_vector_mask_and_effects() {
    let (mut s, _) = fixture(1);
    let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let mut group = Layer::group("Clip group", std::mem::take(&mut doc.layers));
    let square = Path::new(vec![Subpath::polygon(&[(20., 20.), (40., 20.), (40., 40.), (20., 40.)])]);
    group.vector.clip = Some(square.clone());
    group.vector_mask = Some(photocraft_doc::VectorMask::new(square));
    group.effects.items.push(photocraft_doc::Effect::default_drop_shadow());
    let id = group.id;
    doc.layers.push(group);
    let mut c = request(&s, &[id], scale(Point::new(20., 20.), 2., 2.).unwrap());
    c.options.scale_strokes_effects = true;
    let p = prepare(&s, &c).unwrap();
    let group = p.document.layer(id).unwrap();
    assert_eq!(group.vector.clip.as_ref().unwrap().subpaths[0].knots[2].anchor, Point::new(60., 60.));
    assert_eq!(group.vector_mask.as_ref().unwrap().path.subpaths[0].knots[2].anchor, Point::new(60., 60.));
    let photocraft_doc::Effect::DropShadow(shadow) = &group.effects.items[0] else { panic!() };
    assert_eq!(shadow.distance, 10.);
}
#[test]
fn transform34_engine_pattern_options_phase_similarity_and_provider_failure() {
    let path = Path::new(vec![Subpath::polygon(&[(20., 20.), (40., 20.), (40., 40.), (20., 40.)])]);
    let fill = Fill::Pattern { name: "Synthetic".into(), id: String::new(), scale: 1., angle: 0., phase: (3., 4.), link: true };
    let a = Affine { m: scale(Point::new(20., 20.), 2., 2.).unwrap() };
    let mut transformed = fill.clone();
    metadata::paint(&mut transformed, Some(a), true, Some((20., 20., 40., 40.)), &path.transform(&a)).unwrap();
    let Fill::Pattern { scale, phase, .. } = transformed else { panic!() };
    assert_eq!(scale, 2.);
    assert_eq!(phase, (6., 8.));
    let mut fixed = fill.clone();
    let a = Affine::translate(10., 5.);
    metadata::paint(&mut fixed, Some(a), false, Some((20., 20., 40., 40.)), &path.transform(&a)).unwrap();
    let Fill::Pattern { scale, phase, .. } = fixed else { panic!() };
    assert_eq!(scale, 1.);
    assert_eq!(phase, (-7., -1.));
    let mut invalid = fill;
    assert!(metadata::paint(&mut invalid, Some(Affine { m: shear(Point::new(20., 20.), 20., false).unwrap() }), true, None, &path).is_err());
}
#[test]
fn transform34_auxiliary_editable_text_effect_scaling_option() {
    let (mut s, ids) = fixture(1);
    let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let mut text = photocraft_doc::TextLayer {
        text: "A".into(),
        font_family: "Inter".into(),
        size_pt: 20.,
        transform: Affine::translate(20., 100.),
        ..Default::default()
    };
    crate::type_cmds::refresh(doc, &mut text);
    let l = doc.layer_mut(ids[0]).unwrap();
    l.content = LayerContent::Text(text);
    l.effects.items.push(photocraft_doc::Effect::default_drop_shadow());
    let mut c = request(&s, &ids, scale(Point::new(20., 100.), 2., 2.).unwrap());
    for (enabled, distance) in [(false, 5.), (true, 10.)] {
        c.options.scale_strokes_effects = enabled;
        let p = prepare(&s, &c).unwrap();
        let l = p.document.layer(ids[0]).unwrap();
        assert!(matches!(l.content, LayerContent::Text(_)));
        let photocraft_doc::Effect::DropShadow(shadow) = &l.effects.items[0] else { panic!() };
        assert_eq!(shadow.distance, distance);
    }
    assert!(s.journal.is_empty());
}
#[test]
fn transform34_auxiliary_entire_text_set_budget_before_provider_rasterizes() {
    let (mut s, _) = fixture(1);
    let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
    doc.size = Size::new(8192, 8192);
    doc.layers.clear();
    let mut text = photocraft_doc::TextLayer {
        text: "A".into(),
        font_family: "Inter".into(),
        size_pt: 300.,
        transform: Affine::translate(20., 300.),
        ..Default::default()
    };
    // Real appearance initialization, then sparse COW copies of the source cache.
    crate::type_cmds::refresh(doc, &mut text);
    for _ in 0..128 {
        doc.layers.push(Layer::new("Large text", LayerContent::Text(text.clone())));
    }
    let ids = doc.layers.iter().map(|l| l.id).collect::<Vec<_>>();
    let c = request(&s, &ids, scale(Point::new(0., 0.), 4., 4.).unwrap());
    let before = s.active().unwrap().doc.clone();
    let error = match prepare(&s, &c) {
        Ok(_) => panic!("aggregate budget must reject"),
        Err(e) => e.to_string(),
    };
    assert!(error.contains("512 MiB"), "{error}");
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
}
#[test]
fn transform34_auxiliary_patterns_off_nonlinear_geometry_retains_canvas_phase() {
    let source = Path::new(vec![Subpath::polygon(&[(20., 20.), (40., 20.), (40., 40.), (20., 40.)])]);
    let quad = [Point::new(35., 25.), Point::new(45., 25.), Point::new(50., 45.), Point::new(30., 45.)];
    let mapped = projective::map_path(&source, Map::bilinear([20., 20., 20., 20.], quad).unwrap(), 0.25).unwrap().path;
    let mut fill = Fill::Pattern { name: "Synthetic".into(), id: String::new(), scale: 1., angle: 0., phase: (3., 4.), link: true };
    metadata::paint(&mut fill, None, false, Some((20., 20., 40., 40.)), &mapped).unwrap();
    let Fill::Pattern { phase, .. } = fill else { panic!() };
    // Original canvas phase (23,24), transformed geometric bounds origin (30,25).
    assert_eq!(phase, (-7., -1.));
}
#[test]
fn transform34_corner_pattern_frame_rounded_geometry_keeps_canvas_phase() {
    let (mut s, ids) = fixture(1);
    let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let mut tile = photocraft_raster::Surface::new(doc.pixel_format());
    tile.write_region(photocraft_geom::Rect::new(0, 0, 2, 2), &[1.; 16]);
    let pattern = photocraft_doc::Pattern::new("Synthetic", tile, 2, 2);
    let pattern_id = pattern.id.clone();
    doc.patterns.push(pattern);
    let l = doc.layer_mut(ids[0]).unwrap();
    l.vector.corners = vec![KnotCorner { subpath: 0, knot: 0, corner: Corner { radius: 2., kind: CornerKind::Round } }];
    let LayerContent::Shape(sh) = &mut l.content else { panic!() };
    sh.path = Path::new(vec![Subpath::polygon(&[(20., 20.), (40., 40.), (0., 40.)])]);
    sh.stroke = None;
    sh.fill = Some(Fill::Pattern { name: "Synthetic".into(), id: pattern_id, scale: 1., angle: 0., phase: (3., 4.), link: true });
    let mut c = request(&s, &ids, scale(Point::new(0., 0.), 2., 2.).unwrap());
    c.options.patterns = false;
    let p = prepare(&s, &c).unwrap();
    let layer = p.document.layer(ids[0]).unwrap();
    let sh = shape(&p.document, ids[0]);
    let projected = super::super::corners::project_shape(sh, &layer.vector.corners).unwrap();
    let b = projected.path.control_bounds().unwrap();
    let Some(Fill::Pattern { phase, .. }) = &sh.fill else { panic!() };
    // The renderer uses control bounds. The quarter-circle control lies k*r/sqrt(2)
    // above its tangent point, where k = 4/3 tan(pi/8), r = 2.
    let k = 4. / 3. * (std::f64::consts::PI / 8.).tan();
    let original_phase_y = 20. + 2_f64.sqrt() * (1. - k) + 4.;
    assert!((b.1 + f64::from(phase.1) - original_phase_y).abs() < 1e-5);
    assert_eq!(layer.vector.corners[0].corner.radius, 4.);
    assert!(s.journal.is_empty());
}
#[test]
fn transform34_engine_reflect_twice_30_degrees_identity_and_shear_area() {
    let (s, ids) = fixture(1);
    let before = shape(&s.active().unwrap().doc, ids[0]).path.clone();
    let matrix = reflect(Point::new(14., 17.), 30.).unwrap();
    let a = Affine { m: matrix };
    for (old, new) in before.subpaths[0].knots.iter().zip(&before.transform(&a).transform(&a).subpaths[0].knots) {
        assert!((old.anchor.x - new.anchor.x).hypot(old.anchor.y - new.anchor.y) < 1e-9);
    }
    let c = request(&s, &ids, shear(Point::new(20., 20.), 30., false).unwrap());
    let p = prepare(&s, &c).unwrap();
    let area = photocraft_pathops::area(&shape(&p.document, ids[0]).path).unwrap();
    assert!((area - 400.).abs() < 1e-9);
}
#[test]
fn transform34_engine_stroke_profile_effect_and_corner_options() {
    let (mut s, ids) = fixture(1);
    let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let layer = doc.layer_mut(ids[0]).unwrap();
    layer.vector.appearance = Some(Appearance {
        items: vec![AppearanceItem::Stroke(StrokeItem {
            base: ShapeStroke { width: 4., ..Default::default() },
            profile: Some(WidthProfile { points: vec![WidthPoint { t: 0., left: 2., right: 3. }, WidthPoint { t: 1., left: 2., right: 3. }] }),
            ..Default::default()
        })],
    });
    layer.vector.corners = vec![KnotCorner { subpath: 0, knot: 0, corner: Corner { radius: 2., kind: CornerKind::Round } }];
    layer.effects.items.push(photocraft_doc::Effect::default_drop_shadow());
    let mut c = request(&s, &ids, scale(Point::new(20., 20.), 2., 2.).unwrap());
    let p = prepare(&s, &c).unwrap();
    assert_eq!(shape(&p.document, ids[0]).stroke.as_ref().unwrap().width, 4.);
    assert_eq!(p.document.layer(ids[0]).unwrap().vector.corners[0].corner.radius, 4.);
    c.options.scale_strokes_effects = true;
    c.options.scale_corners = false;
    let p = prepare(&s, &c).unwrap();
    let layer = p.document.layer(ids[0]).unwrap();
    assert_eq!(shape(&p.document, ids[0]).stroke.as_ref().unwrap().width, 8.);
    assert_eq!(layer.vector.corners[0].corner.radius, 2.);
    let AppearanceItem::Stroke(stroke) = &layer.vector.appearance.as_ref().unwrap().items[0] else { panic!() };
    assert_eq!(stroke.profile.as_ref().unwrap().points[0].left, 4.);
    let photocraft_doc::Effect::DropShadow(shadow) = &layer.effects.items[0] else { panic!() };
    assert_eq!(shadow.distance, 10.);
}
#[test]
fn transform34_engine_again_copy_three_repeats_exact_offsets() {
    let (mut s, ids) = fixture(1);
    let mut c = request(&s, &ids, [1., 0., 0., 1., 10., 5.]);
    c.copy = true;
    let mut selected = commit(&mut s, &c).unwrap();
    for _ in 0..2 {
        let c = again_request(&s, &selected).unwrap();
        assert!(c.copy);
        selected = commit(&mut s, &c).unwrap();
    }
    let doc = &s.active().unwrap().doc;
    assert_eq!(doc.layers.len(), 4);
    for (i, l) in doc.layers.iter().enumerate() {
        let p = shape(doc, l.id).path.subpaths[0].knots[0].anchor;
        assert_eq!(p, Point::new(20. + 10. * i as f64, 20. + 5. * i as f64));
    }
    assert_eq!(s.journal.len(), 3);
}
#[test]
fn transform34_engine_each_seed_deterministic_local_reference_and_copy() {
    let (s, ids) = fixture(2);
    let mut c = request(&s, &ids, Affine::IDENTITY.m);
    c.operation = Operation::Each(Each { scale: [2., 1.5], movement: [30., 20.], rotation: 30., random: true, seed: 123, ..Default::default() });
    let a = prepare(&s, &c).unwrap();
    let b = prepare(&s, &c).unwrap();
    assert_eq!(a.document, b.document);
    let Operation::Each(e) = &mut c.operation else { panic!() };
    e.seed = 456;
    let other = prepare(&s, &c).unwrap();
    assert_ne!(a.document, other.document);
    let Operation::Each(e) = &mut c.operation else { panic!() };
    e.random = false;
    e.scale = [2., 2.];
    e.movement = [0., 0.];
    e.rotation = 90.;
    c.copy = true;
    let p = prepare(&s, &c).unwrap();
    assert_eq!(p.document.layer_count(), 4);
    for (old, new) in ids.iter().zip(&p.targets) {
        let a = shape(&p.document, *old).path.control_bounds().unwrap();
        let b = shape(&p.document, *new).path.control_bounds().unwrap();
        assert!(((a.0 + a.2 - b.0 - b.2) / 2.).abs() < 1e-9);
        assert!(((a.1 + a.3 - b.1 - b.3) / 2.).abs() < 1e-9);
    }
}
#[test]
fn transform34_engine_reshape_selected_and_falloff_independent() {
    let (mut s, ids) = fixture(1);
    let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let LayerContent::Shape(sh) = &mut doc.layer_mut(ids[0]).unwrap().content else { panic!() };
    sh.path = Path::new(vec![Subpath::polyline(&[(0., 10.), (10., 10.), (20., 10.), (40., 10.)])]);
    let mut c = request(&s, &ids, Affine::IDENTITY.m);
    c.operation = Operation::Reshape { nodes: vec![Node { layer: ids[0], subpath: 0, knot: 0 }], delta: [0., 16.], radius: 20. };
    let p = prepare(&s, &c).unwrap();
    let nodes = &shape(&p.document, ids[0]).path.subpaths[0].knots;
    assert_eq!(nodes[0].anchor.y, 26.);
    assert_eq!(nodes[1].anchor.y, 19.);
    assert_eq!(nodes[2].anchor.y, 10.);
    assert_eq!(nodes[3].anchor.y, 10.);
    assert_eq!(nodes[1].anchor, nodes[1].in_ctrl);
}
#[test]
fn transform34_engine_live_similarity_preserved_nonsimilarity_baked() {
    let (mut s, ids) = fixture(1);
    let d = DesignShape { rect: [20., 20., 20., 20.], angle: 0., parameters: ShapeParameters::Rect { radii: [2.; 4], kinds: [CornerKind::Round; 4] } };
    let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let l = doc.layer_mut(ids[0]).unwrap();
    l.vector.live_shape = Some(d.clone());
    let LayerContent::Shape(sh) = &mut l.content else { panic!() };
    sh.path = super::super::drawing::geometry::generate(&d).unwrap();
    let mut c = request(&s, &ids, rotate(Point::new(30., 30.), 90.).unwrap());
    let p = prepare(&s, &c).unwrap();
    let d = p.document.layer(ids[0]).unwrap().vector.live_shape.as_ref().unwrap();
    let generated = super::super::drawing::geometry::generate(d).unwrap();
    for (a, b) in generated.subpaths[0].knots.iter().zip(&shape(&p.document, ids[0]).path.subpaths[0].knots) {
        assert!((a.anchor.x - b.anchor.x).hypot(a.anchor.y - b.anchor.y) < 1e-9);
    }
    c.operation = Operation::Affine { matrix: shear(Point::new(30., 30.), 20., false).unwrap() };
    let p = prepare(&s, &c).unwrap();
    assert!(p.document.layer(ids[0]).unwrap().vector.live_shape.is_none());
}
#[test]
fn transform34_engine_live_unrepresentable_reflection_bakes_without_geometry_loss() {
    for parameters in [
        ShapeParameters::Cog { teeth: 7, inner: 0.7, hole: 0.2, curve: 0.1 },
        ShapeParameters::Spiral { center: [30., 30.], radius: 8., decay: 0.8, turns: 2. },
    ] {
        let (mut s, ids) = fixture(1);
        let d = DesignShape { rect: [20., 20., 20., 30.], angle: 0.2, parameters };
        let source = super::super::drawing::geometry::generate(&d).unwrap();
        let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
        let l = doc.layer_mut(ids[0]).unwrap();
        l.vector.live_shape = Some(d);
        let LayerContent::Shape(sh) = &mut l.content else { panic!() };
        sh.path = source.clone();
        let c = request(&s, &ids, reflect(Point::new(30., 30.), 30.).unwrap());
        let p = prepare(&s, &c).unwrap();
        assert!(p.document.layer(ids[0]).unwrap().vector.live_shape.is_none());
        let mirrored = &shape(&p.document, ids[0]).path;
        for (before, after) in source.subpaths.iter().flat_map(|s| &s.knots).zip(mirrored.subpaths.iter().flat_map(|s| &s.knots)) {
            for (p, q) in [before.anchor, before.in_ctrl, before.out_ctrl].into_iter().zip([after.anchor, after.in_ctrl, after.out_ctrl]) {
                let x = p.x - 30.;
                let y = p.y - 30.;
                let expected = Point::new(30. + 0.5 * x - 3_f64.sqrt() * y / 2., 30. - 3_f64.sqrt() * x / 2. - 0.5 * y);
                assert!((expected.x - q.x).hypot(expected.y - q.y) < 1e-9);
            }
        }
    }
}
#[test]
fn transform34_engine_entire_target_set_stale_locked_invalid_no_publication() {
    let (mut s, ids) = fixture(2);
    let c = request(&s, &ids, [1., 0., 0., 1., 10., 0.]);
    let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
    doc.layer_mut(ids[1]).unwrap().locks.position = true;
    let before = s.active().unwrap().doc.clone();
    assert!(commit(&mut s, &c).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert!(!s.undo());
    std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(ids[1]).unwrap().locks.position = false;
    let mut bad = c.clone();
    bad.operation = Operation::Affine { matrix: [f64::NAN; 6] };
    assert!(prepare(&s, &bad).is_err());
    commit(&mut s, &c).unwrap();
    assert!(commit(&mut s, &c).is_err());
    assert_eq!(s.journal.len(), 1);
    let mut other = Session::new();
    other.add_document(Document::new("Other", Size::new(256, 256), ColorMode::Rgb, SampleType::U8), None);
    assert!(prepare(&other, &c).is_err());
}
#[test]
fn transform34_engine_group_all_descendants_atomic_and_copy_ids_unique() {
    let (mut s, ids) = fixture(2);
    let doc = std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let children = std::mem::take(&mut doc.layers);
    let group = Layer::group("Squares", children);
    let root = group.id;
    doc.layers.push(group);
    assert!(Capture::begin(&s, &[root, ids[0]]).is_err());
    let mut c = request(&s, &[root], [1., 0., 0., 1., 10., 0.]);
    c.copy = true;
    let p = prepare(&s, &c).unwrap();
    let LayerContent::Group(g) = &p.document.layer(p.targets[0]).unwrap().content else { panic!() };
    assert_eq!(g.children.len(), 2);
    let all: Vec<_> = p.document.walk().iter().map(|(_, _, l)| l.id).collect();
    assert_eq!(all.len(), all.iter().collect::<HashSet<_>>().len());
    assert_eq!(p.document.layer_count(), 6);
    commit(&mut s, &c).unwrap();
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc.layer_count(), 3);
}
#[test]
fn transform34_engine_distort_trapezoid_preview_commit_matches() {
    let (mut s, ids) = fixture(1);
    let mut c = request(&s, &ids, Affine::IDENTITY.m);
    c.operation =
        Operation::Distort { source: [20., 20., 20., 20.], quad: [[24., 20.], [36., 20.], [40., 40.], [20., 40.]], perspective: false, tolerance: 0.25 };
    let p = prepare(&s, &c).unwrap();
    let sh = shape(&p.document, ids[0]);
    assert_eq!(sh.path.subpaths[0].knots[0].anchor, Point::new(24., 20.));
    assert_eq!(sh.path.subpaths[0].knots[1].anchor, Point::new(36., 20.));
    commit(&mut s, &c).unwrap();
    assert_eq!(*s.active().unwrap().doc, p.document);
}
#[test]
fn transform34_engine_old_options_numeric_validation_move_oracle() {
    assert_eq!(serde_json::from_str::<Options>("{}").unwrap(), Options::default());
    assert_eq!(serde_json::from_str::<Each>("{}").unwrap(), Each::default());
    let m = move_by(10., 90.).unwrap();
    assert!(m[4].abs() < 1e-9);
    assert!((m[5] + 10.).abs() < 1e-9);
    assert!(shear(Point::new(0., 0.), 90., false).is_err());
    assert!(scale(Point::new(0., 0.), 0., 1.).is_err());
    assert!(rotate(Point::new(0., 0.), f64::INFINITY).is_err());
}
