//! Small decoder-facing EXIF queries backed by the shared TIFF directory reader.
//! Full schema-aware metadata editing remains in `lightcraft-meta`.

use crate::directory::{File, Ifd};

/// Metadata needed by image decoders without materializing complete EXIF values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExifSummary {
    /// Valid IFD0 orientation (1–8), when present.
    pub orientation: Option<u16>,
    /// Offset and length of an IFD1 JPEG thumbnail within the TIFF payload.
    pub thumbnail: Option<(usize, usize)>,
    /// EXIF ColorSpace 65535 plus InteropIndex R03 denotes Adobe RGB (1998).
    pub adobe_rgb_hint: bool,
}

/// Read only orientation, JPEG thumbnail location and the Adobe RGB colour-space hint.
/// Accepts a TIFF payload or an EXIF payload prefixed by `Exif\0\0`.
/// Malformed directories or absent tags leave their corresponding hints unset.
pub fn summarize(blob: &[u8]) -> ExifSummary {
    let blob = blob.strip_prefix(b"Exif\0\0").unwrap_or(blob);
    let mut summary = ExifSummary::default();
    let Some(file) = File::parse(blob) else { return summary };
    let Some(ifd0) = file.ifd(file.first_ifd) else { return summary };
    // Match the orientation policy used by pixel codecs: the first duplicate wins, even
    // when malformed. Raw entries are needed because unknown field types are skipped by ifd().
    summary.orientation = file
        .entries_at(file.first_ifd)
        .and_then(|mut entries| entries.find(|entry| entry.tag == 274))
        .filter(|entry| entry.count == 1 && matches!(entry.ty, 3 | 4))
        .and_then(|entry| file.uint(&entry, 0))
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| (1..=8).contains(value));
    if ifd0.next != 0
        && ifd0.next != ifd0.at
        && let Some(ifd1) = file.ifd(ifd0.next)
    {
        summary.thumbnail = thumbnail_extent(&file, &ifd1, blob);
    }
    let child = |ifd: &Ifd, tag| file.tag_uint(ifd, tag).and_then(|offset| file.ifd(offset));
    if let Some(exif) = child(&ifd0, 0x8769)
        && file.tag_uint(&exif, 0xa001) == Some(0xffff)
        && let Some(interop) = child(&exif, 0xa005)
    {
        summary.adobe_rgb_hint = interop.entries.iter().any(|entry| entry.tag == 1 && file.data(entry).is_some_and(|data| data.starts_with(b"R03")));
    }
    summary
}

fn thumbnail_extent(file: &File<'_>, ifd: &Ifd, blob: &[u8]) -> Option<(usize, usize)> {
    let offset = usize::try_from(file.tag_uint(ifd, 0x0201)?).ok()?;
    let length = usize::try_from(file.tag_uint(ifd, 0x0202)?).ok()?;
    let bytes = blob.get(offset..offset.checked_add(length)?)?;
    (length > 4 && bytes.starts_with(&[0xff, 0xd8])).then_some((offset, length))
}

/// Build a tiny EXIF blob (little-endian TIFF) with just an orientation tag. Used by tests and by
/// callers that need to stamp orientation on export when no source EXIF exists.
pub fn minimal_exif(orientation: u16) -> Vec<u8> {
    let mut v = Vec::with_capacity(26);
    v.extend_from_slice(b"II");
    v.extend_from_slice(&42u16.to_le_bytes());
    v.extend_from_slice(&8u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&0x0112u16.to_le_bytes());
    v.extend_from_slice(&3u16.to_le_bytes());
    v.extend_from_slice(&1u32.to_le_bytes());
    v.extend_from_slice(&orientation.to_le_bytes());
    v.extend_from_slice(&[0, 0]);
    v.extend_from_slice(&0u32.to_le_bytes());
    v
}

/// Unlink the old IFD1 thumbnail without relocating any opaque metadata or MakerNote.
/// Returns `false` for an invalid TIFF header or incomplete IFD0; no bytes then change.
/// Accepts TIFF bytes with or without a JPEG `Exif\0\0` prefix.
pub fn drop_thumbnail(exif: &mut [u8]) -> bool {
    let body = exif.strip_prefix(b"Exif\0\0").unwrap_or(exif);
    let prefix = exif.len() - body.len();
    let Some(file) = File::parse(body) else { return false };
    let Some(entries) = file.entries_at(file.first_ifd) else { return false };
    let Ok(next_at) = usize::try_from(entries.next_at) else { return false };
    let Some(start) = prefix.checked_add(next_at) else { return false };
    let Some(end) = start.checked_add(if file.big { 8 } else { 4 }) else { return false };
    let Some(slot) = exif.get_mut(start..end) else { return false };
    slot.fill(0);
    true
}

/// Update IFD0 ImageWidth/ImageLength and Exif PixelXDimension/PixelYDimension in place.
///
/// Existing single SHORT values remain SHORT when the new value fits. Every other dimension
/// entry becomes one inline LONG. Unused inline padding, all offsets, opaque MakerNotes and
/// old out-of-line payloads retain their exact bytes. The first directory and at most three
/// Exif directories are inspected, subject to a file-size entry budget. Every directory and
/// patch is validated before any mutation; an error leaves the entire buffer unchanged.
/// Accepts TIFF bytes with or without a JPEG `Exif\0\0` prefix.
pub fn update_dimensions(exif: &mut [u8], width: u32, height: u32) -> crate::Result<()> {
    use std::collections::BTreeSet;
    use std::ops::Range;

    use crate::{ByteOrder, TiffError};

    fn invalid() -> TiffError {
        TiffError::Invalid("invalid Exif directory; cannot update dimensions safely".into())
    }

    // Store complete patch bytes during preflight, so applying them cannot fail partway.
    fn patch(patches: &mut Vec<(Range<usize>, Vec<u8>)>, data: &[u8], at: usize, size: usize, value: u64, order: ByteOrder) -> crate::Result<()> {
        let end = at.checked_add(size).ok_or_else(invalid)?;
        data.get(at..end).ok_or_else(invalid)?;
        let mut bytes = Vec::with_capacity(size);
        match size {
            2 => order.put_u16(&mut bytes, value as u16),
            4 => order.put_u32(&mut bytes, value as u32),
            _ => order.put_u64(&mut bytes, value),
        }
        patches.push((at..end, bytes));
        Ok(())
    }

    let body = exif.strip_prefix(b"Exif\0\0").unwrap_or(exif);
    let prefix = exif.len() - body.len();
    let file = File::parse(body).ok_or_else(invalid)?;
    let (head, stride, offset_size) = if file.big { (8usize, 20usize, 8usize) } else { (2, 12, 4) };
    let mut remaining = body.len() as u64 / 12 + 64;
    let mut pending = vec![(file.first_ifd, false)];
    let mut visited = BTreeSet::new();
    let mut patches = Vec::new();
    while let Some((at, is_exif)) = pending.pop() {
        if at == 0 || visited.contains(&at) {
            continue;
        }
        if visited.len() >= 4 {
            return Err(TiffError::Limit("Exif directory count"));
        }
        visited.insert(at);
        let count = file.entry_count(at).ok_or_else(invalid)?;
        remaining = remaining.checked_sub(count).ok_or(TiffError::Limit("Exif entry budget"))?;
        let entries = file.entries_at(at).ok_or_else(invalid)?;
        let first = usize::try_from(at).ok().and_then(|at| at.checked_add(head)).ok_or_else(invalid)?;
        for (index, entry) in entries.enumerate() {
            if !is_exif && entry.tag == 34665 {
                if entry.count != 1 || !matches!(entry.ty, 4 | 13) && !(file.big && matches!(entry.ty, 16 | 18)) {
                    return Err(invalid());
                }
                let child = file.uint(&entry, 0).ok_or_else(invalid)?;
                if child != 0 && !visited.contains(&child) && !pending.iter().any(|&(offset, _)| offset == child) {
                    if visited.len() + pending.len() >= 4 {
                        return Err(TiffError::Limit("Exif directory count"));
                    }
                    pending.push((child, true));
                }
            }
            let dimension = match (is_exif, entry.tag) {
                (false, 256) | (true, 40962) => width,
                (false, 257) | (true, 40963) => height,
                _ => continue,
            };
            let position = index.checked_mul(stride).and_then(|n| first.checked_add(n)).and_then(|at| prefix.checked_add(at)).ok_or_else(invalid)?;
            let slot = position.checked_add(stride - offset_size).ok_or_else(invalid)?;
            let short = entry.ty == 3 && entry.count == 1 && dimension <= u32::from(u16::MAX);
            let size = if short { 2 } else { 4 };
            patch(&mut patches, exif, position + 2, 2, if short { 3 } else { 4 }, file.order())?;
            patch(&mut patches, exif, position + 4, stride - offset_size - 4, 1, file.order())?;
            patch(&mut patches, exif, slot, size, u64::from(dimension), file.order())?;
        }
    }
    for (range, bytes) in patches {
        // Every range and length was validated above; applying patches never resizes exif.
        exif[range].copy_from_slice(&bytes);
    }
    Ok(())
}
