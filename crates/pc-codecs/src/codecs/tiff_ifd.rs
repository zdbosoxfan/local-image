//! A bounds-checked reader for the TIFF directory structure: classic TIFF (version 42, 4-byte
//! offsets) and BigTIFF (version 43, 8-byte offsets and the LONG8 / SLONG8 / IFD8 types), in
//! either byte order.
//!
//! Every offset and count comes from the file and is untrusted: reads go through `get` with
//! checked arithmetic, directory tables must lie wholly inside the file, the IFD chain and the
//! SubIFD trees (tag 330) are walked with a seen-set, a depth limit and a cap on the number of
//! directories, and nothing is allocated in proportion to a declared count before the bytes
//! behind it are known to exist.
//!
//! This module lists the pages of a file ([`TiffInfo`]) and reads the metadata of a directory
//! without decoding pixels; `tiff.rs` decodes the pixels of one directory.

use std::collections::HashSet;

use crate::error::CodecError;
use crate::format::Format;

/// At most this many directories are listed (main chain and SubIFDs together).
pub(crate) const MAX_IFDS: usize = 10_000;
/// SubIFD nesting depth followed by the page listing.
const MAX_SUB_DEPTH: usize = 4;
pub(crate) const TAG_NEW_SUBFILE_TYPE: u16 = 254;
pub(crate) const TAG_SUBFILE_TYPE: u16 = 255;
pub(crate) const TAG_WIDTH: u16 = 256;
pub(crate) const TAG_HEIGHT: u16 = 257;
pub(crate) const TAG_BITS: u16 = 258;
pub(crate) const TAG_COMPRESSION: u16 = 259;
pub(crate) const TAG_PHOTOMETRIC: u16 = 262;
pub(crate) const TAG_ORIENTATION: u16 = 274;
pub(crate) const TAG_SAMPLES: u16 = 277;
pub(crate) const TAG_ROWS_PER_STRIP: u16 = 278;
pub(crate) const TAG_PLANAR: u16 = 284;
pub(crate) const TAG_COLOR_MAP: u16 = 320;
pub(crate) const TAG_TILE_WIDTH: u16 = 322;
pub(crate) const TAG_TILE_LENGTH: u16 = 323;
pub(crate) const TAG_TILE_OFFSETS: u16 = 324;
pub(crate) const TAG_EXTRA_SAMPLES: u16 = 338;
pub(crate) const TAG_SAMPLE_FORMAT: u16 = 339;

/// Tags the `tiff` crate reads to set up decoding. Their values must lie inside the file, so a
/// hostile count can neither make it allocate nor read past the end.
pub(crate) const STRUCTURAL_TAGS: &[u16] = &[256, 257, 258, 259, 262, 273, 277, 278, 279, 284, 317, 322, 323, 324, 325, 338, 339, 347, 530];

// Keep the codec's private names while the shared TIFF backend owns byte parsing.
pub(crate) use lightcraft_tiff::directory::{File, Ifd};

/// A directory's Orientation (1–8), with the rules of [`lightcraft_tiff::exif::orientation`]: the first
/// Orientation entry wins, it must be exactly one SHORT or LONG, and anything else reads as 1.
pub(crate) fn orientation_of(f: &File<'_>, ifd: &Ifd) -> u16 {
    ifd.get(TAG_ORIENTATION)
        .filter(|e| e.count == 1 && matches!(e.ty, 3 | 4))
        .and_then(|e| f.uint(e, 0))
        .and_then(|v| u16::try_from(v).ok())
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1)
}

/// What a TIFF directory holds, from `NewSubfileType` (254) or the older `SubfileType` (255).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TiffPageKind {
    /// A full-resolution image: a page of the document.
    Page,
    /// A reduced-resolution copy of another image (a thumbnail or a pyramid level).
    ReducedResolution,
    /// A transparency mask for another image.
    Mask,
}

/// One image directory of a TIFF file, described without decoding its pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TiffPage {
    /// Byte offset of the directory in the file.
    pub ifd_offset: u64,
    /// For a SubIFD (tag 330): the index (in [`TiffInfo::pages`]) of the directory listing it.
    /// `None` for the directories of the main IFD chain.
    pub parent: Option<usize>,
    pub kind: TiffPageKind,
    /// 0 when the directory has no readable dimensions.
    pub width: u32,
    pub height: u32,
    pub samples_per_pixel: u16,
    pub bits_per_sample: u16,
    /// TIFF `SampleFormat`: 1 unsigned integer, 2 signed integer, 3 IEEE float.
    pub sample_format: u16,
    /// TIFF `PhotometricInterpretation`, when present.
    pub photometric: Option<u16>,
    /// TIFF `Compression` (1 = none).
    pub compression: u16,
    /// Tiles rather than strips.
    pub tiled: bool,
    /// `PlanarConfiguration` 2: each sample in its own plane.
    pub planar: bool,
}

/// The directory structure of a TIFF file: its byte order, BigTIFF or not, and every image
/// directory of the main chain and its SubIFDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TiffInfo {
    pub big_tiff: bool,
    pub little_endian: bool,
    /// Main-chain directories in file order, each followed by its SubIFDs (depth first).
    pub pages: Vec<TiffPage>,
    /// `false` when the walk stopped early: an unreadable directory, a cycle, or more than
    /// 10 000 directories. The pages listed up to that point are still valid.
    pub complete: bool,
}

impl TiffInfo {
    /// The page opened by default, like Photoshop: the first full-resolution directory of the
    /// main chain (reduced-resolution copies and masks are skipped), else the first directory.
    pub fn default_page(&self) -> Option<usize> {
        self.pages
            .iter()
            .position(|p| p.parent.is_none() && p.kind == TiffPageKind::Page && p.width > 0 && p.height > 0)
            .or_else(|| (!self.pages.is_empty()).then_some(0))
    }

    /// Full-resolution pages in the main chain (what Photoshop calls the pages of the file).
    pub fn page_count(&self) -> usize {
        self.pages.iter().filter(|p| p.parent.is_none() && p.kind == TiffPageKind::Page).count()
    }
}

fn describe(f: &File<'_>, ifd: &Ifd, parent: Option<usize>) -> TiffPage {
    let new_kind = f.tag_uint(ifd, TAG_NEW_SUBFILE_TYPE).unwrap_or(0);
    let old_kind = f.tag_uint(ifd, TAG_SUBFILE_TYPE).unwrap_or(1);
    let kind = if new_kind & 4 != 0 {
        TiffPageKind::Mask
    } else if new_kind & 1 != 0 || old_kind == 2 {
        TiffPageKind::ReducedResolution
    } else {
        TiffPageKind::Page
    };
    let u32_of = |tag| f.tag_uint(ifd, tag).and_then(|v| u32::try_from(v).ok()).unwrap_or(0);
    let u16_of = |tag, default| f.tag_uint(ifd, tag).and_then(|v| u16::try_from(v).ok()).unwrap_or(default);
    TiffPage {
        ifd_offset: ifd.at,
        parent,
        kind,
        width: u32_of(TAG_WIDTH),
        height: u32_of(TAG_HEIGHT),
        samples_per_pixel: u16_of(TAG_SAMPLES, 1),
        bits_per_sample: u16_of(TAG_BITS, 1),
        sample_format: u16_of(TAG_SAMPLE_FORMAT, 1),
        photometric: f.tag_uint(ifd, TAG_PHOTOMETRIC).and_then(|v| u16::try_from(v).ok()),
        compression: u16_of(TAG_COMPRESSION, 1),
        tiled: ifd.get(TAG_TILE_OFFSETS).is_some(),
        planar: f.tag_uint(ifd, TAG_PLANAR) == Some(2),
    }
}

struct Walk<'a, 'f> {
    f: &'f File<'a>,
    seen: HashSet<u64>,
    pages: Vec<TiffPage>,
    complete: bool,
    /// Directory entries that may still be parsed. Real directory tables are disjoint, so their
    /// total size is bounded by the file; overlapping tables (a hostile file) would otherwise
    /// make every one of thousands of directories cost a full 65,535-entry parse.
    entry_budget: u64,
}

impl Walk<'_, '_> {
    /// Follows a chain of directories starting at `at`, listing each with `parent` and then
    /// its SubIFDs.
    fn chain(&mut self, mut at: u64, parent: Option<usize>, depth: usize) {
        while at != 0 {
            if self.pages.len() >= MAX_IFDS || !self.seen.insert(at) {
                self.complete = false;
                return;
            }
            let Some(n) = self.f.entry_count(at) else {
                self.complete = false;
                return;
            };
            if n > self.entry_budget {
                self.complete = false;
                return;
            }
            self.entry_budget -= n;
            let Some(ifd) = self.f.ifd(at) else {
                self.complete = false;
                return;
            };
            let index = self.pages.len();
            self.pages.push(describe(self.f, &ifd, parent));
            if depth < MAX_SUB_DEPTH {
                for sub in self.f.sub_ifds(&ifd) {
                    self.chain(sub, Some(index), depth + 1);
                }
            }
            at = ifd.next;
        }
    }
}

/// Lists the directories of a (Big)TIFF file. An error only when the bytes are not a TIFF or
/// the first directory cannot be read; a damaged chain after it ends the list early
/// (`complete == false`).
pub fn tiff_info(bytes: &[u8]) -> Result<TiffInfo, CodecError> {
    let f = File::parse(bytes).ok_or_else(|| CodecError::malformed(Format::Tiff, "not a TIFF or BigTIFF header"))?;
    info_of(&f)
}

pub(crate) fn info_of(f: &File<'_>) -> Result<TiffInfo, CodecError> {
    let mut w = Walk { f, seen: HashSet::new(), pages: Vec::new(), complete: true, entry_budget: (f.b.len() as u64) / 12 + 64 };
    w.chain(f.first_ifd, None, 0);
    if w.pages.is_empty() {
        return Err(CodecError::malformed(Format::Tiff, "the first image directory is missing or cut off"));
    }
    Ok(TiffInfo { big_tiff: f.big, little_endian: f.le, pages: w.pages, complete: w.complete })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_directory_page_adapter_preserves_subifds_and_default_page() {
        use lightcraft_tiff::{ByteOrder, IfdBuilder, TiffWriter, Value};
        for order in [ByteOrder::Little, ByteOrder::Big] {
            for big in [false, true] {
                let image = |width, height| {
                    IfdBuilder::new()
                        .with(TAG_WIDTH, Value::Long(vec![width]))
                        .with(TAG_HEIGHT, Value::Long(vec![height]))
                        .with(TAG_BITS, Value::Short(vec![16]))
                        .with(TAG_ORIENTATION, Value::Short(vec![6]))
                };
                let mut reduced = image(64, 48).with(TAG_NEW_SUBFILE_TYPE, Value::Long(vec![1]));
                reduced.add_sub_ifd(image(32, 24).with(TAG_NEW_SUBFILE_TYPE, Value::Long(vec![4])));
                let bytes = TiffWriter::new(order, big).write(&[reduced, image(640, 480)]).expect("valid fixture");
                let info = tiff_info(&bytes).expect("shared parser accepts writer output");
                assert_eq!(info.big_tiff, big);
                assert_eq!(info.little_endian, order == ByteOrder::Little);
                assert!(info.complete);
                assert_eq!(info.pages.len(), 3);
                assert_eq!(info.pages[0].kind, TiffPageKind::ReducedResolution);
                assert_eq!(info.pages[1].kind, TiffPageKind::Mask);
                assert_eq!(info.pages[1].parent, Some(0));
                assert_eq!((info.pages[2].width, info.pages[2].height, info.pages[2].bits_per_sample), (640, 480, 16));
                assert_eq!(info.default_page(), Some(2));
                assert_eq!(info.page_count(), 1);
                let file = File::parse(&bytes).expect("header");
                let dir = file.ifd(file.first_ifd).expect("directory");
                assert_eq!(orientation_of(&file, &dir), 6);
            }
        }
    }

    #[test]
    fn shared_directory_page_adapter_retains_partial_chain_on_cycle() {
        use lightcraft_tiff::{IfdBuilder, TiffWriter, Value};
        let mut bytes = TiffWriter::default()
            .write(&[IfdBuilder::new().with(TAG_WIDTH, Value::Long(vec![640])).with(TAG_HEIGHT, Value::Long(vec![480]))])
            .expect("valid fixture");
        let file = File::parse(&bytes).expect("header");
        let dir = file.ifd(file.first_ifd).expect("directory");
        let next_at = dir.next_at as usize;
        let cycle = (file.first_ifd as u32).to_le_bytes();
        bytes[next_at..next_at + 4].copy_from_slice(&cycle);
        let info = tiff_info(&bytes).expect("a damaged tail retains its first page");
        assert!(!info.complete);
        assert_eq!(info.pages.len(), 1);
        assert_eq!((info.pages[0].width, info.pages[0].height), (640, 480));
    }

    /// 10,000 directories 4 bytes apart, each declaring 65,535 overlapping entries: the walk must
    /// stop after the file-size entry budget instead of parsing ~6.5e8 entries.
    #[test]
    fn overlapping_directory_tables_exhaust_the_entry_budget() {
        let first = 8usize;
        let table = 2 + 65_535 * 12 + 4;
        let mut b = vec![0u8; first + table + 4 * 10_000];
        b[..8].copy_from_slice(&[b'I', b'I', 42, 0, first as u8, 0, 0, 0]);
        for i in 0..10_000usize {
            let at = first + 4 * i;
            b[at..at + 2].copy_from_slice(&u16::MAX.to_le_bytes());
            let next_cell = at + 2 + 65_535 * 12;
            let next = (first + 4 * (i + 1)) as u32;
            if let Some(cell) = b.get_mut(next_cell..next_cell + 4) {
                cell.copy_from_slice(&next.to_le_bytes());
            }
        }
        let info = tiff_info(&b).expect("the first directory is readable");
        assert!(!info.complete);
        assert!(info.pages.len() < 200, "{} directories parsed", info.pages.len());
    }
}
