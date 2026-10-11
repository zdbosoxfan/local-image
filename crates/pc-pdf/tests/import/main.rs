#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use photocraft_doc::{
    ColorMode, Document, LayerContent, SampleType, Size,
    vector_design::exchange::{ImportOptions, ImportedScene},
};
use photocraft_pdf::{ImportError, PageBox, PdfOptions, import, import_with, info};
use std::sync::atomic::AtomicBool;
#[path = "../../fixtures/import/source.rs"]
mod fixture;
fn doc(scene: ImportedScene) -> Document {
    let mut d = Document::new("PDF", scene.size, ColorMode::Rgb, SampleType::F32);
    d.layers = scene.layers;
    d
}
fn render(data: &[u8]) -> photocraft_compose::Buffer {
    photocraft_compose::flatten(&doc(import(data, &ImportOptions::default()).unwrap()))
}
#[test]
fn pdf_import_draw_order_and_editable_native_paths() {
    let scene = import(&fixture::artwork(), &ImportOptions::default()).unwrap();
    assert_eq!(scene.size, Size::new(64, 64));
    let mut d = doc(scene);
    assert!(d.walk().iter().filter(|(_, _, l)| matches!(l.content, LayerContent::Shape(_))).count() == 2);
    let r = photocraft_compose::flatten(&d);
    assert_eq!(r.get(10, 10), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(r.get(30, 30), [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(r.get(2, 2), [0.0; 4]);
    let child = match &mut d.layers[0].content {
        LayerContent::Group(g) => &mut g.children[1],
        _ => panic!("page group"),
    };
    if let LayerContent::Shape(s) = &mut child.content {
        s.fill = Some(photocraft_doc::Fill::Solid(photocraft_doc::Color::rgb(0.0, 1.0, 0.0)));
        s.cache = Some(photocraft_vector::render_shape(s, photocraft_doc::PixelFormat::RGBA32F, photocraft_doc::Rect::from_size(d.size)));
    }
    assert_eq!(photocraft_compose::flatten(&d).get(30, 30), [0.0, 1.0, 0.0, 1.0]);
}
#[test]
fn pdf_import_holes_curves_clips_independent_pixel_oracles() {
    let data = fixture::page("1 0 0 rg 4 4 56 56 re 20 20 24 24 re f* q 0 0 32 64 re W n 0 1 0 rg 0 0 64 64 re f Q", "", "", &[]);
    let r = render(&data);
    assert_eq!(r.get(10, 10), [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(r.get(50, 10), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(r.get(40, 30), [0.0; 4]);
    let curve = fixture::page("0 0 1 rg 8 32 m 8 8 56 8 56 32 c 56 56 8 56 8 32 c h f", "", "", &[]);
    let scene = import(&curve, &ImportOptions::default()).unwrap();
    let d = doc(scene);
    let s = d.walk().into_iter().find_map(|(_, _, l)| if let LayerContent::Shape(s) = &l.content { Some(s) } else { None }).unwrap();
    assert!(s.path.subpaths[0].knots.iter().any(|k| k.out_ctrl != k.anchor));
    // Green's theorem: each lobe of this cubic lens has area 691.2, total 1382.4.
    let r = photocraft_compose::flatten(&d);
    let area: f64 = r.px.iter().map(|p| f64::from(p[3])).sum();
    assert!((area - 1382.4).abs() < 1.0, "{area}");
}
#[test]
fn pdf_import_page_boxes_rotation_selection_and_dpi() {
    let bytes = fixture::pages();
    let meta = info(&bytes, &ImportOptions::default(), &AtomicBool::new(false)).unwrap();
    assert_eq!(meta.pages.len(), 2);
    assert_eq!(meta.pages[1].rotation, 90);
    let options = ImportOptions { pages: vec![1], ..Default::default() };
    let scene = import(&bytes, &options).unwrap();
    assert_eq!(scene.size, Size::new(30, 60));
    assert!(scene.warnings.iter().any(|w| w.feature == "glyph-outlines"));
    let scene = import_with(&bytes, &options, &PdfOptions { page_box: PageBox::Media, ignore_rotation: false }, &AtomicBool::new(false)).unwrap();
    assert_eq!(scene.size, Size::new(40, 80));
    let scene = import_with(&bytes, &options, &PdfOptions { page_box: PageBox::Crop, ignore_rotation: true }, &AtomicBool::new(false)).unwrap();
    assert_eq!(scene.size, Size::new(60, 30));
    let scene = import(&bytes, &ImportOptions { dpi: 144.0, pages: vec![1, 0], ..Default::default() }).unwrap();
    assert_eq!(scene.size, Size::new(188, 128));
    assert_eq!(scene.layers.len(), 2);
    assert!(import(&bytes, &ImportOptions { pages: vec![2], ..Default::default() }).is_err());
    assert!(import(&bytes, &ImportOptions { pages: vec![0, 0], ..Default::default() }).is_err());
}
#[test]
fn pdf_import_axial_shading_matches_analytic_colour() {
    let r = render(&fixture::shading());
    for x in [0, 16, 32, 63] {
        let p = r.get(x, 32);
        let t = (x as f32 + 0.5) / 64.0;
        assert!((p[0] - (1.0 - t)).abs() <= 1.0 / 255.0);
        assert!((p[2] - t).abs() <= 1.0 / 255.0);
        assert_eq!(p[3], 1.0);
    }
}
#[test]
fn pdf_import_soft_mask_luminosity_and_image_orientation() {
    let r = render(&fixture::masked());
    assert!((r.get(16, 16)[3] - 0.5).abs() <= 1.0 / 255.0);
    assert_eq!(r.get(48, 16), [1.0, 0.0, 0.0, 1.0]);
    let r = render(&fixture::image());
    assert_eq!(r.get(20, 20), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(r.get(40, 20), [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(r.get(20, 40), [0.0, 0.0, 1.0, 1.0]);
}
#[test]
fn pdf_import_opacity_blend_and_group_overlap() {
    let data =
        fixture::page("1 0 0 rg 0 0 64 64 re f /A gs 0 0 1 rg 8 8 48 48 re f", "/ExtGState << /A 5 0 R >>", "", &[b"<< /ca 0.5 /BM /Normal >>".to_vec()]);
    let r = render(&data);
    assert!(r.get(32, 32).iter().zip([0.5, 0.0, 0.5, 1.0]).all(|(a, b)| (*a - b).abs() <= 1.0 / 255.0));
    let data = fixture::page(
        "/A gs /X Do",
        "/ExtGState << /A 5 0 R >> /XObject << /X 6 0 R >>",
        "",
        &[
            b"<< /ca 0.5 >>".to_vec(),
            fixture::stream(
                "/Type /XObject /Subtype /Form /BBox [0 0 64 64] /Resources << >> /Group << /S /Transparency /I true >>",
                b"1 0 0 rg 0 0 48 64 re f 0 0 1 rg 16 0 48 64 re f",
            ),
        ],
    );
    let r = render(&data);
    assert!((r.get(32, 32)[3] - 0.5).abs() <= 1.0 / 255.0, "{:?}", r.get(32, 32));
}
#[test]
fn pdf_import_limits_cancellation_encryption_and_recursive_forms() {
    let bytes = fixture::artwork();
    let options = ImportOptions { limits: photocraft_doc::vector_design::exchange::ImportLimits { bytes: 10, ..Default::default() }, ..Default::default() };
    assert!(matches!(import(&bytes, &options), Err(ImportError::Limit(_))));
    assert!(matches!(import_with(&bytes, &ImportOptions::default(), &PdfOptions::default(), &AtomicBool::new(true)), Err(ImportError::Cancelled)));
    for dpi in [f32::NAN, f32::INFINITY, 0.0, 3000.0] {
        assert!(import(&bytes, &ImportOptions { dpi, ..Default::default() }).is_err());
    }
    let encrypted = fixture::pdf(&[b"<< /Type /Catalog /Pages 2 0 R /Encrypt 3 0 R >>".to_vec()]);
    assert!(matches!(import(&encrypted, &ImportOptions::default()), Err(ImportError::Encrypted)));
    let recursive = fixture::page(
        "/X Do",
        "/XObject << /X 5 0 R >>",
        "",
        &[fixture::stream("/Type /XObject /Subtype /Form /BBox [0 0 64 64] /Resources << /XObject << /X 5 0 R >> >>", b"/X Do")],
    );
    assert!(import(&recursive, &ImportOptions::default()).is_err());
}
#[test]
fn pdf_import_deflate_bomb_and_sparse_xref_are_rejected_before_parser() {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(&vec![b' '; 1 << 20]).unwrap();
    let zipped = encoder.finish().unwrap();
    let data = fixture::pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Contents 4 0 R >>".to_vec(),
        fixture::stream("/Filter /FlateDecode", &zipped),
    ]);
    let options = ImportOptions {
        limits: photocraft_doc::vector_design::exchange::ImportLimits { decompressed_bytes: 1024, ..Default::default() },
        ..Default::default()
    };
    assert!(matches!(import(&data, &options), Err(ImportError::Limit(_))));
    let data = b"%PDF-1.7\n1 0 obj << /Type /XRef /Size 4294967295 /Length 0 >> stream\n\nendstream\nendobj";
    assert!(matches!(import(data, &ImportOptions::default()), Err(ImportError::Limit(_))));
}
proptest::proptest! {#![proptest_config(proptest::test_runner::Config::with_cases(256))]
    #[test]
    fn pdf_import_random_hostile_bytes_do_not_panic(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(),0..4096)) {
        let wrapped = fixture::pdf(&[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << >> /Contents 4 0 R >>".to_vec(),
            fixture::stream("", &bytes),
        ]);
        for input in [&bytes, &wrapped] {
            let result = std::panic::catch_unwind(|| import(input, &ImportOptions::default()));
            proptest::prop_assert!(result.is_ok());
        }
    }
}
#[test]
fn pdf_import_tiling_and_mesh_have_safe_raster_fallback_warnings() {
    let bytes = fixture::page(
        "/Pattern cs /P scn 0 0 64 64 re f",
        "/Pattern << /P 5 0 R >>",
        "",
        &[fixture::stream(
            "/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 8 8] /XStep 8 /YStep 8 /Resources << >>",
            b"1 0 0 rg 0 0 4 8 re f",
        )],
    );
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    assert!(scene.warnings.iter().any(|w| w.feature == "raster-fallback"));
    let r = photocraft_compose::flatten(&doc(scene));
    assert_eq!(r.get(1, 20), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(r.get(6, 20), [0.0; 4]);
    assert_eq!(r.get(9, 20), [1.0, 0.0, 0.0, 1.0]);
    let mesh = vec![0, 0, 0, 0, 0, 255, 0, 0, 0, 255, 255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 255, 0, 0, 255];
    let bytes = fixture::page(
        "/G sh",
        "/Shading << /G 5 0 R >>",
        "",
        &[fixture::stream(
            "/ShadingType 4 /ColorSpace /DeviceRGB /BitsPerCoordinate 16 /BitsPerComponent 8 /BitsPerFlag 8 /Decode [0 64 0 64 0 1 0 1 0 1]",
            &mesh,
        )],
    );
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    assert!(scene.warnings.iter().any(|w| w.feature == "raster-fallback"));
    let r = photocraft_compose::flatten(&doc(scene));
    let p = r.get(16, 48);
    assert!((p[0] - 0.5).abs() <= 1.0 / 255.0, "{p:?}");
    assert!((p[1] - 16.5 / 64.0).abs() <= 1.0 / 255.0);
    assert!((p[2] - 15.5 / 64.0).abs() <= 1.0 / 255.0);
    assert_eq!(p[3], 1.0);
}
#[test]
fn pdf_import_export21_public_api_roundtrip_and_embedded_font_curves() {
    use photocraft_doc::vector_design::exchange::ExportScope;
    use photocraft_doc::{Color, Fill, Layer, ShapeLayer, TextLayer};
    use photocraft_pdf::export::{ExportOptions, export};
    let mut original = Document::new("Original", Size::new(64, 64), ColorMode::Rgb, SampleType::F32);
    let mut sh =
        ShapeLayer { path: photocraft_vector::shapes::rect(8.0, 8.0, 48.0, 48.0), fill: Some(Fill::Solid(Color::rgb(1.0, 0.0, 0.0))), ..Default::default() };
    sh.cache = Some(photocraft_vector::render_shape(&sh, original.pixel_format(), original.bounds()));
    original.layers.push(Layer::new("Editable red", LayerContent::Shape(sh)));
    for compress in [false, true] {
        let pdf = export(&original, &ExportScope::Document, &ExportOptions { compress, ..Default::default() }).unwrap();
        let decoded = doc(import(&pdf.bytes, &ImportOptions::default()).unwrap());
        let before = photocraft_compose::flatten(&original);
        let after = photocraft_compose::flatten(&decoded);
        let max = before.px.iter().zip(&after.px).flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs())).fold(0.0_f32, f32::max);
        assert!(max <= 1.0 / 255.0, "21→22 render max {max}");
    }
    let mut text = TextLayer {
        text: "PDF".into(),
        font_family: "Inter".into(),
        size_pt: 14.0,
        transform: photocraft_doc::Affine::translate(4.0, 24.0),
        ..Default::default()
    };
    let mut engine = photocraft_text::TextEngine::new();
    engine.render_layer(&mut text, 72.0, photocraft_doc::PixelFormat::RGBA32F);
    original.layers.push(Layer::new("Type", LayerContent::Text(text)));
    let pdf = export(&original, &ExportScope::Document, &ExportOptions::default()).unwrap();
    let decoded = import(&pdf.bytes, &ImportOptions::default()).unwrap();
    assert!(decoded.warnings.iter().any(|w| w.feature == "glyph-outlines"));
    assert!(!decoded.warnings.iter().any(|w| matches!(w.feature.as_str(), "font-substitution" | "unsupported-font")));
    let d = doc(decoded);
    assert_eq!(d.walk().iter().filter(|(_, _, l)| l.name == "PDF glyph").count(), 3);
    for (_, _, glyph) in d.walk().into_iter().filter(|(_, _, l)| l.name == "PDF glyph") {
        assert!(photocraft_compose::render_layer(glyph, d.bounds()).px.iter().any(|p| p[3] > 0.0), "embedded glyph has no ink");
    }
}
#[test]
fn pdf_import_inherited_recursion_cache_depth_and_dash_budgets() {
    let recursive = fixture::page("/X Do", "/XObject << /X 5 0 R >>", "", &[fixture::stream("/Type /XObject /Subtype /Form /BBox [0 0 64 64]", b"/X Do")]);
    assert!(import(&recursive, &ImportOptions::default()).is_err());
    let bytes = fixture::artwork();
    let limits = photocraft_doc::vector_design::exchange::ImportLimits { decompressed_bytes: 128, ..Default::default() };
    assert!(matches!(import(&bytes, &ImportOptions { limits, ..Default::default() }), Err(ImportError::Limit(_))));
    let data = fixture::page("[0.000000001 0.000000001] 0 d 0 32 m 64 32 l S", "", "", &[]);
    assert!(matches!(import(&data, &ImportOptions::default()), Err(ImportError::Limit(_))));
    let data = fixture::page(&format!("{} 0 0 1 1 re f {}", "q ".repeat(80), "Q ".repeat(80)), "", "", &[]);
    assert!(matches!(import(&data, &ImportOptions::default()), Err(ImportError::Limit(_))));
    let data = fixture::page(
        "/I Do",
        "/XObject << /I 5 0 R >>",
        "",
        &[fixture::stream("/Type /XObject /Subtype /Image /Width 2147483647 /Height 2147483647 /ColorSpace /DeviceRGB /BitsPerComponent 8", &[])],
    );
    assert!(matches!(import(&data, &ImportOptions::default()), Err(ImportError::Limit(_))));
}

#[test]
fn pdf_import_strokes_radial_alpha_masks_and_explicit_refusals() {
    // An axis-aligned 8pt butt stroke covers exactly y=28..36 in PDF space.
    let bytes = fixture::page("1 0 0 RG 8 w 8 32 m 56 32 l S", "", "", &[]);
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    let d = doc(scene);
    assert!(d.walk().iter().any(|(_, _, l)| matches!(&l.content, LayerContent::Shape(sh) if sh.stroke.is_some())));
    let p = photocraft_compose::flatten(&d);
    assert_eq!(p.get(20, 30), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(p.get(20, 26)[3], 0.0);
    let bytes = fixture::page("2 0 0 0.5 0 0 cm 0 w 4 32 m 24 32 l S", "", "", &[]);
    let p = render(&bytes);
    let coverage: f32 = (46..=50).map(|y| p.get(20, y)[3]).sum();
    assert!((coverage - 1.0).abs() <= 1.0 / 255.0, "device-pixel hairline coverage {coverage}");
    let bytes = fixture::page("2 0 0 1 0 0 cm 0 0 1 RG 4 w 4 32 m 24 32 l S", "", "", &[]);
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    assert!(scene.warnings.iter().any(|w| w.feature == "stroke-outline"));
    let bytes = fixture::page("/S sh", "/Shading << /S 5 0 R >>", "", &[b"<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [32 32 0 32 32 32] /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> /Extend [true true] >>".to_vec()]);
    let p = render(&bytes);
    let t = (0.5_f32.hypot(0.5)) / 32.0;
    assert!((p.get(32, 32)[0] - (1.0 - t)).abs() <= 1.0 / 255.0);
    assert!((p.get(32, 32)[2] - t).abs() <= 1.0 / 255.0);
    let bytes = fixture::alpha_masked();
    let p = render(&bytes);
    assert_eq!(p.get(16, 32)[3], 1.0);
    for group in ["/S /Transparency /I true /K true", "/S /Transparency /I false"] {
        let bytes = fixture::page(
            "/X Do",
            "/XObject << /X 5 0 R >>",
            "",
            &[fixture::stream(&format!("/Type /XObject /Subtype /Form /BBox [0 0 64 64] /Resources << >> /Group << {group} >>"), b"0 0 64 64 re f")],
        );
        assert!(matches!(import(&bytes, &ImportOptions::default()), Err(ImportError::Unsupported(_))));
    }
    // Binary resources reused as page content cannot evade the content guard.
    let bytes = fixture::pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << >> /Contents 4 0 R >>".to_vec(),
        fixture::stream("/Length1 2", b"BI"),
    ]);
    assert!(matches!(import(&bytes, &ImportOptions::default()), Err(ImportError::Unsupported(_))));
}

#[test]
fn pdf_import_multiply_and_type3_glyph_artwork_are_editable() {
    let bytes =
        fixture::page("1 0 0 rg 0 0 64 64 re f /A gs 0 0 1 rg 8 8 48 48 re f", "/ExtGState << /A 5 0 R >>", "", &[b"<< /ca 0.5 /BM /Multiply >>".to_vec()]);
    let p = render(&bytes);
    assert!((p.get(20, 20)[0] - 0.5).abs() <= 1.0 / 255.0);
    assert_eq!(p.get(20, 20)[1..], [0.0, 0.0, 1.0]);
    let bytes = fixture::page("BT /F 20 Tf 20 20 Td (A) Tj ET", "/Font << /F 5 0 R >>", "", &[
        b"<< /Type /Font /Subtype /Type3 /FontBBox [0 0 600 700] /FontMatrix [0.001 0 0 0.001 0 0] /FirstChar 65 /LastChar 65 /Widths [600] /Encoding << /Type /Encoding /Differences [65 /a] >> /Resources << >> /CharProcs << /a 6 0 R >> >>".to_vec(),
        fixture::stream("", b"600 0 0 0 600 700 d1 1 0 0 rg 0 0 600 700 re f")
    ]);
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    assert!(scene.warnings.iter().any(|w| w.feature == "type3-glyph"));
    let d = doc(scene);
    assert!(d.walk().iter().any(|(_, _, l)| matches!(l.content, LayerContent::Shape(_))));
    let p = photocraft_compose::flatten(&d);
    assert_eq!(p.get(22, 32)[3], 1.0);
    assert_eq!(p.get(18, 32)[3], 0.0);
}

#[test]
fn pdf_import_exponential_page_tree_is_bounded_before_page_collection() {
    let mut objects = vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()];
    for id in 2..=21 {
        let child = id + 1;
        objects.push(format!("<< /Type /Pages /Kids [{child} 0 R {child} 0 R] /Count 1 >>").into_bytes());
    }
    objects.push(b"<< /Type /Page /MediaBox [0 0 64 64] /Resources << >> >>".to_vec());
    assert!(matches!(import(&fixture::pdf(&objects), &ImportOptions::default()), Err(ImportError::Limit("expanded resource object"))));
}

#[test]
fn pdf_import_same_mask_source_retains_alpha_and_luminosity_modes() {
    let bytes = fixture::page(
        "/A gs 1 0 0 rg 0 0 32 64 re f /B gs 32 0 32 64 re f",
        "/ExtGState << /A 5 0 R /B 7 0 R >>",
        "",
        &[
            b"<< /Type /ExtGState /SMask << /S /Luminosity /G 6 0 R >> >>".to_vec(),
            fixture::stream(
                "/Type /XObject /Subtype /Form /BBox [0 0 64 64] /Resources << >> /Group << /S /Transparency /CS /DeviceRGB /I true >>",
                b"0.5 g 0 0 64 64 re f",
            ),
            b"<< /Type /ExtGState /SMask << /S /Alpha /G 6 0 R >> >>".to_vec(),
        ],
    );
    let p = render(&bytes);
    assert!((p.get(16, 32)[3] - 0.5).abs() <= 1.0 / 255.0);
    assert_eq!(p.get(48, 32)[3], 1.0);
}

#[test]
fn pdf_import_packed_mesh_expansion_is_refused_before_decoder() {
    let data = fixture::page(
        "/M sh",
        "/Shading << /M 5 0 R >>",
        "",
        &[fixture::stream(
            "/ShadingType 4 /ColorSpace /DeviceRGB /BitsPerCoordinate 1 /BitsPerComponent 1 /BitsPerFlag 2 /Decode [0 64 0 64 0 1 0 1 0 1]",
            &[0; 64],
        )],
    );
    let options = ImportOptions {
        limits: photocraft_doc::vector_design::exchange::ImportLimits { decompressed_bytes: 8192, ..Default::default() },
        ..Default::default()
    };
    assert!(matches!(import(&data, &options), Err(ImportError::Limit("decompressed byte"))));
}

#[test]
fn pdf_import_sampled_functions_and_predictor_rows_are_bounded() {
    let data = fixture::page("", "", "", &[fixture::stream("/FunctionType 0 /Domain [0 1] /Range [0 1] /Size [1] /BitsPerSample 0", &[0])]);
    assert!(matches!(import(&data, &ImportOptions::default()), Err(ImportError::Invalid(_))));
    let data = fixture::page("", "", "", &[fixture::stream("/FunctionType 0 /Domain [0 1] /Range [0 1] /Size [16] /BitsPerSample 1", &[0; 16])]);
    let options = ImportOptions {
        limits: photocraft_doc::vector_design::exchange::ImportLimits { decompressed_bytes: 8192, ..Default::default() },
        ..Default::default()
    };
    assert!(matches!(import(&data, &options), Err(ImportError::Limit("decompressed byte"))));
    let data = fixture::page("", "", "", &[fixture::stream("/DecodeParms << /Predictor 12 /Columns 1000000 /Colors 1000000 /BitsPerComponent 8 >>", &[])]);
    assert!(matches!(import(&data, &ImportOptions::default()), Err(ImportError::Limit("predictor row"))));
}

#[test]
fn pdf_unresolved_content_references_are_not_zero_cost() {
    let bytes = fixture::page("/Missing Do", "/XObject << /Missing 99 0 R >>", "", &[]);
    assert!(import(&bytes, &ImportOptions::default()).is_err());
}
#[test]
fn pdf_duplicate_object_identity_is_rejected() {
    let mut bytes = fixture::artwork();
    let at = bytes.windows(4).position(|s| s == b"xref").unwrap();
    // A second root with duplicate identity is an ambiguous update, not an admitted graph.
    bytes.splice(at..at, b"1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".iter().copied());
    assert!(import(&bytes, &ImportOptions::default()).is_err());
}

#[test]
fn pdf_xref_hidden_content_cannot_bypass_preflight() {
    // xref redirects the page content to an object header inside an unused string.
    // Only the actual, lexically admitted page stream may execute.
    let hidden = b"(4 0 obj << /Length 8 >> stream\n/Bad Do\nendstream endobj)".to_vec();
    let mut bytes = fixture::page("1 0 0 rg 0 0 64 64 re f", "", "", &[hidden]);
    let hidden_at = bytes.windows(5).rposition(|s| s == b"4 0 o").unwrap();
    let xref_at = bytes.windows(5).position(|s| s == b"xref\n").unwrap();
    let entry = xref_at + b"xref\n0 6\n".len() + 4 * 20;
    bytes[entry..entry + 10].copy_from_slice(format!("{hidden_at:010}").as_bytes());
    let result = render(&bytes);
    assert_eq!(result.get(16, 16), [1.0, 0.0, 0.0, 1.0]);
}
