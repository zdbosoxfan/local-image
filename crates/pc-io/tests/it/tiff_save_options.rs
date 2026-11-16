//! TIFF Options: Save Transparency on, off and unset, flat and layered, and Save Image Pyramid.
//! Our TIFF writes no alpha or spot channels, so upstream's channel cases are not here.

use crate::common;

use common::*;
use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::{Document, Layer, LayerContent};
use photocraft_geom::{Rect, Size};
use photocraft_io::*;
use photocraft_psd::tiff::ImageSourceData;
use photocraft_raster::Surface;

/// A 4×4 document whose one layer is opaque red in its middle and transparent around it.
fn art(mode: ColorMode, depth: SampleType) -> Document {
    let mut doc = Document::new("t", Size::new(4, 4), mode, depth);
    let fmt = doc.pixel_format();
    let mut s = Surface::new(fmt);
    s.fill_rect(Rect::new(1, 1, 3, 3), &photocraft_raster::from_rgba(&fmt, [1.0, 0.0, 0.0, 1.0]));
    doc.layers.push(Layer::new("Layer 1", LayerContent::Raster(s)));
    doc
}

fn save(doc: &Document, layers: bool, transparency: Option<bool>) -> ExportResult {
    export(doc, "t.tif", &ExportOptions { tiff_layers: layers, tiff_transparency: transparency, ..Default::default() }).expect("export")
}

/// The samples of pixel (0, 0), on the 0–1 scale.
fn corner(img: &photocraft_codecs::Image) -> Vec<f32> {
    let f = img.convert(img.layout(), photocraft_codecs::SampleType::F32);
    f.to_f32_samples().unwrap()[..img.layout().channels()].to_vec()
}

fn layer_alpha(doc: &Document, x: i32, y: i32) -> f32 {
    let LayerContent::Raster(s) = &doc.layers[0].content else { panic!("raster") };
    s.pixel(x, y).last().copied().unwrap_or(1.0)
}

#[test]
fn off_mattes_the_composite_on_white_and_layers_keep_theirs() {
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        for layers in [false, true] {
            let case = format!("{depth:?} layers {layers}");
            let doc = art(ColorMode::Rgb, depth);
            let r = save(&doc, layers, Some(false));
            assert!(!r.warnings.iter().any(|w| w.contains("composited over white")), "{case}: the user chose it: {:?}", r.warnings);
            let img = photocraft_codecs::decode(&r.bytes).unwrap();
            assert!(!img.layout().has_alpha(), "{case}");
            assert!(corner(&img).iter().all(|v| (v - 1.0).abs() < 1e-4), "{case}: transparent corner is white");
            if layers {
                let back = import("t.tif", &r.bytes).unwrap().document;
                assert_eq!(layer_alpha(&back, 0, 0), 0.0, "{case}: the layer keeps its transparency");
                assert_eq!(layer_alpha(&back, 1, 1), 1.0, "{case}");
                let (isd, _) = ImageSourceData::from_bytes(photocraft_codecs::tiff_photoshop_tags(&r.bytes).1.unwrap()).unwrap();
                assert!(!isd.layer_info.unwrap().merged_alpha, "{case}: no merged transparency is claimed");
            }
        }
    }
}

#[test]
fn on_and_unset_keep_the_transparency() {
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        for layers in [false, true] {
            for t in [None, Some(true)] {
                let case = format!("{depth:?} layers {layers} {t:?}");
                let doc = art(ColorMode::Rgb, depth);
                let r = save(&doc, layers, t);
                assert!(!r.warnings.iter().any(|w| w.contains("composited over white")), "{case}: {:?}", r.warnings);
                let img = photocraft_codecs::decode(&r.bytes).unwrap();
                assert!(img.layout().has_alpha(), "{case}");
                assert_eq!(corner(&img).last().copied(), Some(0.0), "{case}");
                let back = import("t.tif", &r.bytes).unwrap().document;
                assert_eq!(layer_alpha(&back, 0, 0), 0.0, "{case}");
                assert_eq!(layer_alpha(&back, 1, 1), 1.0, "{case}");
            }
        }
    }
}

/// Save Transparency off in every colour mode a TIFF holds mattes on the mode's own white (no
/// ink for CMYK, L* 100 with a* and b* at their midpoint for Lab), flat and, where the mode
/// keeps layers, layered; only the transparent pixels change.
#[test]
fn every_colour_mode_mattes_on_its_own_white() {
    for mode in [ColorMode::Rgb, ColorMode::Cmyk, ColorMode::Grayscale, ColorMode::Lab] {
        for depth in [SampleType::U8, SampleType::U16] {
            let mut doc = Document::new("t", Size::new(4, 4), mode, depth);
            let fmt = doc.pixel_format();
            let mut s = Surface::new(fmt);
            s.fill_rect(Rect::new(1, 1, 3, 3), &photocraft_raster::from_rgba(&fmt, [0.2, 0.4, 0.6, 1.0]));
            doc.layers.push(Layer::new("Layer 1", LayerContent::Raster(s)));
            for layers in [false, true] {
                for t in [Some(true), Some(false)] {
                    let case = format!("{mode:?} {depth:?} layers {layers} {t:?}");
                    let r = save(&doc, layers, t);
                    let img = photocraft_codecs::decode(&r.bytes).unwrap_or_else(|e| panic!("{case}: {e}"));
                    assert_eq!(img.layout().has_alpha(), t == Some(true), "{case}");
                    if t == Some(false) {
                        let white: &[f32] = match mode {
                            ColorMode::Cmyk => &[0.0, 0.0, 0.0, 0.0],
                            ColorMode::Lab => &[1.0, 128.0 / 255.0, 128.0 / 255.0],
                            ColorMode::Grayscale => &[1.0],
                            _ => &[1.0, 1.0, 1.0],
                        };
                        let f = img.convert(img.layout(), photocraft_codecs::SampleType::F32);
                        let merged = f.to_f32_samples().unwrap();
                        assert_eq!(img.layout().channels(), white.len(), "{case}");
                        assert!(merged.iter().zip(white).all(|(a, b)| (a - b).abs() < 1e-2), "{case}: merged {:?} vs {white:?}", &merged[..white.len()]);
                        // The opaque centre at (1, 1) (sample 5 of the 4-px-wide image) keeps its colour.
                        let centre = &merged[5 * white.len()..6 * white.len()];
                        assert!(centre.iter().zip(white).any(|(a, b)| (a - b).abs() >= 1e-2), "{case}: centre {centre:?} is matte white");
                    }
                }
            }
        }
    }
}

#[test]
fn pyramid_levels_follow_the_full_image_and_open_the_same_document() {
    // Big enough to need a level: 100 px halves to 50.
    let mut doc = Document::new("t", Size::new(100, 100), ColorMode::Rgb, SampleType::U8);
    let fmt = doc.pixel_format();
    let mut s = Surface::new(fmt);
    s.fill_rect(Rect::new(10, 10, 60, 30), &photocraft_raster::from_rgba(&fmt, [1.0, 0.5, 0.0, 1.0]));
    doc.layers.push(Layer::new("Layer 1", LayerContent::Raster(s)));
    for layers in [false, true] {
        let opts = |pyramid| ExportOptions {
            tiff_layers: layers,
            encode: photocraft_codecs::EncodeOptions { tiff_pyramid: pyramid, ..Default::default() },
            ..Default::default()
        };
        let with = export(&doc, "t.tif", &opts(true)).unwrap();
        let without = export(&doc, "t.tif", &opts(false)).unwrap();
        let info = photocraft_codecs::tiff_info(&with.bytes).unwrap();
        let kinds: Vec<_> = info.pages.iter().map(|p| (p.kind, p.width)).collect();
        assert_eq!(kinds, [(photocraft_codecs::TiffPageKind::Page, 100), (photocraft_codecs::TiffPageKind::ReducedResolution, 50)], "layers {layers}");
        // The level holds the image at half size: level (15, 10) covers full-size (30..32, 20..22),
        // inside the orange fill.
        let level = photocraft_codecs::decode_tiff_page(&with.bytes, 1, &photocraft_codecs::DecodeOptions::default()).unwrap();
        assert_eq!(level.dimensions(), (50, 50), "layers {layers}");
        let level = level.convert(level.layout(), photocraft_codecs::SampleType::F32);
        let px = level.to_f32_samples().unwrap();
        let i = (10 * 50 + 15) * level.layout().channels();
        for (c, want) in [1.0f32, 0.5, 0.0].iter().enumerate() {
            assert!((px[i + c] - want).abs() < 0.01, "layers {layers} channel {c}: {}", px[i + c]);
        }
        let a = import("t.tif", &with.bytes).unwrap();
        let b = import("t.tif", &without.bytes).unwrap();
        assert!(!a.warnings.iter().any(|w| w.contains("page")), "layers {layers}: {:?}", a.warnings);
        assert_docs_eq(&b.document, &a.document);
    }
}
