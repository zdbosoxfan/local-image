use photocraft_cms::{
    Builtin,
    synth::{CmykParams, cmyk_profile},
};
use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::{Document, Layer};
use photocraft_geom::Size;
use photocraft_io::{ExportOptions, export};

fn composite(depth: SampleType) -> Document {
    let mut doc = Document::new("inks", Size::new(8, 4), ColorMode::Cmyk, depth);
    for (name, ink) in [("bottom", [0.2, 0.4, 0.1, 0.6, 1.]), ("top", [0.6, 0.1, 0.4, 0.2, 1.])] {
        let mut layer = Layer::raster(name, doc.pixel_format());
        layer.surface_mut().unwrap().fill_rect(doc.bounds(), &ink);
        layer.opacity = 0.5;
        doc.layers.push(layer);
    }
    doc
}

#[test]
fn cmyk_composites_export_inks_and_profile_for_tiff_and_jpeg() {
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        let mut doc = composite(depth);
        let profile =
            cmyk_profile(&CmykParams { description: "Test CMYK".into(), tvi: [0.26, 0.26, 0.26, 0.3], grid_a2b: 5, grid_b2a: 9, ..Default::default() });
        doc.icc_profile = Some(profile.to_bytes());
        for ext in ["tiff", "jpeg"] {
            let encoded = export(&doc, ext, &ExportOptions::default()).unwrap();
            let img = photocraft_codecs::decode(&encoded.bytes).unwrap();
            assert!(img.layout().is_cmyk(), "{ext}");
            assert_eq!(img.icc.as_deref(), doc.icc_profile.as_ref().map(|p| p.as_slice()));
            if ext == "tiff" {
                assert!(img.layout().has_alpha());
                let shown = photocraft_compose::flatten(&doc);
                let space = photocraft_compose::cmyk_space(&doc);
                let expected = photocraft_color::convert::with_cmyk_space(space.as_ref(), || photocraft_raster::from_rgba(&doc.pixel_format(), shown.px[0]));
                for (actual, expected) in img.to_normalized()[..5].iter().zip(expected) {
                    assert!((actual - expected).abs() <= 1. / 255.);
                }
            }
        }
        let png = photocraft_codecs::decode(&export(&doc, "png", &ExportOptions::default()).unwrap().bytes).unwrap();
        assert!(png.layout().is_rgb());
    }
}

#[test]
fn cmyk_composites_reject_invalid_profiles_and_tag_untagged_inks() {
    let mut doc = composite(SampleType::U8);
    let img = photocraft_codecs::decode(&export(&doc, "tiff", &ExportOptions::default()).unwrap().bytes).unwrap();
    assert_eq!(img.icc.as_deref(), Some(Builtin::CoatedCmyk.profile().to_bytes().as_slice()));
    for profile in [std::sync::Arc::new(vec![1, 2, 3]), Builtin::Srgb.profile().to_bytes()] {
        doc.icc_profile = Some(profile);
        assert!(export(&doc, "tiff", &ExportOptions::default()).is_err());
        assert!(export(&doc, "jpeg", &ExportOptions::default()).is_err());
    }
}

#[test]
fn tagged_cmyk_merged_image_reads_through_its_embedded_profile() {
    use photocraft_color::convert::{CmykSpace, cmyk_to_rgb, with_cmyk_space};
    let mut doc = composite(SampleType::U8);
    doc.icc_profile = Some(
        cmyk_profile(&CmykParams { description: "Merged CMYK".into(), tvi: [0.26, 0.26, 0.26, 0.3], grid_a2b: 5, grid_b2a: 9, ..Default::default() })
            .to_bytes(),
    );
    doc.layers.truncate(1);
    doc.layers[0].opacity = 1.;
    let file = photocraft_psd::PsdFile::from_bytes(&export(&doc, "psd", &ExportOptions::default()).unwrap().bytes).unwrap();
    let merged = photocraft_io::merged_composite(&file).unwrap();
    let raw = file.decode_merged().unwrap();
    let space = CmykSpace::for_profile(doc.icc_profile.as_ref());
    let inks = |i| std::array::from_fn(|c| 1. - f32::from(raw[c * 32 + i]) / 255.);
    let expected: Vec<_> = with_cmyk_space(space.as_ref(), || (0..32).map(|i| cmyk_to_rgb(inks(i))).collect());
    for (a, b) in merged.iter().zip(expected) {
        for c in 0..3 {
            assert!((a[c] - b[c]).abs() < 1e-4);
        }
    }
    let builtin = cmyk_to_rgb(inks(0));
    assert!((0..3).any(|c| (merged[0][c] - builtin[c]).abs() > 0.01));
}

#[test]
fn psd_fill_pixels_use_the_embedded_cmyk_profile() {
    use photocraft_doc::{Fill, LayerContent};
    let mut doc = Document::new("tagged fill", Size::new(4, 4), ColorMode::Cmyk, SampleType::U16);
    doc.icc_profile = Some(
        cmyk_profile(&CmykParams { description: "Fill CMYK".into(), tvi: [0.26, 0.26, 0.26, 0.3], grid_a2b: 5, grid_b2a: 9, ..Default::default() }).to_bytes(),
    );
    let color = photocraft_color::Color::rgb(0.2, 0.4, 0.6);
    doc.layers.push(Layer::new("fill", LayerContent::Fill(Fill::Solid(color))));
    let space = photocraft_compose::cmyk_space(&doc);
    let expected = photocraft_color::convert::with_cmyk_space(space.as_ref(), || photocraft_raster::from_rgba(&doc.pixel_format(), [0.2, 0.4, 0.6, 1.0]));
    let saved = export(&doc, "fill.psd", &Default::default()).unwrap();
    let reopened = photocraft_io::import("fill.psd", &saved.bytes).unwrap().document;
    assert_eq!(reopened.icc_profile, doc.icc_profile);
    let actual = reopened.layers[0].fill_cache.as_ref().unwrap().surface.pixel(0, 0);
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((actual - expected).abs() <= 1.0 / 65535.0, "stored ink {actual} != {expected}");
    }
}
