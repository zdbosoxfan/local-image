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

/// Most bytes of dense working buffers one command may hold while it expands sparse content (or
/// the canvas) into flat arrays: the allowance Image Size resamples within. A document stores
/// only the tiles it holds, so two distant tiles, or a huge empty canvas, make a small file whose
/// content bounds would need far more than any machine has.
pub(crate) const MAX_DENSE_BYTES: u64 = crate::image_cmds::MAX_RESAMPLE_BYTES;

/// Refuses, before anything is allocated, an operation that would hold `bytes_per_px` bytes of
/// dense buffers for every pixel of `area` at once.
pub(crate) fn check_dense(area: photocraft_geom::Rect, bytes_per_px: u64, purpose: &str) -> Result<()> {
    let px = u64::from(area.width()).saturating_mul(u64::from(area.height()));
    let bytes = px.saturating_mul(bytes_per_px);
    if bytes <= MAX_DENSE_BYTES {
        return Ok(());
    }
    Err(EngineError::Other(format!(
        "{purpose} would need {} MB of working memory for a {} x {} px area (limit {} MB); the document was not changed",
        bytes >> 20,
        area.width(),
        area.height(),
        MAX_DENSE_BYTES >> 20
    )))
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
    fn dense_area_past_the_budget_is_refused() {
        use photocraft_geom::Rect;
        assert!(check_dense(Rect::from_xywh(0, 0, 4096, 4096), 16, "test").is_ok());
        // Two tiles a million pixels apart: content bounds of a trillion pixels.
        let far = Rect::new(0, 0, 1 << 20, 1 << 20);
        assert!(check_dense(far, 16, "test").is_err_and(|e| e.to_string().contains("working memory")));
        // Overflowing sizes saturate instead of wrapping into a small number.
        assert!(check_dense(Rect::new(i32::MIN, i32::MIN, i32::MAX, i32::MAX), u64::MAX, "test").is_err());
    }

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
