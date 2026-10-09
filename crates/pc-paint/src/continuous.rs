//! Continuous coverage for soft round brushes (Compositor's analytic stroke rendering; idea from
//! Compositor (MIT), our implementation).
//!
//! A stamped stroke builds coverage dab by dab: `c ← c + v·(ceil − c)`, i.e. one minus the
//! product of the dabs' transmittances `Π (1 − flow·cov(x − pᵢ))` over dabs `Δ` apart (`Δ` = the
//! brush's spacing times its diameter). Along a straight run that product is
//! `exp Σ ln(1 − flow·cov(x − pᵢ))`; its continuous equivalent replaces the sum by the integral of
//! the same density per unit length, `exp (1/Δ) ∫ ln(1 − flow·cov(x − p(s))) ds`. That keeps
//! Photoshop's density behaviour (tighter spacing, at flow below 100 %, builds more paint; the mean
//! coverage matches the stamped stroke at the same spacing) but has no beading or banding: every
//! pixel sees the exact integral, evaluated per stroke segment with Gauss–Legendre quadrature over
//! the part of the segment whose dab reaches the pixel. Each end of the stroke adds half a dab
//! (the trapezoid correction between the dab sum and the integral), so a single click is exactly
//! one dab.
//!
//! Only soft round brushes without per-dab variation take this path
//! ([`crate::BrushSettings::continuous_coverage`]); everything else is stamped.

use photocraft_geom::Rect;

use crate::{Dab, dab_coverage};

/// Gauss–Legendre nodes and weights on `[-1, 1]` (6 points).
const GL_X: [f64; 6] =
    [-0.932_469_514_203_152, -0.661_209_386_466_265, -0.238_619_186_083_197, 0.238_619_186_083_197, 0.661_209_386_466_265, 0.932_469_514_203_152];
const GL_W: [f64; 6] =
    [0.171_324_492_379_170, 0.360_761_573_048_139, 0.467_913_934_572_691, 0.467_913_934_572_691, 0.360_761_573_048_139, 0.171_324_492_379_170];

/// The spacing dabs are generated at on the continuous path (a fraction of the diameter): only
/// the segment ends (where pressure is sampled) depend on it, not the coverage.
pub const SEGMENT_SPACING: f32 = 0.25;

/// Largest transmittance loss per evaluation (keeps `ln(1 − flow·cov)` finite at full flow).
const MAX_V: f64 = 1.0 - 1e-6;

/// The tip's fixed shape: angle and roundness, as [`crate::render::BrushContext::rasterize`]
/// applies them.
#[derive(Clone, Copy, Debug)]
struct Tip {
    sn: f32,
    cs: f32,
    hardness: f32,
}

impl Tip {
    fn of(d: &Dab, hardness: f32) -> Tip {
        let (sn, cs) = d.angle.sin_cos();
        Tip { sn, cs, hardness }
    }

    /// Coverage of a dab of radius `r` and roundness `roundness` centred at offset `(dx, dy)`
    /// (pixel centre minus dab centre, y down).
    #[inline]
    fn cov(&self, dx: f32, dy: f32, r: f32, roundness: f32) -> f32 {
        let (ux, uy) = (dx, -dy);
        let u = ux * self.cs + uy * self.sn;
        let v = -ux * self.sn + uy * self.cs;
        let ro = roundness.max(0.5 / r).min(1.0);
        let rm = (r * ro).max(0.5);
        let dd = ((u * ro).powi(2) + v * v).sqrt();
        dab_coverage(dd, rm, self.hardness)
    }
}

/// Pixel rectangle a dab can touch.
pub fn dab_reach(d: &Dab) -> Rect {
    let rr = d.radius.ceil() as i32 + 1;
    let (cx, cy) = (d.center.x.floor() as i32, d.center.y.floor() as i32);
    Rect::new(cx - rr, cy - rr, cx + rr + 1, cy + rr + 1)
}

/// Spacing step (pixels) at diameter `diameter`: as the dab generator spaces dabs.
#[inline]
fn step_px(diameter: f32, spacing: f32) -> f64 {
    f64::from(diameter.max(1.0) * spacing.max(0.01)).max(0.5)
}

/// A half dab (`v = 1 − √(1 − flow·cov)`) at a stroke end, over [`dab_reach`] into `out`.
pub fn end_cap(d: &Dab, hardness: f32, out: &mut Vec<f32>) -> Rect {
    let rect = dab_reach(d);
    let (w, h) = (rect.width() as usize, rect.height() as usize);
    out.clear();
    out.resize(w * h, 0.0);
    let tip = Tip::of(d, hardness);
    let (cx, cy) = (d.center.x as f32, d.center.y as f32);
    for yy in 0..h {
        let dy = (rect.y0 + yy as i32) as f32 + 0.5 - cy;
        for xx in 0..w {
            let dx = (rect.x0 + xx as i32) as f32 + 0.5 - cx;
            let c = tip.cov(dx, dy, d.radius, d.roundness);
            if c > 0.0 {
                let a = (f64::from(d.alpha) * f64::from(c)).min(MAX_V);
                out[yy * w + xx] = (1.0 - (1.0 - a).sqrt()) as f32;
            }
        }
    }
    rect
}

/// The continuous coverage of the stroke segment from dab `a` to dab `b` (centre, radius and
/// flow interpolated linearly; angle and roundness fixed): per pixel
/// `1 − exp((1/Δ) ∫ ln(1 − flow·cov) ds)`, with `Δ` the spacing step at the local diameter.
/// Returns the rectangle written into `out`. Segments of constant size and flow (no pen pressure
/// on them) read the integral from a cumulative table kept in `table` (rebuilt when the dab
/// changes); others integrate per pixel.
pub fn segment(a: &Dab, b: &Dab, hardness: f32, spacing: f32, table: &mut Option<LineTable>, out: &mut Vec<f32>) -> Rect {
    let rect = dab_reach(a).union(&dab_reach(b));
    let (w, h) = (rect.width() as usize, rect.height() as usize);
    out.clear();
    out.resize(w * h, 0.0);
    let (ax, ay) = (a.center.x, a.center.y);
    let (ex, ey) = (b.center.x - ax, b.center.y - ay);
    let len = ex.hypot(ey);
    if len <= 1e-9 {
        return rect;
    }
    let tip = Tip::of(a, hardness);
    if a.radius == b.radius && a.alpha == b.alpha && a.roundness == b.roundness {
        let t = match table.take() {
            Some(t) if t.fits(a, hardness) => t,
            _ => LineTable::new(a, hardness),
        };
        t.segment(a, b, &tip, spacing, rect, out);
        *table = Some(t);
        return rect;
    }
    let rmax = f64::from(a.radius.max(b.radius)) + 1.0;
    let row = |yy: usize, out: &mut [f32]| {
        let py = f64::from(rect.y0 + yy as i32) + 0.5;
        for (xx, o) in out.iter_mut().enumerate() {
            let px = f64::from(rect.x0 + xx as i32) + 0.5;
            // Where along the segment the dab can reach this pixel: |q − c(t)| ≤ rmax.
            let (qx, qy) = (px - ax, py - ay);
            let t_mid = (qx * ex + qy * ey) / (len * len);
            let perp2 = (qx * qx + qy * qy) - t_mid * t_mid * len * len;
            let half2 = rmax * rmax - perp2;
            if half2 <= 0.0 {
                continue;
            }
            let half = half2.sqrt() / len;
            let (t0, t1) = ((t_mid - half).max(0.0), (t_mid + half).min(1.0));
            if t1 <= t0 {
                continue;
            }
            // The integrand peaks where the pixel is closest to the path: split there so each
            // Gauss–Legendre panel sees a smooth, one-sided profile.
            let mut integral = 0.0f64;
            let split = t_mid.clamp(t0, t1);
            for (lo, hi) in [(t0, split), (split, t1)] {
                if hi - lo <= 1e-12 {
                    continue;
                }
                let (mid, rad) = (0.5 * (lo + hi), 0.5 * (hi - lo));
                let mut part = 0.0f64;
                for (gx, gw) in GL_X.iter().zip(GL_W) {
                    let t = mid + rad * gx;
                    let r = a.radius + (b.radius - a.radius) * t as f32;
                    let ro = a.roundness + (b.roundness - a.roundness) * t as f32;
                    let flow = f64::from(a.alpha + (b.alpha - a.alpha) * t as f32);
                    let (cx, cy) = (ax + ex * t, ay + ey * t);
                    let c = tip.cov((px - cx) as f32, (py - cy) as f32, r, ro);
                    if c <= 0.0 {
                        continue;
                    }
                    let v = (flow * f64::from(c)).min(MAX_V);
                    // ds = len·dt; density 1/Δ per unit length.
                    part += gw * (1.0 - v).ln() * len / step_px(2.0 * r, spacing);
                }
                integral += part * rad;
            }
            if integral < 0.0 {
                *o = (1.0 - integral.exp()) as f32;
            }
        }
    };
    out.chunks_mut(w).enumerate().for_each(|(yy, r)| row(yy, r));
    rect
}

/// The cumulative line integral `G(p, x) = ∫_{−∞}^{x} ln(1 − flow·cov(√(p² + σ²))) dσ` of one dab
/// (in the tip's own frame, where it is round), tabulated over `p ∈ [0, reach]`,
/// `x ∈ [−reach, reach]`: a constant segment's integral at a pixel is a difference of two
/// lookups.
#[derive(Clone, Debug)]
pub struct LineTable {
    key: (u32, u32, u32, u32, u32),
    reach: f64,
    np: usize,
    nx: usize,
    g: Vec<f64>,
}

impl LineTable {
    const NP: usize = 96;
    const NX: usize = 384;

    fn key(d: &Dab, hardness: f32) -> (u32, u32, u32, u32, u32) {
        (d.radius.to_bits(), d.roundness.to_bits(), d.alpha.to_bits(), hardness.to_bits(), d.angle.to_bits())
    }

    fn fits(&self, d: &Dab, hardness: f32) -> bool {
        self.key == Self::key(d, hardness)
    }

    pub fn new(d: &Dab, hardness: f32) -> LineTable {
        let ro = d.roundness.max(0.5 / d.radius).min(1.0);
        let rm = (d.radius * ro).max(0.5);
        let reach = f64::from(rm) + 0.5;
        let alpha = f64::from(d.alpha);
        let f = |dist: f64| -> f64 {
            let c = dab_coverage(dist as f32, rm, hardness);
            if c <= 0.0 { 0.0 } else { (1.0 - (alpha * f64::from(c)).min(MAX_V)).ln() }
        };
        let (np, nx) = (Self::NP, Self::NX);
        let (dp, dx) = (reach / np as f64, 2.0 * reach / nx as f64);
        let mut g = vec![0.0f64; (np + 1) * (nx + 1)];
        // Two-point Gauss per cell, four cells per table step.
        let (k0, k1) = (0.5 - 0.5 / 3f64.sqrt(), 0.5 + 0.5 / 3f64.sqrt());
        for ip in 0..=np {
            let p = ip as f64 * dp;
            let row = &mut g[ip * (nx + 1)..(ip + 1) * (nx + 1)];
            let mut acc = 0.0;
            for ix in 1..=nx {
                let x0 = -reach + (ix - 1) as f64 * dx;
                let sub = dx / 4.0;
                for k in 0..4 {
                    let a = x0 + k as f64 * sub;
                    let (s0, s1) = (a + k0 * sub, a + k1 * sub);
                    acc += 0.5 * sub * (f(p.hypot(s0)) + f(p.hypot(s1)));
                }
                row[ix] = acc;
            }
        }
        LineTable { key: Self::key(d, hardness), reach, np, nx, g }
    }

    /// `G(p, x)`, bilinear.
    #[inline]
    fn at(&self, p: f64, x: f64) -> f64 {
        if p >= self.reach || x <= -self.reach {
            return 0.0;
        }
        let fp = p / self.reach * self.np as f64;
        let fx = ((x + self.reach) / (2.0 * self.reach) * self.nx as f64).min(self.nx as f64);
        let (ip, ix) = ((fp as usize).min(self.np - 1), (fx as usize).min(self.nx - 1));
        let (tp, tx) = (fp - ip as f64, fx - ix as f64);
        let w = self.nx + 1;
        let r0 = self.g[ip * w + ix] + (self.g[ip * w + ix + 1] - self.g[ip * w + ix]) * tx;
        let r1 = self.g[(ip + 1) * w + ix] + (self.g[(ip + 1) * w + ix + 1] - self.g[(ip + 1) * w + ix]) * tx;
        r0 + (r1 - r0) * tp
    }

    fn segment(&self, a: &Dab, b: &Dab, tip: &Tip, spacing: f32, rect: Rect, out: &mut [f32]) {
        let w = rect.width() as usize;
        let ro = f64::from(a.roundness.max(0.5 / a.radius).min(1.0));
        let (cs, sn) = (f64::from(tip.cs), f64::from(tip.sn));
        // Pixel offset (y down) → the tip's round frame: (u·ro, v).
        let frame = |dx: f64, dy: f64| {
            let (ux, uy) = (dx, -dy);
            ((ux * cs + uy * sn) * ro, -ux * sn + uy * cs)
        };
        let (ex, ey) = (b.center.x - a.center.x, b.center.y - a.center.y);
        let len = ex.hypot(ey);
        let (vx, vy) = frame(ex, ey);
        let lt = vx.hypot(vy);
        if lt <= 1e-12 {
            return;
        }
        let (nx, ny) = (vx / lt, vy / lt);
        let k = len / (lt * step_px(2.0 * a.radius, spacing));
        for (yy, row) in out.chunks_mut(w).enumerate() {
            let py = f64::from(rect.y0 + yy as i32) + 0.5 - a.center.y;
            for (xx, o) in row.iter_mut().enumerate() {
                let px = f64::from(rect.x0 + xx as i32) + 0.5 - a.center.x;
                let (qx, qy) = frame(px, py);
                let s = qx * nx + qy * ny;
                let p = (qx * ny - qy * nx).abs();
                if p >= self.reach {
                    continue;
                }
                let i = (self.at(p, s) - self.at(p, s - lt)) * k;
                if i < 0.0 {
                    *o = (1.0 - i.exp()) as f32;
                }
            }
        }
    }
}
