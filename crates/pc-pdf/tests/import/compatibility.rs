//! Original compact PDF fixtures, independent of the production admission decoders.
use super::*;
use std::io::Write;
const ART: &[u8] = b"1 0 0 rg 8 8 48 48 re f";
fn objects(body: Vec<u8>, lengths: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << >> /Contents 4 0 R >>".to_vec(),
        body,
    ];
    objects.extend_from_slice(lengths);
    objects
}
fn indirect(data: &[u8], reference: &str) -> Vec<u8> {
    let mut body = format!("<< /Length {reference} >>\nstream\n").into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(b"\nendstream");
    body
}
fn hex(data: &[u8]) -> Vec<u8> {
    let mut out = data.iter().map(|b| format!("{b:02X} ")).collect::<String>().into_bytes();
    out.push(b'>');
    out
}
fn a85(data: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    for bytes in data.chunks(4) {
        let mut group = [0u8; 4];
        group[..bytes.len()].copy_from_slice(bytes);
        let mut value = u32::from_be_bytes(group);
        let mut digits = [0; 5];
        for d in digits.iter_mut().rev() {
            *d = (value % 85) as u8 + b'!';
            value /= 85;
        }
        out.extend_from_slice(&digits[..bytes.len() + 1]);
    }
    out.extend_from_slice(b"~>");
    out
}
fn rl(data: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    for chunk in data.chunks(128) {
        out.push((chunk.len() - 1) as u8);
        out.extend_from_slice(chunk);
    }
    out.push(128);
    out
}
fn assert_art(bytes: &[u8]) {
    let scene = import(bytes, &ImportOptions::default()).unwrap();
    let imported = doc(scene);
    assert!(imported.walk().iter().any(|(_, _, l)| matches!(l.content, LayerContent::Shape(_))));
    let expected = render(&fixture::page(std::str::from_utf8(ART).unwrap(), "", "", &[]));
    assert_eq!(photocraft_compose::flatten(&imported).px, expected.px);
}
#[test]
fn pdf_compat_leading_header_and_original_byte_coordinates() {
    for prefix in [0, 1, 1024] {
        let body = fixture::pdf(&objects(indirect(ART, "5 0 R"), &[ART.len().to_string().into_bytes()]));
        let mut bytes = vec![0; prefix];
        bytes.extend_from_slice(&body);
        assert_art(&bytes);
    }
    let mut bytes = vec![0; 1025];
    bytes.extend_from_slice(&fixture::artwork());
    assert!(import(&bytes, &ImportOptions::default()).is_err());
    assert!(import(b"junk%PDF-not-a-version", &ImportOptions::default()).is_err());
}
#[test]
fn pdf_compat_text_filters_and_chains_preserve_editable_pixels() {
    for (filter, data) in
        [("ASCIIHexDecode", hex(ART)), ("AHx", hex(ART)), ("ASCII85Decode", a85(ART)), ("A85", a85(ART)), ("RunLengthDecode", rl(ART)), ("RL", rl(ART))]
    {
        assert_art(&fixture::pdf(&objects(fixture::stream(&format!("/Filter /{filter}"), &data), &[])));
    }
    let mut compressor = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    compressor.write_all(ART).unwrap();
    let encoded = a85(&compressor.finish().unwrap());
    assert_art(&fixture::pdf(&objects(fixture::stream("/Filter [/ASCII85Decode /FlateDecode]", &encoded), &[])));
    // Repeat packets as well as literals: 128 spaces, followed by visible artwork.
    let mut repeat = vec![129, b' '];
    repeat.extend(rl(ART));
    assert_art(&fixture::pdf(&objects(fixture::stream("/Filter /RunLengthDecode", &repeat), &[])));
}
#[test]
fn pdf_compat_filter_errors_limits_and_cancel_remain_failures() {
    for (filter, data) in [
        ("ASCIIHexDecode", b"12G>".as_slice()),
        ("ASCIIHexDecode", b"12"),
        ("ASCII85Decode", b"!~>"),
        ("ASCII85Decode", b"uuuuu~>"),
        ("ASCII85Decode", b"!z~>"),
        ("ASCII85Decode", b"!!~"),
        ("RunLengthDecode", &[2, 1]),
        ("RunLengthDecode", &[255]),
        ("RunLengthDecode", &[128, 1]),
    ] {
        let bytes = fixture::pdf(&objects(fixture::stream(&format!("/Filter /{filter}"), data), &[]));
        assert!(import(&bytes, &ImportOptions::default()).is_err(), "{filter}: {data:?}");
    }
    for (filter, end) in [("ASCIIHexDecode", ">"), ("ASCII85Decode", "~>")] {
        let encoded = format!("{}{end}", " ".repeat(4096));
        let bytes = fixture::pdf(&objects(fixture::stream(&format!("/Filter /{filter}"), encoded.as_bytes()), &[]));
        let mut options = ImportOptions::default();
        options.limits.decompressed_bytes = 1024;
        assert!(matches!(import(&bytes, &options), Err(ImportError::Limit("decompressed byte"))), "pinned decoder preallocation must be charged");
    }
    let bytes = fixture::pdf(&objects(fixture::stream("/Filter /RunLengthDecode", &[129, b' ', 128]), &[]));
    let mut options = ImportOptions::default();
    options.limits.decompressed_bytes = 127;
    assert!(matches!(import(&bytes, &options), Err(ImportError::Limit("decompressed byte"))));
    let bytes = fixture::pdf(&objects(fixture::stream("/Filter [/ASCIIHexDecode /RunLengthDecode]", &hex(&rl(ART))), &[]));
    options.limits.decompressed_bytes = (ART.len() + 2) as u64;
    assert!(matches!(import(&bytes, &options), Err(ImportError::Limit("decompressed byte"))), "intermediate stages must accumulate");
    assert!(matches!(import_with(&bytes, &ImportOptions::default(), &PdfOptions::default(), &AtomicBool::new(true)), Err(ImportError::Cancelled)));
}
#[test]
fn pdf_compat_indirect_length_forward_backward_and_invalid_identity() {
    let length = ART.len().to_string().into_bytes();
    assert_art(&fixture::pdf(&objects(indirect(ART, "5 0 R"), std::slice::from_ref(&length))));
    let mut backwards = objects(length.clone(), &[indirect(ART, "4 0 R")]);
    backwards[2] = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << >> /Contents 5 0 R >>".to_vec();
    assert_art(&fixture::pdf(&backwards));
    for (reference, value) in [
        ("5 0 R", b"-1".to_vec()),
        ("5 0 R", b"(22)".to_vec()),
        ("5 0 R", b"5 0 R".to_vec()),
        ("5 1 R", length.clone()),
        ("6 0 R", length),
        ("5 0 R", b"9999999999999999999".to_vec()),
    ] {
        assert!(import(&fixture::pdf(&objects(indirect(ART, reference), &[value])), &ImportOptions::default()).is_err());
    }
}
#[test]
fn pdf_compat_xref_cannot_promote_length_headers_from_stream_bytes() {
    // Ten digits keep the enclosing payload's length stable when substituted.
    let fake = b"5 0 obj\n0000000000\nendobj\n1 0 0 rg 8 8 48 48 re f";
    let payload = String::from_utf8(fake.to_vec()).unwrap().replace("0000000000", &format!("{:010}", fake.len()));
    let mut bytes = fixture::pdf(&objects(indirect(payload.as_bytes(), "5 0 R"), &[payload.len().to_string().into_bytes()]));
    let fake_offset = bytes.windows(7).position(|w| w == b"5 0 obj").unwrap();
    let xref = bytes.windows(5).position(|w| w == b"xref\n").unwrap();
    let line = bytes[xref..].windows(20).enumerate().filter(|(_, w)| w[17] == b'n').nth(4).map(|(at, _)| xref + at).unwrap();
    bytes[line..line + 10].copy_from_slice(format!("{fake_offset:010}").as_bytes());
    let error = import(&bytes, &ImportOptions::default()).unwrap_err();
    assert!(error.to_string().contains("length locator was not an admitted object"), "{error}");
}
