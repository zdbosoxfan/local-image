//! Exact inline-image admission plus marker-safe canonical content. Pinned hayro
//! guesses EI boundaries, so raw binary never reaches that heuristic after admission.
use super::{Guard, ImportError, Lexer, Result, Token, Value, cancelled, ccitt_admission, filters, invalid, lzw, predictors, white};
use std::collections::BTreeMap;

pub(super) struct Edit {
    start: usize,
    end: usize,
    replacement: Option<Vec<u8>>,
}
pub(super) struct Content {
    pub work: usize,
    pub calls: Vec<Vec<u8>>,
    pub edits: Vec<Edit>,
    pub has_inline: bool,
}
fn append(g: &mut Guard<'_>, out: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    g.charge(bytes.len() as u64)?;
    out.extend_from_slice(bytes);
    Ok(())
}
fn escaped(g: &mut Guard<'_>, raw: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let literal = raw.first() == Some(&b'(');
    if raw.first() == Some(&b'<') {
        if !raw[1..raw.len().saturating_sub(1)].iter().all(|b| white(*b) || b.is_ascii_hexdigit()) {
            return Err(invalid("content hex string"));
        }
        append(g, &mut out, raw)?;
        return Ok(out);
    }
    let mut at = 0;
    while at < raw.len() {
        if at.is_multiple_of(4096) {
            cancelled(g.cancel)?;
        }
        if literal && raw[at] == b'\\' && at + 1 < raw.len() {
            if raw[at + 1] == b'I' {
                append(g, &mut out, b"\\111")?;
            } else {
                append(g, &mut out, &raw[at..at + 2])?;
            }
            at += 2;
        } else {
            if raw[at] == b'I' {
                append(g, &mut out, if literal { b"\\111" } else { b"#49" })?;
            } else {
                append(g, &mut out, &raw[at..at + 1])?;
            }
            at += 1;
        }
    }
    Ok(out)
}
fn fragment(g: &mut Guard<'_>, raw: &[u8]) -> Result<Vec<u8>> {
    let mut l = Lexer::new(raw, g.options, g.cancel);
    let mut out = Vec::new();
    while let Some(t) = l.next()? {
        if matches!(t, Token::Name(_) | Token::String) {
            let value = escaped(g, &raw[l.start..l.at])?;
            append(g, &mut out, &value)?;
        } else {
            if matches!(&t, Token::Word(w) if w.windows(2).any(|s| s == b"EI" || s == b"BI") || w == b"ID") {
                return Err(invalid("ambiguous marker in inline dictionary"));
            }
            append(g, &mut out, &raw[l.start..l.at])?;
        }
        append(g, &mut out, b" ")?;
    }
    Ok(out)
}
fn name(g: &mut Guard<'_>, out: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    append(g, out, b"/")?;
    for &b in bytes {
        if super::delim(b) || b == b'#' || b == b'I' || !b.is_ascii_graphic() {
            append(g, out, &[b'#', HEX[usize::from(b >> 4)], HEX[usize::from(b & 15)]])?;
        } else {
            append(g, out, &[b])?;
        }
    }
    Ok(())
}
fn alias(key: &[u8]) -> &[u8] {
    match key {
        b"W" => b"Width",
        b"H" => b"Height",
        b"BPC" => b"BitsPerComponent",
        b"CS" => b"ColorSpace",
        b"F" => b"Filter",
        b"DP" => b"DecodeParms",
        b"D" => b"Decode",
        b"IM" => b"ImageMask",
        b"I" => b"Interpolate",
        b"L" => b"Length",
        _ => key,
    }
}
fn flate_end(g: &Guard<'_>, data: &[u8], room: u64) -> Result<usize> {
    let mut decoder = flate2::Decompress::new(true);
    let mut output = [0u8; 8192];
    loop {
        cancelled(g.cancel)?;
        let (before_in, before_out) = (decoder.total_in(), decoder.total_out());
        let status = decoder.decompress(&data[before_in as usize..], &mut output, flate2::FlushDecompress::None).map_err(|_| invalid("inline deflate"))?;
        if decoder.total_out() > room {
            return Err(ImportError::Limit("decompressed byte"));
        }
        if status == flate2::Status::StreamEnd {
            return usize::try_from(decoder.total_in()).map_err(|_| ImportError::Limit("inline byte"));
        }
        if decoder.total_in() == before_in && decoder.total_out() == before_out {
            return Err(invalid("truncated inline deflate"));
        }
    }
}
fn jpeg_end(g: &Guard<'_>, data: &[u8]) -> Result<usize> {
    if !data.starts_with(&[255, 216]) {
        return Err(invalid("inline JPEG SOI"));
    }
    let mut at = 2usize;
    let mut entropy = false;
    loop {
        cancelled(g.cancel)?;
        if entropy {
            while data.get(at).is_some_and(|b| *b != 255) {
                if at.is_multiple_of(4096) {
                    cancelled(g.cancel)?;
                }
                at += 1;
            }
        }
        if data.get(at) != Some(&255) {
            return Err(invalid("truncated inline JPEG marker"));
        }
        while data.get(at) == Some(&255) {
            at += 1;
            if at.is_multiple_of(4096) {
                cancelled(g.cancel)?;
            }
        }
        let marker = *data.get(at).ok_or_else(|| invalid("inline JPEG marker"))?;
        at += 1;
        match marker {
            217 => return Ok(at),
            0 if entropy => continue,
            208..=215 if entropy => continue,
            1 => continue,
            0 | 216 => return Err(invalid("inline JPEG marker order")),
            _ => {
                let size = data.get(at..at + 2).ok_or_else(|| invalid("inline JPEG segment"))?;
                let n = usize::from(u16::from_be_bytes([size[0], size[1]]));
                if n < 2 {
                    return Err(invalid("inline JPEG segment length"));
                }
                at = at.checked_add(n).filter(|&n| n <= data.len()).ok_or_else(|| invalid("truncated inline JPEG segment"))?;
                if marker == 218 {
                    entropy = true;
                }
            }
        }
    }
}
fn ei_separator(g: &Guard<'_>, data: &[u8], mut at: usize) -> Result<bool> {
    if !data.get(at).is_some_and(|b| white(*b)) {
        return Ok(false);
    }
    loop {
        cancelled(g.cancel)?;
        while data.get(at).is_some_and(|b| white(*b)) {
            at += 1;
            if at.is_multiple_of(4096) {
                cancelled(g.cancel)?;
            }
        }
        if data.get(at) != Some(&b'%') {
            break;
        }
        while data.get(at).is_some_and(|b| !matches!(b, 10 | 13)) {
            at += 1;
            if at.is_multiple_of(4096) {
                cancelled(g.cancel)?;
            }
        }
    }
    Ok(data.get(at..at + 2) == Some(b"EI") && data.get(at + 2).is_none_or(|b| super::delim(*b)))
}

// Only inspect positions derived from completed rows and at most six EOLs.
// Try the row boundary first: padding plus PDF LF must never become a fake EOL.
fn fax_end(g: &mut Guard<'_>, value: &Value, params: Option<&Value>, data: &[u8], room: u64) -> Result<usize> {
    let (cost, mut bit) = ccitt_admission::prefix_bits(value, params, data, g.options, room, g.cancel)?;
    g.charge(cost)?;
    let group4 = matches!(params.and_then(|p|p.get(b"K")), Some(Value::Number(k)) if *k < 0.0);
    for marker in 0..=if group4 { 2 } else { 6 } {
        cancelled(g.cancel)?;
        let byte = bit.div_ceil(8);
        if ei_separator(g, data, byte)? {
            return Ok(byte);
        }
        if marker == if group4 { 2 } else { 6 } {
            break;
        }
        let mut zeros = 0;
        while zeros <= 24 && data.get(bit / 8).is_some() && ((data[bit / 8] >> (7 - bit % 8)) & 1) == 0 {
            zeros += 1;
            bit += 1;
        }
        if !(11..=24).contains(&zeros) || data.get(bit / 8).is_none() {
            break;
        }
        bit += 1;
    }
    Err(invalid("CCITT row or terminal boundary is not followed by EI"))
}

fn image(g: &mut Guard<'_>, l: &mut Lexer<'_>) -> Result<Vec<u8>> {
    g.inline_images = g.inline_images.saturating_add(1);
    if g.inline_images > g.options.limits.objects.saturating_sub(g.objects.len()) {
        return Err(ImportError::Limit("inline image object"));
    }
    let mut fields = BTreeMap::new();
    let mut raw_fields = BTreeMap::new();
    loop {
        match l.next()?.ok_or_else(|| invalid("unterminated inline dictionary"))? {
            Token::Word(w) if w == b"ID" => break,
            Token::Name(key) => {
                let key = alias(&key).to_vec();
                let token = l.next()?.ok_or_else(|| invalid("inline dictionary value"))?;
                let start = l.start;
                let mut value = l.value(token, 1)?;
                let mut raw = fragment(g, &l.data[start..l.at])?;
                if key == b"ColorSpace"
                    && let Value::Name(name) = &mut value
                {
                    *name = match name.as_slice() {
                        b"G" => b"DeviceGray".to_vec(),
                        b"RGB" => b"DeviceRGB".to_vec(),
                        b"CMYK" => b"DeviceCMYK".to_vec(),
                        _ => name.clone(),
                    };
                    raw = [b"/".as_slice(), name.as_slice()].concat();
                }
                let mut refs = Vec::new();
                value.refs(&mut refs);
                if !refs.is_empty() {
                    return Err(ImportError::Unsupported("inline image indirect values"));
                }
                if fields.insert(key.clone(), value).is_some() {
                    return Err(invalid("duplicate inline image key or alias"));
                }
                raw_fields.insert(key, raw);
            }
            _ => return Err(invalid("inline dictionary key")),
        }
    }
    let separator = *l.data.get(l.at).filter(|b| white(**b)).ok_or_else(|| invalid("inline ID whitespace"))?;
    l.at += 1;
    if separator == b'\r' && l.data.get(l.at) == Some(&b'\n') {
        l.at += 1;
    }
    let start = l.at;
    fields.insert(b"Subtype".to_vec(), Value::Name(b"Image".to_vec()));
    let value = Value::Dict(fields);
    g.inspect(&value)?;
    let width = value.get(b"Width").and_then(Value::number).filter(|n| *n > 0).ok_or_else(|| invalid("inline width"))?;
    let height = value.get(b"Height").and_then(Value::number).filter(|n| *n > 0).ok_or_else(|| invalid("inline height"))?;
    let stencil = matches!(value.get(b"ImageMask"), Some(Value::Bool(true)));
    let components = if stencil {
        1
    } else {
        match value.get(b"ColorSpace").and_then(Value::name) {
            Some(b"DeviceGray") => 1,
            Some(b"DeviceRGB") => 3,
            Some(b"DeviceCMYK") => 4,
            _ => return Err(ImportError::Unsupported("inline image color space requires resource resolution")),
        }
    };
    let bits = value.get(b"BitsPerComponent").and_then(Value::number).unwrap_or(if stencil { 1 } else { 0 });
    if !matches!(bits, 1 | 2 | 4 | 8 | 16) {
        return Err(invalid("inline component bits"));
    }
    let size = width
        .checked_mul(components)
        .and_then(|n| n.checked_mul(bits))
        .map(|n| n.div_ceil(8))
        .and_then(|n| n.checked_mul(height))
        .ok_or(ImportError::Limit("inline image byte"))?;
    let room = g.options.limits.decompressed_bytes.saturating_sub(g.bytes);
    if width > 1_000_000 || height > 1_000_000 || width.saturating_mul(height) > g.options.limits.pixels || size > room {
        return Err(ImportError::Limit("inline image allocation"));
    }
    let names = match value.get(b"Filter") {
        None => Vec::new(),
        Some(Value::Name(n)) => vec![n.as_slice()],
        Some(Value::Array(a)) => a.iter().map(|v| v.name().ok_or(ImportError::Unsupported("inline image filters"))).collect::<Result<Vec<_>>>()?,
        _ => return Err(ImportError::Unsupported("inline image filters")),
    };
    if names.len() + 1 > g.options.limits.depth.min(64) {
        return Err(ImportError::Limit("inline filter chain"));
    }
    if names.iter().enumerate().any(|(i, n)| matches!(*n, b"DCTDecode" | b"DCT" | b"CCITTFaxDecode" | b"CCF") && i + 1 != names.len()) {
        return Err(ImportError::Unsupported("inline image decoder order"));
    }
    let params = predictors::parameters(&value, matches!(value.get(b"Filter"), Some(Value::Array(_))), names.len())?;
    let tail = &l.data[start..];
    let consumed = match names.first().copied() {
        None => usize::try_from(size).map_err(|_| ImportError::Limit("inline image byte"))?,
        Some(f @ (b"ASCIIHexDecode" | b"AHx" | b"ASCII85Decode" | b"A85" | b"RunLengthDecode" | b"RL")) => {
            let (decoded, n) = filters::prefix(f, tail, room, g.cancel)?;
            g.charge(decoded.len() as u64)?;
            n
        }
        Some(b"FlateDecode" | b"Fl") => flate_end(g, tail, room)?,
        Some(b"LZWDecode" | b"LZW") => {
            let early = predictors::parameter(params.first().copied().flatten(), b"EarlyChange", 1)?;
            if early > 1 {
                return Err(invalid("inline LZW EarlyChange"));
            }
            let (_, cost, n) = lzw::prefix(tail, early == 1, room, g.options.limits.segments.saturating_mul(16).min(16_000_000), g.cancel)?;
            g.charge(cost)?;
            n
        }
        Some(b"CCITTFaxDecode" | b"CCF") => fax_end(g, &value, params.first().copied().flatten(), tail, room)?,
        Some(b"DCTDecode" | b"DCT") => jpeg_end(g, tail)?,
        _ => return Err(ImportError::Unsupported("inline codec has no exact bounded endpoint")),
    };
    if let Some(declared) = value.get(b"Length")
        && declared.number() != Some(consumed as u64)
    {
        return Err(invalid("inline Length mismatch"));
    }
    let end = start.checked_add(consumed).filter(|&end| end <= l.data.len()).ok_or_else(|| invalid("truncated inline pixels"))?;
    if !l.data.get(end).is_some_and(|b| white(*b)) {
        return Err(invalid("inline EI separator"));
    }
    l.at = end;
    if !matches!(l.next()?,Some(Token::Word(w)) if w==b"EI") {
        return Err(invalid("inline codec end is not followed by EI"));
    }
    let pixels = g.stream(&value, &l.data[start..end])?;
    if !names.last().is_some_and(|n| matches!(*n, b"DCTDecode" | b"DCT" | b"CCITTFaxDecode" | b"CCF")) && pixels.len() as u64 != size {
        return Err(invalid("inline decoded row length mismatch"));
    }
    let mut out = Vec::new();
    append(g, &mut out, b"BI ")?;
    for (key, raw) in &raw_fields {
        if matches!(key.as_slice(), b"Filter" | b"DecodeParms" | b"Length") {
            continue;
        }
        name(g, &mut out, key)?;
        append(g, &mut out, b" ")?;
        append(g, &mut out, raw)?;
        append(g, &mut out, b" ")?;
    }
    append(g, &mut out, b"/Filter [/ASCIIHexDecode ")?;
    for n in &names {
        name(g, &mut out, n)?;
        append(g, &mut out, b" ")?;
    }
    append(g, &mut out, b"] /DecodeParms [null ")?;
    if let Some(raw) = raw_fields.get(b"DecodeParms".as_slice()).filter(|_| !matches!(value.get(b"DecodeParms"), Some(Value::Null))) {
        let fragment = if matches!(value.get(b"DecodeParms"), Some(Value::Array(_))) {
            let a = raw.iter().position(|b| *b == b'[').ok_or_else(|| invalid("inline params"))?;
            let b = raw.iter().rposition(|b| *b == b']').ok_or_else(|| invalid("inline params"))?;
            &raw[a + 1..b]
        } else {
            raw.as_slice()
        };
        append(g, &mut out, fragment)?;
    } else {
        for _ in &names {
            append(g, &mut out, b"null ")?;
        }
    }
    append(g, &mut out, b"] ID ")?;
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for chunk in l.data[start..end].chunks(4096) {
        cancelled(g.cancel)?;
        let mut encoded = [0u8; 8192];
        for (i, b) in chunk.iter().enumerate() {
            encoded[2 * i] = HEX[usize::from(b >> 4)];
            encoded[2 * i + 1] = HEX[usize::from(b & 15)];
        }
        append(g, &mut out, &encoded[..chunk.len() * 2])?;
    }
    append(g, &mut out, b"> EI")?;
    l.tokens = l.tokens.saturating_add(16 + names.len() * 2);
    if l.tokens > g.options.limits.segments.saturating_mul(16).min(16_000_000) {
        return Err(ImportError::Limit("normalized syntax token"));
    }
    Ok(out)
}

fn edit(g: &mut Guard<'_>, edits: &mut Vec<Edit>, start: usize, end: usize, replacement: Option<Vec<u8>>) -> Result<()> {
    g.charge(std::mem::size_of::<Edit>() as u64)?;
    edits.push(Edit { start, end, replacement });
    Ok(())
}
pub(super) fn analyze(g: &mut Guard<'_>, data: &[u8], form_group: bool) -> Result<Content> {
    let mut l = Lexer::new(data, g.options, g.cancel);
    let (mut depth, mut saves, mut previous) = (0usize, 0usize, 0usize);
    let mut marked = 0isize;
    let mut last_name = None;
    let mut calls = Vec::new();
    let mut edits = Vec::new();
    let mut has_inline = false;
    loop {
        let token = l.next()?;
        let start = if token.is_some() { l.start } else { l.at };
        if data[previous..start].contains(&b'%') {
            edit(g, &mut edits, previous, start, Some(vec![b' ']))?;
        }
        let Some(token) = token else { break };
        if matches!(&token, Token::Name(_) | Token::String) && data[start..l.at].contains(&b'I') {
            edit(g, &mut edits, start, l.at, None)?;
        }
        match token {
            Token::Name(n) => {
                if n.starts_with(super::groups::PREFIX) {
                    return Err(invalid("reserved PDF group marker"));
                }
                last_name = Some(n);
            }
            Token::Word(w) if depth == 0 && (w == b"BMC" || w == b"BDC") => {
                marked += 1;
                if marked > g.options.limits.depth.min(64) as isize {
                    return Err(ImportError::Limit("marked content depth"));
                }
            }
            Token::Word(w) if depth == 0 && w == b"EMC" => {
                marked -= 1;
                if form_group && marked < 0 {
                    return Err(invalid("unbalanced marked content"));
                }
            }
            Token::Word(w) if w == b"BI" => {
                if depth != 0 {
                    return Err(invalid("inline image inside operand container"));
                }
                let replacement = image(g, &mut l)?;
                edit(g, &mut edits, start, l.at, Some(replacement))?;
                has_inline = true;
                last_name = None;
            }
            Token::Word(w) if w == b"Do" => {
                if let Some(n) = last_name.take() {
                    calls.push(n);
                }
            }
            Token::Word(w) if w.windows(2).any(|s| s == b"EI" || s == b"BI") || w == b"ID" => return Err(invalid("ambiguous inline marker outside image")),
            Token::OpenArray | Token::OpenDict => {
                depth += 1;
                if depth > g.options.limits.depth.min(64) {
                    return Err(ImportError::Limit("content depth"));
                }
            }
            Token::CloseArray | Token::CloseDict => depth = depth.saturating_sub(1),
            Token::Word(w) if w == b"q" => {
                saves += 1;
                if saves.saturating_add(usize::from(form_group)) > g.options.limits.depth.min(64) {
                    return Err(ImportError::Limit("graphics state depth"));
                }
            }
            Token::Word(w) if w == b"Q" => {
                if form_group && saves == 0 {
                    return Err(invalid("Form graphics state underflow"));
                }
                saves = saves.saturating_sub(1);
            }
            _ => {}
        }
        previous = l.at;
    }
    if form_group && saves != 0 {
        return Err(invalid("unbalanced Form graphics state"));
    }
    if form_group && marked != 0 {
        return Err(invalid("unbalanced marked content"));
    }
    Ok(Content { work: l.tokens, calls, edits, has_inline })
}
pub(super) fn materialize(g: &mut Guard<'_>, data: &[u8], edits: &[Edit]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut at = 0;
    for edit in edits {
        cancelled(g.cancel)?;
        append(g, &mut output, &data[at..edit.start])?;
        if let Some(bytes) = &edit.replacement {
            append(g, &mut output, bytes)?;
        } else {
            let bytes = escaped(g, &data[edit.start..edit.end])?;
            append(g, &mut output, &bytes)?;
        }
        at = edit.end;
    }
    append(g, &mut output, &data[at..])?;
    Ok(output)
}
/// Preserve untouched dictionary byte spans, including opaque strings/metadata.
/// Only transport fields change when decoded content is made marker-safe.
pub(super) fn rewrite(g: &mut Guard<'_>, body: &[u8], data: &[u8], value: &mut Value) -> Result<Vec<u8>> {
    let mut l = Lexer::new(body, g.options, g.cancel);
    if !matches!(l.next()?, Some(Token::OpenDict)) {
        return Err(invalid("content stream dictionary"));
    }
    let mut out = Vec::new();
    append(g, &mut out, b"<< ")?;
    loop {
        match l.next()?.ok_or_else(|| invalid("content stream dictionary"))? {
            Token::CloseDict => break,
            Token::Name(key) => {
                let start = l.start;
                let token = l.next()?.ok_or_else(|| invalid("content field"))?;
                l.value(token, 0)?;
                if !matches!(key.as_slice(), b"Length" | b"Filter" | b"DecodeParms" | b"F" | b"DP") {
                    append(g, &mut out, &body[start..l.at])?;
                    append(g, &mut out, b" ")?;
                }
            }
            _ => return Err(invalid("content dictionary key")),
        }
    }
    append(g, &mut out, format!("/Length {} >>\nstream\n", data.len()).as_bytes())?;
    append(g, &mut out, data)?;
    append(g, &mut out, b"\nendstream")?;
    let Value::Dict(dict) = value else {
        return Err(invalid("content record dictionary"));
    };
    for key in [b"Filter".as_slice(), b"DecodeParms", b"F", b"DP"] {
        dict.remove(key);
    }
    dict.insert(b"Length".to_vec(), Value::Number(data.len() as f64));
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::super::ImportOptions;
    use super::*;
    use std::{collections::BTreeSet, io::Write, sync::atomic::AtomicBool};
    fn guard<'a>(options: &'a ImportOptions, cancel: &'a AtomicBool) -> Guard<'a> {
        Guard { options, cancel, objects: BTreeMap::new(), bytes: 0, streams: vec![], binary: BTreeSet::new(), inline_images: 0 }
    }
    #[test]
    fn inline_codec_prefixes_stop_before_following_pdf_operators() {
        let cancel = AtomicBool::new(false);
        let options = ImportOptions::default();
        let g = guard(&options, &cancel);
        let suffix = b" EI Q /X Do";
        for (codec, encoded, expected) in
            [(b"AHx".as_slice(), b"4142>".as_slice(), b"AB".as_slice()), (b"A85", b"9jqo^~>", b"Man "), (b"RL", &[1, b'A', b'B', 128], b"AB")]
        {
            let bytes = [encoded, suffix].concat();
            let (decoded, end) = filters::prefix(codec, &bytes, 1024, &cancel).unwrap();
            assert_eq!(decoded, expected);
            assert_eq!(end, encoded.len());
            assert_eq!(&bytes[end..], suffix);
            assert!(filters::decode(codec, &bytes, 1024, &cancel).is_err(), "whole-stream API must remain strict");
        }
        let encoded = [0x80, 0x0b, 0x60, 0x50, 0x22, 0x0c, 0x0c, 0x85, 0x01];
        let bytes = [encoded.as_slice(), suffix].concat();
        let (decoded, _, end) = lzw::prefix(&bytes, true, 1 << 20, 1000, &cancel).unwrap();
        assert_eq!(decoded, b"-----A---B");
        assert_eq!(end, encoded.len());
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(b"binary EI BI samples").unwrap();
        let encoded = encoder.finish().unwrap();
        let bytes = [encoded.as_slice(), suffix].concat();
        assert_eq!(flate_end(&g, &bytes, 1024).unwrap(), encoded.len());
        assert!(flate_end(&g, &encoded[..encoded.len() - 1], 1024).is_err());
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg).encode(&[255, 0, 0, 0, 255, 0], 2, 1, image::ExtendedColorType::Rgb8).unwrap();
        let bytes = [jpeg.as_slice(), suffix].concat();
        assert_eq!(jpeg_end(&g, &bytes).unwrap(), jpeg.len());
        assert!(jpeg_end(&g, &jpeg[..jpeg.len() - 1]).is_err());
        let value = Value::Dict(BTreeMap::from([
            (b"Width".to_vec(), Value::Number(8.)),
            (b"Height".to_vec(), Value::Number(4.)),
            (b"BitsPerComponent".to_vec(), Value::Number(1.)),
            (b"ColorSpace".to_vec(), Value::Name(b"DeviceGray".to_vec())),
        ]));
        let params = Value::Dict(BTreeMap::from([(b"K".to_vec(), Value::Number(-1.)), (b"Columns".to_vec(), Value::Number(8.))]));
        let bits = b"1111000000000001000000000001"; // four white V0 rows and EOFB
        let encoded: Vec<u8> = bits.chunks(8).map(|part| part.iter().enumerate().fold(0, |v, (i, b)| v | u8::from(*b == b'1') << (7 - i))).collect();
        let bytes = [encoded.as_slice(), suffix].concat();
        let mut g = guard(&options, &cancel);
        let end = fax_end(&mut g, &value, Some(&params), &bytes, 1 << 20).unwrap();
        assert_eq!(end, encoded.len());
    }
    #[test]
    fn inline_escapes_keep_exact_literal_and_name_values_in_pinned_parser() {
        use hayro_syntax::content::{TypedIter, ops::TypedInstruction};
        let options = ImportOptions::default();
        let cancel = AtomicBool::new(false);
        let mut g = guard(&options, &cancel);
        let literal = |raw: &[u8]| {
            let source = [raw, b" Tj"].concat();
            let mut iter = TypedIter::new(&source);
            match iter.next().unwrap() {
                TypedInstruction::ShowText(t) => t.0.as_bytes().to_vec(),
                _ => panic!("text fixture"),
            }
        };
        for raw in [b"(BI EI I7 \\I \\\\I \\111 \\(I\\) \\rI)".as_slice(), b"(\xffI\0I)", b"<4249204549>"] {
            let encoded = escaped(&mut g, raw).unwrap();
            assert_eq!(literal(&encoded), literal(raw));
            assert!(!encoded.windows(2).any(|p| p == b"BI" || p == b"EI"));
        }
        let name = |raw: &[u8]| {
            let source = [raw, b" Do"].concat();
            let mut iter = TypedIter::new(&source);
            match iter.next().unwrap() {
                TypedInstruction::XObject(n) => n.0.as_ref().to_vec(),
                _ => panic!("name fixture"),
            }
        };
        for raw in [b"/B#49_EI#20I".as_slice(), b"/A#23BI#2FI"] {
            let encoded = escaped(&mut g, raw).unwrap();
            assert_eq!(name(&encoded), name(raw));
        }
    }
    #[test]
    fn inline_canonical_stream_rewrite_retains_opaque_dictionary_spans() {
        let options = ImportOptions::default();
        let cancel = AtomicBool::new(false);
        let mut g = guard(&options, &cancel);
        let body = b"<< /Length 0 /Filter /FlateDecode /DecodeParms null /Foreign (EI BI \\111) /Private <00ff> >>\nstream\n\nendstream";
        let mut lexer = Lexer::new(body, &options, &cancel);
        let token = lexer.next().unwrap().unwrap();
        let mut value = lexer.value(token, 0).unwrap();
        let rewritten = rewrite(&mut g, body, b"q Q", &mut value).unwrap();
        assert!(rewritten.windows(b"/Foreign (EI BI \\111)".len()).any(|s| s == b"/Foreign (EI BI \\111)"));
        assert!(rewritten.windows(b"/Private <00ff>".len()).any(|s| s == b"/Private <00ff>"));
        assert!(value.get(b"Filter").is_none());
        assert!(value.get(b"DecodeParms").is_none());
        assert_eq!(value.get(b"Length").and_then(Value::number), Some(3));
        let mut lexer = Lexer::new(&rewritten, &options, &cancel);
        let token = lexer.next().unwrap().unwrap();
        assert_eq!(lexer.value(token, 0).unwrap(), value);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod scope_tests {
    use super::super::{ImportOptions, check};
    use super::*;
    use std::sync::atomic::AtomicBool;
    fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
        let mut out = format!("<< /Length {} {dict} >>\nstream\n", data.len()).into_bytes();
        out.extend_from_slice(data);
        out.extend_from_slice(b"\nendstream");
        out
    }
    fn pdf(objects: &[Vec<u8>]) -> Vec<u8> {
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for n in offsets {
            out.extend_from_slice(format!("{n:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
        out
    }
    #[test]
    fn inline_scope_preserves_xml_cmap_and_unknown_resource_bytes() {
        use hayro_syntax::object::{Name, Object};
        let mut objects = vec![
            b"<< /Type /Catalog /Pages 2 0 R /Metadata 5 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 8 8] /Contents 4 0 R >>".to_vec(),
            stream("", b"BI /W 1 /H 1 /BPC 8 /CS /G ID X EI"),
            stream("/Type /Metadata /Subtype /XML", b"<Image>value</Image>"),
            stream("/Type /CMap", b"/CIDInit /ProcSet findresource begin (EI BI) pop end"),
            stream("/Private true", b"(EI BI) pop %opaque comment EI\n"),
        ];
        let parsed = check(&pdf(&objects), &ImportOptions::default(), &AtomicBool::new(false)).unwrap();
        let raw: Vec<_> = parsed
            .objects()
            .into_iter()
            .filter_map(|o| {
                if let Object::Stream(s) = o { Some((s.dict().get::<Name<'_>>(b"Type").map(|n| n.as_ref().to_vec()), s.raw_data().into_owned())) } else { None }
            })
            .collect();
        assert!(raw.iter().any(|(kind, data)| kind.as_deref() == Some(b"Metadata") && data == b"<Image>value</Image>"));
        assert!(raw.iter().any(|(kind, data)| kind.as_deref() == Some(b"CMap") && data == b"/CIDInit /ProcSet findresource begin (EI BI) pop end"));
        assert!(raw.iter().any(|(_, data)| data == b"(EI BI) pop %opaque comment EI\n"));
        // Opaque resources cannot become a zero-work content bypass.
        objects[2] = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 8 8] /Contents [4 0 R 5 0 R] >>".to_vec();
        assert!(matches!(
            check(&pdf(&objects), &ImportOptions::default(), &AtomicBool::new(false)),
            Err(ImportError::Unsupported("binary resource reused as page content"))
        ));
        // An indirect Contents array must still normalize every actual member.
        objects[2] = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 8 8] /Contents 8 0 R >>".to_vec();
        objects.push(b"[4 0 R 9 0 R]".to_vec());
        objects.push(stream("", b"(EI BI) pop % EI comment\n"));
        let parsed = check(&pdf(&objects), &ImportOptions::default(), &AtomicBool::new(false)).unwrap();
        let page = parsed.pages()[0].page_stream().unwrap();
        assert!(page.windows(b"(E\\111 B\\111)".len()).any(|s| s == b"(E\\111 B\\111)"));
        assert!(!page.windows(b"% EI comment".len()).any(|s| s == b"% EI comment"));
    }
    #[test]
    fn inline_objstm_transport_is_not_content_but_compressed_members_work() {
        let page = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 8 8] /Contents 4 0 R >>";
        let mut member = b"6 0 ".to_vec();
        member.extend_from_slice(page);
        let objects = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [6 0 R] /Count 1 >>".to_vec(),
            b"null".to_vec(),
            stream("", b"BI /W 1 /H 1 /BPC 8 /CS /G ID X EI"),
            stream("/Type /ObjStm /N 1 /First 4", &member),
        ];
        assert_eq!(check(&pdf(&objects), &ImportOptions::default(), &AtomicBool::new(false)).unwrap().pages().len(), 1);
        let page = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 8 8] /Contents 5 0 R >>";
        let mut member = b"6 0 ".to_vec();
        member.extend_from_slice(page);
        let mut invalid = objects;
        invalid[4] = stream("/Type /ObjStm /N 1 /First 4", &member);
        assert!(matches!(
            check(&pdf(&invalid), &ImportOptions::default(), &AtomicBool::new(false)),
            Err(ImportError::Unsupported("binary resource reused as page content"))
        ));
    }
    #[test]
    fn inline_semantic_roles_accept_optional_type_and_refuse_unknown_bi() {
        use hayro_syntax::object::Object;
        let draw = b"BI /W 1 /H 1 /BPC 8 /CS /G ID X EI";
        for (resource, object) in [
            (b"/Font << /F 5 0 R >>".as_slice(), b"<< /Subtype /Type3 /CharProcs << /a 6 0 R >> >>".as_slice()),
            (b"/Pattern << /P 6 0 R >>".as_slice(), b"null".as_slice()),
        ] {
            let pattern = resource.starts_with(b"/Pattern");
            let page = format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 8 8] /Resources << {} >> /Contents 4 0 R >>", String::from_utf8_lossy(resource));
            let objects = vec![
                b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
                b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
                page.into_bytes(),
                stream("", b"q Q"),
                object.to_vec(),
                stream(if pattern { "/PatternType 1" } else { "" }, draw),
            ];
            let admitted = check(&pdf(&objects), &ImportOptions::default(), &AtomicBool::new(false)).unwrap();
            assert!(
                admitted
                    .objects()
                    .into_iter()
                    .any(|o| matches!(o,Object::Stream(ref st) if st.raw_data().windows(b"/ASCIIHexDecode".len()).any(|s|s==b"/ASCIIHexDecode")))
            );
        }
        let mut objects = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 8 8] /Contents 4 0 R >>".to_vec(),
            stream("", b"q Q"),
            stream("/Private true", draw),
        ];
        assert!(matches!(
            check(&pdf(&objects), &ImportOptions::default(), &AtomicBool::new(false)),
            Err(ImportError::Unsupported("inline image in unclassified stream"))
        ));
        objects[4] = stream("/Private true", b"(BI EI) pop % BI is not an instruction here\n");
        let admitted = check(&pdf(&objects), &ImportOptions::default(), &AtomicBool::new(false)).unwrap();
        assert!(
            admitted
                .objects()
                .into_iter()
                .any(|o| matches!(o,Object::Stream(ref st) if st.raw_data().as_ref()==b"(BI EI) pop % BI is not an instruction here\n"))
        );
    }
}
