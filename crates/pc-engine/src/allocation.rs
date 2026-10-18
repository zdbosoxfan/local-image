//! Fallible buffers for memory-intensive document operations.
//!
//! A canvas may be up to 300000 × 300000 pixels, so a whole-canvas buffer can exceed what the
//! machine can reserve. `vec![..]` would abort the process; these reserve with `try_reserve_exact`
//! and return an ordinary command error instead, leaving the document unchanged.

use crate::{EngineError, Result};

#[cfg(test)]
thread_local! {
    static FAIL_NEXT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Reserve and initialize a buffer without invoking the allocator's abort-on-OOM path.
pub(crate) fn filled<T: Clone>(len: usize, value: T, purpose: &str) -> Result<Vec<T>> {
    let mut out = capacity(len, purpose)?;
    out.resize(len, value);
    Ok(out)
}

/// Reserve a vector for a known number of operation results.
pub(crate) fn capacity<T>(len: usize, purpose: &str) -> Result<Vec<T>> {
    let bytes = len.checked_mul(std::mem::size_of::<T>()).ok_or_else(|| error(purpose, "requested size overflowed"))?;
    #[cfg(test)]
    if FAIL_NEXT.with(|flag| flag.replace(false)) {
        return Err(error(purpose, "not enough memory"));
    }
    let mut out = Vec::new();
    out.try_reserve_exact(len).map_err(|_| error(purpose, &format!("not enough memory for {bytes} bytes")))?;
    Ok(out)
}

/// Copy a region of `surface` into an explicitly reserved buffer.
pub(crate) fn read_region(surface: &photocraft_raster::Surface, area: photocraft_geom::Rect, purpose: &str) -> Result<Vec<f32>> {
    let len = (area.width() as usize)
        .checked_mul(area.height() as usize)
        .and_then(|n| n.checked_mul(surface.channels()))
        .ok_or_else(|| error(purpose, "requested size overflowed"))?;
    let mut out = filled(len, 0.0, purpose)?;
    surface.read_region_into(area, &mut out);
    Ok(out)
}

pub(crate) fn error(purpose: &str, detail: &str) -> EngineError {
    EngineError::Other(format!("not enough memory for {purpose} ({detail}); the document was not changed"))
}

/// Make the next reservation on this thread fail, so tests can prove a command's transaction
/// boundary.
#[cfg(test)]
pub(crate) fn fail_next_for_test() {
    FAIL_NEXT.with(|flag| flag.set(true));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_failure_is_reported_without_panicking() {
        fail_next_for_test();
        let result = filled::<u8>(16, 0, "test image");
        assert!(result.as_ref().is_err_and(|e| e.to_string().contains("not enough memory for test image")));
    }

    #[test]
    fn oversized_requests_are_errors_not_aborts() {
        let err = capacity::<[f32; 4]>(usize::MAX / 2, "a huge canvas").unwrap_err();
        assert!(err.to_string().contains("not enough memory for a huge canvas"), "{err}");
        let surface = photocraft_raster::Surface::new(photocraft_color::PixelFormat::RGBA8);
        let whole = photocraft_geom::Rect::new(i32::MIN, i32::MIN, i32::MAX, i32::MAX);
        assert!(read_region(&surface, whole, "reading").is_err());
    }
}
