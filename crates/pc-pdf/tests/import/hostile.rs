//! Damaged and hostile resources from VectorCraft's import fuzzing (MIT OR Apache-2.0): arrays and
//! dictionaries the reader can't parse, junk ink colour spaces and tint transforms, nesting deep
//! enough to overflow a recursive reader, inside the page and inside a compressed object stream.
//! Each is imported or refused, never a panic.

use super::compatibility::deflate;
use super::fixture;
use photocraft_doc::vector_design::exchange::ImportOptions;
use photocraft_pdf::import;

const RESOURCES: &[&str] = &[
    "/Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >>",
    "/ExtGState << /GS0 << /CA 0.5 /ca -3 /BM /Multiply /SMask /None >> >>",
    "/Shading << /Sh0 << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 1e308 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >> >>",
    "/Shading << /Sh0 << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [0 0 0 0 0 -5] /Function << /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [] /N -1 >> >> >>",
    "/XObject << /Im0 << /Type /XObject /Subtype /Image /Width 4294967295 /Height 0 /BitsPerComponent 8 /ColorSpace /DeviceRGB /Length 3 >> >>",
    "/Pattern << /P0 << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 0 0] /XStep 0 /YStep -0 /Resources << >> >> >>",
    "/ColorSpace << /CS0 [/Separation /All /DeviceCMYK << /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [1 1 1 1] /N 1 >>] >>",
    "/ColorSpace << /CS0 [/Separation /#00 /Lab << /FunctionType 2 /Domain [0 1] /C0 [1e308] /C1 [NaN 5] /N -1 >>] >>",
    "/ColorSpace << /CS0 [/DeviceN [] /DeviceGray << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >>] >>",
    "/ColorSpace << /CS0 [/DeviceN [/A /None /Cyan /All] [/ICCBased << /N 4 >>] << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >>] >>",
    "/ColorSpace << /A /#zz /CS0 [/Separation /Gold /DeviceCMYK << /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [0 0 1 0] /N 1 >>] >> /Font << /A 12-.5 >>",
    "/Properties << /P0 [<< /FunctionType 4 /Domain [0 1] /Range [0 1 0 1] /Filter [/FlateDecode] /DecodeParms [null << /Predictor 12 /Colors 4 /Columns 99999999999 >>] /Length 3 >>] >>",
    "/Shading << /Sh0 [ 1 2",
    "/Font << /F1 << /Type /Font /Subtype /Type1 /Widths [1 2 3",
];

const CONTENT: &str = "/CS0 cs 1 scn 0 0 50 50 re f /GS0 gs BT /F1 12 Tf 10 60 Td (Hi) Tj [(a) -250 (b)] TJ ET /Sh0 sh /P0 scn q 10 0 0 10 0 0 cm /Im0 Do Q";

#[test]
fn damaged_and_hostile_resources_import_or_are_refused() {
    for resources in RESOURCES {
        let pdf = fixture::page(CONTENT, resources, "", &[]);
        let _ = import(&pdf, &ImportOptions::default());
    }
}

#[test]
fn deep_nesting_does_not_overflow_the_stack() {
    let deep = |open: &str, close: &str, n: usize| format!("{}0{}", open.repeat(n), close.repeat(n));
    for n in [40, 100_000] {
        for (open, close) in [("[", "]"), ("<< /K ", " >>")] {
            let nested = deep(open, close, n);
            let in_resources = fixture::page("0 0 10 10 re f", &format!("/Properties << /P0 {nested} >>"), "", &[]);
            let r = import(&in_resources, &ImportOptions::default());
            assert_eq!(r.is_err(), n > 64, "{n} levels of {open}: {:?}", r.as_ref().err());
            let in_content = fixture::page(&format!("{nested} pop 0 0 10 10 re f"), "", "", &[]);
            let _ = import(&in_content, &ImportOptions::default());
            // The same nesting inside a compressed object stream, which the reader only sees once
            // it inflates the stream.
            let held = format!("6 0 {nested}");
            let body = deflate(held.as_bytes());
            let mut objstm = format!("<< /Type /ObjStm /N 1 /First 4 /Filter /FlateDecode /Length {} >>\nstream\n", body.len()).into_bytes();
            objstm.extend(&body);
            objstm.extend(b"\nendstream");
            let in_objstm = fixture::page("0 0 10 10 re f", "/Properties << /P0 6 0 R >>", "", &[objstm]);
            let r = import(&in_objstm, &ImportOptions::default());
            assert_eq!(r.is_err(), n > 64, "{n} levels of {open} in an object stream: {:?}", r.as_ref().err());
        }
    }
}
