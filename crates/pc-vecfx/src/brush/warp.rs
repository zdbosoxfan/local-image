//! Adapted from VectorCraft crates/brush/src/warp.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092. MIT OR Apache-2.0.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//! Long art edges subdivide at track corners as well as uniform arc-length steps.
use super::track::{Rng, Track, normal, tracks};
use super::{ArtBrush, ArtScale, Direction, Error, MAX_PIECES, MAX_VERTICES, PatternBrush, PatternFit, Result, Scatter, native, normalize, widths};
use photocraft_doc::vector_design::appearance::StrokeItem;
use photocraft_doc::{Path, Subpath};
use photocraft_pathops::geom::{Point, Rect, Vec2};

#[derive(Default)]
struct Output {
    paths: Vec<Path>,
    vertices: usize,
}
impl Output {
    fn push(&mut self, path: Path) -> Result<()> {
        let n = path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
        if path.is_empty() {
            return Err(Error::Degenerate);
        }
        if self.paths.len() >= MAX_PIECES || self.vertices + n > MAX_VERTICES {
            return Err(Error::Budget);
        }
        self.vertices += n;
        self.paths.push(path);
        Ok(())
    }
}
fn transform(path: &Path, f: impl Fn(Point) -> Point) -> Path {
    let mut out = path.clone();
    for k in out.subpaths.iter_mut().flat_map(|s| &mut s.knots) {
        for p in [&mut k.anchor, &mut k.in_ctrl, &mut k.out_ctrl] {
            let q = f(Point::new(p.x, p.y));
            p.x = q.x;
            p.y = q.y;
        }
    }
    out
}
fn oriented(path: &Path, dir: Direction, along: bool, across: bool) -> Result<(Path, Rect)> {
    let path = transform(path, |p| {
        let (x, y) = match dir {
            Direction::LeftToRight => (p.x, p.y),
            Direction::RightToLeft => (-p.x, p.y),
            Direction::TopToBottom => (p.y, -p.x),
            Direction::BottomToTop => (-p.y, p.x),
        };
        Point::new(if along { -x } else { x }, if across { -y } else { y })
    });
    let b = photocraft_pathops::bounds(&path).map_err(|_| Error::Geometry)?.ok_or(Error::Degenerate)?;
    if b.2 - b.0 <= 1e-6 {
        return Err(Error::Degenerate);
    }
    Ok((path, Rect::new(b.0, b.1, b.2, b.3)))
}
#[derive(Clone, Copy)]
struct AlongMap {
    xs: [f64; 4],
    ss: [f64; 4],
}
impl AlongMap {
    fn linear(width: f64, s0: f64, s1: f64) -> Self {
        Self { xs: [0., 0., width, width], ss: [s0, s0, s1, s1] }
    }
    fn at(&self, x: f64) -> f64 {
        for i in 0..3 {
            if x <= self.xs[i + 1] && self.xs[i + 1] - self.xs[i] > 1e-12 {
                return self.ss[i] + (x - self.xs[i]) / (self.xs[i + 1] - self.xs[i]) * (self.ss[i + 1] - self.ss[i]);
            }
        }
        self.ss[3]
    }
    fn inverse(&self, s: f64) -> f64 {
        for i in 0..3 {
            if s <= self.ss[i + 1] && self.ss[i + 1] - self.ss[i] > 1e-12 {
                return self.xs[i] + (s - self.ss[i]) / (self.ss[i + 1] - self.ss[i]) * (self.xs[i + 1] - self.xs[i]);
            }
        }
        self.xs[3]
    }
    fn max_scale(&self) -> f64 {
        (0..3).filter(|&i| self.xs[i + 1] > self.xs[i]).map(|i| ((self.ss[i + 1] - self.ss[i]) / (self.xs[i + 1] - self.xs[i])).abs()).fold(1e-6, f64::max)
    }
}
fn bend(t: &Track, art: &Path, b: Rect, along: AlongMap, cross: f64, stroke: &StrokeItem, remaining: usize) -> Result<Path> {
    let source = tracks(art, 0.01 / along.max_scale().max(cross.abs()).clamp(1., 1000.))?;
    if source.len() != art.subpaths.len() {
        return Err(Error::Degenerate);
    }
    let mut subs = Vec::new();
    let mut count = 0;
    for (sp, track) in art.subpaths.iter().zip(source) {
        let n = track.segments();
        let mut dense = Vec::new();
        for i in 0..n {
            let (a, q) = (track.verts[i], track.verts[i + 1]);
            let steps = ((q.x - a.x).abs() * along.max_scale() / 1.5).ceil().max(1.) as usize;
            if steps > remaining || count + steps + 1 > remaining {
                return Err(Error::Budget);
            }
            let mut params: Vec<_> = (0..steps).map(|j| j as f64 / steps as f64).collect();
            if !track.closed && i + 1 == n {
                params.push(1.0);
            }
            // Prevent chords from cutting across an art fold or a guide breakpoint.
            if (q.x - a.x).abs() > 1e-12 {
                let mut split = along.xs.to_vec();
                split.extend(t.cum.iter().map(|s| along.inverse(*s)));
                if let Some(p) = &stroke.profile {
                    split.extend(p.points.iter().map(|p| along.inverse(p.t * t.len())));
                }
                for x in split {
                    let u = (x + b.x0 - a.x) / (q.x - a.x);
                    if u > 0. && u < 1. {
                        params.push(u);
                    }
                }
            }
            params.sort_by(f64::total_cmp);
            params.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
            if count + params.len() + 1 > remaining {
                return Err(Error::Budget);
            }
            for u in params {
                let p = a.lerp(q, u);
                let s = along.at(p.x - b.x0);
                let (l, r) = widths(stroke, s / t.len());
                let d = (p.y - b.center().y) * cross * (l + r) / f64::from(stroke.base.width) + (l - r) / 2.;
                dense.push(t.map(s, d));
                count += 1;
            }
        }
        let mut out: Subpath = native(&dense, true);
        out.op = sp.op;
        subs.push(out);
    }
    normalize(Path { subpaths: subs, fill_rule: art.fill_rule, inverted: false })
}
pub(super) fn art(a: &ArtBrush, ts: &[Track], stroke: &StrokeItem) -> Result<Vec<Path>> {
    let (art, b) = oriented(&a.art, a.direction, a.flip_along, a.flip_across)?;
    let mut out = Output::default();
    for t in ts {
        let l = t.len();
        let w = b.width();
        let cross = f64::from(stroke.base.width) * a.width;
        let (along, cross) = match a.scale {
            ArtScale::Stretch => (AlongMap::linear(w, 0., l), cross),
            ArtScale::Proportional => (AlongMap::linear(w, 0., l), l / w * a.width),
            ArtScale::BetweenGuides { start, end } => {
                let (g0, g1) = (start * w, end * w);
                let fixed = (g0 + w - g1) * cross;
                if fixed >= l {
                    (AlongMap::linear(w, 0., l), cross)
                } else {
                    (AlongMap { xs: [0., g0, g1, w], ss: [0., g0 * cross, l - (w - g1) * cross, l] }, cross)
                }
            }
        };
        out.push(bend(t, &art, b, along, cross, stroke, MAX_VERTICES - out.vertices)?)?;
    }
    Ok(out.paths)
}
fn place(art: &Path, b: Rect, p: Point, angle: f64, scale: f64) -> Path {
    let (sin, cos) = angle.sin_cos();
    transform(art, |q| {
        let d = (q - b.center()) * scale;
        p + Vec2::new(cos * d.x - sin * d.y, sin * d.x + cos * d.y)
    })
}
pub(super) fn scatter(sc: &Scatter, ts: &[Track], stroke: &StrokeItem, seed: u64) -> Result<Vec<Path>> {
    let (art, b) = oriented(&sc.art, Direction::LeftToRight, false, false)?;
    let mut rng = Rng::new(seed);
    let mut out = Output::default();
    let mut iterations = 0;
    for t in ts {
        let mut s = None;
        loop {
            iterations += 1;
            if iterations > MAX_PIECES + ts.len() {
                return Err(Error::Budget);
            }
            let size = rng.range(sc.size[0], sc.size[1]) * f64::from(stroke.base.width);
            let spacing = rng.range(sc.spacing[0], sc.spacing[1]);
            let step = b.width() * size * spacing;
            let pos = s.map_or(step / 2., |prev| prev + step);
            if pos > t.len() + 1e-9 || (t.closed && pos >= t.len() - 1e-9 && s.is_some()) {
                break;
            }
            s = Some(pos);
            let off = rng.range(sc.scatter[0], sc.scatter[1]) * b.height() * size;
            let rot = rng.range(sc.rotation[0], sc.rotation[1]).to_radians();
            let (p, tg) = t.frame(pos);
            let (l, r) = widths(stroke, pos / t.len());
            let pressure = (l + r) / f64::from(stroke.base.width);
            let base = if sc.relative_to_path { tg.y.atan2(tg.x) } else { 0. };
            if pressure > 1e-9 {
                out.push(place(&art, b, p + normal(tg) * (off * pressure + (l - r) / 2.), base - rot, size * pressure))?;
            }
        }
    }
    Ok(out.paths)
}
struct Tile {
    art: Path,
    bounds: Rect,
}
fn tile(p: &Path, pb: &PatternBrush) -> Result<Tile> {
    let (art, bounds) = oriented(p, Direction::LeftToRight, pb.flip_along, pb.flip_across)?;
    Ok(Tile { art, bounds })
}
fn optional(p: &Option<Path>, pb: &PatternBrush) -> Result<Option<Tile>> {
    p.as_ref().map(|p| tile(p, pb)).transpose()
}
fn tile_on(out: &mut Output, t: &Track, tile: &Tile, a: f64, b: f64, cross: f64, stroke: &StrokeItem) -> Result<()> {
    out.push(bend(t, &tile.art, tile.bounds, AlongMap::linear(tile.bounds.width(), a, b), cross, stroke, MAX_VERTICES - out.vertices)?)
}
pub(super) fn pattern(pb: &PatternBrush, ts: &[Track], stroke: &StrokeItem) -> Result<Vec<Path>> {
    let side = tile(&pb.side, pb)?;
    let outer = optional(&pb.outer_corner, pb)?;
    let inner = optional(&pb.inner_corner, pb)?;
    let start = optional(&pb.start, pb)?;
    let end = optional(&pb.end, pb)?;
    let k = f64::from(stroke.base.width) * pb.scale;
    let tw = side.bounds.width() * k;
    let gap = tw * pb.spacing;
    let mut out = Output::default();
    for t in ts {
        let l = t.len();
        let corners = t.corner_positions();
        let mut corner_half = 0_f64;
        for &(i, s) in &corners {
            let (a, b) = t.in_out(i);
            let tile = if a.cross(b) > 0. { inner.as_ref().or(outer.as_ref()) } else { outer.as_ref().or(inner.as_ref()) };
            if let Some(tile) = tile {
                corner_half = corner_half.max(tile.bounds.width() * k / 2.);
                let bis = a + b;
                let bis = if bis.hypot() < 1e-9 { a } else { bis / bis.hypot() };
                let (left, right) = widths(stroke, s / l);
                let pressure = (left + right) / f64::from(stroke.base.width);
                if pressure > 1e-9 {
                    out.push(place(&tile.art, tile.bounds, t.verts[i] + normal(bis) * (left - right) / 2., bis.y.atan2(bis.x), k * pressure))?;
                }
            }
        }
        let mut bounds: Vec<_> = corners.iter().map(|c| c.1).collect();
        let runs: Vec<(f64, f64, bool, bool)> = if bounds.is_empty() {
            vec![(0., l, false, false)]
        } else if t.closed {
            bounds.push(bounds[0] + l);
            bounds.windows(2).map(|w| (w[0], w[1], true, true)).collect()
        } else {
            let mut v = vec![0.];
            v.extend(bounds);
            v.push(l);
            let n = v.len();
            v.windows(2).enumerate().map(|(i, w)| (w[0], w[1], i > 0, i + 2 < n)).collect()
        };
        let nruns = runs.len();
        for (ri, (mut a, mut b, ca, cb)) in runs.into_iter().enumerate() {
            if ca {
                a += corner_half;
            }
            if cb {
                b -= corner_half;
            }
            if !t.closed
                && ri == 0
                && let Some(st) = &start
            {
                let w = st.bounds.width() * k;
                if b - a > w {
                    tile_on(&mut out, t, st, a, a + w, k, stroke)?;
                    a += w;
                }
            }
            if !t.closed
                && ri + 1 == nruns
                && let Some(en) = &end
            {
                let w = en.bounds.width() * k;
                if b - a > w {
                    tile_on(&mut out, t, en, b - w, b, k, stroke)?;
                    b -= w;
                }
            }
            let avail = b - a;
            if avail <= 1e-6 {
                continue;
            }
            let raw = (avail + gap) / (tw + gap);
            let n = match pb.fit {
                PatternFit::AddSpace => raw.floor(),
                _ => raw.round(),
            }
            .max(1.) as usize;
            if n + out.paths.len() > MAX_PIECES {
                return Err(Error::Budget);
            }
            let (w, g, lead) = match pb.fit {
                PatternFit::Stretch => {
                    let f = avail / (n as f64 * tw + (n - 1) as f64 * gap);
                    (tw * f, gap * f, 0.)
                }
                PatternFit::AddSpace => {
                    let g = if n > 1 { (avail - n as f64 * tw) / (n - 1) as f64 } else { 0. };
                    (tw, g.max(0.), if n == 1 { (avail - tw) / 2. } else { 0. })
                }
                PatternFit::Approximate => (tw, gap, (avail - (n as f64 * tw + (n - 1) as f64 * gap)) / 2.),
            };
            for i in 0..n {
                let s0 = a + lead + i as f64 * (w + g);
                tile_on(&mut out, t, &side, s0, s0 + w, k, stroke)?;
            }
        }
    }
    Ok(out.paths)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod dsfix_tests {
    use super::*;
    #[test]
    fn dsfix_bent_open_art_includes_its_last_corner() {
        let art = Path::new(vec![Subpath::polyline(&[(0., 0.), (0., 20.), (100., 20.)])]);
        let guide = Path::new(vec![Subpath::polyline(&[(0., 0.), (100., 0.)])]);
        let tracks = tracks(&guide, 0.01).unwrap();
        let out = bend(&tracks[0], &art, Rect::new(0., 0., 100., 20.), AlongMap::linear(100., 0., 100.), 1., &StrokeItem::default(), 1000).unwrap();
        let (_, _, x1, _) = photocraft_pathops::bounds(&out).unwrap().unwrap();
        assert!((x1 - 100.).abs() < 1e-6, "{x1}");
    }
}
