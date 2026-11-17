//! The top-level [`PsdFile`] model, parser and writer.

use crate::Compression;
use crate::compression::{reserve_rle_widening, widen_rle_counts};
use crate::error::{PsdError, Result};
use crate::header::{ColorMode, Header, Version};
use crate::image_data::ImageData;
use crate::io::{Reader, WriteExt};
use crate::layer::{LayerInfo, LayerRecord};
use crate::resources::{self, ImageResource, ResourceData, ids};
use crate::tagged::{TaggedBlock, read_blocks, uses_long_length, write_blocks};

/// Where the layer info was stored in the file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LayerInfoPlacement {
    /// In the layer info field of the layer and mask section (8-bit files).
    #[default]
    Section,
    /// In a global `Lr16` / `Lr32` / `Layr` tagged block (16/32-bit files).
    /// The block is re-inserted at `index` among `global_blocks` on write.
    GlobalBlock {
        /// Position among the global blocks.
        index: usize,
        /// Block signature (`8BIM` / `8B64`).
        signature: [u8; 4],
        /// Block key.
        key: [u8; 4],
        /// Padding after the block (see [`TaggedBlock::padding`]).
        padding: Option<Vec<u8>>,
    },
}

/// Global layer mask info, stored raw with typed accessors.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GlobalLayerMask {
    /// Raw bytes (without the length field).
    pub data: Vec<u8>,
}

impl GlobalLayerMask {
    fn be16(&self, at: usize) -> Option<u16> {
        self.data.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }
    /// Overlay color space.
    pub fn overlay_color_space(&self) -> Option<u16> {
        self.be16(0)
    }
    /// Four color components.
    pub fn color_components(&self) -> Option<[u16; 4]> {
        Some([self.be16(2)?, self.be16(4)?, self.be16(6)?, self.be16(8)?])
    }
    /// Opacity 0..=100.
    pub fn opacity(&self) -> Option<u16> {
        self.be16(10)
    }
    /// Kind: 0 color selected, 1 color protected, 128 per layer.
    pub fn kind(&self) -> Option<u8> {
        self.data.get(12).copied()
    }
}

/// A complete PSD/PSB file.
///
/// `PsdFile::from_bytes(b)?.to_bytes()? == b` for unmodified well-formed
/// files, and `from_bytes(to_bytes(f)) == f` for any model `f`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PsdFile {
    /// Header.
    pub header: Header,
    /// Color mode data (palette for indexed, duotone spec, …).
    pub color_mode_data: Vec<u8>,
    /// Image resources in stored order.
    pub resources: Vec<ImageResource>,
    /// Layer info (`None` when the layer info length is 0 and no
    /// `Lr16`/`Lr32` block holds layers).
    pub layer_info: Option<LayerInfo>,
    /// Where [`Self::layer_info`] is stored.
    pub layer_info_placement: LayerInfoPlacement,
    /// Global layer mask (`None` when the field is absent entirely).
    pub global_layer_mask: Option<GlobalLayerMask>,
    /// Global additional layer information blocks.
    pub global_blocks: Vec<TaggedBlock>,
    /// Bytes at the end of the layer and mask section that do not form a block.
    pub layer_mask_trailing: Vec<u8>,
    /// Merged image data.
    pub image_data: ImageData,
}

impl PsdFile {
    /// Parses a PSD or PSB file.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        Self::from_bytes_recovering(data, |_| false).map(|(file, _)| file)
    }

    /// Accepts damage confined to the merged image when the caller can use the parsed layers.
    /// The returned error must be reported to the user; encoded bytes remain intact.
    /// Missing compression fields and safety-limit failures are always fatal.
    pub fn from_bytes_recovering(data: &[u8], recover: impl FnOnce(&Self) -> bool) -> Result<(Self, Option<PsdError>)> {
        let mut r = Reader::new(data);
        let header = Header::read(&mut r)?;
        let psb = header.version.is_psb();
        let cmd_len = r.u32()?;
        let color_mode_data = r.bytes_u64(u64::from(cmd_len))?.to_vec();
        let resources = resources::read_section(&mut r)?;

        let lm_len = r.len_field(psb)?;
        let mut s = r.sub(lm_len)?;
        let mut layer_info = None;
        let mut global_layer_mask = None;
        let mut global_blocks = Vec::new();
        let mut layer_mask_trailing = Vec::new();
        let mut layer_info_placement = LayerInfoPlacement::Section;
        if !s.is_empty() {
            let li_len = s.len_field(psb)?;
            if li_len > 0 {
                layer_info = Some(LayerInfo::read_body(s.bytes_u64(li_len)?, header.version)?);
            }
            if s.remaining() >= 4 {
                let n = s.u32()?;
                global_layer_mask = Some(GlobalLayerMask { data: s.bytes_u64(u64::from(n))?.to_vec() });
                let (blocks, trailing) = read_blocks(&mut s, header.version)?;
                global_blocks = blocks;
                layer_mask_trailing = trailing;
            } else {
                layer_mask_trailing = s.peek_rest().to_vec();
            }
        }
        // 16/32-bit files keep their layers in a global Lr16/Lr32 block. Photoshop also writes an
        // empty or 8-bit compatibility layer info section in them, which the block overrides.
        let depth_key: Option<&[u8; 4]> = match header.depth {
            16 => Some(b"Lr16"),
            32 => Some(b"Lr32"),
            _ => None,
        };
        let candidate = match depth_key {
            Some(k) => global_blocks.iter().position(|b| &b.key == k),
            None if layer_info.as_ref().is_some_and(|i| !i.layers.is_empty()) => None,
            None => global_blocks.iter().position(|b| matches!(&b.key, b"Lr16" | b"Lr32" | b"Layr")),
        };
        if let Some(i) = candidate {
            match LayerInfo::read_body(&global_blocks[i].data, header.version) {
                Ok(info) => {
                    let b = global_blocks.remove(i);
                    layer_info = Some(info);
                    layer_info_placement = LayerInfoPlacement::GlobalBlock { index: i, signature: b.signature, key: b.key, padding: b.padding };
                }
                // Compatibility records don't decode at 16/32 bits: an unreadable block leaves
                // no layers (the block itself is kept raw).
                Err(_) if depth_key.is_some() => layer_info = None,
                Err(_) => {}
            }
        }
        let mut encoded = r.clone();
        let image_data = ImageData { compression: Compression::Raw, data: Vec::new() };
        let mut file =
            PsdFile { header, color_mode_data, resources, layer_info, layer_info_placement, global_layer_mask, global_blocks, layer_mask_trailing, image_data };
        let error = match ImageData::read(&mut r, &file.header) {
            Ok(image) => {
                file.image_data = image;
                None
            }
            Err(error) => {
                if matches!(error, PsdError::LimitExceeded(_)) || !recover(&file) {
                    return Err(error);
                }
                let compression = Compression::from_u16(encoded.u16()?);
                let source = encoded.peek_rest();
                let mut data = Vec::new();
                data.try_reserve_exact(source.len()).map_err(|_| PsdError::LimitExceeded("not enough memory to retain the damaged merged image"))?;
                data.extend_from_slice(source);
                file.image_data = ImageData { compression, data };
                Some(error)
            }
        };
        Ok((file, error))
    }

    /// Serializes the file.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let v = self.header.version;
        let psb = v.is_psb();
        let mut out = Vec::new();
        self.header.write(&mut out);
        out.put_len(self.color_mode_data.len() as u64, false)?;
        out.put(&self.color_mode_data);
        resources::write_section(&mut out, &self.resources)?;

        let in_block = matches!(self.layer_info_placement, LayerInfoPlacement::GlobalBlock { .. });
        // An entirely empty section is written as a zero length. (A section
        // containing only a zero layer-info length cannot be distinguished in
        // the model and is normalized to this form.)
        let section_empty =
            self.layer_info.is_none() && self.global_layer_mask.is_none() && self.global_blocks.is_empty() && self.layer_mask_trailing.is_empty();
        let at = out.begin_len(psb);
        if !section_empty {
            let li = out.begin_len(psb);
            if let (Some(info), false) = (&self.layer_info, in_block) {
                info.write_body(&mut out, v)?;
            }
            out.end_len(li, psb)?;
            let blocks: std::borrow::Cow<'_, [TaggedBlock]> = match (&self.layer_info, &self.layer_info_placement) {
                (Some(info), LayerInfoPlacement::GlobalBlock { index, signature, key, padding }) => {
                    let mut data = Vec::new();
                    info.write_body(&mut data, v)?;
                    let mut blocks = self.global_blocks.clone();
                    let idx = (*index).min(blocks.len());
                    blocks.insert(idx, TaggedBlock { signature: *signature, key: *key, data, padding: padding.clone() });
                    blocks.into()
                }
                _ => (&self.global_blocks[..]).into(),
            };
            let blocks_ref: &[TaggedBlock] = &blocks;
            let need_mask_field = self.global_layer_mask.is_some() || !blocks_ref.is_empty();
            if need_mask_field {
                let gm = self.global_layer_mask.as_ref().map_or(&[][..], |g| &g.data[..]);
                out.put_len(gm.len() as u64, false)?;
                out.put(gm);
            }
            write_blocks(&mut out, blocks_ref, v)?;
            out.put(&self.layer_mask_trailing);
        }
        out.end_len(at, psb)?;

        out.put_u16(self.image_data.compression.as_u16());
        out.put(&self.image_data.data);
        Ok(out)
    }

    /// The number of bytes [`Self::to_bytes`] writes, computed without serializing the channel
    /// data (only the layer records, which are small, are serialized to measure them). Lets a
    /// writer check a file against the 2 GB PSD limit before choosing the version.
    pub fn encoded_len(&self) -> Result<u64> {
        let v = self.header.version;
        let psb = v.is_psb();
        let len_field = |long: bool| if long { 8u64 } else { 4 };
        let body_len = |info: &LayerInfo| -> Result<u64> {
            let unpadded = info.unpadded_len(v)?;
            let padding = info.padding.as_ref().map_or(unpadded % 2, |p| p.len() as u64);
            Ok(unpadded.saturating_add(padding))
        };
        // Header, colour mode data, resources.
        let mut n = 26u64.saturating_add(4).saturating_add(self.color_mode_data.len() as u64).saturating_add(resources::section_len(&self.resources));

        let in_block = matches!(self.layer_info_placement, LayerInfoPlacement::GlobalBlock { .. });
        let section_empty =
            self.layer_info.is_none() && self.global_layer_mask.is_none() && self.global_blocks.is_empty() && self.layer_mask_trailing.is_empty();
        n = n.saturating_add(len_field(psb));
        if !section_empty {
            n = n.saturating_add(len_field(psb));
            if let (Some(info), false) = (&self.layer_info, in_block) {
                n = n.saturating_add(body_len(info)?);
            }
            let need_mask_field = self.global_layer_mask.is_some() || !self.global_blocks.is_empty() || (in_block && self.layer_info.is_some());
            if need_mask_field {
                n = n.saturating_add(4).saturating_add(self.global_layer_mask.as_ref().map_or(0, |g| g.data.len() as u64));
            }
            n = self.global_blocks.iter().fold(n, |n, b| n.saturating_add(b.encoded_len(v) as u64));
            // The layer block is written as a tagged block: its padding defaults to even.
            if let (Some(info), LayerInfoPlacement::GlobalBlock { key, padding, .. }) = (&self.layer_info, &self.layer_info_placement) {
                let body = body_len(info)?;
                let pad = padding.as_ref().map_or(body % 2, |p| p.len() as u64);
                n = n.saturating_add(8).saturating_add(len_field(uses_long_length(v, key))).saturating_add(body).saturating_add(pad);
            }
            n = n.saturating_add(self.layer_mask_trailing.len() as u64);
        }
        Ok(n.saturating_add(2).saturating_add(self.image_data.data.len() as u64))
    }

    /// Turns a PSD into the equivalent PSB in place, without re-encoding any pixels: the RLE
    /// row-count tables of the layer channels and the merged image widen from two to four bytes
    /// per row (the packed rows are the same in both versions), and the length fields widen when
    /// the file is written. Raw and ZIP channel data is the same in both. A PSB is left as is.
    ///
    /// Atomic: every RLE stream is checked (and the room to widen it reserved) before the first
    /// is changed, so an error leaves the file untouched. The layer info padding is kept; a
    /// writer that pads to an alignment re-pads for the new version.
    pub fn promote_to_psb(&mut self) -> Result<()> {
        if self.header.version.is_psb() {
            return Ok(());
        }
        let merged_rows = match self.image_data.compression {
            Compression::Rle => Some(ImageData::layout(&self.header).rows()?),
            _ => None,
        };
        let layers = self.layer_info.as_mut().map_or(&mut [][..], |info| &mut info.layers[..]);
        let mut streams: Vec<(&mut Vec<u8>, usize)> = Vec::new();
        for layer in layers.iter_mut() {
            let rects: Vec<_> = layer.channels.iter().map(|c| layer.channel_rect(c.id)).collect();
            for (c, rect) in layer.channels.iter_mut().zip(rects) {
                if c.compression == Some(Compression::Rle) {
                    streams.push((&mut c.data, rect.size()?.1));
                }
            }
        }
        if let Some(rows) = merged_rows {
            streams.push((&mut self.image_data.data, rows));
        }
        for (data, rows) in &mut streams {
            reserve_rle_widening(data, *rows)?;
        }
        for (data, rows) in streams {
            widen_rle_counts(data, rows)?;
        }
        self.header.version = Version::Psb;
        Ok(())
    }

    /// Layer records in file order (bottom-most first); empty if none.
    pub fn layers(&self) -> &[LayerRecord] {
        self.layer_info.as_ref().map_or(&[], |i| &i.layers)
    }

    /// Mutable layer records (creates an empty layer info if absent).
    pub fn layers_mut(&mut self) -> &mut Vec<LayerRecord> {
        &mut self.layer_info.get_or_insert_with(LayerInfo::default).layers
    }

    /// First resource with the given id.
    pub fn resource(&self, id: u16) -> Option<&ImageResource> {
        self.resources.iter().find(|r| r.id == id)
    }

    /// First global block with the given key.
    pub fn global_block(&self, key: &[u8; 4]) -> Option<&TaggedBlock> {
        self.global_blocks.iter().find(|b| &b.key == key)
    }

    /// ICC profile bytes (resource 1039).
    pub fn icc_profile(&self) -> Option<&[u8]> {
        self.resource(ids::ICC_PROFILE).map(|r| &r.data[..])
    }

    /// Resolution info (resource 1005).
    pub fn resolution(&self) -> Option<crate::resources::ResolutionInfo> {
        match self.resource(ids::RESOLUTION_INFO)?.parsed()? {
            Ok(ResourceData::ResolutionInfo(r)) => Some(r),
            _ => None,
        }
    }

    /// The "has real merged data" flag of resource 1057, if present.
    pub fn has_real_merged_data(&self) -> Option<bool> {
        match self.resource(ids::VERSION_INFO)?.parsed()? {
            Ok(ResourceData::VersionInfo(v)) => Some(v.has_real_merged_data),
            _ => None,
        }
    }

    /// Decodes the merged image into planar big-endian samples.
    pub fn decode_merged(&self) -> Result<Vec<u8>> {
        self.image_data.decode(&self.header)
    }

    /// `true` if the merged image carries a transparency channel: its first extra channel, when
    /// the layer count was stored negative or a "Saving Merged Transparency" global block (`Mtrn`,
    /// `Mt16`, `Mt32`) is present. Photoshop marks 32-bit documents with `Mt32` and a positive
    /// layer count.
    pub fn merged_has_alpha(&self) -> bool {
        let Some(color) = self.header.color_mode.color_channels() else {
            return false;
        };
        if self.header.channels <= color {
            return false;
        }
        let marked = self.global_blocks.iter().any(|b| matches!(&b.key, b"Mtrn" | b"Mt16" | b"Mt32"));
        match &self.layer_info {
            Some(li) => li.merged_alpha || marked,
            // A flat file from Photoshop names only its alpha and spot channels, while writers of
            // plain RGBA files name nothing: a full list without "Transparency" first means none.
            None => {
                let names = self.alpha_channel_names();
                marked || names.len() != usize::from(self.header.channels - color) || names.first().is_some_and(|n| n == "Transparency")
            }
        }
    }

    /// The alpha channel names: the Unicode names (resource 1045), else the Pascal names (1006).
    /// Reading stops at a truncated entry; empty when the file names no channel.
    pub fn alpha_channel_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Some(r) = self.resource(1045) {
            let mut at = 0usize;
            while let Some(len) = r.data.get(at..at.saturating_add(4)).and_then(|b| <[u8; 4]>::try_from(b).ok()).map(u32::from_be_bytes) {
                at += 4;
                let Some(end) = usize::try_from(len).ok().and_then(|n| n.checked_mul(2)).and_then(|n| at.checked_add(n)) else { break };
                let Some(b) = r.data.get(at..end) else { break };
                let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
                names.push(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string());
                at = end;
            }
        } else if let Some(r) = self.resource(1006) {
            let mut at = 0usize;
            while let Some(&len) = r.data.get(at) {
                let Some(name) = r.data.get(at + 1..at + 1 + usize::from(len)) else { break };
                names.push(String::from_utf8_lossy(name).into_owned());
                at += 1 + usize::from(len);
            }
        }
        names
    }

    /// Checks structural invariants that the writer relies on.
    pub fn validate(&self) -> Result<()> {
        self.header.validate()?;
        if self.header.version == Version::Psd && (self.header.width > 30_000 || self.header.height > 30_000) {
            return Err(PsdError::LimitExceeded("PSD dimensions exceed 30000; use PSB"));
        }
        if self.header.color_mode == ColorMode::Indexed && self.color_mode_data.len() != 768 {
            return Err(PsdError::invalid("indexed color mode requires a 768-byte palette"));
        }
        Ok(())
    }

    /// Reads a file from disk (native targets only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(path: impl AsRef<std::path::Path>) -> std::io::Result<Result<Self>> {
        Ok(Self::from_bytes(&std::fs::read(path)?))
    }

    /// Writes the file to disk (native targets only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let b = self.to_bytes().map_err(std::io::Error::other)?;
        std::fs::write(path, b)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::compression::Compression;
    use crate::testgen;

    /// Photoshop's 16/32-bit files hold an empty or 8-bit compatibility layer info section beside
    /// the global `Lr16`/`Lr32` block that has the real layers: the block wins.
    #[test]
    fn depth_layer_block_overrides_a_compatibility_section() {
        for version in [Version::Psd, Version::Psb] {
            for depth in [16, 32] {
                let file = testgen::layered(version, ColorMode::Rgb, depth, Compression::Raw);
                let key = if depth == 16 { *b"Lr16" } else { *b"Lr32" };
                let mut lr_data = Vec::new();
                file.layer_info.as_ref().unwrap().write_body(&mut lr_data, version).unwrap();
                for compat in [vec![], vec![LayerRecord { name: b"Compatibility".to_vec(), ..Default::default() }]] {
                    let mut with_compat = file.clone();
                    with_compat.layer_info = Some(LayerInfo { layers: compat, ..Default::default() });
                    with_compat.layer_info_placement = LayerInfoPlacement::Section;
                    with_compat.global_blocks.insert(0, TaggedBlock::new(key, lr_data.clone()));
                    let parsed = PsdFile::from_bytes(&with_compat.to_bytes().unwrap()).unwrap();
                    assert_eq!(parsed.layers().len(), file.layers().len(), "{version:?} {depth}");
                    assert_eq!(parsed.layers()[0].name, file.layers()[0].name);
                    assert!(matches!(parsed.layer_info_placement, LayerInfoPlacement::GlobalBlock { key: k, .. } if k == key));
                    assert!(parsed.global_block(&key).is_none(), "the block became the layer info");
                }
            }
        }
    }

    /// Without layer records the extra channels hold a merged transparency
    /// unless the alpha channel names (1045, else 1006) name each of them and none first as
    /// "Transparency".
    #[test]
    fn a_flat_file_names_its_extra_channels() {
        let mut file = testgen::small(Version::Psd, Compression::Raw);
        file.layer_info = None;
        file.header.channels = 5;
        file.resources.retain(|r| r.id != 1045 && r.id != 1006);
        assert!(file.merged_has_alpha(), "nothing named: a plain RGBA file");
        let pascal = |names: &[&str]| names.iter().flat_map(|n| std::iter::once(n.len() as u8).chain(n.bytes())).collect::<Vec<u8>>();
        let unicode = |names: &[&str]| {
            names
                .iter()
                .flat_map(|n| (n.encode_utf16().count() as u32).to_be_bytes().into_iter().chain(n.encode_utf16().flat_map(u16::to_be_bytes)))
                .collect::<Vec<u8>>()
        };
        for (names, transparency) in
            [(&["Spot", "Alpha 1"][..], false), (&["Transparency", "Alpha 1"][..], true), (&["Alpha 1"][..], true), (&["A", "B", "C"][..], true)]
        {
            let mut p = file.clone();
            p.resources.push(ImageResource::new(1006, pascal(names)));
            assert_eq!(p.merged_has_alpha(), transparency, "1006 {names:?}");
            assert_eq!(p.alpha_channel_names(), names, "1006");
            let mut u = file.clone();
            u.resources.push(ImageResource::new(1045, unicode(names)));
            assert_eq!(u.merged_has_alpha(), transparency, "1045 {names:?}");
            assert_eq!(u.alpha_channel_names(), names, "1045");
        }
        // A merged-transparency marker wins over a complete list of names.
        let mut marked = file.clone();
        marked.resources.push(ImageResource::new(1006, pascal(&["Spot", "Alpha 1"])));
        marked.global_blocks.push(TaggedBlock::new(*b"Mt32", Vec::new()));
        assert!(marked.merged_has_alpha());
        // Truncated names stop at the damage.
        let mut cut = file.clone();
        cut.resources.push(ImageResource::new(1006, vec![9, b'a']));
        assert!(cut.alpha_channel_names().is_empty());
        assert!(cut.merged_has_alpha());
    }

    /// `encoded_len` is exactly what `to_bytes` writes, for every generated file and both layer
    /// info placements, with and without padding (#5285).
    #[test]
    fn encoded_len_matches_the_written_length() {
        for case in testgen::all_cases() {
            assert_eq!(case.file.encoded_len().unwrap(), case.file.to_bytes().unwrap().len() as u64, "{}", case.name);
        }
        for version in [Version::Psd, Version::Psb] {
            for key in [*b"Lr16", *b"Layr"] {
                for padding in [None, Some(vec![]), Some(vec![1, 2, 3])] {
                    for body_padding in [None, Some(vec![]), Some(vec![9])] {
                        let mut file = testgen::small(version, Compression::Rle);
                        file.global_blocks = vec![TaggedBlock::new(*b"zz01", vec![11]), TaggedBlock::new(*b"lnk2", vec![1; 5])];
                        file.layer_info_placement = LayerInfoPlacement::GlobalBlock { index: 1, signature: *b"8BIM", key, padding: padding.clone() };
                        file.layer_info.as_mut().unwrap().padding = body_padding;
                        file.layer_mask_trailing = vec![0; 3];
                        file.resources.push(ImageResource { name: b"odd".to_vec(), ..ImageResource::new(4000, vec![1, 2, 3]) });
                        assert_eq!(file.encoded_len().unwrap(), file.to_bytes().unwrap().len() as u64, "{version:?} {key:?}");
                    }
                }
            }
        }
        let mut empty = testgen::small(Version::Psd, Compression::Raw);
        empty.layer_info = None;
        empty.global_layer_mask = None;
        empty.global_blocks.clear();
        assert_eq!(empty.encoded_len().unwrap(), empty.to_bytes().unwrap().len() as u64);
    }

    /// Promoting a PSD gives exactly the PSB written directly from the same pixels: only the RLE
    /// row counts (and the length fields, on write) widen (#5285).
    #[test]
    fn promote_to_psb_equals_the_psb_written_directly() {
        let cases = testgen::all_cases();
        let (psd, psb) = cases.split_at(cases.len() / 2);
        for (psd, psb) in psd.iter().zip(psb) {
            assert_eq!(psd.file.header.version, Version::Psd, "{}", psd.name);
            let mut promoted = psd.file.clone();
            promoted.promote_to_psb().unwrap();
            assert_eq!(promoted, psb.file, "{}", psd.name);
            assert_eq!(promoted.to_bytes().unwrap(), psb.file.to_bytes().unwrap(), "{}", psd.name);
        }
    }

    /// A damaged RLE table is refused before anything changes: the file stays a valid PSD.
    #[test]
    fn promote_to_psb_is_atomic() {
        let mut file = testgen::small(Version::Psd, Compression::Rle);
        // The last layer with pixels ("b", two rows): its red channel loses its row-count table.
        let layers = &mut file.layer_info.as_mut().unwrap().layers;
        let red = layers[2].channels.iter_mut().find(|c| c.id == 0).unwrap();
        assert_eq!(red.compression, Some(Compression::Rle));
        red.data.truncate(1);
        let before = file.clone();
        assert!(file.promote_to_psb().is_err());
        assert_eq!(file, before);
    }
}
