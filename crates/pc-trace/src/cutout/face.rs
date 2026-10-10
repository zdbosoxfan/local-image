//! Region contours assembled from oriented shared chains.
// Adapted from visioncortex/vtracer crates/vtracer/src/mosaic/face.rs
// at 928ed0a6f654408e28fb741b6133d4c456bd0160.
// Copyright (c) 2024 TSANG, Hao Fung, vtracer contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/vtracer-LICENSE-MIT and licenses/vtracer-LICENSE-APACHE.
use super::graph::{BoundaryGraph, SegRef, edge_present, left_pixel_at, reverse, straight, turn_left, turn_right};
use super::{LabelMap, OUTSIDE, RegionId};
use crate::Error;
use std::collections::BTreeMap;

pub(super) type Faces = BTreeMap<RegionId, Vec<Vec<SegRef>>>;

pub(super) fn assemble(graph: &BoundaryGraph, map: &LabelMap, cancel: &dyn Fn() -> bool) -> Result<Faces, Error> {
    let mut faces = Faces::new();
    let mut used = vec![[false; 2]; graph.segments.len()];
    for (id, seg) in graph.segments.iter().enumerate() {
        if id % 1024 == 0 && cancel() {
            return Err(Error::Cancelled);
        }
        for (region, forward) in [(seg.left, true), (seg.right, false)] {
            if region == OUTSIDE || used[id][forward as usize] {
                continue;
            }
            let start = SegRef { seg: id as u32, forward };
            if seg.is_ring() {
                faces.entry(region).or_default().push(vec![start]);
                continue;
            }
            let mut contour = Vec::new();
            let mut cur = start;
            loop {
                if contour.len() % 1024 == 0 && cancel() {
                    return Err(Error::Cancelled);
                }
                let seen = &mut used[cur.seg as usize][cur.forward as usize];
                if *seen {
                    return Err(Error::InvalidParams("cutout contour does not close"));
                }
                *seen = true;
                contour.push(cur);
                let seg = &graph.segments[cur.seg as usize];
                let (node, incoming) = if cur.forward { (seg.end, seg.last_dir) } else { (seg.start, reverse(seg.first_dir)) };
                let node = &graph.nodes[node.ok_or(Error::InvalidParams("cutout contour junction missing"))? as usize];
                // Unlike upstream's pinched figure-eight, take the turn within
                // the current quadrant first: diagonal lobes stay simple rings.
                let dir = [turn_left(incoming), straight(incoming), turn_right(incoming)]
                    .into_iter()
                    .find(|&d| edge_present(map, node.corner.x, node.corner.y, d) && left_pixel_at(map, node.corner.x, node.corner.y, d) == region)
                    .ok_or(Error::InvalidParams("cutout contour successor missing"))?;
                cur = node.out[dir as usize].ok_or(Error::InvalidParams("cutout outgoing chain missing"))?;
                if cur == start {
                    break;
                }
            }
            faces.entry(region).or_default().push(contour);
        }
    }
    Ok(faces)
}
