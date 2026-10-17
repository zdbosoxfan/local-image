//! Called with the actual native-field provider and manifest mirror.
//! No surrogate storage, external fixtures, or skipped adapter can pass this assertion.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
pub fn acceptance(hooks: Hooks) {
    let (mut s, ids) = super::tests::fixture();
    let old = photocraft_format::load_from_bytes(include_bytes!("../../../../pc-format/tests/fixtures/vector-basic.pcraft")).unwrap();
    assert_eq!((hooks.read)(&old).unwrap(), Store::default());
    let before = s.active().unwrap().doc.clone();
    let r = capture(&s, vec![], Operation::New { source: ids[0], name: "Native Ink".into() }).unwrap();
    commit_with_hooks(&mut s, &r, Some(hooks)).unwrap();
    let r = capture(&s, ids[1..].to_vec(), Operation::Apply { style: StyleId(1), add: false }).unwrap();
    commit_with_hooks(&mut s, &r, Some(hooks)).unwrap();
    let document = s.active().unwrap().doc.clone();
    let expected = (hooks.read)(&document).unwrap();
    assert_eq!(expected.graphic_styles.len(), 1);
    assert_eq!(expected.links.len(), 5);
    let bytes = photocraft_format::save_to_bytes(&document, &Default::default()).unwrap();
    let loaded = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!(loaded, *document);
    assert_eq!((hooks.read)(&loaded).unwrap(), expected);
    assert!(s.undo());
    assert_eq!((hooks.read)(&s.active().unwrap().doc).unwrap().links.len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, document);
    s.edit("Local edit", |d, _| {
        d.layer_mut(ids[4]).unwrap().opacity = 0.25;
        d.layer_mut(ids[0]).unwrap().opacity = 0.5;
        Ok(())
    })
    .unwrap();
    let local = s.active().unwrap().doc.layer(ids[4]).unwrap().clone();
    let r = capture(&s, vec![], Operation::Redefine { style: StyleId(1), source: ids[0] }).unwrap();
    commit_with_hooks(&mut s, &r, Some(hooks)).unwrap();
    let doc = &s.active().unwrap().doc;
    assert_eq!(doc.layer(ids[4]).unwrap(), &local);
    for id in &ids[1..4] {
        assert_eq!(doc.layer(*id).unwrap().opacity, 0.5);
    }
    let flat = photocraft_compose::flatten(doc);
    for offset in [46, 92, 138] {
        for y in 8..53 {
            for x in 6..35 {
                assert_eq!(flat.px[y * 240 + x], flat.px[y * 240 + x + offset], "redefined native style pixel {x},{y}");
            }
        }
    }
    let roundtrip = photocraft_format::load_from_bytes(&photocraft_format::save_to_bytes(doc, &Default::default()).unwrap()).unwrap();
    assert_eq!(roundtrip, **doc);
    assert_eq!((hooks.read)(&roundtrip).unwrap(), (hooks.read)(doc).unwrap());
}
#[test]
fn styles38_native_roundtrip_linked_styles_undo_acceptance() {
    crate::vector_integration::initialize();
    acceptance(installed_hooks().expect("actual native provider required"));
}
