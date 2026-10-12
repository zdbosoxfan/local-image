//! Independent inline-vs-XObject fixtures with identical encoded image data.
use super::compatibility::{a85, deflate, hex, lzw_literals, rl};
use super::*;
fn pair(dict: &str, bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let resource = format!("/Type /XObject /Subtype /Image {dict}");
    let additional = [fixture::stream(&resource, bytes)];
    let mut content = format!("q 48 0 0 48 8 8 cm BI {dict} ID ").into_bytes();
    content.extend_from_slice(bytes);
    content.extend_from_slice(b" EI Q 0 0 1 rg 54 54 6 6 re f");
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << /XObject << /Im 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
        fixture::stream("", &content),
        additional[0].clone(),
    ];
    (fixture::pdf(&objects), fixture::page("q 48 0 0 48 8 8 cm /Im Do Q 0 0 1 rg 54 54 6 6 re f", "/XObject << /Im 5 0 R >>", "", &additional))
}
fn equivalent(dict: &str, bytes: &[u8]) {
    let (inline, external) = pair(dict, bytes);
    assert_eq!(render(&inline).px, render(&external).px, "{dict}");
}
#[test]
fn pdf_inline_exact_rows_and_admitted_codec_ends_match_xobjects() {
    let raw = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    let dict = "/Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceRGB";
    equivalent(dict, &raw);
    for (filter, encoded) in [
        ("ASCIIHexDecode", hex(&raw)),
        ("ASCII85Decode", a85(&raw)),
        ("RunLengthDecode", rl(&raw)),
        ("FlateDecode", deflate(&raw)),
        ("LZWDecode", lzw_literals(&raw, true)),
    ] {
        equivalent(&format!("{dict} /Filter /{filter}"), &encoded);
    }
    equivalent(&format!("{dict} /Filter [/ASCII85Decode /FlateDecode] /DecodeParms [null null]"), &a85(&deflate(&raw)));
    let predicted = [0, 255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 255, 255, 255];
    equivalent(&format!("{dict} /Filter /FlateDecode /DecodeParms << /Predictor 15 /Columns 2 /Colors 3 /BitsPerComponent 8 >>"), &deflate(&predicted));
    let fax = super::ccitt::coded(-1, false, true);
    equivalent("/Width 8 /Height 4 /BitsPerComponent 1 /ColorSpace /DeviceGray /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 8 /Rows 4 >>", &fax);
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg).encode(&raw, 2, 2, image::ExtendedColorType::Rgb8).unwrap();
    // Fake JPEG EOI and inline EI inside a length-delimited APP segment.
    let app = b" EI \xff\xd9 BI ";
    let mut decorated = vec![255, 216, 255, 226];
    decorated.extend_from_slice(&((app.len() + 2) as u16).to_be_bytes());
    decorated.extend_from_slice(app);
    decorated.extend_from_slice(&jpeg[2..]);
    equivalent(&format!("{dict} /Filter /DCTDecode"), &decorated);
}
#[test]
fn pdf_inline_fake_ei_binary_and_subsequent_content_array_values_are_preserved() {
    let raw = b" EI Q q BI fakEI"; // sixteen exact grayscale bytes, all ASCII
    assert_eq!(raw.len(), 16);
    let mut inline = b"q 48 0 0 32 8 8 cm BI /W 16 /H 1 /BPC 8 /CS /G /Length 16 ID ".to_vec();
    inline.extend_from_slice(raw);
    inline.extend_from_slice(b" EI Q");
    let image = fixture::stream("/Type /XObject /Subtype /Image /Width 16 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray", raw);
    let later = b"% fake EI and BI inside comment\nBT /F1 8 Tf 4 54 Td (B(EI)\\I\\\\I) Tj ET /EIBI Do q 6 0 0 6 50 50 cm BI /W 16 /H 1 /BPC 8 /CS /G ID ";
    let mut suffix = later.to_vec();
    suffix.extend_from_slice(raw);
    suffix.extend_from_slice(b" EI Q");
    let mut baseline = later[..later.len() - b"BI /W 16 /H 1 /BPC 8 /CS /G ID ".len()].to_vec();
    baseline.extend_from_slice(b"/Im Do Q");
    let build = |first: &[u8], second: &[u8]| {
        fixture::pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << /Font << /F1 6 0 R >> /XObject << /EIBI 7 0 R /Im 8 0 R >> >> /Contents [4 0 R 5 0 R] >>".to_vec(),
        fixture::stream("/Foreign (untouched metadata)",first),fixture::stream("",second),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        fixture::stream("/Type /XObject /Subtype /Form /BBox [0 0 64 64] /Resources << >>",b"0 0 1 rg 40 2 12 5 re f"),image.clone(),
    ])
    };
    assert_eq!(render(&build(&inline, &suffix)).px, render(&build(b"q 48 0 0 32 8 8 cm /Im Do Q", &baseline)).px);
}
#[test]
fn pdf_inline_aliases_whitespace_and_stencil_values_match_uncompressed_reference() {
    let content = b"q 48 0 0 48 8 8 cm BI /W 2 /H 1 /BPC 8 /CS /G ID\r\n \xff EI Q";
    let bytes = fixture::pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Contents 4 0 R >>".to_vec(),
        fixture::stream("", content),
    ]);
    let (_, reference) = pair("/Width 2 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray", &[32, 255]);
    // Reference helper adds a blue marker; compare only the actual image region.
    let actual = render(&bytes);
    let expected = render(&reference);
    for y in 8..54 {
        for x in 8..54 {
            assert_eq!(actual.get(x, y), expected.get(x, y));
        }
    }
    equivalent("/Width 8 /Height 1 /ImageMask true /BitsPerComponent 1", &[0b01010101]);
}
#[test]
fn pdf_inline_truncation_expansion_limits_and_recursive_tail_are_rejected() {
    for (dict, data) in [
        ("/W 3 /H 1 /BPC 8 /CS /G", b"ab".as_slice()),
        ("/W 2 /H 1 /BPC 8 /CS /G /Length 1", b"ab"),
        ("/W 1 /H 1 /BPC 8 /CS /G /F /AHx", b"zz>"),
        ("/W 1 /H 1 /BPC 8 /CS /G /F /Fl", b"bad"),
        ("/W 1 /Width 2 /H 1 /BPC 8 /CS /G", b"a"),
    ] {
        assert!(import(&pair(dict, data).0, &ImportOptions::default()).is_err(), "{dict}");
    }
    let bomb = pair("/Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /FlateDecode", &deflate(&vec![0; 1 << 20])).0;
    let mut options = ImportOptions::default();
    options.limits.decompressed_bytes = 4096;
    assert!(matches!(import(&bomb, &options), Err(ImportError::Limit(_))));
    let ordinary = pair("/Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray", &[42]).0;
    options = ImportOptions::default();
    options.limits.objects = 5;
    assert!(matches!(import(&ordinary, &options), Err(ImportError::Limit(_))));
    options = ImportOptions::default();
    options.limits.depth = 1;
    assert!(import(&pair("/Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceGray /Filter /ASCIIHexDecode", b"2a>").0, &options).is_err());
    assert!(matches!(import_with(&ordinary, &ImportOptions::default(), &PdfOptions::default(), &AtomicBool::new(true)), Err(ImportError::Cancelled)));
    let form = b"BI /W 1 /H 1 /BPC 8 /CS /G ID X EI /Self Do";
    let recursive = fixture::page(
        "/Self Do",
        "/XObject << /Self 5 0 R >>",
        "",
        &[fixture::stream("/Type /XObject /Subtype /Form /BBox [0 0 64 64] /Resources << /XObject << /Self 5 0 R >> >>", form)],
    );
    assert!(import(&recursive, &ImportOptions::default()).unwrap_err().to_string().contains("recursive PDF resource graph"));
}

#[test]
fn pdf_inline_group3_last_row_padding_cannot_consume_lf_before_ei() {
    // White1728 makeup010011011 + white0 00110101; 17 bits plus 7 zero
    // padding bits. Those padding bits plus LF's first five bits mimic EOL.
    let payload = [0x4d, 0x9a, 0x80];
    let dict = "/W 1728 /H 1 /BPC 1 /CS /G /F /CCF /DP << /K 0 /Columns 1728 /Rows 1 /EndOfBlock false >>";
    let mut content = format!("q 48 0 0 48 8 8 cm BI {dict} ID ").into_bytes();
    content.extend_from_slice(&payload);
    content.extend_from_slice(b"\nEI Q 0 0 1 rg 54 54 6 6 re f");
    let inline = fixture::pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Contents 4 0 R >>".to_vec(),
        fixture::stream("", &content),
    ]);
    let (_, reference) = pair(
        "/Width 1728 /Height 1 /BitsPerComponent 1 /ColorSpace /DeviceGray /Filter /CCITTFaxDecode /DecodeParms << /K 0 /Columns 1728 /Rows 1 /EndOfBlock false >>",
        &payload,
    );
    assert_eq!(render(&inline).px, render(&reference).px);
}
