//! Cutout/abutting tracing: one fitted boundary shared by adjacent colours.
// Adapted from visioncortex/vtracer crates/vtracer/src/mosaic.rs
// at 928ed0a6f654408e28fb741b6133d4c456bd0160.
// Copyright (c) 2024 TSANG, Hao Fung, vtracer contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/vtracer-LICENSE-MIT and licenses/vtracer-LICENSE-APACHE.
mod compose;
mod face;
mod fit;
mod graph;
mod merge;
mod topology;

use crate::{Error, Image, Params, Trace, ir::Segmentation};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
type RegionId = u32;
const OUTSIDE: RegionId = u32::MAX;

/// Unique lattice edges, counted before graph allocation. No per-region overdraw.
pub const EDGE_BUDGET: usize = 8_000_000;

/// Additive parameter type for package 00/28 routing. Old settings default to T1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layering {
    #[default]
    Stacked,
    Cutout,
}

struct LabelMap {
    width: u32,
    height: u32,
    labels: Vec<RegionId>,
    colors: Vec<[u8; 4]>,
}
impl LabelMap {
    fn label(&self, x: i32, y: i32) -> RegionId {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height { OUTSIDE } else { self.labels[y as usize * self.width as usize + x as usize] }
    }
    fn from_segmentation(seg: &Segmentation, cancel: &dyn Fn() -> bool) -> Result<Self, Error> {
        // The last occurrence of each quantized paint determines its order,
        // exactly as T1 stack_colors. Equal paints never create internal edges.
        let mut last = BTreeMap::new();
        for (i, layer) in seg.layers.iter().enumerate() {
            let c = layer.paint.color();
            last.insert([c.r, c.g, c.b, 255], i);
        }
        let mut ordered: Vec<_> = last.into_iter().collect();
        ordered.sort_by_key(|(_, i)| *i);
        let colors: Vec<_> = ordered.iter().map(|(c, _)| *c).collect();
        let ids: BTreeMap<_, _> = colors.iter().enumerate().map(|(i, c)| (*c, i as RegionId)).collect();
        let mut labels = vec![OUTSIDE; seg.width as usize * seg.height as usize];
        for layer in &seg.layers {
            let c = layer.paint.color();
            let id = ids[&[c.r, c.g, c.b, 255]];
            let mask = &layer.mask;
            for y in 0..mask.height() {
                for x in 0..mask.width() {
                    if x % 1024 == 0 && cancel() {
                        return Err(Error::Cancelled);
                    }
                    let (gx, gy) = (mask.offset.x + x as i32, mask.offset.y + y as i32);
                    if gx >= 0 && gy >= 0 && (gx as u32) < seg.width && (gy as u32) < seg.height && mask.image.get_pixel(x, y) {
                        labels[gy as usize * seg.width as usize + gx as usize] = id;
                    }
                }
            }
        }
        Ok(Self { width: seg.width, height: seg.height, labels, colors })
    }
}

/// Detached evaluation. Existing `crate::trace` remains stacked by default.
pub fn trace(image: Image<'_>, params: &Params) -> Result<Trace, Error> {
    trace_with_cancel(image, params, || false)
}
pub fn trace_cancellable(image: Image<'_>, params: &Params, cancel: &AtomicBool) -> Result<Trace, Error> {
    trace_with_cancel(image, params, || cancel.load(Ordering::Relaxed))
}
/// Engine job callback; does not mutate a document or publish partial output.
pub fn trace_with_cancel(image: Image<'_>, params: &Params, cancel: impl Fn() -> bool) -> Result<Trace, Error> {
    image.validate()?;
    params.validate()?;
    if params.speckle > crate::PIXEL_BUDGET || u64::from(params.watershed_detail) > crate::PIXEL_BUDGET as u64 {
        return Err(Error::InvalidParams("cutout noise/detail exceeds pixel budget"));
    }
    let seg = crate::frontend::segment(image, params, &cancel)?;
    evaluate(&seg, params, EDGE_BUDGET, &cancel)
}

/// Package 00 may route its common T1 segmentation directly here after palette
/// assignment, before `stack_colors`; no second frontend is required then.
pub(crate) fn evaluate(seg: &Segmentation, params: &Params, edge_budget: usize, cancel: &dyn Fn() -> bool) -> Result<Trace, Error> {
    if cancel() {
        return Err(Error::Cancelled);
    }
    let mut map = LabelMap::from_segmentation(seg, cancel)?;
    // Explicit palette colours remain authoritative. Pixel Art's zero layer
    // difference likewise leaves its exact partition untouched.
    if params.palette.is_empty() {
        merge::similar(&mut map, params.layer_difference, cancel)?;
        if params.preset == crate::Preset::Logo && params.fitter != crate::Fitter::Pixel {
            merge::antialias(&mut map, params.layer_difference, cancel)?;
        }
    }
    let graph = graph::BoundaryGraph::extract(&map, edge_budget, cancel)?;
    let faces = face::assemble(&graph, &map, cancel)?;
    let mut fitted = fit::fit_all(&graph, params, cancel)?;
    // Repair whole shared chains, never independent faces; recheck the complete
    // arrangement until the exact fallback and retained fits coexist safely.
    topology::repair(&graph, &mut fitted, edge_budget, params.detail, cancel)?;
    compose::trace(&map, &faces, &fitted, cancel)
}

#[cfg(test)]
#[path = "../tests/cutout/mod.rs"]
mod tests;
