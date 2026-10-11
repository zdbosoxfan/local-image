#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::vector_design::appearance::{Appearance, FillItem};
use photocraft_doc::{Affine, Color, ColorMode, Path, SampleType, ShapeLayer, ShapeStroke, Size, Subpath, SymbolDef, SymbolInstance};
use std::sync::Arc;
// Test-only storage models 00's read/write contract inside the undoable document. XMP is
// deliberately NOT a production adapter or a claim of native palette integration.
pub(crate) fn test_access() -> Access {
    Access {
        read: |d| serde_json::from_str(d.metadata.xmp.as_deref().unwrap_or("{}")).map_err(|e| bad(e.to_string())),
        write: |d, b| {
            d.metadata.xmp = Some(serde_json::to_string(&b).map_err(|e| bad(e.to_string()))?);
            Ok(())
        },
    }
}
pub(crate) fn fixture() -> (Session, SwatchBook) {
    let mut d = Document::new("Global colour acceptance", Size::new(96, 40), ColorMode::Rgb, SampleType::U8);
    let base = Color::rgb(0.2, 0.4, 0.6);
    let mut book = SwatchBook::default();
    book.swatches.push(Swatch { id: SwatchId(1), name: "Brand".into(), paint: Fill::Solid(base), global: true, spot: false, group: None, stops: vec![] });
    for i in 0..5 {
        let x = (i * 16 + 2) as f64;
        let mut sh = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, 4.), (x + 10., 4.), (x + 10., 14.), (x, 14.)])]),
            fill: Some(Fill::Solid(base)),
            ..Default::default()
        };
        let mut slot = PaintSlot::Fill;
        let mut stop = None;
        if i == 1 || i == 4 {
            sh.fill = None;
            sh.stroke = Some(ShapeStroke { width: 2., paint: Fill::Solid(base), ..Default::default() });
            slot = PaintSlot::Stroke;
        }
        if i == 2 {
            sh.fill = Some(Fill::gradient(vec![(0., base), (1., Color::WHITE)], 0., 100., photocraft_doc::GradientStyle::Linear, false));
            stop = Some(0);
        }
        let mut layer = Layer::new(format!("Object {i}"), LayerContent::Shape(sh));
        if i == 3 {
            layer.vector.appearance = Some(Appearance { items: vec![AppearanceItem::Fill(FillItem::new(Fill::Solid(base)))] });
            slot = PaintSlot::Appearance(0);
        }
        if let LayerContent::Shape(sh) = &mut layer.content {
            sh.cache =
                Some(CompiledAppearance::new(sh, layer.vector.appearance.as_ref(), &[], d.bounds()).unwrap().render(d.pixel_format(), d.bounds()).unwrap());
        }
        let symbol = (i == 4).then_some(SymbolId(7));
        book.links.push(PaintLink {
            target: PaintTarget { symbol, layer: layer.id, paint: slot, stop },
            reference: SwatchRef { swatch: SwatchId(1), tint: if i == 0 { 50. } else { 100. } },
        });
        if i == 4 {
            d.symbols.push(SymbolDef { id: SymbolId(7), name: "Master".into(), layers: vec![layer] });
        } else {
            d.layers.push(layer);
        }
    }
    let instance = SymbolInstance { symbol: SymbolId(7), transform: Affine::IDENTITY, cache: None };
    let mut instance = instance;
    instance.cache = Some(super::super::symbols::render_instance(&d, &instance).unwrap());
    d.layers.push(Layer::new("Instance", LayerContent::Symbol(instance)));
    // Set the sole tint use to its independent arithmetic value before building initial pixels.
    let t = &book.links[0].target;
    set_color(&mut d, t, Color::rgb(0.6, 0.7, 0.8)).unwrap();
    let prepared = finish(d, book.clone(), std::slice::from_ref(t), &super::super::styles::Projection).unwrap();
    let mut d = prepared.document;
    (test_access().write)(&mut d, book.clone()).unwrap();
    let mut s = Session::new();
    s.add_document(d, None);
    (s, book)
}
fn edit(s: &Session, book: &SwatchBook, to: Color) -> Request {
    let mut w = book.swatches[0].clone();
    w.paint = Fill::Solid(to);
    Request { capture: Capture::begin(s).unwrap(), action: Action::Edit { swatch: w } }
}
#[test]
fn colour39_global_five_objects_master_stroke_stop_tint_preview_undo_and_delete() {
    let (mut s, book) = fixture();
    let before = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    let request = edit(&s, &book, Color::rgb(0.8, 0.2, 0.4));
    let p = prepare(&s, test_access(), &request).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, rev);
    assert!(!s.active().unwrap().history.can_undo());
    for (i, l) in book.links.iter().enumerate() {
        assert_eq!(color(&p.document, &l.target).unwrap(), if i == 0 { Color::rgb(0.9, 0.6, 0.7) } else { Color::rgb(0.8, 0.2, 0.4) });
    }
    let cpu = photocraft_compose::flatten(&p.document);
    // Interior solid and appearance samples: independent expected 8-bit arithmetic.
    assert_eq!(cpu.px[8 * 96 + 6].map(|x| (x * 255. + 0.5) as u8), [230, 153, 179, 255]);
    assert_eq!(cpu.px[8 * 96 + 54].map(|x| (x * 255. + 0.5) as u8), [204, 51, 102, 255]);
    let h = s.active().unwrap().history.past_len();
    commit(&mut s, test_access(), &request).unwrap();
    assert_eq!(*s.active().unwrap().doc, p.document);
    assert_eq!(s.active().unwrap().history.past_len(), h + 1);
    let after = s.active().unwrap().doc.clone();
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, after);
    let delete = Request { capture: Capture::begin(&s).unwrap(), action: Action::Delete { ids: vec![SwatchId(1)] } };
    let p = prepare(&s, test_access(), &delete).unwrap();
    assert!(p.book.links.is_empty());
    assert!(p.book.swatches.is_empty());
    assert_eq!(photocraft_compose::flatten(&p.document), cpu);
    commit(&mut s, test_access(), &delete).unwrap();
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, after);
}
#[test]
fn colour39_global_late_locked_target_and_staleness_are_atomic() {
    let (mut s, book) = fixture();
    let request = edit(&s, &book, Color::BLACK);
    Arc::make_mut(&mut s.active_mut().unwrap().doc).symbols[0].layers[0].locks.all = true;
    let before = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    assert!(commit(&mut s, test_access(), &request).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, rev);
    assert!(!s.undo());
    s.edit("Unrelated", |d, _| {
        d.name = "Changed".into();
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    assert!(prepare(&s, test_access(), &request).is_err());
    assert_eq!(s.active().unwrap().doc, before);
}
#[test]
fn colour39_groups_add_used_merge_off_and_import_replacement() {
    let (mut s, book) = fixture();
    let request = Request {
        capture: Capture::begin(&s).unwrap(),
        action: Action::Group { group: SwatchGroup { id: 2, name: "Brand Group".into() }, ids: vec![SwatchId(1)] },
    };
    commit(&mut s, test_access(), &request).unwrap();
    let request = Request { capture: Capture::begin(&s).unwrap(), action: Action::AddUsed { global: false, group: Some(2) } };
    commit(&mut s, test_access(), &request).unwrap();
    let stored = (test_access().read)(&s.active().unwrap().doc).unwrap();
    assert!(stored.swatches.len() >= 3);
    let white = stored.swatches.iter().find(|w| w.paint == Fill::Solid(Color::WHITE)).unwrap();
    let local_stop = prepare(
        &s,
        test_access(),
        &Request { capture: Capture::begin(&s).unwrap(), action: Action::AssignPaint { targets: vec![book.links[2].target.clone()], swatch: white.id } },
    )
    .unwrap();
    assert_eq!(color(&local_stop.document, &book.links[2].target).unwrap(), Color::WHITE);
    assert!(!local_stop.book.links.iter().any(|l| l.target == book.links[2].target));
    let delete_group = Request { capture: Capture::begin(&s).unwrap(), action: Action::DeleteGroup { id: 2 } };
    let deleted = prepare(&s, test_access(), &delete_group).unwrap();
    assert!(deleted.book.groups.is_empty());
    assert!(deleted.book.swatches.is_empty());
    assert_eq!(photocraft_compose::flatten(&deleted.document), photocraft_compose::flatten(&s.active().unwrap().doc));
    let mut w = book.swatches[0].clone();
    w.global = false;
    let before = photocraft_compose::flatten(&s.active().unwrap().doc);
    let p = prepare(&s, test_access(), &Request { capture: Capture::begin(&s).unwrap(), action: Action::Edit { swatch: w } }).unwrap();
    assert!(p.book.links.is_empty());
    assert_eq!(photocraft_compose::flatten(&p.document), before);
    let p =
        prepare(&s, test_access(), &Request { capture: Capture::begin(&s).unwrap(), action: Action::Import { palette: SwatchBook::default(), replace: true } })
            .unwrap();
    assert!(p.book.is_empty());
    assert_eq!(photocraft_compose::flatten(&p.document), before);
    let mut w = book.swatches[0].clone();
    w.id = SwatchId(99);
    w.name = "Replacement".into();
    w.paint = Fill::Solid(Color::BLACK);
    let request = Request { capture: Capture::begin(&s).unwrap(), action: Action::Add { swatch: w } };
    commit(&mut s, test_access(), &request).unwrap();
    let p = prepare(&s, test_access(), &Request { capture: Capture::begin(&s).unwrap(), action: Action::Merge { into: SwatchId(99), ids: vec![SwatchId(1)] } })
        .unwrap();
    assert!(p.book.swatch(SwatchId(1)).is_none());
    assert_eq!(color(&p.document, &book.links[0].target).unwrap(), Color::rgb(0.5, 0.5, 0.5));
}
#[test]
fn colour39_palette_invalid_numbers_and_last_assignment_reject() {
    let (s, book) = fixture();
    let before = s.active().unwrap().doc.clone();
    let mut whole = book.links[2].target.clone();
    whole.stop = None;
    let solid = prepare(
        &s,
        test_access(),
        &Request { capture: Capture::begin(&s).unwrap(), action: Action::Assign { targets: vec![whole.clone()], swatch: SwatchId(1), tint: 50. } },
    )
    .unwrap();
    assert_eq!(color(&solid.document, &whole).unwrap(), Color::rgb(0.6, 0.7, 0.8));
    assert!(!solid.book.links.iter().any(|l| l.target.layer == whole.layer && l.target.stop.is_some()));
    let mut targets = book.links.iter().map(|l| l.target.clone()).collect::<Vec<_>>();
    targets.last_mut().unwrap().stop = Some(1000);
    let r = Request { capture: Capture::begin(&s).unwrap(), action: Action::Assign { targets, swatch: SwatchId(1), tint: 50. } };
    assert!(prepare(&s, test_access(), &r).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    let mut w = book.swatches[0].clone();
    if let Fill::Solid(c) = &mut w.paint {
        c.c[0] = f32::NAN;
    }
    assert!(prepare(&s, test_access(), &Request { capture: Capture::begin(&s).unwrap(), action: Action::Edit { swatch: w } }).is_err());
}
#[cfg(test)]
#[path = "gpu.rs"]
mod gpu;
