//! Original Local Image V1-Q reference gates, using the existing independent vector metrics.
//! Inputs are resolved, disjoint filled regions; this module does not perform path operations.
use crate::vector;
use photocraft_doc::{Path, PathOp};
use photocraft_geom::{Affine, Rect};
use std::collections::BTreeMap;

/// Golden tolerances from vector-quality-plan A.2. Hausdorff is in document pixels.
#[derive(Clone, Copy, Debug)]
pub struct Thresholds {
    pub relative_area: f64,
    pub hausdorff: f64,
    pub iou: f64,
    pub node_ratio: f64,
}
impl Default for Thresholds {
    fn default() -> Self {
        Self { relative_area: 0.001, hausdorff: 0.05, iou: 0.999, node_ratio: 1.5 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Scores {
    pub relative_area: f64,
    pub hausdorff: f64,
    pub iou: f64,
    pub actual_nodes: usize,
    pub reference_nodes: usize,
    pub crossings: usize,
}
impl Scores {
    pub fn passes(self, limits: Thresholds) -> bool {
        [limits.relative_area, limits.hausdorff, limits.iou, limits.node_ratio].iter().all(|v| v.is_finite())
            && limits.relative_area >= 0.0
            && limits.hausdorff >= 0.0
            && (0.0..=1.0).contains(&limits.iou)
            && limits.node_ratio >= 1.0
            && self.relative_area.is_finite()
            && self.hausdorff.is_finite()
            && self.iou.is_finite()
            && self.relative_area <= limits.relative_area
            && self.hausdorff <= limits.hausdorff
            && self.iou >= limits.iou
            && self.actual_nodes as f64 <= self.reference_nodes as f64 * limits.node_ratio
            && self.crossings == 0
    }
}

fn combined(paths: &[Path]) -> Path {
    let mut out = Path::default();
    for path in paths {
        out.subpaths.extend(path.subpaths.iter().enumerate().map(|(i, s)| s.clone().with_op(if i == 0 { PathOp::Combine } else { PathOp::Join })));
    }
    out
}

/// Compare geometry rather than node equality. The independent Green integrals require
/// nonintersecting contours within each region; distinct regions must have disjoint interiors.
/// `view` maps document coordinates to the bounded 8x reference raster, including large/tiny cases.
pub fn compare_regions(actual: &[Path], reference: &[Path], view: Affine, rect: Rect) -> Result<Scores, String> {
    if rect.is_empty() || rect.width() > 2048 || rect.height() > 2048 {
        return Err("reference raster must be nonempty and at most 2048x2048".into());
    }
    if view.m.iter().any(|v| !v.is_finite()) || !view.determinant().is_finite() || view.determinant() == 0.0 {
        return Err("reference view must be finite and nonsingular".into());
    }
    let measure = |paths: &[Path]| -> Result<f64, String> { paths.iter().map(|p| vector::area(p, p.fill_rule)).sum() };
    let a = measure(actual)?;
    let b = measure(reference)?;
    if !a.is_finite() || !b.is_finite() || a < 0.0 || b < 0.0 {
        return Err("invalid region area".into());
    }
    let relative_area = if b == 0.0 { if a == 0.0 { 0.0 } else { f64::INFINITY } } else { (a - b).abs() / b };
    let ap = combined(actual);
    let bp = combined(reference);
    // Render regions separately so that evenodd/nonzero rules are not silently replaced.
    let coverage = |paths: &[Path]| -> Result<Vec<f32>, String> {
        let mut mask = vec![0.0f32; rect.width() as usize * rect.height() as usize];
        for p in paths {
            let p = p.transform(&view);
            if !vector::is_finite(&p) {
                return Err("nonfinite reference view".into());
            }
            let values = photocraft_vector::fill_rasterizer(&p, 1e-6).render(rect);
            for (out, v) in mask.iter_mut().zip(values) {
                *out = (*out + v).min(1.0);
            }
        }
        Ok(mask)
    };
    Ok(Scores {
        relative_area,
        hausdorff: vector::hausdorff(&ap, &bp, 64)?,
        iou: coverage_iou(&coverage(actual)?, &coverage(reference)?)?,
        actual_nodes: vector::node_count(&ap),
        reference_nodes: vector::node_count(&bp),
        crossings: actual.iter().map(|p| vector::self_intersections(p).len()).sum(),
    })
}

/// Intersection/union of supersampled coverage areas, retaining fractional boundary
/// coverage. Thresholding at exactly half a pixel turns sub-ulp geometry differences
/// along diagonal boundaries into whole-pixel disagreements.
fn coverage_iou(a: &[f32], b: &[f32]) -> Result<f64, String> {
    if a.len() != b.len() || a.is_empty() || a.iter().chain(b).any(|n| !n.is_finite() || !(0.0..=1.0).contains(n)) {
        return Err("invalid coverage IoU inputs".into());
    }
    let (intersection, union) = a.iter().zip(b).fold((0., 0.), |(i, u), (a, b)| (i + f64::from(a.min(*b)), u + f64::from(a.max(*b))));
    Ok(if union == 0. { 1. } else { intersection / union })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed(String),
    MissingReference(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CorpusSummary {
    pub total: usize,
    pub passed: usize,
    pub ratcheted: usize,
}

/// Fail closed on missing oracles, stale/duplicate/unknown ratchet entries and new failures.
/// A missing external fixture is never an algorithm failure eligible for the ratchet.
pub fn check_ratchet(cases: &[(String, Outcome)], ratchet: &[(String, String)], minimum_pass: f64) -> Result<CorpusSummary, String> {
    if cases.is_empty() || !minimum_pass.is_finite() || !(0.0..=1.0).contains(&minimum_pass) {
        return Err("invalid corpus/pass threshold".into());
    }
    let mut known = BTreeMap::new();
    for (id, outcome) in cases {
        if id.trim().is_empty() || known.insert(id, outcome).is_some() {
            return Err(format!("empty or duplicate corpus id: {id}"));
        }
    }
    let mut listed = BTreeMap::new();
    for (id, reason) in ratchet {
        if reason.trim().is_empty() || listed.insert(id, reason).is_some() || !known.contains_key(id) {
            return Err(format!("invalid/duplicate/unknown ratchet entry: {id}"));
        }
    }
    let missing: Vec<_> =
        cases.iter().filter_map(|(id, o)| if let Outcome::MissingReference(reason) = o { Some(format!("{id}: {reason}")) } else { None }).collect();
    if !missing.is_empty() {
        return Err(format!("incomplete external corpus: {} missing references: {}", missing.len(), missing.join(", ")));
    }
    let mut passed = 0;
    for (id, outcome) in cases {
        match outcome {
            Outcome::Passed => {
                if listed.contains_key(id) {
                    return Err(format!("passing case must be removed from ratchet: {id}"));
                }
                passed += 1;
            }
            Outcome::Failed(reason) => {
                if !listed.contains_key(id) {
                    return Err(format!("new geometry failure: {id}: {reason}"));
                }
            }
            Outcome::MissingReference(_) => return Err("incomplete external corpus".into()),
        }
    }
    if passed as f64 / (cases.len() as f64) < minimum_pass {
        return Err(format!("corpus below pass threshold: {passed}/{} < {minimum_pass}", cases.len()));
    }
    Ok(CorpusSummary { total: cases.len(), passed, ratcheted: listed.len() })
}

#[cfg(test)]
#[path = "../tests/vector_geometry/gates.rs"]
mod tests;

#[cfg(test)]
mod coverage_regression {
    use super::*;
    #[test]
    fn geometry31_coverage_iou_preserves_half_pixel_boundaries_and_detects_loss() -> Result<(), String> {
        assert!(coverage_iou(&[0.5, 0.5], &[0.5 - 1e-7, 0.5 + 1e-7])? > 0.999);
        assert!(coverage_iou(&[0.5, 0.5], &[0.4, 0.6])? < 0.999);
        assert_eq!(coverage_iou(&[0., 0.], &[0., 0.])?, 1.);
        assert!(coverage_iou(&[f32::NAN], &[0.]).is_err());
        Ok(())
    }
}
