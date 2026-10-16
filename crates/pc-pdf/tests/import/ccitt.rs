//! Hand-coded ITU-T T.4/T.6 fax rows, independent of the production tables.
use super::*;
const EOL: &str = "000000000001";
const WHITE8: &str = "10011";
const BLACK8: &str = "00110101000101"; // white run0, black run8
const HORIZONTAL_BLACK8: &str = "00100110101000101";
fn pack(bits: &str) -> Vec<u8> {
    bits.as_bytes().chunks(8).map(|chunk| chunk.iter().enumerate().fold(0, |v, (i, b)| v | u8::from(*b == b'1') << (7 - i))).collect()
}
pub(super) fn coded(k: i32, aligned: bool, markers: bool) -> Vec<u8> {
    let rows: Vec<String> = if k < 0 {
        vec!["1".into(), HORIZONTAL_BLACK8.into(), "11".into(), "0001".into()]
    } else if k == 0 {
        [WHITE8, BLACK8, BLACK8, WHITE8].map(str::to_owned).to_vec()
    } else {
        vec![format!("1{WHITE8}"), format!("0{HORIZONTAL_BLACK8}"), format!("1{BLACK8}"), "00001".into()]
    };
    let mut bits = String::new();
    for row in rows {
        if k >= 0 && markers {
            bits.push_str(EOL);
        }
        bits.push_str(&row);
        if aligned {
            while !bits.len().is_multiple_of(8) {
                bits.push('0');
            }
        }
    }
    if markers {
        bits.push_str(&EOL.repeat(if k < 0 { 2 } else { 6 }));
    }
    pack(&bits)
}
fn pdf(data: &[u8], filter: &str, params: &str, stencil: bool, height: usize) -> Vec<u8> {
    let image = if stencil { "/ImageMask true" } else { "/ColorSpace /DeviceGray" };
    let dict = format!("/Type /XObject /Subtype /Image /Width 8 /Height {height} /BitsPerComponent 1 {image} {filter} {params}");
    fixture::page("q 0.2 0.4 0.8 rg 48 0 0 48 8 8 cm /Im Do Q", "/XObject << /Im 5 0 R >>", "", &[fixture::stream(&dict, data)])
}
#[test]
fn pdf_ccitt_group3_group4_and_options_match_independent_image_rows() {
    for k in [-1, 0, 2] {
        for aligned in [false, true] {
            for markers in [false, true] {
                for invert in [false, true] {
                    let input = coded(k, aligned, markers);
                    let params = format!(
                        "/DecodeParms << /K {k} /Columns 8 /Rows 4 /EncodedByteAlign {aligned} /EndOfLine {markers} /EndOfBlock {markers} /BlackIs1 {invert} >>"
                    );
                    let raw = if invert { [0, 255, 255, 0] } else { [255, 0, 0, 255] };
                    for stencil in [false, true] {
                        let got = render(&pdf(&input, "/Filter /CCITTFaxDecode", &params, stencil, 4));
                        let expected = render(&pdf(&raw, "", "", stencil, 4));
                        assert_eq!(got.px, expected.px, "K={k} aligned={aligned} markers={markers} invert={invert} stencil={stencil}");
                    }
                }
            }
        }
    }
    // Missing Rows uses Height in the pinned downstream decoder; CCF alias.
    assert_eq!(
        render(&pdf(&coded(-1, false, true), "/Filter /CCF", "/DecodeParms << /K -1 /Columns 8 >>", false, 4)).px,
        render(&pdf(&[255, 0, 0, 255], "", "", false, 4)).px
    );
}
#[test]
fn pdf_ccitt_truncated_partial_and_dimension_mismatch_are_rejected() {
    for data in [vec![], vec![0], pack(&format!("1{}", EOL.repeat(2))), pack("00100110101000")] {
        assert!(import(&pdf(&data, "/Filter /CCITTFaxDecode", "/DecodeParms << /K -1 /Columns 8 /Rows 4 >>", false, 4), &ImportOptions::default()).is_err());
    }
    for params in [
        "/K -1 /Columns 8 /Rows 0",
        "/K 0 /Columns 8 /Rows 0",
        "/K -1 /Columns 8 /Rows 2",
        "/K -1 /Columns 8 /Rows 5",
        "/K -1 /Columns 9 /Rows 4",
        "/K -1 /Columns 8 /DamagedRowsBeforeError 1",
        "/K -1 /Columns 8 /EndOfBlock 1",
    ] {
        let params = format!("/DecodeParms << {params} >>");
        assert!(import(&pdf(&coded(-1, false, true), "/Filter /CCITTFaxDecode", &params, false, 4), &ImportOptions::default()).is_err(), "{params}");
    }
    // A run of nine pixels cannot be silently clamped to an eight-pixel row.
    let overlong = pack("10100");
    assert!(import(&pdf(&overlong, "/Filter /CCITTFaxDecode", "/DecodeParms << /K 0 /Columns 8 /Rows 1 >>", false, 1), &ImportOptions::default()).is_err());
}
#[test]
fn pdf_ccitt_chain_budget_cancellation_and_nonimage_controls() {
    let data = coded(-1, false, true);
    let hex = data.iter().map(|b| format!("{b:02x}")).collect::<String>() + ">";
    let bytes = pdf(hex.as_bytes(), "/Filter [/ASCIIHexDecode /CCITTFaxDecode]", "/DecodeParms [null << /K -1 /Columns 8 /Rows 4 >>]", false, 4);
    assert_eq!(render(&bytes).px, render(&pdf(&[255, 0, 0, 255], "", "", false, 4)).px);
    let mut options = ImportOptions::default();
    options.limits.decompressed_bytes = 600;
    assert!(matches!(import(&bytes, &options), Err(ImportError::Limit(_))));
    assert!(matches!(import_with(&bytes, &ImportOptions::default(), &PdfOptions::default(), &AtomicBool::new(true)), Err(ImportError::Cancelled)));
    let nonimage = fixture::pdf(&[fixture::stream("/Filter /CCITTFaxDecode", &data)]);
    assert!(import(&nonimage, &ImportOptions::default()).is_err());
    let post = pdf(&data, "/Filter [/CCITTFaxDecode /ASCIIHexDecode]", "/DecodeParms [<< /K -1 /Columns 8 /Rows 4 >> null]", false, 4);
    assert!(import(&post, &ImportOptions::default()).is_err());
}

#[test]
fn pdf_ccitt_downstream_aliases_and_earlier_image_decoders_cannot_bypass_bounds() {
    for alias in ["/H 1000000", "/W 1000000", "/CS /DeviceRGB", "/IM true", "/BPC 16"] {
        let params = format!("{alias} /DecodeParms << /K -1 /Columns 8 /Rows 4 >>");
        assert!(import(&pdf(&coded(-1, false, true), "/Filter /CCITTFaxDecode", &params, false, 4), &ImportOptions::default()).is_err());
    }
    let bytes = pdf(&[0], "/Filter [/DCTDecode /CCITTFaxDecode]", "/DecodeParms [null << /K -1 /Columns 8 /Rows 4 >>]", false, 4);
    assert!(matches!(import(&bytes, &ImportOptions::default()), Err(ImportError::Unsupported("CCITT must be the terminal image decoder"))));
}
