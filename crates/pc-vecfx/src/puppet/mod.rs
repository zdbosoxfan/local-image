//! Vector Puppet Warp using VectorCraft's two-stage ARAP mesh, with checked inputs and V1 refit.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0; see arap.rs and licenses/vectorcraft-NOTICE.
pub mod arap;
use crate::liquify::{MAX_NODES, Result, valid_point, validate_path};
use kurbo::{CubicBez, ParamCurve, Point, Rect};
use photocraft_doc::{Knot, Path, Subpath};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub resolution: usize,
    pub expand: f64,
    pub tolerance: f64,
    pub node_budget: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self { resolution: 20, expand: 3.0, tolerance: 0.15, node_budget: MAX_NODES }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        if !(4..=32).contains(&self.resolution)
            || !(4..=MAX_NODES).contains(&self.node_budget)
            || !self.expand.is_finite()
            || !(0.0..=1000.0).contains(&self.expand)
            || !self.tolerance.is_finite()
            || !(0.001..=10.0).contains(&self.tolerance)
        {
            return Err("invalid Puppet Warp mesh options".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub rest: [f64; 2],
    pub at: [f64; 2],
    #[serde(default)]
    pub angle: Option<f64>,
}
impl Pin {
    pub fn new(at: [f64; 2]) -> Self {
        Self { rest: at, at, angle: None }
    }
    fn solver(self) -> arap::Pin {
        arap::Pin { rest: Point::new(self.rest[0], self.rest[1]), target: Point::new(self.at[0], self.at[1]), angle: self.angle }
    }
}
#[derive(Clone, Debug)]
pub struct Rig {
    pub mesh: arap::Mesh,
    pub options: Options,
}
impl Rig {
    /// `paths` are evaluated base paths. Ordered compound fills are normalized only for mesh
    /// construction; editing preserves the original topology, operations and fill rules.
    pub fn new(paths: &[Path], options: Options) -> Result<Self> {
        options.validate()?;
        if paths.is_empty() || paths.len() > 64 {
            return Err("Puppet Warp needs 1..64 paths".into());
        }
        let mut outlines = Vec::new();
        let mut bounds: Option<Rect> = None;
        for path in paths {
            validate_path(path, options.node_budget)?;
            if let Some((x0, y0, x1, y1)) = photocraft_pathops::bounds(path).map_err(|e| e.to_string())? {
                let b = Rect::new(x0, y0, x1, y1);
                bounds = Some(bounds.map_or(b, |old| old.union(b)));
            }
            let mut closed = path.clone();
            closed.subpaths.retain(|s| s.closed);
            if !closed.subpaths.is_empty() {
                if closed.inverted {
                    return Err("expand inverted artwork before Puppet Warp".into());
                }
                let normalized = photocraft_pathops::finish_compound(&closed, None).map_err(|e| e.to_string())?;
                outlines.push(photocraft_pathops::to_geometry(&normalized).to_bezpath());
            }
            let mut open = path.clone();
            open.subpaths.retain(|s| !s.closed);
            if !open.subpaths.is_empty() {
                outlines.push(photocraft_pathops::to_geometry(&open).to_bezpath());
            }
        }
        let bounds = bounds.ok_or_else(|| "Puppet Warp needs nonempty artwork".to_string())?;
        let mesh = arap::Mesh::build(&outlines, bounds, arap::MeshOptions { resolution: options.resolution, expand: options.expand });
        if mesh.verts.is_empty() || mesh.verts.len() > 1500 {
            return Err("invalid Puppet Warp mesh".into());
        }
        Ok(Self { mesh, options })
    }
    pub fn automatic_pins(&self) -> Vec<Pin> {
        arap::default_pins(&self.mesh).into_iter().map(|p| Pin::new([p.x, p.y])).collect()
    }
    pub fn validate_pins(&self, pins: &[Pin]) -> Result<()> {
        if pins.len() > 64 {
            return Err("Puppet Warp exceeds pin budget".into());
        }
        for (i, p) in pins.iter().enumerate() {
            valid_point(p.rest)?;
            valid_point(p.at)?;
            if p.angle.is_some_and(|a| !a.is_finite() || a.abs() > std::f64::consts::TAU * 4.0) {
                return Err("invalid Puppet Warp pin angle".into());
            }
            if !self.mesh.contains(Point::new(p.rest[0], p.rest[1])) {
                return Err("Pins go on the artwork".into());
            }
            if pins[..i].iter().any(|q| Point::new(q.rest[0], q.rest[1]).distance(Point::new(p.rest[0], p.rest[1])) < 1e-6) {
                return Err("duplicate Puppet Warp pin".into());
            }
        }
        Ok(())
    }
    pub fn deform(&self, pins: &[Pin]) -> Result<Vec<Point>> {
        self.validate_pins(pins)?;
        // Avoid soft-constraint roundoff for identity and exact common translation.
        if let Some(delta) = translation(pins) {
            return Ok(self.mesh.verts.iter().map(|p| *p + delta).collect());
        }
        let out = arap::deform(&self.mesh, &pins.iter().copied().map(Pin::solver).collect::<Vec<_>>());
        for p in &out {
            valid_point([p.x, p.y])?;
        }
        if out.len() != self.mesh.verts.len() {
            return Err("Puppet Warp solve failed".into());
        }
        Ok(out)
    }
    pub fn warp(&self, path: &Path, pins: &[Pin], deformed: &[Point]) -> Result<Path> {
        validate_path(path, self.options.node_budget)?;
        self.validate_pins(pins)?;
        if let Some(delta) = translation(pins) {
            let mut out = path.clone();
            for k in out.subpaths.iter_mut().flat_map(|s| &mut s.knots) {
                for p in [&mut k.anchor, &mut k.in_ctrl, &mut k.out_ctrl] {
                    p.x += delta.x;
                    p.y += delta.y;
                }
            }
            validate_path(&out, self.options.node_budget)?;
            return Ok(out);
        }
        if deformed.len() != self.mesh.verts.len() {
            return Err("invalid deformed mesh".into());
        }
        for p in deformed {
            valid_point([p.x, p.y])?;
        }
        let mut out = Path { subpaths: Vec::new(), ..path.clone() };
        let mut count = 0;
        for sp in &path.subpaths {
            let mut knots: Vec<Knot> = Vec::new();
            // Fit each original segment separately to keep cusps and subpath seams.
            for (segment_index, segment) in sp.segments().into_iter().enumerate() {
                let c = CubicBez::new((segment[0].x, segment[0].y), (segment[1].x, segment[1].y), (segment[2].x, segment[2].y), (segment[3].x, segment[3].y));
                let length = c.p0.distance(c.p1) + c.p1.distance(c.p2) + c.p2.distance(c.p3);
                let spacing = (length / 64.0).max(self.options.tolerance).max(0.001);
                let n = ((length / spacing).ceil() as usize).clamp(4, 128);
                let points: Vec<_> = (0..=n)
                    .map(|i| {
                        let p = self.mesh.map(deformed, c.eval(i as f64 / n as f64));
                        photocraft_geom::Point::new(p.x, p.y)
                    })
                    .collect();
                let fitted = photocraft_pathops::fit_path(&points, self.options.tolerance).map_err(|e| e.to_string())?;
                let part = fitted.subpaths.first().ok_or_else(|| "Puppet Warp refit failed".to_string())?;
                if let Some(last) = knots.last_mut() {
                    if let Some(first) = part.knots.first() {
                        last.out_ctrl = first.out_ctrl;
                    }
                    knots.extend(part.knots.iter().skip(1).copied());
                } else {
                    knots.extend(part.knots.iter().copied());
                }
                if let Some(last) = knots.last_mut() {
                    last.smooth = sp.knots[(segment_index + 1) % sp.knots.len()].smooth;
                }
                if let (Some(first), Some(source)) = (knots.first_mut(), sp.knots.first()) {
                    first.smooth = source.smooth;
                }
                if count + knots.len() > self.options.node_budget + 1 {
                    return Err("Puppet Warp refit exceeds node budget".into());
                }
            }
            if sp.closed
                && knots.len() > 1
                && let Some(last) = knots.pop()
            {
                knots[0].in_ctrl = last.in_ctrl;
            }
            if sp.knots.len() < 2 {
                knots = sp
                    .knots
                    .iter()
                    .map(|k| {
                        let map = |p: photocraft_geom::Point| {
                            let q = self.mesh.map(deformed, Point::new(p.x, p.y));
                            photocraft_geom::Point::new(q.x, q.y)
                        };
                        Knot { anchor: map(k.anchor), in_ctrl: map(k.in_ctrl), out_ctrl: map(k.out_ctrl), smooth: k.smooth }
                    })
                    .collect();
            }
            count += knots.len();
            out.subpaths.push(Subpath { knots, ..sp.clone() });
        }
        validate_path(&out, self.options.node_budget)?;
        Ok(out)
    }
}
fn translation(pins: &[Pin]) -> Option<kurbo::Vec2> {
    let Some(first) = pins.first() else {
        return Some(kurbo::Vec2::ZERO);
    };
    let d = kurbo::Vec2::new(first.at[0] - first.rest[0], first.at[1] - first.rest[1]);
    pins.iter().all(|p| p.angle.is_none_or(|a| a == 0.0) && (p.at[0] - p.rest[0] - d.x).abs() < 1e-12 && (p.at[1] - p.rest[1] - d.y).abs() < 1e-12).then_some(d)
}
#[cfg(test)]
#[path = "../../tests/puppet/tests.rs"]
mod tests;
