use photocraft_psd::{Compression, PsdFile, Version, testgen};

#[test]
fn recovery_is_explicit_and_preserves_encoded_bytes() {
    for version in [Version::Psd, Version::Psb] {
        for compression in Compression::ALL {
            let mut original = testgen::small(version, compression);
            original.image_data.data.pop();
            let bytes = original.to_bytes().unwrap();
            let strict = PsdFile::from_bytes(&bytes).unwrap_err();
            assert_eq!(PsdFile::from_bytes_recovering(&bytes, |_| false).unwrap_err(), strict);
            let (file, error) = PsdFile::from_bytes_recovering(&bytes, |_| true).unwrap();
            assert_eq!(error, Some(strict));
            assert_eq!(file, original);
            assert_eq!(file.to_bytes().unwrap(), bytes);
        }
    }
}

#[test]
fn recovery_rejects_missing_compression_and_incomplete_layers() {
    let original = testgen::small(Version::Psd, Compression::Raw);
    let bytes = original.to_bytes().unwrap();
    let offset = bytes.len() - original.image_data.data.len() - 2;
    for cut in [offset - 1, offset, offset + 1] {
        assert!(PsdFile::from_bytes_recovering(&bytes[..cut], |_| true).is_err(), "cut {cut}");
    }
}
