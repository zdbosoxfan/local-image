//! DNG opcode lists (`OpcodeList1/2/3`, DNG 1.7 chapter 7). Opcode lists are always big-endian.
//!
//! Parsed: every opcode (unknown ones are kept as [`Opcode::Unknown`]). Applied: `WarpRectilinear`, `WarpFisheye`,
//! `FixVignetteRadial`, `FixBadPixelsConstant`, `FixBadPixelsList`, `MapTable`, `MapPolynomial`, `GainMap`,
//! `DeltaPerRow/Column`, `ScalePerRow/Column`. `TrimBounds` is recorded but not applied.
//! Geometric warps are applied to the demosaiced image in list 3 only.
//!
//! Value convention: list 1 runs on raw sample values (16-bit scale), lists 2 and 3 on values normalised to
//! [0, 1]; table/polynomial/delta opcodes are defined on the normalised range and are rescaled accordingly.

use crate::{Cfa, RgbFrame};
use serde::{Deserialize, Serialize};

/// Rectangular area + plane selection + pitch shared by the "area" opcodes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Area {
    pub top: u32,
    pub left: u32,
    pub bottom: u32,
    pub right: u32,
    pub plane: u32,
    pub planes: u32,
    pub row_pitch: u32,
    pub col_pitch: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Opcode {
    /// Per-plane radial (kr0..kr3) + tangential (kt0, kt1) coefficients and the relative optical centre.
    WarpRectilinear {
        planes: Vec<[f64; 6]>,
        center: [f64; 2],
    },
    WarpFisheye {
        planes: Vec<[f64; 4]>,
        center: [f64; 2],
    },
    FixVignetteRadial {
        k: [f64; 5],
        center: [f64; 2],
    },
    FixBadPixelsConstant {
        constant: u32,
        bayer_phase: u32,
    },
    FixBadPixelsList {
        bayer_phase: u32,
        points: Vec<(u32, u32)>,
        rects: Vec<[u32; 4]>,
    },
    TrimBounds {
        top: u32,
        left: u32,
        bottom: u32,
        right: u32,
    },
    MapTable {
        area: Area,
        table: Vec<u16>,
    },
    MapPolynomial {
        area: Area,
        coefficients: Vec<f64>,
    },
    GainMap {
        area: Area,
        points_v: u32,
        points_h: u32,
        spacing: [f64; 2],
        origin: [f64; 2],
        map_planes: u32,
        gains: Vec<f32>,
    },
    DeltaPerRow {
        area: Area,
        deltas: Vec<f32>,
    },
    DeltaPerColumn {
        area: Area,
        deltas: Vec<f32>,
    },
    ScalePerRow {
        area: Area,
        scales: Vec<f32>,
    },
    ScalePerColumn {
        area: Area,
        scales: Vec<f32>,
    },
    Unknown {
        id: u32,
        flags: u32,
        params: Vec<u8>,
    },
}

impl Opcode {
    /// Whether [`apply_list`] / [`apply_list3`] implement this opcode. The two
    /// geometric warp opcodes are implemented only by [`apply_list3`].
    pub fn is_applied(&self) -> bool {
        !matches!(self, Opcode::Unknown { .. } | Opcode::TrimBounds { .. })
    }
}

/// The three opcode lists of a DNG raw IFD.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OpcodeLists {
    pub list1: Vec<Opcode>,
    pub list2: Vec<Opcode>,
    pub list3: Vec<Opcode>,
}

struct Rd<'a> {
    d: &'a [u8],
    p: usize,
}

impl Rd<'_> {
    fn u32(&mut self) -> Option<u32> {
        let v = u32::from_be_bytes(self.d.get(self.p..self.p + 4)?.try_into().ok()?);
        self.p += 4;
        Some(v)
    }
    fn u16(&mut self) -> Option<u16> {
        let v = u16::from_be_bytes(self.d.get(self.p..self.p + 2)?.try_into().ok()?);
        self.p += 2;
        Some(v)
    }
    fn f64(&mut self) -> Option<f64> {
        let v = f64::from_be_bytes(self.d.get(self.p..self.p + 8)?.try_into().ok()?);
        self.p += 8;
        Some(v)
    }
    fn f32(&mut self) -> Option<f32> {
        let v = f32::from_be_bytes(self.d.get(self.p..self.p + 4)?.try_into().ok()?);
        self.p += 4;
        Some(v)
    }
    fn area(&mut self) -> Option<Area> {
        Some(Area {
            top: self.u32()?,
            left: self.u32()?,
            bottom: self.u32()?,
            right: self.u32()?,
            plane: self.u32()?,
            planes: self.u32()?,
            row_pitch: self.u32()?.max(1),
            col_pitch: self.u32()?.max(1),
        })
    }
    fn remaining(&self) -> usize {
        self.d.len().saturating_sub(self.p)
    }
}

fn parse_one(id: u32, flags: u32, params: &[u8]) -> Option<Opcode> {
    let mut r = Rd { d: params, p: 0 };
    let f = |r: &mut Rd| r.f64().filter(|v| v.is_finite());
    Some(match id {
        1 => {
            let n = r.u32()? as usize;
            if n == 0 || n > 4 {
                return None;
            }
            let mut planes = Vec::with_capacity(n);
            for _ in 0..n {
                planes.push([f(&mut r)?, f(&mut r)?, f(&mut r)?, f(&mut r)?, f(&mut r)?, f(&mut r)?]);
            }
            Opcode::WarpRectilinear { planes, center: [f(&mut r)?, f(&mut r)?] }
        }
        2 => {
            let n = r.u32()? as usize;
            if n == 0 || n > 4 {
                return None;
            }
            let mut planes = Vec::with_capacity(n);
            for _ in 0..n {
                planes.push([f(&mut r)?, f(&mut r)?, f(&mut r)?, f(&mut r)?]);
            }
            Opcode::WarpFisheye { planes, center: [f(&mut r)?, f(&mut r)?] }
        }
        3 => Opcode::FixVignetteRadial { k: [f(&mut r)?, f(&mut r)?, f(&mut r)?, f(&mut r)?, f(&mut r)?], center: [f(&mut r)?, f(&mut r)?] },
        4 => Opcode::FixBadPixelsConstant { constant: r.u32()?, bayer_phase: r.u32()? },
        5 => {
            let bayer_phase = r.u32()?;
            let (np, nr) = (r.u32()? as usize, r.u32()? as usize);
            if np.saturating_mul(8).saturating_add(nr.saturating_mul(16)) > r.remaining() {
                return None;
            }
            let points = (0..np).map(|_| Some((r.u32()?, r.u32()?))).collect::<Option<_>>()?;
            let rects = (0..nr).map(|_| Some([r.u32()?, r.u32()?, r.u32()?, r.u32()?])).collect::<Option<_>>()?;
            Opcode::FixBadPixelsList { bayer_phase, points, rects }
        }
        6 => Opcode::TrimBounds { top: r.u32()?, left: r.u32()?, bottom: r.u32()?, right: r.u32()? },
        7 => {
            let area = r.area()?;
            let n = r.u32()? as usize;
            if n == 0 || n > 65536 || n * 2 > r.remaining() {
                return None;
            }
            Opcode::MapTable { area, table: (0..n).map(|_| r.u16()).collect::<Option<_>>()? }
        }
        8 => {
            let area = r.area()?;
            let deg = r.u32()? as usize;
            if deg > 8 {
                return None;
            }
            Opcode::MapPolynomial { area, coefficients: (0..=deg).map(|_| f(&mut r)).collect::<Option<_>>()? }
        }
        9 => {
            let area = r.area()?;
            let (pv, ph) = (r.u32()?, r.u32()?);
            let mut spacing = [f(&mut r)?, f(&mut r)?];
            let mut origin = [f(&mut r)?, f(&mut r)?];
            // Adobe SDK treats a singleton axis as constant regardless of its
            // recorded spacing/origin (some cameras write zero spacing here).
            for (axis, points) in [pv, ph].into_iter().enumerate() {
                if points == 1 {
                    spacing[axis] = 1.0;
                    origin[axis] = 0.0;
                }
            }
            let mp = r.u32()?;
            let n = (pv as usize).checked_mul(ph as usize)?.checked_mul(mp as usize)?;
            if n == 0 || mp == 0 || spacing.iter().any(|v| *v <= 0.0) || n.checked_mul(4)? > r.remaining() {
                return None;
            }
            let gains = (0..n).map(|_| r.f32().filter(|v| v.is_finite() && *v >= 0.0)).collect::<Option<_>>()?;
            Opcode::GainMap { area, points_v: pv, points_h: ph, spacing, origin, map_planes: mp, gains }
        }
        10..=13 => {
            let area = r.area()?;
            let n = r.u32()? as usize;
            if n * 4 > r.remaining() {
                return None;
            }
            // deltas are signed offsets (row/column bias corrections); only scales must be non-negative
            let valid = |v: &f32| v.is_finite() && (id <= 11 || *v >= 0.0);
            let v: Vec<f32> = (0..n).map(|_| r.f32().filter(valid)).collect::<Option<_>>()?;
            match id {
                10 => Opcode::DeltaPerRow { area, deltas: v },
                11 => Opcode::DeltaPerColumn { area, deltas: v },
                12 => Opcode::ScalePerRow { area, scales: v },
                _ => Opcode::ScalePerColumn { area, scales: v },
            }
        }
        _ => Opcode::Unknown { id, flags, params: params.to_vec() },
    })
}

fn put_area(o: &mut Vec<u8>, a: &Area) {
    for v in [a.top, a.left, a.bottom, a.right, a.plane, a.planes, a.row_pitch, a.col_pitch] {
        o.extend_from_slice(&v.to_be_bytes());
    }
}

fn params(op: &Opcode) -> (u32, u32, Vec<u8>) {
    let mut o = Vec::new();
    let f64s = |o: &mut Vec<u8>, v: &[f64]| v.iter().for_each(|x| o.extend_from_slice(&x.to_be_bytes()));
    let u32s = |o: &mut Vec<u8>, v: &[u32]| v.iter().for_each(|x| o.extend_from_slice(&x.to_be_bytes()));
    let f32s = |o: &mut Vec<u8>, v: &[f32]| v.iter().for_each(|x| o.extend_from_slice(&x.to_be_bytes()));
    let id = match op {
        Opcode::WarpRectilinear { planes, center } => {
            u32s(&mut o, &[planes.len() as u32]);
            planes.iter().for_each(|p| f64s(&mut o, p));
            f64s(&mut o, center);
            1
        }
        Opcode::WarpFisheye { planes, center } => {
            u32s(&mut o, &[planes.len() as u32]);
            planes.iter().for_each(|p| f64s(&mut o, p));
            f64s(&mut o, center);
            2
        }
        Opcode::FixVignetteRadial { k, center } => {
            f64s(&mut o, k);
            f64s(&mut o, center);
            3
        }
        Opcode::FixBadPixelsConstant { constant, bayer_phase } => {
            u32s(&mut o, &[*constant, *bayer_phase]);
            4
        }
        Opcode::FixBadPixelsList { bayer_phase, points, rects } => {
            u32s(&mut o, &[*bayer_phase, points.len() as u32, rects.len() as u32]);
            points.iter().for_each(|&(r, c)| u32s(&mut o, &[r, c]));
            rects.iter().for_each(|r| u32s(&mut o, r));
            5
        }
        Opcode::TrimBounds { top, left, bottom, right } => {
            u32s(&mut o, &[*top, *left, *bottom, *right]);
            6
        }
        Opcode::MapTable { area, table } => {
            put_area(&mut o, area);
            u32s(&mut o, &[table.len() as u32]);
            table.iter().for_each(|v| o.extend_from_slice(&v.to_be_bytes()));
            7
        }
        Opcode::MapPolynomial { area, coefficients } => {
            put_area(&mut o, area);
            u32s(&mut o, &[coefficients.len().saturating_sub(1) as u32]);
            f64s(&mut o, coefficients);
            8
        }
        Opcode::GainMap { area, points_v, points_h, spacing, origin, map_planes, gains } => {
            put_area(&mut o, area);
            u32s(&mut o, &[*points_v, *points_h]);
            f64s(&mut o, spacing);
            f64s(&mut o, origin);
            u32s(&mut o, &[*map_planes]);
            f32s(&mut o, gains);
            9
        }
        Opcode::DeltaPerRow { area, deltas: v }
        | Opcode::DeltaPerColumn { area, deltas: v }
        | Opcode::ScalePerRow { area, scales: v }
        | Opcode::ScalePerColumn { area, scales: v } => {
            put_area(&mut o, area);
            u32s(&mut o, &[v.len() as u32]);
            f32s(&mut o, v);
            match op {
                Opcode::DeltaPerRow { .. } => 10,
                Opcode::DeltaPerColumn { .. } => 11,
                Opcode::ScalePerRow { .. } => 12,
                _ => 13,
            }
        }
        Opcode::Unknown { id, flags, params } => return (*id, *flags, params.clone()),
    };
    (id, 0, o)
}

/// Serialise an opcode list (inverse of [`parse_list`]).
pub fn write_list(list: &[Opcode]) -> Vec<u8> {
    let mut out = (list.len() as u32).to_be_bytes().to_vec();
    for op in list {
        let (id, flags, p) = params(op);
        for v in [id, 0x0103_0000, flags, p.len() as u32] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        out.extend_from_slice(&p);
    }
    out
}

/// Parse an opcode list blob. Malformed opcodes become [`Opcode::Unknown`]; a truncated list stops early.
pub fn parse_list(d: &[u8]) -> Vec<Opcode> {
    let mut r = Rd { d, p: 0 };
    let Some(count) = r.u32() else { return vec![] };
    let mut out = Vec::new();
    for _ in 0..count.min(4096) {
        let (Some(id), Some(_ver), Some(flags), Some(size)) = (r.u32(), r.u32(), r.u32(), r.u32()) else { break };
        let Some(params) = d.get(r.p..r.p.saturating_add(size as usize)) else { break };
        r.p += size as usize;
        out.push(parse_one(id, flags, params).unwrap_or(Opcode::Unknown { id, flags, params: params.to_vec() }));
    }
    out
}

/// Run `f(x, y, plane, sample)` on every sample `a` selects in an interleaved `w × h × cpp` buffer, rows
/// in parallel (the area opcodes are pointwise).
fn for_area(buf: &mut [f32], w: usize, h: usize, cpp: usize, a: &Area, f: impl Fn(usize, usize, usize, &mut f32) + Sync) {
    use rayon::prelude::*;
    let (t, l) = (a.top as usize, a.left as usize);
    let (b, r) = ((a.bottom as usize).min(h), (a.right as usize).min(w));
    let (p0, p1) = (a.plane as usize, (a.plane as usize).saturating_add(a.planes.max(1) as usize).min(cpp));
    let (row_pitch, col_pitch) = (a.row_pitch.max(1) as usize, a.col_pitch.max(1) as usize);
    if t >= b || l >= r || p0 >= p1 || w == 0 || cpp == 0 {
        return;
    }
    buf.par_chunks_mut(w * cpp).enumerate().skip(t).take(b - t).filter(|(y, _)| (y - t).is_multiple_of(row_pitch)).for_each(|(y, row)| {
        for x in (l..r).step_by(col_pitch) {
            for p in p0..p1 {
                if let Some(v) = row.get_mut(x * cpp + p) {
                    f(x, y, p, v);
                }
            }
        }
    });
}

fn gain_at(points_v: u32, points_h: u32, spacing: [f64; 2], origin: [f64; 2], map_planes: u32, gains: &[f32], rv: f64, rh: f64, plane: usize) -> f32 {
    let (pv, ph) = (points_v as usize, points_h as usize);
    let mp = map_planes as usize;
    let p = plane.min(mp - 1);
    let fy = if spacing[0] > 0.0 { ((rv - origin[0]) / spacing[0]).clamp(0.0, (pv - 1) as f64) } else { 0.0 };
    let fx = if spacing[1] > 0.0 { ((rh - origin[1]) / spacing[1]).clamp(0.0, (ph - 1) as f64) } else { 0.0 };
    let (y0, x0) = (fy.floor() as usize, fx.floor() as usize);
    let (y1, x1) = ((y0 + 1).min(pv - 1), (x0 + 1).min(ph - 1));
    let (ty, tx) = ((fy - y0 as f64) as f32, (fx - x0 as f64) as f32);
    let g = |y: usize, x: usize| gains[(y * ph + x) * mp + p];
    let top = g(y0, x0) + (g(y0, x1) - g(y0, x0)) * tx;
    let bot = g(y1, x0) + (g(y1, x1) - g(y1, x0)) * tx;
    top + (bot - top) * ty
}

/// Radius normaliser: distance from the centre to the farthest image corner.
fn max_radius(cx: f64, cy: f64, w: f64, h: f64) -> f64 {
    [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)].iter().map(|&(x, y)| (x - cx).hypot(y - cy)).fold(0.0, f64::max).max(1e-9)
}

/// Most pixels one bad-pixel opcode repairs (the list is 64 MB, `fix_pixels`' set about twice that). Uncapped, a
/// mosaic equal to `FixBadPixelsConstant`'s constant, or one `FixBadPixelsList` rectangle over the image, lists
/// every pixel: tens of GB at [`crate::MAX_SAMPLES`]. Pixels past the cap, in list order, are left as they are.
const MAX_BAD_PIXELS: usize = 1 << 22;

/// Apply a list to an interleaved `w × h × cpp` buffer. `cfa` (when the data is a mosaic) is used by the
/// bad-pixel opcodes. `scale` is the value of 1.0 in the buffer: 65535 for list 1 (raw 16-bit values), 1 for
/// lists 2 and 3 (normalised values).
pub fn apply_list(list: &[Opcode], buf: &mut [f32], w: usize, h: usize, cpp: usize, cfa: Option<&Cfa>, scale: f32) {
    for op in list {
        match op {
            Opcode::MapTable { area, table } => {
                let n = table.len();
                for_area(buf, w, h, cpp, area, |_, _, _, v| {
                    let idx = ((*v / scale * 65535.0).round().max(0.0) as usize).min(n - 1);
                    *v = table[idx] as f32 / 65535.0 * scale;
                });
            }
            Opcode::MapPolynomial { area, coefficients } => {
                for_area(buf, w, h, cpp, area, |_, _, _, v| {
                    let t = (*v / scale) as f64;
                    let r = coefficients.iter().rev().fold(0.0, |acc, &c| acc * t + c);
                    *v = r as f32 * scale;
                });
            }
            Opcode::GainMap { area, points_v, points_h, spacing, origin, map_planes, gains } => {
                for_area(buf, w, h, cpp, area, |x, y, p, v| {
                    // Adobe dng_gain_map_interpolator evaluates pixel centres, not corners.
                    let rv = (y as f64 + 0.5) / h as f64;
                    let rh = (x as f64 + 0.5) / w as f64;
                    *v *= gain_at(*points_v, *points_h, *spacing, *origin, *map_planes, gains, rv, rh, p - area.plane as usize);
                });
            }
            Opcode::DeltaPerRow { area, deltas } | Opcode::DeltaPerColumn { area, deltas } => {
                let per_row = matches!(op, Opcode::DeltaPerRow { .. });
                for_area(buf, w, h, cpp, area, |x, y, _, v| {
                    let i =
                        if per_row { (y - area.top as usize) / area.row_pitch as usize } else { (x - area.left as usize) / area.col_pitch as usize };
                    if let Some(d) = deltas.get(i) {
                        *v += d * scale;
                    }
                });
            }
            Opcode::ScalePerRow { area, scales } | Opcode::ScalePerColumn { area, scales } => {
                let per_row = matches!(op, Opcode::ScalePerRow { .. });
                for_area(buf, w, h, cpp, area, |x, y, _, v| {
                    let i =
                        if per_row { (y - area.top as usize) / area.row_pitch as usize } else { (x - area.left as usize) / area.col_pitch as usize };
                    if let Some(s) = scales.get(i) {
                        *v *= s;
                    }
                });
            }
            Opcode::FixVignetteRadial { k, center } => vignette(buf, w, h, cpp, k, *center, Binning::none(w, h)),
            Opcode::FixBadPixelsConstant { constant, .. } => {
                let c = (*constant as f64 / 65535.0 * scale as f64) as f32;
                let bad: Vec<(usize, usize)> =
                    (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| cpp == 1 && buf[y * w + x] == c).take(MAX_BAD_PIXELS).collect();
                fix_pixels(buf, w, h, cpp, cfa, &bad);
            }
            Opcode::FixBadPixelsList { points, rects, .. } => {
                let mut bad: Vec<(usize, usize)> =
                    points.iter().map(|&(r, c)| (c as usize, r as usize)).filter(|&(x, y)| x < w && y < h).take(MAX_BAD_PIXELS).collect();
                'rects: for r in rects {
                    let [t, l, b, rr] = r.map(|v| v as usize);
                    for y in t..b.min(h) {
                        for x in l..rr.min(w) {
                            if bad.len() >= MAX_BAD_PIXELS {
                                break 'rects;
                            }
                            bad.push((x, y));
                        }
                    }
                }
                fix_pixels(buf, w, h, cpp, cfa, &bad);
            }
            _ => {}
        }
    }
}

/// Replace listed pixels with the mean of the nearest same-colour neighbours that are not themselves bad.
fn fix_pixels(buf: &mut [f32], w: usize, h: usize, cpp: usize, cfa: Option<&Cfa>, bad: &[(usize, usize)]) {
    if bad.is_empty() {
        return;
    }
    let mut is_bad = std::collections::HashSet::with_capacity(bad.len());
    for &b in bad {
        is_bad.insert(b);
    }
    let step: isize = match cfa {
        Some(c) if c.width == 2 && c.height == 2 => 2,
        Some(c) => c.width.max(c.height) as isize,
        None => 1,
    };
    for &(x, y) in bad {
        for p in 0..cpp {
            let mut sum = 0.0;
            let mut n = 0;
            for (dx, dy) in [(-step, 0), (step, 0), (0, -step), (0, step), (-step, -step), (step, step), (-step, step), (step, -step)] {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize || is_bad.contains(&(nx as usize, ny as usize)) {
                    continue;
                }
                sum += buf[(ny as usize * w + nx as usize) * cpp + p];
                n += 1;
            }
            if n > 0 {
                buf[(y * w + x) * cpp + p] = sum / n as f32;
            }
        }
    }
}

/// `FixVignetteRadial` on an interleaved `w × h × cpp` buffer whose pixels stand for the sensor
/// pixels `b` says.
fn vignette(buf: &mut [f32], w: usize, h: usize, cpp: usize, k: &[f64; 5], center: [f64; 2], b: Binning) {
    use rayon::prelude::*;
    let (sw, sh) = (b.sensor[0] as f64, b.sensor[1] as f64);
    let (cx, cy) = (center[0] * sw, center[1] * sh);
    let m = max_radius(cx, cy, sw, sh);
    buf.par_chunks_mut((w * cpp).max(1)).take(h).enumerate().for_each(|(y, row)| {
        let dy = b.sensor_at(y as f64, 1) + 0.5 - cy;
        for (x, px) in row.chunks_exact_mut(cpp.max(1)).enumerate() {
            let r2 = ((b.sensor_at(x as f64, 0) + 0.5 - cx).powi(2) + dy.powi(2)) / (m * m);
            let g = 1.0 + r2 * (k[0] + r2 * (k[1] + r2 * (k[2] + r2 * (k[3] + r2 * k[4]))));
            for v in px {
                *v *= g as f32;
            }
        }
    });
}

/// Apply `OpcodeList3` (on the demosaiced RGB image, values normalised to [0, 1]).
pub fn apply_list3(list: &[Opcode], img: &mut RgbFrame) {
    let b = Binning::none(img.width, img.height);
    apply_list3_binned(list, img, b);
}

/// Where the pixels of an image binned from the active area sit on it: pixel `x` stands for the
/// `k` sensor pixels from `origin[0] + k·x` (and likewise down). `OpcodeList3` is defined on the
/// whole active area, `sensor` pixels wide and high.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binning {
    pub k: usize,
    pub origin: [usize; 2],
    pub sensor: [usize; 2],
}

impl Binning {
    /// A `w × h` image that is the active area itself.
    pub fn none(w: usize, h: usize) -> Binning {
        Binning { k: 1, origin: [0, 0], sensor: [w, h] }
    }

    /// The sensor index (pixel centres at whole numbers) the binned index `i` stands for along
    /// `axis` (0: x): the middle of its `k` pixels.
    fn sensor_at(&self, i: f64, axis: usize) -> f64 {
        i * self.k as f64 + self.origin[axis] as f64 + (self.k as f64 - 1.0) / 2.0
    }

    /// The binned index (continuous) of the sensor index `s` along `axis`.
    fn binned_at(&self, s: f64, axis: usize) -> f64 {
        (s - self.origin[axis] as f64 - (self.k as f64 - 1.0) / 2.0) / self.k as f64
    }
}

/// Whether [`apply_list3_binned`] can evaluate every opcode of `list` on an image binned by any
/// factor: the warps, the vignette and the gain maps are defined in coordinates relative to the
/// image (only a gain map's area is in pixels, and binning scales it); per-row, per-column and
/// bad-pixel opcodes index single pixels.
pub fn list3_binnable(list: &[Opcode]) -> bool {
    list.iter().all(|op| match op {
        Opcode::WarpRectilinear { .. }
        | Opcode::WarpFisheye { .. }
        | Opcode::FixVignetteRadial { .. }
        | Opcode::TrimBounds { .. }
        | Opcode::Unknown { .. } => true,
        Opcode::GainMap { area, .. } => area.row_pitch <= 1 && area.col_pitch <= 1,
        _ => false,
    })
}

/// [`apply_list3`] on the demosaic of a mosaic binned as `b` says (`RawImage::develop_binned`
/// blocks): the warps, the vignette and the gain maps are evaluated at the sensor positions the
/// binned pixels stand for. Only meaningful when [`list3_binnable`].
pub fn apply_list3_binned(list: &[Opcode], img: &mut RgbFrame, b: Binning) {
    let (w, h) = (img.width, img.height);
    let b = Binning { k: b.k.max(1), ..b };
    for op in list {
        match op {
            Opcode::WarpRectilinear { planes, center } => *img = warp_rectilinear_binned(img, planes, *center, b),
            Opcode::WarpFisheye { planes, center } => *img = warp_fisheye_binned(img, planes, *center, b),
            Opcode::TrimBounds { .. } | Opcode::Unknown { .. } => {}
            Opcode::FixVignetteRadial { k, center } => vignette(img.data.as_flattened_mut(), w, h, 3, k, *center, b),
            Opcode::GainMap { area, points_v, points_h, spacing, origin, map_planes, gains } if b.k > 1 => {
                // the binned pixels whose centre lies in the area's sensor pixels
                let edge = |e: u32, axis: usize| b.binned_at(e as f64 - 0.5, axis).ceil().clamp(0.0, u32::MAX as f64) as u32;
                let binned =
                    Area { top: edge(area.top, 1), left: edge(area.left, 0), bottom: edge(area.bottom, 1), right: edge(area.right, 0), ..*area };
                let (sw, sh) = (b.sensor[0] as f64, b.sensor[1] as f64);
                for_area(img.data.as_flattened_mut(), w, h, 3, &binned, |x, y, p, v| {
                    let rv = (b.sensor_at(y as f64, 1) + 0.5) / sh;
                    let rh = (b.sensor_at(x as f64, 0) + 0.5) / sw;
                    *v *= gain_at(*points_v, *points_h, *spacing, *origin, *map_planes, gains, rv, rh, p - area.plane as usize);
                });
            }
            other => apply_list(std::slice::from_ref(other), img.data.as_flattened_mut(), w, h, 3, None, 1.0),
        }
    }
}

/// Catmull-Rom (Keys, a = −0.5) sample of channel `c` at continuous pixel coordinates (pixel
/// centres at +0.5), clamped to the range of the four nearest pixels so a clipped highlight or a
/// hard edge cannot ring. The warps resample the whole image once; bilinear would soften it
/// unevenly (to 0.71 of the contrast at a quarter cycle a pixel, wherever the offset is half a pixel).
/// Within a pixel of the border, where the outer taps would repeat the edge, it is bilinear.
fn sample_cubic(img: &RgbFrame, x: f64, y: f64, c: usize) -> f32 {
    let cubic = li_raster::resample::Resample::Bicubic;
    let fx = x.clamp(-1.0, img.width as f64 + 1.0) - 0.5;
    let fy = y.clamp(-1.0, img.height as f64 + 1.0) - 0.5;
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let (x0, y0) = (x0 as isize, y0 as isize);
    if x0 < 1 || y0 < 1 || x0 + 2 >= img.width as isize || y0 + 2 >= img.height as isize {
        return img.sample_bilinear(x as f32, y as f32)[c];
    }
    let wx: [f64; 4] = std::array::from_fn(|i| cubic.weight(tx - (i as f64 - 1.0)));
    let wy: [f64; 4] = std::array::from_fn(|j| cubic.weight(ty - (j as f64 - 1.0)));
    let (mut v, mut lo, mut hi) = (0.0f64, f32::MAX, f32::MIN);
    for (j, wyj) in wy.iter().enumerate() {
        let yy = y0 + j as isize - 1;
        let mut row = 0.0f64;
        for (i, wxi) in wx.iter().enumerate() {
            let s = img.get_clamped(x0 + i as isize - 1, yy)[c];
            if (1..=2).contains(&i) && (1..=2).contains(&j) {
                lo = lo.min(s);
                hi = hi.max(s);
            }
            row += wxi * s as f64;
        }
        v += wyj * row;
    }
    (v as f32).clamp(lo, hi)
}

/// DNG `WarpRectilinear`: for each output pixel, the source position is the radial + tangential model around the
/// optical centre, with coordinates normalised by the distance to the farthest corner.
pub fn warp_rectilinear(img: &RgbFrame, planes: &[[f64; 6]], center: [f64; 2]) -> RgbFrame {
    warp_rectilinear_binned(img, planes, center, Binning::none(img.width, img.height))
}

/// [`warp_rectilinear`] evaluated at the sensor positions of an image binned as `b` says: binned
/// pixel `x` stands for sensor index `origin + k·x + (k − 1)/2`.
fn warp_rectilinear_binned(img: &RgbFrame, planes: &[[f64; 6]], center: [f64; 2], b: Binning) -> RgbFrame {
    let (w, h) = (img.width, img.height);
    // Match Adobe dng_filter_warp: bounds are [0,w) × [0,h), while
    // the warp evaluates integer pixel indices (the sampler adds its half pixel).
    let (sw, sh) = (b.sensor[0] as f64, b.sensor[1] as f64);
    let (cx, cy) = (center[0] * sw, center[1] * sh);
    let m = max_radius(cx, cy, sw, sh);
    let mut out = RgbFrame::new(w, h);
    let coef = |p: usize| planes[p.min(planes.len() - 1)];
    li_raster::par_rows(&mut out.data, w, |y, row| {
        for (x, px) in row.iter_mut().enumerate() {
            for (c, v) in px.iter_mut().enumerate() {
                let [kr0, kr1, kr2, kr3, kt0, kt1] = coef(c);
                let dx = (b.sensor_at(x as f64, 0) - cx) / m;
                let dy = (b.sensor_at(y as f64, 1) - cy) / m;
                let r2 = dx * dx + dy * dy;
                let f = kr0 + r2 * (kr1 + r2 * (kr2 + r2 * kr3));
                let sx = dx * f + kt0 * 2.0 * dx * dy + kt1 * (r2 + 2.0 * dx * dx);
                let sy = dy * f + kt1 * 2.0 * dx * dy + kt0 * (r2 + 2.0 * dy * dy);
                let (fx, fy) = (b.binned_at(cx + m * sx, 0), b.binned_at(cy + m * sy, 1));
                *v = sample_cubic(img, fx + 0.5, fy + 0.5, c);
            }
        }
    });
    out
}

/// DNG 1.7.1 `WarpFisheye` (pp. 108–110): transform an output radius `r`,
/// normalised to the farthest corner, to `theta = atan(r)` and evaluate the
/// per-plane odd polynomial in `theta`. Sampling uses the same pixel centres
/// and kernel as [`warp_rectilinear`].
pub fn warp_fisheye(img: &RgbFrame, planes: &[[f64; 4]], center: [f64; 2]) -> RgbFrame {
    warp_fisheye_binned(img, planes, center, Binning::none(img.width, img.height))
}

/// [`warp_fisheye`] at the sensor positions of an image binned as `b` says (see [`warp_rectilinear_binned`]).
fn warp_fisheye_binned(img: &RgbFrame, planes: &[[f64; 4]], center: [f64; 2], b: Binning) -> RgbFrame {
    if img.width == 0 || img.height == 0 || !matches!(planes.len(), 1 | 3) || !center.iter().chain(planes.iter().flatten()).all(|v| v.is_finite()) {
        return img.clone();
    }
    let (w, h) = (img.width, img.height);
    let (sw, sh) = (b.sensor[0] as f64, b.sensor[1] as f64);
    let (cx, cy) = (center[0] * (sw - 1.0), center[1] * (sh - 1.0));
    let m = max_radius(cx, cy, sw - 1.0, sh - 1.0);
    let mut out = RgbFrame::new(w, h);
    li_raster::par_rows(&mut out.data, w, |y, row| {
        for (x, px) in row.iter_mut().enumerate() {
            let dx = b.sensor_at(x as f64, 0) - cx;
            let dy = b.sensor_at(y as f64, 1) - cy;
            let r = dx.hypot(dy) / m;
            for (c, value) in px.iter_mut().enumerate() {
                let kc = planes[if planes.len() == 1 { 0 } else { c }];
                let scale = fisheye_scale(kc, r);
                let (sx, sy) = (b.binned_at(cx + dx * scale, 0), b.binned_at(cy + dy * scale, 1));
                *value = sample_cubic(img, sx + 0.5, sy + 0.5, c);
            }
        }
    });
    out
}

fn fisheye_scale([kr0, kr1, kr2, kr3]: [f64; 4], r: f64) -> f64 {
    if r == 0.0 {
        return kr0;
    }
    let theta = r.atan();
    let theta2 = theta * theta;
    theta / r * (kr0 + theta2 * (kr1 + theta2 * (kr2 + theta2 * kr3)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) struct ListWriter(pub Vec<u8>, pub u32);

    impl ListWriter {
        pub fn new() -> Self {
            ListWriter(Vec::new(), 0)
        }
        pub fn op(&mut self, id: u32, params: &[u8]) -> &mut Self {
            for v in [id, 0x01030000, 0, params.len() as u32] {
                self.0.extend_from_slice(&v.to_be_bytes());
            }
            self.0.extend_from_slice(params);
            self.1 += 1;
            self
        }
        pub fn finish(&self) -> Vec<u8> {
            let mut v = self.1.to_be_bytes().to_vec();
            v.extend_from_slice(&self.0);
            v
        }
    }

    pub(crate) fn be(vals: &[f64]) -> Vec<u8> {
        vals.iter().flat_map(|v| v.to_be_bytes()).collect()
    }
    pub(crate) fn area(a: [u32; 8]) -> Vec<u8> {
        a.iter().flat_map(|v| v.to_be_bytes()).collect()
    }

    #[test]
    fn parse_and_apply_area_ops() {
        let (w, h) = (4usize, 3usize);
        let mut gm = area([0, 0, 3, 4, 0, 1, 1, 1]);
        for v in [2u32, 2] {
            gm.extend_from_slice(&v.to_be_bytes());
        }
        gm.extend(be(&[1.0, 1.0, 0.0, 0.0]));
        gm.extend_from_slice(&1u32.to_be_bytes());
        for g in [1.0f32, 2.0, 1.0, 2.0] {
            gm.extend_from_slice(&g.to_be_bytes());
        }
        let mut dpr = area([0, 0, 3, 4, 0, 1, 1, 1]);
        dpr.extend_from_slice(&3u32.to_be_bytes());
        for d in [0.0f32, 0.1, 0.2] {
            dpr.extend_from_slice(&d.to_be_bytes());
        }
        let mut poly = area([0, 0, 3, 4, 0, 1, 1, 2]);
        poly.extend_from_slice(&1u32.to_be_bytes());
        poly.extend(be(&[0.0, 2.0]));
        let blob = ListWriter::new().op(9, &gm).op(10, &dpr).op(8, &poly).op(99, &[1, 2, 3]).finish();
        let list = parse_list(&blob);
        assert_eq!(list.len(), 4);
        assert!(matches!(list[3], Opcode::Unknown { id: 99, .. }));
        assert!(list[..3].iter().all(|o| o.is_applied()));
        let mut buf = vec![0.25f32; w * h];
        apply_list(&list, &mut buf, w, h, 1, None, 1.0);
        // gain map: horizontal ramp 1 → 2 at pixel centres
        let expect = |x: usize, y: usize| {
            let g = 1.0 + (x as f32 + 0.5) / w as f32;
            let v = 0.25 * g + [0.0, 0.1, 0.2][y];
            if x.is_multiple_of(2) { v * 2.0 } else { v }
        };
        for y in 0..h {
            for x in 0..w {
                assert!((buf[y * w + x] - expect(x, y)).abs() < 1e-6, "{x},{y}: {} vs {}", buf[y * w + x], expect(x, y));
            }
        }
    }

    #[test]
    fn map_table_raw_scale_and_bad_pixels() {
        let mut t = area([0, 0, 2, 2, 0, 1, 1, 1]);
        t.extend_from_slice(&3u32.to_be_bytes());
        for v in [0u16, 30000, 65535] {
            t.extend_from_slice(&v.to_be_bytes());
        }
        let list = parse_list(&ListWriter::new().op(7, &t).finish());
        let mut buf = vec![0.0, 1.0 / 65535.0, 2.0 / 65535.0, 0.5];
        apply_list(&list, &mut buf, 2, 2, 1, None, 1.0);
        assert_eq!(buf[0], 0.0);
        assert!((buf[1] - 30000.0 / 65535.0).abs() < 1e-7);
        assert_eq!(buf[3], 1.0);

        let mut p = vec![];
        for v in [7u32, 0] {
            p.extend_from_slice(&v.to_be_bytes());
        }
        let list = parse_list(&ListWriter::new().op(4, &p).finish());
        let cfa = Cfa::bayer("RGGB").unwrap();
        let mut img: Vec<f32> = (0..36).map(|i| 100.0 + (i % 6) as f32).collect();
        img[2 * 6 + 2] = 7.0;
        apply_list(&list, &mut img, 6, 6, 1, Some(&cfa), 65535.0);
        assert_eq!(img[2 * 6 + 2], 102.0);
    }

    /// A mosaic equal to the bad-pixel constant, or one rectangle over the whole image, used to list every pixel as
    /// bad: w × h entries (plus a set of them), tens of GB at the decoder's size limit. Here the rows between a good
    /// first row (500) and a good last row (900) are bad: 2052 rows of 2048 pixels, four rows more than the 1 << 22
    /// cap. Row 1, within the cap, is repaired from the first row; the second-last row, past the cap, must be left
    /// as it is (uncapped, it was rewritten from the last row).
    fn assert_bad_pixels_capped(op: Opcode) {
        let (w, h) = (2048, 2054);
        let mut buf = vec![0.0f32; w * h];
        buf[..w].fill(500.0);
        buf[(h - 1) * w..].fill(900.0);
        apply_list(&[op], &mut buf, w, h, 1, None, 65535.0);
        assert!(buf[w..2 * w].iter().all(|&v| v == 500.0), "row 1 (within the cap) not repaired: {:?}", &buf[w..w + 4]);
        let past = &buf[(h - 2) * w..(h - 1) * w];
        assert!(past.iter().all(|&v| v == 0.0), "a row past the cap was rewritten: {:?}", &past[..4]);
    }

    #[test]
    fn bad_pixels_constant_is_capped() {
        assert_bad_pixels_capped(Opcode::FixBadPixelsConstant { constant: 0, bayer_phase: 0 });
    }

    #[test]
    fn bad_pixels_list_is_capped_inside_a_rectangle() {
        // top, left, bottom, right: every row but the first and the last
        assert_bad_pixels_capped(Opcode::FixBadPixelsList { bayer_phase: 0, points: vec![], rects: vec![[1, 0, 2053, 2048]] });
    }

    #[test]
    fn vignette_and_warp_identity() {
        let mut img = RgbFrame::from_fn(9, 7, |x, y| [x as f32 / 9.0, y as f32 / 7.0, 0.5]);
        let orig = img.clone();
        let mut p = be(&[0.0; 5]);
        p.extend(be(&[0.5, 0.5]));
        let mut warp = 1u32.to_be_bytes().to_vec();
        warp.extend(be(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.5]));
        let list = parse_list(&ListWriter::new().op(3, &p).op(1, &warp).finish());
        assert_eq!(list.len(), 2);
        apply_list3(&list, &mut img);
        for (a, b) in img.data.iter().zip(&orig.data) {
            for c in 0..3 {
                assert!((a[c] - b[c]).abs() < 1e-5);
            }
        }
        // positive k0 brightens the corners more than the centre
        let mut p = be(&[1.0, 0.0, 0.0, 0.0, 0.0]);
        p.extend(be(&[0.5, 0.5]));
        let list = parse_list(&ListWriter::new().op(3, &p).finish());
        let mut flat = RgbFrame::filled(9, 9, [0.5; 3]);
        apply_list3(&list, &mut flat);
        assert!(flat.get(0, 0)[0] > 0.85 && (flat.get(4, 4)[0] - 0.5).abs() < 0.01, "{:?}", flat.get(0, 0));
    }

    #[test]
    fn warp_barrel_moves_pixels_outward() {
        let img = RgbFrame::from_fn(31, 31, |x, y| if x == 24 && y == 15 { [1.0; 3] } else { [0.0; 3] });
        // source radius = r (1 + k r²) with k < 0 → content moves outward
        let out = warp_rectilinear(&img, &[[1.0, -0.6, 0.0, 0.0, 0.0, 0.0]], [0.5, 0.5]);
        let (bx, _) = (0..31).map(|x| (x, out.get(x, 15)[0])).fold((0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
        assert!(bx >= 25, "{bx}");
    }

    /// Independent Adobe SDK coordinate vectors: non-central optical centre and
    /// half-pixel GainMap origin expose errors hidden by identity transforms.
    #[test]
    fn sony_dji_sdk_opcode_coordinates() {
        let (w, h) = (8, 6);
        let ramp = RgbFrame::from_fn(w, h, |x, y| [x as f32, y as f32, (x + y) as f32]);
        let warped = warp_rectilinear(&ramp, &[[0.5, 0.0, 0.0, 0.0, 0.0, 0.0]], [0.25, 0.75]);
        // SDK image bounds [0,8) × [0,6): centre (2,4.5).
        // At integer index (3,2), source = centre + 0.5 * (index-centre).
        let p = warped.get(3, 2);
        for (v, expected) in p.into_iter().zip([2.5, 3.25, 5.75]) {
            assert!((v - expected).abs() < 1e-6, "{v} versus {expected}");
        }
        let mut flat = vec![1.0; w * h];
        apply_list(&[Opcode::FixVignetteRadial { k: [1.0, 0.0, 0.0, 0.0, 0.0], center: [0.25, 0.75] }], &mut flat, w, h, 1, None, 1.0);
        // Vignette uses pixel centres: (3.5,2.5), max radius 7.5.
        assert!((flat[2 * w + 3] - 10.0 / 9.0).abs() < 1e-6);
        let op = Opcode::GainMap {
            area: Area { bottom: 6, right: 8, planes: 1, row_pitch: 1, col_pitch: 1, ..Default::default() },
            points_v: 2,
            points_h: 2,
            spacing: [1.0, 1.0],
            origin: [0.0, 0.0],
            map_planes: 1,
            gains: vec![1.0, 3.0, 4.0, 6.0],
        };
        flat.fill(1.0);
        apply_list(&[op], &mut flat, w, h, 1, None, 1.0);
        assert!((flat[0] - (1.0 + 2.0 * 0.5 / 8.0 + 3.0 * 0.5 / 6.0)).abs() < 1e-6);
        assert!((flat[47] - (1.0 + 2.0 * 7.5 / 8.0 + 3.0 * 5.5 / 6.0)).abs() < 1e-6);
    }

    #[test]
    fn warp_fisheye_dng_coordinate_ramps_and_center_limit() {
        let original = RgbFrame::from_fn(41, 31, |x, y| [x as f32 / 40.0, y as f32 / 30.0, (x + 2 * y) as f32 / 100.0]);
        let coefficients = [[0.87, 0.025, -0.02, 0.01], [0.92, -0.01, 0.03, -0.005], [0.97, 0.015, -0.01, 0.002]];
        // Off-centre optical axis is exactly at pixel (10, 12). The farthest
        // corner is (40, 30), independently giving sqrt(30² + 18²).
        let m = (30.0f64.powi(2) + 18.0f64.powi(2)).sqrt();
        for planes in [vec![coefficients[0]], coefficients.to_vec()] {
            let op = Opcode::WarpFisheye { planes: planes.clone(), center: [0.25, 0.4] };
            assert!(op.is_applied());
            // Exercise binary opcode parsing and the actual list-3 entry point.
            let parsed = parse_list(&write_list(&[op]));
            let mut warped = original.clone();
            apply_list3(&parsed, &mut warped);
            for y in 0..31 {
                for x in 0..41 {
                    let dx = x as f64 - 10.0;
                    let dy = y as f64 - 12.0;
                    let r = (dx * dx + dy * dy).sqrt() / m;
                    let theta = r.atan();
                    for channel in 0..3 {
                        let k = planes[if planes.len() == 1 { 0 } else { channel }];
                        let scale =
                            if r == 0.0 { k[0] } else { (k[0] * theta + k[1] * theta.powi(3) + k[2] * theta.powi(5) + k[3] * theta.powi(7)) / r };
                        let sx = 10.0 + dx * scale;
                        let sy = 12.0 + dy * scale;
                        let expected = match channel {
                            0 => sx / 40.0,
                            1 => sy / 30.0,
                            _ => (sx + 2.0 * sy) / 100.0,
                        };
                        assert!((f64::from(warped.get(x, y)[channel]) - expected).abs() < 2e-6, "pixel ({x},{y}), plane {channel}");
                    }
                }
            }
            assert_eq!(warped.get(10, 12), original.get(10, 12));
        }
        for k in coefficients {
            assert_eq!(fisheye_scale(k, 0.0), k[0]);
            assert!((fisheye_scale(k, 1e-12) - k[0]).abs() < 1e-12);
        }
        let singleton = RgbFrame::filled(1, 1, [0.2, 0.4, 0.6]);
        assert_eq!(warp_fisheye(&singleton, &[coefficients[0]], [0.5, 0.5]), singleton);
        assert_eq!(warp_fisheye(&original, &[], [0.5, 0.5]), original);
        assert_eq!(warp_fisheye(&original, &coefficients[..2], [0.5, 0.5]), original);
    }

    #[test]
    fn write_parse_roundtrip() {
        let a = Area { top: 1, left: 2, bottom: 30, right: 40, plane: 0, planes: 3, row_pitch: 1, col_pitch: 2 };
        let list = vec![
            Opcode::WarpRectilinear { planes: vec![[1.0, 0.01, -0.002, 0.0, 1e-4, -2e-4]; 3], center: [0.5, 0.49] },
            Opcode::WarpFisheye { planes: vec![[1.0, 0.1, 0.0, 0.0]], center: [0.5, 0.5] },
            Opcode::FixVignetteRadial { k: [0.1, 0.2, 0.0, 0.0, 0.0], center: [0.5, 0.5] },
            Opcode::FixBadPixelsConstant { constant: 0, bayer_phase: 1 },
            Opcode::FixBadPixelsList { bayer_phase: 0, points: vec![(3, 4)], rects: vec![[1, 1, 2, 2]] },
            Opcode::TrimBounds { top: 1, left: 1, bottom: 9, right: 9 },
            Opcode::MapTable { area: a, table: vec![0, 100, 65535] },
            Opcode::MapPolynomial { area: a, coefficients: vec![0.0, 1.0, -0.1] },
            Opcode::GainMap { area: a, points_v: 2, points_h: 1, spacing: [1.0, 1.0], origin: [0.0, 0.0], map_planes: 1, gains: vec![1.0, 1.5] },
            Opcode::DeltaPerRow { area: a, deltas: vec![0.1, -0.2] },
            Opcode::DeltaPerColumn { area: a, deltas: vec![-0.3] },
            Opcode::ScalePerRow { area: a, scales: vec![1.1] },
            Opcode::ScalePerColumn { area: a, scales: vec![0.9, 1.0] },
            Opcode::Unknown { id: 77, flags: 1, params: vec![1, 2, 3] },
        ];
        assert_eq!(parse_list(&write_list(&list)), list);
    }

    /// Delta opcodes are signed offsets: a negative entry must not drop the whole correction.
    #[test]
    fn negative_deltas_parse_and_apply() {
        let area = Area { bottom: 1, right: 2, planes: 1, row_pitch: 1, col_pitch: 1, ..Default::default() };
        let delta = Opcode::DeltaPerColumn { area, deltas: vec![-0.002, 0.001] };
        let list = parse_list(&write_list(std::slice::from_ref(&delta)));
        assert_eq!(list, vec![delta]);
        assert!(list[0].is_applied());
        let mut buf = vec![0.5f32; 2];
        apply_list(&list, &mut buf, 2, 1, 1, None, 1.0);
        assert!((buf[0] - 0.498).abs() < 1e-6 && (buf[1] - 0.501).abs() < 1e-6, "{buf:?}");
        // list 1 runs on 16-bit values: the delta is in normalised units
        let mut raw = vec![32767.5f32; 2];
        apply_list(&list, &mut raw, 2, 1, 1, None, 65535.0);
        assert!((raw[0] - (32767.5 - 0.002 * 65535.0)).abs() < 1e-2, "{raw:?}");
        let row = Opcode::DeltaPerRow { area, deltas: vec![-0.01] };
        assert_eq!(parse_list(&write_list(std::slice::from_ref(&row))), vec![row]);
        // scales stay non-negative
        let scale = Opcode::ScalePerColumn { area, scales: vec![-1.0, 1.0] };
        assert!(matches!(parse_list(&write_list(&[scale]))[0], Opcode::Unknown { id: 13, .. }));
    }

    /// The warps resample with a clamped Catmull-Rom kernel: detail at a quarter cycle per pixel,
    /// sampled half a pixel off, keeps 0.88 of its contrast (bilinear: 0.71); a flat field stays
    /// exact and a hard edge (a clipped window) gets no overshoot.
    #[test]
    fn warp_kernel_keeps_detail_without_ringing() {
        let sine = RgbFrame::from_fn(64, 8, |x, _| [0.5 + 0.4 * (std::f32::consts::FRAC_PI_2 * x as f32).sin(); 3]);
        let amplitude = |f: &dyn Fn(f64) -> f32| {
            let v: Vec<f32> = (8..56).map(|x| f(x as f64 + 1.0)).collect();
            (v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)) / 2.0
        };
        // half a pixel off the samples the sine's peaks fall between: its true amplitude there is 0.4 · sin(π/4)
        let truth = 0.4 * std::f32::consts::FRAC_1_SQRT_2;
        let cubic = amplitude(&|x| sample_cubic(&sine, x, 4.5, 1)) / truth;
        let bilinear = amplitude(&|x| sine.sample_bilinear(x as f32, 4.5)[1]) / truth;
        assert!(cubic > 0.85 && bilinear < 0.75, "cubic {cubic}, bilinear {bilinear}");
        let flat = RgbFrame::filled(16, 16, [0.3, 0.6, 0.9]);
        let warped = warp_rectilinear(&flat, &[[0.97, 0.03, 0.0, 0.0, 0.0, 0.0]], [0.5, 0.5]);
        assert!(warped.data.iter().all(|p| (p[0] - 0.3).abs() < 1e-6 && (p[2] - 0.9).abs() < 1e-6));
        let step = RgbFrame::from_fn(32, 32, |x, _| if x < 16 { [0.1; 3] } else { [2.0; 3] });
        let warped = warp_rectilinear(&step, &[[0.97, 0.03, 0.0, 0.0, 0.0, 0.0]], [0.5, 0.5]);
        assert!(warped.data.iter().flatten().all(|&v| (0.1 - 1e-6..=2.0 + 1e-6).contains(&v)));
    }

    #[test]
    fn truncated_lists_do_not_panic() {
        let mut gm = area([0, 0, 3, 4, 0, 1, 1, 1]);
        gm.extend_from_slice(&[0xff; 64]);
        let blob = ListWriter::new().op(9, &gm).op(5, &[0xff; 12]).op(1, &[0, 0, 0, 9]).finish();
        for n in 0..blob.len() {
            let l = parse_list(&blob[..n]);
            let mut buf = vec![0.5f32; 12];
            apply_list(&l, &mut buf, 4, 3, 1, None, 1.0);
        }
    }
}
