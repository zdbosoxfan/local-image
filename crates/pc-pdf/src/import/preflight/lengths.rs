//! A classic xref is a tentative locator, never proof that bytes are objects.
//! Every forward lookup is verified against the independent sequential admission
//! pass before constructing a PDF. Fake headers inside streams cannot qualify.
use super::{ImportError, ImportOptions, Lexer, Result, Token, Value, cancelled, invalid, white};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::AtomicBool,
};
type Id = (u32, u16);
pub(super) struct Index {
    offsets: BTreeMap<Id, usize>,
    used: BTreeMap<Id, (usize, u64)>,
    xref: usize,
}
fn word(l: &mut Lexer<'_>) -> Result<Vec<u8>> {
    match l.next()? {
        Some(Token::Word(w)) => Ok(w),
        _ => Err(invalid("classic xref token")),
    }
}
fn uint(w: &[u8]) -> Result<u64> {
    if w.is_empty() || !w.iter().all(u8::is_ascii_digit) {
        return Err(invalid("xref integer"));
    }
    std::str::from_utf8(w).ok().and_then(|s| s.parse().ok()).ok_or_else(|| invalid("xref integer"))
}
impl Index {
    pub(super) fn read(data: &[u8], header: usize, options: &ImportOptions, cancel: &AtomicBool) -> Result<Self> {
        cancelled(cancel)?;
        // Only the terminal startxref declaration is examined, never arbitrary
        // object-like bytes or endstream delimiters inside binary payloads.
        let tail = data.get(data.len().saturating_sub(1024)..).unwrap_or(data);
        let end = tail.iter().rposition(|b| !white(*b)).map(|n| n + 1).ok_or_else(|| invalid("PDF trailer"))?;
        let tail = tail[..end].strip_suffix(b"%%EOF").ok_or_else(|| invalid("PDF EOF for indirect length"))?;
        let at = tail.windows(9).rposition(|w| w == b"startxref").ok_or(ImportError::Unsupported("forward stream length requires classic xref"))?;
        let mut pointer = Lexer::new(&tail[at + 9..], options, cancel);
        let raw = usize::try_from(uint(&word(&mut pointer)?)?).map_err(|_| ImportError::Limit("xref offset"))?;
        if pointer.next()?.is_some() {
            return Err(invalid("terminal startxref"));
        }
        let mut origin = 0;
        let xref = if data.get(raw..).is_some_and(|s| s.starts_with(b"xref")) {
            raw
        } else {
            origin = header;
            raw.checked_add(header)
                .filter(|&at| data.get(at..).is_some_and(|s| s.starts_with(b"xref")))
                .ok_or(ImportError::Unsupported("forward stream length requires classic xref"))?
        };
        let mut l = Lexer::new(&data[xref..], options, cancel);
        if word(&mut l)? != b"xref" {
            return Err(invalid("classic xref"));
        }
        let mut offsets = BTreeMap::new();
        let mut rows = 0u64;
        loop {
            let first = word(&mut l)?;
            if first == b"trailer" {
                break;
            }
            let first = uint(&first)?;
            let count = uint(&word(&mut l)?)?;
            rows = rows.checked_add(count).ok_or(ImportError::Limit("xref object"))?;
            if rows > (options.limits.objects as u64).saturating_add(1)
                || first.checked_add(count).is_none_or(|n| n > (options.limits.objects as u64).saturating_add(1))
            {
                return Err(ImportError::Limit("xref object"));
            }
            for id in first..first + count {
                let offset = uint(&word(&mut l)?)?;
                let generation = u16::try_from(uint(&word(&mut l)?)?).map_err(|_| invalid("xref generation"))?;
                let kind = word(&mut l)?;
                if kind == b"n" {
                    let offset = usize::try_from(offset)
                        .ok()
                        .and_then(|n| n.checked_add(origin))
                        .filter(|&n| n < data.len())
                        .ok_or_else(|| invalid("xref object offset"))?;
                    if offsets.insert((u32::try_from(id).map_err(|_| ImportError::Limit("object number"))?, generation), offset).is_some() {
                        return Err(invalid("duplicate xref identity"));
                    }
                } else if kind != b"f" {
                    return Err(invalid("xref entry type"));
                }
            }
        }
        let token = l.next()?.ok_or_else(|| invalid("xref trailer dictionary"))?;
        let trailer = l.value(token, 0)?;
        if !matches!(trailer, Value::Dict(_)) {
            return Err(invalid("xref trailer dictionary"));
        }
        if trailer.get(b"Prev").is_some() || trailer.get(b"XRefStm").is_some() {
            return Err(ImportError::Unsupported("forward lengths in incremental or hybrid xref"));
        }
        Ok(Self { offsets, used: BTreeMap::new(), xref })
    }
    pub(super) fn resolve(&mut self, id: Id, data: &[u8], options: &ImportOptions, cancel: &AtomicBool) -> Result<u64> {
        if let Some((_, n)) = self.used.get(&id) {
            return Ok(*n);
        }
        let offset = *self.offsets.get(&id).ok_or_else(|| invalid("stream length object missing from xref"))?;
        let mut l = Lexer::new(&data[offset..], options, cancel);
        if uint(&word(&mut l)?)? != u64::from(id.0) || uint(&word(&mut l)?)? != u64::from(id.1) || word(&mut l)? != b"obj" {
            return Err(invalid("stream length xref identity mismatch"));
        }
        let n = uint(&word(&mut l)?)?;
        if word(&mut l)? != b"endobj" {
            return Err(invalid("stream length must be an integer object"));
        }
        self.used.insert(id, (offset, n));
        Ok(n)
    }
    pub(super) fn verify(&self, objects: &BTreeMap<Id, super::Record>, starts: &BTreeMap<Id, usize>, xrefs: &BTreeSet<usize>) -> Result<()> {
        if !xrefs.contains(&self.xref) {
            return Err(invalid("xref locator was inside a stream or object"));
        }
        for (id, (offset, n)) in &self.used {
            if starts.get(id) != Some(offset) || objects.get(id).and_then(|r| r.value.number()) != Some(*n) {
                return Err(invalid("stream length locator was not an admitted object"));
            }
        }
        Ok(())
    }
}
