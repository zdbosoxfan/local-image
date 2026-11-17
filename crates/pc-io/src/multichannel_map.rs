//! Multichannel documents <-> PSD colour mode 7.
//!
//! A Multichannel document has no layers: its image is a set of ink (spot) channels, stored in
//! [`Document::channels`] with ink density as the value (1 = solid ink, like every spot channel).
//! A PSD stores the same planes the way Photoshop shows them in the Channels panel: 0 (black) =
//! solid ink, max = paper. Alpha-type channels (DisplayInfo kind 0 / 1) are stored as-is, like the
//! alpha channels of every other mode (max = selected). Channel names come from resources 1006 /
//! 1045 and ink colours from DisplayInfo (1077), exactly as for spot channels in the other modes.

use photocraft_color::{Color, ColorMode, PixelFormat, SampleType};
use photocraft_doc::{AlphaChannel, Document};
use photocraft_geom::Rect;
use photocraft_psd::file::{GlobalLayerMask, LayerInfoPlacement};
use photocraft_psd::resources::{ImageResource, ResolutionInfo, ids, version_info_resource};
use photocraft_psd::{ColorMode as PsdMode, Compression, Header, ImageData, PsdFile, TaggedBlock};
use photocraft_raster::Surface;

use crate::pixels::{encode_be, psd_depth};

/// Grid and guides.
const GRID_AND_GUIDES: u16 = 1032;
/// Alpha identifiers: one u32 per extra channel.
const ALPHA_IDENTIFIERS: u16 = 1053;
/// Resources that only describe layers: target layer (1024), layer groups (1026), layer
/// selection ids (1069) and layer groups enabled (1072). A Multichannel document has no layers.
const LAYER_ONLY_RESOURCES: [u16; 4] = [ids::LAYER_STATE, ids::LAYER_GROUP_INFO, 1069, 1072];

/// Value of sample `i` of a big-endian plane, or `None` past its end.
fn decode(plane: &[u8], i: usize, s: SampleType) -> Option<f32> {
    let start = i.checked_mul(s.bytes())?;
    let bytes = plane.get(start..start.checked_add(s.bytes())?)?;
    Some(match s {
        SampleType::U8 => f32::from(bytes[0]) / 255.0,
        SampleType::U16 => f32::from(u16::from_be_bytes(bytes.try_into().ok()?)) / 65535.0,
        SampleType::F32 => f32::from_be_bytes(bytes.try_into().ok()?),
    })
}

/// Fills `doc` (already in Multichannel mode at the file depth) from the merged planes `all`.
/// Each plane is converted one tile row at a time, so no full-plane float buffer is held. A
/// plane layout that doesn't match the document depth, or a failed allocation, is an error.
pub(crate) fn import(file: &PsdFile, all: &[u8], doc: &mut Document, names: &[String], warnings: &mut Vec<String>) -> Result<(), String> {
    let h = &file.header;
    let depth = doc.depth;
    let (w, hh) = (h.width as usize, h.height as usize);
    let overflow = || "multichannel pixel size overflow".to_string();
    let plane = h.row_bytes().checked_mul(hh).ok_or_else(overflow)?;
    if w.checked_mul(hh).and_then(|n| n.checked_mul(depth.bytes())).ok_or_else(overflow)? != plane {
        return Err("multichannel channel depth does not match its pixel layout".into());
    }
    let fmt = PixelFormat::new(ColorMode::Grayscale, depth, false);
    let band = photocraft_geom::TILE_SIZE as usize;
    for k in 0..usize::from(h.channels) {
        let Some(p) = k.checked_mul(plane).and_then(|start| all.get(start..start.checked_add(plane)?)) else {
            warnings.push(format!("multichannel: channel {} is missing", k + 1));
            break;
        };
        let mut s = Surface::new(fmt);
        for y in (0..hh).step_by(band) {
            let bottom = (y + band).min(hh);
            let mut vals = Vec::new();
            vals.try_reserve_exact(w * (bottom - y)).map_err(|e| format!("not enough memory for multichannel pixels ({e})"))?;
            for i in y * w..bottom * w {
                vals.push(1.0 - decode(p, i, depth).ok_or_else(overflow)?);
            }
            s.try_write_region(Rect::new(0, y as i32, h.width as i32, bottom as i32), &vals).map_err(|e| format!("multichannel pixel allocation: {e}"))?;
        }
        s.prune();
        let name = names.get(k).cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| format!("Channel {}", k + 1));
        // Without DisplayInfo the channels print in black, as Photoshop shows them.
        doc.channels.push(AlphaChannel { spot: Some((Color::BLACK, 0.0)), ..AlphaChannel::new(name, s) });
    }
    if let Some(r) = file.resource(crate::channel_map::DISPLAY_INFO) {
        crate::channel_map::apply_display_info(&r.data, true, 0, &mut doc.channels);
    } else if let Some(r) = file.resource(crate::channel_map::DISPLAY_INFO_OLD) {
        crate::channel_map::apply_display_info(&r.data, false, 0, &mut doc.channels);
    }
    // Alpha-type planes are stored as-is, not as ink: undo the inversion for them.
    for ch in doc.channels.iter_mut().filter(|c| c.spot.is_none()) {
        let mut s = Surface::new(fmt);
        let mut vals = Vec::new();
        vals.try_reserve_exact(w * band.min(hh)).map_err(|e| format!("not enough memory for multichannel pixels ({e})"))?;
        for y in (0..hh).step_by(band) {
            let r = Rect::new(0, y as i32, h.width as i32, (y + band).min(hh) as i32);
            ch.surface.read_region_into(r, &mut vals);
            for v in &mut vals {
                *v = 1.0 - *v;
            }
            s.try_write_region(r, &vals).map_err(|e| format!("multichannel pixel allocation: {e}"))?;
        }
        s.prune();
        ch.surface = s;
    }
    Ok(())
}

/// Writes a Multichannel document as a mode-7 PSD (no layer section; every channel a plane).
/// A file past `size_limit` bytes is written as PSB (see `psd_export::fit_psd_size`).
pub(crate) fn document_to_psd(doc: &Document, force_psb: bool, size_limit: u64) -> (PsdFile, Vec<String>) {
    let mut warnings = Vec::new();
    // Photoshop's Multichannel mode is 8 or 16 bits per channel.
    let sample = match doc.depth {
        SampleType::F32 => {
            warnings.push("32-bit Multichannel written as 16 bits/channel (Photoshop has no 32-bit Multichannel)".into());
            SampleType::U16
        }
        s => s,
    };
    if !doc.layers.is_empty() {
        warnings.push("Multichannel documents have no layers: the layers were not written".into());
    }
    let canvas = doc.bounds();
    let channels: Vec<&AlphaChannel> = doc.channels.iter().take(56).collect();
    if doc.channels.len() > 56 {
        warnings.push("only 56 channels fit in PSD; the rest were dropped".into());
    }
    let mut planes = Vec::new();
    if channels.is_empty() {
        // A PSD needs at least one channel: an empty (paper) plane.
        for _ in 0..canvas.width() as usize * canvas.height() as usize {
            encode_be(1.0, sample, &mut planes);
        }
    }
    for ch in &channels {
        // Inks are stored as tone (0 = solid ink), alpha-type channels as-is.
        let ink = ch.spot.is_some();
        for v in ch.surface.read_region(canvas).chunks_exact(ch.surface.channels()) {
            encode_be(if ink { 1.0 - v[0] } else { v[0] }, sample, &mut planes);
        }
    }
    let big = doc.size.width > 30_000 || doc.size.height > 30_000;
    let version = crate::psd_export::psd_version(doc, force_psb);
    if big && !force_psb {
        warnings.push("document exceeds 30000 px; written as PSB".into());
    }
    let header = Header::new(version, doc.size.width, doc.size.height, channels.len().max(1) as u16, psd_depth(sample), PsdMode::Multichannel);
    let image_data = ImageData::encode(Compression::Rle, &planes, &header)
        .or_else(|_| ImageData::encode(Compression::Raw, &planes, &header))
        .unwrap_or(ImageData { compression: Compression::Raw, data: planes });
    let mut resources = vec![
        ImageResource::new(ids::RESOLUTION_INFO, ResolutionInfo::from_dpi(f64::from(doc.resolution_dpi)).to_bytes()),
        ImageResource::new(ids::GLOBAL_ANGLE, (doc.global_light.angle.round() as i32).to_be_bytes().to_vec()),
        ImageResource::new(ids::GLOBAL_ALTITUDE, (doc.global_light.altitude.round() as i32).to_be_bytes().to_vec()),
    ];
    if !doc.guides.horizontal.is_empty() || !doc.guides.vertical.is_empty() {
        resources.push(ImageResource::new(GRID_AND_GUIDES, crate::psd_export::guides_resource(doc)));
    }
    if !channels.is_empty() {
        let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
        resources.push(ImageResource::new(1006, crate::psd_export::pascal_names_resource(&names)));
        resources.push(ImageResource::new(1045, crate::psd_export::unicode_names_resource(&names)));
        resources.push(ImageResource::new(crate::channel_map::DISPLAY_INFO, crate::channel_map::display_info(&channels, false)));
    }
    resources.extend(crate::psd_export::path_resources(doc, &mut warnings));
    if let Some(x) = &doc.metadata.xmp {
        // The pixels are saved as they are shown: never let a reader rotate them again.
        resources.push(ImageResource::new(ids::XMP, lightcraft_meta::upright_xmp(x).as_bytes().to_vec()));
    }
    if let Some(e) = &doc.metadata.exif {
        resources.push(ImageResource::new(ids::EXIF, lightcraft_tiff::exif::upright(e).into_owned()));
    }
    // Every other imported resource (print settings, IPTC, the layer-id seed, ...), except the
    // ones that only describe layers and any already written above.
    let written: Vec<u16> = resources.iter().map(|r| r.id).collect();
    for (id, name, data) in &doc.metadata.psd_resources {
        if LAYER_ONLY_RESOURCES.contains(id) || written.contains(id) || !crate::psd_export::keep_imported_resource(doc, *id, data) {
            continue;
        }
        // Alpha identifiers: one u32 per channel, only while they still match the channels.
        if *id == ALPHA_IDENTIFIERS && data.len() != channels.len().saturating_mul(4) {
            continue;
        }
        let mut r = ImageResource::new(*id, data.to_vec());
        r.name = crate::psd_export::legacy_name(name);
        resources.push(r);
    }
    if let Some(data) = crate::annotations_map::fresh_scale_resource(doc) {
        resources.push(ImageResource::new(crate::annotations_map::MEASUREMENT_SCALE, data));
    }
    resources.push(version_info_resource(true));
    // Document-level tagged blocks (patterns, notes, ...) are valid without a layer section; a
    // kept layer block would describe layers the document doesn't have.
    let global_blocks: Vec<TaggedBlock> = crate::annotations_map::export_blocks(doc, crate::pattern_map::export_global_blocks(doc))
        .into_iter()
        .filter(|(_, key, _)| !matches!(key, b"Layr" | b"Lr16" | b"Lr32"))
        .map(|(sig, key, data)| crate::psd_export::global_block(sig, key, &data))
        .collect();
    let color_mode_data = photocraft_psd::hdr::color_mode_data_for_depth(header.depth);
    let mut file = PsdFile {
        header,
        color_mode_data,
        resources,
        layer_info: None,
        layer_info_placement: LayerInfoPlacement::Section,
        global_layer_mask: (!global_blocks.is_empty()).then(GlobalLayerMask::default),
        global_blocks,
        layer_mask_trailing: Vec::new(),
        image_data,
    };
    warnings.extend(crate::psd_export::fit_psd_size(&mut file, size_limit));
    (file, warnings)
}

#[cfg(test)]
mod import_allocation_tests {
    use super::*;

    #[test]
    fn missing_planes_remain_recoverable_without_reading_past_input() {
        let file = photocraft_psd::testgen::merged_only(photocraft_psd::Version::Psd, PsdMode::Multichannel, 8, Compression::Raw, 3, 2);
        let mut doc = Document::new("Partial", photocraft_geom::Size::new(3, 2), ColorMode::Multichannel, SampleType::U8);
        let mut warnings = Vec::new();
        import(&file, &[0; 6], &mut doc, &[], &mut warnings).unwrap();
        assert_eq!(doc.channels.len(), 1);
        assert_eq!(doc.channels[0].surface.read_region(doc.bounds()), vec![1.0; 6]);
        assert_eq!(warnings.len(), 1);
        assert!(decode(&[], usize::MAX, SampleType::F32).is_none());
    }

    /// A plane narrower than the document depth (here a 1-bit header in an 8-bit document) used
    /// to index past the plane and panic.
    #[test]
    fn a_plane_that_does_not_match_the_depth_is_refused() {
        let mut file = photocraft_psd::testgen::merged_only(photocraft_psd::Version::Psd, PsdMode::Multichannel, 8, Compression::Raw, 16, 2);
        file.header.depth = 1;
        let mut doc = Document::new("Narrow", photocraft_geom::Size::new(16, 2), ColorMode::Multichannel, SampleType::U8);
        let mut warnings = Vec::new();
        assert!(import(&file, &[0; 64], &mut doc, &[], &mut warnings).is_err());
        assert!(doc.channels.is_empty());
    }
}
