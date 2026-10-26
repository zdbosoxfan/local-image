//! Hue-preserving gamut mapping along constant-lightness OkLab hue lines.
//!
//! Output uses relative colorimetric clipping: destination RGB inside the cube stays identical;
//! outside colours use the first crossing from gray, blended with an RGB ray trace near blue.
//! OkLab hue lines can leave and re-enter RGB gamuts, so their cubic face crossings partition the
//! search before bisection. The scene stage instead uses the smooth ACES reference compression
//! curve ([`knee_curve`]) against the exposure-normalized working gamut.
//!
//! The GPU kernels in `lightcraft-gpu`'s `gamut.wgsl` mirror these operations.

use std::sync::OnceLock;

use crate::output::OutputSpace;

use li_color::perceptual::{oklab_from_2020, oklab_matrices, oklab_to_2020};

pub type M3 = [[f32; 3]; 3];

/// Absolute destination RGB membership tolerance; also shared with WGSL.
pub const OUTPUT_EPS: f32 = 1e-6;
/// Upper-envelope round-off for choosing the highest-chroma lightness endpoint.
pub const ENDPOINT_EPS: f32 = 4e-7;
/// Output chroma and endpoint lightness bisection steps, shared with WGSL.
pub const OUTPUT_STEPS: u32 = 24;
/// Bisection steps that bracket each root of a channel's cubic before its secant.
pub const STEPS: u32 = 12;
/// OkLab chroma where the search along a line ends when none of its cube-rooted LMS falls.
pub const MAX_CHROMA: f32 = 2.0;

/// Padded hue windows containing the blue pinches, shared with WGSL.
pub const BLUE_SRGB: [f32; 2] = [-1.674_503, -1.671_768];
pub const BLUE_REC2020: [f32; 2] = [-2.005_771, -2.001_952];

fn blue_weight(space: OutputSpace, h: f32) -> f32 {
    let [lo, hi] = match space {
        OutputSpace::Srgb => BLUE_SRGB,
        OutputSpace::Rec2020 => BLUE_REC2020,
        _ => return 0.0,
    };
    let center = 0.5 * (lo + hi);
    let distance = ((h - center + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).abs() - 0.5 * (hi - lo);
    let t = ((distance - 0.01) / 0.09).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

struct Mats {
    /// OkLab → cube-rooted LMS.
    from_lab: M3,
    from_srgb: M3,
    /// LMS → linear Rec. 2020, the working gamut's channels.
    working: M3,
}

fn mats() -> &'static Mats {
    static M: OnceLock<Mats> = OnceLock::new();
    M.get_or_init(|| {
        let [_, from_lms, _, from_lab] = oklab_matrices();
        Mats { from_lab, working: from_lms, from_srgb: li_color::SRGB.to_space(&li_color::REC2020).to_f32() }
    })
}

#[inline]
fn mul3(m: &M3, v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// OkLab's LMS → linear RGB of the gamut that `to_rgb` (linear Rec. 2020 → its RGB) leads to: the
/// matrix [`output`] evaluates that gamut's channels with.
pub fn lms_to(to_rgb: &M3) -> M3 {
    let w = &mats().working;
    std::array::from_fn(|r| std::array::from_fn(|c| to_rgb[r][0] * w[0][c] + to_rgb[r][1] * w[1][c] + to_rgb[r][2] * w[2][c]))
}

/// The ACES reference gamut compression curve (power 1.2) on relative chroma `s`: the identity up to
/// `knee`, C1 there and monotone beyond, with `limit` landing on 1. The identity everywhere when
/// `limit` is not past 1.
#[inline]
pub fn knee_curve(s: f32, knee: f32, limit: f32) -> f32 {
    if s <= knee || limit <= 1.0 + 1e-6 {
        return s;
    }
    const P: f32 = 1.2;
    let scale = (limit - knee) / (((1.0 - knee) / (limit - knee)).powf(-P) - 1.0).powf(1.0 / P);
    let u = (s - knee) / scale;
    knee + scale * u / (1.0 + u.powf(P)).powf(1.0 / P)
}

#[inline]
fn horner(c: [f32; 4], t: f32) -> f32 {
    ((c[3] * t + c[2]) * t + c[1]) * t + c[0]
}

/// Where the cubic `c` (coefficients of t⁰..t³) turns in `0..hi`: the bounds of its monotone pieces
/// (`cuts[..=n]`, from 0 to `hi`) and `n`.
fn pieces(c: [f32; 4], hi: f32) -> ([f32; 4], usize) {
    let (a, b, cc) = (3.0 * c[3], 2.0 * c[2], c[1]);
    let mut cuts = [0.0, hi, hi, hi];
    let mut n = 1;
    let disc = b * b - 4.0 * a * cc;
    if disc >= 0.0 && (a != 0.0 || b != 0.0) {
        let q = -0.5 * (b + disc.sqrt().copysign(b));
        let roots = [if a != 0.0 { q / a } else { f32::NAN }, if q != 0.0 { cc / q } else { f32::NAN }];
        for r in roots {
            if r > 0.0 && r < hi {
                cuts[n] = r;
                n += 1;
            }
        }
        if n == 3 && cuts[1] > cuts[2] {
            cuts.swap(1, 2);
        }
    }
    cuts[n] = hi;
    (cuts, n)
}

/// Where the cubic `c` changes sign in `0..hi`: bisection on each of its monotone pieces, then the
/// bracket's secant.
fn sign_changes(c: [f32; 4], hi: f32, steps: u32, out: &mut impl FnMut(f32)) {
    let (cuts, n) = pieces(c, hi);
    for k in 0..n {
        let (mut lo, mut up) = (cuts[k], cuts[k + 1]);
        let below = horner(c, lo) < 0.0;
        if below == (horner(c, up) < 0.0) {
            continue;
        }
        for _ in 0..steps {
            let mid = 0.5 * (lo + up);
            if (horner(c, mid) < 0.0) == below {
                lo = mid;
            } else {
                up = mid;
            }
        }
        // the last bracket's secant
        let (flo, fup) = (horner(c, lo), horner(c, up));
        out(if flo != fup { (lo + (up - lo) * flo / (flo - fup)).clamp(lo, up) } else { 0.5 * (lo + up) });
    }
}

/// The constant-lightness, constant-hue OkLab line through a colour: cube-rooted LMS `l + t·d`, the
/// colour's OkLab chroma `c`.
#[derive(Clone, Copy, Debug)]
struct Line {
    l: f32,
    c: f32,
    d: [f32; 3],
}

impl Line {
    /// `None` for zero or negative lightness and for neutrals, which have no hue to hold.
    fn new(lab: [f32; 3]) -> Option<Line> {
        let [l, a, b] = lab;
        let c = a.hypot(b);
        if !(l > 0.0 && c > 1e-7) {
            return None;
        }
        let m = &mats().from_lab;
        Some(Line { l, c, d: std::array::from_fn(|i| m[i][1] * a + m[i][2] * b) })
    }

    /// Linear RGB at chroma scale `t`, through `m` (see [`lms_to`]).
    #[inline]
    fn at(&self, m: &M3, t: f32) -> [f32; 3] {
        mul3(
            m,
            self.d.map(|d| {
                let q = self.l + t * d;
                q * q * q
            }),
        )
    }

    /// How far inside the gamut `m` chroma scale `t` lies, from 0 to 1 across `zone`: 1 where every
    /// channel is at least `zone.1` inside its faces, 0 where one is `zone.0` or further outside,
    /// linear between. Zones are fractions of the neutral's level `n` (the face at 0) or of its
    /// headroom `1 − n` (the face at 1, with `upper`); negative is outside. Also whether a channel
    /// lies within the zone.
    #[inline]
    fn depth(&self, m: &M3, t: f32, upper: bool, zone: (f32, f32)) -> (f32, bool) {
        let n = self.l * self.l * self.l;
        let (mut depth, mut within) = (1.0f32, false);
        for x in self.at(m, t) {
            let margins = [x / n, if upper { (1.0 - x) / (1.0 - n) } else { f32::INFINITY }];
            for margin in margins {
                let d = if zone.1 > zone.0 {
                    (margin - zone.0) / (zone.1 - zone.0)
                } else if margin >= zone.1 {
                    1.0
                } else {
                    0.0
                };
                within |= d > 0.0 && d < 1.0;
                depth = depth.min(d);
            }
        }
        (depth.clamp(0.0, 1.0), within)
    }

    /// Where the search along the line ends: where the first cube-rooted LMS reaches 0 (every gamut
    /// ends before it, and past it the channels fold back), or at [`MAX_CHROMA`].
    fn reach(&self) -> f32 {
        self.d.iter().filter(|d| **d < 0.0).map(|d| self.l / -d).fold(MAX_CHROMA / self.c, f32::min)
    }

    /// The cubics in the chroma scale that are ≥ 0 where the line is `shift` (see [`Line::depth`])
    /// inside the faces of the gamut `m`: each channel's face at 0 and, with `upper`, at 1 (`[..n]`).
    fn faces(&self, m: &M3, upper: bool, shift: f32) -> ([[f32; 4]; 6], usize) {
        let l = self.l;
        let n3 = l * l * l;
        let mut faces = [[0f32; 4]; 6];
        let mut n = 0;
        for row in m {
            let mut c = [0f32; 4];
            for (w, d) in row.iter().zip(self.d) {
                c[0] += w * l * l * l;
                c[1] += w * 3.0 * l * l * d;
                c[2] += w * 3.0 * l * d * d;
                c[3] += w * d * d * d;
            }
            faces[n] = [c[0] - shift * n3, c[1], c[2], c[3]];
            n += 1;
            if upper {
                faces[n] = [1.0 - shift * (1.0 - n3) - c[0], -c[1], -c[2], -c[3]];
                n += 1;
            }
        }
        (faces, n)
    }

    /// Whether the line stays `shift` inside the gamut `m` all along `0..hi`.
    fn stays_inside(&self, m: &M3, upper: bool, shift: f32, hi: f32) -> bool {
        let (faces, n) = self.faces(m, upper, shift);
        faces[..n].iter().all(|c| {
            let (cuts, k) = pieces(*c, hi);
            cuts[..=k].iter().all(|t| horner(*c, *t) >= 0.0)
        })
    }

    /// The gamut's extent along the line: the integral over `0..hi` of [`Line::depth`] across
    /// `zone`. Where the line leaves the gamut once, that is its edge, moved half the zone outwards
    /// or inwards; folds the line passes through shorten it by how deep they reach, so it moves
    /// smoothly with the colour. Uses 3-point Gauss–Legendre quadrature between the points where a
    /// channel enters or leaves the zone; changes of the limiting face within a piece are approximate.
    fn extent(&self, m: &M3, upper: bool, zone: (f32, f32), hi: f32) -> f32 {
        let mut cuts = [0f32; 38];
        let mut n = 2;
        cuts[1] = hi;
        for shift in [zone.0, zone.1] {
            let (faces, count) = self.faces(m, upper, shift);
            for c in &faces[..count] {
                sign_changes(*c, hi, STEPS, &mut |t| {
                    cuts[n] = t;
                    n += 1;
                });
            }
        }
        let cuts = &mut cuts[..n];
        cuts.sort_by(f32::total_cmp);
        const GAUSS: f32 = 0.774_596_7;
        cuts.windows(2)
            .map(|w| {
                let (mid, half) = (0.5 * (w[0] + w[1]), 0.5 * (w[1] - w[0]));
                let (depth, within) = self.depth(m, mid, upper, zone);
                if !within {
                    return depth * 2.0 * half;
                }
                let side = |t| self.depth(m, t, upper, zone).0;
                half * (5.0 * side(mid - GAUSS * half) + 8.0 * depth + 5.0 * side(mid + GAUSS * half)) / 9.0
            })
            .sum()
    }

    /// The chroma scale the colour moves to: its chroma relative to `extent` compressed past
    /// `knee`, `limit` landing on the extent and anything further on it; `None` when it stays.
    fn compressed(&self, extent: f32, knee: f32, limit: f32) -> Option<f32> {
        if extent.is_nan() || extent <= 0.0 {
            return Some(0.0);
        }
        let s = 1.0 / extent;
        let mapped = knee_curve(s, knee, limit).min(1.0);
        (mapped != s).then_some(mapped * extent)
    }

    /// The scene knee on this line (Rec. 2020 without an upper bound); `None` inside the knee.
    fn scene(&self, knee: f32, limit: f32) -> Option<[f32; 3]> {
        let w = &mats().working;
        // The scene gamut is a cone: normalizing lightness keeps its boundary independent of
        // exposure and the cubic coefficients small enough for HDR samples.
        let normalized = Self { l: 1.0, c: self.c / self.l, d: self.d.map(|v| v / self.l) };
        if normalized.stays_inside(w, false, 0.0, 1.0 / knee) {
            return None;
        }
        let t = normalized.compressed(normalized.extent(w, false, (0.0, 0.0), normalized.reach()), knee, limit)?;
        Some(self.at(w, t))
    }
}

/// Whether destination RGB is inside its cube, with `eps` numerical slack.
#[inline]
fn in_cube(rgb: [f32; 3], eps: f32) -> bool {
    rgb.iter().all(|v| *v >= -eps && *v <= 1.0 + eps)
}

impl Line {
    /// First exit from the expanded cube walking out from gray.
    fn output_chroma(&self, m: &M3, input: f32) -> Option<f32> {
        if !in_cube(self.at(m, 0.0), OUTPUT_EPS) {
            return None;
        }
        let mut chroma = input;
        let (faces, count) = self.faces(m, true, 0.0);
        for face in &faces[..count] {
            let mut face = *face;
            face[0] += OUTPUT_EPS;
            let (cuts, count) = pieces(face, input);
            for k in 0..count {
                let (mut lo, mut up) = (cuts[k], cuts[k + 1]);
                if horner(face, lo) < 0.0 || horner(face, up) >= 0.0 {
                    continue;
                }
                for _ in 0..OUTPUT_STEPS {
                    let mid = 0.5 * (lo + up);
                    if horner(face, mid) >= 0.0 { lo = mid; } else { up = mid; }
                }
                chroma = chroma.min(lo);
            }
        }
        Some(chroma)
    }

    /// At C=qL, RGB=L³*v(q). The largest feasible L minimizes the upper envelope of v's
    /// three cubics on their expanded-cube intervals: stationary points, channel crossings,
    /// or interval endpoints. This avoids bisection on a vanishing-width chroma interval.
    fn output_endpoint(&self, m: &M3) -> (f32, [f32; 3]) {
        let unit = Self { l: 1.0, ..*self };
        let (faces, _) = unit.faces(m, false, 0.0);
        // Nine turns/endpoints, 27 expanded lower-face roots, nine channel crossings, neutral.
        let mut qs = [0.0; 46];
        let mut count = 1;
        let mut add = |q| {
            qs[count] = q;
            count += 1;
        };
        // The neutral endpoint is above L=0.5 in every supported output cube; C is below 2.
        let hi = 2.0 * MAX_CHROMA;
        for i in 0..3 {
            let (turns, n) = pieces(faces[i], hi);
            for q in &turns[1..=n] {
                add(*q);
            }
            // At the upper boundary, RGB=(1+eps)*v/max(v). Each possible maximum
            // channel gives a cubic lower-face constraint (1+eps)*v_i+eps*v_j >= 0.
            for j in 0..3 {
                let lower = std::array::from_fn(|k| (1.0 + OUTPUT_EPS) * faces[i][k] + OUTPUT_EPS * faces[j][k]);
                sign_changes(lower, hi, OUTPUT_STEPS, &mut add);
            }
            for j in 0..i {
                sign_changes(std::array::from_fn(|k| faces[i][k] - faces[j][k]), hi, OUTPUT_STEPS, &mut add);
            }
        }
        let candidates = || {
            qs[..count].iter().filter_map(|q| {
                let rgb = unit.at(m, *q);
                let max = rgb.into_iter().fold(0.0, f32::max);
                let lower = -OUTPUT_EPS * max / (1.0 + OUTPUT_EPS);
                (rgb.iter().all(|v| *v >= lower) && max > 0.0).then_some((*q, rgb, max))
            })
        };
        let minimum = candidates().map(|(_, _, max)| max).fold(f32::INFINITY, f32::min);
        // An almost-flat upper face has indistinguishable minima in f32. Take its outer end
        // rather than letting CPU/GPU rounding choose different ends of the face.
        let (q, rgb, max) = candidates().filter(|(_, _, max)| *max <= minimum + ENDPOINT_EPS).max_by(|a, b| a.0.total_cmp(&b.0)).unwrap_or((
            0.0,
            unit.at(m, 0.0),
            minimum,
        ));
        let scale = (1.0 + OUTPUT_EPS) / max;
        let l = scale.cbrt();
        (q * l, rgb.map(|v| v * scale))
    }
}

/// Relative colorimetric output from working RGB `d` and its destination RGB `r`.
/// Membership is tested directly in `r`, including imaginary ProPhoto colours. Only outside
/// colours move to the first constant-lightness/hue boundary from gray. Near the sRGB and
/// Rec.2020 blue pinches an RGB ray trace blend trades a small hue shift for continuity.
/// If the lightness has no valid chroma, first find the highest feasible lightness at that hue.
/// Identity and search share the `OUTPUT_EPS`-expanded cube. Callers clamp its numerical slack
/// to [0,1] only after mapping, before encoding.
pub fn output(d: [f32; 3], r: [f32; 3], to_out: &M3, space: OutputSpace) -> [f32; 3] {
    if in_cube(r, OUTPUT_EPS) {
        return r;
    }
    output_lab(oklab_from_2020(d), to_out, r, space)
}

fn output_lab([l, a, b]: [f32; 3], to_out: &M3, r: [f32; 3], space: OutputSpace) -> [f32; 3] {
    if l <= 0.0 {
        return [0.0; 3];
    }
    let c = a.hypot(b);
    if c <= 1e-7 {
        return mul3(to_out, [l.min((1.0 + OUTPUT_EPS).cbrt()).powi(3); 3]).map(|v| v.clamp(-OUTPUT_EPS, 1.0 + OUTPUT_EPS));
    }
    // Unit-chroma coefficients keep cubics and their discriminants finite even at 1e30 exposure.
    // Both chroma and lightness of every supported destination cube are below 2.
    let m = &mats().from_lab;
    let mut line = Line { l: l.min(2.0), c: 1.0, d: std::array::from_fn(|i| m[i][1] * (a / c) + m[i][2] * (b / c)) };
    let input = c.min(MAX_CHROMA);
    let chroma = if let Some(chroma) = line.output_chroma(to_out, input) {
        chroma
    } else {
        let (c, rgb) = line.output_endpoint(to_out);
        if c <= input {
            return rgb;
        }
        let (mut lo, mut up) = (0.0, line.l);
        let mut chroma = 0.0;
        // RGB cubes contain every scaled-down colour, so feasible lightness is an interval.
        for _ in 0..OUTPUT_STEPS {
            line.l = 0.5 * (lo + up);
            if let Some(found) = line.output_chroma(to_out, input) {
                lo = line.l;
                chroma = found;
            } else {
                up = line.l;
            }
        }
        line.l = lo;
        chroma
    };
    let clipped = line.at(to_out, chroma);
    let weight = blue_weight(space, b.atan2(a));
    if weight == 0.0 {
        return clipped;
    }
    let gray = line.l.clamp(1e-4, 1.0 - 1e-4).powi(3);
    let mut target = r;
    let mut boundary = [gray; 3];
    for _ in 0..3 {
        let delta = target.map(|v| v - gray);
        let scale = delta.iter().map(|v| {
            if *v > 0.0 { (1.0 - gray) / v } else if *v < 0.0 { -gray / v } else { f32::INFINITY }
        }).fold(f32::INFINITY, f32::min);
        boundary = if scale.is_finite() { delta.map(|v| gray + scale * v) } else { [gray; 3] };
        let working = if space == OutputSpace::Srgb { mul3(&mats().from_srgb, boundary) } else { boundary };
        let lab = oklab_from_2020(working);
        target = line.at(to_out, lab[1].hypot(lab[2]));
    }
    std::array::from_fn(|i| clipped[i] + weight * (boundary[i] - clipped[i]))
}

/// The scene knee on scene-linear Rec. 2020 `c` (no upper bound): chroma past `knee` of the
/// Rec. 2020 gamut's extent is compressed with `limit` times the extent landing on it, and anything
/// beyond on it. Colours inside the knee, neutrals and non-positive lightness return unchanged.
pub fn scene(c: [f32; 3], knee: f32, limit: f32) -> [f32; 3] {
    Line::new(oklab_from_2020(c)).and_then(|line| line.scene(knee, limit)).unwrap_or(c)
}

/// [`scene`] on an OkLab colour, returned as linear Rec. 2020.
pub fn scene_lab(lab: [f32; 3], knee: f32, limit: f32) -> [f32; 3] {
    match Line::new(lab) {
        Some(line) => line.scene(knee, limit).unwrap_or_else(|| line.at(&mats().working, 1.0)),
        None => oklab_to_2020(lab),
    }
}

#[cfg(test)]
mod output_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputSpace;
    use li_color::perceptual::lab_to_lch;

    const KNEE: f32 = crate::tone2::SCENE_GAMUT_KNEE;

    fn hue_error(a: [f32; 3], b: [f32; 3]) -> f32 {
        let (ha, hb) = (lab_to_lch(oklab_from_2020(a))[2], lab_to_lch(oklab_from_2020(b))[2]);
        (ha - hb + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
    }

    /// The colour at OkLab lightness `l`, hue `h` and chroma `c`, in linear Rec. 2020.
    fn lch(l: f32, c: f32, h: f32) -> [f32; 3] {
        oklab_to_2020([l, c * h.cos(), c * h.sin()])
    }

    /// The output gamut's first edge chroma at (l, h), found finely and without slack.
    fn exact_edge(space: OutputSpace, l: f32, h: f32) -> f32 {
        let m = space.from_working();
        let inside = |c: f32| mul3(&m, lch(l, c, h)).iter().all(|v| (0.0..=1.0).contains(v));
        let step = (1..=2000).map(|i| i as f32 * 1e-3).find(|c| !inside(*c)).unwrap_or(2.0);
        let (mut lo, mut hi) = (step - 1e-3, step);
        for _ in 0..30 {
            let mid = 0.5 * (lo + hi);
            if inside(mid) { lo = mid } else { hi = mid }
        }
        lo
    }

    /// The finished output colour of `d` in `space` (knee and final containment), in Rec. 2020.
    fn render(space: OutputSpace, d: [f32; 3]) -> [f32; 3] {
        let to_out = space.from_working();
        let r = mul3(&to_out, d);
        let mapped = crate::finish::gamut_map(output(d, r, &lms_to(&to_out), space), space.luma()).0;
        mul3(&space.rgb_space().to_space(&li_color::REC2020).to_f32(), mapped)
    }

    #[test]
    fn colours_inside_the_knee_are_returned_bit_for_bit() {
        for space in OutputSpace::ALL {
            let to_out = space.from_working();
            let m = lms_to(&to_out);
            for li in 1..20 {
                let l = li as f32 / 20.0;
                for hi in 0..72 {
                    let h = hi as f32 * std::f32::consts::TAU / 72.0;
                    for f in [0.2, 0.6, 0.9, 0.96] {
                        let d = lch(l, f * exact_edge(space, l, h), h);
                        let r = mul3(&to_out, d);
                        assert_eq!(output(d, r, &m, space), r, "{} {d:?}", space.label());
                    }
                }
            }
            // neutrals and black
            for v in [0.0, 0.18, 1.0] {
                let r = mul3(&to_out, [v; 3]);
                assert_eq!(output([v; 3], r, &m, space), r);
            }
        }
    }

    #[test]
    fn colours_pushed_out_of_each_output_gamut_keep_their_hue() {
        // Saturated hues at many lightnesses, from the knee to three times past the output's edge,
        // and Rec. 2020's primaries and secondaries at several levels.
        let mut colours = Vec::new();
        for l in [0.02, 0.05].into_iter().chain((2..20).map(|i| i as f32 * 0.05)) {
            for hi in 0..48 {
                let h = hi as f32 * std::f32::consts::TAU / 48.0;
                colours.push((l, h));
            }
        }
        for space in OutputSpace::ALL {
            let mut worst = [(0f32, [0f32; 3]); 2];
            let mut lightness = [0f32; 2];
            for &(l, h) in &colours {
                let edge = exact_edge(space, l, h);
                for f in [0.99, 1.02, 1.1, 1.5, 3.0] {
                    let d = lch(l, f * edge, h);
                    if mul3(&li_color::perceptual::oklab_matrices()[0], d).iter().any(|v| *v <= 0.0) {
                        continue; // past the spectral locus' cone: no physical colour has this hue
                    }
                    let r = mul3(&space.from_working(), d);
                    if in_cube(r, OUTPUT_EPS) && !in_cube(r, 0.0) {
                        assert_eq!(output(d, r, &lms_to(&space.from_working()), space), r);
                        continue;
                    }
                    let out = render(space, d);
                    let e = hue_error(out, d).abs();
                    let band = usize::from(blue_weight(space, h) > 0.0);
                    if e > worst[band].0 { worst[band] = (e, d); }
                    lightness[band] = lightness[band].max((oklab_from_2020(out)[0] - l).abs());
                }
            }
            for level in [0.1f32, 0.4, 0.9] {
                for c in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 0.0, 1.0]] {
                    let d = c.map(|v| v * level);
                    let out = render(space, d);
                    let e = hue_error(out, d).abs();
                    let lab = oklab_from_2020(d);
                    let band = usize::from(blue_weight(space, lab[2].atan2(lab[1])) > 0.0);
                    if e > worst[band].0 { worst[band] = (e, d); }
                }
            }
            for (band, (hue, light)) in [(1e-3, 2e-4), (0.045, 1e-3)].into_iter().enumerate() {
                assert!(worst[band].0 < hue, "{} band {band}: hue moved by {} rad at {:?}", space.label(), worst[band].0, worst[band].1);
                assert!(lightness[band] < light, "{} band {band}: OkLab lightness moved by {}", space.label(), lightness[band]);
            }
        }
    }

    #[test]
    fn small_changes_of_a_colour_move_its_knees_little() {
        // Nearby working colours exercise sensitivity to the small input differences between
        // CPU and GPU rendering, including hue lines with multiple gamut intervals.
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut random = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 40) as f32 / (1u64 << 24) as f32 * 1.2 - 0.1
        };
        let colours: Vec<[f32; 3]> = (0..20_000).map(|_| [random(), random(), random()]).collect();
        let nudged = |d: [f32; 3]| [d[0] * (1.0 + 2e-5), d[1] * (1.0 - 1e-5), d[2] * (1.0 + 1e-5)];
        let moved = |a: [f32; 3], b: [f32; 3]| (0..3).map(|k| (a[k] - b[k]).abs()).fold(0f32, f32::max);
        for space in [OutputSpace::Srgb, OutputSpace::DisplayP3, OutputSpace::AdobeRgb, OutputSpace::Rec2020] {
            let to_out = space.from_working();
            let m = lms_to(&to_out);
            let knee = |d: [f32; 3]| output(d, mul3(&to_out, d), &m, space);
            let worst = colours.iter().map(|d| moved(knee(*d), knee(nudged(*d)))).fold(0f32, f32::max);
            assert!(worst < 1e-3, "{}: a 2e-5 nudge moves the output knee by {worst}", space.label());
        }
        let knee = |d: [f32; 3]| scene(d.map(|v| v * 3.0), 0.99, 2.5);
        let worst = colours.iter().map(|d| moved(knee(*d), knee(nudged(*d)))).fold(0f32, f32::max);
        assert!(worst < 1e-2, "a 2e-5 nudge moves the scene knee by {worst}");
    }

    #[test]
    fn the_knee_is_continuous_monotone_and_lands_inside_the_gamut() {
        for space in [OutputSpace::Srgb, OutputSpace::DisplayP3, OutputSpace::AdobeRgb] {
            let to_out = space.from_working();
            let m = lms_to(&to_out);
            for (l, h) in [(0.45f32, -1.67f32), (0.63, 0.5), (0.85, 1.9), (0.3, 3.0), (0.7, -2.5)] {
                let edge = exact_edge(space, l, h);
                let chroma = |f: f32| {
                    let d = lch(l, f * edge, h);
                    let out = output(d, mul3(&to_out, d), &m, space);
                    let lab = oklab_from_2020(mul3(&space.rgb_space().to_space(&li_color::REC2020).to_f32(), out));
                    (lab[1].hypot(lab[2]) / edge, out)
                };
                let mut previous = 0.0f32;
                let mut steepest = 0.0f32;
                // up to 2.5 times the edge's chroma: further out the colours leave the LMS cone
                for i in 0..=1600 {
                    let f = 0.9 + i as f32 * 1e-3;
                    let (c, out) = chroma(f);
                    assert!(c.is_finite());
                    assert!(c >= previous - 2e-5, "{} at ({l}, {h}): chroma {c} after {previous} at {f}", space.label());
                    if i > 0 {
                        steepest = steepest.max(c - previous);
                    }
                    previous = c;
                    let inside = in_cube(out, OUTPUT_EPS);
                    assert!(inside, "{} at ({l}, {h}): {out:?} outside the gamut at {f}", space.label());
                }
                // no jump anywhere along the sweep: steps stay near the input's 1e-3
                assert!(steepest < 1.5e-3, "{} at ({l}, {h}): step {steepest}", space.label());
            }
        }
    }

    #[test]
    fn scene_curve_is_c1_at_the_knee_and_maps_the_limit_to_the_edge() {
        let k = KNEE;
        for limit in [1.05f32, 1.6, 2.5, 6.0] {
            let slope = |a: f32, b: f32| (knee_curve(b, k, limit) - knee_curve(a, k, limit)) / (b - a);
            assert_eq!(slope(k - 1e-3, k), 1.0);
            // the slope beyond the knee tends to 1 as the step shrinks (it falls off as u^1.2)
            let (coarse, fine) = (slope(k, k + 1e-3), slope(k, k + 1e-4));
            assert!((fine - 1.0).abs() < 5e-3 && fine > coarse, "slopes {coarse} {fine} at limit {limit}");
            assert!((knee_curve(limit, k, limit) - 1.0).abs() < 1e-5);
            let mut previous = 0.0;
            for i in 0..=1000 {
                let v = knee_curve(i as f32 * limit / 1000.0, k, limit);
                assert!(v >= previous && v <= 1.0 + 1e-5);
                previous = v;
            }
        }
        assert_eq!(knee_curve(1.3, KNEE, 1.0), 1.3, "a limit inside the gamut compresses nothing");
    }

    #[test]
    fn scene_compression_is_independent_of_exposure() {
        for c in [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [-0.1, 0.5, 0.2], [0.8, -0.02, 0.1]] {
            let reference = scene(c, 0.99, 2.5);
            for exposure in [1e-6, 0.01, 10.0, 1000.0, 1e6, 1e30] {
                let out = scene(c.map(|v| v * exposure), 0.99, 2.5).map(|v| v / exposure);
                for i in 0..3 {
                    assert!(
                        out[i].is_finite() && (out[i] - reference[i]).abs() < 2e-5,
                        "{c:?} at exposure {exposure}: {out:?}, expected {reference:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn hostile_inputs_stay_finite() {
        let m = lms_to(&OutputSpace::Srgb.from_working());
        for d in [[0.0; 3], [-1.0, -2.0, -3.0], [1e-30, 0.0, 0.0], [1e6, -1e6, 3.0], [-0.2, 0.5, 0.1], [0.0, 0.0, 5.0], [2.0, 2.0, 2.0]] {
            let r = mul3(&OutputSpace::Srgb.from_working(), d);
            assert!(output(d, r, &m, OutputSpace::Srgb).iter().all(|v| v.is_finite()), "{d:?}");
            assert!(scene(d, 0.99, 2.5).iter().all(|v| v.is_finite()), "{d:?}");
        }
    }

    #[test]
    fn the_scene_knee_holds_hue_and_contains_colours_past_rec2020() {
        let working = &mats().working;
        for li in 1..12 {
            let l = li as f32 * 0.15;
            for hi in 0..36 {
                let h = hi as f32 * std::f32::consts::TAU / 36.0;
                let line = Line::new([l, h.cos(), h.sin()]).unwrap();
                let edge = line.extent(working, false, (0.0, 0.0), line.reach());
                let mut previous = 0.0f32;
                for f in [0.5, 0.95, 1.0, 1.2, 1.6, 2.4] {
                    let c = lch(l, f * edge, h);
                    let out = scene(c, 0.99, 2.5);
                    assert!(hue_error(out, c).abs() < 1e-3, "({l}, {h}) x{f}: {c:?} -> {out:?}");
                    let lab = oklab_from_2020(out);
                    assert!((lab[0] - l).abs() < 1e-3 * l.max(1.0));
                    let n = l * l * l;
                    assert!(out.iter().all(|v| *v >= -1e-3 * n), "({l}, {h}) x{f}: {out:?}");
                    let chroma = lab[1].hypot(lab[2]);
                    assert!(chroma >= previous, "({l}, {h}) x{f}: chroma {chroma} after {previous}");
                    previous = chroma;
                    if f < 0.99 {
                        assert_eq!(out, c);
                    }
                }
            }
        }
    }
}
