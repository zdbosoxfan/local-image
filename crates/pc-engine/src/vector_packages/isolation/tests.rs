#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, ColorMode, Fill, SampleType, ShapeLayer, Size};
fn shape(doc: &Document, x: f64) -> Layer {
    let mut s = ShapeLayer { path: photocraft_vector::shapes::rect(x, 2., 8., 8.), fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
    crate::vector_cmds::refresh_shape(doc, &mut s);
    Layer::new("Rectangle", LayerContent::Shape(s))
}
fn fixture() -> (Session, LayerId, LayerId, LayerId) {
    let mut doc = Document::new("Isolation", Size::new(64, 64), ColorMode::Rgb, SampleType::U8);
    let child = shape(&doc, 2.);
    let id = child.id;
    let group = Layer::group("Group", vec![child]);
    let group_id = group.id;
    let outside = shape(&doc, 22.);
    let outside_id = outside.id;
    doc.layers = vec![group, outside];
    let mut session = Session::new();
    session.add_document(doc, None);
    (session, group_id, id, outside_id)
}
fn enter(s: &mut Session, state: &mut State, id: LayerId) {
    let c = Capture::new(s, state).unwrap();
    state.enter(s, c, id).unwrap();
}
#[test]
fn view37_isolation_group_insertion_one_edit_and_scope_excludes_others() {
    let (mut s, group, child, outside) = fixture();
    let mut state = State::default();
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let journal = s.journal.len();
    enter(&mut s, &mut state, group);
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, revision);
    assert_eq!(s.journal.len(), journal);
    let scope = state.scope(&before).unwrap();
    assert_eq!(scope.select_all(&before), vec![child]);
    assert!(!scope.selectable(&before, outside));
    assert!(!scope.selectable(&before, group));
    let layer = shape(&before, 14.);
    let id = layer.id;
    let capture = Capture::new(&s, &state).unwrap();
    insert_prepared(&mut s, &state, capture, None, layer).unwrap();
    assert_eq!(s.active().unwrap().doc.layer(group).unwrap().children().unwrap().iter().map(|l| l.id).collect::<Vec<_>>(), vec![child, id]);
    assert_eq!(s.active().unwrap().revision, revision + 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    state.exit();
    assert!(state.scope(&before).unwrap().selectable(&before, outside));
}
#[test]
fn view37_isolation_nested_breadcrumbs_dim_only_outside_and_switch_exits() {
    let (mut s, parent, child, _) = fixture();
    let nested = Layer::group("Nested", vec![shape(&s.active().unwrap().doc, 12.)]);
    let nested_id = nested.id;
    s.edit("Fixture", |doc, _| {
        doc.layer_mut(parent).unwrap().children_mut().unwrap().push(nested);
        Ok(())
    })
    .unwrap();
    let mut state = State::default();
    enter(&mut s, &mut state, parent);
    enter(&mut s, &mut state, nested_id);
    let doc = s.active().unwrap().doc.clone();
    let scope = state.scope(&doc).unwrap();
    assert!(scope.dimmed(&doc, child));
    assert!(!scope.dimmed(&doc, parent));
    assert!(!scope.dimmed(&doc, nested_id));
    assert_eq!(state.breadcrumbs().len(), 2);
    state.navigate(1).unwrap();
    assert_eq!(state.current(), Some(Target::Group { layer: parent }));
    let rev = s.active().unwrap().revision;
    assert!(state.synchronize(&s, false));
    assert!(state.current().is_none());
    assert_eq!(s.active().unwrap().revision, rev);
    enter(&mut s, &mut state, parent);
    s.add_document(Document::new("Other", Size::new(8, 8), ColorMode::Rgb, SampleType::U8), None);
    assert!(state.synchronize(&s, true));
    assert!(state.current().is_none());
}
#[test]
fn view37_isolation_stale_or_invalid_target_never_changes_document_or_scope() {
    let (mut s, group, _, outside) = fixture();
    let mut state = State::default();
    let c = Capture::new(&s, &state).unwrap();
    s.edit("Fixture", |doc, _| {
        doc.name = "Changed".into();
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    assert!(state.enter(&mut s, c, group).is_err());
    assert!(state.current().is_none());
    let c = Capture::new(&s, &state).unwrap();
    assert!(state.enter(&mut s, c, outside).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, rev);
    enter(&mut s, &mut state, group);
    let c = Capture::new(&s, &state).unwrap();
    state.exit();
    enter(&mut s, &mut state, group);
    let layer = shape(&before, 14.);
    assert!(insert_prepared(&mut s, &state, c, None, layer).is_err());
}
#[test]
fn view37_isolation_locked_ancestor_missing_target_and_deletion_are_rejected() {
    let (mut s, group, _, outside) = fixture();
    let mut state = State::default();
    s.edit("Lock", |doc, _| {
        doc.layer_mut(group).unwrap().locks.all = true;
        Ok(())
    })
    .unwrap();
    let c = Capture::new(&s, &state).unwrap();
    assert!(state.enter(&mut s, c, group).is_err());
    assert!(state.enter(&mut s, c, LayerId(u64::MAX)).is_err());
    s.undo();
    enter(&mut s, &mut state, group);
    let capture = Capture::new(&s, &state).unwrap();
    let layer = shape(&s.active().unwrap().doc, 12.);
    let before = s.active().unwrap().doc.clone();
    assert!(insert_prepared(&mut s, &state, capture, Some(outside), layer).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    s.edit("Delete", |doc, _| {
        doc.remove(group);
        Ok(())
    })
    .unwrap();
    assert!(state.synchronize(&s, true));
    assert!(state.current().is_none());
}
#[test]
fn view37_isolation_unprepared_or_duplicate_insertion_is_atomic() {
    let (mut s, group, child, _) = fixture();
    let mut state = State::default();
    enter(&mut s, &mut state, group);
    let before = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    let c = Capture::new(&s, &state).unwrap();
    let duplicate = before.layer(child).unwrap().clone();
    assert!(insert_prepared(&mut s, &state, c, None, duplicate).is_err());
    let unprepared = Layer::new("Bad", LayerContent::Shape(ShapeLayer::default()));
    assert!(insert_prepared(&mut s, &state, c, None, unprepared).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, rev);
}
#[test]
fn view37_isolation_compound_scope_and_missing_provider_are_explicit() {
    let (mut s, _, child, _) = fixture();
    s.edit("Compound", |doc, _| {
        if let LayerContent::Shape(shape) = &mut doc.layer_mut(child).unwrap().content {
            shape.path.subpaths.extend(photocraft_vector::shapes::rect(14., 14., 4., 4.).subpaths);
        }
        Ok(())
    })
    .unwrap();
    let mut state = State::default();
    enter(&mut s, &mut state, child);
    let before = s.active().unwrap().doc.clone();
    assert_eq!(state.scope(&before).unwrap().select_all(&before), vec![child]);
    let c = Capture::new(&s, &state).unwrap();
    assert!(insert_prepared(&mut s, &state, c, None, shape(&before, 20.)).is_err());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn view37_isolation_symbol_master_insertion_refreshes_instances_and_undo() {
    use photocraft_doc::{SymbolDef, SymbolId, SymbolInstance};
    let (mut s, _, _, _) = fixture();
    let master = SymbolId(700);
    let mut template = shape(&s.active().unwrap().doc, 2.);
    if let LayerContent::Shape(shape) = &mut template.content {
        // Master source caches are optional; preparing new insertion must not require them.
        shape.cache = None;
    }
    let instance = Layer::new(
        "Instance",
        LayerContent::Symbol(SymbolInstance { symbol: master, transform: Affine::translate(20., 10.).mul(&Affine::scale(2.)), cache: None }),
    );
    let instance_id = instance.id;
    s.edit("Fixture", |doc, _| {
        doc.symbols.push(SymbolDef { id: master, name: "Master".into(), layers: vec![template] });
        doc.layers.push(instance);
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    let mut state = State::default();
    enter(&mut s, &mut state, instance_id);
    let target = state.insertion_target(&before).unwrap();
    assert_eq!(target.from_document.apply(photocraft_geom::Point::new(24., 14.)), photocraft_geom::Point::new(2., 2.));
    assert_eq!(state.scope(&before).unwrap().master_layers(&before).unwrap().unwrap().len(), 1);
    let layer = shape(&before, 14.);
    let id = layer.id;
    let c = Capture::new(&s, &state).unwrap();
    insert_prepared(&mut s, &state, c, None, layer).unwrap();
    assert!(s.active().unwrap().doc.symbols[0].layers.iter().any(|l| l.id == id));
    if let LayerContent::Symbol(instance) = &s.active().unwrap().doc.layer(instance_id).unwrap().content {
        assert!(instance.cache.is_some());
    } else {
        panic!("symbol missing");
    }
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn view37_view_commands_check_captures_and_never_edit_or_journal() {
    let (s, group, _, _) = fixture();
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let mut state = ViewState::default();
    for action in [ViewAction::Set { mode: ViewMode::Xray }, ViewAction::TogglePixels, ViewAction::ToggleOutline, ViewAction::LayerOutline { layer: group }] {
        let request = ViewChange { document: before.id, revision, generation: state.generation, action };
        change_view(&s, &mut state, &request).unwrap();
        assert!(change_view(&s, &mut state, &request).is_err(), "replayed stale view gesture must fail");
    }
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, revision);
    assert!(s.journal.is_empty());
    assert!(specs().iter().chain(view_specs().iter()).all(|spec| !spec.journal));
    let request = ViewChange { document: before.id, revision, generation: state.generation, action: ViewAction::LayerOutline { layer: LayerId(u64::MAX) } };
    let generation = state.generation;
    assert!(change_view(&s, &mut state, &request).is_err());
    assert_eq!(state.generation, generation);
}

#[test]
fn view37_insertion_nested_anchor_depth_budget_is_atomic() {
    let mut doc = Document::new("Depth boundary", Size::new(64, 64), ColorMode::Rgb, SampleType::U8);
    let mut nested = shape(&doc, 2.);
    let anchor = nested.id;
    for _ in 0..photocraft_doc::MAX_GROUP_DEPTH {
        nested = Layer::group("Nested", vec![nested]);
    }
    doc.layers = vec![nested];
    assert_eq!(doc.max_group_depth(), photocraft_doc::MAX_GROUP_DEPTH);
    let new = Layer::group("New group", vec![shape(&doc, 14.)]);
    let mut session = Session::new();
    session.add_document(doc, None);
    let before = session.active().unwrap().doc.clone();
    let revision = session.active().unwrap().revision;
    let state = State::default();
    let capture = Capture::new(&session, &state).unwrap();
    assert!(insert_prepared(&mut session, &state, capture, Some(anchor), new).is_err());
    assert_eq!(session.active().unwrap().doc, before);
    assert_eq!(session.active().unwrap().revision, revision);
    assert!(session.journal.is_empty());
}
