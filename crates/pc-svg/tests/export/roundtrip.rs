//! Dependency gate: needs the public SVG importer.
//! This never supplies a substitute importer.
use super::*;
use photocraft_doc::vector_design::exchange::ImportOptions;
fn imported(bytes: &[u8]) -> Document {
    let scene = photocraft_svg::import::import(bytes, &ImportOptions { dpi: 96., ..Default::default() }).unwrap();
    let mut doc = Document::new("Reimport", scene.size, ColorMode::Rgb, SampleType::U8);
    doc.layers = scene.layers;
    doc.resolution_dpi = scene.resolution_dpi;
    doc.design_units = scene.units;
    doc.symbols = scene.symbols;
    doc
}
#[test]
fn svg_export_import_render_iou_gate() {
    for fill in [Fill::Solid(Color::BLACK), Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 31., 0.8, GradientStyle::Linear, false)] {
        let doc = scene(photocraft_vector::shapes::ellipse(24., 24., 208., 208.), fill);
        let bytes = export::export(&doc, &ExportScope::Document, &Default::default()).unwrap().bytes;
        let reimport = imported(&bytes);
        let a = photocraft_compose::flatten(&doc);
        let b = photocraft_compose::flatten(&reimport);
        let union = a.px.iter().zip(&b.px).filter(|(a, b)| a[3] >= 0.5 || b[3] >= 0.5).count();
        let intersection = a.px.iter().zip(&b.px).filter(|(a, b)| a[3] >= 0.5 && b[3] >= 0.5).count();
        let iou = intersection as f64 / union.max(1) as f64;
        eprintln!("SVG import/export IoU={iou}");
        assert!(iou > 0.99);
    }
}
#[test]
fn svg_export_second_roundtrip_is_byte_stable_gate() {
    let mut translucent = Color::BLACK;
    translucent.alpha = 0.25;
    let alpha_gradient = Fill::Gradient {
        method: Default::default(),
        stops: vec![(0., translucent), (1., Color::WHITE)],
        angle: 31.,
        scale: 0.8,
        style: GradientStyle::Linear,
        reverse: true,
        opacity_stops: vec![(0., 0.5), (1., 1.)],
        midpoints: vec![0.3],
        offset: (0.1, -0.05),
        dither: false,
        align: true,
    };
    for fill in [
        Fill::Solid(Color::BLACK),
        alpha_gradient,
        Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 31., 0.8, GradientStyle::Linear, false),
        Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 90., 0.8, GradientStyle::Reflected, false),
        Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 0., 0.8, GradientStyle::Radial, false),
    ] {
        for clip in [false, true] {
            let mut doc = scene(photocraft_vector::shapes::rect(24., 24., 208., 208.), fill.clone());
            if clip {
                doc.layers[0].vector.clip = Some(photocraft_vector::shapes::rect(40., 40., 128., 128.));
            }
            let first = export::export(&doc, &ExportScope::Document, &Default::default()).unwrap();
            let second = export::export(&imported(&first.bytes), &ExportScope::Document, &Default::default()).unwrap();
            let third = export::export(&imported(&second.bytes), &ExportScope::Document, &Default::default()).unwrap();
            assert_eq!(
                std::str::from_utf8(&second.bytes).unwrap(),
                std::str::from_utf8(&third.bytes).unwrap(),
                "normalized second export must be byte stable, clip={clip}, fill={fill:?}"
            );
        }
    }
}
