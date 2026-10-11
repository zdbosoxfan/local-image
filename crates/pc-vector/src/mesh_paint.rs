//! Gradient mesh geometry/editing adapted from VectorCraft crates/doc/src/live.rs,
//! pin d522c1d7be4035bd4f4a84cd6ebfca44f5155092 (MIT OR Apache-2.0).
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//! Unlike upstream's flat quads, sampling uses exact bicubic Coons geometry and
//! premultiplied linear-light colour. Envelope 20's private Catmull-Rom evaluator
//! is a different surface and is not copied here. Coordinates are document pixels.
use crate::{DEFAULT_TOLERANCE, Rasterizer, fill_rasterizer};
use photocraft_color::{
    PixelFormat,
    convert::{linear_to_srgb, srgb_to_linear},
};
use photocraft_doc::{Path, vector_design::exchange::VectorWarning};
use photocraft_geom::{Affine, Rect, TILE_SIZE};
use photocraft_raster::Surface;
use std::collections::BTreeMap;

pub const MAX_LINES: usize = 50;
pub const MAX_COORDINATE: f64 = 1_000_000.;
pub const RIGHT: usize = 0;
pub const LEFT: usize = 1;
pub const DOWN: usize = 2;
pub const UP: usize = 3;
/// Offsets from position, in right/left/down/up order. RGBA is straight sRGB.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshPoint {
    pub position: [f64; 2],
    pub handles: [[f64; 2]; 4],
    pub rgba: [f32; 4],
    pub opacity: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub rows: usize,
    pub cols: usize,
    pub points: Vec<MeshPoint>,
    pub dither: bool,
    pub seed: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    Flat,
    ToCenter,
    ToEdge,
}
type P = [f64; 2];
fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: P, t: f64) -> P {
    [a[0] * t, a[1] * t]
}
fn lerp(a: P, b: P, t: f64) -> P {
    add(mul(a, 1. - t), mul(b, t))
}
fn length(a: P) -> f64 {
    a[0].hypot(a[1])
}
fn finite(p: P) -> bool {
    p.iter().all(|v| v.is_finite() && v.abs() <= MAX_COORDINATE)
}
fn split(c: [P; 4], t: f64) -> ([P; 4], [P; 4]) {
    let a = lerp(c[0], c[1], t);
    let b = lerp(c[1], c[2], t);
    let d = lerp(c[2], c[3], t);
    let e = lerp(a, b, t);
    let f = lerp(b, d, t);
    let g = lerp(e, f, t);
    ([c[0], a, e, g], [g, f, d, c[3]])
}
fn cubic(c: [P; 4], t: f64) -> P {
    split(c, t).0[3]
}
fn derivative(c: [P; 4], t: f64) -> P {
    let a = mul(sub(c[1], c[0]), 3.);
    let b = mul(sub(c[2], c[1]), 3.);
    let d = mul(sub(c[3], c[2]), 3.);
    lerp(lerp(a, b, t), lerp(b, d, t), t)
}
/// Row-major tensor-product control net. Interior controls are implied by Coons boundaries.
#[derive(Clone, Debug)]
pub struct Patch {
    pub controls: [[P; 4]; 4],
    pub colors: [[f32; 4]; 4],
}
impl Patch {
    pub fn point(&self, u: f64, v: f64) -> P {
        cubic(self.controls.map(|row| cubic(row, u)), v)
    }
    fn derivatives(&self, u: f64, v: f64) -> (P, P) {
        (cubic(self.controls.map(|row| derivative(row, u)), v), derivative(self.controls.map(|row| cubic(row, u)), v))
    }
    /// Premultiplied linear RGBA, with bilinear corner weights.
    pub fn linear_rgba(&self, u: f64, v: f64) -> [f32; 4] {
        let weights = [(1. - u) * (1. - v), u * (1. - v), (1. - u) * v, u * v];
        std::array::from_fn(|ch| self.colors.iter().zip(weights).map(|(c, w)| c[ch] * w as f32).sum())
    }
    fn bounds(&self) -> [f64; 4] {
        self.controls
            .iter()
            .flatten()
            .fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])
    }
    fn split_u(&self, t: f64) -> (Self, Self) {
        let rows = self.controls.map(|c| split(c, t));
        (Self { controls: rows.map(|c| c.0), colors: self.colors }, Self { controls: rows.map(|c| c.1), colors: self.colors })
    }
    fn split_v(&self, t: f64) -> (Self, Self) {
        let cols: [[P; 4]; 4] = std::array::from_fn(|i| std::array::from_fn(|j| self.controls[j][i]));
        let cols = cols.map(|c| split(c, t));
        (
            Self { controls: std::array::from_fn(|j| std::array::from_fn(|i| cols[i].0[j])), colors: self.colors },
            Self { controls: std::array::from_fn(|j| std::array::from_fn(|i| cols[i].1[j])), colors: self.colors },
        )
    }
    fn inverse(&self, p: P, mut u: f64, mut v: f64) -> Option<(f64, f64)> {
        for _ in 0..16 {
            let delta = sub(self.point(u, v), p);
            if length(delta) < 1e-7 && (-1e-8..=1. + 1e-8).contains(&u) && (-1e-8..=1. + 1e-8).contains(&v) {
                return Some((u.clamp(0., 1.), v.clamp(0., 1.)));
            }
            let (a, b) = self.derivatives(u, v);
            let det = a[0] * b[1] - a[1] * b[0];
            if det.abs() < 1e-12 {
                return None;
            }
            u -= (delta[0] * b[1] - delta[1] * b[0]) / det;
            v -= (a[0] * delta[1] - a[1] * delta[0]) / det;
            if !u.is_finite() || !v.is_finite() || !(-0.25..=1.25).contains(&u) || !(-0.25..=1.25).contains(&v) {
                return None;
            }
        }
        None
    }
}
impl Mesh {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=MAX_LINES).contains(&self.rows) || !(1..=MAX_LINES).contains(&self.cols) || self.points.len() != (self.rows + 1) * (self.cols + 1) {
            return Err("mesh dimensions must be 1..50 patches with a complete point grid".into());
        }
        for p in &self.points {
            if !finite(p.position)
                || p.handles.iter().any(|h| !finite(*h) || !finite(add(p.position, *h)))
                || p.rgba.iter().any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
                || !p.opacity.is_finite()
                || !(0. ..=1.).contains(&p.opacity)
            {
                return Err("mesh points, handles, colours and opacity must be finite and bounded".into());
            }
        }
        Ok(())
    }
    pub fn index(&self, r: usize, c: usize) -> usize {
        r * (self.cols + 1) + c
    }
    pub fn regular(
        frame: [f64; 4],
        rows: usize,
        cols: usize,
        appearance: Appearance,
        highlight: f32,
        mut color: impl FnMut(P) -> [f32; 4],
    ) -> Result<Self, String> {
        if !(1..=MAX_LINES).contains(&rows)
            || !(1..=MAX_LINES).contains(&cols)
            || frame.iter().any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE)
            || frame[2] <= frame[0]
            || frame[3] <= frame[1]
            || !highlight.is_finite()
            || !(0. ..=100.).contains(&highlight)
        {
            return Err("invalid gradient mesh creation options".into());
        }
        let dx = (frame[2] - frame[0]) / cols as f64;
        let dy = (frame[3] - frame[1]) / rows as f64;
        let mut points = Vec::with_capacity((rows + 1) * (cols + 1));
        for r in 0..=rows {
            for c in 0..=cols {
                let p = [frame[0] + c as f64 * dx, frame[1] + r as f64 * dy];
                let mut rgba = color(p);
                let d = ((c as f32 / cols as f32 - 0.5).abs().max((r as f32 / rows as f32 - 0.5).abs())) * 2.;
                let w = match appearance {
                    Appearance::Flat => 0.,
                    Appearance::ToCenter => 1. - d,
                    Appearance::ToEdge => d,
                } * highlight
                    / 100.;
                for ch in &mut rgba[..3] {
                    *ch = linear_to_srgb(srgb_to_linear(*ch) * (1. - w) + w);
                }
                points.push(MeshPoint {
                    position: p,
                    handles: [
                        if c < cols { [dx / 3., 0.] } else { [0.; 2] },
                        if c > 0 { [-dx / 3., 0.] } else { [0.; 2] },
                        if r < rows { [0., dy / 3.] } else { [0.; 2] },
                        if r > 0 { [0., -dy / 3.] } else { [0.; 2] },
                    ],
                    rgba,
                    opacity: 1.,
                });
            }
        }
        let m = Self { rows, cols, points, dither: true, seed: 0 };
        m.validate()?;
        Ok(m)
    }
    fn edge(&self, r: usize, c: usize, vertical: bool) -> [P; 4] {
        let a = &self.points[self.index(r, c)];
        let b = &self.points[self.index(r + usize::from(vertical), c + usize::from(!vertical))];
        [a.position, add(a.position, a.handles[if vertical { DOWN } else { RIGHT }]), add(b.position, b.handles[if vertical { UP } else { LEFT }]), b.position]
    }
    fn patch_unchecked(&self, r: usize, c: usize) -> Patch {
        let top = self.edge(r, c, false);
        let bottom = self.edge(r + 1, c, false);
        let left = self.edge(r, c, true);
        let right = self.edge(r, c + 1, true);
        let controls = std::array::from_fn(|j| {
            std::array::from_fn(|i| {
                let u = i as f64 / 3.;
                let v = j as f64 / 3.;
                sub(add(lerp(top[i], bottom[i], v), lerp(left[j], right[j], u)), lerp(lerp(top[0], top[3], u), lerp(bottom[0], bottom[3], u), v))
            })
        });
        let colors = [self.index(r, c), self.index(r, c + 1), self.index(r + 1, c), self.index(r + 1, c + 1)].map(|i| {
            let p = &self.points[i];
            let a = p.rgba[3] * p.opacity;
            [srgb_to_linear(p.rgba[0]) * a, srgb_to_linear(p.rgba[1]) * a, srgb_to_linear(p.rgba[2]) * a, a]
        });
        Patch { controls, colors }
    }
    pub fn patch(&self, r: usize, c: usize) -> Result<Patch, String> {
        self.validate()?;
        if r >= self.rows || c >= self.cols {
            return Err("invalid mesh patch index".into());
        }
        Ok(self.patch_unchecked(r, c))
    }
    pub fn move_point(&mut self, index: usize, handle: Option<usize>, position: P) -> Result<(), String> {
        self.validate()?;
        if !finite(position) || index >= self.points.len() || handle.is_some_and(|h| h >= 4) {
            return Err("invalid mesh point or handle".into());
        }
        let mut next = self.clone();
        let p = &mut next.points[index];
        if let Some(h) = handle {
            p.handles[h] = sub(position, p.position);
        } else {
            p.position = position;
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
    pub fn set_color(&mut self, index: usize, rgba: [f32; 4], opacity: f32) -> Result<(), String> {
        self.validate()?;
        let mut next = self.clone();
        let p = next.points.get_mut(index).ok_or("invalid mesh point")?;
        p.rgba = rgba;
        p.opacity = opacity;
        next.validate()?;
        *self = next;
        Ok(())
    }
    fn interpolate_point(&self, r: usize, c: usize, u: f64, v: f64) -> MeshPoint {
        let p = self.patch_unchecked(r, c);
        let rgba = straight(p.linear_rgba(u, v));
        let corners = [self.index(r, c), self.index(r, c + 1), self.index(r + 1, c), self.index(r + 1, c + 1)];
        let w = [(1. - u) * (1. - v), u * (1. - v), (1. - u) * v, u * v];
        let opacity: f32 = corners.into_iter().zip(w).map(|(i, w)| self.points[i].opacity * w as f32).sum();
        let mut rgba = rgba;
        if opacity > 0. {
            rgba[3] /= opacity;
        } else {
            rgba[3] = 0.;
        }
        MeshPoint { position: p.point(u, v), rgba, opacity, handles: [[0.; 2]; 4] }
    }
    fn insert_row(&mut self, r: usize, v: f64) {
        let mut row = Vec::new();
        for c in 0..=self.cols {
            let (a, b) = split(self.edge(r, c, true), v);
            let pc = c.min(self.cols - 1);
            let u = if c == self.cols { 1. } else { 0. };
            let mut p = self.interpolate_point(r, pc, u, v);
            p.handles[DOWN] = sub(b[1], b[0]);
            p.handles[UP] = sub(a[2], a[3]);
            if c < self.cols {
                p.handles[RIGHT] = mul(self.patch_unchecked(r, c).derivatives(0., v).0, 1. / 3.);
            }
            if c > 0 {
                p.handles[LEFT] = mul(self.patch_unchecked(r, c - 1).derivatives(1., v).0, -1. / 3.);
            }
            row.push((p, sub(a[1], a[0]), sub(b[2], b[3])));
        }
        for (c, (_, down, up)) in row.iter().enumerate() {
            let i = self.index(r, c);
            let j = self.index(r + 1, c);
            self.points[i].handles[DOWN] = *down;
            self.points[j].handles[UP] = *up;
        }
        let at = self.index(r + 1, 0);
        self.points.splice(at..at, row.into_iter().map(|p| p.0));
        self.rows += 1;
    }
    fn insert_col(&mut self, c: usize, u: f64) {
        let mut col = Vec::new();
        for r in 0..=self.rows {
            let (a, b) = split(self.edge(r, c, false), u);
            let pr = r.min(self.rows - 1);
            let v = if r == self.rows { 1. } else { 0. };
            let mut p = self.interpolate_point(pr, c, u, v);
            p.handles[RIGHT] = sub(b[1], b[0]);
            p.handles[LEFT] = sub(a[2], a[3]);
            if r < self.rows {
                p.handles[DOWN] = mul(self.patch_unchecked(r, c).derivatives(u, 0.).1, 1. / 3.);
            }
            if r > 0 {
                p.handles[UP] = mul(self.patch_unchecked(r - 1, c).derivatives(u, 1.).1, -1. / 3.);
            }
            col.push((p, sub(a[1], a[0]), sub(b[2], b[3])));
        }
        let mut points = Vec::new();
        for (r, (p, right, left)) in col.into_iter().enumerate() {
            for cc in 0..=self.cols {
                let mut q = self.points[self.index(r, cc)].clone();
                if cc == c {
                    q.handles[RIGHT] = right;
                }
                if cc == c + 1 {
                    q.handles[LEFT] = left;
                }
                points.push(q);
                if cc == c {
                    points.push(p.clone());
                }
            }
        }
        self.cols += 1;
        self.points = points;
    }
    /// Add a row and column through a located patch coordinate. Shift passes no colour.
    pub fn insert(&mut self, r: usize, c: usize, u: f64, v: f64, color: Option<[f32; 4]>) -> Result<usize, String> {
        self.validate()?;
        if r >= self.rows || c >= self.cols || !u.is_finite() || !v.is_finite() || !(0. ..=1.).contains(&u) || !(0. ..=1.).contains(&v) {
            return Err("mesh insertion needs a located patch coordinate".into());
        }
        let row_new = (0.02..0.98).contains(&v);
        let col_new = (0.02..0.98).contains(&u);
        if (row_new && self.rows == MAX_LINES) || (col_new && self.cols == MAX_LINES) {
            return Err("mesh insertion exceeds the 50-line budget".into());
        }
        let mut next = self.clone();
        let rr = if row_new || v >= 0.98 { r + 1 } else { r };
        let cc = if col_new || u >= 0.98 { c + 1 } else { c };
        if row_new {
            next.insert_row(r, v);
        }
        if col_new {
            next.insert_col(c, u);
        }
        let index = next.index(rr, cc);
        if let Some(color) = color {
            next.points[index].rgba = color;
        }
        next.validate()?;
        *self = next;
        Ok(index)
    }
    /// Remove interior lines only; a corner/border cannot destroy the outer shape.
    pub fn delete(&mut self, index: usize) -> Result<(), String> {
        self.validate()?;
        if index >= self.points.len() {
            return Err("invalid mesh point".into());
        }
        let (r, c) = (index / (self.cols + 1), index % (self.cols + 1));
        if r == 0 || c == 0 || r == self.rows || c == self.cols {
            return Err("only interior mesh points can be deleted".into());
        }
        let w = self.cols + 1;
        self.points.drain(r * w..(r + 1) * w);
        self.rows -= 1;
        let mut i = 0;
        self.points.retain(|_| {
            let keep = i % w != c;
            i += 1;
            keep
        });
        self.cols -= 1;
        for rr in 0..=self.rows {
            let a = self.index(rr, c - 1);
            let b = self.index(rr, c);
            let d = mul(sub(self.points[b].position, self.points[a].position), 1. / 3.);
            self.points[a].handles[RIGHT] = d;
            self.points[b].handles[LEFT] = mul(d, -1.);
        }
        for cc in 0..=self.cols {
            let a = self.index(r - 1, cc);
            let b = self.index(r, cc);
            let d = mul(sub(self.points[b].position, self.points[a].position), 1. / 3.);
            self.points[a].handles[DOWN] = d;
            self.points[b].handles[UP] = mul(d, -1.);
        }
        Ok(())
    }
    pub fn transform(&self, a: Affine) -> Result<Self, String> {
        self.validate()?;
        if a.m.iter().any(|v| !v.is_finite()) || a.determinant().abs() < 1e-12 {
            return Err("invalid mesh transform".into());
        }
        let mut next = self.clone();
        for p in &mut next.points {
            let x = p.position;
            p.position = [a.m[0] * x[0] + a.m[2] * x[1] + a.m[4], a.m[1] * x[0] + a.m[3] * x[1] + a.m[5]];
            for h in &mut p.handles {
                let x = *h;
                *h = [a.m[0] * x[0] + a.m[2] * x[1], a.m[1] * x[0] + a.m[3] * x[1]];
            }
        }
        next.validate()?;
        Ok(next)
    }
}
fn straight(p: [f32; 4]) -> [f32; 4] {
    if p[3] <= 0. {
        return [0.; 4];
    }
    [linear_to_srgb(p[0] / p[3]), linear_to_srgb(p[1] / p[3]), linear_to_srgb(p[2] / p[3]), p[3]]
}
#[derive(Clone)]
struct Cell {
    patch: usize,
    bounds: [f64; 4],
    uv: [f64; 4],
}
/// Exact geometry subdivision for conservative candidate bounds. Shared global origins
/// make tile bands and scaled requests use identical deterministic gradient noise.
pub struct CompiledMesh {
    patches: Vec<Patch>,
    cells: Vec<Cell>,
    bins: BTreeMap<(i32, i32), Vec<usize>>,
    coverage: Rasterizer,
    bounds: Rect,
    dither: bool,
    seed: u32,
    cols: usize,
}
impl CompiledMesh {
    pub fn new(mesh: &Mesh, path: &Path, clip: Rect) -> Result<Self, String> {
        mesh.validate()?;
        if path.subpaths.iter().flat_map(|s| &s.knots).any(|k| ![k.anchor, k.in_ctrl, k.out_ctrl].iter().all(|p| finite([p.x, p.y]))) {
            return Err("nonfinite mesh clip path".into());
        }
        let coverage = fill_rasterizer(path, DEFAULT_TOLERANCE);
        let bounds = if path.inverted { clip } else { coverage.pixel_bounds().unwrap_or(Rect::EMPTY).intersect(&clip) };
        let mut out =
            Self { patches: Vec::new(), cells: Vec::new(), bins: BTreeMap::new(), coverage, bounds, dither: mesh.dither, seed: mesh.seed, cols: mesh.cols };
        let mut references = 0usize;
        for r in 0..mesh.rows {
            for c in 0..mesh.cols {
                let p = mesh.patch_unchecked(r, c);
                let index = out.patches.len();
                let (a, b) = p.split_u(0.5);
                let (a, b, c, d) = (a.split_v(0.5).0, a.split_v(0.5).1, b.split_v(0.5).0, b.split_v(0.5).1);
                for (sub, uv) in [(a, [0., 0., 0.5, 0.5]), (b, [0., 0.5, 0.5, 1.]), (c, [0.5, 0., 1., 0.5]), (d, [0.5, 0.5, 1., 1.])] {
                    let bounds = sub.bounds();
                    let rect =
                        Rect::new(bounds[0].floor() as i32, bounds[1].floor() as i32, bounds[2].ceil() as i32, bounds[3].ceil() as i32).intersect(&out.bounds);
                    let cell = out.cells.len();
                    out.cells.push(Cell { patch: index, bounds, uv });
                    if !rect.is_empty() {
                        let count = (i64::from((rect.x1 - 1).div_euclid(32) - rect.x0.div_euclid(32) + 1)
                            * i64::from((rect.y1 - 1).div_euclid(32) - rect.y0.div_euclid(32) + 1)) as usize;
                        references = references.saturating_add(count);
                        if references > 1_048_576 {
                            return Err("mesh spatial cache exceeds its one-million-reference budget".into());
                        }
                        for y in rect.y0.div_euclid(32)..=(rect.y1 - 1).div_euclid(32) {
                            for x in rect.x0.div_euclid(32)..=(rect.x1 - 1).div_euclid(32) {
                                out.bins.entry((x, y)).or_default().push(cell);
                            }
                        }
                    }
                }
                out.patches.push(p);
            }
        }
        Ok(out)
    }
    pub fn bounds(&self) -> Rect {
        self.bounds
    }
    pub fn cache_bytes(&self, format: PixelFormat) -> u128 {
        if self.bounds.is_empty() {
            return 0;
        }
        let b = self.bounds;
        let n = |lo: i32, hi: i32| i128::from((hi - 1).div_euclid(TILE_SIZE) - lo.div_euclid(TILE_SIZE) + 1) as u128;
        n(b.x0, b.x1) * n(b.y0, b.y1) * TILE_SIZE as u128 * TILE_SIZE as u128 * format.channels() as u128 * 4
    }
    pub fn locate(&self, p: P) -> Option<(usize, usize, f64, f64)> {
        if p.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let cells = self.bins.get(&((p[0].floor() as i32).div_euclid(32), (p[1].floor() as i32).div_euclid(32)))?;
        for &i in cells.iter().rev() {
            let cell = &self.cells[i];
            let b = cell.bounds;
            if p[0] < b[0] - 1e-8 || p[0] > b[2] + 1e-8 || p[1] < b[1] - 1e-8 || p[1] > b[3] + 1e-8 {
                continue;
            }
            let uv = cell.uv;
            if let Some((u, v)) = self.patches[cell.patch].inverse(p, (uv[0] + uv[2]) / 2., (uv[1] + uv[3]) / 2.) {
                return Some((cell.patch / self.cols, cell.patch % self.cols, u, v));
            }
        }
        None
    }
    pub fn at(&self, x: f64, y: f64) -> [f32; 4] {
        let Some((r, c, u, v)) = self.locate([x, y]) else {
            return [0.; 4];
        };
        let mut p = straight(self.patches[r * self.cols + c].linear_rgba(u, v));
        if self.dither {
            let dx = self.seed as i32;
            let dy = self.seed.rotate_left(16) as i32;
            let n = (photocraft_color::dither_noise((x.floor() as i32).wrapping_add(dx), (y.floor() as i32).wrapping_add(dy)) - 0.5) / 255.;
            for ch in &mut p[..3] {
                *ch = (*ch + n).clamp(0., 1.);
            }
        }
        p
    }
    pub fn render_rgba(&self, rect: Rect) -> Result<Vec<[f32; 4]>, String> {
        if rect.is_empty() {
            return Ok(Vec::new());
        }
        let n = rect.width() as u64 * rect.height() as u64;
        if n > 16_777_216 {
            return Err("mesh render request exceeds pixel budget".into());
        }
        let coverage = self.coverage.render(rect);
        let w = rect.width() as usize;
        let mut out = vec![[0.; 4]; n as usize];
        for (i, (p, a)) in out.iter_mut().zip(coverage).enumerate() {
            if a > 0. {
                let x = rect.x0 + (i % w) as i32;
                let y = rect.y0 + (i / w) as i32;
                if x >= self.bounds.x0 && y >= self.bounds.y0 && x < self.bounds.x1 && y < self.bounds.y1 {
                    *p = self.at(f64::from(x) + 0.5, f64::from(y) + 0.5);
                    p[3] *= a;
                }
            }
        }
        Ok(out)
    }
    pub fn render(&self, format: PixelFormat) -> Result<Surface, String> {
        if self.cache_bytes(format) > 256 << 20 {
            return Err("mesh render exceeds 256 MiB cache budget".into());
        }
        let mut s = Surface::new(format);
        let ch = format.channels();
        for band in crate::tile_bands(self.bounds) {
            let rgba = self.render_rgba(band)?;
            let mut values = vec![0.; rgba.len() * ch];
            for (p, o) in rgba.into_iter().zip(values.chunks_exact_mut(ch)) {
                photocraft_raster::from_rgba_into(&format, p, o);
            }
            for (rect, values) in crate::band_tiles(band, &values, ch, |p| if format.alpha { p[ch - 1] > 0. } else { p.iter().any(|v| *v != 0.) }) {
                s.write_region(rect, &values);
            }
        }
        Ok(s)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Pdf,
    Svg,
    Psd,
}
/// krilla 0.8.2 has linear/radial/sweep/pattern paint, no type 6/7 public shading API.
/// A future provider must also represent linear-light colour, dither and point alpha.
pub fn raster_fallback_warning(warnings: &mut Vec<VectorWarning>, format: ExportFormat) {
    if warnings.iter().any(|w| w.feature == "gradient_mesh") {
        return;
    }
    warnings.push(VectorWarning {
        feature: "gradient_mesh".into(),
        object: None,
        message: format!("Gradient mesh paint was rasterized for {format:?}; mesh points lost editability."),
    });
}
#[cfg(test)]
#[path = "../tests/mesh_paint/tests.rs"]
mod tests;

impl From<&photocraft_doc::vector_design::appearance::GradientMesh> for Mesh {
    fn from(mesh: &photocraft_doc::vector_design::appearance::GradientMesh) -> Self {
        Self {
            rows: mesh.rows,
            cols: mesh.cols,
            dither: mesh.dither,
            seed: mesh.seed,
            points: mesh.points.iter().map(|p| MeshPoint { position: p.position, handles: p.handles, rgba: p.rgba, opacity: p.opacity }).collect(),
        }
    }
}
impl From<&Mesh> for photocraft_doc::vector_design::appearance::GradientMesh {
    fn from(mesh: &Mesh) -> Self {
        Self {
            rows: mesh.rows,
            cols: mesh.cols,
            dither: mesh.dither,
            seed: mesh.seed,
            points: mesh
                .points
                .iter()
                .map(|p| photocraft_doc::vector_design::appearance::MeshPoint { position: p.position, handles: p.handles, rgba: p.rgba, opacity: p.opacity })
                .collect(),
        }
    }
}
