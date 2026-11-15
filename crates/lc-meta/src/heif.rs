//! HEIF / HEIC / AVIF metadata: the `Exif` and XMP items of the ISO base media file container
//! (box structure from ISO/IEC 14496-12, item model from ISO/IEC 23008-12).
//!
//! The `meta` box holds an item information box (`iinf`, one `infe` per item) naming each item's
//! type, and an item location box (`iloc`) saying where its bytes are: in the file, or in the
//! `idat` box next to it. The `Exif` item is a four-byte offset followed by a TIFF stream; the XMP
//! packet is a `mime` item whose content type is `application/rdf+xml`.
//!
//! Only the container is walked, so no HEVC decoder is needed. The read is best-effort: an item
//! that can't be located is left out, not reported, so [`crate::reads_container`] does not count
//! HEIF as read.
//!
//! Every size and offset is checked against the buffer; box counts, extents and item sizes are bounded.

use crate::container::Embedded;
use crate::cr3::{BoxRef, MAX_BOXES, be16, be32, be64, boxes};

/// Brands that declare a still image in the ISO base media container.
const IMAGE_BRANDS: [&[u8]; 12] = [b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs", b"mif1", b"msf1", b"avif", b"avis"];

/// Longest `ftyp` box brands are read from.
const MAX_FTYP: usize = 4096;
/// Most extents one item's bytes are split into.
const MAX_EXTENTS: u16 = 1024;
/// Largest metadata item returned; a real photo's Exif and XMP are a few hundred kilobytes.
const MAX_ITEM: usize = 32 << 20;

/// Does this file declare an ISO base media still image (HEIF / HEIC / AVIF)?
fn is_heif(bytes: &[u8]) -> bool {
    if bytes.get(4..8) != Some(b"ftyp".as_slice()) {
        return false;
    }
    let Some(size) = be32(bytes, 0).map(|v| v as usize) else {
        return false;
    };
    let end = size.min(bytes.len()).min(MAX_FTYP);
    let brand = |at: usize| bytes.get(at..at.saturating_add(4)).is_some_and(|b| IMAGE_BRANDS.contains(&b));
    // the major brand, then (after the minor version) the compatible brands
    brand(8) || (16..end).step_by(4).any(brand)
}

/// The Exif and XMP blocks of a HEIF file; empty for any other file.
pub(crate) fn metadata(bytes: &[u8]) -> Embedded {
    let mut out = Embedded::default();
    if !is_heif(bytes) {
        return out;
    }
    let mut budget = MAX_BOXES;
    let Some(meta) = boxes(bytes, 0, bytes.len(), &mut budget).into_iter().find(|b| &b.kind == b"meta") else {
        return out;
    };
    // `meta` is a FullBox: a version and flags before its children
    let (mut iinf, mut iloc, mut idat) = (None, None, None);
    for b in boxes(bytes, meta.body.saturating_add(4), meta.end, &mut budget) {
        match &b.kind {
            b"iinf" => iinf = Some(b),
            b"iloc" => iloc = bytes.get(b.body..b.end),
            b"idat" => idat = bytes.get(b.body..b.end),
            _ => {}
        }
    }
    let (Some(iinf), Some(iloc)) = (iinf, iloc) else {
        return out;
    };
    let (exif, xmp) = metadata_items(bytes, &iinf, &mut budget);
    let item = |id: Option<u32>| item_bytes(bytes, idat, &item_location(iloc, id?)?);
    // ISO/IEC 23008-12 A.2.1: a four-byte offset from the end of itself to the TIFF header
    out.exif = item(exif).and_then(|block| {
        let tiff_at = usize::try_from(be32(&block, 0)?).ok()?.checked_add(4)?;
        block.get(tiff_at..).filter(|tiff| !tiff.is_empty()).map(<[u8]>::to_vec)
    });
    out.xmp = item(xmp).map(|block| String::from_utf8_lossy(&block).trim_end_matches('\0').to_owned());
    out
}

/// The ids of the first `Exif` item and of the first XMP item in `iinf`. Entries of version 0 and
/// 1 name no item type and are skipped; HEIF files use version 2 or 3.
fn metadata_items(bytes: &[u8], iinf: &BoxRef, budget: &mut usize) -> (Option<u32>, Option<u32>) {
    // FullBox version and flags, then a 16- or 32-bit entry count before the `infe` boxes
    let entries_at = iinf.body.saturating_add(if bytes.get(iinf.body) == Some(&0) { 6 } else { 8 });
    let (mut exif, mut xmp) = (None, None);
    for b in boxes(bytes, entries_at, iinf.end, budget) {
        let Some(e) = bytes.get(b.body..b.end).filter(|_| &b.kind == b"infe") else { continue };
        let (id, kind_at) = match e.first().copied() {
            Some(2) => (be16(e, 4).map(u32::from), 8),
            Some(3) => (be32(e, 4), 10),
            _ => continue,
        };
        let (Some(id), Some(kind)) = (id, e.get(kind_at..kind_at + 4)) else { continue };
        // the item name, then (for a `mime` item) the content type, both NUL-terminated
        let content_type = e.get(kind_at + 4..).and_then(|rest| rest.split(|&c| c == 0).nth(1));
        if kind == b"Exif" {
            exif = exif.or(Some(id));
        } else if kind == b"mime" && content_type == Some(b"application/rdf+xml".as_slice()) {
            xmp = xmp.or(Some(id));
        }
    }
    (exif, xmp)
}

/// Where one item's bytes are.
struct Loc {
    /// 0 = offsets into the file, 1 = into the `idat` box, 2 = another item (not read).
    construction: u8,
    base: u64,
    /// `(offset, length)` per extent, in order.
    extents: Vec<(u64, u64)>,
}

/// The entry of item `id` in `iloc` (the payload after the box header). `None` when the item has
/// no entry, its bytes are in another file, or the box is malformed up to its entry.
fn item_location(p: &[u8], id: u32) -> Option<Loc> {
    let version = *p.first()?;
    let (sizes, base_index) = (*p.get(4)?, *p.get(5)?);
    let (offset_size, length_size) = (usize::from(sizes >> 4), usize::from(sizes & 0xf));
    let (base_size, index_size) = (usize::from(base_index >> 4), if version > 0 { usize::from(base_index & 0xf) } else { 0 });
    let (count, mut at) = match version {
        0 | 1 => (u32::from(be16(p, 6)?), 8),
        2 => (be32(p, 6)?, 10),
        _ => return None,
    };
    let stride = index_size + offset_size + length_size;
    // Every entry is read before it is skipped, so a count larger than the box ends the walk; an
    // entry advances `at` by under 3 MiB (a 16-bit extent count of 45-byte extents), so it can't overflow.
    for _ in 0..count {
        let (item, after_id) = if version == 2 { (be32(p, at)?, at + 4) } else { (u32::from(be16(p, at)?), at + 2) };
        // versions 1 and 2 put the construction method in the low nibble of a 16-bit field
        let (construction, after_method) = if version == 0 { (0, after_id) } else { ((be16(p, after_id)? & 0xf) as u8, after_id + 2) };
        let data_reference = be16(p, after_method)?;
        let base = uint(p, after_method + 2, base_size)?;
        let extent_count = be16(p, after_method + 2 + base_size)?;
        let extents_at = after_method + 2 + base_size + 2;
        if item != id {
            at = extents_at + usize::from(extent_count) * stride;
            continue;
        }
        // a data reference other than 0 names another file
        if data_reference != 0 || extent_count > MAX_EXTENTS {
            return None;
        }
        let mut extents = Vec::with_capacity(usize::from(extent_count));
        for i in 0..usize::from(extent_count) {
            let offset_at = extents_at + i * stride + index_size;
            extents.push((uint(p, offset_at, offset_size)?, uint(p, offset_at + offset_size, length_size)?));
        }
        return Some(Loc { construction, base, extents });
    }
    None
}

/// A big-endian unsigned integer of `size` bytes; `size` 0 is the value 0, and the item boxes
/// allow no other width than 4 or 8.
fn uint(b: &[u8], at: usize, size: usize) -> Option<u64> {
    match size {
        0 => Some(0),
        4 => be32(b, at).map(u64::from),
        8 => be64(b, at),
        _ => None,
    }
}

/// An item's bytes, its extents concatenated. `None` when they don't fit the file (or the `idat`
/// box), when the item is built by referencing another one, or when it is implausibly large:
/// past [`MAX_ITEM`], or longer than its source, which only extents read more than once can be.
fn item_bytes(bytes: &[u8], idat: Option<&[u8]>, loc: &Loc) -> Option<Vec<u8>> {
    let src = match loc.construction {
        0 => bytes,
        1 => idat?,
        _ => return None,
    };
    let mut out = Vec::new();
    for &(offset, length) in &loc.extents {
        let at = loc.base.checked_add(offset)?;
        let (start, end) = (usize::try_from(at).ok()?, usize::try_from(at.checked_add(length)?).ok()?);
        let extent = src.get(start..end)?;
        if out.len().checked_add(extent.len())? > MAX_ITEM.min(src.len()) {
            return None;
        }
        out.extend_from_slice(extent);
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use crate::exif::tests::sample_exif;
    use lightcraft_tiff::ByteOrder;

    fn bx(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(payload);
        v
    }

    fn full_box(kind: &[u8; 4], version: u8, payload: &[u8]) -> Vec<u8> {
        bx(kind, &[&[version, 0, 0, 0][..], payload].concat())
    }

    struct Item {
        kind: &'static [u8; 4],
        content_type: &'static str,
        data: Vec<u8>,
    }

    /// A HEIC-shaped file holding `items`: `ftyp`, a `meta` box with one `infe` per item, and the
    /// item bytes in `idat` (`in_idat`, construction method 1) or in `mdat` (method 0).
    fn heic(items: &[Item], iloc_version: u8, in_idat: bool) -> Vec<u8> {
        let mut iinf = (items.len() as u16).to_be_bytes().to_vec();
        for (i, it) in items.iter().enumerate() {
            let mut e = vec![2u8, 0, 0, 0]; // version 2, flags
            e.extend_from_slice(&(i as u16 + 1).to_be_bytes());
            e.extend_from_slice(&[0, 0]); // protection index
            e.extend_from_slice(it.kind);
            e.extend_from_slice(b"an item\0");
            if it.kind == b"mime" {
                e.extend_from_slice(it.content_type.as_bytes());
                e.push(0);
            }
            iinf.extend_from_slice(&bx(b"infe", &e));
        }
        let iinf = full_box(b"iinf", 0, &iinf);
        let payload: Vec<u8> = items.iter().flat_map(|i| i.data.iter().copied()).collect();
        let idat = if in_idat { bx(b"idat", &payload) } else { Vec::new() };
        let iloc = |base: u32| {
            // four-byte offsets and lengths, no base offset and no extent index
            let mut l = vec![0x44, 0x00];
            match iloc_version {
                2 => l.extend_from_slice(&(items.len() as u32).to_be_bytes()),
                _ => l.extend_from_slice(&(items.len() as u16).to_be_bytes()),
            }
            let mut at = base;
            for (i, it) in items.iter().enumerate() {
                match iloc_version {
                    2 => l.extend_from_slice(&(i as u32 + 1).to_be_bytes()),
                    _ => l.extend_from_slice(&(i as u16 + 1).to_be_bytes()),
                }
                if iloc_version > 0 {
                    l.extend_from_slice(&[0, u8::from(in_idat)]); // construction method
                }
                l.extend_from_slice(&[0, 0]); // data reference index: this file
                l.extend_from_slice(&[0, 1]); // one extent
                l.extend_from_slice(&at.to_be_bytes());
                l.extend_from_slice(&(it.data.len() as u32).to_be_bytes());
                at += it.data.len() as u32;
            }
            full_box(b"iloc", iloc_version, &l)
        };
        let ftyp = bx(b"ftyp", b"heic\0\0\0\0mif1heic");
        let meta = |base| full_box(b"meta", 0, &[iloc(base), iinf.clone(), idat.clone()].concat());
        if in_idat {
            // the offsets are into `idat`, so they don't depend on where the box lands
            return [ftyp, meta(0)].concat();
        }
        let base = (ftyp.len() + meta(0).len() + 8) as u32;
        [ftyp, meta(base), bx(b"mdat", &payload)].concat()
    }

    /// The items a phone writes, after a `mime` item that is not the packet: the XMP packet, and
    /// an Exif item whose TIFF header sits six bytes on, behind an `Exif\0\0` prefix.
    fn items() -> [Item; 3] {
        let xmp = crate::write_xmp(&crate::Metadata { title: Some("Cusco".into()), ..Default::default() }, None);
        let mut exif = vec![0, 0, 0, 6];
        exif.extend_from_slice(b"Exif\0\0");
        exif.extend(sample_exif(ByteOrder::Little));
        [
            Item { kind: b"mime", content_type: "text/plain", data: b"not the packet".to_vec() },
            Item { kind: b"mime", content_type: "application/rdf+xml", data: xmp.into_bytes() },
            Item { kind: b"Exif", content_type: "", data: exif },
        ]
    }

    /// Upstream LightCraft 6c2ff558: a HEIC yielded no blocks, so it was catalogued with no capture
    /// time (the import fell back to the file's modification time), camera, lens or keywords. The
    /// items are found in every location box version, in `mdat` and in `idat`.
    #[test]
    fn a_heic_is_read_from_its_exif_and_xmp_items() {
        for (iloc_version, in_idat) in [(0, false), (1, false), (1, true), (2, false), (2, true)] {
            let m = crate::extract(&heic(&items(), iloc_version, in_idat));
            let layout = format!("iloc v{iloc_version}, idat {in_idat}");
            assert_eq!(m.capture_time.map(|d| d.to_iso()).as_deref(), Some("2023-07-14T18:30:05.420+02:00"), "{layout}");
            assert_eq!(m.model.as_deref(), Some("Model X"), "{layout}");
            assert_eq!(m.title.as_deref(), Some("Cusco"), "{layout}");
        }
    }

    /// A cut-off or scrambled file never panics, and a box that claims more than it holds is read
    /// within its bytes.
    #[test]
    fn broken_heif_files_are_read_within_bounds_without_panicking() {
        let good = heic(&items(), 1, false);
        for cut in 0..good.len() {
            let _ = crate::embedded(&good[..cut]);
        }
        for at in 0..good.len() {
            let mut bad = good.clone();
            bad[at] ^= 0xff;
            let _ = crate::embedded(&bad);
        }
        // version 1 with 65,535 entries declared and one present: the walk ends where the bytes do
        let one_entry = [1, 0, 0, 0, 0x44, 0, 0xff, 0xff, 0, 1, 0, 0, 0, 0, 0, 0];
        assert!(super::item_location(&one_entry, 9).is_none());
        // extents read twice, to outgrow their source
        let twice = super::Loc { construction: 0, base: 0, extents: vec![(0, good.len() as u64); 2] };
        assert!(super::item_bytes(&good, None, &twice).is_none());
    }
}
