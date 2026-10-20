//! PDF LZW admission with a fixed 4096-entry prefix dictionary. The pinned
//! parser copies strings into its dictionary; account those allocations too.
use super::{ImportError, Result, cancelled, invalid};
use std::sync::atomic::AtomicBool;

pub(super) fn decode(data: &[u8], early: bool, limit: u64, work_limit: usize, cancel: &AtomicBool) -> Result<(Vec<u8>, u64)> {
    let (bytes, cost, _) = prefix(data, early, limit, work_limit, cancel)?;
    Ok((bytes, cost))
}
pub(super) fn prefix(data: &[u8], early: bool, limit: u64, work_limit: usize, cancel: &AtomicBool) -> Result<(Vec<u8>, u64, usize)> {
    let mut prefix = [0u16; 4096];
    let mut suffix = [0u8; 4096];
    let mut length = [1u16; 4096];
    let mut stack = [0u8; 4096];
    let mut next = 258usize;
    let mut width = 9usize;
    let mut previous: Option<usize> = None;
    let mut bit = 0usize;
    let mut output = Vec::new();
    // Upper bound on pinned dictionary entry storage, plus singleton byte data.
    let mut cost = (4096 * std::mem::size_of::<Option<Vec<u8>>>() + 256) as u64;
    if cost > limit {
        return Err(ImportError::Limit("LZW dictionary byte"));
    }
    let mut codes = 0usize;
    loop {
        cancelled(cancel)?;
        codes = codes.saturating_add(1);
        if codes > work_limit {
            return Err(ImportError::Limit("LZW code work"));
        }
        if bit.checked_add(width).is_none_or(|end| end > data.len().saturating_mul(8)) {
            return Err(invalid("missing LZW EOD"));
        }
        let mut code = 0usize;
        for _ in 0..width {
            code = code * 2 + usize::from((data[bit / 8] >> (7 - bit % 8)) & 1);
            bit += 1;
        }
        if code == 256 {
            next = 258;
            width = 9;
            previous = None;
            continue;
        }
        if code == 257 {
            return Ok((output, cost, bit.div_ceil(8)));
        }
        let special = code == next;
        let mut current = if special { previous.ok_or_else(|| invalid("LZW missing previous code"))? } else { code };
        if current >= next || matches!(current, 256 | 257) {
            return Err(invalid("invalid LZW code"));
        }
        let mut count = 0;
        while current >= 258 {
            if count >= 4095 {
                return Err(invalid("LZW dictionary cycle"));
            }
            stack[count] = suffix[current];
            count += 1;
            current = usize::from(prefix[current]);
        }
        if current > 255 {
            return Err(invalid("invalid LZW prefix"));
        }
        stack[count] = current as u8;
        count += 1;
        let first = stack[count - 1];
        let produced = count + usize::from(special);
        // The upstream parser constructs a candidate entry even at a full table.
        let dictionary_copy = previous.map_or(0, |p| u64::from(length[p]) + 1);
        cost = cost.checked_add(produced as u64).and_then(|n| n.checked_add(dictionary_copy)).ok_or(ImportError::Limit("decompressed byte"))?;
        if cost > limit {
            return Err(ImportError::Limit("decompressed byte"));
        }
        output.extend(stack[..count].iter().rev());
        if special {
            output.push(first);
        }
        if let Some(p) = previous {
            if next < 4096 {
                prefix[next] = p as u16;
                suffix[next] = first;
                length[next] = length[p] + 1;
                next += 1;
            } else if special {
                return Err(invalid("LZW dictionary full"));
            }
        }
        let adjusted = next + usize::from(early);
        width = if adjusted >= 2048 {
            12
        } else if adjusted >= 1024 {
            11
        } else if adjusted >= 512 {
            10
        } else {
            9
        };
        previous = Some(code);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn lzw_known_answer_covers_dictionary_special_case() {
        // Pinned hayro-syntax 0.8.0 known-answer vector (filter/lzw_flate.rs).
        let encoded = [0x80, 0x0b, 0x60, 0x50, 0x22, 0x0c, 0x0c, 0x85, 0x01];
        let (bytes, cost) = decode(&encoded, true, 1 << 20, 1000, &AtomicBool::new(false)).unwrap();
        assert_eq!(bytes, b"-----A---B");
        assert!(cost > bytes.len() as u64);
        assert!(matches!(decode(&encoded, true, cost - 1, 1000, &AtomicBool::new(false)), Err(ImportError::Limit(_))));
        assert!(matches!(decode(&encoded, true, 1 << 20, 1, &AtomicBool::new(false)), Err(ImportError::Limit("LZW code work"))));
        assert!(matches!(decode(&encoded, true, 1 << 20, 1000, &AtomicBool::new(true)), Err(ImportError::Cancelled)));
    }
}
