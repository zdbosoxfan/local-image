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

// A deliberately simple literal/code packer, not the production prefix decoder.
fn lzw_pack(codes: &[u16], early: bool) -> Vec<u8> {
    let (mut next, mut width, mut previous) = (258usize, 9usize, false);
    let mut bits = Vec::new();
    for &code in codes {
        for shift in (0..width).rev() {
            bits.push(((code >> shift) & 1) as u8);
        }
        match code {
            256 => {
                next = 258;
                width = 9;
                previous = false;
            }
            257 => {}
            _ => {
                if previous {
                    next = (next + 1).min(4096);
                }
                previous = true;
                let adjusted = next + usize::from(early);
                width = if adjusted >= 2048 {
                    12
                } else if adjusted >= 1024 {
                    11
                } else if adjusted >= 512 {
                    10
                } else {
                    9
                };
            }
        }
    }
    bits.chunks(8).map(|b| b.iter().enumerate().fold(0, |v, (i, bit)| v | bit << (7 - i))).collect()
}
fn lzw_literals(data: &[u8], early: bool) -> Vec<u8> {
    let mut codes = vec![256];
    codes.extend(data.iter().map(|b| u16::from(*b)));
    codes.push(257);
    lzw_pack(&codes, early)
}
fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}
fn png_predict(data: &[u8], columns: usize, stride: usize, predictor: u8) -> Vec<u8> {
    let mut out = Vec::new();
    for (r, row) in data.chunks_exact(columns).enumerate() {
        let tag = if predictor == 15 { (r % 5) as u8 } else { predictor - 10 };
        out.push(tag);
        for (i, &byte) in row.iter().enumerate() {
            let left = if i >= stride { row[i - stride] } else { 0 };
            let up = if r > 0 { data[(r - 1) * columns + i] } else { 0 };
            let corner = if r > 0 && i >= stride { data[(r - 1) * columns + i - stride] } else { 0 };
            let p = match tag {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => {
                    let (a, b, c) = (i32::from(left), i32::from(up), i32::from(corner));
                    let p = a + b - c;
                    let d = [(p - a).abs(), (p - b).abs(), (p - c).abs()];
                    if d[0] <= d[1] && d[0] <= d[2] {
                        left
                    } else if d[1] <= d[2] {
                        up
                    } else {
                        corner
                    }
                }
                _ => unreachable!(),
            };
            out.push(byte.wrapping_sub(p));
        }
    }
    out
}
#[test]
fn pdf_lzw_early_change_clear_and_all_code_widths_preserve_art() {
    let mut raw = ART.to_vec();
    raw.extend(vec![b' '; 5000]);
    for early in [false, true] {
        for clear in [false, true] {
            let mut codes = vec![256];
            for (i, &byte) in raw.iter().enumerate() {
                if clear && i == 1024 {
                    codes.push(256);
                }
                codes.push(u16::from(byte));
            }
            codes.push(257);
            let encoded = lzw_pack(&codes, early);
            for name in ["LZWDecode", "LZW"] {
                let dictionary = format!("/Filter /{name} /DecodeParms << /EarlyChange {} >>", u8::from(early));
                assert_art(&fixture::pdf(&objects(fixture::stream(&dictionary, &encoded), &[])));
            }
        }
    }
}
#[test]
fn pdf_lzw_and_flate_predictors_analyze_and_render_decoded_bytes() {
    let row = [ART, b"\n"].concat();
    let raw = row.repeat(5);
    for name in ["LZWDecode", "FlateDecode"] {
        for predictor in [2, 10, 11, 12, 13, 14, 15] {
            let predicted = if predictor == 2 {
                raw.chunks_exact(row.len()).flat_map(|row| row.iter().enumerate().map(|(i, b)| b.wrapping_sub(if i == 0 { 0 } else { row[i - 1] }))).collect()
            } else {
                png_predict(&raw, row.len(), 1, predictor)
            };
            let encoded = if name == "LZWDecode" { lzw_literals(&predicted, true) } else { deflate(&predicted) };
            let params = format!("/Filter /{name} /DecodeParms << /Predictor {predictor} /Columns {} /Colors 1 /BitsPerComponent 8 >>", row.len());
            assert_art(&fixture::pdf(&objects(fixture::stream(&params, &encoded), &[])));
        }
    }
    let lzw = a85(&lzw_literals(ART, false));
    assert_art(&fixture::pdf(&objects(fixture::stream("/Filter [/ASCII85Decode /LZWDecode] /DecodeParms [null << /EarlyChange 0 >>]", &lzw), &[])));
    assert_art(&fixture::pdf(&objects(fixture::stream("/Filter /LZWDecode /DP << /EarlyChange 0 >>", &lzw_literals(ART, false)), &[])));
}
#[test]
fn pdf_lzw_invalid_codes_predictor_parameters_and_budgets_fail_safely() {
    for codes in [vec![256, 258, 257], vec![256, 400, 257], vec![256, 65]] {
        let encoded = lzw_pack(&codes, true);
        assert!(import(&fixture::pdf(&objects(fixture::stream("/Filter /LZWDecode", &encoded), &[])), &ImportOptions::default()).is_err());
    }
    for params in [
        "/EarlyChange 2",
        "/EarlyChange -1",
        "/Predictor 3",
        "/Predictor 2 /Columns 0",
        "/Predictor 2 /Colors 32 /BitsPerComponent 16",
        "/Predictor 2 /Columns 3 /BitsPerComponent 1",
        "/Predictor 15 /Colors 3 /BitsPerComponent 4 /Columns 1",
    ] {
        let dict = format!("/Filter /LZWDecode /DecodeParms << {params} >>");
        assert!(import(&fixture::pdf(&objects(fixture::stream(&dict, &lzw_literals(ART, true)), &[])), &ImportOptions::default()).is_err(), "{params}");
    }
    for params in ["/DecodeParms 5 0 R", "/DecodeParms [null]", "/DP << >> /DecodeParms << >>"] {
        let dict = format!("/Filter /LZWDecode {params}");
        assert!(import(&fixture::pdf(&objects(fixture::stream(&dict, &lzw_literals(ART, true)), &[b"<< >>".to_vec()])), &ImportOptions::default()).is_err());
    }
    let mut codes = vec![256, 65];
    codes.extend(258..2000);
    codes.push(257);
    let bomb = fixture::pdf(&objects(fixture::stream("/Filter /LZWDecode", &lzw_pack(&codes, true)), &[]));
    let mut options = ImportOptions::default();
    options.limits.decompressed_bytes = 150_000;
    assert!(matches!(import(&bomb, &options), Err(ImportError::Limit(_))));
    let mut codes = vec![256; 64];
    codes.push(257);
    let work = fixture::pdf(&objects(fixture::stream("/Filter /LZWDecode", &lzw_pack(&codes, true)), &[]));
    options = ImportOptions::default();
    options.limits.segments = 2;
    assert!(matches!(import(&work, &options), Err(ImportError::Limit(_))));
    assert!(matches!(import_with(&bomb, &ImportOptions::default(), &PdfOptions::default(), &AtomicBool::new(true)), Err(ImportError::Cancelled)));
}
#[test]
fn pdf_lzw_predictor_hidden_form_cycles_are_not_zero_work() {
    let raw = b"/X Do";
    let predicted: Vec<_> = raw.iter().enumerate().map(|(i, b)| b.wrapping_sub(if i == 0 { 0 } else { raw[i - 1] })).collect();
    for name in ["LZWDecode", "FlateDecode"] {
        let encoded = if name == "LZWDecode" { lzw_literals(&predicted, true) } else { deflate(&predicted) };
        let params = format!(
            "/Type /XObject /Subtype /Form /BBox [0 0 64 64] /Resources << /XObject << /X 5 0 R >> >> /Filter /{name} /DecodeParms << /Predictor 2 /Columns 5 >>"
        );
        let bytes = fixture::page("/X Do", "/XObject << /X 5 0 R >>", "", &[fixture::stream(&params, &encoded)]);
        let error = import(&bytes, &ImportOptions::default()).unwrap_err();
        assert!(error.to_string().contains("recursive PDF resource graph"), "{name}: {error}");
    }
}

fn tiff_gray_predict(data: &[u8], bits: usize) -> Vec<u8> {
    let mask = (1u32 << bits) - 1;
    let mut out = Vec::new();
    for row in data.chunks_exact(bits) {
        // eight grayscale samples per row
        let mut encoded = vec![0u8; bits];
        let mut previous = 0u32;
        for sample in 0..8 {
            let offset = sample * bits;
            let mut value = 0;
            for k in 0..bits {
                value = value * 2 + u32::from((row[(offset + k) / 8] >> (7 - (offset + k) % 8)) & 1);
            }
            let delta = value.wrapping_sub(previous) & mask;
            previous = value;
            for k in 0..bits {
                encoded[(offset + k) / 8] |= (((delta >> (bits - 1 - k)) & 1) as u8) << (7 - (offset + k) % 8);
            }
        }
        out.extend(encoded);
    }
    out
}
fn image_pdf(data: &[u8], extra: &str, bits: usize, width: usize, color: &str) -> Vec<u8> {
    let dict = format!("/Type /XObject /Subtype /Image /Width {width} /Height 2 /ColorSpace /{color} /BitsPerComponent {bits} {extra}");
    fixture::page("q 48 0 0 48 8 8 cm /Im Do Q", "/XObject << /Im 5 0 R >>", "", &[fixture::stream(&dict, data)])
}
#[test]
fn pdf_lzw_tiff_packed_and_png_rgb_images_match_unfiltered_pixels() {
    for bits in [1, 2, 4, 8, 16] {
        let raw: Vec<u8> = (0..bits * 2).map(|i| (i as u8).wrapping_mul(37).wrapping_add(23)).collect();
        let expected = render(&image_pdf(&raw, "", bits, 8, "DeviceGray"));
        let predicted = tiff_gray_predict(&raw, bits);
        let extra = format!("/Filter /LZWDecode /DecodeParms << /Predictor 2 /Columns 8 /Colors 1 /BitsPerComponent {bits} >>");
        assert_eq!(render(&image_pdf(&lzw_literals(&predicted, true), &extra, bits, 8, "DeviceGray")).px, expected.px, "TIFF bits={bits}");
    }
    let raw = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    let expected = render(&image_pdf(&raw, "", 8, 2, "DeviceRGB"));
    for predictor in 10..=15 {
        let predicted = png_predict(&raw, 6, 3, predictor);
        let extra = format!("/Filter /LZWDecode /DecodeParms << /Predictor {predictor} /Columns 2 /Colors 3 /BitsPerComponent 8 >>");
        assert_eq!(render(&image_pdf(&lzw_literals(&predicted, true), &extra, 8, 2, "DeviceRGB")).px, expected.px, "PNG predictor={predictor}");
    }
}

#[test]
fn pdf_lzw_predictor_truncation_tags_and_row_allocation_are_rejected() {
    for (params, data) in [
        ("/Predictor 15 /Columns 2", vec![5, 1, 2]),
        ("/Predictor 15 /Columns 2", vec![0, 1]),
        ("/Predictor 2 /Columns 4", vec![1, 2, 3]),
        ("/Predictor 2 /Colors 0", vec![1]),
        ("/Predictor 2 /Columns 1 /BitsPerComponent 3", vec![1]),
    ] {
        let dict = format!("/Filter /LZWDecode /DecodeParms << {params} >>");
        let bytes = fixture::pdf(&objects(fixture::stream(&dict, &lzw_literals(&data, true)), &[]));
        assert!(import(&bytes, &ImportOptions::default()).is_err(), "{params}: {data:?}");
    }
    let bytes = fixture::pdf(&objects(fixture::stream("/Filter /LZWDecode /DecodeParms << /Predictor 15 /Columns 1000000 >>", &lzw_literals(&[], true)), &[]));
    let mut options = ImportOptions::default();
    options.limits.decompressed_bytes = 150000;
    assert!(matches!(import(&bytes, &options), Err(ImportError::Limit(_))));
}
