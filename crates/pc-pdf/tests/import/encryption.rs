//! Synthetic permission-only PDFs generated independently; see fixture provenance.
use super::*;
const ART: &str = "1 0 0 rg 8 8 48 48 re f 0 0 1 rg 24 24 16 16 re f";
#[test]
fn pdf_empty_password_standard_rc4_aes128_matches_plaintext() {
    let expected = render(&fixture::page(ART, "", "", &[]));
    for bytes in [
        include_bytes!("../../fixtures/import/encryption/rc4-r2.pdf").as_slice(),
        include_bytes!("../../fixtures/import/encryption/rc4-r3.pdf"),
        include_bytes!("../../fixtures/import/encryption/rc4-r4.pdf"),
        include_bytes!("../../fixtures/import/encryption/aes128-r4.pdf"),
        include_bytes!("../../fixtures/import/encryption/aes128-clear-metadata.pdf"),
        include_bytes!("../../fixtures/import/encryption/aes128-identity-streams.pdf"),
        include_bytes!("../../fixtures/import/encryption/aes128-identity-strings.pdf"),
        include_bytes!("../../fixtures/import/encryption/aes128-object-members.pdf"),
        include_bytes!("../../fixtures/import/encryption/aes128-direct-real-length.pdf"),
    ] {
        let source = bytes.to_vec();
        let scene = import(bytes, &ImportOptions::default()).unwrap();
        let imported = doc(scene);
        assert!(imported.walk().iter().any(|(_, _, l)| matches!(l.content, LayerContent::Shape(_))));
        assert_eq!(photocraft_compose::flatten(&imported).px, expected.px);
        assert_eq!(bytes, source);
    }
}
#[test]
fn pdf_encryption_wrong_password_bad_blocks_padding_and_schema_fail() {
    assert!(matches!(
        import(include_bytes!("../../fixtures/import/encryption/aes128-user-password.pdf"), &ImportOptions::default()),
        Err(ImportError::Encrypted)
    ));
    for bytes in [
        include_bytes!("../../fixtures/import/encryption/invalid-padding.pdf").as_slice(),
        include_bytes!("../../fixtures/import/encryption/invalid-string-padding.pdf"),
        include_bytes!("../../fixtures/import/encryption/invalid-block.pdf"),
        include_bytes!("../../fixtures/import/encryption/invalid-mapping.pdf"),
        include_bytes!("../../fixtures/import/encryption/invalid-owner.pdf"),
    ] {
        assert!(import(bytes, &ImportOptions::default()).is_err());
    }
    let bytes = include_bytes!("../../fixtures/import/encryption/aes128-r4.pdf");
    let mut options = ImportOptions::default();
    options.limits.decompressed_bytes = 1024;
    assert!(matches!(import(bytes, &options), Err(ImportError::Limit(_))));
    assert!(matches!(import_with(bytes, &ImportOptions::default(), &PdfOptions::default(), &AtomicBool::new(true)), Err(ImportError::Cancelled)));
}
