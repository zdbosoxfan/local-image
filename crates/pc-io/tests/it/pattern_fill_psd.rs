use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Fill, Layer, LayerContent, Pattern};
use photocraft_geom::{Rect, Size};
use photocraft_raster::Surface;

#[test]
fn pattern_fill_keeps_its_pixels_in_psd_and_layered_tiff() {
    let mut tile = Surface::new(PixelFormat::RGBA8);
    tile.fill_rect(Rect::new(0, 0, 4, 4), &[0.6, 0.6, 0.6, 1.]);
    tile.fill_rect(Rect::new(0, 0, 2, 4), &[0.2, 0.2, 0.2, 1.]);
    let pattern = Pattern::new("stripes", tile, 4, 4);
    for mode in [ColorMode::Rgb, ColorMode::Grayscale, ColorMode::Cmyk, ColorMode::Lab] {
        for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
            let mut doc = Document::new("pattern", Size::new(8, 8), mode, depth);
            doc.patterns.push(pattern.clone());
            doc.layers.push(Layer::new(
                "pattern",
                LayerContent::Fill(Fill::Pattern { name: pattern.name.clone(), id: pattern.id.clone(), scale: 100., angle: 0., link: true, phase: (0., 0.) }),
            ));
            let expected = photocraft_compose::flatten(&doc);
            let extensions: &[&str] = if mode == ColorMode::Lab { &["psd"] } else { &["psd", "tiff"] };
            for &extension in extensions {
                let options = photocraft_io::ExportOptions { tiff_layers: true, ..Default::default() };
                let saved = photocraft_io::export(&doc, extension, &options).unwrap();
                let reopened = photocraft_io::import(extension, &saved.bytes).unwrap().document;
                assert!(matches!(reopened.layers[0].content, LayerContent::Fill(Fill::Pattern { .. })), "{mode:?} {depth:?} {extension}");
                let actual = photocraft_compose::flatten(&reopened);
                for (a, b) in actual.px.iter().zip(&expected.px) {
                    assert!((a[3] - b[3]).abs() < 1. / 255., "{mode:?} {depth:?} {extension}: lost pattern coverage");
                    for c in 0..3 {
                        assert!((a[c] - b[c]).abs() < 0.03, "{mode:?} {depth:?} {extension}: {a:?} != {b:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn vector_masked_pattern_fill_keeps_coverage_through_psd() {
    let mut tile = Surface::new(PixelFormat::RGBA8);
    tile.fill_rect(Rect::new(0, 0, 4, 4), &[0.2, 0.4, 0.6, 1.0]);
    let pattern = Pattern::new("masked", tile, 4, 4);
    for density in [1.0, 0.5] {
        let mut doc = Document::new("masked pattern", Size::new(8, 8), ColorMode::Rgb, SampleType::U8);
        doc.patterns.push(pattern.clone());
        let mut layer = Layer::new(
            "pattern",
            LayerContent::Fill(Fill::Pattern { name: pattern.name.clone(), id: pattern.id.clone(), scale: 100.0, angle: 0.0, link: true, phase: (0.0, 0.0) }),
        );
        let mut mask = photocraft_doc::VectorMask::new(photocraft_doc::Path::new(vec![photocraft_doc::Subpath::polygon(&[
            (2.0, 2.0),
            (6.0, 2.0),
            (6.0, 6.0),
            (2.0, 6.0),
        ])]));
        mask.density = density;
        layer.vector_mask = Some(mask);
        let mut pixel_mask = photocraft_doc::LayerMask::reveal_all();
        pixel_mask.surface.fill_rect(doc.bounds(), &[0.5]);
        layer.mask = Some(pixel_mask);
        doc.layers.push(layer);
        let expected = photocraft_compose::flatten(&doc);
        for _ in 0..2 {
            let saved = photocraft_io::export(&doc, "masked.psd", &Default::default()).unwrap();
            doc = photocraft_io::import("masked.psd", &saved.bytes).unwrap().document;
            let actual = photocraft_compose::flatten(&doc);
            assert!(crate::common::max_diff(&actual.px, &expected.px) <= 1.0 / 255.0, "density {density}: {:?} != {:?}", actual.px[0], expected.px[0]);
        }
    }
}
