//! Local Image generation provenance, independent of standard PSD pixels and byte order.
//! This data is not authentication and never executes commands during import. Like native
//! Layer::generation it survives pixel edits; the current field replaces the owned block.
use photocraft_doc::{Document, Layer};
use photocraft_psd::{LayerRecord, TaggedBlock};
use serde_json::Value;
use std::io::{self, Write};

pub(crate) const KEY: [u8; 4] = *b"liGM";
const MAGIC: &[u8; 8] = b"LIGEN001";
const MAX_BYTES: usize = 256 * 1024;
// Serialized bytes plus conservative owned Value/map-entry overhead. Separate from pixels.
pub(crate) const DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_VALUES: usize = 16 * 1024;
const MAX_DEPTH: usize = 32;

fn invalid() -> String {
    "invalid or over-budget Local Image generation metadata; standard layer pixels retained".into()
}

fn inspect(value: &Value, depth: usize, count: &mut usize) -> Result<(), String> {
    *count += 1;
    if depth > MAX_DEPTH || *count > MAX_VALUES {
        return Err(invalid());
    }
    match value {
        Value::Array(values) => {
            for value in values {
                inspect(value, depth + 1, count)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                if key.len() > MAX_BYTES {
                    return Err(invalid());
                }
                inspect(value, depth + 1, count)?;
            }
        }
        Value::String(value) if value.len() > MAX_BYTES => return Err(invalid()),
        _ => {}
    }
    Ok(())
}

struct BoundedJson {
    bytes: Vec<u8>,
}
impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_BYTES.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("generation metadata exceeds 256 KiB"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn encode(value: &Value) -> Result<(Vec<u8>, usize), String> {
    if !value.is_object() {
        return Err(invalid());
    }
    let mut values = 0;
    inspect(value, 0, &mut values)?;
    let mut writer = BoundedJson { bytes: Vec::new() };
    serde_json::to_writer(&mut writer, value).map_err(|_| invalid())?;
    let cost = writer.bytes.len().saturating_add(values.saturating_mul(128));
    Ok((writer.bytes, cost))
}

/// Check before raster projection or returning replacement bytes. The adapter's older
/// infallible model-only API still returns an explicit warning on inadmissible fields.
pub(crate) fn admit(doc: &Document) -> Result<(), crate::IoError> {
    let mut remaining = DOCUMENT_BYTES;
    for (_, _, layer) in doc.walk() {
        if let Some(value) = &layer.generation {
            let (_, cost) = encode(value).map_err(|_| {
                crate::IoError::Unsupported(
                    "Generation settings exceed the layered-format metadata limits. Reduce them or save a native document instead.".into(),
                )
            })?;
            remaining = remaining.checked_sub(cost).ok_or_else(|| {
                crate::IoError::Unsupported(
                    "Generation settings exceed the 8 MiB document metadata limit. Reduce them or save a native document instead.".into(),
                )
            })?;
        }
    }
    Ok(())
}

pub(crate) fn attach(layer: &Layer, record: &mut LayerRecord, remaining: &mut usize) -> Result<(), String> {
    record.blocks.retain(|block| block.key != KEY);
    let Some(value) = &layer.generation else { return Ok(()) };
    let (bytes, cost) = encode(value)?;
    let after = remaining.checked_sub(cost).ok_or_else(invalid)?;
    let mut payload = Vec::with_capacity(bytes.len() + 13);
    payload.extend_from_slice(MAGIC);
    payload.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    payload.extend_from_slice(&bytes);
    if !payload.len().is_multiple_of(2) {
        payload.push(0);
    }
    record.blocks.push(TaggedBlock::new(KEY, payload));
    *remaining = after;
    Ok(())
}

pub(crate) fn restore(record: &LayerRecord, remaining: &mut usize) -> Result<Option<Value>, String> {
    let mut blocks = record.blocks.iter().filter(|block| block.key == KEY);
    let Some(block) = blocks.next() else { return Ok(None) };
    if blocks.next().is_some() || block.data.len() < 12 || block.data.len() > MAX_BYTES + 13 || !block.data.starts_with(MAGIC) {
        return Err(invalid());
    }
    let len = u32::from_be_bytes([block.data[8], block.data[9], block.data[10], block.data[11]]) as usize;
    if len > MAX_BYTES || len > *remaining {
        return Err(invalid());
    }
    let end = 12usize.checked_add(len).ok_or_else(invalid)?;
    let bytes = block.data.get(12..end).ok_or_else(invalid)?;
    if block.data.len() - end > 1 || block.data[end..].iter().any(|byte| *byte != 0) {
        return Err(invalid());
    }
    // Wire bytes are bounded before parsing; serde_json's recursion limit remains
    // enabled. Parsed values have a separate cumulative budget, never pixel memory.
    let value: Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if !value.is_object() {
        return Err(invalid());
    }
    let mut values = 0;
    inspect(&value, 0, &mut values)?;
    let cost = len.saturating_add(values.saturating_mul(128));
    *remaining = remaining.checked_sub(cost).ok_or_else(invalid)?;
    Ok(Some(value))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use photocraft_doc::{LayerContent, PixelFormat, Surface};
    use serde_json::json;

    fn layer(value: Value) -> Layer {
        let mut layer = Layer::new("generated", LayerContent::Raster(Surface::new(PixelFormat::RGBA8)));
        layer.generation = Some(value);
        layer
    }
    fn record(bytes: &[u8]) -> LayerRecord {
        let mut data = MAGIC.to_vec();
        data.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        data.extend_from_slice(bytes);
        LayerRecord { blocks: vec![TaggedBlock::new(KEY, data)], ..Default::default() }
    }
    #[test]
    fn generation_wire_limits_and_full_u64_are_exact() {
        let value = json!({"command":"ai.generate", "seed":u64::MAX, "extension":{"array":[true, null, -5, 0.125]}});
        let mut record = LayerRecord::default();
        let mut left = DOCUMENT_BYTES;
        attach(&layer(value.clone()), &mut record, &mut left).unwrap();
        let mut import_budget = DOCUMENT_BYTES;
        assert_eq!(restore(&record, &mut import_budget).unwrap(), Some(value));
        let base = serde_json::to_vec(&json!({"prompt":""})).unwrap().len();
        let exact = json!({"prompt":"x".repeat(MAX_BYTES - base)});
        assert_eq!(encode(&exact).unwrap().0.len(), MAX_BYTES);
        assert!(encode(&json!({"prompt":"x".repeat(MAX_BYTES - base + 1)})).is_err());
        assert!(encode(&json!({"escaped":"\n".repeat(MAX_BYTES / 2)})).is_err(), "wire escaping also counts");
    }
    #[test]
    fn generation_malformed_records_never_consume_budget() {
        let valid = record(br#"{"prompt":"kept"}"#);
        let mut cases = vec![];
        let mut bad = valid.clone();
        bad.blocks[0].data[7] = b'2';
        cases.push(bad);
        let mut bad = valid.clone();
        bad.blocks[0].data.truncate(11);
        cases.push(bad);
        let mut bad = valid.clone();
        bad.blocks[0].data[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        cases.push(bad);
        let mut bad = valid.clone();
        bad.blocks[0].data.extend_from_slice(&[0, 0, 0]);
        cases.push(bad);
        let mut bad = valid.clone();
        bad.blocks[0].data.push(1);
        cases.push(bad);
        let mut bad = valid.clone();
        bad.blocks.push(bad.blocks[0].clone());
        cases.push(bad);
        cases.extend([record(&[0xff]), record(b"[]"), record(b"null"), record(b"{broken"), record(&vec![b' '; MAX_BYTES + 1])]);
        let mut deep = json!(null);
        for _ in 0..MAX_DEPTH + 1 {
            deep = json!({"nested":deep});
        }
        cases.push(record(&serde_json::to_vec(&deep).unwrap()));
        cases.push(record(&serde_json::to_vec(&json!({"values":vec![0; MAX_VALUES]})).unwrap()));
        for record in cases {
            let mut left = DOCUMENT_BYTES;
            assert!(restore(&record, &mut left).is_err());
            assert_eq!(left, DOCUMENT_BYTES);
        }
        let mut left = 1;
        assert!(restore(&valid, &mut left).is_err());
        assert_eq!(left, 1);
    }
    #[test]
    fn generation_budget_is_shared_and_clearing_removes_owned_bytes() {
        let layer = layer(json!({"prompt":"hi"}));
        let (_, cost) = encode(layer.generation.as_ref().unwrap()).unwrap();
        let mut left = cost * 2;
        let mut record = LayerRecord::default();
        attach(&layer, &mut record, &mut left).unwrap();
        attach(&layer, &mut record, &mut left).unwrap();
        assert_eq!(left, 0);
        assert!(attach(&layer, &mut record, &mut left).is_err());
        assert!(record.block(&KEY).is_none());
        let mut cleared = layer.clone();
        cleared.generation = None;
        let mut export_budget = DOCUMENT_BYTES;
        attach(&layer, &mut record, &mut export_budget).unwrap();
        attach(&cleared, &mut record, &mut left).unwrap();
        assert!(record.block(&KEY).is_none());
    }
}
