//! Compose colour paths by copying oriented shared controls.
// Adapted from visioncortex/vtracer crates/vtracer/src/mosaic/compose.rs
// at 928ed0a6f654408e28fb741b6133d4c456bd0160.
// Copyright (c) 2024 TSANG, Hao Fung, vtracer contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/vtracer-LICENSE-MIT and licenses/vtracer-LICENSE-APACHE.
use super::{LabelMap, face::Faces, fit::Geometry};
use crate::{ColorPath, Error, Trace};
use kurbo::{BezPath, ParamCurve, PathSeg};
fn emit(b: &mut BezPath, seg: PathSeg) {
    match seg {
        PathSeg::Line(l) => b.line_to(l.p1),
        PathSeg::Quad(q) => b.quad_to(q.p1, q.p2),
        PathSeg::Cubic(c) => b.curve_to(c.p1, c.p2, c.p3),
    }
}
pub(super) fn trace(map: &LabelMap, faces: &Faces, fitted: &[Geometry], cancel: &dyn Fn() -> bool) -> Result<Trace, Error> {
    let mut layers = Vec::new();
    for (&region, contours) in faces {
        let mut b = BezPath::new();
        for contour in contours {
            let mut first = true;
            for r in contour {
                if cancel() {
                    return Err(Error::Cancelled);
                }
                let geom = &fitted[r.seg as usize];
                for i in 0..geom.len() {
                    let s = if r.forward { geom[i] } else { geom[geom.len() - 1 - i].reverse() };
                    if first {
                        b.move_to(s.start());
                        first = false;
                    }
                    emit(&mut b, s);
                }
            }
            b.close_path();
        }
        let path = crate::adapter::bezpath_to_path(&b)?;
        if !path.is_empty() {
            layers.push(ColorPath { color: map.colors[region as usize], path });
        }
    }
    if cancel() {
        return Err(Error::Cancelled);
    }
    let nodes = layers.iter().flat_map(|l| &l.path.subpaths).map(|s| s.knots.len()).sum();
    Ok(Trace { width: map.width, height: map.height, layers, nodes })
}
