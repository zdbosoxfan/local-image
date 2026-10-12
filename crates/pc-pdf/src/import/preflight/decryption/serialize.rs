use super::*;
use security::{Cipher, Security};
pub(super) fn hex(out: &mut Vec<u8>, bytes: &[u8], budget: &Budget<'_>) -> Result<()> {
    const H: &[u8; 16] = b"0123456789ABCDEF";
    budget.append(out, b"<")?;
    for chunk in bytes.chunks(4096) {
        budget.step()?;
        let mut block = [0u8; 8192];
        for (i, &b) in chunk.iter().enumerate() {
            block[2 * i] = H[usize::from(b >> 4)];
            block[2 * i + 1] = H[usize::from(b & 15)];
        }
        budget.append(out, &block[..chunk.len() * 2])?;
    }
    budget.append(out, b">")
}

// Only PDF numeric syntax is retained. In particular Rust f64 exponent syntax
// and overflowing pinned-parser mantissas must never reach the final parser.
fn number(word: &[u8]) -> Result<()> {
    let digits = word.strip_prefix(b"+").or_else(|| word.strip_prefix(b"-")).unwrap_or(word);
    let mut dot = false;
    let mut count = 0;
    let mut mantissa = 0u64;
    for &b in digits {
        match b {
            b'.' if !dot => dot = true,
            b'0'..=b'9' => {
                count += 1;
                mantissa = mantissa.checked_mul(10).and_then(|v| v.checked_add(u64::from(b - b'0'))).ok_or_else(|| invalid("PDF numeric mantissa overflow"))?;
            }
            _ => return Err(invalid("invalid PDF numeric syntax")),
        }
    }
    if count == 0 || (!dot && std::str::from_utf8(word).ok().and_then(|s| s.parse::<i64>().ok()).is_none()) {
        return Err(invalid("invalid PDF numeric range"));
    }
    Ok(())
}
fn cipher_shape(cipher: Cipher, len: usize) -> Result<()> {
    if cipher == Cipher::Aes128 && (len < 32 || !(len - 16).is_multiple_of(16)) {
        return Err(invalid("AES ciphertext block length"));
    }
    Ok(())
}
fn walk(
    lex: &mut Lexer<'_>,
    token: Token,
    out: &mut Vec<u8>,
    budget: &Budget<'_>,
    strings: &mut impl FnMut(Vec<u8>) -> Result<Vec<u8>>,
    depth: usize,
    stream_length: Option<usize>,
) -> Result<()> {
    budget.step()?;
    if depth > budget.options.limits.depth.min(64) {
        return Err(ImportError::Limit("decrypted object depth"));
    }
    let start = lex.start;
    match token {
        Token::String => {
            let cipher = string_bytes(&lex.data[start..lex.at], budget.options.limits.bytes, budget)?;
            hex(out, &strings(cipher)?, budget)?;
        }
        Token::OpenDict => {
            budget.append(out, b"<< ")?;
            loop {
                match lex.next()?.ok_or_else(|| invalid("decrypted dictionary end"))? {
                    Token::CloseDict => break,
                    Token::Name(key) => {
                        budget.append(out, &lex.data[lex.start..lex.at])?;
                        budget.append(out, b" ")?;
                        let token = lex.next()?.ok_or_else(|| invalid("decrypted dictionary value"))?;
                        if key == b"Length"
                            && let Some(length) = stream_length
                        {
                            // Validate source syntax, but emit an authoritative integer
                            // even if the original direct length was an integral real.
                            let mut discarded = Vec::new();
                            walk(lex, token, &mut discarded, budget, strings, depth + 1, None)?;
                            budget.append(out, length.to_string().as_bytes())?;
                        } else {
                            walk(lex, token, out, budget, strings, depth + 1, None)?;
                        }
                        budget.append(out, b" ")?;
                    }
                    _ => return Err(invalid("decrypted dictionary key")),
                }
            }
            budget.append(out, b">>")?;
        }
        Token::OpenArray => {
            budget.append(out, b"[ ")?;
            loop {
                let token = lex.next()?.ok_or_else(|| invalid("decrypted array end"))?;
                if matches!(token, Token::CloseArray) {
                    break;
                }
                walk(lex, token, out, budget, strings, depth + 1, None)?;
                budget.append(out, b" ")?;
            }
            budget.append(out, b"]")?;
        }
        Token::Name(_) => budget.append(out, &lex.data[start..lex.at])?,
        Token::Word(word) => {
            if !matches!(word.as_slice(), b"true" | b"false" | b"null") {
                number(&word)?;
                let old = (lex.at, lex.start, lex.tokens);
                let second = lex.next()?;
                let third = if matches!(second, Some(Token::Word(_))) { lex.next()? } else { None };
                if let (Some(Token::Word(generation)), Some(Token::Word(r))) = (second, third)
                    && r == b"R"
                {
                    let obj = std::str::from_utf8(&word).ok().and_then(|s| s.parse::<i32>().ok()).filter(|v| *v > 0);
                    let generation = std::str::from_utf8(&generation).ok().and_then(|s| s.parse::<u16>().ok());
                    if obj.is_none() || generation.is_none() {
                        return Err(invalid("invalid PDF object reference"));
                    }
                } else {
                    (lex.at, lex.start, lex.tokens) = old;
                }
            }
            budget.append(out, &lex.data[start..lex.at])?;
        }
        _ => return Err(invalid("invalid encrypted object syntax")),
    }
    Ok(())
}
fn syntax(data: &[u8], raw: &Raw, budget: &Budget<'_>, strings: &mut impl FnMut(Vec<u8>) -> Result<Vec<u8>>, stream_length: Option<usize>) -> Result<Vec<u8>> {
    let bytes = &data[raw.body.start..raw.dict_end];
    let mut lex = Lexer::metered(bytes, budget.options, budget.cancel, &budget.bytes);
    let token = lex.next()?.ok_or_else(|| invalid("encrypted object value"))?;
    let mut out = Vec::new();
    walk(&mut lex, token, &mut out, budget, strings, 0, stream_length)?;
    if lex.next()?.is_some() {
        return Err(invalid("trailing encrypted object syntax"));
    }
    Ok(out)
}
pub(super) fn prepare(data: &[u8], raw: &Raw, cipher: Cipher, budget: &Budget<'_>) -> Result<Vec<Vec<u8>>> {
    let mut strings = Vec::new();
    syntax(
        data,
        raw,
        budget,
        &mut |bytes| {
            cipher_shape(cipher, bytes.len())?;
            budget.charge(bytes.len().saturating_add(24))?;
            strings.push(bytes.clone());
            Ok(bytes)
        },
        None,
    )?;
    Ok(strings)
}
pub(super) fn body(data: &[u8], raw: &Raw, strings: &[Vec<u8>], payload: Option<&[u8]>, budget: &Budget<'_>) -> Result<Vec<u8>> {
    let mut values = strings.iter();
    let mut out = syntax(
        data,
        raw,
        budget,
        &mut |_| {
            let value = values.next().ok_or_else(|| invalid("decrypted string count"))?;
            budget.charge(value.len())?;
            Ok(value.clone())
        },
        payload.map(<[u8]>::len),
    )?;
    if values.next().is_some() {
        return Err(invalid("decrypted string count"));
    }
    if let Some(bytes) = payload {
        budget.append(&mut out, b"\nstream\n")?;
        budget.append(&mut out, bytes)?;
        budget.append(&mut out, b"\nendstream")?;
    }
    Ok(out)
}
pub(super) fn stream_shape(data: &[u8], raw: &Raw, security: &Security) -> Result<()> {
    if let Some(span) = &raw.stream {
        if raw.value.get(b"Type").and_then(Value::name) == Some(b"XRef") {
            return Err(ImportError::Unsupported("encrypted xref streams"));
        }
        let mut filters = Vec::new();
        if let Some(v) = raw.value.get(b"Filter") {
            match v {
                Value::Name(n) => filters.push(n.as_slice()),
                Value::Array(a) => filters.extend(a.iter().filter_map(Value::name)),
                _ => {}
            }
        }
        if filters.contains(&b"Crypt".as_slice()) {
            return Err(ImportError::Unsupported("per-stream Crypt filters"));
        }
        if security.metadata || raw.value.get(b"Type").and_then(Value::name) != Some(b"Metadata") {
            cipher_shape(security.streams, data[span.clone()].len())?;
        }
    }
    Ok(())
}
