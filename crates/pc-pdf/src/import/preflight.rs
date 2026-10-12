//! Allocation/work guard before hayro's unbounded decoders run. PDF 1.7 §§3.2–3.5.
//! This is a restrictive admission check, not a replacement PDF interpreter. In particular
//! indirect filters, encryption, CCITT/JPX/JBIG2 and inline images are refused. Every
//! accepted stream is measured with bounded decoding; object-stream dictionaries and the
//! reference graph are checked before Pdf::new can traverse/decompress them.
use super::{ImportError, Result, cancelled};
use photocraft_doc::vector_design::exchange::ImportOptions;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    sync::atomic::AtomicBool,
};

#[path = "preflight/filters.rs"]
mod filters;
#[path = "preflight/lengths.rs"]
mod lengths;
#[path = "preflight/lzw.rs"]
mod lzw;
#[path = "preflight/predictors.rs"]
mod predictors;

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Number(f64),
    Bool(bool),
    Name(Vec<u8>),
    Array(Vec<Value>),
    Dict(BTreeMap<Vec<u8>, Value>),
    Ref(u32, u16),
    Null,
    Other,
}
impl Value {
    fn get(&self, key: &[u8]) -> Option<&Self> {
        if let Self::Dict(d) = self { d.get(key) } else { None }
    }
    fn number(&self) -> Option<u64> {
        if let Self::Number(n) = self
            && *n >= 0.0
            && n.fract() == 0.0
            && *n < u64::MAX as f64
        {
            Some(*n as u64)
        } else {
            None
        }
    }
    fn name(&self) -> Option<&[u8]> {
        if let Self::Name(n) = self { Some(n) } else { None }
    }
    fn refs(&self, out: &mut Vec<(u32, u16)>) {
        match self {
            Self::Ref(a, b) => out.push((*a, *b)),
            Self::Array(a) => {
                for v in a {
                    v.refs(out);
                }
            }
            Self::Dict(d) => {
                for (k, v) in d {
                    if !matches!(k.as_slice(), b"Parent" | b"P" | b"Prev" | b"XObject") {
                        v.refs(out);
                    }
                }
            }
            _ => {}
        }
    }
}
#[derive(Clone, Debug)]
enum Token {
    Word(Vec<u8>),
    Name(Vec<u8>),
    OpenDict,
    CloseDict,
    OpenArray,
    CloseArray,
    String,
}
struct Lexer<'a> {
    data: &'a [u8],
    at: usize,
    start: usize,
    tokens: usize,
    options: &'a ImportOptions,
    cancel: &'a AtomicBool,
}
fn white(b: u8) -> bool {
    matches!(b, 0 | 9 | 10 | 12 | 13 | 32)
}
fn delim(b: u8) -> bool {
    white(b) || b"()<>[]{}/%".contains(&b)
}
fn invalid(s: &str) -> ImportError {
    ImportError::Invalid(s.into())
}
impl<'a> Lexer<'a> {
    fn new(data: &'a [u8], options: &'a ImportOptions, cancel: &'a AtomicBool) -> Self {
        Self { data, at: 0, start: 0, tokens: 0, options, cancel }
    }
    fn next(&mut self) -> Result<Option<Token>> {
        cancelled(self.cancel)?;
        while let Some(&b) = self.data.get(self.at) {
            if white(b) {
                self.at += 1;
            } else if b == b'%' {
                while self.data.get(self.at).is_some_and(|b| !matches!(b, 10 | 13)) {
                    self.at += 1;
                }
            } else {
                break;
            }
        }
        let Some(&b) = self.data.get(self.at) else { return Ok(None) };
        self.tokens = self.tokens.saturating_add(1);
        if self.tokens > self.options.limits.segments.saturating_mul(16).min(16_000_000) {
            return Err(ImportError::Limit("syntax token"));
        }
        self.start = self.at;
        self.at += 1;
        Ok(Some(match b {
            b'<' if self.data.get(self.at) == Some(&b'<') => {
                self.at += 1;
                Token::OpenDict
            }
            b'>' if self.data.get(self.at) == Some(&b'>') => {
                self.at += 1;
                Token::CloseDict
            }
            b'[' => Token::OpenArray,
            b']' => Token::CloseArray,
            b'(' => {
                let mut depth = 1usize;
                while depth > 0 {
                    let c = *self.data.get(self.at).ok_or_else(|| invalid("unterminated string"))?;
                    self.at += 1;
                    match c {
                        b'\\' => {
                            if self.at < self.data.len() {
                                self.at += 1;
                            }
                        }
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    };
                    if depth > self.options.limits.depth.min(64) {
                        return Err(ImportError::Limit("string depth"));
                    }
                }
                Token::String
            }
            b'<' => {
                while self.data.get(self.at).is_some_and(|b| *b != b'>') {
                    self.at += 1;
                }
                if self.at == self.data.len() {
                    return Err(invalid("unterminated hex string"));
                }
                self.at += 1;
                Token::String
            }
            b'/' => {
                let start = self.at;
                while self.data.get(self.at).is_some_and(|b| !delim(*b)) {
                    self.at += 1;
                }
                let raw = &self.data[start..self.at];
                let mut n = Vec::new();
                let mut i = 0;
                while i < raw.len() {
                    if raw[i] == b'#' {
                        let digits = raw.get(i + 1..i + 3).ok_or_else(|| invalid("name escape"))?;
                        let s = std::str::from_utf8(digits).map_err(|_| invalid("name escape"))?;
                        n.push(u8::from_str_radix(s, 16).map_err(|_| invalid("name escape"))?);
                        i += 3;
                    } else {
                        n.push(raw[i]);
                        i += 1;
                    }
                }
                Token::Name(n)
            }
            b')' | b'>' | b'{' | b'}' => return Err(invalid("unexpected delimiter")),
            _ => {
                let start = self.at - 1;
                while self.data.get(self.at).is_some_and(|b| !delim(*b)) {
                    self.at += 1;
                }
                Token::Word(self.data[start..self.at].to_vec())
            }
        }))
    }
    fn value(&mut self, first: Token, depth: usize) -> Result<Value> {
        if depth > self.options.limits.depth.min(64) {
            return Err(ImportError::Limit("dictionary depth"));
        }
        Ok(match first {
            Token::Name(n) => Value::Name(n),
            Token::OpenDict => {
                let mut d = BTreeMap::new();
                loop {
                    match self.next()?.ok_or_else(|| invalid("unterminated dictionary"))? {
                        Token::CloseDict => break,
                        Token::Name(k) => {
                            let t = self.next()?.ok_or_else(|| invalid("missing dictionary value"))?;
                            let v = self.value(t, depth + 1)?;
                            if d.insert(k, v).is_some() {
                                return Err(invalid("duplicate dictionary key"));
                            }
                        }
                        _ => return Err(invalid("dictionary key")),
                    }
                }
                Value::Dict(d)
            }
            Token::OpenArray => {
                let mut a = Vec::new();
                loop {
                    let t = self.next()?.ok_or_else(|| invalid("unterminated array"))?;
                    if matches!(t, Token::CloseArray) {
                        break;
                    }
                    a.push(self.value(t, depth + 1)?);
                }
                Value::Array(a)
            }
            Token::Word(w) if w == b"null" => Value::Null,
            Token::Word(w) if w == b"true" || w == b"false" => Value::Bool(w == b"true"),
            Token::Word(w) => {
                if let Ok(s) = std::str::from_utf8(&w)
                    && let Ok(n) = s.parse::<f64>()
                {
                    if !n.is_finite() {
                        return Err(invalid("nonfinite number"));
                    }
                    let old = (self.at, self.tokens, self.start);
                    let second = self.next()?;
                    let third = if matches!(second, Some(Token::Word(_))) { self.next()? } else { None };
                    if let (Some(Token::Word(g)), Some(Token::Word(r))) = (second, third)
                        && r == b"R"
                    {
                        let a = s.parse::<u32>().map_err(|_| invalid("object reference"))?;
                        let b = std::str::from_utf8(&g).ok().and_then(|s| s.parse::<u16>().ok()).ok_or_else(|| invalid("object generation"))?;
                        Value::Ref(a, b)
                    } else {
                        (self.at, self.tokens, self.start) = old;
                        Value::Number(n)
                    }
                } else {
                    Value::Other
                }
            }
            Token::String => Value::Other,
            _ => return Err(invalid("unexpected container end")),
        })
    }
}
struct Record {
    value: Value,
    work: usize,
    calls: Vec<Vec<u8>>,
}
struct Guard<'a> {
    options: &'a ImportOptions,
    cancel: &'a AtomicBool,
    objects: BTreeMap<(u32, u16), Record>,
    bytes: u64,
    streams: Vec<((u32, u16), Vec<u8>)>,
    binary: BTreeSet<(u32, u16)>,
}
impl Guard<'_> {
    fn charge(&mut self, n: u64) -> Result<()> {
        self.bytes = self.bytes.saturating_add(n);
        if self.bytes > self.options.limits.decompressed_bytes {
            return Err(ImportError::Limit("decompressed byte"));
        }
        cancelled(self.cancel)
    }
    fn record(&mut self, id: (u32, u16), value: Value, work: usize, calls: Vec<Vec<u8>>) -> Result<()> {
        if u64::from(id.0) > self.options.limits.objects as u64 {
            return Err(ImportError::Limit("object number"));
        }
        self.inspect(&value)?;
        if self.objects.insert(id, Record { value, work, calls }).is_some() {
            return Err(invalid("ambiguous duplicate PDF object"));
        }
        if self.objects.len() > self.options.limits.objects {
            return Err(ImportError::Limit("object"));
        }
        Ok(())
    }
    fn inspect(&self, v: &Value) -> Result<()> {
        match v {
            Value::Dict(d) => {
                if d.get(b"S".as_slice()).and_then(Value::name) == Some(b"Transparency") {
                    if matches!(d.get(b"K".as_slice()), Some(Value::Bool(true))) {
                        return Err(ImportError::Unsupported("knockout transparency groups"));
                    }
                    if !matches!(d.get(b"I".as_slice()), Some(Value::Bool(true))) {
                        return Err(ImportError::Unsupported("nonisolated transparency groups"));
                    }
                }
                if d.get(b"Predictor".as_slice()).and_then(Value::number).is_some_and(|n| n > 1) {
                    let columns = d.get(b"Columns".as_slice()).map_or(Some(1), Value::number).ok_or_else(|| invalid("predictor columns"))?;
                    let colors = d.get(b"Colors".as_slice()).map_or(Some(1), Value::number).ok_or_else(|| invalid("predictor colours"))?;
                    let bits = d.get(b"BitsPerComponent".as_slice()).map_or(Some(8), Value::number).ok_or_else(|| invalid("predictor bits"))?;
                    let row = columns.saturating_mul(colors).saturating_mul(bits).div_ceil(8);
                    if row > self.options.limits.decompressed_bytes {
                        return Err(ImportError::Limit("predictor row"));
                    }
                }
                for (k, v) in d {
                    if matches!(
                        k.as_slice(),
                        b"FunctionType"
                            | b"ShadingType"
                            | b"BitsPerSample"
                            | b"BitsPerComponent"
                            | b"BitsPerCoordinate"
                            | b"BitsPerFlag"
                            | b"Columns"
                            | b"Colors"
                            | b"Predictor"
                    ) && matches!(v, Value::Ref(_, _))
                    {
                        return Err(ImportError::Unsupported("indirect decoder declarations"));
                    }
                    if matches!(k.as_slice(), b"Range" | b"Decode" | b"Domain")
                        && let Value::Array(a) = v
                        && a.len() > 128
                    {
                        return Err(ImportError::Limit("function/colour components"));
                    }
                    if k == b"ColorSpace"
                        && let Value::Array(a) = v
                        && a.first().and_then(Value::name) == Some(b"DeviceN")
                        && let Some(Value::Array(names)) = a.get(1)
                        && names.len() > 32
                    {
                        return Err(ImportError::Limit("colour components"));
                    }
                    if k == b"Encrypt" {
                        return Err(ImportError::Encrypted);
                    }
                    if matches!(k.as_slice(), b"BitsPerCoordinate" | b"BitsPerComponent" | b"BitsPerFlag" | b"BitsPerSample")
                        && v.number().is_some_and(|n| n == 0 || n > 32)
                    {
                        return Err(ImportError::Invalid("invalid sample bit width".into()));
                    }
                    if k == b"VerticesPerRow" && v.number().is_some_and(|n| n < 2 || n > self.options.limits.objects as u64) {
                        return Err(ImportError::Limit("mesh row"));
                    }
                    if d.get(b"Type".as_slice()).and_then(Value::name) == Some(b"XRef")
                        && k == b"W"
                        && let Value::Array(a) = v
                        && (a.len() != 3 || a.iter().any(|v| v.number().is_none_or(|n| n > 8)))
                    {
                        return Err(ImportError::Invalid("xref field widths".into()));
                    }
                    if d.get(b"Type".as_slice()).and_then(Value::name) == Some(b"XRef")
                        && k == b"Index"
                        && let Value::Array(a) = v
                        && a.iter().any(|v| v.number().is_none_or(|n| n > self.options.limits.objects as u64))
                    {
                        return Err(ImportError::Limit("xref index"));
                    }
                    if k == b"Size"
                        && let Value::Array(a) = v
                    {
                        let size = a.iter().try_fold(1_u64, |n, v| {
                            let d = v.number().filter(|&v| v > 0).ok_or_else(|| invalid("sampled function dimension"))?;
                            n.checked_mul(d).ok_or(ImportError::Limit("sampled function size"))
                        })?;
                        if size > self.options.limits.pixels {
                            return Err(ImportError::Limit("sampled function size"));
                        }
                    }
                    if k == b"W"
                        && matches!(d.get(b"Subtype".as_slice()).and_then(Value::name), Some(b"CIDFontType0" | b"CIDFontType2"))
                        && let Value::Array(a) = v
                    {
                        let mut at = 0;
                        while at < a.len() {
                            let first = a[at].number().filter(|&n| n <= 65535).ok_or(ImportError::Limit("CID width range"))?;
                            match a.get(at + 1) {
                                Some(Value::Array(widths)) => {
                                    if first + widths.len() as u64 > 65536 {
                                        return Err(ImportError::Limit("CID width range"));
                                    }
                                    at += 2;
                                }
                                Some(last) => {
                                    let last = last.number().filter(|&n| n >= first && n <= 65535).ok_or(ImportError::Limit("CID width range"))?;
                                    let _ = last;
                                    if !matches!(a.get(at + 2), Some(Value::Number(_))) {
                                        return Err(invalid("CID width value"));
                                    }
                                    at += 3;
                                }
                                None => return Err(invalid("CID width value")),
                            }
                        }
                    }
                    if matches!(k.as_slice(), b"Size" | b"Count" | b"N") && v.number().is_some_and(|n| n > self.options.limits.objects as u64) {
                        return Err(ImportError::Limit("declared object"));
                    }
                    if matches!(k.as_slice(), b"Columns" | b"Colors" | b"BitsPerComponent")
                        && v.number().is_some_and(|n| n > self.options.limits.pixels.min(1_000_000))
                    {
                        return Err(ImportError::Limit("predictor dimension"));
                    }
                    // PostScript calculator functions can run unbounded loops in upstream eval.
                    if k == b"FunctionType" && v.number() == Some(4) {
                        return Err(ImportError::Unsupported("PostScript calculator functions"));
                    }
                    self.inspect(v)?;
                }
            }
            Value::Array(a) => {
                for v in a {
                    self.inspect(v)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn stream(&mut self, value: &Value, raw: &[u8]) -> Result<Vec<u8>> {
        if value.get(b"F").is_some() {
            return Err(ImportError::Unsupported("external file streams"));
        }
        let filters = match value.get(b"Filter") {
            None => vec![],
            Some(Value::Name(n)) => vec![n.as_slice()],
            Some(Value::Array(a)) => a.iter().map(|v| v.name().ok_or(ImportError::Unsupported("indirect filters"))).collect::<Result<Vec<_>>>()?,
            _ => return Err(ImportError::Unsupported("indirect filters")),
        };
        if filters.len() > self.options.limits.depth.min(64) {
            return Err(ImportError::Limit("filter chain"));
        }
        let params = predictors::parameters(value, matches!(value.get(b"Filter"), Some(Value::Array(_))), filters.len())?;
        let image = value.get(b"Subtype").and_then(Value::name) == Some(b"Image");
        let dct = filters.iter().any(|f| matches!(*f, b"DCTDecode" | b"DCT"));
        let mut declared_pixels = 0;
        if image {
            let w = value.get(b"Width").and_then(Value::number).ok_or_else(|| invalid("image width"))?;
            let h = value.get(b"Height").and_then(Value::number).ok_or_else(|| invalid("image height"))?;
            let pixels = w.saturating_mul(h);
            if w == 0 || h == 0 || w > 1_000_000 || h > 1_000_000 || pixels > self.options.limits.pixels {
                return Err(ImportError::Limit("image pixel"));
            }
            declared_pixels = pixels;
            if !dct {
                self.charge(pixels.saturating_mul(16))?;
            }
        }
        let mut data = raw.to_vec();
        let mut decoded_charged = false;
        for (f, params) in filters.into_iter().zip(params) {
            data = match f {
                b"ASCIIHexDecode" | b"AHx" | b"ASCII85Decode" | b"A85" | b"RunLengthDecode" | b"RL" => {
                    // The pinned ASCII decoders reserve from encoded length even
                    // when most input is whitespace. Admit that allocation too.
                    let reserve = match f {
                        b"ASCIIHexDecode" | b"AHx" => (data.len() as u64).div_ceil(2),
                        b"ASCII85Decode" | b"A85" => (data.len() as u64).saturating_mul(4) / 5,
                        _ => 0,
                    };
                    let room = self.options.limits.decompressed_bytes.saturating_sub(self.bytes);
                    if reserve > room {
                        return Err(ImportError::Limit("decompressed byte"));
                    }
                    let decoded = filters::decode(f, &data, room, self.cancel)?;
                    self.charge((decoded.len() as u64).max(reserve))?;
                    decoded_charged = true;
                    decoded
                }
                b"LZWDecode" | b"LZW" => {
                    let early = predictors::parameter(params, b"EarlyChange", 1)?;
                    if early > 1 {
                        return Err(invalid("LZW EarlyChange must be 0 or 1"));
                    }
                    let room = self.options.limits.decompressed_bytes.saturating_sub(self.bytes);
                    let (decoded, cost) = lzw::decode(&data, early == 1, room, self.options.limits.segments.saturating_mul(16).min(16_000_000), self.cancel)?;
                    self.charge(cost)?;
                    decoded_charged = true;
                    decoded
                }
                b"FlateDecode" | b"Fl" => {
                    let room = self.options.limits.decompressed_bytes.saturating_sub(self.bytes);
                    let mut decoded = Vec::new();
                    flate2::read::ZlibDecoder::new(data.as_slice())
                        .take(room.saturating_add(1))
                        .read_to_end(&mut decoded)
                        .map_err(|_| invalid("deflate stream"))?;
                    self.charge(decoded.len() as u64)?;
                    decoded_charged = true;
                    decoded
                }
                b"DCTDecode" | b"DCT" if image => {
                    let reader = image::ImageReader::new(std::io::Cursor::new(&data)).with_guessed_format().map_err(|_| invalid("image header"))?;
                    let (w, h) = reader.into_dimensions().map_err(|_| invalid("JPEG header"))?;
                    if u64::from(w) * u64::from(h) > self.options.limits.pixels {
                        return Err(ImportError::Limit("JPEG pixel"));
                    }
                    self.charge(declared_pixels.max(u64::from(w) * u64::from(h)).saturating_mul(16))?;
                    return Ok(vec![]);
                }
                _ => return Err(ImportError::Unsupported("stream codec has no bounded decoder")),
            };
            if matches!(f, b"LZWDecode" | b"LZW" | b"FlateDecode" | b"Fl") {
                let (predicted, cost) = predictors::apply(data, params, self.options.limits.decompressed_bytes.saturating_sub(self.bytes), self.cancel)?;
                self.charge(cost)?;
                data = predicted;
            }
        }
        if !decoded_charged {
            self.charge(data.len() as u64)?;
        }
        if value.get(b"FunctionType").and_then(Value::number) == Some(0) {
            let bits = value.get(b"BitsPerSample").and_then(Value::number).filter(|&n| (1..=32).contains(&n)).ok_or_else(|| invalid("sample bit width"))?;
            let Some(Value::Array(size)) = value.get(b"Size") else {
                return Err(ImportError::Unsupported("indirect sampled function dimensions"));
            };
            if size.is_empty() || size.len() > 4 {
                return Err(ImportError::Limit("sampled function dimensions"));
            }
            self.charge((data.len() as u64 * 8).div_ceil(bits).saturating_mul(1024))?;
        }
        if value.get(b"ShadingType").and_then(Value::number).is_some_and(|n| (4..=7).contains(&n)) {
            let coord = value.get(b"BitsPerCoordinate").and_then(Value::number).ok_or(ImportError::Unsupported("indirect mesh declarations"))?;
            let component = value.get(b"BitsPerComponent").and_then(Value::number).ok_or(ImportError::Unsupported("indirect mesh declarations"))?;
            let flag =
                if value.get(b"ShadingType").and_then(Value::number) == Some(5) { 0 } else { value.get(b"BitsPerFlag").and_then(Value::number).unwrap_or(1) };
            // At least two coordinates and one component are consumed per vertex.
            // Patches consume more; this deliberately overestimates their allocation.
            let bits = coord.saturating_mul(2).saturating_add(component).saturating_add(flag).max(1);
            let vertices = (data.len() as u64 * 8).div_ceil(bits);
            if vertices > self.options.limits.segments as u64 / 3 {
                return Err(ImportError::Limit("mesh vertex expansion"));
            }
            self.charge(vertices.saturating_mul(4096))?;
        }
        Ok(data)
    }
    fn content(&self, data: &[u8]) -> Result<(usize, Vec<Vec<u8>>)> {
        let mut l = Lexer::new(data, self.options, self.cancel);
        let mut depth = 0usize;
        let mut saves = 0usize;
        let mut last_name = None;
        let mut calls = vec![];
        while let Some(t) = l.next()? {
            match t {
                Token::Name(n) => last_name = Some(n),
                Token::Word(w) if w == b"Do" => {
                    if let Some(n) = last_name.take() {
                        calls.push(n);
                    }
                }
                Token::OpenArray | Token::OpenDict => {
                    depth += 1;
                    if depth > self.options.limits.depth.min(64) {
                        return Err(ImportError::Limit("content depth"));
                    }
                }
                Token::CloseArray | Token::CloseDict => depth = depth.saturating_sub(1),
                Token::Word(w) if w == b"BI" => return Err(ImportError::Unsupported("inline images require a bounded decoder")),
                Token::Word(w) if w == b"q" => {
                    saves += 1;
                    if saves > self.options.limits.depth.min(64) {
                        return Err(ImportError::Limit("graphics state depth"));
                    }
                }
                Token::Word(w) if w == b"Q" => saves = saves.saturating_sub(1),
                _ => {}
            }
        }
        Ok((l.tokens, calls))
    }
    fn graph(&self) -> Result<()> {
        // Forms may inherit their caller's Resources. Conservatively resolve Do names against
        // every declared XObject dictionary too; this catches cycles lacking local Resources.
        fn xobjects(v: &Value, objects: &BTreeMap<(u32, u16), Record>, out: &mut BTreeMap<Vec<u8>, Vec<(u32, u16)>>) {
            match v {
                Value::Dict(d) => {
                    for (k, v) in d {
                        if k == b"XObject" {
                            let v = if let Value::Ref(a, b) = v { objects.get(&(*a, *b)).map(|r| &r.value).unwrap_or(v) } else { v };
                            if let Value::Dict(xs) = v {
                                for (name, value) in xs {
                                    if let Value::Ref(a, b) = value {
                                        out.entry(name.clone()).or_default().push((*a, *b));
                                    }
                                }
                            }
                        }
                        xobjects(v, objects, out);
                    }
                }
                Value::Array(a) => {
                    for v in a {
                        xobjects(v, objects, out);
                    }
                }
                _ => {}
            }
        }
        let mut named = BTreeMap::new();
        for r in self.objects.values() {
            xobjects(&r.value, &self.objects, &mut named);
        }

        fn visit(
            id: (u32, u16),
            g: &Guard<'_>,
            active: &mut BTreeSet<(u32, u16)>,
            done: &mut BTreeMap<(u32, u16), (usize, usize)>,
            named: &BTreeMap<Vec<u8>, Vec<(u32, u16)>>,
            depth: usize,
        ) -> Result<(usize, usize)> {
            if depth > g.options.limits.depth.min(64) {
                return Err(ImportError::Limit("reference depth"));
            }
            if active.contains(&id) {
                return Err(ImportError::Invalid("recursive PDF resource graph".into()));
            }
            if let Some(&w) = done.get(&id) {
                return Ok(w);
            }
            let r = g.objects.get(&id).ok_or_else(|| invalid("unresolved PDF resource reference"))?;
            active.insert(id);
            let mut refs = vec![];
            r.value.refs(&mut refs);
            for call in &r.calls {
                if let Some(ids) = named.get(call) {
                    refs.extend(ids.iter().copied());
                }
            }
            let mut work = r.work.max(1);
            let mut nodes = 1usize;
            for child in refs {
                let (w, n) = visit(child, g, active, done, named, depth + 1)?;
                nodes = nodes.saturating_add(n);
                if nodes > g.options.limits.objects {
                    return Err(ImportError::Limit("expanded resource object"));
                }
                work = work.saturating_add(w.saturating_mul(r.work.max(1)));
                if work > g.options.limits.segments.saturating_mul(16) {
                    return Err(ImportError::Limit("expanded interpreter work"));
                }
            }
            active.remove(&id);
            done.insert(id, (work, nodes));
            Ok((work, nodes))
        }
        let mut done = BTreeMap::new();
        for &id in self.objects.keys() {
            cancelled(self.cancel)?;
            visit(id, self, &mut BTreeSet::new(), &mut done, &named, 0)?;
        }
        Ok(())
    }
}
pub(super) fn check(bytes: &[u8], options: &ImportOptions, cancel: &AtomicBool) -> Result<hayro_syntax::Pdf> {
    cancelled(cancel)?;
    if bytes.len() > options.limits.bytes {
        return Err(ImportError::Limit("input byte"));
    }
    let header = bytes[..bytes.len().min(1029)].windows(5).position(|s| s == b"%PDF-").ok_or_else(|| invalid("missing PDF header within 1024 bytes"))?;
    if !bytes.get(header + 5..header + 8).is_some_and(|v| v[0].is_ascii_digit() && v[1] == b'.' && v[2].is_ascii_digit()) {
        return Err(invalid("invalid PDF header version"));
    }
    if !options.dpi.is_finite() || !(1.0..=2400.0).contains(&options.dpi) {
        return Err(invalid("DPI must be finite and between 1 and 2400"));
    }
    let mut g = Guard { options, cancel, objects: BTreeMap::new(), bytes: 0, streams: vec![], binary: BTreeSet::new() };
    let mut l = Lexer::new(bytes, options, cancel);
    l.at = header;
    let mut words: Vec<(Vec<u8>, usize)> = Vec::new();
    let mut object_starts = BTreeMap::new();
    let mut xref_starts = BTreeSet::new();
    let mut length_index: Option<lengths::Index> = None;
    let mut bodies = BTreeMap::new();
    let mut root = None;
    fn set_root(root: &mut Option<(u32, u16)>, value: &Value) -> Result<()> {
        if let Some(Value::Ref(a, b)) = value.get(b"Root") {
            if root.is_some_and(|old| old != (*a, *b)) {
                return Err(invalid("ambiguous PDF root"));
            }
            *root = Some((*a, *b));
        }
        Ok(())
    }
    while let Some(t) = l.next()? {
        let token_start = l.start;
        match t {
            Token::Word(w) if w == b"obj" => {
                let id = words
                    .last()
                    .and_then(|(s, _)| std::str::from_utf8(s).ok())
                    .and_then(|s| s.parse::<u16>().ok())
                    .zip(words.iter().rev().nth(1).and_then(|(s, _)| std::str::from_utf8(s).ok()).and_then(|s| s.parse::<u32>().ok()))
                    .map(|(b, a)| (a, b))
                    .ok_or_else(|| invalid("indirect object header"))?;
                let object_start = words.iter().rev().nth(1).map(|(_, start)| *start).ok_or_else(|| invalid("object header start"))?;
                object_starts.insert(id, object_start);
                let body_start = l.at;
                let t = l.next()?.ok_or_else(|| invalid("object value"))?;
                let value = l.value(t, 0)?;
                g.inspect(&value)?;
                if value.get(b"Type").and_then(Value::name) == Some(b"XRef") {
                    set_root(&mut root, &value)?;
                }
                let value_end = l.at;
                let next = l.next()?;
                let work = 0;
                let calls = vec![];
                if matches!(&next,Some(Token::Word(w)) if w==b"stream") {
                    if l.data.get(l.at) == Some(&b'\r') {
                        l.at += 1;
                    }
                    if l.data.get(l.at) == Some(&b'\n') {
                        l.at += 1;
                    } else if l.data.get(l.at.wrapping_sub(1)) != Some(&b'\r') {
                        return Err(invalid("stream newline"));
                    }
                    let n = match value.get(b"Length") {
                        Some(Value::Ref(a, b)) => {
                            let id = (*a, *b);
                            if let Some(record) = g.objects.get(&id) {
                                record.value.number().ok_or_else(|| invalid("stream length must be an integer object"))?
                            } else {
                                if length_index.is_none() {
                                    length_index = Some(lengths::Index::read(bytes, header, options, cancel)?);
                                }
                                length_index.as_mut().ok_or_else(|| invalid("missing length index"))?.resolve(id, bytes, options, cancel)?
                            }
                        }
                        Some(v) => v.number().ok_or_else(|| invalid("stream length must be a nonnegative integer"))?,
                        None => return Err(invalid("missing stream length")),
                    };
                    let end =
                        l.at.checked_add(usize::try_from(n).map_err(|_| ImportError::Limit("stream length"))?)
                            .filter(|&n| n <= bytes.len())
                            .ok_or_else(|| invalid("stream length"))?;
                    let data = g.stream(&value, &bytes[l.at..end])?;
                    l.at = end;
                    if !matches!(l.next()?,Some(Token::Word(w)) if w==b"endstream") {
                        return Err(invalid("stream length mismatch"));
                    }
                    if value.get(b"Type").and_then(Value::name) == Some(b"ObjStm") {
                        let n = value.get(b"N").and_then(Value::number).ok_or_else(|| invalid("object stream count"))? as usize;
                        let first = value.get(b"First").and_then(Value::number).ok_or_else(|| invalid("object stream offset"))? as usize;
                        let header = data.get(..first).ok_or_else(|| invalid("object stream offset"))?;
                        let mut h = Lexer::new(header, options, cancel);
                        let mut ids = Vec::new();
                        for _ in 0..n {
                            let Some(Token::Word(a)) = h.next()? else { return Err(invalid("object stream header")) };
                            let Some(Token::Word(b)) = h.next()? else { return Err(invalid("object stream offset")) };
                            let parse =
                                |s: &[u8]| std::str::from_utf8(s).ok().and_then(|s| s.parse::<usize>().ok()).ok_or_else(|| invalid("object stream number"));
                            ids.push((parse(&a)?, parse(&b)?));
                        }
                        for (id, offset) in ids {
                            let at = first.checked_add(offset).filter(|&a| a < data.len()).ok_or_else(|| invalid("object stream member offset"))?;
                            let mut v = Lexer::new(&data[at..], options, cancel);
                            let t = v.next()?.ok_or_else(|| invalid("object stream member"))?;
                            let value = v.value(t, 0)?;
                            bodies.insert((u32::try_from(id).map_err(|_| ImportError::Limit("object number"))?, 0), data[at..at + v.at].to_vec());
                            g.record((u32::try_from(id).map_err(|_| ImportError::Limit("object number"))?, 0), value, 0, vec![])?;
                        }
                    } else if value.get(b"Subtype").and_then(Value::name) == Some(b"Form")
                        || (value.get(b"Subtype").and_then(Value::name) != Some(b"Image")
                            && value.get(b"Type").and_then(Value::name) != Some(b"XRef")
                            && value.get(b"Length1").is_none()
                            && !matches!(value.get(b"Subtype").and_then(Value::name), Some(b"OpenType" | b"Type1C" | b"CIDFontType0C"))
                            && value.get(b"ShadingType").is_none()
                            && value.get(b"FunctionType").is_none()
                            && value.get(b"N").is_none())
                    {
                        g.streams.push((id, data));
                    } else {
                        g.binary.insert(id);
                    }
                    let body_end = l.at;
                    bodies.insert(id, bytes[body_start..body_end].to_vec());
                    if !matches!(l.next()?,Some(Token::Word(w)) if w==b"endobj") {
                        return Err(invalid("object terminator"));
                    }
                } else if !matches!(next,Some(Token::Word(w)) if w==b"endobj") {
                    return Err(invalid("object terminator"));
                }
                bodies.entry(id).or_insert_with(|| bytes[body_start..value_end].to_vec());
                g.record(id, value, work, calls)?;
                words.clear();
            }
            Token::Word(w) => {
                if w == b"xref" {
                    xref_starts.insert(token_start);
                }
                words.push((w, token_start));
                if words.len() > 2 {
                    words.remove(0);
                }
            }
            Token::OpenDict => {
                let v = l.value(Token::OpenDict, 0)?;
                g.inspect(&v)?;
                set_root(&mut root, &v)?;
                words.clear();
            }
            _ => words.clear(),
        }
    }
    if let Some(index) = length_index {
        index.verify(&g.objects, &object_starts, &xref_starts)?;
    }
    // FontFile2 streams need not carry a Subtype or Length1. Identify them from their
    // descriptors after all objects have been read, before lexing candidate content.
    fn font_streams(v: &Value, out: &mut BTreeSet<(u32, u16)>) {
        match v {
            Value::Dict(d) => {
                for (k, v) in d {
                    if matches!(k.as_slice(), b"FontFile" | b"FontFile2" | b"FontFile3") {
                        let mut ids = vec![];
                        v.refs(&mut ids);
                        out.extend(ids);
                    }
                    font_streams(v, out);
                }
            }
            Value::Array(a) => {
                for v in a {
                    font_streams(v, out);
                }
            }
            _ => {}
        }
    }
    // Reusing a binary resource as a content stream would bypass content work checks.
    fn content_refs(v: &Value, out: &mut Vec<(u32, u16)>) {
        match v {
            Value::Dict(d) => {
                for (k, v) in d {
                    if matches!(k.as_slice(), b"Contents" | b"CharProcs") {
                        v.refs(out);
                    }
                    content_refs(v, out);
                }
            }
            Value::Array(a) => {
                for v in a {
                    content_refs(v, out);
                }
            }
            _ => {}
        }
    }
    let mut contents = vec![];
    for r in g.objects.values() {
        content_refs(&r.value, &mut contents);
    }
    let mut content_ids = BTreeSet::new();
    while let Some(id) = contents.pop() {
        if content_ids.insert(id)
            && let Some(r) = g.objects.get(&id)
        {
            // A CharProcs dictionary may itself be indirect.
            if let Value::Dict(d) = &r.value
                && !d.contains_key(b"Length".as_slice())
            {
                r.value.refs(&mut contents);
            }
        }
    }
    let mut fonts = BTreeSet::new();
    for r in g.objects.values() {
        font_streams(&r.value, &mut fonts);
    }
    if content_ids.iter().any(|id| fonts.contains(id) || g.binary.contains(id)) {
        return Err(ImportError::Unsupported("binary resource reused as page content"));
    }
    for (id, data) in std::mem::take(&mut g.streams) {
        if !fonts.contains(&id) {
            let (work, calls) = g.content(&data)?;
            if let Some(r) = g.objects.get_mut(&id) {
                r.work = work;
                r.calls = calls;
            }
        }
    }
    g.graph()?;
    // hayro builds its page tree during Pdf::new, before we can inspect its xref.
    // Supply a canonical table pointing only at the exact bodies admitted above.
    // This also removes repair-parser discrepancies and hidden-object offsets.
    let root = root.filter(|id| g.objects.contains_key(id)).ok_or_else(|| invalid("PDF root was not admitted by preflight"))?;
    let mut admitted = b"%PDF-1.7\n".to_vec();
    let mut offsets = BTreeMap::new();
    for (id, body) in &bodies {
        cancelled(cancel)?;
        offsets.insert(*id, admitted.len());
        admitted.extend_from_slice(format!("{} {} obj\n", id.0, id.1).as_bytes());
        admitted.extend_from_slice(body);
        admitted.extend_from_slice(b"\nendobj\n");
    }
    let xref = admitted.len();
    admitted.extend_from_slice(b"xref\n0 1\n0000000000 65535 f \n");
    for (id, offset) in offsets {
        admitted.extend_from_slice(format!("{} 1\n{offset:010} {:05} n \n", id.0, id.1).as_bytes());
    }
    let size = bodies.keys().map(|id| id.0).max().unwrap_or(0).saturating_add(1);
    admitted.extend_from_slice(format!("trailer\n<< /Size {size} /Root {} {} R >>\nstartxref\n{xref}\n%%EOF\n", root.0, root.1).as_bytes());
    let pdf = hayro_syntax::Pdf::new(admitted).map_err(|e| invalid(&format!("{e:?}")))?;
    if pdf.len() > options.limits.objects {
        return Err(ImportError::Limit("xref object"));
    }
    let root = pdf.xref().root_id();
    if !g.objects.contains_key(&(root.obj_number as u32, root.gen_number as u16)) {
        return Err(invalid("xref root was not admitted by preflight"));
    }
    for object in pdf.objects() {
        cancelled(cancel)?;
        let dict = match object {
            hayro_syntax::object::Object::Dict(d) => Some(d),
            hayro_syntax::object::Object::Stream(s) => Some(s.dict().clone()),
            _ => None,
        };
        if let Some(dict) = dict {
            let mut lexer = Lexer::new(dict.data(), options, cancel);
            let token = lexer.next()?.ok_or_else(|| invalid("empty xref dictionary"))?;
            let value = lexer.value(token, 0)?;
            g.inspect(&value)?;
            let id = dict.obj_id().ok_or_else(|| invalid("xref dictionary without identity"))?;
            let record = g.objects.get(&(id.obj_number as u32, id.gen_number as u16)).ok_or_else(|| invalid("xref object was not admitted by preflight"))?;
            if value != record.value {
                return Err(invalid("xref/preflight object mismatch"));
            }
        }
    }
    Ok(pdf)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod dsfix_tests {
    use super::*;
    fn guard<'a>(options: &'a ImportOptions, cancel: &'a AtomicBool) -> Guard<'a> {
        Guard { options, cancel, objects: BTreeMap::new(), bytes: 0, streams: vec![], binary: BTreeSet::new() }
    }
    #[test]
    fn dsfix_flate_decoded_bytes_are_charged_once_and_bombs_stay_bounded() {
        use std::io::Write;
        let mut options = ImportOptions::default();
        options.limits.decompressed_bytes = 32;
        let cancel = AtomicBool::new(false);
        let dict = Value::Dict(BTreeMap::from([(b"Filter".to_vec(), Value::Name(b"FlateDecode".to_vec()))]));
        let compressed = |n| {
            let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(&vec![42; n]).unwrap();
            encoder.finish().unwrap()
        };
        let mut g = guard(&options, &cancel);
        assert_eq!(g.stream(&dict, &compressed(32)).unwrap().len(), 32);
        assert_eq!(g.bytes, 32);
        assert!(guard(&options, &cancel).stream(&dict, &compressed(33)).is_err());
    }
    #[test]
    fn dsfix_jpeg_pixel_allocation_charged_once_with_header_validation() {
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut bytes).encode(&[0; 12], 2, 2, image::ExtendedColorType::Rgb8).unwrap();
        let dict = |w| {
            Value::Dict(BTreeMap::from([
                (b"Filter".to_vec(), Value::Name(b"DCTDecode".to_vec())),
                (b"Subtype".to_vec(), Value::Name(b"Image".to_vec())),
                (b"Width".to_vec(), Value::Number(w)),
                (b"Height".to_vec(), Value::Number(2.)),
            ]))
        };
        let mut options = ImportOptions::default();
        options.limits.decompressed_bytes = 64;
        let cancel = AtomicBool::new(false);
        let mut g = guard(&options, &cancel);
        assert!(g.stream(&dict(2.), &bytes).unwrap().is_empty());
        assert_eq!(g.bytes, 64);
        assert!(guard(&options, &cancel).stream(&dict(3.), &bytes).is_err());
    }
}
