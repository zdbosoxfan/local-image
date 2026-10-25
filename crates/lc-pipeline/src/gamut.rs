//! Hue-preserving gamut compression.
//!
//! A colour outside a gamut moves towards the neutral axis at constant OkLab lightness and hue: the
//! constant-lightness projection of Björn Ottosson's "sRGB gamut clipping" (2021,
//! <https://bottosson.github.io/posts/gamutclipping/>), so the OkLab hue the colour tools work in
//! stays where it was. Along that line OkLab's cube-rooted LMS are `L + t·Δ` at chroma scale `t`
//! (`t = 1` is the colour itself), so each RGB channel is a cubic in `t`. The colour's chroma
//! relative to the gamut's extent on the line, `s`, is compressed past a knee along the ACES
//! reference gamut compression curve ([`knee_curve`]); colours inside the knee come back bit for
//! bit.
//!
//! OkLab hue lines bend against an RGB gamut's flat faces, so near some corners (the blue one of
//! sRGB and Rec. 2020; ProPhoto's beyond Rec. 2020) a line leaves the gamut and comes back into it.
//! The extent is therefore not where the line first leaves but how much of it lies inside
//! ([`Line::extent`], from the roots of the channels' cubics): the edge where the line leaves once,
//! shorter by any fold it passes through, so it moves continuously with the colour. The output knee
//! softens its gamut's faces across [`OUTPUT_ZONE`], past the shallow folds a line to a colour
//! inside passes through, so the gamut's own colours keep their place and deeper folds count in
//! smoothly; the final containment clips the sliver past the face. The scene knee feeds the tone
//! curve, which must see colours land on the face rather than past it, so it measures the gamut
//! itself.
//!
//! The GPU kernels (`gamut_*` in `lightcraft-gpu`'s `gamut.wgsl`) are a straight port.

use std::sync::OnceLock;

use li_color::perceptual::{oklab_from_2020, oklab_matrices, oklab_to_2020};

pub type M3 = [[f32; 3]; 3];

/// The zone across which the output knee's gamut edge is softened (see [`Line::depth`]): from the
/// deepest fold of an OkLab hue line to a colour inside sRGB, Display P3, Adobe RGB, Rec. 2020, or
/// ProPhoto within Rec. 2020 (0.65 % of the neutral's level, sRGB's near blue), which still counts
/// as inside so that a gamut's own colours keep their place, out to 2 % past the face.
pub const OUTPUT_ZONE: (f32, f32) = (-0.02, -0.008);
/// Bisection steps that bracket each root of a channel's cubic before its secant.
pub const STEPS: u32 = 12;
/// OkLab chroma where the search along a line ends when none of its cube-rooted LMS falls.
pub const MAX_CHROMA: f32 = 2.0;

struct Mats {
    /// OkLab → cube-rooted LMS.
    from_lab: M3,
    /// LMS → linear Rec. 2020, the working gamut's channels.
    working: M3,
}

fn mats() -> &'static Mats {
    static M: OnceLock<Mats> = OnceLock::new();
    M.get_or_init(|| {
        let [_, from_lms, _, from_lab] = oklab_matrices();
        Mats { from_lab, working: from_lms }
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
fn sign_changes(c: [f32; 4], hi: f32, out: &mut impl FnMut(f32)) {
    let (cuts, n) = pieces(c, hi);
    for k in 0..n {
        let (mut lo, mut up) = (cuts[k], cuts[k + 1]);
        let below = horner(c, lo) < 0.0;
        if below == (horner(c, up) < 0.0) {
            continue;
        }
        for _ in 0..STEPS {
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

    /// Whether the line is inside the gamut `m` at chroma scale `t` (by at least `shift`, see
    /// [`Line::depth`]).
    #[inline]
    fn inside(&self, m: &M3, t: f32, upper: bool, shift: f32) -> bool {
        self.depth(m, t, upper, (shift, shift)).0 >= 1.0
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
    /// smoothly with the colour. Exact for the cubic depth within the zone (3-point Gauss–Legendre
    /// between the points where a channel enters or leaves it).
    fn extent(&self, m: &M3, upper: bool, zone: (f32, f32), hi: f32) -> f32 {
        let mut cuts = [0f32; 38];
        let mut n = 2;
        cuts[1] = hi;
        for shift in [zone.0, zone.1] {
            let (faces, count) = self.faces(m, upper, shift);
            for c in &faces[..count] {
                sign_changes(*c, hi, &mut |t| {
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
        if self.stays_inside(w, false, 0.0, 1.0 / knee) {
            return None;
        }
        let t = self.compressed(self.extent(w, false, (0.0, 0.0), self.reach()), knee, limit)?;
        Some(self.at(w, t))
    }
}

/// The output knee on linear Rec. 2020 `d`, which is `r` in the output space whose [`lms_to`]
/// matrix is `to_out`. Chroma past `knee` of the output gamut's extent (at `d`'s OkLab lightness and
/// hue) is compressed so that the working gamut's extent (Rec. 2020, 0..1) lands on the output's,
/// and anything further lands on it too; where the output reaches past the working gamut, only
/// colours outside the output move, onto its edge. [`crate::finish::gamut_map`] then clips the
/// sliver past a face and whatever else remains outside. Colours inside the knee, lightness outside
/// 0..1, lines without an inside neutral and neutrals return `r` unchanged.
pub fn output(d: [f32; 3], r: [f32; 3], to_out: &M3, knee: f32) -> [f32; 3] {
    let Some(line) = Line::new(oklab_from_2020(d)) else {
        return r;
    };
    if line.l >= 1.0 || line.stays_inside(to_out, true, OUTPUT_ZONE.1, 1.0 / knee) || !line.inside(to_out, 0.0, true, 0.0) {
        return r;
    }
    let hi = line.reach();
    let o = line.extent(to_out, true, OUTPUT_ZONE, hi);
    let w = line.extent(&mats().working, true, OUTPUT_ZONE, hi);
    line.compressed(o, knee, w / o).map_or(r, |t| line.at(to_out, t))
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
mod tests {
    use super::*;
    use crate::output::OutputSpace;
    use li_color::perceptual::lab_to_lch;

    const KNEE: f32 = crate::finish::GAMUT_KNEE;

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
        let mapped = crate::finish::gamut_map(output(d, r, &lms_to(&to_out), KNEE), space.luma()).0;
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
                        assert_eq!(output(d, r, &m, KNEE), r, "{} {d:?}", space.label());
                    }
                }
            }
            // neutrals and black
            for v in [0.0, 0.18, 1.0] {
                let r = mul3(&to_out, [v; 3]);
                assert_eq!(output([v; 3], r, &m, KNEE), r);
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
            let mut worst = (0f32, [0f32; 3]);
            let mut lightness = 0f32;
            for &(l, h) in &colours {
                let edge = exact_edge(space, l, h);
                for f in [0.99, 1.02, 1.1, 1.5, 3.0] {
                    let d = lch(l, f * edge, h);
                    if mul3(&li_color::perceptual::oklab_matrices()[0], d).iter().any(|v| *v <= 0.0) {
                        continue; // past the spectral locus' cone: no physical colour has this hue
                    }
                    let out = render(space, d);
                    let e = hue_error(out, d).abs();
                    if e > worst.0 {
                        worst = (e, d);
                    }
                    lightness = lightness.max((oklab_from_2020(out)[0] - l).abs());
                }
            }
            for level in [0.1f32, 0.4, 0.9] {
                for c in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 0.0, 1.0]] {
                    let d = c.map(|v| v * level);
                    let out = render(space, d);
                    let e = hue_error(out, d).abs();
                    if e > worst.0 {
                        worst = (e, d);
                    }
                }
            }
            // What the final containment clips: slivers of faces the line grazes (and ProPhoto's
            // faces, which fold back past Rec. 2020).
            // What moves the hue at all is the final containment clipping the sliver past a face
            // that the softened edge leaves (under 0.7 degrees); ProPhoto's faces also fold back
            // past Rec. 2020 (its blue primary is imaginary), and colours out there land in a fold.
            let (hue, light) = if space == OutputSpace::ProPhoto { (0.12, 0.02) } else { (0.012, 1.5e-3) };
            assert!(worst.0 < hue, "{}: hue moved by {} rad at {:?}", space.label(), worst.0, worst.1);
            assert!(lightness < light, "{}: OkLab lightness moved by {lightness}", space.label());
        }
    }

    #[test]
    fn small_changes_of_a_colour_move_its_knees_little() {
        // Where a hue line folds in and out of a gamut, an edge taken where the line first leaves
        // jumps between the fold's ends; the extent moves with the colour instead. CPU and GPU
        // inputs differ by about 1e-5, so this is what keeps the two renderers together.
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
            let knee = |d: [f32; 3]| output(d, mul3(&to_out, d), &m, KNEE);
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
                    let out = output(d, mul3(&to_out, d), &m, KNEE);
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
                    let n = l * l * l;
                    let (lo, hi) = (OUTPUT_ZONE.0 * n * 1.001, 1.0 - OUTPUT_ZONE.0 * (1.0 - n) * 1.001);
                    let inside = out.iter().all(|v| *v >= lo && *v <= hi);
                    assert!(inside, "{} at ({l}, {h}): {out:?} outside the gamut at {f}", space.label());
                }
                // no jump anywhere along the sweep: steps stay near the input's 1e-3
                assert!(steepest < 1.5e-3, "{} at ({l}, {h}): step {steepest}", space.label());
            }
        }
    }

    #[test]
    fn the_knee_curve_is_c1_at_the_knee_and_maps_the_limit_to_the_edge() {
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
    fn hostile_inputs_stay_finite() {
        let m = lms_to(&OutputSpace::Srgb.from_working());
        for d in [[0.0; 3], [-1.0, -2.0, -3.0], [1e-30, 0.0, 0.0], [1e6, -1e6, 3.0], [-0.2, 0.5, 0.1], [0.0, 0.0, 5.0], [2.0, 2.0, 2.0]] {
            let r = mul3(&OutputSpace::Srgb.from_working(), d);
            assert!(output(d, r, &m, KNEE).iter().all(|v| v.is_finite()), "{d:?}");
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
