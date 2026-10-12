//! Strict bounded admission decoders. Original bytes still reach the pinned parser,
//! so accepted syntax is a subset with identical decoded bytes, not a repair codec.
use super::{ImportError, Result, cancelled, invalid, white};
use std::sync::atomic::AtomicBool;

fn append(out: &mut Vec<u8>, bytes: &[u8], limit: u64) -> Result<()> {
    if bytes.len() as u64 > limit.saturating_sub(out.len() as u64) {
        return Err(ImportError::Limit("decompressed byte"));
    }
    out.extend_from_slice(bytes);
    Ok(())
}
fn trailing(data: &[u8], cancel: &AtomicBool) -> Result<()> {
    for chunk in data.chunks(4096) {
        cancelled(cancel)?;
        if !chunk.iter().all(|b| white(*b)) {
            return Err(invalid("data after filter terminator"));
        }
    }
    Ok(())
}
pub(super) fn decode(name: &[u8], data: &[u8], limit: u64, cancel: &AtomicBool) -> Result<Vec<u8>> {
    cancelled(cancel)?;
    let mut out = Vec::new();
    match name {
        b"ASCIIHexDecode" | b"AHx" => {
            let mut high = None;
            for (i, &b) in data.iter().enumerate() {
                if i.is_multiple_of(4096) {
                    cancelled(cancel)?;
                }
                if white(b) {
                    continue;
                }
                if b == b'>' {
                    if let Some(n) = high {
                        append(&mut out, &[n << 4], limit)?;
                    }
                    trailing(&data[i + 1..], cancel)?;
                    return Ok(out);
                }
                let n = match b {
                    b'0'..=b'9' => b - b'0',
                    b'a'..=b'f' => b - b'a' + 10,
                    b'A'..=b'F' => b - b'A' + 10,
                    _ => return Err(invalid("ASCIIHex digit")),
                };
                if let Some(h) = high.take() {
                    append(&mut out, &[h << 4 | n], limit)?;
                } else {
                    high = Some(n);
                }
            }
            Err(invalid("missing ASCIIHex terminator"))
        }
        b"ASCII85Decode" | b"A85" => {
            let mut digits = [84u8; 5];
            let mut n = 0;
            let flush = |digits: &[u8; 5], count: usize, out: &mut Vec<u8>| -> Result<()> {
                let v = digits
                    .iter()
                    .try_fold(0u32, |v, &d| v.checked_mul(85).and_then(|v| v.checked_add(u32::from(d))))
                    .ok_or_else(|| invalid("ASCII85 overflow"))?;
                append(out, &v.to_be_bytes()[..count], limit)
            };
            for (i, &b) in data.iter().enumerate() {
                if i.is_multiple_of(4096) {
                    cancelled(cancel)?;
                }
                if white(b) {
                    continue;
                }
                match b {
                    b'!'..=b'u' => {
                        digits[n] = b - b'!';
                        n += 1;
                        if n == 5 {
                            flush(&digits, 4, &mut out)?;
                            digits = [84; 5];
                            n = 0;
                        }
                    }
                    b'z' if n == 0 => append(&mut out, &[0; 4], limit)?,
                    b'~' if data.get(i + 1) == Some(&b'>') => {
                        if n == 1 {
                            return Err(invalid("short ASCII85 group"));
                        }
                        if n > 1 {
                            flush(&digits, n - 1, &mut out)?;
                        }
                        trailing(&data[i + 2..], cancel)?;
                        return Ok(out);
                    }
                    _ => return Err(invalid("ASCII85 character or terminator")),
                }
            }
            Err(invalid("missing ASCII85 terminator"))
        }
        b"RunLengthDecode" | b"RL" => {
            let mut at = 0;
            while let Some(&n) = data.get(at) {
                cancelled(cancel)?;
                at += 1;
                match n {
                    128 => {
                        trailing(&data[at..], cancel)?;
                        return Ok(out);
                    }
                    0..=127 => {
                        let end = at + usize::from(n) + 1;
                        append(&mut out, data.get(at..end).ok_or_else(|| invalid("short RunLength literal"))?, limit)?;
                        at = end;
                    }
                    _ => {
                        let b = *data.get(at).ok_or_else(|| invalid("short RunLength repeat"))?;
                        at += 1;
                        append(&mut out, &[b; 128][..257 - usize::from(n)], limit)?;
                    }
                }
            }
            Err(invalid("missing RunLength terminator"))
        }
        _ => Err(ImportError::Unsupported("stream codec has no bounded decoder")),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn bounded_filter_known_answers_padding_and_limits() {
        let cancel = AtomicBool::new(false);
        for (filter, encoded, expected) in [
            (b"AHx".as_slice(), b"61 62 6>".as_slice(), b"ab`".as_slice()),
            (b"A85", b"9jqo^~>", b"Man "),
            (b"A85", b"z!!~>", &[0; 5]),
            (b"RL", &[0, b'A', 254, b'B', 128], b"ABBB"),
        ] {
            assert_eq!(decode(filter, encoded, expected.len() as u64, &cancel).unwrap(), expected);
            assert!(matches!(decode(filter, encoded, expected.len() as u64 - 1, &cancel), Err(ImportError::Limit("decompressed byte"))));
            assert!(matches!(decode(filter, encoded, 1024, &AtomicBool::new(true)), Err(ImportError::Cancelled)));
        }
    }
}
