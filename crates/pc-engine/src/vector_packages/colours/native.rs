//! Native acceptance through the production document fields and persistence mirror.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
#[test]
fn colour39_native_old_document_roundtrip_and_remapped_global_links() {
    crate::vector_integration::initialize();
    let access = access().expect("00 native Access must be installed");
    let old = photocraft_format::load_from_bytes(include_bytes!("../../../../pc-format/tests/fixtures/vector-v1.pcraft")).unwrap();
    assert!((access.read)(&old).unwrap().is_empty());
    let (s, book) = tests::fixture();
    let mut doc = (*s.active().unwrap().doc).clone();
    doc.metadata.xmp = None;
    (access.write)(&mut doc, book.clone()).unwrap();
    let bytes = photocraft_format::save_to_bytes(&doc, &Default::default()).unwrap();
    let back = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!((access.read)(&back).unwrap(), book);
    assert_eq!(photocraft_compose::flatten(&back), photocraft_compose::flatten(&doc));
    let remapped = photocraft_format::load_from_bytes_with(&bytes, &photocraft_format::LoadOptions { preserve_ids: false, ..Default::default() }).unwrap();
    let links = (access.read)(&remapped).unwrap();
    assert_ne!(links.links[0].target.layer, book.links[0].target.layer);
    for link in &links.links {
        assert_eq!(color(&remapped, &link.target).unwrap(), links.resolve(link.reference).unwrap());
    }
}
