#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, ColorMode, Fill, SampleType, ShapeLayer, Subpath};
use std::sync::Arc;
fn shape(doc: &Document, name: &str, b: Box2) -> Layer {
    let mut shape = ShapeLayer { path: crop_path(b).unwrap(), fill: Some(Fill::Solid(Color::rgb(0.8, 0.2, 0.1))), ..Default::default() };
    shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
    Layer::new(name, LayerContent::Shape(shape))
}
fn setup() -> (Session, Vec<LayerId>) {
    let mut doc = Document::new("Layout", Size::new(256, 192), ColorMode::Rgb, SampleType::U8);
    for (name, b) in [("A", [10., 20., 20., 30.]), ("B", [70., 35., 40., 20.]), ("C", [140., 10., 30., 40.])] {
        doc.layers.push(shape(&doc, name, b));
    }
    let ids = doc.layers.iter().map(|l| l.id).collect();
    let mut s = Session::new();
    s.add_document(doc, None);
    (s, ids)
}
fn b(s: &Session, id: LayerId) -> Box2 {
    bounds(s.active().unwrap().doc.layer(id).unwrap()).unwrap()
}
#[test]
fn layout16_anchor_transform_keeps_selected_anchor_and_has_one_undo() {
    let (mut s, ids) = setup();
    let before = s.active().unwrap().doc.clone();
    let capture = Capture::begin(&s, vec![ids[0]]).unwrap();
    let pivot = Point::new(30., 50.);
    let matrix = anchored(pivot, Point::new(0., 0.), [2., 3.], 0., 0.).unwrap();
    let request = Commit { capture, operation: Operation::Transform { matrix } };
    let detached = preview(&s, &request).unwrap();
    assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc));
    assert!(s.journal.is_empty());
    assert_eq!(bounds(detached.layer(ids[0]).unwrap()).unwrap(), [-10., -40., 40., 90.]);
    s.execute("vector.layout.apply", json!(request)).unwrap();
    assert_eq!(s.active().unwrap().doc.as_ref(), &detached);
    assert_eq!(s.journal.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
}
#[test]
fn layout16_point_similarity_has_independent_pivot_and_endpoint_oracles() {
    let pivot = Point::new(10., 20.);
    let from = Point::new(40., 20.);
    let to = Point::new(10., 80.);
    let a = point_transform(pivot, from, to, false, false).unwrap();
    for (actual, expected) in [(a.apply(pivot), pivot), (a.apply(from), to)] {
        assert!((actual.x - expected.x).abs() < 1e-10);
        assert!((actual.y - expected.y).abs() < 1e-10);
    }
    assert!((a.determinant() - 4.).abs() < 1e-10);
    let only_rotation = point_transform(pivot, from, to, false, true).unwrap();
    assert!((only_rotation.determinant() - 1.).abs() < 1e-10);
    assert!(point_transform(pivot, pivot, to, false, false).is_err());
}
#[test]
fn layout16_key_alignment_and_interior_key_spacing_preserve_key() {
    let (mut s, ids) = setup();
    let capture = Capture::begin(&s, ids.clone()).unwrap();
    s.execute("vector.layout.align", json!({"capture":capture,"key":ids[1].0,"edge":"right"})).unwrap();
    for id in &ids {
        let box2 = b(&s, *id);
        assert_eq!(box2[0] + box2[2], 110.);
    }
    assert!(s.undo());
    let before = s.active().unwrap().doc.clone();
    let capture = Capture::begin(&s, ids.clone()).unwrap();
    s.execute("vector.layout.spacing", json!({"capture":capture,"key":ids[1].0,"axis":"x","spacing":12.5})).unwrap();
    assert_eq!(b(&s, ids[1]), [70., 35., 40., 20.]);
    assert_eq!(b(&s, ids[0])[0], 37.5);
    assert_eq!(b(&s, ids[2])[0], 122.5);
    assert_eq!(b(&s, ids[1])[0] - (b(&s, ids[0])[0] + b(&s, ids[0])[2]), 12.5);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn layout16_automatic_spacing_preserves_outer_edges() {
    let (mut s, ids) = setup();
    let capture = Capture::begin(&s, ids.clone()).unwrap();
    s.execute("vector.layout.spacing", json!({"capture":capture,"axis":"x"})).unwrap();
    assert_eq!(b(&s, ids[0])[0], 10.);
    assert_eq!(b(&s, ids[2])[0] + b(&s, ids[2])[2], 170.);
    let left = b(&s, ids[0]);
    let middle = b(&s, ids[1]);
    let right = b(&s, ids[2]);
    assert_eq!(middle[0] - left[0] - left[2], right[0] - middle[0] - middle[2]);
}
#[test]
fn layout16_invalid_late_target_duplicate_key_and_stale_capture_are_atomic() {
    let (mut s, ids) = setup();
    let before = s.active().unwrap().doc.clone();
    for targets in [vec![ids[0], LayerId(u64::MAX)], vec![ids[0], ids[0]]] {
        let c = Capture { document: before.id.0, revision: 0, layers: targets };
        assert!(s.execute("vector.layout.apply", json!(Commit { capture: c, operation: Operation::Crop { rect: Some([20., 20., 10., 10.]) } })).is_err());
        assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc));
        assert!(s.journal.is_empty());
    }
    let c = Capture::begin(&s, ids.clone()).unwrap();
    assert!(s.execute("vector.layout.spacing", json!({"capture":c,"axis":"x","key":99999999})).is_err());
    s.edit("Other edit", |doc, _| {
        doc.name = "Other".into();
        Ok(())
    })
    .unwrap();
    let after = s.active().unwrap().doc.clone();
    assert!(s.execute("vector.layout.apply", json!(Commit { capture: c, operation: Operation::Transform { matrix: Affine::IDENTITY } })).is_err());
    assert!(Arc::ptr_eq(&after, &s.active().unwrap().doc));
    assert!(s.journal.is_empty());
}
#[test]
fn layout16_all_descendant_locks_and_ancestor_overlap_are_preflighted() {
    let (mut s, ids) = setup();
    let mut doc = (*s.active().unwrap().doc).clone();
    let mut group = Layer::group("Group", vec![doc.layers.remove(0), doc.layers.remove(0)]);
    let gid = group.id;
    if let LayerContent::Group(g) = &mut group.content {
        g.children[1].locks.position = true;
    }
    doc.layers.push(group);
    s.active_mut().unwrap().doc = Arc::new(doc);
    assert!(Capture::begin(&s, vec![gid]).is_err());
    assert!(Capture::begin(&s, vec![gid, ids[0]]).is_err());
}
#[test]
fn layout16_crop_preserves_geometry_masks_and_undo_clear_restore() {
    let (mut s, ids) = setup();
    s.edit("Mask", |doc, _| {
        doc.layer_mut(ids[0]).unwrap().vector_mask = Some(VectorMask::new(crop_path([12., 20., 16., 30.]).unwrap()));
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    let original_path = match &before.layer(ids[0]).unwrap().content {
        LayerContent::Shape(s) => s.path.clone(),
        _ => panic!(),
    };
    let c = Capture::begin(&s, vec![ids[0]]).unwrap();
    let request = Commit { capture: c, operation: Operation::Crop { rect: Some([16., 25., 8., 15.]) } };
    let preview = preview(&s, &request).unwrap();
    let shown = project_clips(&preview).unwrap();
    let pixels = photocraft_compose::flatten(&shown);
    assert_eq!(pixels.get(17, 30)[3], 1.);
    assert_eq!(pixels.get(13, 30)[3], 0.);
    s.execute("vector.layout.apply", json!(request)).unwrap();
    let l = s.active().unwrap().doc.layer(ids[0]).unwrap();
    let LayerContent::Shape(sh) = &l.content else { panic!() };
    assert_eq!(sh.path, original_path);
    assert_eq!(l.vector_mask, before.layer(ids[0]).unwrap().vector_mask);
    let crop = s.active().unwrap().doc.clone();
    let c = Capture::begin(&s, vec![ids[0]]).unwrap();
    s.execute("vector.layout.apply", json!(Commit { capture: c, operation: Operation::Crop { rect: None } })).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, crop);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn layout16_crop_content_move_keeps_clip_in_place() {
    let (mut s, ids) = setup();
    let c = Capture::begin(&s, vec![ids[0]]).unwrap();
    s.execute("vector.layout.apply", json!(Commit { capture: c, operation: Operation::Crop { rect: Some([15., 25., 10., 15.]) } })).unwrap();
    let clip = s.active().unwrap().doc.layer(ids[0]).unwrap().vector.clip.clone();
    let c = Capture::begin(&s, vec![ids[0]]).unwrap();
    s.execute("vector.layout.apply", json!(Commit { capture: c, operation: Operation::ContentMove { matrix: Affine::translate(20., 0.) } })).unwrap();
    assert_eq!(s.active().unwrap().doc.layer(ids[0]).unwrap().vector.clip, clip);
    assert_eq!(b(&s, ids[0])[0], 30.);
}
#[test]
fn layout16_resize_rotated_child_preserves_right_margin_and_exact_vertices() {
    let (mut s, ids) = setup();
    let mut doc = (*s.active().unwrap().doc).clone();
    let mut child = doc.layers.remove(0);
    let path = Path::new(vec![Subpath::polygon(&[(60., 30.), (70., 20.), (80., 30.), (70., 40.)])]);
    if let LayerContent::Shape(sh) = &mut child.content {
        sh.path = path.clone();
        crate::vector_cmds::refresh_shape(&doc, sh);
    }
    child.vector.constraints =
        Some(photocraft_doc::vector_design::structure::Constraints { right: true, top: true, fixed_width: true, fixed_height: true, ..Default::default() });
    let mut board = Layer::group("Board", vec![child]);
    let gid = board.id;
    if let LayerContent::Group(g) = &mut board.content {
        g.artboard =
            Some(Artboard { rect: Rect::new(0, 0, 100, 100), background: ArtboardBackground::Transparent, preset: String::new(), drawing_scale: None });
    }
    doc.layers.push(board);
    s.active_mut().unwrap().doc = Arc::new(doc);
    let c = Capture::begin(&s, vec![gid]).unwrap();
    s.execute("vector.layout.apply", json!(Commit { capture: c, operation: Operation::Resize { rect: [0., 0., 160., 100.], basis: Affine::IDENTITY } }))
        .unwrap();
    let LayerContent::Shape(sh) = &s.active().unwrap().doc.layer(ids[0]).unwrap().content else { panic!() };
    for (actual, original) in sh.path.subpaths[0].knots.iter().zip(&path.subpaths[0].knots) {
        assert_eq!(actual.anchor, Point::new(original.anchor.x + 60., original.anchor.y));
    }
    assert_eq!(160. - b(&s, ids[0])[0] - b(&s, ids[0])[2], 20.);
}
#[test]
fn layout16_existing_artboard_set_applies_constraints_without_changing_legacy_unconstrained_children() {
    let (mut s, ids) = setup();
    let mut doc = (*s.active().unwrap().doc).clone();
    let mut constrained = doc.layers.remove(0);
    constrained.vector.constraints =
        Some(photocraft_doc::vector_design::structure::Constraints { right: true, fixed_width: true, top: true, fixed_height: true, ..Default::default() });
    let unconstrained = doc.layers.remove(0);
    let mut board = Layer::group("Board", vec![constrained, unconstrained]);
    let gid = board.id;
    if let LayerContent::Group(g) = &mut board.content {
        g.artboard =
            Some(Artboard { rect: Rect::new(0, 0, 200, 150), background: ArtboardBackground::Transparent, preset: String::new(), drawing_scale: None });
    }
    doc.layers.push(board);
    s.active_mut().unwrap().doc = Arc::new(doc);
    let before = s.active().unwrap().doc.clone();
    s.execute("layer.select", json!({"layer":gid.0})).unwrap();
    s.execute("layer.artboard.set", json!({"layer":gid.0,"width":250,"height":150})).unwrap();
    assert_eq!(b(&s, ids[0])[0], 60.);
    assert_eq!(b(&s, ids[1]), [70., 35., 40., 20.]);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn layout16_units_exact_definitions_and_legacy_default() {
    for dpi in [72., 96., 144., 300., 600.] {
        for (unit, value) in [(DesignUnits::Points, 72.), (DesignUnits::Millimeters, 25.4), (DesignUnits::Inches, 1.), (DesignUnits::Pixels, dpi)] {
            assert!((to_pixels(value, unit, dpi).unwrap() - dpi).abs() < 1e-10);
            assert!((from_pixels(dpi, unit, dpi).unwrap() - value).abs() < 1e-10);
        }
    }
    assert_eq!(serde_json::from_str::<photocraft_doc::vector_design::structure::Constraints>("{}").unwrap(), Default::default());
    assert_eq!(DesignUnits::default(), DesignUnits::Pixels);
    let (mut s, _) = setup();
    assert_eq!(s.active().unwrap().doc.design_units, DesignUnits::Pixels);
    s.execute("vector.layout.newDocument", json!({"category":"Print","width":25.4,"height":50.8,"dpi":300})).unwrap();
    assert_eq!(s.active().unwrap().doc.design_units, DesignUnits::Millimeters);
    assert_eq!(s.active().unwrap().doc.size, Size::new(300, 600));
    s.execute("vector.layout.newDocument", json!({"category":"Art & Illustration","width":72,"height":144,"dpi":144})).unwrap();
    assert_eq!(s.active().unwrap().doc.design_units, DesignUnits::Points);
    assert_eq!(s.active().unwrap().doc.size, Size::new(144, 288));
    assert_eq!(preset_units("Photo"), DesignUnits::Pixels);
    for dpi in [0., -1., f64::NAN, 40000.] {
        assert!(to_pixels(1., DesignUnits::Points, dpi).is_err());
    }
    assert!(to_pixels(f64::INFINITY, DesignUnits::Pixels, 72.).is_err());
}
#[test]
fn layout16_bounded_caches_and_invalid_transforms() {
    let (s, ids) = setup();
    let c = Capture::begin(&s, ids).unwrap();
    for matrix in [Affine { m: [1., 0., 0., 0., 0., 0.] }, Affine { m: [f64::NAN, 0., 0., 1., 0., 0.] }, Affine::scale(10000.)] {
        assert!(preview(&s, &Commit { capture: c.clone(), operation: Operation::Transform { matrix } }).is_err());
    }
    let doc = preview(&s, &Commit { capture: c, operation: Operation::Transform { matrix: Affine::translate(1., 1.) } }).unwrap();
    for l in &doc.layers {
        assert!(l.surface().unwrap().tile_count() <= 4);
    }
}
#[test]
fn layout16_native_old_fixture_defaults_pixels_and_crop_roundtrip() {
    let old = include_bytes!("../../../tests/fixtures/vector-v1.pcraft");
    let old = photocraft_format::load_from_bytes(old).unwrap();
    assert_eq!(old.design_units, DesignUnits::Pixels);
    let (s, ids) = setup();
    let c = Capture::begin(&s, vec![ids[0]]).unwrap();
    let doc = preview(&s, &Commit { capture: c, operation: Operation::Crop { rect: Some([12., 20., 10., 10.]) } }).unwrap();
    // The native mirror already owns serde defaults and clip persistence.
    let bytes = photocraft_format::save_to_bytes(&doc, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!(loaded.layer(ids[0]).unwrap().vector.clip, doc.layer(ids[0]).unwrap().vector.clip);
}
#[path = "gpu.rs"]
mod gpu;

#[test]
fn layout16_large_parent_frame_never_allocates_a_parent_sized_cache() {
    let mut doc = Document::new("Large layout", Size::new(6000, 4000), ColorMode::Rgb, SampleType::U8);
    let mut child = shape(&doc, "Small pinned child", [5900., 20., 40., 30.]);
    let id = child.id;
    child.vector.constraints =
        Some(photocraft_doc::vector_design::structure::Constraints { right: true, fixed_width: true, fixed_height: true, top: true, ..Default::default() });
    let mut parent = Layer::group("Large artboard", vec![child]);
    let gid = parent.id;
    if let LayerContent::Group(g) = &mut parent.content {
        g.artboard =
            Some(Artboard { rect: Rect::new(0, 0, 6000, 4000), background: ArtboardBackground::Transparent, preset: String::new(), drawing_scale: None });
    }
    doc.layers.push(parent);
    let mut s = Session::new();
    s.add_document(doc, None);
    let capture = Capture::begin(&s, vec![gid]).unwrap();
    let detached = preview(&s, &Commit { capture, operation: Operation::Resize { rect: [0., 0., 6400., 4000.], basis: Affine::IDENTITY } }).unwrap();
    assert_eq!(bounds(detached.layer(id).unwrap()).unwrap(), [6300., 20., 40., 30.]);
    assert!(detached.layer(gid).unwrap().surface().is_none());
    assert!(detached.layer(id).unwrap().surface().unwrap().tile_count() <= 4);
}

#[test]
fn layout16_aggregate_output_budget_refuses_many_safe_individual_extents_atomically() {
    let (mut s, ids) = setup();
    s.edit("Large source geometry", |doc, _| {
        for id in &ids {
            let LayerContent::Shape(sh) = &mut doc.layer_mut(*id).unwrap().content else { panic!() };
            sh.path = crop_path([0., 0., 1000., 1000.]).unwrap();
        }
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    let capture = Capture::begin(&s, ids).unwrap();
    let matrix = Affine::scale(4.);
    let error = s.execute("vector.layout.apply", json!(Commit { capture, operation: Operation::Transform { matrix } })).unwrap_err();
    assert!(error.to_string().contains("aggregate"));
    assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc));
    assert!(s.journal.is_empty());
}

#[test]
fn layout16_aggregate_preflight_counts_cached_appearance_extent_not_only_small_paths() {
    let (mut s, ids) = setup();
    s.edit("Sparse wide cache", |doc, _| {
        let fmt = doc.pixel_format();
        for id in &ids {
            let LayerContent::Shape(sh) = &mut doc.layer_mut(*id).unwrap().content else { panic!() };
            let mut cache = photocraft_doc::Surface::new(fmt);
            cache.fill_rect(Rect::new(0, 0, 1, 1), &[1., 0., 0., 1.]);
            cache.fill_rect(Rect::new(4095, 4095, 4096, 4096), &[1., 0., 0., 1.]);
            sh.cache = Some(cache);
        }
        Ok(())
    })
    .unwrap();
    assert!(Capture::begin(&s, ids).unwrap_err().to_string().contains("aggregate"));
}

#[test]
fn layout16_negative_pasteboard_artboard_create_resize_does_not_wrap_canvas_dimensions() {
    let (mut s, _) = setup();
    let before = s.active().unwrap().doc.clone();
    let capture = Capture { document: before.id.0, revision: s.active().unwrap().revision, layers: vec![] };
    s.execute("vector.layout.apply", json!(Commit { capture, operation: Operation::Artboard { rect: [-150., -100., 80., 60.] } })).unwrap();
    let st = s.active().unwrap();
    assert_eq!(st.doc.size, before.size);
    let id = st.doc.artboards()[0].0;
    assert_eq!(st.doc.layer(id).unwrap().artboard().unwrap().rect, Rect::new(-150, -100, -70, -40));
    let capture = Capture::begin(&s, vec![id]).unwrap();
    s.execute("vector.layout.apply", json!(Commit { capture, operation: Operation::Resize { rect: [-100., -60., 100., 80.], basis: Affine::IDENTITY } }))
        .unwrap();
    assert_eq!(s.active().unwrap().doc.size, before.size);
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().artboard().unwrap().rect, Rect::new(-100, -60, 0, 20));
    assert!(s.undo());
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}

#[test]
fn layout16_fractional_artboard_requests_pin_to_the_published_integer_frame_without_drift() {
    let (mut s, ids) = setup();
    let gid = s
        .edit("Pinned board", |doc, _| {
            let mut child = doc.layers.remove(0);
            child.vector.constraints = Some(photocraft_doc::vector_design::structure::Constraints {
                right: true,
                top: true,
                fixed_width: true,
                fixed_height: true,
                ..Default::default()
            });
            let mut parent = Layer::group("Rounded board", vec![child]);
            if let LayerContent::Group(g) = &mut parent.content {
                g.artboard =
                    Some(Artboard { rect: Rect::new(0, 0, 200, 160), background: ArtboardBackground::Transparent, preset: String::new(), drawing_scale: None });
            }
            let id = parent.id;
            doc.layers.push(parent);
            Ok(id)
        })
        .unwrap();
    for width in [240.25, 280.25] {
        let capture = Capture::begin(&s, vec![gid]).unwrap();
        s.execute("vector.layout.apply", json!(Commit { capture, operation: Operation::Resize { rect: [0., 0., width, 160.], basis: Affine::IDENTITY } }))
            .unwrap();
        let st = s.active().unwrap();
        let parent = st.doc.layer(gid).unwrap().artboard().unwrap().rect;
        let child = b(&s, ids[0]);
        assert_eq!(f64::from(parent.x1) - child[0] - child[2], 170.);
        assert_eq!((child[1], child[2], child[3]), (20., 20., 30.));
    }
}
#[test]
fn layout16_group_padding_without_a_persisted_frame_is_rejected_atomically() {
    let (mut s, _) = setup();
    let group = s
        .edit("Frameless group", |doc, _| {
            let mut child = doc.layers.remove(0);
            child.vector.constraints =
                Some(photocraft_doc::vector_design::structure::Constraints { fixed_width: true, fixed_height: true, ..Default::default() });
            let group = Layer::group("No stored frame", vec![child]);
            let id = group.id;
            doc.layers.push(group);
            Ok(id)
        })
        .unwrap();
    let before = s.active().unwrap().doc.clone();
    let capture = Capture::begin(&s, vec![group]).unwrap();
    let err = s
        .execute("vector.layout.apply", json!(Commit { capture, operation: Operation::Resize { rect: [10., 20., 40., 30.], basis: Affine::IDENTITY } }))
        .unwrap_err();
    assert!(err.to_string().contains("container frame"));
    assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc));
    assert!(s.journal.is_empty());
}

#[test]
fn layout16_constraint_set_requires_boolean_object_or_null_and_is_atomic() {
    let (mut s, ids) = setup();
    let before = s.active().unwrap().doc.clone();
    let capture = Capture::begin(&s, vec![ids[0]]).unwrap();
    for raw in [json!([true, false, false, false, true, false]), json!({"right":1}), json!({"misspelled":true})] {
        assert!(s.execute("vector.constraints.set", json!({"capture":capture,"constraints":raw})).is_err());
        assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc));
        assert!(s.journal.is_empty());
    }
    s.execute("vector.constraints.set", json!({"capture":capture,"constraints":{"right":true,"fixed_width":true}})).unwrap();
    assert!(s.active().unwrap().doc.layer(ids[0]).unwrap().vector.constraints.unwrap().right);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}

#[test]
fn review_regression_layout16_automatic_spacing_uses_farthest_edge_not_last_origin() {
    let (mut s, ids) = setup();
    s.edit("Wide middle", |doc, _| {
        let layer = shape(doc, "Wide middle", [70., 35., 130., 20.]);
        doc.layer_mut(ids[1]).unwrap().content = layer.content;
        Ok(())
    })
    .unwrap();
    let capture = Capture::begin(&s, ids.clone()).unwrap();
    let result = s.execute("vector.layout.spacing", json!({"capture":capture,"axis":"x"})).unwrap();
    assert_eq!(result["spacing"], 5.);
    assert_eq!(b(&s, ids[0])[0], 10.);
    assert_eq!(b(&s, ids[2])[0] + b(&s, ids[2])[2], 200.);
}
