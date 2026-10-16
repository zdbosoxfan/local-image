//! Strict CCITT admission for the exact dimensions/options used by pinned hayro.
use super::{ImportError, ImportOptions, Result, Value, ccitt, invalid, predictors};
use std::sync::atomic::{AtomicBool, Ordering};

fn boolean(params: Option<&Value>, name: &[u8], default: bool) -> Result<bool> {
    match params.and_then(|p| p.get(name)) {
        None => Ok(default),
        Some(Value::Bool(b)) => Ok(*b),
        _ => Err(invalid("CCITT option must be a direct boolean")),
    }
}
struct Counter<'a> {
    columns: u32,
    height: u32,
    column: u32,
    rows: u32,
    cancel: &'a AtomicBool,
}
impl Counter<'_> {
    fn add(&mut self, pixels: u32) -> ccitt::Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(ccitt::DecodeError::Cancelled);
        }
        if self.rows >= self.height || pixels > self.columns.saturating_sub(self.column) {
            return Err(ccitt::DecodeError::OutputLimit);
        }
        self.column += pixels;
        Ok(())
    }
}
impl ccitt::Decoder for Counter<'_> {
    fn push_pixel(&mut self, _: bool) -> ccitt::Result<()> {
        self.add(1)
    }
    fn push_pixel_chunk(&mut self, _: bool, chunks: u32) -> ccitt::Result<()> {
        self.add(chunks.checked_mul(8).ok_or(ccitt::DecodeError::OutputLimit)?)
    }
    fn next_line(&mut self) -> ccitt::Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(ccitt::DecodeError::Cancelled);
        }
        if self.column != self.columns || self.rows >= self.height {
            return Err(ccitt::DecodeError::LineLengthMismatch);
        }
        self.rows += 1;
        self.column = 0;
        Ok(())
    }
}
pub(super) fn check(value: &Value, params: Option<&Value>, data: &[u8], options: &ImportOptions, room: u64, cancel: &AtomicBool) -> Result<u64> {
    run(value, params, data, options, room, cancel, false).map(|(cost, _)| cost)
}
pub(super) fn prefix_bits(value: &Value, params: Option<&Value>, data: &[u8], options: &ImportOptions, room: u64, cancel: &AtomicBool) -> Result<(u64, usize)> {
    run(value, params, data, options, room, cancel, true)
}
fn run(value: &Value, params: Option<&Value>, data: &[u8], options: &ImportOptions, room: u64, cancel: &AtomicBool, prefix: bool) -> Result<(u64, usize)> {
    if [b"W".as_slice(), b"H", b"BPC", b"CS", b"IM"].iter().any(|key| value.get(key).is_some()) {
        return Err(ImportError::Unsupported("CCITT image aliases require bounded normalization"));
    }
    let width = value.get(b"Width").and_then(Value::number).ok_or_else(|| invalid("CCITT image width"))?;
    let height = value.get(b"Height").and_then(Value::number).ok_or_else(|| invalid("CCITT image height"))?;
    let columns = predictors::parameter(params, b"Columns", 1728)?;
    // Missing Rows uses image height in hayro-syntax0.7.2. Explicit zero does
    // not: Group3 would continue to EOF and Group4 would stop before any row.
    // Never validate to one bound then let downstream decode to another.
    let rows = predictors::parameter(params, b"Rows", height)?;
    if rows == 0 {
        return Err(ImportError::Unsupported("CCITT Rows=0 requires decoder normalization"));
    }
    if width != columns || height != rows || width == 0 || height == 0 || width > 1_000_000 || height > 1_000_000 {
        return Err(invalid("CCITT decoder and image dimensions must match"));
    }
    let stencil = match value.get(b"ImageMask") {
        None => false,
        Some(Value::Bool(b)) => *b,
        _ => return Err(invalid("CCITT ImageMask must be a direct boolean")),
    };
    let bits = match value.get(b"BitsPerComponent") {
        None if stencil => Some(1),
        Some(v) => v.number(),
        _ => None,
    };
    if bits != Some(1) {
        return Err(invalid("CCITT image must use one-bit components"));
    }
    if !stencil && value.get(b"ColorSpace").and_then(Value::name) != Some(b"DeviceGray") {
        return Err(ImportError::Unsupported("CCITT color space requires bounded normalization"));
    }
    if predictors::parameter(params, b"DamagedRowsBeforeError", 0)? != 0 {
        return Err(ImportError::Unsupported("CCITT damaged-row repair"));
    }
    let k = match params.and_then(|p| p.get(b"K")) {
        None => 0,
        Some(Value::Number(n)) if n.fract() == 0.0 && *n >= i32::MIN as f64 && *n <= i32::MAX as f64 => *n as i32,
        _ => return Err(invalid("CCITT K parameter")),
    };
    // Admission retains only row transitions. Charge both bounded row vectors
    // and downstream Luma8 allocation (including its growth spare capacity).
    let cost =
        width.checked_mul(height).and_then(|n| n.checked_mul(2)).and_then(|n| n.checked_add(width * 64)).ok_or(ImportError::Limit("CCITT allocation"))?;
    if cost > room {
        return Err(ImportError::Limit("CCITT allocation"));
    }
    let settings = ccitt::DecodeSettings {
        columns: width as u32,
        rows: height as u32,
        end_of_block: boolean(params, b"EndOfBlock", true)?,
        end_of_line: boolean(params, b"EndOfLine", false)?,
        rows_are_byte_aligned: boolean(params, b"EncodedByteAlign", false)?,
        invert_black: boolean(params, b"BlackIs1", false)?,
        encoding: if k < 0 {
            ccitt::EncodingMode::Group4
        } else if k == 0 {
            ccitt::EncodingMode::Group3_1D
        } else {
            ccitt::EncodingMode::Group3_2D { k: k as u32 }
        },
    };
    let mut counter = Counter { columns: settings.columns, height: settings.rows, column: 0, rows: 0, cancel };
    let work = options.limits.segments.saturating_mul(16).min(16_000_000);
    let mut context = ccitt::DecoderContext::new(settings);
    let decoded = if prefix {
        ccitt::decode_prefix_bits(data, &mut counter, &mut context, cancel, work)
    } else {
        ccitt::decode(data, &mut counter, &mut context, cancel, work)
    };
    let consumed = decoded.map_err(|e| match e {
        ccitt::DecodeError::Cancelled => ImportError::Cancelled,
        ccitt::DecodeError::WorkLimit => ImportError::Limit("CCITT decode work"),
        ccitt::DecodeError::OutputLimit => ImportError::Limit("CCITT decoded output"),
        _ => invalid(&format!("CCITT stream: {e}")),
    })?;
    if counter.rows != settings.rows || counter.column != 0 {
        return Err(invalid("CCITT image ended before all declared rows"));
    }
    Ok((cost, consumed))
}
