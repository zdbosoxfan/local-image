#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{ColorMode, SampleType, ShapeLayer, ShapeStroke, Size, Subpath};
fn shape(name: &str, x: f64, color: Color) -> Layer {
    Layer::new(
        name,
        LayerContent::Shape(ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, 10.), (x + 10., 10.), (x + 10., 20.), (x, 20.)])]),
            fill: Some(Fill::Solid(color)),
            ..Default::default()
        }),
    )
}
fn session(layers: Vec<Layer>) -> Session {
    let mut doc = Document::new("Selection fixtures", Size::new(256, 64), ColorMode::Rgb, SampleType::U8);
    doc.layers = layers;
    let mut s = Session::new();
    s.add_document(doc, None);
    s
}
fn request(s: &Session, query: Query) -> Request {
    let st = s.active().unwrap();
    Request { document: st.doc.id, revision: st.revision, scope: Scope { active: st.active_layer, ..Default::default() }, mode: Mode::Replace, query }
}
fn select(s: &mut Session, q: Query) -> Preview {
    let r = request(s, q);
    run(s, &r).unwrap()
}
fn same_query(criterion: Same, reference: LayerId) -> Query {
    Query::Same { criterion, reference, compare: Compare::Equal, value: None, item: None }
}
fn target(s: &mut Session, id: LayerId) {
    let st = s.active_mut().unwrap();
    st.active_layer = Some(id);
    st.selected_layers = vec![id];
}
#[test]
fn selection_same_fill_six_shapes_scope_locked_and_nested_oracle() {
    let red = Color::rgb(1., 0., 0.);
    let a = shape("red A", 0., red);
    let b = shape("red B", 20., red);
    let c = shape("red C", 40., red);
    let blue = shape("blue", 60., Color::rgb(0., 0., 1.));
    let nested = shape("nested red", 80., red);
    let nested_id = nested.id;
    let mut locked = shape("locked red", 100., red);
    locked.locks.all = true;
    let expected = vec![a.id, b.id, c.id];
    let reference = a.id;
    let mut s = session(vec![a, b, c, blue, Layer::group("outer", vec![Layer::group("inner", vec![nested])]), locked]);
    target(&mut s, reference);
    let mut r = request(&s, same_query(Same::FillColor, reference));
    r.scope.edit_all_layers = false;
    let p = run(&mut s, &r).unwrap();
    assert_eq!(p.layers, expected);
    r.revision = s.active().unwrap().revision;
    r.scope.edit_all_layers = true;
    let p = run(&mut s, &r).unwrap();
    assert_eq!(p.layers, [expected, vec![nested_id]].concat());
}
#[test]
fn selection_stroke_weight_all_five_comparisons() {
    let mut layers = Vec::new();
    for width in [1., 2., 3.] {
        let mut l = shape("stroke", width * 20., Color::BLACK);
        let LayerContent::Shape(sh) = &mut l.content else { panic!() };
        sh.stroke = Some(ShapeStroke { width: width as f32, ..Default::default() });
        layers.push(l);
    }
    let ids: Vec<_> = layers.iter().map(|l| l.id).collect();
    let mut s = session(layers);
    target(&mut s, ids[1]);
    for (compare, expected) in [
        (Compare::Less, vec![ids[0]]),
        (Compare::LessEqual, vec![ids[0], ids[1]]),
        (Compare::Equal, vec![ids[1]]),
        (Compare::GreaterEqual, vec![ids[1], ids[2]]),
        (Compare::Greater, vec![ids[2]]),
    ] {
        assert_eq!(select(&mut s, Query::Same { criterion: Same::StrokeWeight, reference: ids[1], compare, value: None, item: None }).layers, expected);
    }
}
#[test]
fn selection_appearance_whole_stack_order_and_attribute() {
    let red = AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(1., 0., 0.))));
    let blue = AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(0., 0., 1.))));
    let mut a = shape("same", 0., Color::BLACK);
    a.vector.appearance = Some(Appearance { items: vec![red.clone(), blue.clone()] });
    let mut b = shape("same", 20., Color::BLACK);
    b.vector.appearance = a.vector.appearance.clone();
    let mut c = shape("same", 40., Color::BLACK);
    c.vector.appearance = Some(Appearance { items: vec![blue.clone(), red.clone()] });
    let mut d = shape("same", 60., Color::BLACK);
    d.vector.appearance = Some(Appearance { items: vec![red] });
    let ids = [a.id, b.id, c.id, d.id];
    let mut s = session(vec![a, b, c, d]);
    target(&mut s, ids[0]);
    assert_eq!(select(&mut s, same_query(Same::Appearance, ids[0])).layers, ids[..2]);
    assert_eq!(
        select(&mut s, Query::Same { criterion: Same::AppearanceAttribute, reference: ids[0], compare: Compare::Equal, value: None, item: Some(1) }).layers,
        ids[..3]
    );
    let r = request(&s, Query::Same { criterion: Same::AppearanceAttribute, reference: ids[0], compare: Compare::Equal, value: None, item: Some(9) });
    assert!(run(&mut s, &r).is_err());
}
#[test]
fn selection_open_and_stray_seeded_mixed_components() {
    let mut open = shape("open", 0., Color::BLACK);
    let mut stray = shape("stray", 20., Color::BLACK);
    let closed = shape("closed", 40., Color::BLACK);
    let empty = Layer::new("empty", LayerContent::Shape(ShapeLayer::default()));
    let LayerContent::Shape(sh) = &mut open.content else { panic!() };
    sh.path.subpaths[0].closed = false;
    let LayerContent::Shape(sh) = &mut stray.content else { panic!() };
    sh.path = Path::new(vec![Subpath::polyline(&[(30., 10.)])]);
    let ids = [open.id, stray.id];
    let mut s = session(vec![open, stray, closed, empty]);
    assert_eq!(select(&mut s, Query::Object { criterion: Object::OpenPaths }).layers, [ids[0]]);
    assert_eq!(select(&mut s, Query::Object { criterion: Object::StrayPoints }).layers, [ids[1]]);
}
#[test]
fn selection_wand_oklab_independent_boundary_and_modes() {
    // Equal-channel sRGB: Oklab L = cbrt(decoded sRGB), so .49 and .51 lie
    // independently on opposite sides of 100*cbrt(sRGB_decode(.5)) = 59.81807266.
    let black = shape("black", 0., Color::BLACK);
    let below = shape("below", 20., Color::gray(0.49));
    let above = shape("above", 40., Color::gray(0.51));
    let ids = [black.id, below.id, above.id];
    let mut s = session(vec![black, below, above]);
    target(&mut s, ids[0]);
    let mut r = request(&s, Query::Wand { reference: ids[0], settings: Wand { fill_tolerance: 59.81807266, ..Default::default() } });
    assert_eq!(run(&mut s, &r).unwrap().layers, ids[..2]);
    r.mode = Mode::Add;
    r.query = Query::Wand { reference: ids[2], settings: Wand { fill_tolerance: 0., ..Default::default() } };
    assert_eq!(run(&mut s, &r).unwrap().layers, ids);
    r.mode = Mode::Subtract;
    assert_eq!(run(&mut s, &r).unwrap().layers, ids[..2]);
    assert!((delta_e_ok(Color::BLACK, Color::WHITE).unwrap() - 100.).abs() < 1e-5);
}
#[test]
fn selection_wand_enabled_attributes_intersect_not_union() {
    let a = shape("a", 0., Color::BLACK);
    let mut b = shape("b", 20., Color::BLACK);
    b.opacity = 0.5;
    let mut c = shape("c", 40., Color::BLACK);
    c.blend = photocraft_doc::BlendMode::Multiply;
    let id = a.id;
    let mut s = session(vec![a, b, c]);
    target(&mut s, id);
    assert_eq!(
        select(
            &mut s,
            Query::Wand { reference: id, settings: Wand { fill_tolerance: 0., opacity: true, opacity_tolerance: 0., blend_mode: true, ..Default::default() } }
        )
        .layers,
        [id]
    );
}
#[test]
fn selection_group_click_chain_stops_at_isolation_and_outermost() {
    let circle = shape("circle", 0., Color::BLACK);
    let leaf = circle.id;
    let inner = Layer::group("inner", vec![circle]);
    let group = inner.id;
    let outer = Layer::group("outer", vec![inner]);
    let top = outer.id;
    let mut s = session(vec![outer]);
    let mut last = None;
    for expected in [leaf, group, top, top] {
        let p = select(&mut s, Query::Group { leaf, previous: last });
        assert_eq!(p.layers, [expected]);
        last = Some(expected);
    }
    target(&mut s, leaf);
    let mut r = request(&s, Query::Group { leaf, previous: Some(group) });
    r.scope.isolation = Some(top);
    assert_eq!(run(&mut s, &r).unwrap().layers, [group]);
}
#[test]
fn selection_hidden_and_locked_ancestors_cannot_leak() {
    let leaf = shape("hidden child", 0., Color::BLACK);
    let id = leaf.id;
    let mut hidden = Layer::group("hidden", vec![leaf]);
    hidden.visible = false;
    let leaf = shape("locked child", 20., Color::BLACK);
    let mut locked = Layer::group("locked", vec![leaf]);
    locked.locks.all = true;
    let mut s = session(vec![hidden, locked]);
    assert!(select(&mut s, Query::All).layers.is_empty());
    let mut r = request(&s, Query::All);
    r.scope.select_hidden = true;
    assert_eq!(run(&mut s, &r).unwrap().layers, [id]);
}
#[test]
fn selection_lasso_three_of_five_and_touch_without_centroid() {
    let layers: Vec<_> = (0..5).map(|i| shape("shape", i as f64 * 20., Color::BLACK)).collect();
    let ids: Vec<_> = layers.iter().map(|l| l.id).collect();
    let mut s = session(layers);
    let p = select(&mut s, Query::Lasso { points: vec![[-1., 9.], [51., 9.], [51., 21.], [-1., 21.]], nodes: false });
    assert_eq!(p.layers, ids[..3]);
    let p = select(&mut s, Query::Lasso { points: vec![[-2., 13.], [1., 13.], [1., 17.], [-2., 17.]], nodes: false });
    assert_eq!(p.layers, [ids[0]], "loop crosses closing left edge without enclosing any object knot or centre");
}
#[test]
fn selection_lasso_compound_hole_is_not_bounds_hit() {
    let mut donut = shape("donut", 0., Color::BLACK);
    let LayerContent::Shape(sh) = &mut donut.content else { panic!() };
    sh.path = Path::new(vec![
        Subpath::polygon(&[(0., 0.), (100., 0.), (100., 100.), (0., 100.)]),
        Subpath::polygon(&[(20., 20.), (80., 20.), (80., 80.), (20., 80.)]).with_op(photocraft_doc::PathOp::Subtract),
    ]);
    let mut s = session(vec![donut]);
    assert!(select(&mut s, Query::Lasso { points: vec![[40., 40.], [60., 40.], [60., 60.], [40., 60.]], nodes: false }).layers.is_empty());
}
#[test]
fn selection_node_lasso_across_selected_curves_and_boundary() {
    let a = shape("a", 0., Color::BLACK);
    let b = shape("b", 20., Color::BLACK);
    let c = shape("unselected", 40., Color::BLACK);
    let ids = [a.id, b.id];
    let mut s = session(vec![a, b, c]);
    let st = s.active_mut().unwrap();
    st.active_layer = Some(ids[1]);
    st.selected_layers = ids.to_vec();
    let p = select(&mut s, Query::Lasso { points: vec![[-1., 10.], [31., 10.], [31., 15.], [-1., 15.]], nodes: true });
    assert_eq!(p.layers, ids);
    assert_eq!(p.nodes.unwrap(), ids.into_iter().map(|layer| Nodes { layer, anchors: vec![[0, 0], [0, 1]] }).collect::<Vec<_>>());
}
#[test]
fn selection_inverse_next_reselect_and_same_layers() {
    let a = shape("a", 0., Color::BLACK);
    let b = shape("b", 20., Color::BLACK);
    let c = shape("c", 40., Color::BLACK);
    let ids = [a.id, b.id, c.id];
    let mut s = session(vec![a, b, c]);
    target(&mut s, ids[1]);
    assert_eq!(select(&mut s, Query::Next { above: true }).layers, [ids[2]]);
    assert_eq!(select(&mut s, Query::Next { above: true }).layers, [ids[2]]);
    assert_eq!(select(&mut s, Query::Next { above: false }).layers, [ids[1]]);
    assert_eq!(select(&mut s, Query::Inverse).layers, [ids[0], ids[2]]);
    assert_eq!(select(&mut s, Query::Deselect).layers, []);
    assert_eq!(select(&mut s, Query::Reselect { ids: vec![ids[1]] }).layers, [ids[1]]);
    assert_eq!(select(&mut s, Query::Object { criterion: Object::AllOnSameLayers }).layers, ids);
}
#[test]
fn selection_dimensions_name_and_live_shape_kind() {
    use photocraft_doc::vector_design::drawing::{DesignShape, ShapeParameters};
    let mut a = shape("same", 0., Color::BLACK);
    let mut b = shape("same", 20., Color::BLACK);
    let mut c = shape("different", 40., Color::BLACK);
    for l in [&mut a, &mut b, &mut c] {
        l.vector.live_shape = Some(DesignShape { rect: [0., 0., 10., 10.], angle: 0.25, parameters: ShapeParameters::Ellipse });
    }
    c.vector.live_shape.as_mut().unwrap().parameters = ShapeParameters::Heart { depth: 0.5 };
    let ids = [a.id, b.id, c.id];
    let mut s = session(vec![a, b, c]);
    target(&mut s, ids[0]);
    for criterion in [Same::Width, Same::Height, Same::Rotation] {
        assert_eq!(select(&mut s, same_query(criterion, ids[0])).layers, ids);
    }
    for criterion in [Same::Name, Same::ShapeType] {
        assert_eq!(select(&mut s, same_query(criterion, ids[0])).layers, ids[..2]);
    }
}
#[test]
fn selection_graphic_style_requires_reader_and_link() {
    let a = shape("style:one", 0., Color::BLACK);
    let b = shape("style:one", 20., Color::BLACK);
    let c = shape("style:two", 40., Color::BLACK);
    let ids = [a.id, b.id, c.id];
    let s = session(vec![a, b, c]);
    let r = request(&s, same_query(Same::GraphicStyle, ids[0]));
    let st = s.active().unwrap();
    assert!(preview(&st.doc, st.revision, &st.selected_layers(), &r, Readers::default()).is_err());
    let readers = Readers { graphic_style: Some(|l| Ok(l.name.strip_prefix("style:").map(str::to_owned))), ..Default::default() };
    assert_eq!(preview(&st.doc, st.revision, &st.selected_layers(), &r, readers).unwrap().layers, ids[..2]);
}
#[test]
fn selection_stale_invalid_and_numeric_inputs_are_atomic() {
    let a = shape("a", 0., Color::BLACK);
    let id = a.id;
    let mut s = session(vec![a]);
    target(&mut s, id);
    let r = request(&s, Query::Deselect);
    let st = s.active().unwrap();
    let p = preview(&st.doc, st.revision, &st.selected_layers(), &r, Readers::default()).unwrap();
    let mut bad = p.clone();
    bad.layers = vec![LayerId(u64::MAX)];
    assert!(apply(&mut s, &bad).is_err());
    assert_eq!(s.active().unwrap().selected_layers(), [id]);
    let mut stale = r.clone();
    stale.revision += 1;
    assert!(run(&mut s, &stale).is_err());
    s.active_mut().unwrap().selected_layers.clear();
    s.active_mut().unwrap().active_layer = None;
    assert!(apply(&mut s, &p).is_err());
    target(&mut s, id);
    for value in [f64::NAN, f64::INFINITY, -1., 201.] {
        let r = request(&s, Query::Wand { reference: id, settings: Wand { fill_tolerance: value, ..Default::default() } });
        assert!(run(&mut s, &r).is_err());
    }
    let r = request(&s, Query::Lasso { points: vec![[0., 0.], [f64::INFINITY, 0.], [0., 10.]], nodes: false });
    assert!(run(&mut s, &r).is_err());
    s.add_document(Document::new("other", Size::new(20, 20), ColorMode::Rgb, SampleType::U8), None);
    assert!(apply(&mut s, &p).is_err());
}
#[test]
fn selection_commands_preserve_revision_journal_history_and_document_arc() {
    let a = shape("a", 0., Color::BLACK);
    let b = shape("b", 20., Color::BLACK);
    let id = a.id;
    let mut s = session(vec![a, b]);
    target(&mut s, id);
    let st = s.active().unwrap();
    let doc = st.doc.clone();
    let revision = st.revision;
    let saved = st.saved_revision;
    let history = st.history.past_len();
    let journal = s.journal.clone();
    for q in [same_query(Same::FillColor, id), Query::Inverse, Query::All, Query::Deselect, Query::Reselect { ids: vec![id] }] {
        let r = request(&s, q);
        s.execute("vector.select", json!(r)).unwrap();
    }
    let selected = s.active().unwrap().selected_layers();
    let queried: Preview = serde_json::from_value(s.execute("vector.select.query", json!(request(&s, Query::All))).unwrap()).unwrap();
    assert_eq!(queried.layers.len(), 2);
    assert_eq!(s.active().unwrap().selected_layers(), selected);
    let st = s.active().unwrap();
    assert!(std::sync::Arc::ptr_eq(&st.doc, &doc));
    assert_eq!(st.revision, revision);
    assert_eq!(st.saved_revision, saved);
    assert_eq!(st.history.past_len(), history);
    assert_eq!(s.journal, journal);
    assert!(!s.undo());
    assert!(specs().iter().all(|s| !s.journal));
}
#[test]
fn selection_old_options_load_with_defaults_and_strict_requests() {
    assert_eq!(serde_json::from_str::<Wand>("{}").unwrap(), Wand::default());
    assert!(serde_json::from_value::<Wand>(json!({"unknown":1})).is_err());
    let a = shape("a", 0., Color::BLACK);
    let s = session(vec![a]);
    let r = request(&s, Query::All);
    assert_eq!(serde_json::from_value::<Request>(json!(r)).unwrap(), r);
}
#[test]
fn selection_edit_all_layers_on_artboards_descends_current_board_only() {
    let a = shape("a", 0., Color::BLACK);
    let id = a.id;
    let b = shape("nested", 20., Color::BLACK);
    let nested = b.id;
    let c = shape("other board", 40., Color::BLACK);
    let other = c.id;
    let group = Layer::group("nested group", vec![b]);
    let group_id = group.id;
    let mut first = Layer::group("first", vec![a, group]);
    let LayerContent::Group(g) = &mut first.content else { panic!() };
    g.artboard = Some(photocraft_doc::Artboard::new(photocraft_geom::Rect::new(0, 0, 40, 40)));
    let mut second = Layer::group("second", vec![c]);
    let LayerContent::Group(g) = &mut second.content else { panic!() };
    g.artboard = Some(photocraft_doc::Artboard::new(photocraft_geom::Rect::new(40, 0, 80, 40)));
    let mut s = session(vec![first, second]);
    target(&mut s, id);
    let mut r = request(&s, same_query(Same::FillColor, id));
    r.scope.edit_all_layers = false;
    assert_eq!(run(&mut s, &r).unwrap().layers, [id, nested]);
    assert_eq!(hit(&s.active().unwrap().doc, &r.scope, [45., 15.], 1., Readers::default()).unwrap(), None);
    r.query = Query::Group { leaf: nested, previous: Some(group_id) };
    assert_eq!(run(&mut s, &r).unwrap().layers, [group_id], "group ascent must stay inside the active artboard");
    r.scope.edit_all_layers = true;
    r.query = same_query(Same::FillColor, id);
    assert_eq!(run(&mut s, &r).unwrap().layers, [id, nested, other]);
}
#[test]
fn selection_256_seeded_enclosure_translation_and_add_subtract_properties() {
    // Independent rectangle inequalities: no shared polygon routine generates expected
    // membership. Deterministic cases cover negative coordinates and reversed winding.
    for seed in 0..256 {
        let x = (seed % 17) as f64 - 8.;
        let y = (seed % 23) as f64 - 11.;
        let w = (seed % 7 + 1) as f64;
        let h = (seed % 11 + 1) as f64;
        let mut poly = vec![Point::new(x, y), Point::new(x + w, y), Point::new(x + w, y + h), Point::new(x, y + h)];
        if seed % 2 == 1 {
            poly.reverse();
        }
        for p in [
            Point::new(x + w * 0.37, y + h * 0.71),
            Point::new(x - 0.01, y + h * 0.5),
            Point::new(x + w + 0.01, y + h * 0.5),
            Point::new(x + w * 0.5, y),
            Point::new(x + w * 0.5, y + h),
        ] {
            assert_eq!(enclosed(&poly, p), p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h);
        }
    }
}
#[test]
fn selection_object_paint_groups_clipping_and_explicit_empty_appearance() {
    let a = shape("painted", 0., Color::BLACK);
    let mut b = shape("unfilled stroked", 20., Color::BLACK);
    let LayerContent::Shape(sh) = &mut b.content else { panic!() };
    sh.fill = None;
    sh.stroke = Some(ShapeStroke::default());
    let mut c = shape("empty stack", 40., Color::BLACK);
    c.vector.appearance = Some(Appearance::default());
    c.vector.clip = Some(Path::new(vec![Subpath::polygon(&[(0., 0.), (1., 0.), (1., 1.)])]));
    let group = Layer::group("group", Vec::new());
    let ids = [a.id, b.id, c.id, group.id];
    let mut s = session(vec![a, b, c, group]);
    for (criterion, expected) in [
        (Object::Unfilled, vec![ids[1], ids[2]]),
        (Object::Stroked, vec![ids[1]]),
        (Object::Unstroked, vec![ids[0], ids[2]]),
        (Object::Groups, vec![ids[3]]),
        (Object::ClippingMasks, vec![ids[2]]),
    ] {
        assert_eq!(select(&mut s, Query::Object { criterion }).layers, expected);
    }
}
