//! Aggregate caps for metadata boxes that can make heic-rs spend unbounded parse work.

use heic_rs::boxes::BoxIter;

use crate::Error;

// heic-rs rescans earlier item ids while parsing ipma associations.
const MAX_IPMA_BYTES: usize = 64 << 10;
// iloc extents may each rebuild the item they reference.
const MAX_ILOC_BYTES: usize = 64 << 10;

pub(super) fn cap(bytes: &[u8]) -> Result<(), Error> {
    let mut census = Census::default();
    depth_boxes(&mut census, BoxIter::new(bytes), 0);
    if census.ipma > MAX_IPMA_BYTES {
        return Err(Error::Limit(format!("the image carries {} bytes of property associations, over the {MAX_IPMA_BYTES}-byte cap", census.ipma)));
    }
    if census.iloc > MAX_ILOC_BYTES {
        return Err(Error::Limit(format!("the image carries {} bytes of item locations, over the {MAX_ILOC_BYTES}-byte cap", census.iloc)));
    }
    Ok(())
}

#[derive(Default)]
struct Census {
    ipma: usize,
    iloc: usize,
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
            census.iloc = census.iloc.saturating_add(box_header.payload.len());
        } else if box_header.is(b"meta") {
            if box_header.payload.len() >= 4 {
                depth_boxes(census, BoxIter::new(&box_header.payload[4..]), depth + 1);
            }
        } else if box_header.is(b"iprp") {
            depth_boxes(census, box_header.children(), depth + 1);
        }
    }
}
