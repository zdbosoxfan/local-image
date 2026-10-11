//! Synthetic GPL-3.0-or-later fixtures; independent pixel/attribute oracles.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::vector_design::appearance::StrokeItem;
use photocraft_doc::{
    ColorMode, GradientStyle, Path, SampleType, ShapeLayer, ShapeStroke, Size, Subpath, TextLayer,
    text::{CharStyle, ParagraphRun, ParagraphStyle, TextAlign, TextRun},
};
use photocraft_vector::appearance::NoProjection;

pub(super) fn fixture() -> (Session, Vec<LayerId>) {
    let mut doc = Document::new("Styles 38", Size::new(240, 72), ColorMode::Rgb, SampleType::U8);
    let stack = Appearance {
        items: vec![
            AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(1., 0., 0.)))),
            AppearanceItem::Fill(FillItem {
                paint: Fill::Solid(Color::rgb(0., 1., 0.)),
                opacity: 0.5,
                blend: photocraft_doc::BlendMode::Normal,
                visible: true,
            }),
            AppearanceItem::Stroke(StrokeItem {
                base: ShapeStroke { paint: Fill::Solid(Color::rgb(0., 0., 1.)), width: 2., dashes: vec![3., 2.], dash_offset: 1., ..Default::default() },
                ..Default::default()
            }),
        ],
    };
    let mut ids = Vec::new();
    for i in 0..5 {
        let x = 8. + 46. * f64::from(i);
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, 10.), (x + 24., 10.), (x + 24., 50.), (x, 50.)])]),
            fill: Some(Fill::Solid(Color::WHITE)),
            ..Default::default()
        };
        let appearance = (i == 0).then_some(stack.clone());
        sh.cache = Some(renderer::render_shape(&sh, appearance.as_ref(), &doc.patterns, doc.pixel_format(), doc.bounds()).unwrap());
        let mut layer = Layer::new(format!("Object {i}"), LayerContent::Shape(sh));
        layer.vector.appearance = appearance;
        ids.push(layer.id);
        doc.layers.push(layer);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    (s, ids)
}
fn prepared(s: &Session, store: &Store, targets: &[LayerId], operation: Operation) -> Preview {
    let r = capture(s, targets.to_vec(), operation).unwrap();
    let st = s.active().unwrap();
    prepare(&st.doc, st.revision, store, &r, &NoProjection).unwrap()
}
fn advance(s: &mut Session, p: &Preview) {
    s.edit("fixture publication", |d, _| {
        *d = p.document.clone();
        Ok(())
    })
    .unwrap();
}
fn sample_request(s: &Session, source: LayerId, targets: &[LayerId]) -> Request {
    capture(
        s,
        targets.to_vec(),
        Operation::Copy { sample: Box::new(sample(&s.active().unwrap().doc, source, None).unwrap()), attributes: Attributes::ALL, append: false },
    )
    .unwrap()
}
#[test]
fn styles38_cpu_exact_stack_pixels_detached_and_undo() {
    let (mut s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let r = sample_request(&s, ids[0], &ids[1..3]);
    let p = preview(&s, &r).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    let src = sample(&p.document, ids[0], None).unwrap();
    for id in &ids[1..3] {
        assert_eq!(sample(&p.document, *id, None).unwrap(), src);
    }
    for (i, id) in ids[1..3].iter().enumerate() {
        let source = p.document.layer(ids[0]).unwrap().surface().unwrap();
        let target = p.document.layer(*id).unwrap().surface().unwrap();
        for y in 8..53 {
            for x in 6..35 {
                assert_eq!(source.rgba(x, y), target.rgba(x + 46 * (i as i32 + 1), y));
            }
        }
    }
    // Independent two opaque/half-opacity fill oracle, away from the dashed stroke.
    let c = p.document.layer(ids[1]).unwrap().surface().unwrap().rgba(65, 30);
    for (actual, expected) in c.into_iter().zip([0.5, 0.5, 0., 1.]) {
        assert!((actual - expected).abs() <= 1. / 255.);
    }
    commit(&mut s, &r).unwrap();
    assert_eq!(*s.active().unwrap().doc, p.document);
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
    assert_eq!(*s.active().unwrap().doc, p.document);
    let bytes = photocraft_format::save_to_bytes(&p.document, &Default::default()).unwrap();
    assert_eq!(photocraft_format::load_from_bytes(&bytes).unwrap(), p.document);
}
#[test]
fn styles38_cpu_redefine_three_links_preserves_local_edit() {
    let (mut s, ids) = fixture();
    let p = prepared(&s, &Store::default(), &[], Operation::New { source: ids[0], name: "Ink".into() });
    let mut store = p.store.clone();
    advance(&mut s, &p);
    let p = prepared(&s, &store, &ids[1..], Operation::Apply { style: StyleId(1), add: false });
    store = p.store.clone();
    advance(&mut s, &p);
    assert_eq!(same_style(&s.active().unwrap().doc, &store, ids[1]).unwrap().len(), 5);
    s.edit("Local edit", |d, _| {
        d.layer_mut(ids[4]).unwrap().opacity = 0.25;
        d.layer_mut(ids[0]).unwrap().opacity = 0.5;
        Ok(())
    })
    .unwrap();
    let unlinked = s.active().unwrap().doc.layer(ids[4]).unwrap().clone();
    let p = prepared(&s, &store, &[], Operation::Redefine { style: StyleId(1), source: ids[0] });
    assert_eq!(p.document.layer(ids[4]).unwrap(), &unlinked);
    assert!(!p.store.links.contains_key(&ids[4]));
    assert_eq!(same_style(&p.document, &p.store, ids[1]).unwrap(), ids[..4]);
    for id in &ids[..4] {
        assert_eq!(p.document.layer(*id).unwrap().opacity, 0.5);
    }
    let flat = photocraft_compose::flatten(&p.document);
    for x in [20, 66, 112, 158] {
        let c = flat.px[30 * 240 + x];
        assert!((c[3] - 0.5).abs() <= 1. / 255.);
    }
}
#[test]
fn styles38_cpu_options_append_and_colour_preserve_excluded_attributes() {
    let (s, ids) = fixture();
    let doc = &s.active().unwrap().doc;
    let source = sample(doc, ids[0], None).unwrap();
    let target = sample(doc, ids[1], None).unwrap();
    let p = prepared(
        &s,
        &Store::default(),
        &[ids[1]],
        Operation::Copy { sample: Box::new(source.clone()), attributes: Attributes { stroke: true, ..Attributes::NONE }, append: false },
    );
    let result = sample(&p.document, ids[1], None).unwrap();
    assert_eq!(result.opacity, target.opacity);
    assert_eq!(result.object, target.object);
    assert_eq!(result.appearance.unwrap().items[0], target.appearance.unwrap().items[0]);
    let p = prepared(&s, &Store::default(), &[ids[1]], Operation::Copy { sample: Box::new(source.clone()), attributes: Attributes::ALL, append: true });
    let a = sample(&p.document, ids[1], None).unwrap().appearance.unwrap();
    assert_eq!(a.items.len(), 4);
    assert_eq!(&a.items[1..], source.appearance.unwrap().items);
    let p = prepared(&s, &Store::default(), &[ids[0]], Operation::Color { color: Color::WHITE, stroke: false });
    let a = sample(&p.document, ids[0], None).unwrap().appearance.unwrap();
    assert_eq!(a.items.len(), 3);
    assert_eq!(a.items[0], sample(doc, ids[0], None).unwrap().appearance.unwrap().items[0]);
    assert_eq!(a.items[2], sample(doc, ids[0], None).unwrap().appearance.unwrap().items[2]);
}
#[test]
fn styles38_cpu_invalid_last_locked_stale_nan_and_budget_are_atomic() {
    let (mut s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let r = sample_request(&s, ids[0], &[ids[1], LayerId(u64::MAX)]);
    assert!(commit(&mut s, &r).is_err());
    let r = sample_request(&s, ids[0], &[ids[1], ids[1]]);
    assert!(commit(&mut s, &r).is_err());
    let mut r = sample_request(&s, ids[0], &ids[1..]);
    r.revision += 1;
    assert!(commit(&mut s, &r).is_err());
    let mut r = sample_request(&s, ids[0], &ids[1..]);
    if let Operation::Copy { sample, .. } = &mut r.operation {
        sample.opacity = f32::NAN;
    }
    assert!(commit(&mut s, &r).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().history.past_len(), 0);
    Arc::make_mut(&mut s.active_mut().unwrap().doc).layer_mut(ids[4]).unwrap().locks.all = true;
    let r = sample_request(&s, ids[0], &ids[1..]);
    assert!(commit(&mut s, &r).is_err());
    assert_eq!(s.active().unwrap().history.past_len(), 0);
    let doc = Arc::make_mut(&mut s.active_mut().unwrap().doc);
    doc.size = Size::new(1_000_000, 1_000_000);
    if let LayerContent::Shape(sh) = &mut doc.layer_mut(ids[1]).unwrap().content {
        sh.path = Path::new(vec![Subpath::polygon(&[(0., 0.), (1., 0.), (1., 1_000_000.), (0., 1_000_000.)])]);
    }
    let mut r = sample_request(&s, ids[0], &[ids[1]]);
    if let Operation::Copy { sample, .. } = &mut r.operation {
        // Isolate cache storage from the separate dashed-stroke geometry budget.
        sample.appearance.as_mut().unwrap().items.retain(|i| matches!(i, AppearanceItem::Fill(_)));
    }
    let failure = preview(&s, &r).unwrap_err().to_string();
    assert!(failure.contains("512 MiB"), "{failure}");
}
#[test]
fn styles38_cpu_raster_and_gradient_intermediate_independent_oracles() {
    let (mut s, ids) = fixture();
    let doc = Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let mut raster = Layer::raster("pixel", doc.pixel_format());
    if let LayerContent::Raster(surface) = &mut raster.content {
        surface.write_pixel(3, 5, &[0.2, 0.4, 0.8, 1.]);
    }
    let id = raster.id;
    doc.layers.push(raster);
    let c = color_at(doc, id, 3.9, 5.1, false).unwrap();
    for (v, e) in c.to_rgb().into_iter().zip([0.2, 0.4, 0.8]) {
        assert!((v - e).abs() < 1. / 255.);
    }
    let mut gradient = Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 0., 1., GradientStyle::Linear, false);
    if let Fill::Gradient { midpoints, .. } = &mut gradient {
        *midpoints = vec![0.25];
    }
    doc.layer_mut(ids[0]).unwrap().vector.appearance = Some(Appearance { items: vec![AppearanceItem::Fill(FillItem::new(gradient))] });
    // Pixel-centre convention: x=14.5 lies six pixels into the 24-pixel ramp.
    let c = color_at(doc, ids[0], 14.5, 30.5, false).unwrap();
    assert!((c.c[0] - 0.5).abs() < 0.001, "midpoint {c:?}");
}
#[test]
fn styles38_cpu_text_sample_character_paragraph_and_content_preserved() {
    let (mut s, ids) = fixture();
    let doc = Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let text = TextLayer {
        text: "aβ\nc".into(),
        transform: photocraft_geom::Affine::translate(10., 40.),
        runs: vec![TextRun { len: 1, style: CharStyle::default() }, TextRun { len: 4, style: CharStyle { size_pt: 18., tracking: 35., ..Default::default() } }],
        paragraphs: vec![ParagraphRun { len: 5, style: ParagraphStyle { align: TextAlign::Center, ..Default::default() } }],
        ..Default::default()
    };
    let layer = Layer::new("text", LayerContent::Text(text));
    let src = layer.id;
    doc.layers.push(layer);
    let target = Layer::new(
        "target",
        LayerContent::Text(TextLayer { text: "different".into(), transform: photocraft_geom::Affine::translate(100., 40.), ..Default::default() }),
    );
    let id = target.id;
    doc.layers.push(target);
    let sampled = sample(doc, src, Some(1)).unwrap();
    assert_eq!(sampled.character.as_ref().unwrap().size_pt, 18.);
    assert!(sample(doc, src, Some(2)).is_err());
    let p = prepared(
        &s,
        &Store::default(),
        &[id],
        Operation::Copy { sample: Box::new(sampled), attributes: Attributes { character: true, paragraph: true, ..Attributes::NONE }, append: false },
    );
    let LayerContent::Text(t) = &p.document.layer(id).unwrap().content else { panic!() };
    assert_eq!(t.text, "different");
    assert_eq!(t.runs[0].style.size_pt, 18.);
    assert_eq!(t.paragraphs[0].style.align, TextAlign::Center);
    assert!(t.cache.is_some());
    assert!(p.document.layer(ids[0]).unwrap().vector.appearance.is_some());
}
#[test]
fn styles38_cpu_library_rename_duplicate_delete_and_old_defaults() {
    let (mut s, ids) = fixture();
    let mut p = prepared(&s, &Store::default(), &[], Operation::New { source: ids[0], name: "Original".into() });
    advance(&mut s, &p);
    p = prepared(&s, &p.store, &[], Operation::Duplicate { style: StyleId(1), name: "Copy".into() });
    assert_eq!(p.store.graphic_styles[1].id, StyleId(2));
    advance(&mut s, &p);
    p = prepared(&s, &p.store, &[], Operation::Rename { style: StyleId(1), name: "Renamed".into() });
    assert_eq!(p.store.links[&ids[0]].style, StyleId(1));
    advance(&mut s, &p);
    let old = s.active().unwrap().doc.layer(ids[0]).unwrap().clone();
    p = prepared(&s, &p.store, &[], Operation::Delete { style: StyleId(1) });
    assert_eq!(p.document.layer(ids[0]).unwrap(), &old);
    assert!(p.store.links.is_empty());
    assert_eq!(serde_json::from_str::<Store>("{}").unwrap(), Store::default());
    let old = photocraft_format::load_from_bytes(include_bytes!("../../../../pc-format/tests/fixtures/vector-v1.pcraft")).unwrap();
    assert_eq!(store(&old).unwrap(), Store::default());
    assert_eq!(serde_json::from_str::<Store>(&serde_json::to_string(&p.store).unwrap()).unwrap(), p.store);
}
// XMP is ONLY a synthetic test provider to verify the Session history transaction. It is not
// a native styles adapter and does not count as native-format acceptance.
pub(super) fn fixture_hooks() -> Hooks {
    Hooks {
        read: |d| d.metadata.xmp.as_deref().map_or_else(|| Ok(Store::default()), |s| serde_json::from_str(s).map_err(|e| error(e.to_string()))),
        write: |d, s| {
            d.metadata.xmp = Some(serde_json::to_string(s).map_err(|e| error(e.to_string()))?);
            Ok(())
        },
    }
}
#[test]
fn styles38_cpu_provider_transaction_undo_redo_and_failure() {
    let (mut s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let r = capture(&s, vec![], Operation::New { source: ids[0], name: "Style".into() }).unwrap();
    let h = fixture_hooks();
    commit_with_hooks(&mut s, &r, Some(h)).unwrap();
    let after = s.active().unwrap().doc.clone();
    assert_eq!((h.read)(&after).unwrap().graphic_styles.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, after);
    let r = capture(&s, vec![ids[1]], Operation::Apply { style: StyleId(1), add: false }).unwrap();
    let fail = Hooks { write: |_, _| Err(error("provider rejected")), ..h };
    assert!(commit_with_hooks(&mut s, &r, Some(fail)).is_err());
    assert_eq!(s.active().unwrap().doc, after);
    assert!(commit_with_hooks(&mut s, &r, None).is_err());
}

#[test]
fn styles38_cpu_fill_replacement_keeps_stroke_above_fills_and_add_preserves_opacity() {
    let (mut s, ids) = fixture();
    s.edit("target fixture", |d, _| {
        let target = d.layer_mut(ids[1]).unwrap();
        target.opacity = 0.3;
        target.vector.appearance =
            Some(Appearance { items: vec![AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::WHITE))), AppearanceItem::Stroke(StrokeItem::default())] });
        Ok(())
    })
    .unwrap();
    let source = sample(&s.active().unwrap().doc, ids[0], None).unwrap();
    let p = prepared(
        &s,
        &Store::default(),
        &[ids[1]],
        Operation::Copy { sample: Box::new(source.clone()), attributes: Attributes { fill: true, ..Attributes::NONE }, append: false },
    );
    let stack = sample(&p.document, ids[1], None).unwrap().appearance.unwrap();
    assert!(matches!(stack.items[0], AppearanceItem::Fill(_)));
    assert!(matches!(stack.items[1], AppearanceItem::Fill(_)));
    assert!(matches!(stack.items[2], AppearanceItem::Stroke(_)));
    let p = prepared(&s, &Store::default(), &[ids[1]], Operation::Copy { sample: Box::new(source), attributes: Attributes::ALL, append: true });
    assert_eq!(p.document.layer(ids[1]).unwrap().opacity, 0.3);
}
#[test]
fn styles38_cpu_text_second_run_local_edit_breaks_style_key() {
    let (mut s, _) = fixture();
    let doc = Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let layer = Layer::new(
        "Runs",
        LayerContent::Text(TextLayer {
            text: "ab".into(),
            runs: vec![TextRun { len: 1, style: CharStyle::default() }, TextRun { len: 1, style: CharStyle::default() }],
            ..Default::default()
        }),
    );
    let id = layer.id;
    doc.layers.push(layer);
    let baseline = link(doc, id, StyleId(7)).unwrap();
    assert_eq!(style_key(doc.layer(id).unwrap(), Some(&baseline)).unwrap(), Some("7".into()));
    if let LayerContent::Text(t) = &mut doc.layer_mut(id).unwrap().content {
        t.runs[1].style.tracking = 77.;
    }
    assert!(!in_sync(doc, id, &baseline));
    assert_eq!(style_key(doc.layer(id).unwrap(), Some(&baseline)).unwrap(), None);
}

#[test]
fn styles38_cpu_full_text_sampling_preserves_native_faux_bold_pixels() {
    let (mut s, _) = fixture();
    let doc = Arc::make_mut(&mut s.active_mut().unwrap().doc);
    let mut source = TextLayer {
        text: "Ink".into(),
        transform: photocraft_geom::Affine::translate(10., 40.),
        runs: vec![TextRun { len: 3, style: CharStyle { size_pt: 18., faux_bold: true, color: Color::rgb(0.8, 0.1, 0.2), ..Default::default() } }],
        ..Default::default()
    };
    crate::type_cmds::refresh(doc, &mut source);
    let source = Layer::new("Native source", LayerContent::Text(source));
    let source_id = source.id;
    doc.layers.push(source);
    let target = Layer::new(
        "Text target",
        LayerContent::Text(TextLayer { text: "Ink".into(), transform: photocraft_geom::Affine::translate(100., 40.), ..Default::default() }),
    );
    let id = target.id;
    doc.layers.push(target);
    let p = prepared(
        &s,
        &Store::default(),
        &[id],
        Operation::Copy { sample: Box::new(sample(&s.active().unwrap().doc, source_id, None).unwrap()), attributes: Attributes::ALL, append: false },
    );
    let source = p.document.layer(source_id).unwrap().surface().unwrap();
    let target = p.document.layer(id).unwrap().surface().unwrap();
    let mut opaque = 0;
    for y in 5..65 {
        for x in 5..60 {
            let a = source.rgba(x, y);
            let b = target.rgba(x + 90, y);
            for channel in 0..4 {
                let premultiply = |c: [f32; 4]| if channel == 3 { c[3] } else { c[channel] * c[3] };
                assert!((premultiply(a) - premultiply(b)).abs() <= 1. / 255., "text pixel {x},{y}: {a:?} vs {b:?}");
            }
            if a[3] > 0.5 {
                opaque += 1;
            }
        }
    }
    assert!(opaque > 20, "native text oracle must contain actual glyph ink");
}

#[test]
fn styles38_cpu_style_transfer_retains_target_live_corner_geometry() {
    use photocraft_doc::vector_design::drawing::{Corner, CornerKind, KnotCorner};
    let (mut s, ids) = fixture();
    s.edit("rounded target", |doc, _| {
        doc.layer_mut(ids[1]).unwrap().vector.corners = vec![KnotCorner { subpath: 0, knot: 0, corner: Corner { radius: 8., kind: CornerKind::Round } }];
        Ok(())
    })
    .unwrap();
    let original = s.active().unwrap().doc.layer(ids[1]).unwrap().clone();
    let source = sample(&s.active().unwrap().doc, ids[0], None).unwrap();
    let p = prepared(
        &s,
        &Store::default(),
        &[ids[1]],
        Operation::Copy { sample: Box::new(source), attributes: Attributes { fill: true, ..Attributes::NONE }, append: false },
    );
    let target = p.document.layer(ids[1]).unwrap();
    assert_eq!(target.vector.corners, original.vector.corners);
    let (LayerContent::Shape(before), LayerContent::Shape(after)) = (&original.content, &target.content) else { panic!() };
    assert_eq!(after.path, before.path);
    // Independent quarter-circle oracle: (54.5,10.5) is outside radius 8 centred (62,18).
    let surface = target.surface().unwrap();
    assert_eq!(surface.rgba(54, 10)[3], 0.);
    assert_eq!(surface.rgba(66, 30)[3], 1.);
}

#[test]
fn review_followup_style_rejects_raw_effect_budget_before_serialization() {
    let (s, ids) = fixture();
    let mut attributes = sample(&s.active().unwrap().doc, ids[0], None).unwrap();
    attributes.effects.psd_raw = Some(vec![0; (1 << 20) + 1]);
    assert_eq!(validate_sample(&attributes).unwrap_err().to_string(), "style raw FX budget");
}
