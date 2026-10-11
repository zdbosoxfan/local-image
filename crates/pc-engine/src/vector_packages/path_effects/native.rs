//! Requires 00's real VectorData field; no test persistence substitute passes this gate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
#[test]
#[ignore = "integration: package 00 must install native VectorData.effects; speed rules defer native acceptance"]
fn path_effects45_native_old_document_editable_roundtrip_and_one_journal() {
    let a = access().expect("native path-effect field provider required");
    let old = photocraft_format::load_from_bytes(include_bytes!("../../../../pc-format/tests/fixtures/vector-v1.pcraft")).unwrap();
    for (_, _, l) in old.walk() {
        assert!((a.read)(l).unwrap().is_empty());
    }
    let (mut s, ids) = tests::fixture();
    let original = s.active().unwrap().doc.clone();
    let mut gesture = Gesture::begin(&s, &ids, a).unwrap();
    gesture
        .update(&s, |e| {
            // Persistence covers all nine variants in one valid editable stack.
            // Individual nonidentity geometry is covered by the separate GPU gate.
            e.extend(EffectKind::presets().into_iter().map(|mut kind| {
                match &mut kind {
                    EffectKind::Roughen { size, .. } | EffectKind::ZigZag { size, .. } => *size = 0.,
                    EffectKind::PuckerBloat { amount } => *amount = 0.,
                    EffectKind::Twist { angle } => *angle = 0.,
                    EffectKind::OffsetPath { distance, .. } => *distance = 0.,
                    EffectKind::Transform { scale, move_by, angle, copies, .. } => {
                        *scale = [100., 100.];
                        *move_by = [0., 0.];
                        *angle = 0.;
                        *copies = 0;
                    }
                    _ => {}
                }
                PathEffect::new(kind)
            }));
            Ok(())
        })
        .unwrap();
    commit(&mut s, &gesture.request).expect("registered journaled command required");
    assert_eq!(s.journal.len(), 1);
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    let saved = s.active().unwrap().doc.clone();
    let bytes = photocraft_format::save_to_bytes(&saved, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    for (_, _, l) in loaded.walk() {
        if matches!(l.content, LayerContent::Shape(_)) {
            assert_eq!((a.read)(l).unwrap().len(), 9);
        }
    }
    let mut cache = ProjectionCache::default();
    let shown = project_document(&loaded, a, &mut cache).unwrap();
    let expected = project_document(&saved, a, &mut ProjectionCache::default()).unwrap();
    assert_eq!(photocraft_compose::flatten(&shown).px, photocraft_compose::flatten(&expected).px);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, original);
}
