//! Point-level path editing, Photoshop's Direct Selection and Convert Point tools: move anchors,
//! drag direction handles, reshape a segment by dragging it, convert between smooth and corner
//! points, and hit-test a path's parts. Pure geometry on [`Path`]; the engine's `path.*` edit
//! commands and the canvas previews share it, so a preview is exactly what the command commits.
//!
//! Node deletion/conversion is adapted from VectorCraft crates/pathops/src/edit.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//!
//! Knots are addressed as `[subpath, knot]`. Every edit validates its indices and returns an
//! error message instead of panicking.

use photocraft_doc::{Knot, Path};
use photocraft_geom::Point;

/// One of a knot's two direction handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// The control point of the segment arriving at the knot.
    In,
    /// The control point of the segment leaving the knot.
    Out,
}

impl Handle {
    pub fn parse(s: &str) -> Option<Handle> {
        match s {
            "in" => Some(Handle::In),
            "out" => Some(Handle::Out),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Handle::In => "in",
            Handle::Out => "out",
        }
    }
}

/// The part of a path under the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Anchor([usize; 2]),
    Handle([usize; 2], Handle),
    /// The segment leaving knot `[subpath, knot]`, at curve parameter `t`.
    Segment([usize; 2], f64),
}

fn add(p: Point, d: [f64; 2]) -> Point {
    Point::new(p.x + d[0], p.y + d[1])
}

fn dist(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn knot_mut(path: &mut Path, [s, k]: [usize; 2]) -> Result<&mut Knot, String> {
    path.subpaths.get_mut(s).and_then(|sp| sp.knots.get_mut(k)).ok_or_else(|| format!("no knot {k} in subpath {s}"))
}

/// The knot ending the segment that leaves `[s, k]` (wrapping on closed subpaths).
fn segment_end(path: &Path, [s, k]: [usize; 2]) -> Result<[usize; 2], String> {
    let sp = path.subpaths.get(s).ok_or_else(|| format!("no subpath {s}"))?;
    let n = sp.knots.len();
    match k.checked_add(1) {
        Some(next) if k < n && next < n => Ok([s, next]),
        Some(_) if k < n && sp.closed && n > 1 => Ok([s, 0]),
        _ => Err(format!("no segment leaves knot {k} of subpath {s}")),
    }
}

fn handle_mut(k: &mut Knot, h: Handle) -> &mut Point {
    match h {
        Handle::In => &mut k.in_ctrl,
        Handle::Out => &mut k.out_ctrl,
    }
}

fn handle(k: &Knot, h: Handle) -> Point {
    match h {
        Handle::In => k.in_ctrl,
        Handle::Out => k.out_ctrl,
    }
}

fn other(h: Handle) -> Handle {
    match h {
        Handle::In => Handle::Out,
        Handle::Out => Handle::In,
    }
}

/// A smooth knot keeps its handles collinear: point handle `h`'s opposite away from it, keeping
/// the opposite's length (a retracted handle stays retracted, as in Photoshop).
fn keep_smooth(k: &mut Knot, h: Handle) {
    if !k.smooth {
        return;
    }
    let (a, moved) = (k.anchor, handle(k, h));
    let opp = handle_mut(k, other(h));
    let (len, d) = (dist(*opp, a), dist(moved, a));
    if len > 1e-9 && d > 1e-9 {
        *opp = Point::new(a.x - (moved.x - a.x) / d * len, a.y - (moved.y - a.y) / d * len);
    }
}

/// Moves anchors (with their handles) by `d`. Each knot moves once, however often it is listed.
fn move_anchors_unchecked(path: &mut Path, anchors: &[[usize; 2]], d: [f64; 2]) -> Result<(), String> {
    let mut list = anchors.to_vec();
    list.sort_unstable();
    list.dedup();
    for r in &list {
        knot_mut(path, *r)?;
    }
    for r in list {
        let k = knot_mut(path, r)?;
        *k = Knot { anchor: add(k.anchor, d), in_ctrl: add(k.in_ctrl, d), out_ctrl: add(k.out_ctrl, d), smooth: k.smooth };
    }
    Ok(())
}

/// Puts handle `h` of a knot at `to`. A smooth knot turns its other handle to stay collinear,
/// unless `independent` (Convert Point / ⌥), which makes it a corner whose handles move apart.
fn move_handle_unchecked(path: &mut Path, knot: [usize; 2], h: Handle, to: Point, independent: bool) -> Result<(), String> {
    let k = knot_mut(path, knot)?;
    *handle_mut(k, h) = to;
    if independent {
        k.smooth = false;
    }
    keep_smooth(k, h);
    Ok(())
}

/// Convert Point: without `out`, a corner with retracted handles (a click on an anchor); with
/// `out`, a smooth knot whose handles are `out` and its mirror (a drag from an anchor).
fn convert_point_unchecked(path: &mut Path, knot: [usize; 2], out: Option<Point>) -> Result<(), String> {
    let k = knot_mut(path, knot)?;
    let a = k.anchor;
    *k = match out {
        Some(o) => Knot::smooth(a, Point::new(2.0 * a.x - o.x, 2.0 * a.y - o.y), o),
        None => Knot::corner(a.x, a.y),
    };
    Ok(())
}

/// Drags the segment leaving `knot` at curve parameter `t` by `d`. A straight segment (both
/// handles retracted) moves with its two anchors; a curve reshapes so the point at `t` follows
/// the pointer exactly, with the nearer handle moving more. Smooth end knots stay smooth.
fn bend_segment_unchecked(path: &mut Path, knot: [usize; 2], t: f64, d: [f64; 2]) -> Result<(), String> {
    let end = segment_end(path, knot)?;
    let retracted = |k: &Knot, h: Handle| dist(handle(k, h), k.anchor) < 1e-9;
    let a = *knot_mut(path, knot)?;
    let b = *knot_mut(path, end)?;
    if retracted(&a, Handle::Out) && retracted(&b, Handle::In) {
        return move_anchors(path, &[knot, end], d);
    }
    // B(t) moves by 3(1−t)²t·Δc1 + 3(1−t)t²·Δc2. With Δc1 = (1−t)·s·d and Δc2 = t·s·d that is
    // 3t(1−t)((1−t)² + t²)·s·d, so s below makes it exactly d. The ends are clamped: a drag right
    // at an anchor would need an unbounded handle move.
    let t = if t.is_finite() { t.clamp(0.05, 0.95) } else { 0.5 };
    let u = 1.0 - t;
    let s = 1.0 / (3.0 * t * u * (u * u + t * t));
    let (w1, w2) = (u * s, t * s);
    let ka = knot_mut(path, knot)?;
    ka.out_ctrl = add(ka.out_ctrl, [d[0] * w1, d[1] * w1]);
    keep_smooth(ka, Handle::Out);
    let kb = knot_mut(path, end)?;
    kb.in_ctrl = add(kb.in_ctrl, [d[0] * w2, d[1] * w2]);
    keep_smooth(kb, Handle::In);
    Ok(())
}

/// Every anchor of subpath `s` (⌥-click with Direct Selection selects the whole subpath).
pub fn subpath_anchors(path: &Path, s: usize) -> Vec<[usize; 2]> {
    path.subpaths.get(s).map_or_else(Vec::new, |sp| (0..sp.knots.len()).map(|k| [s, k]).collect())
}

/// Anchors inside the box `[x0, y0, x1, y1]` (a Direct Selection marquee), in path order.
pub fn anchors_in(path: &Path, [x0, y0, x1, y1]: [f64; 4]) -> Vec<[usize; 2]> {
    let (x0, x1, y0, y1) = (x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1));
    let mut out = Vec::new();
    for (s, sp) in path.subpaths.iter().enumerate() {
        for (k, kn) in sp.knots.iter().enumerate() {
            if (x0..=x1).contains(&kn.anchor.x) && (y0..=y1).contains(&kn.anchor.y) {
                out.push([s, k]);
            }
        }
    }
    out
}

fn eval(seg: &[Point; 4], t: f64) -> Point {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    Point::new(a * seg[0].x + b * seg[1].x + c * seg[2].x + d * seg[3].x, a * seg[0].y + b * seg[1].y + c * seg[2].y + d * seg[3].y)
}

/// The part of `path` within `tol` of `p`: one of the `handles` shown (they sit on top), else an
/// anchor, else a segment; the nearest wins within each kind.
pub fn hit(path: &Path, p: Point, tol: f64, handles: &[([usize; 2], Handle)]) -> Option<Hit> {
    let nearest = |it: &mut dyn Iterator<Item = (f64, Hit)>| it.filter(|(d, _)| *d <= tol).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, h)| h);
    let knot = |r: [usize; 2]| path.subpaths.get(r[0]).and_then(|sp| sp.knots.get(r[1]));
    let on_handle = nearest(&mut handles.iter().filter_map(|&(r, h)| {
        let k = knot(r)?;
        let d = dist(handle(k, h), p);
        (d < dist(k.anchor, p)).then_some((d, Hit::Handle(r, h)))
    }));
    if on_handle.is_some() {
        return on_handle;
    }
    let anchors =
        path.subpaths.iter().enumerate().flat_map(|(s, sp)| sp.knots.iter().enumerate().map(move |(k, kn)| (dist(kn.anchor, p), Hit::Anchor([s, k]))));
    if let Some(h) = nearest(&mut anchors.into_iter()) {
        return Some(h);
    }
    // Segments: the nearest of 64 samples, refined once around it (plenty at screen tolerances).
    const N: usize = 64;
    let mut segs = path.subpaths.iter().enumerate().flat_map(|(s, sp)| {
        sp.segments().into_iter().enumerate().map(move |(k, seg)| {
            let best = (0..=N).map(|i| i as f64 / N as f64).map(|t| (dist(eval(&seg, t), p), t)).min_by(|a, b| a.0.total_cmp(&b.0)).unwrap_or((f64::MAX, 0.5));
            let step = 1.0 / N as f64;
            let fine = (0..=16)
                .map(|i| (best.1 - step + 2.0 * step * i as f64 / 16.0).clamp(0.0, 1.0))
                .map(|t| (dist(eval(&seg, t), p), t))
                .fold(best, |a, b| if b.0 < a.0 { b } else { a });
            (fine.0, Hit::Segment([s, k], fine.1))
        })
    });
    nearest(&mut segs)
}

fn checked(path: &mut Path, f: impl FnOnce(&mut Path) -> Result<(), String>) -> Result<(), String> {
    validate(path)?;
    let mut edited = path.clone();
    f(&mut edited)?;
    validate(&edited)?;
    *path = edited;
    Ok(())
}
fn check_numbers(numbers: &[f64]) -> Result<(), String> {
    if numbers.iter().any(|n| !n.is_finite() || n.abs() > MAX_COORDINATE) {
        return Err("node input must be finite and bounded".into());
    }
    Ok(())
}
/// Moves anchors and handles once each, rejecting invalid input/result without mutation.
pub fn move_anchors(path: &mut Path, anchors: &[[usize; 2]], delta: [f64; 2]) -> Result<(), String> {
    check_numbers(&delta)?;
    checked(path, |p| move_anchors_unchecked(p, anchors, delta))
}
/// Moves one handle, keeping the opposite collinear unless independent.
pub fn move_handle(path: &mut Path, knot: [usize; 2], handle: Handle, to: Point, independent: bool) -> Result<(), String> {
    check_numbers(&[to.x, to.y])?;
    checked(path, |p| move_handle_unchecked(p, knot, handle, to, independent))
}
/// Retracts or mirrors a knot's handles; input and resulting coordinates are bounded.
pub fn convert_point(path: &mut Path, knot: [usize; 2], out: Option<Point>) -> Result<(), String> {
    if let Some(p) = out {
        check_numbers(&[p.x, p.y])?;
    }
    checked(path, |p| convert_point_unchecked(p, knot, out))
}
/// Bends the chosen cubic at t, or translates a straight segment's anchors.
pub fn bend_segment(path: &mut Path, knot: [usize; 2], t: f64, delta: [f64; 2]) -> Result<(), String> {
    check_numbers(&delta)?;
    if !t.is_finite() || !(0.0..=1.0).contains(&t) {
        return Err("bend t must be in 0..1".into());
    }
    checked(path, |p| bend_segment_unchecked(p, knot, t, delta))
}

/// Coordinate budget shared by node previews and engine commits.
pub const MAX_COORDINATE: f64 = 1_000_000.0;

/// Validate before publishing a detached edit, including its resulting handles.
pub fn validate(path: &Path) -> Result<(), String> {
    if path
        .subpaths
        .iter()
        .flat_map(|sp| &sp.knots)
        .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
        .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > MAX_COORDINATE || p.y.abs() > MAX_COORDINATE)
    {
        return Err("node coordinates must be finite and at most 1000000 in magnitude".into());
    }
    Ok(())
}

/// New-index → old-index provenance, used to remap live corners and node selection atomically.
/// Inserted nodes have no origin; a broken curve can contain the same origin at both ends.
pub type Origins = Vec<Vec<Option<[usize; 2]>>>;

pub fn origins(path: &Path) -> Origins {
    path.subpaths.iter().enumerate().map(|(s, sp)| (0..sp.knots.len()).map(|k| Some([s, k])).collect()).collect()
}

fn selected(path: &mut Path, anchors: &[[usize; 2]]) -> Result<Vec<[usize; 2]>, String> {
    let mut list = anchors.to_vec();
    list.sort_unstable();
    list.dedup();
    for &r in &list {
        knot_mut(path, r)?;
    }
    Ok(list)
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn checked_topology(path: &mut Path, f: impl FnOnce(&mut Path) -> Result<Origins, String>) -> Result<Origins, String> {
    validate(path)?;
    let mut edited = path.clone();
    let map = f(&mut edited)?;
    validate(&edited)?;
    *path = edited;
    Ok(map)
}
/// Inserts an exact cubic knot, returning new-to-old provenance without partial mutation.
pub fn insert(path: &mut Path, knot: [usize; 2], t: f64) -> Result<Origins, String> {
    checked_topology(path, |p| insert_unchecked(p, knot, t))
}
/// Deletes selected knots, refitting the neighboring cubics and returning index provenance.
pub fn delete(path: &mut Path, anchors: &[[usize; 2]]) -> Result<Origins, String> {
    checked_topology(path, |p| delete_unchecked(p, anchors))
}
/// Converts a set of knots using one common neighbor snapshot.
pub fn set_kind(path: &mut Path, anchors: &[[usize; 2]], kind: PointKind) -> Result<(), String> {
    checked(path, |p| set_kind_unchecked(p, anchors, kind))
}
/// Reverses the selected subpaths and swaps their incoming/outgoing handles.
pub fn reverse_nodes(path: &mut Path, anchors: &[[usize; 2]]) -> Result<Origins, String> {
    checked_topology(path, |p| reverse_nodes_unchecked(p, anchors))
}
/// Closes selected curves; all selected subpaths are validated before publication.
pub fn close_nodes(path: &mut Path, anchors: &[[usize; 2]]) -> Result<(), String> {
    checked(path, |p| close_nodes_unchecked(p, anchors))
}
/// Applies a bounded finite affine to selected knots and their handles.
pub fn transform_nodes(path: &mut Path, anchors: &[[usize; 2]], matrix: &photocraft_geom::Affine) -> Result<(), String> {
    check_numbers(&matrix.m)?;
    checked(path, |p| transform_nodes_unchecked(p, anchors, matrix))
}

/// Exact de Casteljau insertion; both adjacent cubic pieces retain the original shape.
fn insert_unchecked(path: &mut Path, [s, k]: [usize; 2], t: f64) -> Result<Origins, String> {
    if !t.is_finite() || !(0.0 < t && t < 1.0) {
        return Err("insertion t must be between 0 and 1, exclusive".into());
    }
    let end = segment_end(path, [s, k])?;
    let mut map = origins(path);
    let a = *knot_mut(path, [s, k])?;
    let b = *knot_mut(path, end)?;
    let (ab, bc, cd) = (lerp(a.anchor, a.out_ctrl, t), lerp(a.out_ctrl, b.in_ctrl, t), lerp(b.in_ctrl, b.anchor, t));
    let (abc, bcd) = (lerp(ab, bc, t), lerp(bc, cd, t));
    let mid = lerp(abc, bcd, t);
    let straight = a.out_ctrl == a.anchor && b.in_ctrl == b.anchor;
    knot_mut(path, [s, k])?.out_ctrl = ab;
    knot_mut(path, end)?.in_ctrl = cd;
    path.subpaths[s].knots.insert(k + 1, Knot { anchor: mid, in_ctrl: abc, out_ctrl: bcd, smooth: !straight });
    map[s].insert(k + 1, None);
    Ok(map)
}

/// Adapted from VectorCraft pathops/edit.rs::remove_anchor at d522c1d7 (MIT OR Apache-2.0).
/// Fit the facing handles along the original end tangents, searching the join parameter.
/// This preserves an inserted cubic exactly (to floating point precision) and minimizes the
/// sampled error for general deletion; arbitrary corners cannot be preserved by one cubic.
fn merged_handles(left: [Point; 4], right: [Point; 4]) -> (Point, Point) {
    let direction = |a: Point, b: Point, fallback: Point| {
        let v = Point::new(b.x - a.x, b.y - a.y);
        let len = v.x.hypot(v.y);
        if len > 1e-12 {
            Point::new(v.x / len, v.y / len)
        } else {
            let v = Point::new(fallback.x - a.x, fallback.y - a.y);
            let len = v.x.hypot(v.y).max(1e-12);
            Point::new(v.x / len, v.y / len)
        }
    };
    let v0 = direction(left[0], left[1], right[3]);
    let v1 = direction(right[3], right[2], left[0]);
    let fit = |join: f64| {
        let (mut aa, mut ab, mut bb, mut ax, mut bx) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let mut samples = Vec::with_capacity(33);
        for i in 0..=32 {
            let (q, t) = if i <= 16 {
                (eval(&left, i as f64 / 16.0), join * i as f64 / 16.0)
            } else {
                (eval(&right, (i - 16) as f64 / 16.0), join + (1.0 - join) * (i - 16) as f64 / 16.0)
            };
            let u = 1.0 - t;
            let (b1, b2) = (3.0 * u * u * t, 3.0 * u * t * t);
            let base = lerp(left[0], right[3], b2 + t * t * t);
            let r = Point::new(q.x - base.x, q.y - base.y);
            aa += b1 * b1;
            ab += b1 * b2 * (v0.x * v1.x + v0.y * v1.y);
            bb += b2 * b2;
            ax += b1 * (r.x * v0.x + r.y * v0.y);
            bx += b2 * (r.x * v1.x + r.y * v1.y);
            samples.push((q, t));
        }
        let det = aa * bb - ab * ab;
        let fallback = dist(left[0], right[3]) / 3.0;
        let (a, b) = if det.abs() > 1e-12 { ((ax * bb - bx * ab) / det, (bx * aa - ax * ab) / det) } else { (fallback, fallback) };
        let (a, b) = (a.max(0.0), b.max(0.0));
        let c1 = add(left[0], [v0.x * a, v0.y * a]);
        let c2 = add(right[3], [v1.x * b, v1.y * b]);
        let seg = [left[0], c1, c2, right[3]];
        let error: f64 = samples.iter().map(|(p, t)| dist(*p, eval(&seg, *t)).powi(2)).sum();
        (error, c1, c2)
    };
    // Coarse search avoids choosing the wrong local minimum on cusps, then bounded golden search.
    let best = (1..100).map(|i| (i as f64 / 100.0, fit(i as f64 / 100.0).0)).min_by(|a, b| a.1.total_cmp(&b.1)).map_or(0.5, |b| b.0);
    let (mut lo, mut hi) = ((best - 0.01).max(0.000001), (best + 0.01).min(0.999999));
    for _ in 0..48 {
        let (a, b) = (hi - (hi - lo) * 0.618_033_988_749_895, lo + (hi - lo) * 0.618_033_988_749_895);
        if fit(a).0 < fit(b).0 {
            hi = b;
        } else {
            lo = a;
        }
    }
    let (_, a, b) = fit((lo + hi) * 0.5);
    (a, b)
}

fn delete_unchecked(path: &mut Path, anchors: &[[usize; 2]]) -> Result<Origins, String> {
    let list = selected(path, anchors)?;
    let mut map = origins(path);
    for [s, k] in list.into_iter().rev() {
        let sp = &mut path.subpaths[s];
        let n = sp.knots.len();
        if n >= 3 && (sp.closed || (k > 0 && k + 1 < n)) {
            let (prev, next) = ((k + n - 1) % n, (k + 1) % n);
            let (a, m, b) = (sp.knots[prev], sp.knots[k], sp.knots[next]);
            if a.out_ctrl != a.anchor || m.in_ctrl != m.anchor || m.out_ctrl != m.anchor || b.in_ctrl != b.anchor {
                let (out, incoming) = merged_handles([a.anchor, a.out_ctrl, m.in_ctrl, m.anchor], [m.anchor, m.out_ctrl, b.in_ctrl, b.anchor]);
                sp.knots[prev].out_ctrl = out;
                sp.knots[next].in_ctrl = incoming;
            }
        }
        sp.knots.remove(k);
        map[s].remove(k);
        if sp.knots.len() < 3 {
            sp.closed = sp.closed && sp.knots.len() == 2 && sp.knots.iter().any(|a| a.in_ctrl != a.anchor || a.out_ctrl != a.anchor);
        }
        if !sp.closed {
            if let Some(a) = sp.knots.first_mut() {
                a.in_ctrl = a.anchor;
            }
            if let Some(a) = sp.knots.last_mut() {
                a.out_ctrl = a.anchor;
            }
        }
    }
    let keep: Vec<_> = path.subpaths.iter().enumerate().filter(|(_, sp)| !sp.knots.is_empty()).map(|(s, _)| s).collect();
    path.subpaths = keep.iter().map(|&s| path.subpaths[s].clone()).collect();
    Ok(keep.iter().map(|&s| map[s].clone()).collect())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointKind {
    Sharp,
    Smooth,
    Smart,
}

/// Smooth keeps existing handle lengths; Smart recalculates lengths from adjacent chords.
/// Neighbours are read from the same snapshot, so multi-node conversion is order-independent.
fn set_kind_unchecked(path: &mut Path, anchors: &[[usize; 2]], kind: PointKind) -> Result<(), String> {
    let list = selected(path, anchors)?;
    let base = path.clone();
    for [s, i] in list {
        let sp = &base.subpaths[s];
        let a = sp.knots[i].anchor;
        if kind == PointKind::Sharp {
            *knot_mut(path, [s, i])? = Knot::corner(a.x, a.y);
            continue;
        }
        let n = sp.knots.len();
        let prev = if i > 0 || sp.closed { sp.knots[(i + n - 1) % n].anchor } else { a };
        let next = if i + 1 < n || sp.closed { sp.knots[(i + 1) % n].anchor } else { a };
        let old = sp.knots[i];
        let dir = if kind == PointKind::Smooth && dist(old.in_ctrl, old.out_ctrl) > 1e-9 {
            Point::new(old.out_ctrl.x - old.in_ctrl.x, old.out_ctrl.y - old.in_ctrl.y)
        } else {
            Point::new(next.x - prev.x, next.y - prev.y)
        };
        let len = dir.x.hypot(dir.y);
        let (x, y) = if len > 1e-9 { (dir.x / len, dir.y / len) } else { (1.0, 0.0) };
        let lengths = if kind == PointKind::Smooth { (dist(a, old.in_ctrl), dist(a, old.out_ctrl)) } else { (0.0, 0.0) };
        let (incoming, outgoing) =
            (if lengths.0 > 1e-9 { lengths.0 } else { dist(a, prev) / 3.0 }, if lengths.1 > 1e-9 { lengths.1 } else { dist(a, next) / 3.0 });
        *knot_mut(path, [s, i])? = Knot::smooth(a, Point::new(a.x - x * incoming, a.y - y * incoming), Point::new(a.x + x * outgoing, a.y + y * outgoing));
    }
    Ok(())
}

/// Reverse selected subpaths without reordering PSD components or changing winding semantics.
fn reverse_nodes_unchecked(path: &mut Path, anchors: &[[usize; 2]]) -> Result<Origins, String> {
    let list = selected(path, anchors)?;
    let mut subs = Vec::new();
    for [s, _] in list {
        let mut root = s;
        while root > 0 && path.subpaths[root].op == photocraft_doc::PathOp::Join {
            root -= 1;
        }
        subs.push(root);
        let mut next = root + 1;
        while next < path.subpaths.len() && path.subpaths[next].op == photocraft_doc::PathOp::Join {
            subs.push(next);
            next += 1;
        }
    }
    subs.sort_unstable();
    subs.dedup();
    let mut map = origins(path);
    for s in subs {
        path.subpaths[s].knots.reverse();
        for k in &mut path.subpaths[s].knots {
            std::mem::swap(&mut k.in_ctrl, &mut k.out_ctrl);
        }
        map[s].reverse();
    }
    Ok(map)
}

fn close_nodes_unchecked(path: &mut Path, anchors: &[[usize; 2]]) -> Result<(), String> {
    for [s, _] in selected(path, anchors)? {
        if path.subpaths[s].knots.len() < 2 {
            return Err("close requires two nodes".into());
        }
        path.subpaths[s].closed = true;
    }
    Ok(())
}

/// Transform anchors and their handles together; unselected knots stay in document space.
fn transform_nodes_unchecked(path: &mut Path, anchors: &[[usize; 2]], matrix: &photocraft_geom::Affine) -> Result<(), String> {
    for r in selected(path, anchors)? {
        let k = knot_mut(path, r)?;
        *k = k.transform(matrix);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_doc::Subpath;

    fn tri() -> Path {
        Path::new(vec![Subpath::polygon(&[(0.0, 0.0), (100.0, 0.0), (50.0, 80.0)])])
    }

    /// A smooth knot at (50, 0) between corners, handles 20 px either side.
    fn curve() -> Path {
        let mut sp = Subpath::polyline(&[(0.0, 50.0), (50.0, 0.0), (100.0, 50.0)]);
        sp.knots[1] = Knot::smooth(Point::new(50.0, 0.0), Point::new(30.0, 0.0), Point::new(70.0, 0.0));
        Path::new(vec![sp])
    }

    fn at(path: &Path, s: usize, k: usize) -> Knot {
        path.subpaths[s].knots[k]
    }

    #[test]
    fn anchors_move_with_their_handles_once_each() {
        let mut p = curve();
        move_anchors(&mut p, &[[0, 1], [0, 1]], [5.0, -2.0]).unwrap();
        let k = at(&p, 0, 1);
        assert_eq!((k.anchor, k.in_ctrl, k.out_ctrl), (Point::new(55.0, -2.0), Point::new(35.0, -2.0), Point::new(75.0, -2.0)));
        assert_eq!(at(&p, 0, 0).anchor, Point::new(0.0, 50.0), "other anchors stay");
    }

    #[test]
    fn bad_indices_are_errors_and_change_nothing() {
        let mut p = tri();
        assert!(move_anchors(&mut p, &[[0, 0], [0, 9]], [1.0, 1.0]).is_err());
        assert_eq!(p, tri(), "validated before any knot moved");
        assert!(move_handle(&mut p, [3, 0], Handle::In, Point::new(0.0, 0.0), false).is_err());
        assert!(bend_segment(&mut p, [0, 7], 0.5, [1.0, 1.0]).is_err());
        assert!(convert_point(&mut p, [usize::MAX, usize::MAX], None).is_err());
        let mut open = curve();
        assert!(bend_segment(&mut open, [0, 2], 0.5, [1.0, 1.0]).is_err(), "an open subpath's last knot starts no segment");
        assert!(bend_segment(&mut open, [0, usize::MAX], 0.5, [1.0, 1.0]).is_err());
    }

    #[test]
    fn smooth_handles_stay_collinear_and_keep_their_length() {
        let mut p = curve();
        move_handle(&mut p, [0, 1], Handle::Out, Point::new(50.0, 30.0), false).unwrap();
        let k = at(&p, 0, 1);
        assert_eq!(k.out_ctrl, Point::new(50.0, 30.0));
        assert!(dist(k.in_ctrl, Point::new(50.0, -20.0)) < 1e-9, "opposite turned, length 20 kept: {:?}", k.in_ctrl);
        assert!(k.smooth);
    }

    #[test]
    fn independent_handles_break_the_knot() {
        let mut p = curve();
        move_handle(&mut p, [0, 1], Handle::In, Point::new(40.0, 20.0), true).unwrap();
        let k = at(&p, 0, 1);
        assert_eq!((k.in_ctrl, k.out_ctrl, k.smooth), (Point::new(40.0, 20.0), Point::new(70.0, 0.0), false));
    }

    #[test]
    fn convert_point_retracts_or_pulls_mirrored_handles() {
        let mut p = curve();
        convert_point(&mut p, [0, 1], None).unwrap();
        assert_eq!(at(&p, 0, 1), Knot::corner(50.0, 0.0));
        convert_point(&mut p, [0, 0], Some(Point::new(10.0, 60.0))).unwrap();
        let k = at(&p, 0, 0);
        assert_eq!((k.in_ctrl, k.out_ctrl, k.smooth), (Point::new(-10.0, 40.0), Point::new(10.0, 60.0), true));
    }

    #[test]
    fn straight_segments_move_with_both_anchors() {
        let mut p = tri();
        // The closing segment (2 → 0) wraps.
        bend_segment(&mut p, [0, 2], 0.5, [0.0, 10.0]).unwrap();
        assert_eq!((at(&p, 0, 2).anchor, at(&p, 0, 0).anchor, at(&p, 0, 1).anchor), (Point::new(50.0, 90.0), Point::new(0.0, 10.0), Point::new(100.0, 0.0)));
    }

    #[test]
    fn curves_bend_through_the_dragged_point() {
        for t in [0.2, 0.5, 0.8] {
            let mut p = curve();
            let seg = |p: &Path| p.subpaths[0].segments()[1];
            let before = eval(&seg(&p), t);
            bend_segment(&mut p, [0, 1], t, [3.0, 12.0]).unwrap();
            let after = eval(&seg(&p), t);
            assert!(dist(after, Point::new(before.x + 3.0, before.y + 12.0)) < 1e-9, "t={t}: {after:?}");
            let k = at(&p, 0, 1);
            let (vi, vo) = ((k.in_ctrl.x - 50.0, k.in_ctrl.y), (k.out_ctrl.x - 50.0, k.out_ctrl.y));
            assert!((vi.0 * vo.1 - vi.1 * vo.0).abs() < 1e-9 && vi.0 * vo.0 + vi.1 * vo.1 < 0.0, "the smooth knot stayed smooth");
        }
    }

    #[test]
    fn hits_prefer_handles_then_anchors_then_segments() {
        let p = curve();
        let shown = [([0, 1], Handle::Out)];
        assert_eq!(hit(&p, Point::new(71.0, 1.0), 3.0, &shown), Some(Hit::Handle([0, 1], Handle::Out)));
        assert_eq!(hit(&p, Point::new(71.0, 1.0), 3.0, &[]), None, "hidden handles aren't hit");
        assert_eq!(hit(&p, Point::new(49.0, 1.0), 3.0, &shown), Some(Hit::Anchor([0, 1])));
        let Some(Hit::Segment(r, t)) = hit(&tri(), Point::new(50.0, 1.0), 3.0, &[]) else { panic!("segment") };
        assert_eq!(r, [0, 0]);
        assert!((t - 0.5).abs() < 0.02, "{t}");
        assert_eq!(hit(&tri(), Point::new(50.0, 40.0), 3.0, &[]), None);
    }

    #[test]
    fn marquee_and_subpath_selections() {
        let p = tri();
        assert_eq!(anchors_in(&p, [110.0, -5.0, 40.0, 5.0]), vec![[0, 1]]);
        assert_eq!(subpath_anchors(&p, 0), vec![[0, 0], [0, 1], [0, 2]]);
        assert!(subpath_anchors(&p, 4).is_empty());
    }
    #[test]
    fn node_insert_preserves_every_sample_and_delete_recovers_the_cubic() {
        let a = Point::new(10.0, 40.0);
        let b = Point::new(120.0, 50.0);
        let mut sp = Subpath::polyline(&[(a.x, a.y), (b.x, b.y)]);
        sp.knots[0].out_ctrl = Point::new(20.0, -40.0);
        sp.knots[1].in_ctrl = Point::new(100.0, 120.0);
        let original = Path::new(vec![sp]);
        let cubic = original.subpaths[0].segments()[0];
        for t in [0.1, 0.37, 0.5, 0.85] {
            let mut p = original.clone();
            insert(&mut p, [0, 0], t).unwrap();
            let pieces = p.subpaths[0].segments();
            for i in 0..=100 {
                let u = i as f64 / 100.0;
                let sample = if u <= t { eval(&pieces[0], u / t) } else { eval(&pieces[1], (u - t) / (1.0 - t)) };
                assert!(dist(sample, eval(&cubic, u)) < 1e-10);
            }
            delete(&mut p, &[[0, 1]]).unwrap();
            let merged = p.subpaths[0].segments()[0];
            for i in 0..=100 {
                let u = i as f64 / 100.0;
                assert!(dist(eval(&merged, u), eval(&cubic, u)) < 1e-6, "t {t}");
            }
        }
    }
    #[test]
    fn node_closed_insert_delete_and_provenance_reorder() {
        let mut p = tri();
        let original = p.clone();
        let map = insert(&mut p, [0, 2], 0.5).unwrap();
        assert_eq!(map[0], vec![Some([0, 0]), Some([0, 1]), Some([0, 2]), None]);
        delete(&mut p, &[[0, 3]]).unwrap();
        for (a, b) in p.subpaths[0].segments().iter().zip(original.subpaths[0].segments()) {
            for i in 0..=100 {
                let t = i as f64 / 100.0;
                assert!(dist(eval(a, t), eval(&b, t)) < 1e-5);
            }
        }
        let map = reverse_nodes(&mut p, &[[0, 0]]).unwrap();
        assert_eq!(map[0][0], Some([0, 2]));
    }
    #[test]
    fn node_nonfinite_and_overflow_leave_geometry_unchanged() {
        let mut p = tri();
        let before = p.clone();
        assert!(move_anchors(&mut p, &[[0, 0]], [f64::NAN, 1.0]).is_err());
        assert!(move_anchors(&mut p, &[[0, 1]], [MAX_COORDINATE, 0.0]).is_err());
        assert!(convert_point(&mut p, [0, 1], Some(Point::new(-1e6, 0.0))).is_err());
        assert!(bend_segment(&mut p, [0, 0], f64::NAN, [1.0, 0.0]).is_err());
        assert_eq!(p, before);
    }
    #[test]
    fn node_smooth_and_smart_are_collinear_and_sharp_retracts() {
        for kind in [PointKind::Smooth, PointKind::Smart] {
            let mut p = tri();
            set_kind(&mut p, &[[0, 0], [0, 1], [0, 2]], kind).unwrap();
            for k in &p.subpaths[0].knots {
                let a = Point::new(k.in_ctrl.x - k.anchor.x, k.in_ctrl.y - k.anchor.y);
                let b = Point::new(k.out_ctrl.x - k.anchor.x, k.out_ctrl.y - k.anchor.y);
                assert!((a.x * b.y - a.y * b.x).abs() < 1e-9);
                assert!(k.smooth);
            }
            set_kind(&mut p, &[[0, 1]], PointKind::Sharp).unwrap();
            assert_eq!(p.subpaths[0].knots[1], Knot::corner(100.0, 0.0));
        }
    }
    #[test]
    fn node_reverse_keeps_psd_component_holes_by_coverage_oracle() {
        let outer = Subpath::polygon(&[(2.0, 2.0), (30.0, 2.0), (30.0, 30.0), (2.0, 30.0)]);
        let hole = Subpath::polygon(&[(8.0, 8.0), (8.0, 24.0), (24.0, 24.0), (24.0, 8.0)]).with_op(photocraft_doc::PathOp::Join);
        let mut path = Path::new(vec![outer, hole]);
        let rect = photocraft_geom::Rect::new(0, 0, 32, 32);
        let before = crate::path_coverage(&path, rect);
        assert_eq!(before[12 * 32 + 12], 0.0);
        reverse_nodes(&mut path, &[[1, 0]]).unwrap();
        assert_eq!(crate::path_coverage(&path, rect), before);
    }
}
