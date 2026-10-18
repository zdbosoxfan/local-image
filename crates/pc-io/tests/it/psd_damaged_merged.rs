use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::{Document, Layer};
use photocraft_geom::Size;
use photocraft_io::{ExportOptions, export, import};
use photocraft_psd::{Compression, PsdFile};

#[test]
fn damaged_merged_data_recovers_layers_at_each_depth_and_compression() {
    for extension in ["psd", "psb"] {
        for mode in [ColorMode::Rgb, ColorMode::Grayscale, ColorMode::Cmyk, ColorMode::Lab] {
            for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
                let mut doc = Document::new("recover", Size::new(3, 2), mode, depth);
                let mut layer = Layer::raster("pixels", doc.pixel_format());
                let mut pixel = vec![0.; doc.pixel_format().channels()];
                photocraft_raster::from_rgba_into(&doc.pixel_format(), [1., 0., 0., 1.], &mut pixel);
                layer.surface_mut().unwrap().fill_rect(doc.bounds(), &pixel);
                doc.layers.push(layer);
                let original = export(&doc, extension, &ExportOptions::default()).unwrap().bytes;
                let expected = photocraft_compose::flatten(&import(extension, &original).unwrap().document).px;
                for compression in Compression::ALL {
                    let mut file = PsdFile::from_bytes(&original).unwrap();
                    file.image_data = photocraft_psd::ImageData::encode(compression, &file.decode_merged().unwrap(), &file.header).unwrap();
                    file.image_data.data.truncate(file.image_data.data.len() / 2);
                    let bytes = file.to_bytes().unwrap();
                    assert!(PsdFile::from_bytes(&bytes).is_err());
                    let result = import(extension, &bytes).unwrap();
                    assert_eq!(result.document.layer_count(), 1);
                    assert!(result.warnings.iter().any(|w| w.contains("merged image")));
                    assert_eq!(photocraft_compose::flatten(&result.document).px, expected);
                    let saved = export(&result.document, extension, &ExportOptions::default()).unwrap();
                    PsdFile::from_bytes(&saved.bytes).unwrap().decode_merged().unwrap();
                    file.layer_info = None;
                    assert!(import(extension, &file.to_bytes().unwrap()).is_err());
                }
            }
        }
    }
}

#[test]
fn lost_extra_channels_are_reported_without_losing_layers() {
    let mut file = photocraft_psd::testgen::small(photocraft_psd::Version::Psd, Compression::Raw);
    file.header.channels = 5;
    file.image_data.data.clear();
    let result = import("extra.psd", &file.to_bytes().unwrap()).unwrap();
    assert!(result.document.layer_count() > 0);
    assert!(result.document.channels.is_empty());
    assert!(result.warnings.iter().any(|w| w.contains("alpha/spot channels")));
    assert_eq!(result.warnings.iter().filter(|w| w.contains("merged image")).count(), 1, "{:?}", result.warnings);
}

#[test]
fn empty_groups_recover_but_flattened_only_modes_and_broken_layer_sections_fail() {
    let mut builder = photocraft_psd::PsdBuilder::new(3, 2);
    builder.begin_group(photocraft_psd::GroupSpec::new("empty"));
    builder.end_group().unwrap();
    let mut file = builder.build().unwrap();
    file.image_data.data.clear();
    let result = import("empty.psd", &file.to_bytes().unwrap()).unwrap();
    assert_eq!(result.document.layers[0].name, "empty");
    assert!(result.warnings.iter().any(|w| w.contains("merged image")));
    for mode in [photocraft_psd::ColorMode::Indexed, photocraft_psd::ColorMode::Multichannel, photocraft_psd::ColorMode::Duotone] {
        file.header.color_mode = mode;
        assert!(import("bad.psd", &file.to_bytes().unwrap()).is_err());
    }
    let file = photocraft_psd::testgen::small(photocraft_psd::Version::Psd, Compression::Raw);
    let mut bytes = file.to_bytes().unwrap();
    bytes.truncate(bytes.len() - file.image_data.data.len() - 3);
    assert!(import("broken-layers.psd", &bytes).is_err());
}
