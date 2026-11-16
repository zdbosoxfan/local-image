//! The `hvcC` parameter-set bounds checked before heic-rs parses them.

use crate::Error;

/// The most parameter sets one `hvcC` may carry: one per id H.265 allows (16 VPS, 16 SPS, 64 PPS).
/// heic-rs 0.1.4 replaces a repeated id by scanning every set kept so far, so more sets (repeats,
/// or ids past the standard's range) only make that collection quadratic.
const MAX_PARAMETER_SETS: usize = 16 + 16 + 64;
/// Highest `pps_pic_parameter_set_id` H.265 allows (7.4.3.3.1).
const MAX_PPS_ID: u32 = 63;
/// Most tile columns and rows a picture may have: H.265's level limits (Table A.8, levels 6 to
/// 6.2). heic-rs 0.1.4 builds its tile tables by walking the tile boundaries for every coding tree
/// block, so its work grows with blocks × tiles; these keep that a small multiple of the blocks.
const MAX_TILE_COLS: u32 = 20;
const MAX_TILE_ROWS: u32 = 22;
/// NAL unit type of a picture parameter set.
const PPS_NUT: u8 = 34;

/// Refuses parameter sets that would make heic-rs's set collection or tile tables cost far more
/// than the picture: too many sets, a PPS id out of range, or more tiles than any level allows.
pub(crate) fn check_parameter_sets(sets: &[&[u8]]) -> Result<(), Error> {
    if sets.len() > MAX_PARAMETER_SETS {
        return Err(Error::Limit(format!("the hvcC record carries {} parameter sets, over the {MAX_PARAMETER_SETS}-set cap", sets.len())));
    }
    for pps in sets.iter().filter(|n| n.first().is_some_and(|b| (b >> 1) & 0x3f == PPS_NUT)) {
        let rbsp = unescape(pps.get(2..).unwrap_or_default());
        let (id, cols, rows) = pps_layout(&mut Bits { data: &rbsp, pos: 0 }).ok_or_else(|| Error::Malformed("unreadable picture parameter set".into()))?;
        if id > MAX_PPS_ID {
            return Err(Error::Malformed(format!("picture parameter set id {id} is out of range")));
        }
        if cols > MAX_TILE_COLS || rows > MAX_TILE_ROWS {
            return Err(Error::Limit(format!("a picture of {cols} × {rows} tiles, over the {MAX_TILE_COLS} × {MAX_TILE_ROWS} H.265 allows")));
        }
    }
    Ok(())
}

/// A PPS's id and tile columns and rows (1 × 1 without tiles), read up to the tile counts
/// (H.265 7.3.2.3.1).
fn pps_layout(r: &mut Bits) -> Option<(u32, u32, u32)> {
    let id = r.ue()?;
    r.ue()?; // pps_seq_parameter_set_id
    // dependent slices, output flag, 3 extra slice header bits, sign hiding, cabac_init_present
    r.skip(7)?;
    r.ue()?; // num_ref_idx_l0_default_active_minus1
    r.ue()?; // num_ref_idx_l1_default_active_minus1
    r.ue()?; // init_qp_minus26
    r.skip(2)?; // constrained_intra_pred_flag, transform_skip_enabled_flag
    if r.flag()? {
        r.ue()?; // diff_cu_qp_delta_depth
    }
    r.ue()?; // pps_cb_qp_offset
    r.ue()?; // pps_cr_qp_offset
    // slice chroma qp offsets present, weighted pred, weighted bipred, transquant bypass
    r.skip(4)?;
    let tiles = r.flag()?;
    r.skip(1)?; // entropy_coding_sync_enabled_flag
    let (cols, rows) = if tiles { (r.ue()?.checked_add(1)?, r.ue()?.checked_add(1)?) } else { (1, 1) };
    Some((id, cols, rows))
}

/// The RBSP of a NAL payload: emulation-prevention bytes (`00 00 03`) removed.
fn unescape(nal: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nal.len());
    let mut zeros = 0usize;
    for &b in nal {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn u(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            let byte = *self.data.get(self.pos / 8)?;
            v = (v << 1) | u32::from((byte >> (7 - self.pos % 8)) & 1);
            self.pos += 1;
        }
        Some(v)
    }

    fn flag(&mut self) -> Option<bool> {
        self.u(1).map(|b| b == 1)
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        self.pos = self.pos.checked_add(n)?;
        (self.pos <= self.data.len().saturating_mul(8)).then_some(())
    }

    /// `ue(v)` (and `se(v)`, read as its code number); values past 32 bits are refused.
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0u32;
        while !self.flag()? {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        let rest = self.u(zeros)?;
        ((1u64 << zeros) - 1 + u64::from(rest)).try_into().ok()
    }
}
