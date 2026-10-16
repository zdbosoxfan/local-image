#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::vector_design::appearance::{AppearanceItem, FillItem, StrokeItem};
use photocraft_doc::{Color, ColorMode, Fill, Layer, Path, SampleType, ShapeLayer, Size, Subpath};

pub(super) fn setup() -> (Session, Vec<LayerId>) {
    let mut doc = Document::new("Appearance", Size::new(96, 96), ColorMode::Rgb, SampleType::U8);
    let mut ids = Vec::new();
    for x in [12., 52.] {
        let mut shape = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, 20.), (x + 24., 20.), (x + 24., 60.), (x, 60.)])]),
            fill: Some(Fill::Solid(Color::WHITE)),
            ..Default::default()
        };
        shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
        let l = Layer::new("Shape", LayerContent::Shape(shape));
        ids.push(l.id);
        doc.layers.push(l);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    (s, ids)
}
fn add(g: &mut Gesture) {
    g.update(|a| {
        a.items.push(AppearanceItem::Stroke(StrokeItem::default()));
        Ok(())
    })
    .unwrap();
}
#[test]
fn appearance_transaction_detached_native_roundtrip_and_single_undo() {
    let (mut s, ids) = setup();
    let before = s.active().unwrap().doc.clone();
    let mut g = Gesture::begin(&s, &ids).unwrap();
    add(&mut g);
    let shown = preview_document(&s, g.request()).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.journal.is_empty());
    assert!(shown.layers.iter().all(|l| l.vector.appearance.is_some()));
    g.commit(&mut s).unwrap();
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    assert_eq!(*s.active().unwrap().doc, shown);
    let bytes = photocraft_format::save_to_bytes(&shown, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!(loaded, shown);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
    assert_eq!(*s.active().unwrap().doc, shown);
}
#[test]
fn appearance_invalid_last_target_duplicate_locked_stale_and_missing_pattern_are_atomic() {
    let (mut s, ids) = setup();
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let mut g = Gesture::begin(&s, &ids).unwrap();
    add(&mut g);
    let valid = g.request().clone();
    let mut invalid = valid.clone();
    invalid.edits[1].layer = LayerId(u64::MAX);
    assert!(s.execute("shape.appearance.set", json!(invalid)).is_err());
    let mut duplicate = valid.clone();
    duplicate.edits.push(duplicate.edits[0].clone());
    assert!(s.execute("shape.appearance.set", json!(duplicate)).is_err());
    let mut invalid = valid.clone();
    invalid.revision += 1;
    assert!(s.execute("shape.appearance.set", json!(invalid)).is_err());
    let mut invalid = valid.clone();
    invalid.document += 1;
    assert!(s.execute("shape.appearance.set", json!(invalid)).is_err());
    let mut invalid = valid.clone();
    invalid.edits[1].appearance.items.push(AppearanceItem::Fill(FillItem::new(Fill::Pattern {
        name: "missing".into(),
        id: String::new(),
        scale: 1.,
        angle: 0.,
        link: true,
        phase: (0., 0.),
    })));
    assert!(s.execute("shape.appearance.set", json!(invalid)).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, revision);
    assert!(s.journal.is_empty());
    assert!(!s.undo());
    let mut locked = (*before).clone();
    locked.layer_mut(ids[1]).unwrap().locks.pixels = true;
    s.active_mut().unwrap().doc = std::sync::Arc::new(locked);
    assert!(s.execute("shape.appearance.set", json!(valid)).is_err());
    assert!(s.journal.is_empty());
}
#[test]
fn appearance_explicit_empty_preview_and_old_native_load() {
    let (mut s, ids) = setup();
    let mut g = Gesture::begin(&s, &ids).unwrap();
    g.update(|a| {
        a.items.clear();
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().revision;
    s.execute("shape.appearance.preview", json!(g.request())).unwrap();
    assert_eq!(s.active().unwrap().revision, before);
    assert!(s.journal.is_empty());
    g.commit(&mut s).unwrap();
    for l in &s.active().unwrap().doc.layers {
        assert_eq!(l.surface().unwrap().tile_count(), 0);
        assert_eq!(l.vector.appearance, Some(Appearance::default()));
    }
    let old = photocraft_format::load_from_bytes(include_bytes!("../../../../pc-format/tests/fixtures/vector-basic.pcraft")).unwrap();
    assert!(old.layers.iter().all(|l| l.vector.appearance.is_none()));
}
#[test]
fn appearance_gesture_update_rejects_nan_without_changing_any_target() {
    let (s, ids) = setup();
    let mut g = Gesture::begin(&s, &ids).unwrap();
    let before = g.request().clone();
    assert!(
        g.update(|a| {
            if let AppearanceItem::Fill(f) = &mut a.items[0] {
                f.opacity = f32::NAN;
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(g.request(), &before);
}
#[path = "gpu.rs"]
mod gpu;

#[test]
fn appearance_prepared_budget_counts_padded_tiles_before_render() {
    let (mut s, ids) = setup();
    s.edit("large narrow fixture", |doc, _| {
        doc.size = Size::new(1_000_000, 1_000_000);
        if let LayerContent::Shape(sh) = &mut doc.layer_mut(ids[0]).unwrap().content {
            sh.path = Path::new(vec![Subpath::polygon(&[(0., 0.), (1., 0.), (1., 1_000_000.), (0., 1_000_000.)])]);
        }
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    let journal = s.journal.len();
    let g = Gesture::begin(&s, &[ids[0]]).unwrap();
    assert!(preview_document(&s, g.request()).unwrap_err().to_string().contains("512 MiB"));
    assert!(g.commit(&mut s).unwrap_err().to_string().contains("512 MiB"));
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.journal.len(), journal);
}
