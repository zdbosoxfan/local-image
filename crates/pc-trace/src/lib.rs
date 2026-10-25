//! Deterministic, pure Rust image tracing with stacked colour regions and two curve fitters.
//! The T1 engine owns its adapter to photocraft-doc; it has no pc-pathops dependency.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed")]
#![cfg_attr(test, expect(clippy::map_err_ignore, reason = "baseline 2026-10-25: unreviewed"))]

pub mod adapter;
pub mod cleanup;
#[cfg(test)]
extern crate self as pc_trace;
mod boundary;
pub mod centerline;
mod colorfit;
mod error;
mod frontend;
mod ir;
mod potrace;
mod spline;
#[cfg(test)]
#[path = "../tests/it/common/mod.rs"]
mod test_common;
mod topology;
mod vc;
pub use error::Error;
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
pub enum Mode {
    Fill,
    Centerline,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
    pub preset: Preset,
    pub layering: cutout::Layering,
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
            mode: None,
            preset,
            layering: cutout::Layering::Stacked,
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
                p.speckle = 0;
                if preset == Preset::Sketch {
                    p.detail = 0.8;
                }
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
    pub fn resolved_mode(&self) -> Mode {
        self.mode.unwrap_or(if matches!(self.preset, Preset::LineArt | Preset::Sketch) { Mode::Centerline } else { Mode::Fill })
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.resolved_mode() == Mode::Centerline && !matches!(self.preset, Preset::LineArt | Preset::Sketch | Preset::BlackWhite) {
            return Err(Error::InvalidParams("centreline requires Line Art, Sketch or Black & White"));
        }
        if self.resolved_mode() == Mode::Fill && matches!(self.preset, Preset::LineArt | Preset::Sketch) {
            return Err(Error::InvalidParams("Line Art and Sketch require centreline mode"));
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strokes: Vec<centerline::StrokePath>,
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
    if params.resolved_mode() == Mode::Centerline {
        let out = centerline::trace_with_cancel(image, params, cancel)?;
        return Ok(Trace { width: out.width, height: out.height, layers: Vec::new(), strokes: out.strokes, nodes: out.nodes });
    }
    let check = || if cancel() { Err(Error::Cancelled) } else { Ok(()) };
    check()?;
    let seg = frontend::segment(image, params, &cancel)?;
    if params.layering == cutout::Layering::Cutout {
        return cutout::evaluate(&seg, params, BOUNDARY_BUDGET, &cancel);
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
    Ok(Trace { width: image.width, height: image.height, strokes: Vec::new(), layers, nodes })
}

// Shared-boundary cutout evaluator.
pub mod cutout;
