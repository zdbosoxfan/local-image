// Original PDF 1.7 test fixtures. Local Image contributors, GPL-3.0-or-later.
// A conventional xref writer independent of both hayro and krilla.
pub fn pdf(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(o);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in &offsets[1..] {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len()).as_bytes());
    out
}
pub fn stream(dict: &str, content: &[u8]) -> Vec<u8> {
    let mut out = format!("<< /Length {} {dict} >>\nstream\n", content.len()).into_bytes();
    out.extend(content);
    out.extend(b"\nendstream");
    out
}
pub fn page(content: &str, resources: &str, extra: &str, additional: &[Vec<u8>]) -> Vec<u8> {
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << {resources} >> /Contents 4 0 R {extra} >>").into_bytes(),
        stream("", content.as_bytes()),
    ];
    objects.extend_from_slice(additional);
    pdf(&objects)
}
pub fn artwork() -> Vec<u8> {
    page("1 0 0 rg 8 8 48 48 re f 0 0 1 rg 24 24 16 16 re f", "", "", &[])
}
pub fn pages() -> Vec<u8> {
    pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << >> /Contents 4 0 R >>".to_vec(),
        stream("", b"1 0 0 rg 0 0 64 64 re f"),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 80 40] /CropBox [10 5 70 35] /Rotate 90 /Resources << /Font << /F 7 0 R >> >> /Contents 6 0 R >>"
            .to_vec(),
        stream("", b"0 0 1 rg 10 5 60 30 re f BT /F 10 Tf 20 20 Td (A) Tj ET"),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ])
}
pub fn masked() -> Vec<u8> {
    mask_kind("Luminosity")
}
pub fn alpha_masked() -> Vec<u8> {
    mask_kind("Alpha")
}
fn mask_kind(kind: &str) -> Vec<u8> {
    page(
        "/M gs 1 0 0 rg 0 0 64 64 re f",
        "/ExtGState << /M 5 0 R >>",
        "",
        &[
            format!("<< /Type /ExtGState /SMask << /S /{kind} /G 6 0 R >> >>").into_bytes(),
            stream(
                "/Type /XObject /Subtype /Form /BBox [0 0 64 64] /Resources << >> /Group << /S /Transparency /CS /DeviceRGB /I true >>",
                b"0.5 g 0 0 32 64 re f 1 g 32 0 32 64 re f",
            ),
        ],
    )
}
pub fn shading() -> Vec<u8> {
    page(
        "/G sh",
        "/Shading << /G 5 0 R >>",
        "",
        &[
            b"<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 64 0] /Function 6 0 R /Extend [true true] >>".to_vec(),
            b"<< /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >>".to_vec(),
        ],
    )
}
pub fn image() -> Vec<u8> {
    page(
        "q 32 0 0 32 16 16 cm /I Do Q",
        "/XObject << /I 5 0 R >>",
        "",
        &[stream(
            "/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8",
            &[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
        )],
    )
}

/// Original lossless codec fixtures: 16×16 quadrant RGB and bi-level checker pixels.
/// The reference pixels are defined analytically; neither fixture came from a PDF renderer.
pub fn fallback_codec(jbig2: bool) -> Vec<u8> {
    let (filter, space, bits, data): (&str, &str, u8, &[u8]) = if jbig2 {
        ("JBIG2Decode", "DeviceGray", 1, include_bytes!("fallback-checker.jb2"))
    } else {
        ("JPXDecode", "DeviceRGB", 8, include_bytes!("fallback-quadrants.jp2"))
    };
    page(
        "q 64 0 0 64 0 0 cm /I Do Q",
        "/XObject << /I 5 0 R >>",
        "",
        &[stream(&format!("/Type /XObject /Subtype /Image /Width 16 /Height 16 /BitsPerComponent {bits} /ColorSpace /{space} /Filter /{filter}"), data)],
    )
}
