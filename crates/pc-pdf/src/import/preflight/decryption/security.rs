use super::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Cipher {
    Identity,
    Rc4,
    Aes128,
}
pub(super) struct Security {
    pub dictionary: Vec<u8>,
    pub ids: Vec<u8>,
    pub object: Option<Id>,
    pub streams: Cipher,
    pub strings: Cipher,
    pub metadata: bool,
}
fn integer(value: &Value, key: &[u8], default: Option<u64>) -> Result<u64> {
    value
        .get(key)
        .map_or_else(|| default.ok_or_else(|| invalid("missing encryption integer")), |v| v.number().ok_or_else(|| invalid("invalid encryption integer")))
}
fn selected(value: &Value, key: &[u8]) -> Result<Cipher> {
    let name = match value.get(key) {
        None => b"Identity".as_slice(),
        Some(Value::Name(n)) => n.as_slice(),
        _ => return Err(ImportError::Unsupported("indirect crypt filter selection")),
    };
    if name == b"Identity" {
        return Ok(Cipher::Identity);
    }
    let dict = value.get(b"CF").and_then(|v| v.get(name)).ok_or_else(|| invalid("missing selected crypt filter"))?;
    if !matches!(dict, Value::Dict(_)) {
        return Err(ImportError::Unsupported("indirect crypt filter dictionary"));
    }
    if dict.get(b"AuthEvent").is_some_and(|v| v.name() != Some(b"DocOpen")) {
        return Err(ImportError::Unsupported("embedded-file crypt filter"));
    }
    if integer(dict, b"Length", Some(16))? != 16 {
        return Err(ImportError::Unsupported("crypt filter key length"));
    }
    match dict.get(b"CFM").and_then(Value::name) {
        Some(b"None" | b"Identity") => Ok(Cipher::Identity),
        Some(b"V2") => Ok(Cipher::Rc4),
        Some(b"AESV2") => Ok(Cipher::Aes128),
        _ => Err(ImportError::Unsupported("crypt filter algorithm")),
    }
}
fn hex(bytes: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::new();
    for &b in bytes {
        out.push(H[usize::from(b >> 4)] as char);
        out.push(H[usize::from(b & 15)] as char);
    }
    out
}
impl Security {
    pub(super) fn payload_dictionary(&self) -> Vec<u8> {
        // The selected stream CF is used for safe string transports. Object key
        // derivation uses the same object number/generation for both targets.
        String::from_utf8_lossy(&self.dictionary).replace("/StrF /Strings", "/StrF /Streams").into_bytes()
    }
    pub(super) fn parse(data: &[u8], index: &lengths::Index, objects: &BTreeMap<Id, Raw>, budget: &Budget<'_>) -> Result<Self> {
        let trailer = index.trailer();
        let (value, raw, object) = match trailer.get(b"Encrypt") {
            Some(Value::Ref(a, b)) => {
                let id = (*a, *b);
                let object = objects.get(&id).filter(|r| r.stream.is_none()).ok_or_else(|| invalid("encryption dictionary reference"))?;
                (&object.value, &data[object.body.clone()], Some(id))
            }
            Some(v @ Value::Dict(_)) => (v, field_span(index.trailer_bytes(data), b"Encrypt", budget)?.ok_or_else(|| invalid("encryption dictionary"))?, None),
            _ => return Err(invalid("encryption dictionary")),
        };
        if raw.len() > 16_384 {
            return Err(ImportError::Limit("encryption dictionary byte"));
        }
        if value.get(b"Filter").and_then(Value::name) != Some(b"Standard") {
            return Err(ImportError::Unsupported("PDF security handler"));
        }
        let v = integer(value, b"V", None)?;
        let r = integer(value, b"R", None)?;
        let length = integer(value, b"Length", Some(if v == 4 { 128 } else { 40 }))?;
        if !matches!((v, r), (1, 2) | (2, 3) | (4, 4))
            || !(40..=128).contains(&length)
            || !length.is_multiple_of(8)
            || (v == 1 && length != 40)
            || (v == 4 && length != 128)
        {
            return Err(ImportError::Unsupported("PDF Standard encryption revision/key length"));
        }
        let permissions = match value.get(b"P") {
            Some(Value::Number(n)) if n.fract() == 0.0 && *n >= i32::MIN as f64 && *n <= u32::MAX as f64 => *n as i64,
            _ => return Err(invalid("encryption permissions integer")),
        };
        let metadata = match value.get(b"EncryptMetadata") {
            None => true,
            Some(Value::Bool(b)) => *b,
            _ => return Err(invalid("EncryptMetadata must be boolean")),
        };
        if v != 4 && !metadata {
            return Err(ImportError::Unsupported("EncryptMetadata requires revision4"));
        }
        let owner = string_bytes(field_span(raw, b"O", budget)?.ok_or_else(|| invalid("owner authentication string"))?, 32, budget)?;
        let user = string_bytes(field_span(raw, b"U", budget)?.ok_or_else(|| invalid("user authentication string"))?, 32, budget)?;
        if owner.len() != 32 || user.len() != 32 {
            return Err(invalid("encryption authentication string length"));
        }
        let idraw = field_span(index.trailer_bytes(data), b"ID", budget)?.ok_or_else(|| invalid("encrypted PDF ID missing"))?;
        let mut lex = Lexer::new(idraw, budget.options, budget.cancel);
        if !matches!(lex.next()?, Some(Token::OpenArray)) {
            return Err(invalid("encrypted PDF ID array"));
        }
        let mut ids = Vec::new();
        loop {
            match lex.next()?.ok_or_else(|| invalid("encrypted PDF ID array end"))? {
                Token::CloseArray => break,
                Token::String => {
                    let bytes = string_bytes(&idraw[lex.start..lex.at], 64, budget)?;
                    if bytes.is_empty() || ids.len() >= 2 {
                        return Err(invalid("encrypted PDF ID count/length"));
                    }
                    ids.push(bytes);
                }
                _ => return Err(invalid("encrypted PDF ID string")),
            }
        }
        if ids.is_empty() {
            return Err(invalid("encrypted PDF ID missing"));
        }
        if let Some(eff) = value.get(b"EFF")
            && eff.name() != value.get(b"StmF").and_then(Value::name).or(Some(b"Identity"))
        {
            return Err(ImportError::Unsupported("separate embedded-file encryption filter"));
        }
        let streams = if v == 4 { selected(value, b"StmF")? } else { Cipher::Rc4 };
        let strings = if v == 4 { selected(value, b"StrF")? } else { Cipher::Rc4 };
        let mut dictionary = format!(
            "<< /Filter /Standard /V {v} /R {r} /Length {length} /P {permissions} /O <{}> /U <{}> /EncryptMetadata {metadata}",
            hex(&owner),
            hex(&user)
        );
        if v == 4 {
            let tag = |c| match c {
                Cipher::Identity => "None",
                Cipher::Rc4 => "V2",
                Cipher::Aes128 => "AESV2",
            };
            dictionary.push_str(&format!(" /StmF /Streams /StrF /Strings /CF << /Streams << /CFM /{} /Length 16 /AuthEvent /DocOpen >> /Strings << /CFM /{} /Length 16 /AuthEvent /DocOpen >> >>",tag(streams),tag(strings)));
        }
        dictionary.push_str(" >>");
        let ids = format!("[{}]", ids.iter().map(|id| format!("<{}>", hex(id))).collect::<Vec<_>>().join(" "));
        budget.charge(dictionary.len() + ids.len())?;
        Ok(Self { dictionary: dictionary.into_bytes(), ids: ids.into_bytes(), object, streams, strings, metadata })
    }
}
pub(super) fn validate_cipher(cipher: Cipher, encrypted: usize, plain: usize) -> Result<()> {
    match cipher {
        Cipher::Identity | Cipher::Rc4 if encrypted == plain => Ok(()),
        Cipher::Aes128 if encrypted >= 32 && (encrypted - 16).is_multiple_of(16) && plain < encrypted - 16 && (1..=16).contains(&(encrypted - 16 - plain)) => {
            Ok(())
        }
        _ => Err(invalid("encrypted object length or AES padding")),
    }
}
