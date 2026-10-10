//! Optional T2 cleanup around the existing T1 evaluator. No document mutation or pathops edge.
//! All tolerances and returned geometry are in source pixels, including upscaled tracing.
mod contour;
mod geometry;
pub use geometry::clean_geometry;
mod palette_trace;
mod preprocess;
mod recognize;
mod symmetry;
mod topology;

use crate::{Error, Fitter, Image, Params, Preset, Trace};
use photocraft_geom::Affine;
use serde::{Deserialize, Serialize};

/// Explicit scale requests fail before allocation if their working image exceeds T1's budget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Upscale {
    #[default]
    Off,
    Auto,
    X2,
    X4,
}
impl Upscale {
    fn factor(self, image: Image<'_>) -> u32 {
        match self {
            Self::Off => 1,
            Self::X2 => 2,
            Self::X4 => 4,
            Self::Auto => match image.width.max(image.height) {
                0..=127 => 4,
                128..=511 => 2,
                _ => 1,
            },
        }
    }
}

/// Default is the legacy evaluator. The dialog can opt into `logo()` for a new Logo request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub prefilter: bool,
    pub filter_sigma: f64,
    pub upscale: Upscale,
    pub aa_palette: bool,
    pub axes: bool,
    pub collinear: bool,
    pub right_angles: bool,
    pub circles: bool,
    pub ellipses: bool,
    pub arcs: bool,
    pub symmetry: bool,
    pub small_holes: bool,
    /// Maximum displacement, in source pixels. Applies to handles/curves as well as anchors.
    pub tolerance: f64,
    pub angle_degrees: f64,
    /// Hole area in source pixels; only enclosed, low-contrast opaque defects are filled.
    pub hole_area: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            prefilter: false,
            filter_sigma: 16.,
            upscale: Upscale::Off,
            aa_palette: false,
            axes: false,
            collinear: false,
            right_angles: false,
            circles: false,
            ellipses: false,
            arcs: false,
            symmetry: false,
            small_holes: false,
            tolerance: 0.2,
            angle_degrees: 2.,
            hole_area: 4,
        }
    }
}
impl Options {
    pub fn logo() -> Self {
        Self {
            tolerance: 0.75,
            upscale: Upscale::Auto,
            aa_palette: true,
            axes: true,
            collinear: true,
            right_angles: true,
            circles: true,
            ellipses: true,
            arcs: true,
            symmetry: true,
            small_holes: true,
            ..Self::default()
        }
    }
    pub fn validate(&self) -> Result<(), Error> {
        if !self.tolerance.is_finite()
            || !(0.001..=2.).contains(&self.tolerance)
            || !self.angle_degrees.is_finite()
            || !(0.0..=10.).contains(&self.angle_degrees)
            || !self.filter_sigma.is_finite()
            || !(1.0..=64.).contains(&self.filter_sigma)
            || self.hole_area > 1024
        {
            return Err(Error::InvalidParams("cleanup tolerance/angle/filter/hole bounds"));
        }
        Ok(())
    }
    fn geometry_enabled(&self) -> bool {
        self.axes || self.collinear || self.right_angles || self.circles || self.ellipses || self.arcs || self.symmetry
    }
}

/// Deterministic counters: wall-clock time is measured by the caller rather than serialized here.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub scale: u32,
    pub working_pixels: usize,
    pub filtered_pixels: usize,
    pub palette_pixels: usize,
    pub palette: Vec<[u8; 3]>,
    pub filled_holes: usize,
    pub nodes_before_geometry: usize,
    pub nodes_after: usize,
    pub axis_segments: usize,
    pub collinear_nodes: usize,
    pub right_angle_corners: usize,
    pub circles: usize,
    pub ellipses: usize,
    pub arcs: usize,
    pub symmetry_axes: usize,
    pub symmetry_iou: Vec<f64>,
    pub rejected_paths: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Output {
    pub trace: Trace,
    pub stats: Stats,
}

pub fn trace(image: Image<'_>, params: &Params, options: &Options) -> Result<Output, Error> {
    trace_with_cancel(image, params, options, || false)
}

/// Pure, detached and cancellable. Integrators pass the job context's cancellation callback.
pub fn trace_with_cancel(image: Image<'_>, params: &Params, options: &Options, cancel: impl Fn() -> bool) -> Result<Output, Error> {
    image.validate()?;
    params.validate()?;
    options.validate()?;
    if params.speckle > crate::PIXEL_BUDGET || params.watershed_detail > 16000 {
        return Err(Error::InvalidParams("cleanup noise/watershed parameters exceed image bounds"));
    }
    check(&cancel)?;
    let scale = options.upscale.factor(image);
    let pixels = u64::from(image.width) * u64::from(image.height) * u64::from(scale).pow(2);
    if pixels > crate::PIXEL_BUDGET as u64 {
        return Err(Error::PixelBudget(crate::PIXEL_BUDGET));
    }
    let mut stats = Stats { scale, working_pixels: pixels as usize, ..Stats::default() };
    let prepared = preprocess::prepare(image, params, options, scale, &mut stats, &cancel)?;
    let mut working_params = params.clone();
    if options.prefilter || options.aa_palette || options.small_holes || scale > 1 {
        working_params.use_alpha = true;
        working_params.ignore_color = None;
    }
    working_params.detail = (params.detail * f64::from(scale)).min(16.);
    working_params.opttolerance = (params.opttolerance * f64::from(scale)).min(16.);
    working_params.speckle = params.speckle.saturating_mul(scale as usize * scale as usize);
    working_params.adaptive_window = params.adaptive_window.saturating_mul(scale).min(16000);
    if options.aa_palette && params.palette.is_empty() && !stats.palette.is_empty() && !matches!(params.preset, Preset::BlackWhite | Preset::Silhouette) {
        working_params.palette.clone_from(&stats.palette);
    }
    let mut out = if options.aa_palette && !stats.palette.is_empty() && !matches!(params.preset, Preset::BlackWhite | Preset::Silhouette) {
        palette_trace::trace(image, &prepared, &working_params, &stats.palette, scale, &cancel)?
    } else {
        crate::trace_with_cancel(Image { width: image.width * scale, height: image.height * scale, rgba: &prepared.pixels }, &working_params, &cancel)?
    };
    if params.fitter == Fitter::Pixel && scale > 1 {
        // Repeated source pixels add unit working-grid anchors along straight edges.
        // Keep only their exact corners before returning to the source lattice.
        for layer in &mut out.layers {
            for sub in &mut layer.path.subpaths {
                check(&cancel)?;
                let points: Vec<_> = sub.knots.iter().map(|k| kurbo::Point::new(k.anchor.x, k.anchor.y)).collect();
                sub.knots = crate::boundary::remove_collinear(&points).into_iter().map(|p| photocraft_doc::Knot::corner(p.x, p.y)).collect();
            }
        }
        out.nodes = out.layers.iter().flat_map(|l| &l.path.subpaths).map(|s| s.knots.len()).sum();
    }
    let inverse = Affine::scale(1. / f64::from(scale));
    for layer in &mut out.layers {
        layer.path = layer.path.transform(&inverse);
    }
    out.width = image.width;
    out.height = image.height;
    stats.nodes_before_geometry = out.nodes;
    if options.geometry_enabled() {
        geometry::clean(&mut out, options, &prepared.pixels, scale, &mut stats, &cancel)?;
    }
    check(&cancel)?;
    out.nodes = out.layers.iter().flat_map(|l| &l.path.subpaths).map(|s| s.knots.len()).sum();
    stats.nodes_after = out.nodes;
    Ok(Output { trace: out, stats })
}

pub(super) fn check(cancel: &impl Fn() -> bool) -> Result<(), Error> {
    if cancel() { Err(Error::Cancelled) } else { Ok(()) }
}

#[cfg(test)]
#[path = "../../tests/cleanup/mod.rs"]
mod tests;
