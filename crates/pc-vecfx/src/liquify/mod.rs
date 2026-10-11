//! Checked document-path interface to the VectorCraft liquify kernels.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0; see kernel.rs and licenses/vectorcraft-NOTICE.
mod kernel;
use kernel::{Dabber, PathStroke};
pub use kernel::{LiquifyKind as Kind, LiquifyParams as Options, MAX_ANCHORS as MAX_NODES, MAX_DABS};
use photocraft_doc::Path;
use serde::{Deserialize, Serialize};
pub const MAX_COORDINATE: f64 = 1_000_000.0;
pub const MAX_SAMPLES: usize = 4096;
pub type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub at: [f64; 2],
    #[serde(default = "full_pressure")]
    pub pressure: f64,
}
fn full_pressure() -> f64 {
    1.0
}
impl Sample {
    pub fn new(x: f64, y: f64, pressure: f64) -> Self {
        Self { at: [x, y], pressure }
    }
    pub fn validate(&self) -> Result<()> {
        valid_point(self.at)?;
        bounded(self.pressure, 0.0, 1.0)
    }
}
pub fn valid_point(p: [f64; 2]) -> Result<()> {
    for v in p {
        bounded(v, -MAX_COORDINATE, MAX_COORDINATE)?;
    }
    Ok(())
}
fn bounded(v: f64, lo: f64, hi: f64) -> Result<()> {
    if v.is_finite() && (lo..=hi).contains(&v) { Ok(()) } else { Err("distort input must be finite and bounded".into()) }
}
impl Default for Options {
    fn default() -> Self {
        Self::new(Kind::Warp)
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        for v in [self.width, self.height] {
            bounded(v, 0.5, 10_000.0)?;
        }
        bounded(self.angle, -360.0, 360.0)?;
        bounded(self.intensity, 0.0, 1.0)?;
        bounded(self.detail, 1.0, 10.0)?;
        bounded(self.simplify, 0.0, 100.0)?;
        bounded(self.rate, -180.0, 180.0)?;
        bounded(self.complexity, 0.0, 15.0)?;
        bounded(self.horizontal, 0.0, 1.0)?;
        bounded(self.vertical, 0.0, 1.0)?;
        if !(4..=MAX_NODES).contains(&self.node_budget) {
            return Err("invalid distort node budget".into());
        }
        Ok(())
    }
}
pub fn node_count(path: &Path) -> usize {
    path.subpaths.iter().map(|s| s.knots.len()).sum()
}
pub fn validate_path(path: &Path, budget: usize) -> Result<()> {
    if path.subpaths.len() > 1024 || node_count(path) > budget {
        return Err("distort path exceeds node budget".into());
    }
    for p in path.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]) {
        valid_point([p.x, p.y])?;
    }
    Ok(())
}
/// Immutable stroke with an incremental dab prefix; no resampling of already applied dabs.
#[derive(Clone, Debug)]
pub struct Stroke {
    base: Path,
    options: Options,
    dabber: Dabber,
    state: PathStroke,
}
impl Stroke {
    pub fn new(base: Path, options: Options, seed: u64) -> Result<Self> {
        options.validate()?;
        validate_path(&base, options.node_budget)?;
        Ok(Self { state: PathStroke::new(photocraft_pathops::to_geometry(&base), seed), base, dabber: Dabber::new(&options), options })
    }
    /// Evaluated geometry captured before the first dab.
    pub fn base(&self) -> &Path {
        &self.base
    }
    pub fn push(&mut self, sample: Sample) -> Result<()> {
        sample.validate()?;
        if self.dabber.samples().len() >= MAX_SAMPLES {
            return Err("distort stroke exceeds sample budget".into());
        }
        if !self.dabber.try_push((kurbo::Point::new(sample.at[0], sample.at[1]), sample.pressure)) {
            return Err("distort stroke exceeds dab budget".into());
        }
        self.state.advance(&self.dabber.dabs, &self.options);
        Ok(())
    }
    pub fn path(&self) -> Result<Path> {
        let Some(geometry) = self.state.finish(&self.dabber.dabs, self.dabber.tail(), &self.options) else {
            return Ok(self.base.clone());
        };
        let mut out = photocraft_pathops::from_geometry(&geometry);
        out.fill_rule = self.base.fill_rule;
        out.inverted = self.base.inverted;
        for (sp, old) in out.subpaths.iter_mut().zip(&self.base.subpaths) {
            sp.op = old.op;
        }
        validate_path(&out, self.options.node_budget)?;
        let tolerance = self.options.simplify_tolerance();
        if tolerance > 0.0 {
            for (sp, old) in out.subpaths.iter_mut().zip(&self.base.subpaths) {
                if sp == old {
                    continue;
                }
                let original = sp.clone();
                let single = Path::new(vec![original.clone()]);
                let fitted = photocraft_pathops::simplify_interactive(&single, tolerance).map_err(|e| e.to_string())?;
                if fitted.subpaths.len() != 1 {
                    return Err("liquify refit changed contour topology".into());
                }
                let fitted = fitted.subpaths.into_iter().next().ok_or_else(|| "liquify refit lost a contour".to_string())?;
                *sp = photocraft_doc::Subpath { op: original.op, ..fitted };
            }
        }
        validate_path(&out, self.options.node_budget)?;
        Ok(out)
    }
}
pub fn apply(path: &Path, samples: &[Sample], options: Options, seed: u64) -> Result<Path> {
    if samples.len() > MAX_SAMPLES {
        return Err("distort stroke exceeds sample budget".into());
    }
    let mut stroke = Stroke::new(path.clone(), options, seed)?;
    for &sample in samples {
        stroke.push(sample)?;
    }
    stroke.path()
}
#[cfg(test)]
#[path = "../../tests/liquify/tests.rs"]
mod tests;
