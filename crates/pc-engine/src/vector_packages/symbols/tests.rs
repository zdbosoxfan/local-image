use super::*;
use photocraft_doc::{Color, ColorMode, Fill, SampleType, ShapeLayer, Size};
fn shape(x: f64, y: f64) -> Layer {
    Layer::new(
        "Badge",
        LayerContent::Shape(ShapeLayer {
            path: photocraft_vector::shapes::rect(x, y, 12., 10.),
            fill: Some(Fill::Solid(Color::rgba(0.8, 0.2, 0.1, 0.75))),
            ..Default::default()
        }),
    )
}
pub(super) fn fixture() -> Session {
    let mut s = Session::new();
    let mut doc = Document::new("Symbols", Size::new(128, 96), ColorMode::Rgb, SampleType::U8);
    let mut l = shape(2., 3.);
    if let LayerContent::Shape(sh) = &mut l.content {
        crate::vector_cmds::refresh_shape(&doc, sh);
    }
    let id = l.id;
    doc.layers.push(l);
    s.add_document(doc, None);
    s.select_layer(id).unwrap();
    s
}
fn call(s: &mut Session, id: &str, values: Value) -> Value {
    let mut p = Capture::new(s).unwrap().params();
    for (key, value) in values.as_object().unwrap() {
        p[key] = value.clone();
    }
    s.execute(id, p).unwrap()
}
pub(super) fn make_symbol(s: &mut Session) -> (SymbolId, LayerId) {
    let id = s.active().unwrap().doc.layers[0].id;
    let v = call(s, "symbol.create", json!({"layers":[id.0],"name":"Badge"}));
    (SymbolId(v["symbol"].as_u64().unwrap()), LayerId(v["layer"].as_u64().unwrap()))
}
pub(super) fn three(s: &mut Session) -> (SymbolId, Vec<LayerId>) {
    let (master, first) = make_symbol(s);
    let mut ids = vec![first];
    for x in [30., 60.] {
        let v = call(s, "symbol.place", json!({"symbol":master.0,"transform":Affine::translate(x,20.)}));
        ids.push(LayerId(v["layer"].as_u64().unwrap()));
    }
    (master, ids)
}
#[test]
fn symbols_master_edit_updates_three_pixels_detach_independence_and_undo() {
    let mut s = fixture();
    let before = s.active().unwrap().doc.clone();
    let (id, instances) = three(&mut s);
    assert_eq!(s.journal.len(), 3);
    let edit_before = s.active().unwrap().doc.clone();
    let child = edit_before.symbols[0].layers[0].id;
    call(&mut s, "symbol.editMaster", json!({"symbol":id.0,"edits":[{"layer":child.0,"fill":Fill::Solid(Color::rgb(0.1,0.3,0.9))}]}));
    let edited = s.active().unwrap().doc.clone();
    // Independent oracle: ordinary shapes, without symbol expansion or instance caches.
    let mut expected = Document::new("Oracle", edited.size, edited.mode, edited.depth);
    for (x, y) in [(2., 3.), (32., 23.), (62., 23.)] {
        let mut sh = shape(x, y);
        if let LayerContent::Shape(p) = &mut sh.content {
            p.fill = Some(Fill::Solid(Color::rgb(0.1, 0.3, 0.9)));
            crate::vector_cmds::refresh_shape(&expected, p);
        }
        expected.layers.push(sh);
    }
    assert_eq!(photocraft_compose::flatten(&edited).px, photocraft_compose::flatten(&expected).px);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, edit_before);
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, edited);
    call(&mut s, "symbol.detach", json!({"layers":[instances[1].0]}));
    let detached = s.active().unwrap().doc.layer(instances[1]).unwrap().clone();
    assert!(matches!(detached.content, LayerContent::Group(_)));
    let detached_pixels = photocraft_compose::flatten(&s.active().unwrap().doc).px;
    assert_eq!(detached_pixels, photocraft_compose::flatten(&edited).px);
    call(&mut s, "symbol.editMaster", json!({"symbol":id.0,"edits":[{"layer":child.0,"fill":Fill::Solid(Color::BLACK)}]}));
    assert_eq!(s.active().unwrap().doc.layer(instances[1]).unwrap(), &detached);
    let pixels = photocraft_compose::flatten(&s.active().unwrap().doc);
    assert_eq!(pixels.get(34, 25), expected_sample(0.1, 0.3, 0.9));
    while s.undo() {}
    assert_eq!(s.active().unwrap().doc, before);
}
fn expected_sample(r: f32, g: f32, b: f32) -> [f32; 4] {
    [((r * 255.).round() / 255.), ((g * 255.).round() / 255.), ((b * 255.).round() / 255.), 1.]
}
#[test]
fn symbols_cycles_duplicate_ids_dangling_refs_and_stale_targets_are_atomic() {
    let mut s = fixture();
    let (master, _) = make_symbol(&mut s);
    let doc = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let journal = s.journal.len();
    let layer = Layer::new("Cycle", LayerContent::Symbol(SymbolInstance { symbol: master, transform: Affine::IDENTITY, cache: None }));
    let c = Capture::new(&s).unwrap();
    assert!(update_master(&mut s, c, master, vec![layer]).is_err());
    let duplicate = doc.symbols[0].layers[0].clone();
    assert!(update_master(&mut s, c, master, vec![duplicate.clone(), duplicate]).is_err());
    let duplicate_root = doc.layers[0].clone();
    assert!(update_master(&mut s, c, master, vec![duplicate_root]).is_err());
    let missing = Layer::new("Dangling", LayerContent::Symbol(SymbolInstance { symbol: SymbolId(9999), transform: Affine::IDENTITY, cache: None }));
    assert!(update_master(&mut s, c, master, vec![missing]).is_err());
    let mut p = c.params();
    p["layers"] = json!([doc.layers[0].id.0, 999_999]);
    assert!(s.execute("symbol.detach", p).is_err());
    let mut bad_a = Affine::IDENTITY;
    bad_a.m[0] = f64::NAN;
    assert!(place_preview(&s, c, master, bad_a, None).is_err());
    assert!(place_preview(&s, c, master, Affine::scale(0.), None).is_err());
    assert!(place_preview(&s, c, master, Affine::translate(1e20, 0.), None).is_err());
    let mut invalid_paint = doc.symbols[0].layers.clone();
    if let LayerContent::Shape(sh) = &mut invalid_paint[0].content {
        sh.fill = Some(Fill::Solid(Color::rgb(f32::NAN, 0., 0.)));
    }
    assert!(update_master(&mut s, c, master, invalid_paint).is_err());
    let mut invalid_path = doc.symbols[0].layers.clone();
    if let LayerContent::Shape(sh) = &mut invalid_path[0].content {
        sh.path.subpaths[0].knots[0].anchor.x = f64::INFINITY;
    }
    assert!(update_master(&mut s, c, master, invalid_path).is_err());
    assert_eq!(s.active().unwrap().doc, doc);
    assert_eq!(s.active().unwrap().revision, revision);
    assert_eq!(s.journal.len(), journal);
    call(&mut s, "symbol.rename", json!({"symbol":master.0,"name":"New"}));
    let now = s.active().unwrap().doc.clone();
    assert!(place_preview(&s, c, master, Affine::IDENTITY, None).is_err());
    assert_eq!(s.active().unwrap().doc, now);
}
#[test]
fn symbols_nested_transitive_updates_delete_and_public_export_projection() {
    let mut s = fixture();
    let (master, instance) = make_symbol(&mut s);
    let nested = call(&mut s, "symbol.create", json!({"layers":[instance.0],"name":"Nested"}));
    let nested_id = SymbolId(nested["symbol"].as_u64().unwrap());
    let base = s.active().unwrap().doc.symbols.iter().find(|d| d.id == master).unwrap().layers[0].id;
    call(&mut s, "symbol.editMaster", json!({"symbol":master.0,"edits":[{"layer":base.0,"fill":Fill::Solid(Color::WHITE)}]}));
    let doc = s.active().unwrap().doc.clone();
    assert!(dependents(&doc, master).contains(&nested_id.0));
    let output = prepare_expansion(&doc, expansion::expand(&doc, &doc.layers).unwrap()).unwrap();
    let mut expanded = (*doc).clone();
    expanded.layers = output;
    assert!(expanded.walk().iter().all(|(_, _, l)| !matches!(l.content, LayerContent::Symbol(_))));
    assert_eq!(photocraft_compose::flatten(&expanded).px, photocraft_compose::flatten(&doc).px);
    let mut cycle = doc.symbols.iter().find(|d| d.id == master).unwrap().layers.clone();
    cycle.push(Layer::new("Nested cycle", LayerContent::Symbol(SymbolInstance { symbol: nested_id, transform: Affine::IDENTITY, cache: None })));
    let c = Capture::new(&s).unwrap();
    assert!(update_master(&mut s, c, master, cycle).is_err());
    assert_eq!(s.active().unwrap().doc, doc);
    call(&mut s, "symbol.delete", json!({"symbol":master.0}));
    assert_eq!(photocraft_compose::flatten(&s.active().unwrap().doc).px, photocraft_compose::flatten(&doc).px);
    validation::document(&s.active().unwrap().doc).unwrap();
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, doc);
}
#[test]
fn symbols_locked_batch_and_detached_preview_do_not_publish() {
    let mut s = fixture();
    let (master, ids) = three(&mut s);
    let c = Capture::new(&s).unwrap();
    let before = s.active().unwrap().doc.clone();
    let journal = s.journal.len();
    let (preview, id) = place_preview(&s, c, master, Affine::translate(90., 50.), Some(ids[0])).unwrap();
    assert!(preview.layer(id).is_some());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.journal.len(), journal);
    s.edit("Lock target", |doc, _| {
        doc.layer_mut(ids[2]).unwrap().locks.all = true;
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    let mut p = Capture::new(&s).unwrap().params();
    p["layers"] = json!(ids.iter().map(|i| i.0).collect::<Vec<_>>());
    assert!(s.execute("symbol.detach", p).is_err());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn symbols_validation_bounds_nested_depth_and_exponential_fanout() {
    let mut doc = Document::new("Hostile", Size::new(10, 10), ColorMode::Rgb, SampleType::U8);
    doc.symbols.push(SymbolDef { id: SymbolId(1), name: "Leaf".into(), layers: vec![shape(0., 0.)] });
    for n in 2..22 {
        let children = (0..2)
            .map(|_| Layer::new("Repeat", LayerContent::Symbol(SymbolInstance { symbol: SymbolId(n - 1), transform: Affine::IDENTITY, cache: None })))
            .collect();
        doc.symbols.push(SymbolDef { id: SymbolId(n), name: "Nested".into(), layers: children });
    }
    assert!(validation::document(&doc).is_err());
    doc.symbols.truncate(1);
    let mut layer = shape(0., 0.);
    for _ in 0..=photocraft_doc::MAX_GROUP_DEPTH {
        layer = Layer::group("Group", vec![layer]);
    }
    doc.symbols[0].layers = vec![layer];
    assert!(validation::document(&doc).is_err());
}
#[test]
fn symbols_master_generations_propagate_and_unchanged_templates_keep_keys() {
    let mut s = fixture();
    let (id, instance) = make_symbol(&mut s);
    let nested = call(&mut s, "symbol.create", json!({"layers":[instance.0],"name":"Nested"}));
    let nested = SymbolId(nested["symbol"].as_u64().unwrap());
    let mut tracker = expansion::MasterGenerations::default();
    tracker.sync(&s.active().unwrap().doc).unwrap();
    let first = tracker.generation(id).unwrap();
    let nested_first = tracker.generation(nested).unwrap();
    tracker.sync(&s.active().unwrap().doc).unwrap();
    assert_eq!(tracker.generation(id), Some(first));
    let child = s.active().unwrap().doc.symbols[0].layers[0].id;
    call(&mut s, "symbol.editMaster", json!({"symbol":id.0,"edits":[{"layer":child.0,"fill":Fill::Solid(Color::WHITE)}]}));
    tracker.sync(&s.active().unwrap().doc).unwrap();
    assert!(tracker.generation(id).unwrap() > first);
    assert!(tracker.generation(nested).unwrap() > nested_first);
    assert!(s.undo());
    tracker.sync(&s.active().unwrap().doc).unwrap();
    assert!(tracker.generation(id).unwrap() > first + 1);
}
#[test]
fn symbols_256_bounded_placements_match_independent_rectangle_oracle() {
    let mut s = fixture();
    let (master, _) = make_symbol(&mut s);
    let c = Capture::new(&s).unwrap();
    for case in 0..256 {
        let x = f64::from(case % 16);
        let y = f64::from(case / 16);
        let mut one = (*s.active().unwrap().doc).clone();
        one.layers.clear();
        let mut instance = SymbolInstance { symbol: master, transform: Affine::translate(x, y), cache: None };
        instance.cache = Some(render_instance(&one, &instance).unwrap());
        one.layers.push(Layer::new("Instance", LayerContent::Symbol(instance)));
        let mut oracle = one.clone();
        let mut layer = shape(2. + x, 3. + y);
        if let LayerContent::Shape(sh) = &mut layer.content {
            crate::vector_cmds::refresh_shape(&oracle, sh);
        };
        oracle.layers = vec![layer];
        assert_eq!(photocraft_compose::flatten(&one).px, photocraft_compose::flatten(&oracle).px, "case {case}");
        c.check(&s).unwrap();
    }
}

#[test]
fn symbols_cross_document_copy_remaps_colliding_master_ids_and_undo() {
    let mut s = fixture();
    let (_, source_instance) = make_symbol(&mut s);
    let source = s.active().unwrap().doc.clone();
    let mut destination = (*source).clone();
    destination.id = DocId::fresh();
    destination.name = "Destination".into();
    destination.layers = destination.layers.iter().map(Layer::duplicate).collect();
    for def in &mut destination.symbols {
        def.layers = def.layers.iter().map(Layer::duplicate).collect();
    }
    let active = destination.layers[0].id;
    s.add_document(destination, None);
    s.select_layer(active).unwrap();
    let before = s.active().unwrap().doc.clone();
    s.set_active(0);
    let journal = s.journal.len();
    assert!(s.execute("layer.copyToDocument", json!({"document":1,"layers":[source_instance.0],"offset":[1e7,0]})).is_err());
    assert!(std::sync::Arc::ptr_eq(&s.documents()[0].doc, &source));
    assert!(std::sync::Arc::ptr_eq(&s.documents()[1].doc, &before));
    assert_eq!(s.journal.len(), journal);
    s.execute("layer.copyToDocument", json!({"document":1,"layers":[source_instance.0]})).unwrap();
    let copied = &s.active().unwrap().doc;
    assert_eq!(copied.symbols.len(), 2);
    validation::document(copied).unwrap();
    let LayerContent::Symbol(instance) = &copied.layers.last().unwrap().content else { panic!() };
    assert_ne!(instance.symbol, source.symbols[0].id);
    assert!(copied.symbols.iter().any(|d| d.id == instance.symbol));
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.documents()[0].doc, source);
}

#[test]
fn symbols_scale_and_quarter_turn_match_independent_rectangles() {
    let mut s = fixture();
    let (master, _) = make_symbol(&mut s);
    for (transform, rect) in [(Affine::scale(2.), [4., 6., 24., 20.]), (Affine { m: [0., 1., -1., 0., 30., 10.] }, [17., 12., 10., 12.])] {
        let mut actual = (*s.active().unwrap().doc).clone();
        actual.layers.clear();
        let mut instance = SymbolInstance { symbol: master, transform, cache: None };
        instance.cache = Some(render_instance(&actual, &instance).unwrap());
        actual.layers.push(Layer::new("Transformed", LayerContent::Symbol(instance)));
        let mut oracle = actual.clone();
        let mut ordinary = shape(0., 0.);
        if let LayerContent::Shape(sh) = &mut ordinary.content {
            sh.path = photocraft_vector::shapes::rect(rect[0], rect[1], rect[2], rect[3]);
            crate::vector_cmds::refresh_shape(&oracle, sh);
        }
        oracle.layers = vec![ordinary];
        assert_eq!(photocraft_compose::flatten(&actual).px, photocraft_compose::flatten(&oracle).px);
    }
}

#[test]
fn symbols_master_group_and_child_properties_publish_once() {
    let mut s = fixture();
    s.edit("Group", |doc, active| {
        let group = Layer::group("Grouped badge", std::mem::take(&mut doc.layers));
        *active = Some(group.id);
        doc.layers.push(group);
        Ok(())
    })
    .unwrap();
    let (id, _) = make_symbol(&mut s);
    let before = s.active().unwrap().doc.clone();
    let group = &before.symbols[0].layers[0];
    let child = group.children().unwrap()[0].id;
    let revision = s.active().unwrap().revision;
    let journal = s.journal.len();
    call(&mut s, "symbol.editMaster", json!({"symbol":id.0,"edits":[{"layer":group.id.0,"visible":false},{"layer":child.0,"fill":Fill::Solid(Color::WHITE)}]}));
    assert_eq!(s.active().unwrap().revision, revision + 1);
    assert_eq!(s.journal.len(), journal + 1);
    assert!(photocraft_compose::flatten(&s.active().unwrap().doc).px.iter().all(|p| p[3] == 0.));
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}

#[test]
fn symbols_large_cache_budget_is_rejected_without_publication() {
    let mut s = Session::new();
    let mut doc = Document::new("Bounded cache", Size::new(5000, 5000), ColorMode::Rgb, SampleType::U8);
    let layer = Layer::new(
        "Large rectangle",
        LayerContent::Shape(ShapeLayer {
            path: photocraft_vector::shapes::rect(0., 0., 5000., 5000.),
            fill: Some(Fill::Solid(Color::WHITE)),
            ..Default::default()
        }),
    );
    let id = layer.id;
    doc.layers.push(layer);
    s.add_document(doc, None);
    s.select_layer(id).unwrap();
    let before = s.active().unwrap().doc.clone();
    let c = Capture::new(&s).unwrap();
    assert!(create_from_selection(&mut s, c, &[id], "Oversized".into()).is_err());
    assert!(std::sync::Arc::ptr_eq(&s.active().unwrap().doc, &before));
    assert!(s.journal.is_empty());
    c.check(&s).unwrap();
    // Two sparse pixels have a large bounding region: shrinking must reject the
    // source read before the ordinary warp materializes 25 million pixels.
    let mut cache = photocraft_raster::Surface::new(before.pixel_format());
    cache.fill_rect(photocraft_geom::Rect::new(0, 0, 1, 1), &[1., 1., 1., 1.]);
    cache.fill_rect(photocraft_geom::Rect::new(4999, 4999, 5000, 5000), &[1., 1., 1., 1.]);
    let mut instance = SymbolInstance { symbol: SymbolId(1), transform: Affine::IDENTITY, cache: Some(cache) };
    let original = instance.clone();
    assert!(transform_cached_instance(&mut instance, Affine::scale(0.5)).is_err());
    assert_eq!(instance, original);
}

#[test]
fn symbols_transformed_canvas_fill_matches_independent_rectangle() {
    let mut s = Session::new();
    let mut doc = Document::new("Fill template", Size::new(16, 16), ColorMode::Rgb, SampleType::U8);
    let layer = Layer::new("White canvas fill", LayerContent::Fill(Fill::Solid(Color::WHITE)));
    let id = layer.id;
    doc.layers.push(layer);
    s.add_document(doc, None);
    s.select_layer(id).unwrap();
    let (master, _) = make_symbol(&mut s);
    let mut actual = (*s.active().unwrap().doc).clone();
    actual.layers.clear();
    let mut instance = SymbolInstance { symbol: master, transform: Affine::translate(4., 3.), cache: None };
    instance.cache = Some(render_instance(&actual, &instance).unwrap());
    actual.layers.push(Layer::new("Instance", LayerContent::Symbol(instance)));
    let mut oracle = actual.clone();
    let mut sh = ShapeLayer { path: photocraft_vector::shapes::rect(4., 3., 16., 16.), fill: Some(Fill::Solid(Color::WHITE)), ..Default::default() };
    crate::vector_cmds::refresh_shape(&oracle, &mut sh);
    oracle.layers = vec![Layer::new("Rectangle", LayerContent::Shape(sh))];
    assert_eq!(photocraft_compose::flatten(&actual).px, photocraft_compose::flatten(&oracle).px);
    let detached = detach_prepared(&actual, &actual.layers[0]).unwrap();
    let mut expanded = actual.clone();
    expanded.layers = vec![detached];
    assert_eq!(photocraft_compose::flatten(&expanded).px, photocraft_compose::flatten(&oracle).px);
}

#[test]
fn symbols_text_master_edit_matches_independent_type_layers() {
    fn empty() -> Session {
        let mut s = Session::new();
        s.add_document(Document::new("Text symbols", Size::new(128, 96), ColorMode::Rgb, SampleType::U8), None);
        s
    }
    let mut s = empty();
    let text = s.execute("type.create", json!({"x":2,"y":20,"text":"AAA","size":12})).unwrap()["layer"].as_u64().unwrap();
    let created = call(&mut s, "symbol.create", json!({"layers":[text],"name":"Label"}));
    let master = created["symbol"].as_u64().unwrap();
    for x in [30., 60.] {
        call(&mut s, "symbol.place", json!({"symbol":master,"transform":Affine::translate(x,20.)}));
    }
    let child = s.active().unwrap().doc.symbols[0].layers[0].id;
    call(&mut s, "symbol.editMaster", json!({"symbol":master,"edits":[{"layer":child.0,"text":"BBB"}]}));
    let mut ordinary = empty();
    for (x, y) in [(2, 20), (32, 40), (62, 40)] {
        ordinary.execute("type.create", json!({"x":x,"y":y,"text":"BBB","size":12})).unwrap();
    }
    let expected = photocraft_compose::flatten(&ordinary.active().unwrap().doc);
    assert!(expected.px.iter().any(|p| p[3] > 0.5), "Latin font must render");
    assert_eq!(photocraft_compose::flatten(&s.active().unwrap().doc).px, expected.px);
    let LayerContent::Text(t) = &s.active().unwrap().doc.symbols[0].layers[0].content else { panic!() };
    assert_eq!(t.runs.iter().map(|r| r.len).sum::<usize>(), t.text.len());
}
