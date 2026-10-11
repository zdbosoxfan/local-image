#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem};
use photocraft_doc::vector_design::exchange::ExportScope;
use photocraft_doc::{Color, ColorMode, SampleType, Size, Subpath, TextLayer};
use std::sync::Arc;

pub(super) fn fixture() -> (Session, Vec<LayerId>) {
    let mut doc = Document::new("Clip/compound 35", Size::new(96, 96), ColorMode::Rgb, SampleType::F32);
    let paths = [
        photocraft_vector::shapes::rect(0., 0., 96., 96.),
        photocraft_vector::shapes::rect(8., 16., 72., 32.),
        photocraft_vector::shapes::rect(28., 2., 40., 90.),
        Path::new(vec![Subpath::polygon(&[
            (48., 4.),
            (58., 34.),
            (90., 34.),
            (64., 54.),
            (74., 86.),
            (48., 66.),
            (22., 86.),
            (32., 54.),
            (6., 34.),
            (38., 34.),
        ])]),
    ];
    let mut ids = Vec::new();
    for (i, path) in paths.into_iter().enumerate() {
        let color = Color::rgb([0.9, 0.2, 0.3, 0.6][i], [0.1, 0.8, 0.3, 0.5][i], [0.1, 0.3, 0.9, 0.5][i]);
        let mut layer = Layer::new(format!("Shape {i}"), LayerContent::Shape(ShapeLayer { path, fill: Some(Fill::Solid(color)), ..Default::default() }));
        render_layer(&doc, &mut layer).unwrap();
        ids.push(layer.id);
        doc.layers.push(layer);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    s.active_mut().unwrap().selected_layers = ids.clone();
    (s, ids)
}
fn command(s: &mut Session, ids: &[LayerId], cmd: &str) -> Value {
    let params = Capture::begin(s, ids).unwrap().params().unwrap();
    s.execute(cmd, params).unwrap()
}
fn group(s: &mut Session, ids: &[LayerId]) -> LayerId {
    LayerId(command(s, ids, "object.clippingMask.make")["layer"].as_u64().unwrap())
}
fn assert_diff(a: &[[f32; 4]], b: &[[f32; 4]]) {
    assert_eq!(a.len(), b.len());
    let max = a
        .iter()
        .zip(b)
        .flat_map(|(a, b)| {
            (0..4).map(move |c| {
                let pa = if c == 3 { a[c] } else { a[c] * a[3] };
                let pb = if c == 3 { b[c] } else { b[c] * b[3] };
                (pa - pb).abs()
            })
        })
        .fold(0f32, f32::max);
    assert!(max <= 1. / 255. + 1e-7, "premultiplied max diff={max}");
}
#[test]
fn clip35_star_intersection_oracle_release_geometry_and_exact_undo() {
    let (mut s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let mut contents = (*before).clone();
    let mask = shape_path(&contents, ids[3]).unwrap();
    contents.remove(ids[3]);
    let unmasked = photocraft_compose::flatten(&contents);
    // Independent pixel oracle: multiply the separately composited three shapes by
    // star coverage. It does not construct a clipping group or reuse its command.
    let coverage = photocraft_vector::path_coverage(&mask, contents.bounds());
    let expected = unmasked.px.iter().zip(coverage).map(|(p, a)| [p[0], p[1], p[2], p[3] * a]).collect::<Vec<_>>();
    let c = Capture::begin(&s, &ids).unwrap();
    let preview = prepare(&s, &c, &Action::MakeClip).unwrap();
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &before));
    assert!(s.journal.is_empty());
    assert_diff(&photocraft_compose::flatten(&preview.document).px, &expected);
    let gid = group(&mut s, &ids);
    assert_eq!(s.active().unwrap().revision, c.revision + 1);
    assert_eq!(s.journal.len(), 1);
    assert_diff(&photocraft_compose::flatten(&s.active().unwrap().doc).px, &expected);
    let clipped = s.active().unwrap().doc.clone();
    let result = command(&mut s, &[gid], "object.clippingMask.release");
    assert_eq!(result["layers"].as_array().unwrap().len(), 4);
    let doc = &s.active().unwrap().doc;
    assert_eq!(doc.layers.len(), 4);
    assert_eq!(&doc.layers[..3], &before.layers[..3]);
    let LayerContent::Shape(released) = &doc.layers[3].content else { panic!("released mask is shape") };
    assert_eq!(released.path, mask);
    assert!(released.fill.is_none() && released.stroke.is_none());
    assert!(doc.layers[3].vector.appearance.as_ref().unwrap().items.is_empty());
    assert_eq!(s.journal.len(), 2);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, clipped);
    assert!(s.undo());
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &before));
    assert!(!s.undo());
}
#[test]
fn clip35_release_uses_edited_authoritative_mask_and_selection_queries_are_unjournaled() {
    let (mut s, ids) = fixture();
    let gid = group(&mut s, &ids);
    let revision = s.active().unwrap().revision;
    let doc = s.active().unwrap().doc.clone();
    command(&mut s, &[ids[1]], "object.clippingMask.editMask");
    assert_eq!(s.active().unwrap().selected_layers(), vec![gid]);
    command(&mut s, &[gid], "object.clippingMask.editContents");
    assert_eq!(s.active().unwrap().selected_layers(), ids[..3]);
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &doc));
    assert_eq!(s.active().unwrap().revision, revision);
    assert_eq!(s.journal.len(), 1);
    let edited = photocraft_vector::shapes::ellipse(20., 20., 50., 50.);
    s.execute("layer.vectorMask.edit", json!({"layer":gid.0,"path":crate::vector_cmds::path_json(&edited)})).unwrap();
    command(&mut s, &[gid], "object.clippingMask.release");
    let LayerContent::Shape(mask) = &s.active().unwrap().doc.layers[3].content else { panic!("mask") };
    assert_eq!(mask.path, edited);
}
#[test]
fn clip35_compound_circle_area_fill_rule_bottom_appearance_release_and_undo() {
    let mut doc = Document::new("Concentric", Size::new(96, 96), ColorMode::Rgb, SampleType::F32);
    let mut ids = Vec::new();
    for (i, r) in [30., 15.].into_iter().enumerate() {
        let sh = ShapeLayer {
            path: photocraft_vector::shapes::ellipse(48. - r, 48. - r, 2. * r, 2. * r),
            fill: Some(Fill::Solid(if i == 0 { Color::BLACK } else { Color::WHITE })),
            ..Default::default()
        };
        let mut l = Layer::new("Circle", LayerContent::Shape(sh));
        if i == 0 {
            l.vector.appearance = Some(Appearance { items: vec![AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(0.2, 0.4, 0.8))))] });
        }
        render_layer(&doc, &mut l).unwrap();
        ids.push(l.id);
        doc.layers.push(l);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    let before = s.active().unwrap().doc.clone();
    // Reversed selection order still uses the document's bottom appearance.
    let cid = LayerId(command(&mut s, &[ids[1], ids[0]], "object.compoundPath.make")["layer"].as_u64().unwrap());
    let compound = s.active().unwrap().doc.clone();
    assert_eq!(compound.layers.len(), 1);
    assert_eq!(compound.layers[0].vector.appearance, before.layers[0].vector.appearance);
    let LayerContent::Shape(sh) = &compound.layers[0].content else { panic!("shape") };
    assert_eq!(sh.path.subpaths.len(), 2);
    let disc = photocraft_pathops::area(&sh.path).unwrap();
    let analytic = std::f64::consts::PI * 30. * 30.;
    assert!((disc - analytic).abs() < analytic * 0.001);
    assert!(photocraft_compose::flatten(&compound).get(48, 48)[3] > 0.99);
    let capture = Capture::begin(&s, &[cid]).unwrap();
    s.execute("object.fillRule", json!({"capture":capture,"rule":FillRule::EvenOdd})).unwrap();
    let ring = s.active().unwrap().doc.clone();
    let LayerContent::Shape(sh) = &ring.layers[0].content else { panic!("shape") };
    let area = photocraft_pathops::area(&sh.path).unwrap();
    assert!((area - std::f64::consts::PI * (30. * 30. - 15. * 15.)).abs() < analytic * 0.001);
    let pixels = photocraft_compose::flatten(&ring);
    assert!(pixels.get(48, 48)[3] <= 1e-6, "ring center alpha must be negligible");
    assert!(pixels.get(70, 48)[3] > 0.99);
    command(&mut s, &[cid], "object.compoundPath.release");
    let released = &s.active().unwrap().doc.layers;
    assert_eq!(released.len(), 2);
    for (l, original) in released.iter().zip(&before.layers) {
        let (LayerContent::Shape(a), LayerContent::Shape(b)) = (&l.content, &original.content) else { panic!("shape") };
        assert_eq!(a.path.subpaths[0].knots, b.path.subpaths[0].knots);
        assert_eq!(l.vector.appearance, before.layers[0].vector.appearance);
        assert!(a.cache.as_ref().unwrap().content_bounds().width() <= 62);
    }
    assert_eq!(s.journal.len(), 3);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, ring);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, compound);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn clip35_draw_inside_ellipse_rectangle_preview_commit_and_repeated_insertion() {
    let (mut s, ids) = fixture();
    let mut doc = (*s.active().unwrap().doc).clone();
    doc.layers.clear();
    let mut ellipse = Layer::new(
        "Ellipse",
        LayerContent::Shape(ShapeLayer {
            path: photocraft_vector::shapes::ellipse(12., 12., 60., 60.),
            fill: Some(Fill::Solid(Color::WHITE)),
            ..Default::default()
        }),
    );
    render_layer(&doc, &mut ellipse).unwrap();
    let target = ellipse.id;
    doc.layers.push(ellipse);
    s.active_mut().unwrap().doc = Arc::new(doc);
    s.select_layer(target).unwrap();
    let before = s.active().unwrap().doc.clone();
    let mut request = DrawRequest {
        capture: Capture::begin(&s, &[target]).unwrap(),
        mode: DrawMode::Inside,
        target: Some(target),
        layer: LayerId::fresh(),
        path: photocraft_vector::shapes::rect(0., 0., 96., 96.),
        fill: Some(Fill::Solid(Color::BLACK)),
        stroke: None,
    };
    let p = preview_draw(&s, &request).unwrap();
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &before));
    let preview = photocraft_compose::flatten(&p.document);
    assert_eq!(preview.get(0, 0)[3], 0.);
    assert_eq!(preview.get(40, 40), [0., 0., 0., 1.]);
    let gid = LayerId(s.execute("object.draw.insert", serde_json::to_value(&request).unwrap()).unwrap()["layer"].as_u64().unwrap());
    assert_diff(&photocraft_compose::flatten(&s.active().unwrap().doc).px, &preview.px);
    assert_eq!(s.active().unwrap().doc.layer(gid).unwrap().children().unwrap()[0], before.layers[0]);
    assert_eq!(s.journal.len(), 1);
    request.capture = Capture::begin(&s, &[gid]).unwrap();
    request.target = Some(gid);
    request.layer = LayerId::fresh();
    s.execute("object.draw.insert", serde_json::to_value(request).unwrap()).unwrap();
    assert_eq!(s.active().unwrap().doc.layer(gid).unwrap().children().unwrap().len(), 3);
    assert_eq!(s.journal.len(), 2);
    assert!(s.undo());
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(ids.len() == 4);
}
#[test]
fn clip35_move_inside_outside_preserves_geometry_and_rejects_cycles() {
    let (mut s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let capture = Capture::begin(&s, &[ids[0], ids[1]]).unwrap();
    s.execute("object.moveInside", json!({"capture":capture,"target":ids[3]})).unwrap();
    let gid = engine_group(&s, ids[0]);
    let clipped = s.active().unwrap().doc.clone();
    assert_eq!(clipped.layer(ids[0]), before.layer(ids[0]));
    let capture = Capture::begin(&s, &[gid]).unwrap();
    assert!(prepare(&s, &capture, &Action::MoveInside(ids[0])).is_err());
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &clipped));
    command(&mut s, &[ids[0], ids[1]], "object.moveOutside");
    assert_eq!(s.active().unwrap().doc.layer(ids[0]), before.layer(ids[0]));
    assert_eq!(s.active().unwrap().doc.layers.iter().map(|l| l.id).collect::<Vec<_>>(), vec![ids[2], gid, ids[0], ids[1]]);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, clipped);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
fn engine_group(s: &Session, id: LayerId) -> LayerId {
    clip_parent(&s.active().unwrap().doc, id).unwrap()
}
#[test]
fn clip35_errors_are_atomic_stale_locked_mixed_parent_nonfinite_and_depth() {
    let (mut s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    for targets in [vec![], vec![ids[0], ids[0]], vec![ids[0], LayerId(u64::MAX)], vec![ids[0]]] {
        let capture = Capture { document: before.id.0, revision: rev, layers: targets };
        assert!(prepare(&s, &capture, &Action::MakeClip).is_err());
        assert!(s.execute("object.clippingMask.make", capture.params().unwrap()).is_err());
        assert!(Arc::ptr_eq(&s.active().unwrap().doc, &before));
    }
    let mut c = Capture::begin(&s, &ids).unwrap();
    c.revision += 1;
    assert!(prepare(&s, &c, &Action::MakeCompound).is_err());
    c.revision = rev;
    c.document += 1;
    assert!(prepare(&s, &c, &Action::MakeClip).is_err());
    let mut doc = (*before).clone();
    let pixels = doc.layers[1].surface().unwrap().clone();
    doc.layers[1].content = LayerContent::Raster(pixels);
    s.active_mut().unwrap().doc = Arc::new(doc);
    let unchanged = s.active().unwrap().doc.clone();
    let capture = Capture::begin(&s, &[ids[0], ids[1]]).unwrap();
    assert!(s.execute("object.fillRule", json!({"capture":capture,"rule":FillRule::EvenOdd})).is_err());
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &unchanged));
    // No large surface is allocated: this request exceeds the aggregate cache budget.
    let mut doc = (*before).clone();
    doc.size = Size::new(10_000, 10_000);
    s.active_mut().unwrap().doc = Arc::new(doc);
    let unchanged = s.active().unwrap().doc.clone();
    let request = DrawRequest {
        capture: Capture::begin(&s, &[ids[0]]).unwrap(),
        mode: DrawMode::Normal,
        target: Some(ids[0]),
        layer: LayerId::fresh(),
        path: photocraft_vector::shapes::rect(0., 0., 10_000., 10_000.),
        fill: Some(Fill::Solid(Color::BLACK)),
        stroke: None,
    };
    assert!(preview_draw(&s, &request).is_err());
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &unchanged));
    let mut doc = (*before).clone();
    doc.layers[1].locks.position = true;
    s.active_mut().unwrap().doc = Arc::new(doc);
    assert!(Capture::begin(&s, &ids).is_err());
    let mut doc = (*before).clone();
    let LayerContent::Shape(sh) = &mut doc.layers[3].content else { panic!("shape") };
    sh.path.subpaths[0].knots[0].anchor.x = f64::NAN;
    s.active_mut().unwrap().doc = Arc::new(doc);
    assert!(Capture::begin(&s, &ids).is_err());
    let mut doc = (*before).clone();
    let nested = doc.layers.remove(0);
    doc.layers.push(Layer::group("Different parent", vec![nested]));
    s.active_mut().unwrap().doc = Arc::new(doc);
    let c = Capture::begin(&s, &ids).unwrap();
    assert!(prepare(&s, &c, &Action::MakeClip).is_err());
    let mut deepest = Layer::group("Last legal parent", before.layers.clone());
    for _ in 1..photocraft_doc::MAX_GROUP_DEPTH {
        deepest = Layer::group("Parent", vec![deepest]);
    }
    let mut doc = (*before).clone();
    doc.layers = vec![deepest];
    s.active_mut().unwrap().doc = Arc::new(doc);
    let c = Capture::begin(&s, &ids).unwrap();
    assert!(prepare(&s, &c, &Action::MakeClip).is_err());
    assert_eq!(s.active().unwrap().revision, rev);
    assert!(s.journal.is_empty());
    assert!(!s.undo());
}
#[test]
fn clip35_text_outline_mask_counter_and_release_unpainted_path() {
    let (mut s, ids) = fixture();
    let mut doc = (*s.active().unwrap().doc).clone();
    let text =
        TextLayer { text: "O".into(), font_family: "Inter".into(), size_pt: 42., transform: photocraft_doc::Affine::translate(15., 60.), ..Default::default() };
    let layer = Layer::new("Text mask", LayerContent::Text(text));
    let tid = layer.id;
    doc.layers.push(layer);
    s.active_mut().unwrap().doc = Arc::new(doc);
    let outlines = shape_path(&s.active().unwrap().doc, tid).unwrap();
    let gid = group(&mut s, &[ids[0], tid]);
    let mask = &s.active().unwrap().doc.layer(gid).unwrap().vector_mask.as_ref().unwrap().path;
    assert_eq!(mask, &outlines);
    assert!(mask.subpaths.len() >= 2, "O retains its counter");
    let values = photocraft_vector::path_coverage(mask, s.active().unwrap().doc.bounds());
    assert!(values.iter().any(|v| *v > 0.99));
    assert!(photocraft_pathops::area(mask).unwrap() > 0.);
    command(&mut s, &[gid], "object.clippingMask.release");
    let released = s.active().unwrap().active_layer.unwrap();
    let LayerContent::Shape(sh) = &s.active().unwrap().doc.layer(released).unwrap().content else { panic!("shape") };
    assert_eq!(sh.path, outlines);
    assert!(sh.fill.is_none());
}
#[test]
fn dsfix_v060_clip35_svg_pdf_psd_exports_keep_native_vector_masks_and_format_roundtrip() {
    let (mut s, ids) = fixture();
    let gid = group(&mut s, &ids);
    let doc = &s.active().unwrap().doc;
    let svg = photocraft_svg::export::export(doc, &ExportScope::Document, &Default::default()).unwrap();
    let xml = String::from_utf8(svg.bytes).unwrap();
    assert!(xml.contains("<clipPath"));
    assert!(xml.contains("clip-path="));
    assert!(!xml.contains("<image"));
    let pdf =
        photocraft_pdf::export::export(doc, &ExportScope::Document, &photocraft_pdf::export::ExportOptions { compress: false, ..Default::default() }).unwrap();
    let text = String::from_utf8_lossy(&pdf.bytes);
    assert!(text.contains("W\n") || text.contains("W*\n"), "native clipping operator");
    assert!(!text.contains("/Subtype /Image"));
    let exported = photocraft_io::export(doc, "psd", &Default::default()).unwrap();
    let imported = photocraft_io::import("clip35.psd", &exported.bytes).unwrap().document;
    let g = imported.walk().into_iter().find(|(_, _, l)| l.is_group() && l.vector_mask.is_some()).unwrap().2;
    let a = &g.vector_mask.as_ref().unwrap().path;
    let b = &doc.layer(gid).unwrap().vector_mask.as_ref().unwrap().path;
    assert_eq!(a.fill_rule, b.fill_rule);
    assert_eq!(a.subpaths.len(), b.subpaths.len());
    assert_eq!(a.subpaths.iter().map(|s| s.knots.len()).collect::<Vec<_>>(), b.subpaths.iter().map(|s| s.knots.len()).collect::<Vec<_>>());
    for (a, b) in a.subpaths.iter().flat_map(|s| &s.knots).zip(b.subpaths.iter().flat_map(|s| &s.knots)) {
        for (a, b) in [a.anchor, a.in_ctrl, a.out_ctrl].into_iter().zip([b.anchor, b.in_ctrl, b.out_ctrl]) {
            assert!((a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4, "PSD 8.24 coordinate quantization");
        }
    }
    assert_diff(&photocraft_compose::flatten(doc).px, &photocraft_compose::flatten(&imported).px);
    let bytes = photocraft_format::save_to_bytes(doc, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    let group = loaded.walk().into_iter().find(|(_, _, l)| l.is_group() && l.vector_mask.is_some()).unwrap().2;
    assert_eq!(group.vector_mask, doc.layer(gid).unwrap().vector_mask);
    assert_diff(&photocraft_compose::flatten(doc).px, &photocraft_compose::flatten(&loaded).px);
}
#[test]
fn clip35_draw_normal_and_behind_insert_exact_sibling_positions_and_settings_default() {
    let (s, ids) = fixture();
    for (mode, index) in [(DrawMode::Normal, 2), (DrawMode::Behind, 1)] {
        let l = Layer::new(
            "New",
            LayerContent::Shape(ShapeLayer {
                path: photocraft_vector::shapes::rect(1., 1., 4., 4.),
                fill: Some(Fill::Solid(Color::BLACK)),
                ..Default::default()
            }),
        );
        let id = l.id;
        let c = Capture::begin(&s, &[ids[1]]).unwrap();
        let p = prepare_insert(&s, &c, mode, Some(ids[1]), l).unwrap();
        assert_eq!(p.document.layers[index].id, id);
    }
    assert_eq!(serde_json::from_str::<DrawMode>("\"normal\"").unwrap(), DrawMode::Normal);
}

#[path = "gpu_tests.rs"]
#[cfg(not(target_arch = "wasm32"))]
mod gpu_tests;

#[test]
fn review_regression_clip35_move_outside_keeps_unselected_locked_sibling_unchanged() {
    let (mut s, ids) = fixture();
    let mut doc = (*s.active().unwrap().doc).clone();
    let mut children = vec![doc.layers.remove(0), doc.layers.remove(0)];
    children[1].locks.all = true;
    let sibling = children[1].clone();
    let group = Layer::group("Parent", children);
    let gid = group.id;
    doc.layers = vec![group];
    s.active_mut().unwrap().doc = Arc::new(doc);
    let before = s.active().unwrap().doc.clone();
    command(&mut s, &[ids[0]], "object.moveOutside");
    assert_eq!(s.active().unwrap().doc.layers.len(), 2);
    assert_eq!(s.active().unwrap().doc.layer(gid).unwrap().children().unwrap(), &[sibling]);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn review_regression_clip35_release_nondefault_mask_density_feather_fails_without_losing_data() {
    for (density, feather) in [(0.5, 0.), (1., 2.)] {
        let (mut s, ids) = fixture();
        let gid = group(&mut s, &ids);
        s.edit("Mask parameters", |doc, _| {
            let mask = doc.layer_mut(gid).unwrap().vector_mask.as_mut().unwrap();
            mask.density = density;
            mask.feather = feather;
            Ok(())
        })
        .unwrap();
        let before = s.active().unwrap().doc.clone();
        let history = s.active().unwrap().history.past_len();
        let journal = s.journal.len();
        let capture = Capture::begin(&s, &[gid]).unwrap();
        assert!(s.execute("object.clippingMask.release", capture.params().unwrap()).is_err());
        assert_eq!(s.active().unwrap().doc, before);
        assert_eq!(s.active().unwrap().history.past_len(), history);
        assert_eq!(s.journal.len(), journal);
    }
}

#[test]
fn dsfix_v111_release_rejects_density_and_feather_without_mutating_source() {
    for (density, feather) in [(0.5, 0.), (1., 2.)] {
        let (mut s, ids) = fixture();
        let gid = group(&mut s, &ids);
        s.edit("mask settings", |doc, _| {
            let mask = doc.layer_mut(gid).unwrap().vector_mask.as_mut().unwrap();
            mask.density = density;
            mask.feather = feather;
            Ok(())
        })
        .unwrap();
        let before = s.active().unwrap().doc.clone();
        let count = s.journal.len();
        let c = Capture::begin(&s, &[gid]).unwrap();
        assert!(prepare(&s, &c, &Action::ReleaseClip).is_err());
        assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc));
        assert_eq!(s.journal.len(), count);
    }
}
