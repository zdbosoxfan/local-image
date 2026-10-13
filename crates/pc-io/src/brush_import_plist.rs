//! Small non-recursive bplist00 reader for NSKeyedArchiver brush dictionaries.
//! Format markers/trailer follow Apple's public CFBinaryPList format description; this is
//! independently written Rust. Objects retain references, so shared graphs never expand exponentially.
use std::collections::{BTreeMap, HashSet};
const MAX_OBJECTS: usize = 32768;
const MAX_REFS: usize = 131072;
const MAX_BYTES: usize = 8 << 20;
#[derive(Debug)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Uid(usize),
    Array(Vec<usize>),
    Dict(Vec<(usize, usize)>),
    Data,
}
#[derive(Debug)]
pub(crate) struct Plist {
    objects: Vec<Value>,
    top: usize,
}
fn uint(bytes: &[u8]) -> Result<u64, String> {
    if bytes.is_empty() || bytes.len() > 8 {
        return Err("invalid plist integer width".into());
    }
    Ok(bytes.iter().fold(0, |n, b| (n << 8) | u64::from(*b)))
}
fn slice(bytes: &[u8], pos: usize, n: usize) -> Result<&[u8], String> {
    pos.checked_add(n).and_then(|end| bytes.get(pos..end)).ok_or_else(|| "truncated plist object".into())
}
fn length(b: &[u8], p: &mut usize, n: u8) -> Result<usize, String> {
    if n < 15 {
        return Ok(n as usize);
    }
    let marker = *b.get(*p).ok_or("missing plist extended length")?;
    *p += 1;
    if marker >> 4 != 1 || marker & 15 > 3 {
        return Err("invalid plist extended length".into());
    }
    let w = 1usize << (marker & 15);
    let n = uint(slice(b, *p, w)?)?;
    *p += w;
    usize::try_from(n).map_err(|_| "plist length overflow".into())
}
impl Plist {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES || bytes.len() < 40 || bytes.get(..8) != Some(b"bplist00") {
            return Err("not a bounded bplist00 archive".into());
        }
        let t = &bytes[bytes.len() - 32..];
        let (ow, rw) = (t[6] as usize, t[7] as usize);
        if !(1..=8).contains(&ow) || !(1..=8).contains(&rw) {
            return Err("invalid plist reference/offset width".into());
        }
        let count = usize::try_from(uint(&t[8..16])?).map_err(|_| "plist object count overflow")?;
        let top = usize::try_from(uint(&t[16..24])?).map_err(|_| "plist top overflow")?;
        let table = usize::try_from(uint(&t[24..32])?).map_err(|_| "plist table overflow")?;
        if count == 0 || count > MAX_OBJECTS || top >= count || table < 8 || table >= bytes.len() - 32 {
            return Err("invalid plist trailer".into());
        }
        let offsets = slice(&bytes[..bytes.len() - 32], table, count.checked_mul(ow).ok_or("plist table overflow")?)?;
        let b = &bytes[..table];
        let mut objects = Vec::with_capacity(count);
        let mut refs = 0usize;
        let mut budget = 0usize;
        let mut seen = HashSet::new();
        for off in offsets.chunks_exact(ow) {
            let mut p = usize::try_from(uint(off)?).map_err(|_| "plist offset overflow")?;
            if p < 8 || p >= table || !seen.insert(p) {
                return Err("invalid or duplicate plist object offset".into());
            }
            let marker = *b.get(p).ok_or("bad plist object")?;
            p += 1;
            let (kind, n) = (marker >> 4, marker & 15);
            let obj = match kind {
                0 => match n {
                    0 => Value::Null,
                    8 => Value::Bool(false),
                    9 => Value::Bool(true),
                    _ => return Err("unsupported plist simple value".into()),
                },
                1 => {
                    if n > 3 {
                        return Err("oversized plist integer".into());
                    }
                    let w = 1usize << n;
                    let v = uint(slice(b, p, w)?)?;
                    Value::Number(if w == 8 { (v as i64) as f64 } else { v as f64 })
                }
                2 => {
                    let v = match n {
                        2 => {
                            let s = slice(b, p, 4)?;
                            f32::from_be_bytes([s[0], s[1], s[2], s[3]]) as f64
                        }
                        3 => f64::from_bits(uint(slice(b, p, 8)?)?),
                        _ => return Err("invalid plist real".into()),
                    };
                    if !v.is_finite() {
                        return Err("non-finite plist real".into());
                    }
                    Value::Number(v)
                }
                3 => {
                    if n != 3 {
                        return Err("invalid plist date".into());
                    }
                    let _ = slice(b, p, 8)?;
                    Value::Data
                }
                4..=6 => {
                    let count = length(b, &mut p, n)?;
                    let size = count.checked_mul(if kind == 6 { 2 } else { 1 }).ok_or("plist string overflow")?;
                    budget = budget.checked_add(size).filter(|v| *v <= MAX_BYTES).ok_or("plist data budget exceeded")?;
                    let s = slice(b, p, size)?;
                    match kind {
                        4 => Value::Data,
                        5 => Value::String(std::str::from_utf8(s).map_err(|_| "invalid plist string")?.to_owned()),
                        _ => Value::String(
                            String::from_utf16(&s.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>())
                                .map_err(|_| "invalid plist UTF16")?,
                        ),
                    }
                }
                8 => Value::Uid(usize::try_from(uint(slice(b, p, n as usize + 1)?)?).map_err(|_| "plist UID overflow")?),
                10 | 13 => {
                    let count = length(b, &mut p, n)?;
                    let nr = count.checked_mul(if kind == 13 { 2 } else { 1 }).ok_or("plist reference overflow")?;
                    refs = refs.checked_add(nr).filter(|v| *v <= MAX_REFS).ok_or("plist reference budget exceeded")?;
                    let r = slice(b, p, nr.checked_mul(rw).ok_or("plist reference overflow")?)?
                        .chunks_exact(rw)
                        .map(|s| usize::try_from(uint(s)?).map_err(|_| "plist reference overflow".to_string()))
                        .collect::<Result<Vec<_>, _>>()?;
                    if kind == 10 { Value::Array(r) } else { Value::Dict(r[..count].iter().copied().zip(r[count..].iter().copied()).collect()) }
                }
                _ => return Err(format!("unsupported plist marker {marker:02x}")),
            };
            objects.push(obj);
        }
        for o in &objects {
            let invalid = match o {
                Value::Array(v) => v.iter().any(|&id| id >= objects.len()),
                Value::Dict(v) => v.iter().any(|&(a, b)| a >= objects.len() || b >= objects.len()),
                _ => false,
            };
            if invalid {
                return Err("invalid plist object reference".into());
            }
        }
        Ok(Self { objects, top })
    }
    fn get(&self, id: usize) -> Option<&Value> {
        self.objects.get(id)
    }
    fn key(&self, id: usize, key: &str) -> Option<usize> {
        match self.get(id)? {
            Value::Dict(v) => v.iter().find_map(|&(k, v)| matches!(self.get(k),Some(Value::String(s)) if s==key).then_some(v)),
            _ => None,
        }
    }
    /// Resolve the NSKeyedArchiver root and its scalar fields, following UIDs through $objects.
    pub(crate) fn brush(&self) -> Result<BTreeMap<String, Scalar>, String> {
        let archiver = self.key(self.top, "$archiver").and_then(|id| self.get(id));
        if !matches!(archiver,Some(Value::String(s)) if s=="NSKeyedArchiver") {
            return Err("not NSKeyedArchiver".into());
        }
        let ids = match self.key(self.top, "$objects").and_then(|id| self.get(id)) {
            Some(Value::Array(v)) => v,
            _ => return Err("missing archive objects".into()),
        };
        let top = self.key(self.top, "$top").ok_or("missing archive top")?;
        let root = self.key(top, "root").ok_or("missing archive root")?;
        let resolve = |mut id: usize| -> Result<usize, String> {
            let mut seen = HashSet::new();
            for _ in 0..32 {
                if !seen.insert(id) {
                    return Err("cyclic archive UID".into());
                }
                match self.get(id) {
                    Some(Value::Uid(n)) => id = *ids.get(*n).ok_or("archive UID out of range")?,
                    Some(_) => return Ok(id),
                    None => return Err("archive reference out of range".into()),
                }
            }
            Err("archive UID depth limit".into())
        };
        let root = resolve(root)?;
        let dict = match self.get(root) {
            Some(Value::Dict(d)) => d,
            _ => return Err("brush root is not a dictionary".into()),
        };
        let mut out = BTreeMap::new();
        let mut materialized = 0usize;
        for &(k, v) in dict {
            let key = match self.get(k) {
                Some(Value::String(s)) if s.len() <= 1024 => s.as_str(),
                _ => return Err("archive key is not a bounded string".into()),
            };
            let source = self.get(resolve(v)?);
            let bytes = key.len().saturating_add(match source {
                Some(Value::String(s)) => s.len(),
                _ => 32,
            });
            materialized = materialized.checked_add(bytes).filter(|n| *n <= MAX_BYTES).ok_or("archive scalar materialization budget exceeded")?;
            let val = match source {
                Some(Value::String(s)) => Some(Scalar::String(s.clone())),
                Some(Value::Number(n)) => Some(Scalar::Number(*n)),
                Some(Value::Bool(b)) => Some(Scalar::Bool(*b)),
                _ => None,
            };
            if let Some(val) = val {
                out.insert(key.to_string(), val);
            }
        }
        Ok(out)
    }
}
#[derive(Debug)]
pub(crate) enum Scalar {
    String(String),
    Number(f64),
    Bool(bool),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn brush_import_shared_plist_scalars_cannot_expand_past_budget() {
        let mut objects = vec![
            Value::String("$archiver".into()),
            Value::String("NSKeyedArchiver".into()),
            Value::String("$objects".into()),
            Value::Array(vec![9, 11]),
            Value::String("$top".into()),
            Value::Dict(vec![(6, 7)]),
            Value::String("root".into()),
            Value::Uid(0),
            Value::Dict(vec![(0, 1), (2, 3), (4, 5)]),
            Value::Dict(vec![]),
            Value::Uid(1),
            Value::String("a".repeat(256 << 10)),
        ];
        for i in 0..64 {
            objects.push(Value::String(format!("field{i}")));
        }
        objects[9] = Value::Dict((0..64).map(|i| (12 + i, 10)).collect());
        let plist = Plist { objects, top: 8 };
        assert!(plist.brush().unwrap_err().contains("materialization budget"));
    }
}
