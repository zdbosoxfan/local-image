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
            count_iloc_records(census, box_header.payload);
        } else if box_header.is(b"meta") {
            if box_header.payload.len() >= 4 {
                depth_boxes(census, BoxIter::new(&box_header.payload[4..]), depth + 1);
            }
        } else if box_header.is(b"iprp") {
            depth_boxes(census, box_header.children(), depth + 1);
        }
    }
}

// Walks the box as heic-rs 0.1.4 parses it and never charges less than heic-rs builds: every stop
// here is a refusal there, so the records before it stay counted. The walk can only overcharge a
// list heic-rs has already refused.
fn count_iloc_records(census: &mut Census, payload: &[u8]) -> Option<()> {
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

    let item_count = if version < 2 { u32::from(reader.u16("iloc item count").ok()?) } else { reader.u32("iloc item count").ok()? };
    let id_size = if version < 2 { 2 } else { 4 };
    let method_size = if version >= 1 { 2 } else { 0 };
    let item_header_size = id_size + method_size + 2 + usize::from(base_offset_size);
    // Unknown extent field widths are skipped as declared: heic-rs refuses one only when it reads
    // that field, which entries without extents never do.
    let extent_size = usize::from(offset_size) + usize::from(length_size) + usize::from(index_size);
    // Each pass consumes at least six bytes or stops, so a hostile item_count ends with the data.
    for _ in 0..item_count {
        reader.skip(item_header_size, "iloc item header").ok()?;
        let extent_count = usize::from(reader.u16("iloc extent count").ok()?);
        reader.skip(extent_count.checked_mul(extent_size)?, "iloc extents").ok()?;
        census.iloc_items = census.iloc_items.saturating_add(1);
        census.iloc_extents = census.iloc_extents.saturating_add(extent_count);
    }
    Some(())
}
