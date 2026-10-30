//! Aggregate caps for metadata boxes that can make heic-rs spend unbounded parse work.

use heic_rs::boxes::BoxIter;
use heic_rs::reader::Reader;

use crate::Error;

// heic-rs rescans earlier item ids while parsing ipma associations.
const MAX_IPMA_BYTES: usize = 64 << 10;
// These entries and extent records are allocated even when their fields have zero width.
const MAX_ILOC_ITEMS: usize = 65_536;
const MAX_ILOC_EXTENTS: usize = 65_536;

pub(super) fn cap(bytes: &[u8]) -> Result<(), Error> {
    let mut census = Census::default();
    depth_boxes(&mut census, BoxIter::new(bytes), 0);
    if census.ipma > MAX_IPMA_BYTES {
        return Err(Error::Limit(format!("the image carries {} bytes of property associations, over the {MAX_IPMA_BYTES}-byte cap", census.ipma)));
    }
    if census.iloc_items > MAX_ILOC_ITEMS {
        return Err(Error::Limit(format!("the image declares {} item locations, over the {MAX_ILOC_ITEMS}-item cap", census.iloc_items)));
    }
    if census.iloc_extents > MAX_ILOC_EXTENTS {
        return Err(Error::Limit(format!("the image declares {} item extents, over the {MAX_ILOC_EXTENTS}-extent cap", census.iloc_extents)));
    }
    Ok(())
}

#[derive(Default)]
struct Census {
    ipma: usize,
    iloc_items: usize,
    iloc_extents: usize,
}

// HEIF places the counted boxes under meta and iprp; malformed structure is left for heic-rs.
fn depth_boxes(census: &mut Census, boxes: BoxIter<'_>, depth: u32) {
    if depth > 3 {
        return;
    }
    for result in boxes {
        let Ok(box_header) = result else {
            return;
        };
        if box_header.is(b"ipma") {
            census.ipma = census.ipma.saturating_add(box_header.payload.len());
        } else if box_header.is(b"iloc") {
            if let Some((items, extents)) = iloc_allocations(box_header.payload) {
                census.iloc_items = census.iloc_items.saturating_add(items);
                census.iloc_extents = census.iloc_extents.saturating_add(extents);
            }
        } else if box_header.is(b"meta") {
            if box_header.payload.len() >= 4 {
                depth_boxes(census, BoxIter::new(&box_header.payload[4..]), depth + 1);
            }
        } else if box_header.is(b"iprp") {
            depth_boxes(census, box_header.children(), depth + 1);
        }
    }
}

fn iloc_allocations(payload: &[u8]) -> Option<(usize, usize)> {
    let mut reader = Reader::new(payload);
    let (version, _) = reader.full_box("iloc").ok()?;
    if version > 2 {
        return None;
    }

    let sizes = reader.u8("iloc field sizes").ok()?;
    let offset_size = sizes >> 4;
    let length_size = sizes & 0x0f;
    let sizes2 = reader.u8("iloc field sizes").ok()?;
    let base_offset_size = sizes2 >> 4;
    let index_size = if version >= 1 { sizes2 & 0x0f } else { 0 };
    if ![offset_size, length_size, base_offset_size, index_size].into_iter().all(valid_field_width) {
        return None;
    }

    let item_count = if version < 2 { usize::from(reader.u16("iloc item count").ok()?) } else { usize::try_from(reader.u32("iloc item count").ok()?).ok()? };
    let id_size = if version < 2 { 2 } else { 4 };
    let method_size = if version >= 1 { 2 } else { 0 };
    let minimum_item_size = id_size + method_size + 2 + usize::from(base_offset_size) + 2;
    if item_count.checked_mul(minimum_item_size)? > reader.remaining() {
        return None;
    }
    if item_count > MAX_ILOC_ITEMS {
        return Some((item_count, 0));
    }

    let extent_size = usize::from(offset_size) + usize::from(length_size) + usize::from(index_size);
    let mut extent_count = 0_usize;
    for _ in 0..item_count {
        reader.skip(id_size, "iloc item id").ok()?;
        if version >= 1 {
            reader.skip(2, "iloc construction method").ok()?;
        }
        reader.skip(2, "iloc data reference index").ok()?;
        reader.skip(usize::from(base_offset_size), "iloc base offset").ok()?;
        let count = usize::from(reader.u16("iloc extent count").ok()?);
        reader.skip(count.checked_mul(extent_size)?, "iloc extents").ok()?;
        extent_count = extent_count.saturating_add(count);
        if extent_count > MAX_ILOC_EXTENTS {
            return Some((item_count, extent_count));
        }
    }
    Some((item_count, extent_count))
}

fn valid_field_width(width: u8) -> bool {
    matches!(width, 0 | 4 | 8)
}
