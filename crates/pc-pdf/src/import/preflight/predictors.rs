//! Apply only predictor layouts for which the pinned decoder has matching,
//! byte-aligned row semantics. Never inspect pre-predictor bytes as page content.
use super::{ImportError, Result, Value, cancelled, invalid};
use std::sync::atomic::AtomicBool;

pub(super) fn parameter(params: Option<&Value>, key: &[u8], default: u64) -> Result<u64> {
    params.and_then(|p| p.get(key)).map_or(Ok(default), |v| v.number().ok_or_else(|| invalid("predictor parameter must be a direct unsigned integer")))
}
pub(super) fn apply(mut data: Vec<u8>, params: Option<&Value>, limit: u64, cancel: &AtomicBool) -> Result<(Vec<u8>, u64)> {
    cancelled(cancel)?;
    let predictor = parameter(params, b"Predictor", 1)?;
    if predictor == 1 {
        return Ok((data, 0));
    }
    if predictor != 2 && !(10..=15).contains(&predictor) {
        return Err(ImportError::Unsupported("predictor type"));
    }
    let colors = parameter(params, b"Colors", 1)?;
    let bits = parameter(params, b"BitsPerComponent", 8)?;
    let columns = parameter(params, b"Columns", 1)?;
    if colors == 0 || colors > 32 || columns == 0 || !matches!(bits, 1 | 2 | 4 | 8 | 16) || colors * bits > 255 {
        return Err(invalid("predictor dimensions or component width"));
    }
    let row_bits = columns.checked_mul(colors * bits).ok_or(ImportError::Limit("predictor row"))?;
    let row = usize::try_from(row_bits.div_ceil(8)).map_err(|_| ImportError::Limit("predictor row"))?;
    let stride = (colors * bits).div_ceil(8) as usize;
    let png = predictor >= 10;
    // Pinned BitChunks drops incomplete pixel chunks and its TIFF writer does
    // not align sub-byte rows. Admit only layouts with exact matching semantics.
    if png && !row.is_multiple_of(stride) || !png && !row_bits.is_multiple_of(8) {
        return Err(ImportError::Unsupported("predictor row packing"));
    }
    let encoded_row = row.checked_add(usize::from(png)).ok_or(ImportError::Limit("predictor row"))?;
    if row as u64 > limit {
        return Err(ImportError::Limit("predictor row"));
    }
    if !data.len().is_multiple_of(encoded_row) {
        return Err(invalid("truncated predictor row"));
    }
    let output_len = (data.len() / encoded_row).checked_mul(row).ok_or(ImportError::Limit("predictor output"))?;
    let cost = (output_len as u64).checked_add(row as u64).ok_or(ImportError::Limit("predictor output"))?;
    if cost > limit {
        return Err(ImportError::Limit("decompressed byte"));
    }
    if png {
        let mut output = vec![0u8; output_len];
        for (r, input) in data.chunks_exact(encoded_row).enumerate() {
            cancelled(cancel)?;
            let filter = input[0];
            if filter > 4 {
                return Err(invalid("PNG predictor row tag"));
            }
            let at = r * row;
            for i in 0..row {
                if i.is_multiple_of(4096) {
                    cancelled(cancel)?;
                }
                let left = if i >= stride { output[at + i - stride] } else { 0 };
                let up = if r > 0 { output[at + i - row] } else { 0 };
                let corner = if r > 0 && i >= stride { output[at + i - row - stride] } else { 0 };
                let predict = match filter {
                    0 => 0,
                    1 => left,
                    2 => up,
                    3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                    4 => {
                        let p = i32::from(left) + i32::from(up) - i32::from(corner);
                        let (a, b, c) = ((p - i32::from(left)).abs(), (p - i32::from(up)).abs(), (p - i32::from(corner)).abs());
                        if a <= b && a <= c {
                            left
                        } else if b <= c {
                            up
                        } else {
                            corner
                        }
                    }
                    _ => return Err(invalid("PNG predictor row tag")),
                };
                output[at + i] = input[i + 1].wrapping_add(predict);
            }
        }
        Ok((output, cost))
    } else {
        let samples = (columns * colors) as usize;
        let mask = (1u32 << bits) - 1;
        for row in data.chunks_exact_mut(row) {
            cancelled(cancel)?;
            let mut previous = [0u32; 32];
            for i in 0..samples {
                if i.is_multiple_of(4096) {
                    cancelled(cancel)?;
                }
                let bit = i * bits as usize;
                let read = if bits == 16 {
                    u32::from(u16::from_be_bytes([row[bit / 8], row[bit / 8 + 1]]))
                } else {
                    u32::from(row[bit / 8] >> (8 - bits as usize - bit % 8)) & mask
                };
                let channel = i % colors as usize;
                let value = (read + previous[channel]) & mask;
                previous[channel] = value;
                if bits == 16 {
                    row[bit / 8..bit / 8 + 2].copy_from_slice(&(value as u16).to_be_bytes());
                } else {
                    let shift = 8 - bits as usize - bit % 8;
                    row[bit / 8] = (row[bit / 8] & !((mask as u8) << shift)) | ((value as u8) << shift);
                }
            }
        }
        Ok((data, cost))
    }
}

pub(super) fn parameters(value: &Value, array: bool, count: usize) -> Result<Vec<Option<&Value>>> {
    if value.get(b"DP").is_some() && value.get(b"DecodeParms").is_some() {
        return Err(invalid("ambiguous decoder parameter aliases"));
    }
    let params = value.get(b"DP").or_else(|| value.get(b"DecodeParms"));
    match params {
        None | Some(Value::Null) => Ok(vec![None; count]),
        Some(v @ Value::Dict(_)) if !array && count == 1 => Ok(vec![Some(v)]),
        Some(Value::Array(values)) if array && values.len() == count => values
            .iter()
            .map(|v| match v {
                Value::Null => Ok(None),
                Value::Dict(_) => Ok(Some(v)),
                Value::Ref(_, _) => Err(ImportError::Unsupported("indirect decoder parameters")),
                _ => Err(invalid("decoder parameter array entry")),
            })
            .collect(),
        Some(Value::Ref(_, _)) => Err(ImportError::Unsupported("indirect decoder parameters")),
        _ => Err(invalid("decoder parameters must match the filter list")),
    }
}
