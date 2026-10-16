//! Native PDF image fallback boundary. The UI never executes a renderer or reads its files.
use photocraft_doc::{
    DesignUnits, Layer, LayerContent, PixelFormat, Rect, Size, Surface,
    vector_design::exchange::{ImportLimits, ImportedScene, VectorWarning},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub const MAX_PIXELS: u64 = 16_000_000;
pub const MAX_PAGES: usize = 1024;

#[derive(Clone, Debug)]
pub struct Request {
    pub bytes: Arc<[u8]>,
    pub limits: ImportLimits,
    pub operation: Operation,
}
#[derive(Clone, Copy, Debug)]
pub enum Operation {
    Metadata,
    Render { page: usize, dpi: f32, crop: bool, apply_rotation: bool },
}
#[derive(Debug)]
pub enum Reply {
    Metadata { pages: usize },
    Pixels { width: u32, height: u32, dpi: f32, rgba: Vec<u8> },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    Unavailable(String),
    Limit(&'static str),
    Failed(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("PDF image import cancelled"),
            Self::Unavailable(s) => write!(f, "PDF image import is unavailable: {s}"),
            Self::Limit(s) => write!(f, "PDF image import exceeds the {s} limit"),
            Self::Failed(s) => write!(f, "PDF image import failed: {s}"),
        }
    }
}
impl std::error::Error for Error {}
pub type Service = Arc<dyn Fn(Request, &AtomicBool) -> Result<Reply, Error> + Send + Sync>;

/// Build a detached, explicitly opaque page; publication remains the ordinary PDF transaction.
pub fn scene(reply: Reply, limits: ImportLimits, cancel: &AtomicBool) -> Result<ImportedScene, Error> {
    let Reply::Pixels { width, height, dpi, rgba } = reply else { return Err(Error::Failed("renderer returned metadata instead of pixels".into())) };
    let pixels = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || width > 1_000_000
        || height > 1_000_000
        || pixels > limits.pixels.min(MAX_PIXELS)
        || pixels.checked_mul(4) != Some(rgba.len() as u64)
    {
        return Err(Error::Limit("pixel"));
    }
    if !dpi.is_finite() || !(1.0..=2400.0).contains(&dpi) || rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 255) {
        return Err(Error::Failed("expected an opaque white-page rendering with a valid resolution".into()));
    }
    let mut surface = Surface::new(PixelFormat::RGBA8);
    for (y, bytes) in rgba.chunks_exact(width as usize * 4).enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let row: Vec<f32> = bytes.iter().map(|&v| f32::from(v) / 255.0).collect();
        surface.write_region(Rect::new(0, y as i32, width as i32, y as i32 + 1), &row);
    }
    Ok(ImportedScene {
        size: Size::new(width, height),
        resolution_dpi: dpi,
        units: DesignUnits::Points,
        layers: vec![Layer::new("PDF page image", LayerContent::Raster(surface))],
        symbols: vec![],
        warnings: vec![VectorWarning {
            feature: "pdf-page-image".into(),
            object: Some("PDF page image".into()),
            message: "The PDF page is an opaque image on white. Text and paths are not editable; keep the original PDF.".into(),
        }],
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    #[test]
    fn image_scene_rejects_bad_dimensions_alpha_and_cancellation() {
        let limits = ImportLimits::default();
        let render = |rgba| Reply::Pixels { width: 1, height: 1, dpi: 72.0, rgba };
        assert!(scene(render(vec![255; 3]), limits, &AtomicBool::new(false)).is_err());
        assert!(scene(render(vec![255, 255, 255, 0]), limits, &AtomicBool::new(false)).is_err());
        assert_eq!(scene(render(vec![255; 4]), limits, &AtomicBool::new(true)).unwrap_err(), Error::Cancelled);
        let page = scene(render(vec![255; 4]), limits, &AtomicBool::new(false)).unwrap();
        assert_eq!(page.layers.len(), 1);
        assert_eq!(page.warnings[0].feature, "pdf-page-image");
    }
}
