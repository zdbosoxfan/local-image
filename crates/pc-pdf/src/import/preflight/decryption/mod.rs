//! Bounded empty-user-password decryption through pinned hayro public APIs.
//! Bootstrap parsing sees a synthetic blank page tree, never the original tree.
use super::{ImportError, ImportOptions, Lexer, Result, Token, Value, cancelled, invalid, lengths, white};
use std::{cell::Cell, collections::BTreeMap, ops::Range, sync::atomic::AtomicBool};
mod security;
mod serialize;
type Id = (u32, u16);
struct Raw {
    body: Range<usize>,
    value: Value,
    stream: Option<Range<usize>>,
    dict_end: usize,
    start: usize,
}
struct Budget<'a> {
    options: &'a ImportOptions,
    cancel: &'a AtomicBool,
    bytes: Cell<u64>,
    work: Cell<usize>,
}
impl Budget<'_> {
    fn step(&self) -> Result<()> {
        cancelled(self.cancel)?;
        self.work.set(self.work.get().saturating_add(1));
        if self.work.get() > self.options.limits.segments.saturating_mul(16).min(16_000_000) {
            return Err(ImportError::Limit("decryption work"));
        }
        Ok(())
    }
    fn charge(&self, n: usize) -> Result<()> {
        self.step()?;
        self.bytes.set(self.bytes.get().saturating_add(n as u64));
        if self.bytes.get() > self.options.limits.decompressed_bytes {
            return Err(ImportError::Limit("decryption byte"));
        }
        Ok(())
    }
    fn append(&self, out: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
        self.charge(bytes.len())?;
        out.extend_from_slice(bytes);
        Ok(())
    }
}
fn field_span<'a>(data: &'a [u8], key: &[u8], budget: &Budget<'_>) -> Result<Option<&'a [u8]>> {
    let mut l = Lexer::metered(data, budget.options, budget.cancel, &budget.bytes);
    if !matches!(l.next()?, Some(Token::OpenDict)) {
        return Err(invalid("security dictionary"));
    }
    loop {
        budget.step()?;
        match l.next()?.ok_or_else(|| invalid("security dictionary end"))? {
            Token::CloseDict => return Ok(None),
            Token::Name(name) => {
                let first = l.next()?.ok_or_else(|| invalid("security field value"))?;
                let start = l.start;
                l.value(first, 0)?;
                if name == key {
                    return Ok(Some(&data[start..l.at]));
                }
            }
            _ => return Err(invalid("security dictionary key")),
        }
    }
}
fn string_bytes(raw: &[u8], limit: usize, budget: &Budget<'_>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 1;
    let push = |out: &mut Vec<u8>, b: u8, budget: &Budget<'_>| -> Result<()> {
        if out.len() >= limit {
            return Err(ImportError::Limit("encrypted string byte"));
        }
        budget.append(out, &[b])
    };
    if raw.first() == Some(&b'<') && raw.last() == Some(&b'>') {
        let mut high = None;
        for &b in &raw[1..raw.len() - 1] {
            budget.step()?;
            if white(b) {
                continue;
            }
            let n = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(invalid("encrypted hex string")),
            };
            if let Some(h) = high.take() {
                push(&mut out, h * 16 + n, budget)?;
            } else {
                high = Some(n);
            }
        }
        if let Some(h) = high {
            push(&mut out, h * 16, budget)?;
        }
        return Ok(out);
    }
    if raw.first() != Some(&b'(') || raw.last() != Some(&b')') {
        return Err(invalid("encrypted string syntax"));
    }
    while at + 1 < raw.len() {
        budget.step()?;
        let b = raw[at];
        at += 1;
        if b == b'\\' {
            let b = *raw.get(at).ok_or_else(|| invalid("encrypted string escape"))?;
            at += 1;
            match b {
                b'\r' => {
                    if raw.get(at) == Some(&b'\n') {
                        at += 1;
                    }
                }
                b'\n' => {}
                b'0'..=b'7' => {
                    let mut n = u16::from(b - b'0');
                    for _ in 0..2 {
                        if let Some(b @ b'0'..=b'7') = raw.get(at) {
                            n = n * 8 + u16::from(*b - b'0');
                            at += 1;
                        } else {
                            break;
                        }
                    }
                    push(&mut out, n as u8, budget)?;
                }
                _ => push(
                    &mut out,
                    match b {
                        b'n' => 10,
                        b'r' => 13,
                        b't' => 9,
                        b'b' => 8,
                        b'f' => 12,
                        _ => b,
                    },
                    budget,
                )?,
            }
        } else if b == b'\r' {
            if raw.get(at) == Some(&b'\n') {
                at += 1;
            }
            push(&mut out, 10, budget)?;
        } else {
            push(&mut out, b, budget)?;
        }
    }
    Ok(out)
}
fn collect(data: &[u8], header: usize, index: &mut lengths::Index, budget: &Budget<'_>) -> Result<BTreeMap<Id, Raw>> {
    let mut l = Lexer::metered(data, budget.options, budget.cancel, &budget.bytes);
    l.at = header;
    let mut words: Vec<(Vec<u8>, usize)> = Vec::new();
    let mut out = BTreeMap::new();
    let mut active_numbers = std::collections::BTreeSet::new();
    let mut found_xref = false;
    while let Some(token) = l.next()? {
        budget.step()?;
        let start = l.start;
        match token {
            Token::Word(w) if w == b"obj" => {
                let number = |entry: Option<&(Vec<u8>, usize)>| {
                    entry.and_then(|(w, _)| std::str::from_utf8(w).ok()).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| invalid("encrypted object header"))
                };
                let id = (number(words.iter().rev().nth(1))?, u16::try_from(number(words.last())?).map_err(|_| invalid("encrypted object generation"))?);
                if id.0 == 0 || id.0 as usize > budget.options.limits.objects {
                    return Err(ImportError::Limit("encrypted object number"));
                }
                if !active_numbers.insert(id.0) {
                    return Err(invalid("ambiguous active encrypted object generations"));
                }
                let object_start = words.iter().rev().nth(1).map(|v| v.1).ok_or_else(|| invalid("encrypted object start"))?;
                let body_start = l.at;
                let token = l.next()?.ok_or_else(|| invalid("encrypted object value"))?;
                let value = l.value(token, 0)?;
                let dict_end = l.at;
                let next = l.next()?;
                let (mut stream, mut body_end) = (None, dict_end);
                if matches!(&next,Some(Token::Word(w))if w==b"stream") {
                    if l.data.get(l.at) == Some(&b'\r') {
                        l.at += 1;
                    }
                    if l.data.get(l.at) == Some(&b'\n') {
                        l.at += 1;
                    } else if l.data.get(l.at.wrapping_sub(1)) != Some(&b'\r') {
                        return Err(invalid("encrypted stream newline"));
                    }
                    let n = match value.get(b"Length") {
                        Some(Value::Ref(a, b)) => index.resolve((*a, *b), data, budget.options, budget.cancel)?,
                        Some(v) => v.number().ok_or_else(|| invalid("encrypted stream length"))?,
                        None => return Err(invalid("encrypted stream length")),
                    };
                    let end =
                        l.at.checked_add(usize::try_from(n).map_err(|_| ImportError::Limit("encrypted stream length"))?)
                            .filter(|&n| n <= data.len())
                            .ok_or_else(|| invalid("encrypted stream span"))?;
                    stream = Some(l.at..end);
                    l.at = end;
                    if !matches!(l.next()?,Some(Token::Word(w))if w==b"endstream") {
                        return Err(invalid("encrypted stream length mismatch"));
                    }
                    body_end = l.at;
                    if !matches!(l.next()?,Some(Token::Word(w))if w==b"endobj") {
                        return Err(invalid("encrypted stream object end"));
                    }
                } else if !matches!(next,Some(Token::Word(w))if w==b"endobj") {
                    return Err(invalid("encrypted object end"));
                }
                budget.charge(std::mem::size_of::<Raw>() + 64)?;
                if out.insert(id, Raw { body: body_start..body_end, value, stream, dict_end, start: object_start }).is_some() {
                    return Err(invalid("duplicate encrypted object"));
                }
                if out.len() > budget.options.limits.objects {
                    return Err(ImportError::Limit("encrypted object"));
                }
                words.clear();
            }
            Token::Word(w) => {
                if w == b"xref" && start == index.xref_offset() {
                    found_xref = true;
                }
                words.push((w, start));
                if words.len() > 2 {
                    words.remove(0);
                }
            }
            Token::OpenDict => {
                let trailer = l.value(Token::OpenDict, 0)?;
                if trailer.get(b"Root").is_some_and(|v| Some(v) != index.trailer().get(b"Root")) {
                    return Err(invalid("ambiguous encrypted PDF root"));
                }
                if trailer.get(b"Encrypt").is_some_and(|v| Some(v) != index.trailer().get(b"Encrypt")) {
                    return Err(invalid("ambiguous encrypted PDF security handler"));
                }
                words.clear();
            }
            _ => words.clear(),
        }
    }
    if !found_xref || out.len() != index.offsets().len() || index.offsets().iter().any(|(id, offset)| out.get(id).is_none_or(|r| r.start != *offset)) {
        return Err(invalid("encrypted xref does not match admitted object boundaries"));
    }
    Ok(out)
}
fn assemble(objects: &BTreeMap<Id, Vec<u8>>, trailer: &[u8], budget: &Budget<'_>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    budget.append(&mut out, b"%PDF-1.7\n")?;
    let mut offsets = BTreeMap::new();
    for (id, body) in objects {
        budget.step()?;
        offsets.insert(*id, out.len());
        budget.append(&mut out, format!("{} {} obj\n", id.0, id.1).as_bytes())?;
        budget.append(&mut out, body)?;
        budget.append(&mut out, b"\nendobj\n")?;
    }
    let xref = out.len();
    budget.append(&mut out, b"xref\n0 1\n0000000000 65535 f \n")?;
    for (id, offset) in offsets {
        budget.append(&mut out, format!("{} 1\n{offset:010} {:05} n \n", id.0, id.1).as_bytes())?;
    }
    let size = objects.keys().map(|id| id.0).max().unwrap_or(0).saturating_add(1);
    budget.append(&mut out, format!("trailer\n<< /Size {size} ").as_bytes())?;
    budget.append(&mut out, trailer)?;
    budget.append(&mut out, format!(" >>\nstartxref\n{xref}\n%%EOF\n").as_bytes())?;
    Ok(out)
}

// These are deliberately arrays of hex strings, never copies of source PDF
// objects. Even pinned XRef repair sees no stream/ObjStm to decode. Source object
// IDs/generations select the correct encryption key without exposing its graph.
fn transport(
    values: &BTreeMap<Id, Vec<Vec<u8>>>,
    security: &security::Security,
    max: u32,
    payload: bool,
    budget: &Budget<'_>,
) -> Result<BTreeMap<Id, Vec<Vec<u8>>>> {
    use hayro_syntax::object::{Array, MaybeRef, Object, ObjectIdentifier};
    let auth = security.object.unwrap_or((max + 4, 0));
    let mut objects = BTreeMap::new();
    for (&id, strings) in values {
        budget.charge(128)?;
        let mut body = Vec::new();
        budget.append(&mut body, b"[ ")?;
        for bytes in strings {
            serialize::hex(&mut body, bytes, budget)?;
            budget.append(&mut body, b" ")?;
        }
        budget.append(&mut body, b"]")?;
        objects.insert(id, body);
    }
    budget.charge(security.dictionary.len())?;
    objects.insert(auth, if payload { security.payload_dictionary() } else { security.dictionary.clone() });
    let blank_root = (max + 1, 0);
    objects.insert(blank_root, format!("<< /Type /Catalog /Pages {} 0 R >>", max + 2).into_bytes());
    objects.insert((max + 2, 0), format!("<< /Type /Pages /Kids [{} 0 R] /Count 1 >>", max + 3).into_bytes());
    objects.insert((max + 3, 0), b"<< /Type /Page /MediaBox [0 0 1 1] >>".to_vec());
    let trailer = format!(
        "/Root {} 0 R /Encrypt {} {} R /ID {}",
        blank_root.0,
        auth.0,
        auth.1,
        std::str::from_utf8(&security.ids).map_err(|_| invalid("bootstrap ID encoding"))?
    );
    let bootstrap = assemble(&objects, trailer.as_bytes(), budget)?;
    // Reserve pinned parser containers/plaintext upper bounds before entry.
    for strings in values.values() {
        budget.charge(256)?;
        for bytes in strings {
            budget.charge(bytes.len().saturating_add(128))?;
        }
    }
    let parsed = hayro_syntax::Pdf::new(bootstrap).map_err(|error| match error {
        hayro_syntax::LoadPdfError::Decryption(hayro_syntax::DecryptionError::PasswordProtected) => ImportError::Encrypted,
        _ => invalid("invalid encrypted PDF security configuration"),
    })?;
    budget.step()?;
    if parsed.pages().len() != 1 || parsed.xref().root_id().obj_number != blank_root.0 as i32 {
        return Err(invalid("decryption bootstrap root mismatch"));
    }
    #[cfg(test)]
    assert!(parsed.objects().into_iter().all(|object| !matches!(object, Object::Stream(_))), "crypto bootstrap must contain no original or synthetic streams");
    let cipher = if payload { security.streams } else { security.strings };
    let mut result = BTreeMap::new();
    for (&id, encrypted) in values {
        budget.step()?;
        let array =
            parsed.xref().get::<Array<'_>>(ObjectIdentifier::new(id.0 as i32, i32::from(id.1))).ok_or_else(|| invalid("canonical ciphertext transport"))?;
        let mut plain = Vec::new();
        let mut entries = array.raw_iter();
        for original in encrypted {
            budget.step()?;
            let Some(MaybeRef::NotRef(Object::String(string))) = entries.next() else {
                return Err(invalid("canonical ciphertext string"));
            };
            security::validate_cipher(cipher, original.len(), string.as_bytes().len())?;
            budget.charge(string.as_bytes().len().saturating_add(24))?;
            plain.push(string.as_bytes().to_vec());
        }
        if entries.next().is_some() {
            return Err(invalid("canonical ciphertext count"));
        }
        budget.charge(128)?;
        result.insert(id, plain);
    }
    Ok(result)
}

pub(super) fn normalize(data: &[u8], options: &ImportOptions, cancel: &AtomicBool) -> Result<Option<Vec<u8>>> {
    cancelled(cancel)?;
    if data.len() > options.limits.bytes {
        return Err(ImportError::Limit("input byte"));
    }
    let Some(header) = data[..data.len().min(1029)].windows(5).position(|s| s == b"%PDF-") else {
        return Ok(None);
    };
    match lengths::Index::has_encryption(data, header, options, cancel) {
        Ok(true) => {}
        Err(ImportError::Cancelled) => return Err(ImportError::Cancelled),
        _ => return Ok(None),
    }
    let budget = Budget { options, cancel, bytes: Cell::new(0), work: Cell::new(0) };
    let mut index = lengths::Index::read_metered(data, header, options, cancel, &budget.bytes)?;
    if index.trailer().get(b"Encrypt").is_none() {
        return Ok(None);
    }
    let raw = collect(data, header, &mut index, &budget)?;
    let security = security::Security::parse(data, &index, &raw, &budget)?;
    let root = match index.trailer().get(b"Root") {
        Some(Value::Ref(a, b)) => (*a, *b),
        _ => return Err(invalid("encrypted root reference")),
    };
    let max = raw.keys().map(|id| id.0).max().unwrap_or(0);
    if raw.len().saturating_add(4) > options.limits.objects || max > i32::MAX as u32 - 4 {
        return Err(ImportError::Limit("decryption bootstrap object"));
    }
    let mut strings = BTreeMap::new();
    let mut payloads = BTreeMap::new();
    let mut auth_strings = Vec::new();
    for (&id, object) in &raw {
        budget.charge(128)?;
        let cipher = if Some(id) == security.object { security::Cipher::Identity } else { security.strings };
        let prepared = serialize::prepare(data, object, cipher, &budget)?;
        if Some(id) == security.object {
            auth_strings = prepared;
        } else {
            strings.insert(id, prepared);
        }
        serialize::stream_shape(data, object, &security)?;
        if let Some(span) = &object.stream
            && (security.metadata || object.value.get(b"Type").and_then(Value::name) != Some(b"Metadata"))
        {
            budget.charge(span.len().saturating_add(152))?;
            payloads.insert(id, vec![data[span.clone()].to_vec()]);
        }
    }
    // No original bytes are passed to pinned parsing; both bootstrap PDFs contain
    // only canonical arrays/hex strings and the constant authentication/page tree.
    let strings = transport(&strings, &security, max, false, &budget)?;
    let payloads = transport(&payloads, &security, max, true, &budget)?;
    let mut plain = BTreeMap::new();
    for (&id, object) in &raw {
        budget.step()?;
        let values = if Some(id) == security.object { &auth_strings } else { strings.get(&id).ok_or_else(|| invalid("missing decrypted strings"))? };
        let payload = if let Some(span) = &object.stream {
            if !security.metadata && object.value.get(b"Type").and_then(Value::name) == Some(b"Metadata") {
                Some(&data[span.clone()])
            } else {
                Some(payloads.get(&id).and_then(|v| v.first()).ok_or_else(|| invalid("missing decrypted payload"))?.as_slice())
            }
        } else {
            None
        };
        let body = serialize::body(data, object, values, payload, &budget)?;
        budget.charge(128)?;
        plain.insert(id, body);
    }
    let mut trailer = format!("/Root {} {} R /ID {}", root.0, root.1, std::str::from_utf8(&security.ids).map_err(|_| invalid("plaintext ID encoding"))?);
    if let Some(Value::Ref(a, b)) = index.trailer().get(b"Info") {
        trailer.push_str(&format!(" /Info {a} {b} R"));
    }
    Ok(Some(assemble(&plain, trailer.as_bytes(), &budget)?))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use hayro_syntax::object::{Dict, MaybeRef, Number, ObjRef, Object, Stream, String as PdfString};
    const AES: &[u8] = include_bytes!("../../../../fixtures/import/encryption/aes128-r4.pdf");
    fn id(a: i32, b: i32) -> hayro_syntax::object::ObjectIdentifier {
        ObjRef::new(a, b).into()
    }
    #[test]
    fn decryption_preserves_ids_references_strings_and_clear_metadata() {
        let options = ImportOptions::default();
        let cancel = AtomicBool::new(false);
        for bytes in [
            AES,
            include_bytes!("../../../../fixtures/import/encryption/aes128-clear-metadata.pdf").as_slice(),
            include_bytes!("../../../../fixtures/import/encryption/aes128-identity-streams.pdf"),
            include_bytes!("../../../../fixtures/import/encryption/aes128-identity-strings.pdf"),
        ] {
            let plain = normalize(bytes, &options, &cancel).unwrap().unwrap();
            let parsed = super::super::check_plain(&plain, &options, &cancel).unwrap();
            let meta = parsed.xref().get::<Stream<'_>>(id(5, 0)).unwrap();
            assert_eq!(meta.raw_data().as_ref(), b"<Image>plaintext metadata EI</Image>");
            assert_eq!(meta.dict().get::<PdfString<'_>>(b"Description").unwrap().as_bytes(), b"metadata dictionary string");
            let info = parsed.xref().get::<Dict<'_>>(id(6, 0)).unwrap();
            assert_eq!(info.get::<PdfString<'_>>(b"Title").unwrap().as_bytes(), b"Permission-only EI title");
            assert_eq!(info.get::<PdfString<'_>>(b"Private").unwrap().as_bytes(), b"\x00\xff exact string");
            let Object::Array(array) = parsed.xref().get::<Object<'_>>(id(7, 0)).unwrap() else { panic!("array") };
            let mut entries = array.raw_iter();
            assert!(matches!(entries.next(),Some(MaybeRef::Ref(r)) if r.obj_number==4&&r.gen_number==2));
            assert!(matches!(entries.next(),Some(MaybeRef::NotRef(Object::String(s))) if s.as_bytes()==b"array literal"));
            assert_eq!(parsed.xref().get::<Number>(id(8, 0)).unwrap().as_i64(), 1844674407370955161);
        }
    }
    #[test]
    fn decryption_bootstrap_does_not_walk_original_cycle_or_decode_bomb() {
        let options = ImportOptions::default();
        let cancel = AtomicBool::new(false);
        let mut cycle = AES.to_vec();
        let at = cycle.windows(b"/Kids [3 0 R]".len()).position(|s| s == b"/Kids [3 0 R]").unwrap();
        cycle[at + b"/Kids [".len()] = b'2';
        let plain = normalize(&cycle, &options, &cancel).unwrap().unwrap();
        assert!(super::super::check_plain(&plain, &options, &cancel).is_err(), "original tree is visited only by guarded plaintext admission");
        let bomb = include_bytes!("../../../../fixtures/import/encryption/invalid-bomb.pdf");
        let mut options = options;
        options.limits.decompressed_bytes = 64 << 10;
        let plain = normalize(bomb, &options, &cancel).unwrap().unwrap();
        assert!(matches!(super::super::check_plain(&plain, &options, &cancel), Err(ImportError::Limit("decompressed byte"))));
    }
    #[test]
    fn decryption_rejects_xref_offsets_outside_verified_object_starts() {
        let mut changed = AES.to_vec();
        let xref = changed.windows(5).position(|w| w == b"xref\n").unwrap();
        let marker = b"4 1\n";
        let at = xref + changed[xref..].windows(marker.len()).position(|w| w == marker).unwrap() + marker.len();
        changed[at..at + 10].copy_from_slice(b"0000000001");
        let result = normalize(&changed, &ImportOptions::default(), &AtomicBool::new(false));
        assert!(matches!(result, Err(ImportError::Invalid(message)) if message.contains("object boundaries")));
    }
    #[test]
    fn decryption_object_members_are_expanded_only_after_plaintext_admission() {
        let source = include_bytes!("../../../../fixtures/import/encryption/aes128-object-members.pdf");
        let options = ImportOptions::default();
        let cancel = AtomicBool::new(false);
        let plain = normalize(source, &options, &cancel).unwrap().unwrap();
        let parsed = super::super::check_plain(&plain, &options, &cancel).unwrap();
        let member = parsed.xref().get::<Dict<'_>>(id(11, 0)).unwrap();
        assert_eq!(member.get::<PdfString<'_>>(b"PrivateString").unwrap().as_bytes(), b"member stays plaintext after stream decryption");
    }
    #[test]
    fn decryption_unreadable_values_cannot_trigger_object_stream_repair() {
        let mut options = ImportOptions::default();
        options.limits.decompressed_bytes = 256 << 10;
        let cancel = AtomicBool::new(false);
        for source in [
            include_bytes!("../../../../fixtures/import/encryption/invalid-object-bomb-exponent.pdf").as_slice(),
            include_bytes!("../../../../fixtures/import/encryption/invalid-object-bomb-unknown.pdf"),
            include_bytes!("../../../../fixtures/import/encryption/invalid-object-bomb-nested-exponent.pdf"),
            include_bytes!("../../../../fixtures/import/encryption/invalid-object-bomb-nested-unknown.pdf"),
            include_bytes!("../../../../fixtures/import/encryption/invalid-object-bomb-hex.pdf"),
            include_bytes!("../../../../fixtures/import/encryption/invalid-object-bomb-name.pdf"),
            include_bytes!("../../../../fixtures/import/encryption/invalid-object-bomb-number-overflow.pdf"),
        ] {
            assert!(
                matches!(normalize(source, &options, &cancel), Err(ImportError::Invalid(_))),
                "malformed value must fail before any pinned bootstrap parser"
            );
        }
        let source = include_bytes!("../../../../fixtures/import/encryption/aes128-object-bomb.pdf");
        let plain = normalize(source, &options, &cancel).unwrap().unwrap();
        // normalize's transport assertions also inspect all pinned bootstrap
        // objects: none is a Stream. The first ObjStm expansion occurs here.
        assert!(matches!(super::super::check_plain(&plain, &options, &cancel), Err(ImportError::Limit("decompressed byte"))));
    }
    #[test]
    fn decryption_canonicalizes_direct_real_length_and_rejects_indirect_real() {
        let options = ImportOptions::default();
        let cancel = AtomicBool::new(false);
        let source = include_bytes!("../../../../fixtures/import/encryption/aes128-direct-real-length.pdf");
        let plain = normalize(source, &options, &cancel).unwrap().unwrap();
        let parsed = super::super::check_plain(&plain, &options, &cancel).unwrap();
        let stream = parsed.xref().get::<Stream<'_>>(id(4, 2)).unwrap();
        assert_eq!(stream.dict().get::<u32>(b"Length").unwrap() as usize, stream.raw_data().len());
        assert!(matches!(
            normalize(include_bytes!("../../../../fixtures/import/encryption/invalid-indirect-real-length.pdf"), &options, &cancel),
            Err(ImportError::Invalid(_))
        ));
    }
    #[test]
    fn decryption_probe_leaves_plaintext_and_nonclassic_input_to_normal_guard() {
        for source in [b"%PDF-1.7\n1 0 obj << /Type /XRef >> endobj\nstartxref\n9\n%%EOF".as_slice(), b"%PDF-1.7\n1 0 obj 1e2 endobj\n"] {
            assert!(normalize(source, &ImportOptions::default(), &AtomicBool::new(false)).unwrap().is_none());
        }
    }
}
