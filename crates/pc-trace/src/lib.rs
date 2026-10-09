//! Deterministic, pure Rust image tracing with stacked colour regions and two curve fitters.
//! The T1 engine owns its adapter to photocraft-doc; it has no pc-pathops dependency.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
pub mod adapter;
mod boundary;
mod colorfit;
mod error;
mod frontend;
mod ir;
mod potrace;
mod spline;
mod topology;
mod vc;
use colorfit::ColorFitter;
pub use error::Error;
use frontend::Frontend;
pub use frontend::Threshold;
use photocraft_doc::{Path, PathOp};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

/// Memory bound applied before allocation, including preview inputs.
pub const PIXEL_BUDGET: usize = 16_777_216;
// Allow all four lattice edges per pixel in the full 1024² corpus, plus
// stacked overlap, while bounding pathological high-resolution geometry.
const BOUNDARY_BUDGET: usize = 8_000_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    Logo,
    BlackWhite,
    FewColors,
    Silhouette,
    LineArt,
    Sketch,
    Photo,
    PixelArt,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fitter {
    Potrace,
    Spline,
    Pixel,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Params {
    pub preset: Preset,
    pub fitter: Fitter,
    pub max_colors: usize,
    pub palette: Vec<[u8; 3]>,
    pub detail: f64,
    pub alphamax: f64,
    pub opttolerance: f64,
    pub speckle: usize,
    pub color_precision_loss: i32,
    pub layer_difference: i32,
    pub watershed_detail: u32,
    pub adaptive: bool,
    pub threshold: u8,
    pub adaptive_window: u32,
    pub adaptive_sensitivity: f64,
    pub use_alpha: bool,
    pub ignore_color: Option<[u8; 3]>,
}
impl Default for Params {
    fn default() -> Self {
        Self::for_preset(Preset::Logo)
    }
}
impl Params {
    pub fn for_preset(preset: Preset) -> Self {
        let mut p = Self {
            preset,
            fitter: Fitter::Potrace,
            max_colors: 8,
            palette: Vec::new(),
            detail: 0.2,
            alphamax: 1.,
            opttolerance: 0.2,
            speckle: 4,
            color_precision_loss: 2,
            layer_difference: 16,
            watershed_detail: 128,
            adaptive: false,
            threshold: 128,
            adaptive_window: 0,
            adaptive_sensitivity: 15.,
            use_alpha: true,
            ignore_color: None,
        };
        match preset {
            Preset::BlackWhite => {
                p.max_colors = 1;
                p.speckle = 2;
            }
            Preset::FewColors => p.max_colors = 6,
            Preset::Silhouette => p.max_colors = 1,
            Preset::LineArt | Preset::Sketch => {
                p.adaptive = true;
                p.max_colors = 1;
            }
            Preset::Photo => {
                p.max_colors = 64;
                p.fitter = Fitter::Spline;
                p.detail = 0.5;
                p.speckle = 16;
            }
            Preset::PixelArt => {
                p.fitter = Fitter::Pixel;
                p.max_colors = 64;
                p.speckle = 0;
                p.color_precision_loss = 0;
                p.layer_difference = 0;
            }
            Preset::Logo => {}
        }
        p
    }
    pub fn validate(&self) -> Result<(), Error> {
        if matches!(self.preset, Preset::LineArt | Preset::Sketch) {
            return Err(Error::InvalidParams("centreline tracing is not available"));
        }
        if !(1..=64).contains(&self.max_colors) || self.palette.len() > 64 {
            return Err(Error::InvalidParams("colours must be 1..64"));
        }
        if !self.detail.is_finite() || !(0.01..=16.).contains(&self.detail) {
            return Err(Error::InvalidParams("detail must be finite and 0.01..16"));
        }
        if !self.alphamax.is_finite() || !(0.0..=4.).contains(&self.alphamax) || !self.opttolerance.is_finite() || !(0.0..=16.).contains(&self.opttolerance) {
            return Err(Error::InvalidParams("corner/optimisation tolerance"));
        }
        if !(0..=7).contains(&self.color_precision_loss)
            || !(0..=765).contains(&self.layer_difference)
            || !self.adaptive_sensitivity.is_finite()
            || !(0.0..=100.).contains(&self.adaptive_sensitivity)
            || self.adaptive_window > 16000
        {
            return Err(Error::InvalidParams("clustering/threshold parameters"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColorPath {
    pub color: [u8; 4],
    pub path: Path,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Trace {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<ColorPath>,
    pub nodes: usize,
}
/// An RGBA8 view; dimensions and length are checked before any reads or allocations.
#[derive(Clone, Copy)]
pub struct Image<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}
impl Image<'_> {
    pub fn validate(&self) -> Result<(), Error> {
        if self.width == 0 || self.height == 0 {
            return Err(Error::EmptyImage);
        }
        let n = u64::from(self.width) * u64::from(self.height);
        if n > PIXEL_BUDGET as u64 {
            return Err(Error::PixelBudget(PIXEL_BUDGET));
        }
        if self.rgba.len() != n as usize * 4 {
            return Err(Error::InvalidBuffer);
        }
        Ok(())
    }
}
pub fn trace(image: Image<'_>, params: &Params) -> Result<Trace, Error> {
    trace_cancellable(image, params, &AtomicBool::new(false))
}
/// Cancellation is observed before segmentation, after segmentation, and between fitted regions.
pub fn trace_cancellable(image: Image<'_>, params: &Params, cancel: &AtomicBool) -> Result<Trace, Error> {
    trace_with_cancel(image, params, || cancel.load(Ordering::Relaxed))
}
/// Callback form for the engine job context.
pub fn trace_with_cancel(image: Image<'_>, params: &Params, cancel: impl Fn() -> bool) -> Result<Trace, Error> {
    image.validate()?;
    params.validate()?;
    let check = || if cancel() { Err(Error::Cancelled) } else { Ok(()) };
    check()?;
    let mut img = vc::ColorImage { width: image.width as usize, height: image.height as usize, pixels: image.rgba.to_vec() };
    let mut has_alpha = false;
    for px in img.pixels.as_chunks_mut::<4>().0 {
        if !params.use_alpha {
            px[3] = 255;
        }
        if params.ignore_color == Some([px[0], px[1], px[2]]) {
            px[3] = 0;
        }
        if px[3] < 128 {
            has_alpha = true;
            px[3] = 0;
        } else {
            px[3] = 255;
        }
    }
    if img.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        return Ok(Trace { width: image.width, height: image.height, layers: Vec::new(), nodes: 0 });
    }
    let binary = matches!(params.preset, Preset::BlackWhite | Preset::Silhouette);
    let mut seg = if binary {
        if params.preset == Preset::Silhouette && has_alpha {
            for p in img.pixels.as_chunks_mut::<4>().0 {
                let v = if p[3] == 0 { 255 } else { 0 };
                p[..3].fill(v);
            }
        } else {
            for p in img.pixels.as_chunks_mut::<4>().0 {
                if p[3] == 0 {
                    p[..3].fill(255);
                }
            }
        }
        frontend::BinaryFrontend {
            threshold: if params.adaptive {
                Threshold::Adaptive { window: params.adaptive_window, t: params.adaptive_sensitivity }
            } else {
                Threshold::Fixed(params.threshold)
            },
            diagonal: false,
            min_area: params.speckle,
        }
        .segment_cancellable(&img, &cancel)?
    } else if params.preset == Preset::Photo && !has_alpha {
        frontend::WatershedFrontend { detail: params.watershed_detail, min_area: params.speckle }.segment_cancellable(&img, &cancel)?
    } else {
        frontend::ColorClusterFrontend {
            color_precision_loss: params.color_precision_loss,
            layer_difference: params.layer_difference,
            good_min_area: params.speckle,
        }
        .segment_cancellable(&img, &cancel)?
    };
    check()?;
    if !params.palette.is_empty() {
        colorfit::FixedPalette::new(params.palette.iter().map(|c| vc::Color::new(c[0], c[1], c[2])).collect()).fit(&mut seg);
    } else if !binary {
        colorfit::AutoQuantize { max_colors: params.max_colors }.fit(&mut seg);
    }
    let mut layers: Vec<ColorPath> = Vec::new();
    let mut boundary_budget = BOUNDARY_BUDGET;
    for layer in ir::stack_colors(seg).layers {
        check()?;
        let c = layer.paint.color();
        let color = [c.r, c.g, c.b, 255];
        let loops = boundary::contours(&layer.mask.image, &mut boundary_budget, &cancel)?;
        let mut path = Path::default();
        for points in &loops {
            check()?;
            if points.len() < 3 {
                continue;
            }
            let bez = match params.fitter {
                Fitter::Potrace => spline::refine(&potrace::fit(points, params.alphamax, params.opttolerance), params.detail),
                Fitter::Spline => spline::fit(points, params.detail),
                Fitter::Pixel => boundary::polygon(points),
            };
            let mut part = adapter::bezpath_to_path(&bez)?;
            for s in &mut part.subpaths {
                for k in &mut s.knots {
                    let offset = photocraft_geom::Point::new(layer.mask.offset.x as f64, layer.mask.offset.y as f64);
                    k.anchor.x += offset.x;
                    k.anchor.y += offset.y;
                    k.in_ctrl.x += offset.x;
                    k.in_ctrl.y += offset.y;
                    k.out_ctrl.x += offset.x;
                    k.out_ctrl.y += offset.y;
                }
                s.op = PathOp::Join;
            }
            path.subpaths.extend(part.subpaths);
        }
        if topology::crosses(&path) {
            path = Path::default();
            for points in &loops {
                let bez = boundary::polygon(&boundary::remove_collinear(points));
                let mut part = adapter::bezpath_to_path(&bez)?;
                for s in &mut part.subpaths {
                    s.op = PathOp::Join;
                    for k in &mut s.knots {
                        let dx = layer.mask.offset.x as f64;
                        let dy = layer.mask.offset.y as f64;
                        k.anchor.x += dx;
                        k.anchor.y += dy;
                        k.in_ctrl = k.anchor;
                        k.out_ctrl = k.anchor;
                    }
                }
                path.subpaths.extend(part.subpaths);
            }
        }
        if path.is_empty() {
            continue;
        }
        if let Some(first) = path.subpaths.first_mut() {
            first.op = PathOp::Combine;
        }
        // Consecutive equal paints can be folded without changing their stacked occlusion.
        if let Some(last) = layers.last_mut().filter(|l| l.color == color) {
            for s in &mut path.subpaths {
                s.op = PathOp::Join;
            }
            last.path.subpaths.extend(path.subpaths);
        } else {
            layers.push(ColorPath { color, path });
        }
    }
    let nodes = layers.iter().flat_map(|l| &l.path.subpaths).map(|s| s.knots.len()).sum();
    Ok(Trace { width: image.width, height: image.height, layers, nodes })
}
