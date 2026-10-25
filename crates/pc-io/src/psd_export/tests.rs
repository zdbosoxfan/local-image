use super::*;

fn document(width: u32, height: u32) -> Document {
    Document::new("size estimate", photocraft_geom::Size::new(width, height), ColorMode::Rgb, SampleType::U8)
}

#[test]
fn layer_flags_mark_adjustment_pixels_irrelevant() {
    let mut doc = document(2, 2);
    doc.layers.push(Layer::raster("pixels", doc.pixel_format()));
    doc.layers.push(Layer::new("invert", LayerContent::Adjustment(photocraft_doc::Adjustment::Invert)));
    let file = document_to_psd(&doc);
    let records = file.layers();
    assert_eq!(records[0].flags.0, LayerFlags::BIT4_USEFUL);
    assert_eq!(records[1].flags.0, LayerFlags::BIT4_USEFUL | LayerFlags::PIXEL_DATA_IRRELEVANT);
}

#[test]
fn photo_filter_psd_is_four_byte_aligned_and_round_trips() {
    let color = [60000u16, 30000, 0].map(|x| x as f32 / 65535.0);
    let adjustment = photocraft_doc::Adjustment::PhotoFilter { color, density: 0.14, preserve_luminosity: true };
    let mut doc =
        Document::with_background("Photo Filter PSD", photocraft_geom::Size::new(32, 24), ColorMode::Rgb, SampleType::U8, photocraft_doc::Color::WHITE);
    doc.layers.push(Layer::new("Warming Filter", LayerContent::Adjustment(adjustment.clone())));

    let assert_phfl = |file: &PsdFile| {
        let records = &file.layer_info.as_ref().unwrap().layers;
        let data = records.iter().flat_map(|r| r.blocks.iter()).find(|b| b.key == *b"phfl").map(|b| b.data.as_slice()).expect("Photo Filter adjustment data");
        assert_eq!(data.len(), 20, "PSD layer block length includes four-byte padding");
        assert_eq!(&data[0..2], &[0, 2], "version 2 RGB Photo Filter");
        assert_eq!(&data[17..], &[0, 0, 0], "three padding bytes after the 17-byte payload");
    };

    let exported = document_to_psd(&doc);
    assert_phfl(&exported);
    let encoded = exported.to_bytes().unwrap();
    let parsed = PsdFile::from_bytes(&encoded).unwrap();
    assert_phfl(&parsed);
    let (restored, _) = crate::psd_import::psd_to_document(&parsed).unwrap();
    assert!(restored.layers.iter().any(|l| l.content == LayerContent::Adjustment(adjustment.clone())));
}

#[test]
fn size_limit_boundary_keeps_limit_and_switches_above_it() {
    assert!(!exceeds_psd_size_limit(Some(PSD_MAX_FILE_SIZE)));
    assert!(exceeds_psd_size_limit(Some(PSD_MAX_FILE_SIZE + 1)));
    assert!(exceeds_psd_size_limit(None));
}

#[test]
fn encoded_size_estimator_rejects_arithmetic_overflow() {
    assert_eq!(estimate_encoded_size(u64::MAX, 0, 0), None);
    assert_eq!(estimate_encoded_size(0, u64::MAX, 0), None);
    assert_eq!(estimate_encoded_size(0, 0, u64::MAX), Some(u64::MAX));
    assert!(exceeds_psd_size_limit(estimate_psd_size(&document(u32::MAX, u32::MAX))));
}

#[test]
fn large_encoded_size_selects_psb_without_changing_explicit_choice() {
    let large = document(30_000, 30_000);
    assert!(estimate_psd_size(&large).is_some_and(|size| size > PSD_MAX_FILE_SIZE));
    let large_size_exceeds_limit = psd_size_exceeds_limit(&large);
    assert_eq!(psd_version(&large, false, large_size_exceeds_limit), Version::Psb);

    let small = document(1, 1);
    let small_size_exceeds_limit = psd_size_exceeds_limit(&small);
    assert_eq!(psd_version(&small, false, small_size_exceeds_limit), Version::Psd);
    assert_eq!(psd_version(&small, true, small_size_exceeds_limit), Version::Psb);
}

#[test]
fn kept_psb_blocks_are_written_as_8bim_in_a_psd() {
    // Photoshop 27.11 refuses a PSD holding `8B64` blocks ("not compatible with this
    // version"); they came from re-saving a PSB as PSD with its global FMsk and cinf kept.
    let mut doc = document(4, 4);
    doc.metadata.psd_global_blocks =
        vec![(*b"8B64", *b"FMsk", std::sync::Arc::new(vec![0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 50])), (*b"8BIM", *b"Patt", std::sync::Arc::new(Vec::new()))];
    let signatures = |force_psb| {
        let opts = PsdExportOptions { force_psb, ..PsdExportOptions::default() };
        let bytes = document_to_psd_with(&doc, &opts).0.to_bytes().unwrap();
        let file = PsdFile::from_bytes(&bytes).unwrap();
        file.global_blocks.iter().map(|b| (b.signature, b.key)).collect::<Vec<_>>()
    };
    assert_eq!(signatures(false), vec![(*b"8BIM", *b"FMsk"), (*b"8BIM", *b"Patt")]);
    assert_eq!(signatures(true), vec![(*b"8B64", *b"FMsk"), (*b"8BIM", *b"Patt")], "a PSB keeps 8B64");
}

/// Photoshop opens a PSD's bottom layer as its Background only when that layer has no
/// transparency channel; with the channel written, re-exported files reopened with the
/// Background as an ordinary layer.
#[test]
fn a_background_is_written_without_a_transparency_channel() {
    use photocraft_doc::Color;
    let size = photocraft_geom::Size::new(8, 6);
    let mut doc = Document::with_background("bg", size, ColorMode::Rgb, SampleType::U8, Color::rgb(0.2, 0.4, 0.6));
    doc.layers.push(Layer::raster("Layer 1", doc.pixel_format()));
    let file = document_to_psd(&doc);
    let layers = &file.layer_info.as_ref().unwrap().layers;
    let ids = |i: usize| layers[i].channels.iter().map(|c| c.id).collect::<Vec<_>>();
    assert_eq!(ids(0), vec![0, 1, 2], "the Background has no -1 channel");
    assert!(layers[0].flags.transparency_protected());
    assert!(ids(1).contains(&-1), "other layers keep their transparency");
    // It comes back as the same locked, opaque Background.
    let back = crate::import("bg.psd", &file.to_bytes().unwrap()).unwrap().document;
    let bg = &back.layers[0];
    assert!(bg.locks.transparency && bg.locks.position, "{:?}", bg.locks);
    assert_eq!(bg.surface().unwrap().read_region(back.bounds()), doc.layers[0].surface().unwrap().read_region(doc.bounds()));
    // A bottom layer with transparency (or unlocked) is not a Background.
    let mut holey = doc.clone();
    holey.layers[0].surface_mut().unwrap().write_pixel(2, 2, &[0.0, 0.0, 0.0, 0.0]);
    assert!(document_to_psd(&holey).layer_info.unwrap().layers[0].channels.iter().any(|c| c.id == -1));
    let mut unlocked = doc.clone();
    unlocked.layers[0].locks.position = false;
    assert!(document_to_psd(&unlocked).layer_info.unwrap().layers[0].channels.iter().any(|c| c.id == -1));
}

/// A flat 1-bit Bitmap file (no layers) opens as its locked Background, as in Photoshop, and
/// is written back as one (no transparency channel).
#[test]
fn a_flat_bitmap_file_opens_and_saves_as_a_background() {
    let file = photocraft_psd::testgen::merged_only(Version::Psd, PsdMode::Bitmap, 1, photocraft_psd::Compression::Raw, 8, 4);
    let doc = crate::import("flat.psd", &file.to_bytes().unwrap()).unwrap().document;
    let bg = &doc.layers[0];
    assert!(bg.name == "Background" && bg.locks.transparency && bg.locks.position, "{:?}", bg.locks);
    let out = document_to_psd(&doc);
    assert!(out.layer_info.unwrap().layers[0].channels.iter().all(|c| c.id >= 0));
}

#[test]
fn group_divider_id_wraps_without_colliding_with_imported_layer_ids() {
    let mut doc = document(8, 8);
    let mut group = Layer::group("Imported group", vec![Layer::raster("Child", doc.pixel_format())]);
    group.psd_id = Some(u32::MAX);
    doc.layers.push(group);

    let file = document_to_psd(&doc);
    let ids: Vec<u32> = file.layers().iter().filter_map(|layer| layer.layer_id()).collect();
    let unique: std::collections::HashSet<u32> = ids.iter().copied().collect();
    assert_eq!(ids.len(), 3, "group, child and closing divider");
    assert_eq!(unique.len(), ids.len(), "no generated ID may collide");
    assert!(unique.contains(&u32::MAX), "preserve the imported group ID");
    assert!(unique.contains(&1), "reuse the first unused positive ID on wrap");

    let parsed = PsdFile::from_bytes(&file.to_bytes().unwrap()).unwrap();
    let reloaded_ids: std::collections::HashSet<u32> = parsed.layers().iter().filter_map(|layer| layer.layer_id()).collect();
    assert_eq!(unique, reloaded_ids);
}

/// A layered document with transparency in the merged image and an alpha channel: the name
/// and DisplayInfo resources hold one entry per extra channel, the merged transparency first
/// (one short, Photoshop 27.11 refused the file with "unexpected end-of-file"). The alpha
/// channel keeps its own name and options on the way back.
#[test]
fn alpha_names_count_the_merged_transparency() {
    let mut doc = document(8, 6);
    let mut layer = Layer::raster("Layer 1", doc.pixel_format());
    layer.surface_mut().unwrap().write_pixel(1, 1, &[1.0, 0.0, 0.0, 1.0]);
    doc.layers.push(layer);
    let mut a = photocraft_doc::AlphaChannel::new("Alpha 1", Surface::new(PixelFormat::GRAY8));
    a.opacity = 0.5;
    doc.channels.push(a);
    let file = document_to_psd(&doc);
    let header_extra = usize::from(file.header.channels) - 3;
    let resource = |id| file.resources.iter().find(|r| r.id == id).map(|r| r.data.clone()).unwrap();
    let pascal = resource(1006);
    let (mut count, mut i) = (0, 0);
    while i < pascal.len() {
        count += 1;
        i += 1 + usize::from(pascal[i]);
    }
    assert_eq!((header_extra, count), (2, 2), "transparency + Alpha 1");
    assert_eq!(resource(crate::channel_map::DISPLAY_INFO).len(), 4 + 2 * 13);
    let back = crate::import("a.psd", &file.to_bytes().unwrap()).unwrap().document;
    assert_eq!(back.channels.len(), 1);
    assert_eq!(back.channels[0].name, "Alpha 1");
    assert!((back.channels[0].opacity - 0.5).abs() < 1e-6);
}

/// A 16-bit file whose Lr16 block claims five layers but holds none opens flattened with a
/// warning, and a save with layers writes one layer block, not the kept bad one beside it.
#[test]
fn unreadable_lr16_warns_on_import_and_is_not_saved_twice() {
    let mut doc = Document::new("t", photocraft_geom::Size::new(4, 4), ColorMode::Rgb, SampleType::U16);
    doc.layers.push(Layer::raster("Layer 1", doc.pixel_format()));
    let mut file = document_to_psd(&doc);
    file.layer_info = None;
    file.layer_info_placement = LayerInfoPlacement::Section;
    file.global_blocks.insert(0, TaggedBlock::new(*b"Lr16", vec![0, 5, 0, 0]));
    let opened = crate::import("bad.psd", &file.to_bytes().unwrap()).unwrap();
    assert!(opened.warnings.iter().any(|w| w.contains("Lr16 block")), "{:?}", opened.warnings);
    let mut back = opened.document;
    back.layers.push(Layer::raster("Added", back.pixel_format()));
    let saved = PsdFile::from_bytes(&document_to_psd(&back).to_bytes().unwrap()).unwrap();
    assert!(matches!(saved.layer_info_placement, LayerInfoPlacement::GlobalBlock { key: k, .. } if k == *b"Lr16"));
    assert!(saved.global_block(b"Lr16").is_none(), "the bad block is not kept beside the new one");
}

/// Photoshop won't open a file whose `Lr32` comes before the `Mt32` marker it keeps for a
/// 32-bit document's merged transparency. The layer block follows a kept marker.
#[test]
fn layer_block_follows_a_kept_merged_transparency_marker() {
    for (depth, key, marker) in [(SampleType::F32, *b"Lr32", *b"Mt32"), (SampleType::U16, *b"Lr16", *b"Mt16"), (SampleType::F32, *b"Lr32", *b"Mtrn")] {
        let mut doc = Document::new("t", photocraft_geom::Size::new(8, 6), ColorMode::Rgb, depth);
        let mut s = Surface::new(doc.pixel_format());
        s.fill_rect(photocraft_geom::Rect::new(1, 1, 5, 4), &[0.5, 0.2, 0.9, 1.0]);
        doc.layers.push(Layer::new("Layer 1", LayerContent::Raster(s)));
        doc.metadata.psd_global_blocks = vec![(*b"8BIM", *b"Patt", std::sync::Arc::new(Vec::new())), (*b"8BIM", marker, std::sync::Arc::new(Vec::new()))];
        let saved = PsdFile::from_bytes(&document_to_psd(&doc).to_bytes().unwrap()).unwrap();
        let LayerInfoPlacement::GlobalBlock { index, key: written, .. } = saved.layer_info_placement else { panic!("{depth:?}: a global layer block") };
        assert_eq!(written, key);
        let at = saved.global_blocks.iter().position(|b| b.key == marker).expect("the marker is kept");
        assert!(at < index, "{depth:?}: {marker:?} at {at}, the layer block at {index}");
    }
    // Without a marker the layer block stays first.
    let mut doc = Document::new("t", photocraft_geom::Size::new(8, 6), ColorMode::Rgb, SampleType::F32);
    doc.layers.push(Layer::new("Layer 1", LayerContent::Raster(Surface::new(doc.pixel_format()))));
    doc.metadata.psd_global_blocks = vec![(*b"8BIM", *b"Patt", std::sync::Arc::new(Vec::new()))];
    let saved = PsdFile::from_bytes(&document_to_psd(&doc).to_bytes().unwrap()).unwrap();
    assert!(matches!(saved.layer_info_placement, LayerInfoPlacement::GlobalBlock { index: 0, .. }));
}

/// A temporary file holding `bytes`.
fn temp_file(name: &str, bytes: &[u8]) -> String {
    let path = std::env::temp_dir().join(format!("pc-io-linked-{}-{name}", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path.to_string_lossy().into_owned()
}

/// An image Local Image can open: a file that embeds it carries these exact bytes, while pixels
/// written for a layer don't.
fn png() -> Vec<u8> {
    let image = photocraft_codecs::Image::from_u8(8, 8, photocraft_codecs::ChannelLayout::Rgb, (0..192u8).collect()).unwrap();
    photocraft_codecs::encode(&image, photocraft_codecs::Format::Png, &Default::default()).unwrap()
}

/// A red smart object over an 8×8 RGB canvas, with `source`.
fn smart_layer(source: photocraft_doc::SmartSource) -> Layer {
    let fmt = document(8, 8).pixel_format();
    let mut cache = Surface::new(fmt);
    cache.fill_rect(photocraft_geom::Rect::new(0, 0, 8, 8), &[1.0, 0.0, 0.0, 1.0]);
    Layer::new("smart", LayerContent::Smart(photocraft_doc::SmartObject::new(source, photocraft_geom::Affine::IDENTITY, Some(cache))))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn an_export_that_reads_no_linked_files_writes_linked_smart_objects_as_pixels() {
    let png = png();
    let path = temp_file("pixels.png", &png);
    let mut doc = document(8, 8);
    doc.layers.push(smart_layer(photocraft_doc::SmartSource::Linked { path: path.clone() }));
    for ext in ["psd", "psb", "tif"] {
        let trusted = crate::export(&doc, ext, &crate::ExportOptions { tiff_layers: true, ..Default::default() }).unwrap();
        assert!(contains(&trusted.bytes, &png), "{ext}: a trusted save embeds the linked file");
        let opts = crate::ExportOptions { tiff_layers: true, read_linked_files: false, ..Default::default() };
        let r = crate::export(&doc, ext, &opts).unwrap();
        assert!(!contains(&r.bytes, &png), "{ext}: the linked file is not embedded");
        assert!(r.warnings.iter().any(|w| w.contains("smart object written as pixels")), "{ext}: {:?}", r.warnings);
        let back = crate::import(&format!("back.{ext}"), &r.bytes).unwrap().document;
        let layer = back.layers.iter().find(|l| l.name == "smart").unwrap();
        assert!(matches!(layer.content, LayerContent::Raster(_)), "{ext}: the layer keeps its pixels");
        assert_eq!(layer.surface().unwrap().rgba(3, 3), [1.0, 0.0, 0.0, 1.0], "{ext}");
    }
    std::fs::remove_file(path).unwrap();
}

/// A link that names a device or kernel file is never read: PSD export reads linked files through
/// the bounded reader every open uses (a link to `/dev/zero` would otherwise read until memory
/// runs out).
#[cfg(target_os = "linux")]
#[test]
fn links_to_device_and_kernel_files_are_not_read() {
    for path in ["/proc/self/status", "/dev/null"] {
        let mut doc = document(8, 8);
        doc.layers.push(smart_layer(photocraft_doc::SmartSource::Linked { path: path.into() }));
        let (file, warnings) = document_to_psd_with(&doc, &PsdExportOptions::default());
        assert!(warnings.iter().any(|w| w.contains("smart object written as pixels") && w.contains("can't be read")), "{path}: {warnings:?}");
        assert!(!file.global_blocks.iter().any(|b| &b.key == b"lnk2"), "{path}: nothing is embedded");
    }
}
