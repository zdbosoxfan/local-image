//! Adapted from VectorCraft crates/tools/src/distort/liquify.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0; see licenses/vectorcraft-NOTICE.
//! Liquify tools: Warp (Shift+R), Twirl, Pucker, Bloat, Scallop, Crystallize, Wrinkle.
//!
//! A drag previews `object.liquify {tool, points, width, height, angle, intensity, detail,
//! simplify, rate?, complexity?, horizontal?, vertical?, affect…?, usePressure?, ids?}` with every
//! pointer sample so far (and its pen pressure while Use Pressure Pen is on), so the result is a
//! pure function of the parameters (replay reproduces it exactly). Holding the brush still with
//! Twirl, Pucker or Bloat repeats the last sample every 0.1 seconds of the UI tick
//! time: one more dab there each time.
//!
//! The kernel resamples the stroke into dabs spaced a fraction of the brush apart ([`Dabber`]:
//! more samples only add dabs, so a growing stroke can be applied as it grows). For each dab it
//! first subdivides the segments the brush touches (Detail: more anchors where the brush passes,
//! lines become curves so the result stays smooth), then moves anchors and handles by the tool's
//! displacement field weighted by a smooth falloff `(1 − r²)²` inside the (elliptical, rotated)
//! brush. Noise for Scallop/Crystallize/Wrinkle is a hash of the dab and anchor indices.

use kurbo::{ParamCurveArclen, ParamCurveNearest};
use photocraft_pathops::geom::{Anchor, AnchorKind, PathData, Point, Rect, SubPath, Vec2};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LiquifyKind {
    Warp,
    Twirl,
    Pucker,
    Bloat,
    Scallop,
    Crystallize,
    Wrinkle,
}

impl LiquifyKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "warp" => Self::Warp,
            "twirl" => Self::Twirl,
            "pucker" => Self::Pucker,
            "bloat" => Self::Bloat,
            "scallop" => Self::Scallop,
            "crystallize" => Self::Crystallize,
            "wrinkle" => Self::Wrinkle,
            _ => return None,
        })
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Warp => "warp",
            Self::Twirl => "twirl",
            Self::Pucker => "pucker",
            Self::Bloat => "bloat",
            Self::Scallop => "scallop",
            Self::Crystallize => "crystallize",
            Self::Wrinkle => "wrinkle",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Warp => "Warp",
            Self::Twirl => "Twirl",
            Self::Pucker => "Pucker",
            Self::Bloat => "Bloat",
            Self::Scallop => "Scallop",
            Self::Crystallize => "Crystallize",
            Self::Wrinkle => "Wrinkle",
        }
    }
    /// Holding the brush still keeps applying the tool (a repeated stroke sample is one more dab).
    pub fn holds(self) -> bool {
        matches!(self, Self::Twirl | Self::Pucker | Self::Bloat)
    }
    /// Has the Simplify option (Scallop, Crystallize and Wrinkle have Brush Affects instead).
    pub fn simplifies(self) -> bool {
        matches!(self, Self::Warp | Self::Twirl | Self::Pucker | Self::Bloat)
    }
    /// Has the Brush Affects options (anchor points, in and out tangent handles).
    pub fn has_affects(self) -> bool {
        !self.simplifies()
    }
}

/// Brush and tool options (Warp Tool Options and friends).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LiquifyParams {
    pub kind: LiquifyKind,
    /// Global knot budget across every subpath.
    pub node_budget: usize,
    /// Brush width / height in document pixels and rotation in degrees.
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    /// 0..1.
    pub intensity: f64,
    /// Use Pressure Pen: each sample scales the configured intensity by pressure.
    pub use_pressure: bool,
    /// 1..10: anchor density added where the brush passes.
    pub detail: f64,
    /// 0..100: removal of redundant flat anchors afterwards (Warp, Twirl, Pucker, Bloat).
    pub simplify: f64,
    /// The Simplify checkbox.
    pub simplify_on: bool,
    /// Twirl rate, degrees (−180..180).
    pub rate: f64,
    /// Scallop/Crystallize/Wrinkle complexity (0..15).
    pub complexity: f64,
    /// Wrinkle horizontal / vertical amount (0..1).
    pub horizontal: f64,
    pub vertical: f64,
    pub affect_anchors: bool,
    pub affect_in: bool,
    pub affect_out: bool,
}

impl LiquifyParams {
    pub fn new(kind: LiquifyKind) -> Self {
        Self {
            kind,
            node_budget: MAX_ANCHORS,
            width: 100.0,
            height: 100.0,
            angle: 0.0,
            intensity: 0.5,
            use_pressure: false,
            detail: 2.0,
            simplify: 50.0,
            simplify_on: true,
            rate: 40.0,
            complexity: 1.0,
            horizontal: 0.0,
            vertical: 1.0,
            affect_anchors: true,
            affect_in: true,
            affect_out: true,
        }
    }

    /// How far off the chord Simplify removes anchors (0: Simplify is off or not an option).
    pub fn simplify_tolerance(&self) -> f64 {
        if self.kind.simplifies() && self.simplify_on { self.simplify / 100.0 * self.detail_spacing() * 0.02 } else { 0.0 }
    }

    fn radii(&self) -> (f64, f64) {
        (self.width / 2.0, self.height / 2.0)
    }

    /// Spacing between dabs.
    pub fn dab_spacing(&self) -> f64 {
        let (rx, ry) = self.radii();
        (rx.min(ry) * 0.2).max(0.5)
    }

    /// Target segment length inside the brush.
    fn detail_spacing(&self) -> f64 {
        let (rx, ry) = self.radii();
        let extra = match self.kind {
            LiquifyKind::Scallop | LiquifyKind::Crystallize | LiquifyKind::Wrinkle => 1.0 + self.complexity * 0.5,
            _ => 1.0,
        };
        (rx.min(ry) * 2.0 / (self.detail * 2.0 + 1.0) / extra).max(0.5)
    }

    /// Bounding box of the brush at `c`.
    pub fn brush_bounds(&self, c: Point) -> Rect {
        let r = self.width.max(self.height) / 2.0;
        Rect::new(c.x - r, c.y - r, c.x + r, c.y + r)
    }

    /// Falloff weight of `q` for a brush centred at `c` (0 outside).
    pub fn falloff(&self, c: Point, q: Point) -> f64 {
        let (rx, ry) = self.radii();
        let (s, co) = (-self.angle.to_radians()).sin_cos();
        let d = q - c;
        let u = Vec2::new(d.x * co - d.y * s, d.x * s + d.y * co);
        let r2 = (u.x / rx).powi(2) + (u.y / ry).powi(2);
        if r2 >= 1.0 { 0.0 } else { (1.0 - r2).powi(2) }
    }
}

/// One brush dab: its centre and intensity (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dab {
    pub c: Point,
    pub intensity: f64,
}

/// A stroke sample: the pointer position and its pen pressure (0..1).
pub type Sample = (Point, f64);

/// Most dabs a stroke makes (later samples add none).
pub const MAX_DABS: usize = 2048;

/// Resamples a growing stroke into dabs `spacing` apart along it, the first sample always a dab.
/// More samples only add dabs ([`Self::dabs`] is a prefix of what it becomes), so a stroke can be
/// applied as it grows; the stroke's end, when it is off the spacing, is the [`Self::tail`].
#[derive(Clone, Debug, Default)]
pub struct Dabber {
    spacing: f64,
    pressure_scale: f64,
    /// Every dab's intensity; None: each sample's pressure.
    intensity: Option<f64>,
    /// A repeated sample is one more dab there (the brush held still).
    holds: bool,
    samples: Vec<Sample>,
    /// Distance from the last dab to the last sample.
    carry: f64,
    /// The dabs on the spacing so far.
    pub dabs: Vec<Dab>,
}

impl Dabber {
    /// The dabs of a stroke with brush `prm`.
    pub fn new(prm: &LiquifyParams) -> Self {
        Self { pressure_scale: prm.intensity, ..Self::with(prm.dab_spacing(), (!prm.use_pressure).then_some(prm.intensity), prm.kind.holds()) }
    }

    fn with(spacing: f64, intensity: Option<f64>, holds: bool) -> Self {
        Self { spacing, pressure_scale: 1.0, intensity, holds, ..Self::default() }
    }

    /// The samples so far.
    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    fn dab(&self, c: Point, pressure: f64) -> Dab {
        Dab { c, intensity: self.intensity.unwrap_or(pressure.clamp(0.0, 1.0) * self.pressure_scale) }
    }

    /// Preflight additional dabs without copying the retained prefix or allocating.
    pub fn try_push(&mut self, sample: Sample) -> bool {
        let additional = match self.samples.last().copied() {
            None => 1,
            Some((a, _)) => {
                let len = a.distance(sample.0);
                if len < 1e-12 {
                    usize::from(self.holds)
                } else {
                    let mut count = 0;
                    let mut s = self.spacing - self.carry;
                    while s <= len {
                        count += 1;
                        if self.dabs.len() + count > MAX_DABS {
                            return false;
                        }
                        s += self.spacing;
                    }
                    count
                }
            }
        };
        if self.dabs.len() + additional > MAX_DABS {
            return false;
        }
        self.push(sample);
        true
    }

    /// Add a sample, and the dabs up to it.
    pub fn push(&mut self, (b, pb): Sample) {
        let prev = self.samples.last().copied();
        self.samples.push((b, pb));
        let Some((a, pa)) = prev else {
            self.dabs.push(self.dab(b, pb));
            return;
        };
        if self.dabs.len() >= MAX_DABS {
            return;
        }
        let len = a.distance(b);
        if len < 1e-12 {
            if self.holds {
                self.dabs.push(self.dab(b, pb));
                self.carry = 0.0;
            }
            return;
        }
        let mut s = self.spacing - self.carry;
        while s <= len && self.dabs.len() < MAX_DABS {
            let t = s / len;
            self.dabs.push(self.dab(a.lerp(b, t), pa + (pb - pa) * t));
            s += self.spacing;
        }
        self.carry = len - (s - self.spacing);
    }

    /// The dab at the last sample when it is off the spacing (the stroke ends there).
    pub fn tail(&self) -> Option<Dab> {
        let &(end, p) = self.samples.last()?;
        let last = self.dabs.last()?.c;
        (end.distance(last) > self.spacing * 0.25).then(|| self.dab(end, p))
    }
}

fn hash(mut x: u64) -> u64 {
    // splitmix64
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Deterministic noise in [-1, 1].
fn noise(a: u64, b: u64, c: u64) -> f64 {
    let h = hash(a.wrapping_mul(0x1000_0000_01b3) ^ hash(b.wrapping_mul(31) ^ hash(c)));
    (h >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
}

pub const MAX_ANCHORS: usize = 20_000;

/// Split the segments of `sp` that the brush at `c` touches until they are at most `spacing` long.
/// Straight segments become curves first so the deformation stays smooth.
fn subdivide(sp: &mut SubPath, prm: &LiquifyParams, c: Point, spacing: f64, remaining: &mut usize) {
    let bb = prm.brush_bounds(c);
    let mut seg = 0;
    while seg < sp.segment_count() {
        if *remaining == 0 {
            return;
        }
        let cub = sp.segment(seg);
        let sb = kurbo::ParamCurveExtrema::bounding_box(&cub);
        let touched = sb.intersect(bb).area() > 0.0 || (sb.width() == 0.0 || sb.height() == 0.0) && sb.inflate(1e-6, 1e-6).intersect(bb).area() > 0.0;
        if !touched || cub.arclen(1e-3) <= spacing {
            seg += 1;
            continue;
        }
        // Query the curve in normalized elliptical brush coordinates. Fixed sample counts
        // miss a narrow brush crossing the middle of a long segment.
        let local = |p: Point| {
            let (s, co) = (-prm.angle.to_radians()).sin_cos();
            let d = p - c;
            Point::new((d.x * co - d.y * s) / (prm.width / 2.), (d.x * s + d.y * co) / (prm.height / 2.))
        };
        let nearest = kurbo::CubicBez::new(local(cub.p0), local(cub.p1), local(cub.p2), local(cub.p3)).nearest(Point::ORIGIN, 1e-6);
        if nearest.distance_sq >= 1.0 {
            seg += 1;
            continue;
        }
        let n = sp.anchors.len();
        if sp.segment_is_line(seg) {
            let (i0, i1) = (seg % n, (seg + 1) % n);
            let (a, b) = (sp.anchors[i0].p, sp.anchors[i1].p);
            sp.anchors[i0].h_out = a.lerp(b, 1. / 3.);
            sp.anchors[i1].h_in = a.lerp(b, 2. / 3.);
        }
        let cub = sp.segment(seg);
        let nearest = kurbo::CubicBez::new(local(cub.p0), local(cub.p1), local(cub.p2), local(cub.p3)).nearest(Point::ORIGIN, 1e-6);
        // Split close to the brush first, then revisit the two pieces until their lengths
        // meet Detail. Clamping prevents microscopic endpoint splits and bounds work.
        sp.insert_anchor(seg, nearest.t.clamp(0.05, 0.95));
        *remaining -= 1;
    }
}

/// Displace one point (`which`: 0 anchor, 1 in-handle, 2 out-handle).
#[allow(clippy::too_many_arguments)]
fn displace(prm: &LiquifyParams, c: Point, prev: Point, dab: u64, key: u64, q: Point, anchor_new: Option<Point>, anchor_old: Point) -> Point {
    let f = prm.falloff(c, q);
    let i = prm.intensity;
    let (rx, ry) = prm.radii();
    let r = rx.max(ry);
    let d = q - c;
    let len = d.hypot();
    let dir = if len > 1e-9 { d / len } else { Vec2::ZERO };
    match prm.kind {
        LiquifyKind::Warp => q + (c - prev) * (i * f),
        LiquifyKind::Twirl => {
            let a = prm.rate.to_radians() * i * f * 0.25;
            let (s, co) = a.sin_cos();
            c + Vec2::new(d.x * co - d.y * s, d.x * s + d.y * co)
        }
        LiquifyKind::Pucker => q - d * (i * f * 0.2),
        LiquifyKind::Bloat => q + dir * (i * f * 0.1 * r),
        LiquifyKind::Scallop => match anchor_new {
            None => q - d * (i * f * 0.12),
            Some(a) => {
                // Handles swing sideways (curls) and stretch: arc-like details on the outline.
                let h = q - anchor_old;
                let phi = noise(dab, key, 7) * std::f64::consts::FRAC_PI_2 * i * f * (1.0 + prm.complexity * 0.2);
                let (s, co) = phi.sin_cos();
                a + Vec2::new(h.x * co - h.y * s, h.x * s + h.y * co) * (1.0 + 0.6 * i * f)
            }
        },
        LiquifyKind::Crystallize => match anchor_new {
            None => q + dir * (i * f * 0.1 * r * (0.5 + 0.5 * noise(dab, key, 3).abs())),
            // Handles are pulled in: spikes.
            Some(a) => a + (q - anchor_old) * (1.0 - 0.6 * i * f),
        },
        LiquifyKind::Wrinkle => {
            let amp = i * f * 0.06 * r * (1.0 + prm.complexity * 0.1);
            q + Vec2::new(noise(dab, key, 11) * prm.horizontal * amp, noise(dab, key, 13) * prm.vertical * amp)
        }
    }
}

/// Apply one dab to a subpath (after subdivision).
fn apply_dab(sp: &mut SubPath, prm: &LiquifyParams, c: Point, prev: Point, dab: u64, salt: u64) -> bool {
    let bb = prm.brush_bounds(c);
    let mut changed = false;
    for (ai, a) in sp.anchors.iter_mut().enumerate() {
        if !bb.contains(a.p) && !bb.contains(a.h_in) && !bb.contains(a.h_out) {
            continue;
        }
        let key = hash(salt ^ (ai as u64).wrapping_mul(0x9e37_79b9));
        let old = *a;
        let np = if prm.affect_anchors { displace(prm, c, prev, dab, key, old.p, None, old.p) } else { old.p };
        let moved = np - old.p;
        let handle = |h: Point, affect: bool, k: u64| -> Point {
            if h.distance(old.p) < 1e-12 {
                return np;
            }
            if !affect {
                return h + moved;
            }
            match prm.kind {
                LiquifyKind::Scallop | LiquifyKind::Crystallize => displace(prm, c, prev, dab, key ^ k, h, Some(np), old.p),
                _ => displace(prm, c, prev, dab, key ^ k, h, None, old.p),
            }
        };
        let hi = handle(old.h_in, prm.affect_in, 1);
        let ho = handle(old.h_out, prm.affect_out, 2);
        if np != old.p || hi != old.h_in || ho != old.h_out {
            changed = true;
            *a = Anchor {
                p: np,
                h_in: hi,
                h_out: ho,
                kind: if old.kind == AnchorKind::Smooth && prm.kind == LiquifyKind::Crystallize { AnchorKind::Corner } else { old.kind },
            };
        }
    }
    changed
}

/// Does the brush box `bb` reach subpath `sp`?
pub(super) fn touches(sp: &SubPath, bb: Rect) -> bool {
    sp_bounds(sp).is_some_and(|spb| spb.x0 <= bb.x1 && spb.x1 >= bb.x0 && spb.y0 <= bb.y1 && spb.y1 >= bb.y0)
}

/// Apply dab `d` (number `di`, after the dab at `prev`) to every subpath of `path` it reaches.
fn dab_on(path: &mut PathData, di: usize, d: Dab, prev: Point, prm: &LiquifyParams, salt: u64) -> bool {
    if d.intensity == 0.0 || (prm.kind == LiquifyKind::Warp && d.c == prev) {
        return false;
    }
    let prm = LiquifyParams { intensity: d.intensity, ..*prm };
    let (bb, spacing) = (prm.brush_bounds(d.c), prm.detail_spacing());
    let mut changed = false;
    let mut remaining = prm.node_budget.saturating_sub(path.anchor_count());
    for (si, sp) in path.subpaths.iter_mut().enumerate() {
        if !touches(sp, bb) {
            continue;
        }
        subdivide(sp, &prm, d.c, spacing, &mut remaining);
        changed |= apply_dab(sp, &prm, d.c, prev, di as u64, hash(salt ^ (si as u64 + 1).wrapping_mul(0x1234_5678_9abc_def1)));
    }
    changed
}

/// A path under a growing stroke: the dabs applied so far (Simplify comes at [`Self::finish`]).
/// Applying a stroke's dabs in any number of steps gives the same path as applying them at once.
#[derive(Clone, Debug)]
pub struct PathStroke {
    path: PathData,
    /// Decorrelates noise between paths.
    salt: u64,
    /// Dabs applied.
    applied: usize,
    /// Where dabs moved something (Simplify works there), None while nothing moved.
    region: Option<Rect>,
}

impl PathStroke {
    pub fn new(path: PathData, salt: u64) -> Self {
        Self { path, salt, applied: 0, region: None }
    }

    /// Apply the stroke's dabs not applied yet.
    pub fn advance(&mut self, dabs: &[Dab], prm: &LiquifyParams) {
        for di in self.applied..dabs.len() {
            let Some(&d) = dabs.get(di) else { break };
            let prev = di.checked_sub(1).and_then(|i| dabs.get(i)).map_or(d.c, |p| p.c);
            if dab_on(&mut self.path, di, d, prev, prm, self.salt) {
                let bb = prm.brush_bounds(d.c);
                self.region = Some(self.region.map_or(bb, |r| r.union(bb)));
            }
        }
        self.applied = self.applied.max(dabs.len());
    }

    /// The path after the stroke (`dabs`, all applied, then `tail`), simplified; None when
    /// nothing moved.
    pub fn finish(&self, dabs: &[Dab], tail: Option<Dab>, prm: &LiquifyParams) -> Option<PathData> {
        let mut region = self.region;
        let mut out = None;
        if let Some(t) = tail.filter(|t| self.path.subpaths.iter().any(|sp| touches(sp, prm.brush_bounds(t.c)))) {
            let mut p = self.path.clone();
            if dab_on(&mut p, dabs.len(), t, dabs.last().map_or(t.c, |d| d.c), prm, self.salt) {
                let bb = prm.brush_bounds(t.c);
                region = Some(region.map_or(bb, |r| r.union(bb)));
            }
            out = Some(p);
        }
        let region = region?;
        let path = out.unwrap_or_else(|| self.path.clone());
        let _ = region;
        Some(path)
    }
}

fn sp_bounds(sp: &SubPath) -> Option<Rect> {
    let mut it = sp.anchors.iter().flat_map(|a| [a.p, a.h_in, a.h_out]);
    let f = it.next()?;
    Some(it.fold(Rect::from_points(f, f), |r, p| r.union_pt(p)))
}
