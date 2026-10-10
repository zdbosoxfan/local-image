//! Open stroked centre lines, adapted from VectorCraft `crates/trace/src/centerline.rs`
//! at d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
// Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
// Exact Euclidean distances replace upstream's chamfer/area width estimates.

mod distance;
mod graph;
mod mask;
mod skeleton;

use crate::{Error, Image, Params, Preset};
use photocraft_doc::vector_design::appearance::WidthProfile;
use photocraft_doc::{Path, ShapeStroke};
use serde::{Deserialize, Serialize};

/// An individual graph run. A loop repeats its first endpoint, remaining an open
/// subpath; separate runs have separate profiles measured along their own arc length.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrokePath {
    pub path: Path,
    pub stroke: ShapeStroke,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<WidthProfile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CenterlineTrace {
    pub width: u32,
    pub height: u32,
    pub strokes: Vec<StrokePath>,
    pub nodes: usize,
}

/// Bounds work, including successive thinning passes, independently of pixels.
const WORK_BUDGET: usize = 128_000_000;
const STROKE_BUDGET: usize = 8192;
const NODE_BUDGET: usize = 1_048_576;

struct Control<'a, F> {
    cancel: &'a F,
    remaining: usize,
}
impl<F: Fn() -> bool> Control<'_, F> {
    fn poll(&self) -> Result<(), Error> {
        if (self.cancel)() { Err(Error::Cancelled) } else { Ok(()) }
    }
    fn work(&mut self, n: usize) -> Result<(), Error> {
        self.poll()?;
        self.remaining = self.remaining.checked_sub(n).ok_or(Error::BoundaryBudget)?;
        Ok(())
    }
}

/// Package evaluator: no document mutation, thread pool, pathops or renderer dependency.
/// Routing in T1 calls this after mode validation. The cloned fill preset below
/// validates existing numeric fields on the pre-integration T1 baseline too.
pub fn trace_with_cancel(image: Image<'_>, params: &Params, cancel: impl Fn() -> bool) -> Result<CenterlineTrace, Error> {
    image.validate()?;
    let mut checked = params.clone();
    checked.preset = Preset::BlackWhite;
    checked.validate()?;
    let mut control = Control { cancel: &cancel, remaining: WORK_BUDGET };
    control.poll()?;
    let (w, h) = (image.width as usize, image.height as usize);
    let foreground = mask::foreground(image, params, &mut control)?;
    let distances = distance::squared(&foreground, w, h, &mut control)?;
    let mut strokes = Vec::new();
    let mut seen = vec![false; foreground.len()];
    let mut stack = Vec::new();
    let mut nodes = 0;
    let mut profile_points = 0;
    for start in 0..foreground.len() {
        if start.is_multiple_of(1024) {
            control.work(1024)?;
        }
        if !foreground[start] || seen[start] {
            continue;
        }
        let mut component = Vec::new();
        stack.push(start);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            if component.len().is_multiple_of(1024) {
                control.work(1024)?;
            }
            component.push(i);
            for j in skeleton::neighbours(i, w, h) {
                if foreground[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        if component.len() < params.speckle {
            continue;
        }
        let mut grid = skeleton::Grid::new(&component, w, &mut control)?;
        grid.thin(&mut control)?;
        let runs = graph::runs(&grid, &distances, w, &mut control)?;
        for run in runs {
            control.poll()?;
            let stroke = graph::fit(run, params.detail, &mut control)?;
            nodes += stroke.path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
            profile_points += stroke.profile.as_ref().map_or(0, |p| p.points.len());
            if nodes > NODE_BUDGET || profile_points > NODE_BUDGET || strokes.len() == STROKE_BUDGET {
                return Err(Error::BoundaryBudget);
            }
            strokes.push(stroke);
        }
    }
    control.poll()?;
    Ok(CenterlineTrace { width: image.width, height: image.height, strokes, nodes })
}

pub fn trace(image: Image<'_>, params: &Params) -> Result<CenterlineTrace, Error> {
    trace_with_cancel(image, params, || false)
}

#[cfg(test)]
#[path = "../tests/centerline/algorithm.rs"]
mod tests;
