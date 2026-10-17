//! Detached adaptation of VectorCraft pen placement, close and handle-break replay logic.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! MIT OR Apache-2.0, upstream pin d522c1d7be4035bd4f4a84cd6ebfca44f5155092.

use super::{Commit, MAX_COORDINATE, MAX_KNOTS, Target, check_fresh, check_target, error, validate};
use crate::{EngineError, Result, Session};
use photocraft_doc::{Fill, Knot, LayerContent, Path, ShapeStroke, Subpath};
use photocraft_geom::Point;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Pen,
    Smart,
    Polygon,
    Line,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub alt: bool,
    pub shift: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Preview,
    Finished(Box<Commit>),
}

#[derive(Clone, Debug)]
struct Drag {
    knot: usize,
    closing: bool,
    outgoing_only: bool,
    incoming: Point,
    pointer: Point,
    moved: bool,
}

/// One complete, cancellable curve. Releases between nodes never edit the Session.
#[derive(Clone, Debug)]
pub struct Gesture {
    request: Commit,
    original: Path,
    mode: Mode,
    subpath: usize,
    at_first: bool,
    auto: Vec<bool>,
    drag: Option<Drag>,
    hover: Option<Point>,
    generation: u64,
}

fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}
fn point(p: Point) -> Result<Point> {
    if !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > MAX_COORDINATE || p.y.abs() > MAX_COORDINATE {
        return Err(error("pen pointer must be finite and bounded"));
    }
    Ok(p)
}
/// Shared construction constraint, applied before snapping and inside the gesture.
pub fn constrain(origin: Point, p: Point, shift: bool) -> Point {
    if !shift {
        return p;
    }
    let (x, y) = (p.x - origin.x, p.y - origin.y);
    let a = (y.atan2(x) / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
    Point::new(origin.x + x.hypot(y) * a.cos(), origin.y + x.hypot(y) * a.sin())
}

impl Gesture {
    /// Start at an existing open endpoint, or add a detached subpath to the captured target.
    pub fn begin(s: &Session, target: Target, mode: Mode, at: Point, tolerance: f64, fill: Option<Fill>, stroke: Option<ShapeStroke>) -> Result<Self> {
        point(at)?;
        if !tolerance.is_finite() || !(0.0..=10_000.0).contains(&tolerance) {
            return Err(error("invalid pen hit tolerance"));
        }
        let st = s.active().ok_or(EngineError::NoDocument)?;
        check_target(&st.doc, &target)?;
        let original = match target {
            Target::Shape { layer } => match &st.doc.layer(layer).ok_or(EngineError::NoLayer(layer))?.content {
                LayerContent::Shape(sh) => sh.path.clone(),
                _ => return Err(error("pen target is not a curve")),
            },
            Target::Work => st.doc.work_path.clone().unwrap_or_default(),
            Target::VectorMask { layer } => {
                st.doc.layer(layer).and_then(|l| l.vector_mask.as_ref()).map(|m| m.path.clone()).ok_or_else(|| error("pen target has no vector mask"))?
            }
            Target::NewShape { .. } => Path::default(),
        };
        validate(&original)?;
        let endpoint = original
            .subpaths
            .iter()
            .enumerate()
            .filter(|(_, sp)| !sp.closed && !sp.knots.is_empty())
            .flat_map(|(si, sp)| [(si, false, sp.knots[sp.knots.len() - 1].anchor), (si, true, sp.knots[0].anchor)])
            .filter(|(_, _, p)| mode != Mode::Line && distance(*p, at) <= tolerance)
            .min_by(|a, b| distance(a.2, at).total_cmp(&distance(b.2, at)));
        let mut path = original.clone();
        let (subpath, at_first, drag) = if let Some((si, first, _)) = endpoint {
            (si, first, None) // Clicking an endpoint preserves both original handles.
        } else {
            if path.subpaths.iter().map(|sp| sp.knots.len()).sum::<usize>() >= MAX_KNOTS {
                return Err(error("pen knot budget exceeded"));
            }
            let si = path.subpaths.len();
            path.subpaths.push(Subpath::polyline(&[(at.x, at.y)]));
            (si, false, Some(Drag { knot: 0, closing: false, outgoing_only: false, incoming: at, pointer: at, moved: false }))
        };
        let mut auto = vec![false; path.subpaths[subpath].knots.len()];
        if endpoint.is_none() && mode == Mode::Smart {
            auto[0] = true;
        }
        Ok(Self {
            request: Commit { document: st.doc.id.0, revision: st.revision, target, path, fill, stroke },
            original,
            mode,
            subpath,
            at_first,
            auto,
            drag,
            hover: Some(at),
            generation: 1,
        })
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn path(&self) -> &Path {
        &self.request.path
    }
    pub fn request(&self) -> &Commit {
        &self.request
    }
    pub fn captured(&self) -> bool {
        self.drag.is_some()
    }
    pub fn is_fresh(&self, s: &Session) -> bool {
        check_fresh(s, self.request.document, self.request.revision).is_ok()
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Snapping uses this origin along the same 45-degree construction ray.
    pub fn constraint_origin(&self) -> Point {
        let index = if self.mode == Mode::Line { 0 } else { self.drag.as_ref().map_or_else(|| self.end(), |d| d.knot) };
        self.request.path.subpaths[self.subpath].knots[index].anchor
    }

    fn end(&self) -> usize {
        if self.at_first { 0 } else { self.request.path.subpaths[self.subpath].knots.len() - 1 }
    }

    pub fn press(&mut self, at: Point, mods: Modifiers, tolerance: f64) -> Result<()> {
        let at = point(at)?;
        if !tolerance.is_finite() || !(0.0..=10_000.0).contains(&tolerance) {
            return Err(error("invalid pen hit tolerance"));
        }
        if self.drag.is_some() {
            return Err(error("pen already owns the pointer"));
        }
        let end = self.end();
        let sp = &mut self.request.path.subpaths[self.subpath];
        let opposite = if self.at_first { sp.knots.len() - 1 } else { 0 };
        if self.mode != Mode::Line && sp.knots.len() >= 2 && distance(at, sp.knots[opposite].anchor) <= tolerance {
            sp.closed = true;
            self.drag = Some(Drag {
                knot: opposite,
                closing: true,
                outgoing_only: false,
                incoming: if self.at_first { sp.knots[opposite].in_ctrl } else { sp.knots[opposite].out_ctrl },
                pointer: at,
                moved: false,
            });
            self.smart_handles();
        } else if distance(at, sp.knots[end].anchor) <= tolerance {
            let k = &mut sp.knots[end];
            if self.at_first {
                k.in_ctrl = k.anchor;
            } else {
                k.out_ctrl = k.anchor;
            }
            k.smooth = false;
            self.auto[end] = false;
            self.drag = Some(Drag {
                knot: end,
                closing: false,
                outgoing_only: true,
                incoming: if self.at_first { k.out_ctrl } else { k.in_ctrl },
                pointer: at,
                moved: false,
            });
        } else {
            if self.request.path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() >= MAX_KNOTS {
                return Err(error("pen node budget exhausted"));
            }
            let sp = &mut self.request.path.subpaths[self.subpath];
            let at = point(constrain(sp.knots[end].anchor, at, mods.shift))?;
            let k = Knot::corner(at.x, at.y);
            let index = if self.at_first {
                sp.knots.insert(0, k);
                self.auto.insert(0, self.mode == Mode::Smart);
                0
            } else {
                sp.knots.push(k);
                self.auto.push(self.mode == Mode::Smart);
                sp.knots.len() - 1
            };
            self.drag = Some(Drag { knot: index, closing: false, outgoing_only: false, incoming: at, pointer: at, moved: false });
            self.smart_handles();
        }
        self.hover = Some(at);
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// Modifiers are sampled on each move, including changes while the pointer is stationary.
    pub fn motion(&mut self, at: Point, mods: Modifiers) -> Result<()> {
        let at = point(at)?;
        self.hover = Some(at);
        let Some(drag) = &mut self.drag else { return Ok(()) };
        if self.mode == Mode::Line {
            let count: usize = self.request.path.subpaths.iter().map(|s| s.knots.len()).sum();
            let sp = &mut self.request.path.subpaths[self.subpath];
            let origin = sp.knots[0].anchor;
            let at = point(constrain(origin, at, mods.shift))?;
            let k = Knot::corner(at.x, at.y);
            if sp.knots.len() == 2 && sp.knots[1] == k {
                return Ok(());
            }
            self.generation = self.generation.wrapping_add(1);
            if sp.knots.len() == 1 && distance(at, origin) > 1e-9 {
                if count >= MAX_KNOTS {
                    return Err(error("pen node budget exhausted"));
                }
                sp.knots.push(k);
                self.auto.push(false);
            } else if sp.knots.len() == 2 {
                sp.knots[1] = k;
            }
            return Ok(());
        }
        if self.mode == Mode::Polygon {
            return Ok(());
        }
        let k = &mut self.request.path.subpaths[self.subpath].knots[drag.knot];
        if !drag.moved && distance(drag.pointer, at) < 1e-9 {
            return Ok(());
        }
        let previous = *k;
        drag.moved = true;
        let out = point(constrain(k.anchor, at, mods.shift))?;
        let reflected = point(Point::new(2.0 * k.anchor.x - out.x, 2.0 * k.anchor.y - out.y))?;
        // Closing drags shape the arriving handle; Alt preserves the first node's
        // outgoing handle so the already drawn first segment does not change.
        if drag.closing {
            if self.at_first {
                k.out_ctrl = reflected;
                if !mods.alt {
                    k.in_ctrl = out;
                }
            } else {
                k.in_ctrl = reflected;
                if !mods.alt {
                    k.out_ctrl = out;
                }
            }
        } else {
            let incoming = if mods.alt || drag.outgoing_only { drag.incoming } else { reflected };
            drag.incoming = incoming;
            if self.at_first {
                k.in_ctrl = out;
                k.out_ctrl = incoming;
            } else {
                k.out_ctrl = out;
                k.in_ctrl = incoming;
            }
        }
        k.smooth = !mods.alt && !drag.outgoing_only;
        self.auto[drag.knot] = false;
        if *k != previous {
            self.generation = self.generation.wrapping_add(1);
        }
        Ok(())
    }

    pub fn release(&mut self) -> Result<Outcome> {
        let Some(drag) = self.drag.take() else { return Ok(Outcome::Preview) };
        if self.mode == Mode::Line {
            let sp = &mut self.request.path.subpaths[self.subpath];
            if sp.knots.len() == 2 && distance(sp.knots[0].anchor, sp.knots[1].anchor) < 1e-9 {
                sp.knots.truncate(1);
                self.auto.truncate(1);
                self.generation = self.generation.wrapping_add(1);
                return Ok(Outcome::Preview);
            }
        }
        if drag.closing || (self.mode == Mode::Line && self.request.path != self.original && self.request.path.subpaths[self.subpath].knots.len() >= 2) {
            return self.finish().map(|c| Outcome::Finished(Box::new(c)));
        }
        Ok(Outcome::Preview)
    }

    /// Release mouse capture without abandoning the anchors already placed.
    pub fn suspend(&mut self) {
        self.drag = None;
    }
    /// Remove one newly placed anchor; a continuation never removes the original curve.
    pub fn undo_anchor(&mut self) -> bool {
        self.drag = None;
        let original = self.original.subpaths.get(self.subpath).map_or(0, |s| s.knots.len());
        let subpath = &mut self.request.path.subpaths[self.subpath];
        if subpath.knots.len() <= original.max(1) {
            return false;
        }
        let index = if self.at_first { 0 } else { subpath.knots.len() - 1 };
        subpath.knots.remove(index);
        self.auto.remove(index);
        subpath.closed = false;
        self.smart_handles();
        self.generation = self.generation.wrapping_add(1);
        true
    }
    pub fn finish(&self) -> Result<Commit> {
        validate(&self.request.path)?;
        if self.request.path == self.original || self.request.path.subpaths[self.subpath].knots.len() < 2 {
            return Err(error("no finished pen curve"));
        }
        Ok(self.request.clone())
    }

    /// Rubber band is geometry only; it cannot leak into the committed Path.
    pub fn rubber_band(&self, mods: Modifiers) -> Option<Subpath> {
        if self.drag.is_some() {
            return None;
        }
        let k = self.request.path.subpaths[self.subpath].knots[self.end()];
        let p = constrain(k.anchor, self.hover?, mods.shift);
        if point(p).is_err() {
            return None;
        }
        let mut end = Knot::corner(p.x, p.y);
        if self.mode == Mode::Smart {
            end.in_ctrl = Point::new(p.x + (k.anchor.x - p.x) / 3.0, p.y + (k.anchor.y - p.y) / 3.0);
        }
        let mut start = k;
        if self.at_first {
            std::mem::swap(&mut start.in_ctrl, &mut start.out_ctrl);
        }
        Some(Subpath { knots: vec![start, end], ..Default::default() })
    }

    fn smart_handles(&mut self) {
        if self.mode != Mode::Smart {
            return;
        }
        // Uniform Catmull–Rom -> cubic Bézier tangents. Only automatically placed
        // nodes are refitted; continued endpoints and explicitly dragged nodes stay exact.
        let sp = &mut self.request.path.subpaths[self.subpath];
        let anchors: Vec<_> = sp.knots.iter().map(|k| k.anchor).collect();
        let n = anchors.len();
        for (i, k) in sp.knots.iter_mut().enumerate() {
            if !self.auto[i] || n < 2 {
                continue;
            }
            let p = if i > 0 {
                anchors[i - 1]
            } else if sp.closed {
                anchors[n - 1]
            } else {
                anchors[i]
            };
            let q = if i + 1 < n {
                anchors[i + 1]
            } else if sp.closed {
                anchors[0]
            } else {
                anchors[i]
            };
            let factor = if !sp.closed && (i == 0 || i + 1 == n) { 3.0 } else { 6.0 };
            let (dx, dy) = ((q.x - p.x) / factor, (q.y - p.y) / factor);
            k.in_ctrl = Point::new(k.anchor.x - dx, k.anchor.y - dy);
            k.out_ctrl = Point::new(k.anchor.x + dx, k.anchor.y + dy);
            k.smooth = true;
        }
    }
}
