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
