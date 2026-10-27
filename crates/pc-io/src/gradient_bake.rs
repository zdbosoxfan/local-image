//! Photoshop gradient appearance -> dense linear stops.
//!
//! Photoshop interpolates gradient colours with a *Smoothness* setting (0-100 %) and, since the
//! 2023 releases, an interpolation *method* (Smooth / Perceptual / Linear / Classic).
//! Native Perceptual fills remain editable; unsupported imported appearances are sampled into
//! dense sRGB stops for the compositors.
//! The model was fitted to Photoshop's own composites in the test corpus (clean-room, from
//! pixels): Smoothness blends linear interpolation with a Catmull-Rom spline through the stop
//! colours (end tangents at half the chord slope); Perceptual interpolates in Oklab, Linear in
//! linear light, Classic in sRGB. Stop midpoints remap each segment piecewise-linearly.
//! Smooth goes straight across each segment in OKLab with its own easing, independent of
//! Smoothness. The easing was measured on Photoshop 27.11 at sixteenth intervals on three colour
//! pairs and is kept in [`SMOOTH_EASE`].

use photocraft_color::{Color, ColorMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    Classic,
    Perceptual,
    Linear,
    Smooth,
}

impl Method {
    pub(crate) fn from_code(code: Option<&[u8]>) -> Method {
        match code {
            Some(b"Perc") => Method::Perceptual,
            Some(b"Lnr ") => Method::Linear,
            Some(b"Smoo") => Method::Smooth,
            _ => Method::Classic,
        }
    }
}

fn lin(c: f32) -> f32 {
    li_color::transfer::srgb_to_linear(c)
}
fn enc(c: f32) -> f32 {
    li_color::transfer::linear_to_srgb(c.clamp(0.0, 1.0))
}

fn to_oklab(rgb: [f32; 3]) -> [f32; 3] {
    li_color::srgb_perceptual::linear_srgb_to_oklab(rgb.map(lin))
}

fn from_oklab(p: [f32; 3]) -> [f32; 3] {
    li_color::srgb_perceptual::oklab_to_linear_srgb(p).map(enc)
}

fn into_space(m: Method, rgb: [f32; 3]) -> [f32; 3] {
    match m {
        Method::Classic => rgb,
        Method::Linear => rgb.map(lin),
        Method::Perceptual | Method::Smooth => to_oklab(rgb),
    }
}

fn out_of_space(m: Method, p: [f32; 3]) -> [f32; 3] {
    match m {
        Method::Classic => p.map(|v| v.clamp(0.0, 1.0)),
        Method::Linear => p.map(enc),
        Method::Perceptual | Method::Smooth => from_oklab(p),
    }
}

// Photoshop 27.11's measured progress along an OKLab segment at sixteenth intervals.
const SMOOTH_EASE: [f32; 17] = [0.0, 0.0438, 0.099, 0.1644, 0.2303, 0.2988, 0.3667, 0.433, 0.4974, 0.5642, 0.6302, 0.6985, 0.7656, 0.8333, 0.8945, 0.9517, 1.0];

/// Colour at `t` for sorted stops (positions in 0..1) in `m`'s space.
fn eval(stops: &[(f32, [f32; 3])], mids: &[f32], smooth: f32, t: f32, method: Method) -> [f32; 3] {
    let n = stops.len();
    if t <= stops[0].0 {
        return stops[0].1;
    }
    if t >= stops[n - 1].0 {
        return stops[n - 1].1;
    }
    let i = stops.windows(2).position(|w| t >= w[0].0 && t <= w[1].0).unwrap_or(0);
    let (t0, t1) = (stops[i].0, stops[i + 1].0);
    let h = (t1 - t0).max(1e-6);
    let mut u = (t - t0) / h;
    // Midpoint: where the segment reaches 50 % (default 0.5 = identity).
    let mid = mids.get(i).copied().unwrap_or(0.5).clamp(0.05, 0.95);
    u = if u <= mid { 0.5 * u / mid } else { 0.5 + 0.5 * (u - mid) / (1.0 - mid) };
    let (p0, p1) = (stops[i].1, stops[i + 1].1);
    if method == Method::Smooth {
        let x = u.clamp(0.0, 1.0) * 16.0;
        let i = (x.floor() as usize).min(15);
        let progress = SMOOTH_EASE[i] + (SMOOTH_EASE[i + 1] - SMOOTH_EASE[i]) * (x - i as f32);
        return std::array::from_fn(|ch| p0[ch] + (p1[ch] - p0[ch]) * progress);
    }
    // Catmull-Rom tangents per unit t; the ends use half the chord slope.
    let tangent = |k: usize| -> [f32; 3] {
        let (a, b) = match k {
            0 => (0, 1),
            k if k == n - 1 => (n - 2, n - 1),
            k => (k - 1, k + 1),
        };
        let d = (stops[b].0 - stops[a].0).max(1e-6);
        let half = if k == 0 || k == n - 1 { 0.5 } else { 1.0 };
        std::array::from_fn(|c| (stops[b].1[c] - stops[a].1[c]) / d * half)
    };
    let (m0, m1) = (tangent(i), tangent(i + 1));
    let (u2, u3) = (u * u, u * u * u);
    let (h00, h10, h01, h11) = (2.0 * u3 - 3.0 * u2 + 1.0, u3 - 2.0 * u2 + u, -2.0 * u3 + 3.0 * u2, u3 - u2);
    std::array::from_fn(|c| {
        let linear = p0[c] + (p1[c] - p0[c]) * u;
        let spline = h00 * p0[c] + h10 * h * m0[c] + h01 * p1[c] + h11 * h * m1[c];
        linear + (spline - linear) * smooth
    })
}

/// Dense linear stops reproducing Photoshop's interpolation. Returns the input unchanged when it
/// already interpolates linearly in sRGB.
pub(crate) fn bake(stops: Vec<(f32, Color)>, midpoints: &[f32], smoothness: f32, method: Method) -> Vec<(f32, Color)> {
    let plain_mids = midpoints.iter().all(|m| (m - 0.5).abs() < 1e-3);
    // Classic interpolates in the stops' own model: Lab stops (Lab documents) blend in L*a*b*, which the
    // compositors' sRGB interpolation can't reproduce (psd-tools 4x4_16bit_lab).
    let lab = method == Method::Classic && stops.iter().all(|(_, c)| c.mode == ColorMode::Lab);
    if stops.len() < 2 || (smoothness <= 0.0 && method == Method::Classic && plain_mids && !lab) {
        return stops;
    }
    let mut s = stops;
    s.sort_by(|a, b| a.0.total_cmp(&b.0));
    let alpha = |t: f32| -> f32 {
        // Stop colours carry no alpha in Photoshop (opacity has its own stops); keep the nearest.
        s.iter().min_by(|a, b| (a.0 - t).abs().total_cmp(&(b.0 - t).abs())).map_or(1.0, |x| x.1.alpha)
    };
    let pts: Vec<(f32, [f32; 3])> = s.iter().map(|(t, c)| (*t, if lab { [c.c[0], c.c[1], c.c[2]] } else { into_space(method, c.to_rgb()) })).collect();
    const N: usize = 96;
    let mut out: Vec<(f32, Color)> = (0..=N)
        .map(|k| {
            let t = k as f32 / N as f32;
            let v = eval(&pts, midpoints, smoothness.clamp(0.0, 1.0), t, method);
            if lab {
                return (t, Color { mode: ColorMode::Lab, c: [v[0].clamp(0.0, 1.0), v[1].clamp(0.0, 1.0), v[2].clamp(0.0, 1.0), 0.0], alpha: alpha(t) });
            }
            let rgb = out_of_space(method, v);
            (t, Color::rgba(rgb[0], rgb[1], rgb[2], alpha(t)))
        })
        .collect();
    out.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(stops: &[(f32, Color)], t: f32) -> [f32; 3] {
        let i = stops.windows(2).position(|w| t >= w[0].0 && t <= w[1].0).unwrap();
        let (a, b) = (&stops[i], &stops[i + 1]);
        let u = (t - a.0) / (b.0 - a.0);
        let (x, y) = (a.1.to_rgb(), b.1.to_rgb());
        std::array::from_fn(|c| x[c] + (y[c] - x[c]) * u)
    }

    #[test]
    fn classic_without_smoothness_is_untouched() {
        let s = vec![(0.0, Color::BLACK), (1.0, Color::WHITE)];
        assert_eq!(bake(s.clone(), &[0.5], 0.0, Method::Classic), s);
    }

    #[test]
    fn smoothness_eases_between_two_stops() {
        // Two stops, 100 % smoothness: t + ½(smoothstep(t) − t) (Catmull-Rom with half-slope ends).
        let s = bake(vec![(0.0, Color::BLACK), (1.0, Color::WHITE)], &[0.5], 1.0, Method::Classic);
        for t in [0.1f32, 0.25, 0.5, 0.75, 0.9] {
            let ss = t * t * (3.0 - 2.0 * t);
            let want = t + 0.5 * (ss - t);
            assert!((at(&s, t)[0] - want).abs() < 2e-3, "{t}: {} vs {want}", at(&s, t)[0]);
        }
    }

    #[test]
    fn perceptual_matches_photoshop_sample() {
        // Measured from Photoshop's composite (ag-psd gradient-mode): orange→blue, Perceptual,
        // 100 % smoothness; at t≈0.491 Photoshop shows (0.592, 0.612, 0.596).
        let s = bake(vec![(0.0, Color::rgb(0.882, 0.602, 0.201)), (1.0, Color::rgb(0.164, 0.568, 0.851))], &[0.5], 1.0, Method::Perceptual);
        let p = at(&s, 0.491);
        for (got, want) in p.iter().zip([0.592f32, 0.612, 0.596]) {
            assert!((got - want).abs() < 0.01, "{p:?}");
        }
        // And near the orange end (t≈0.113 → Photoshop 0.839, 0.608, 0.290).
        let p = at(&s, 0.113);
        for (got, want) in p.iter().zip([0.839f32, 0.608, 0.290]) {
            assert!((got - want).abs() < 0.012, "{p:?}");
        }
    }

    #[test]
    fn smooth_matches_photoshop() {
        // Photoshop 27.11, Gradient tool, Method Smooth, dither off, 256 px wide, every 32 px.
        type Row = (&'static str, [f32; 3], [f32; 3], [[u8; 3]; 8]);
        let measured: [Row; 2] = [
            ("black->white", [0.0; 3], [1.0; 3], [[0; 3], [4; 3], [29; 3], [63; 3], [99; 3], [137; 3], [179; 3], [221; 3]]),
            (
                "red->blue",
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [[255, 0, 0], [232, 51, 67], [202, 71, 105], [171, 81, 136], [140, 83, 162], [109, 80, 188], [76, 69, 213], [41, 49, 236]],
            ),
        ];
        for (name, a, b, want) in measured {
            let s = bake(vec![(0.0, Color::rgb(a[0], a[1], a[2])), (1.0, Color::rgb(b[0], b[1], b[2]))], &[0.5], 1.0, Method::Smooth);
            for (k, w) in want.iter().enumerate() {
                let x = k * 32;
                let got = at(&s, (x as f32 + 0.5) / 256.0).map(|v| (v * 255.0).round() as i32);
                for c in 0..3 {
                    assert!((got[c] - i32::from(w[c])).abs() <= 2, "{name} x {x}: {got:?} vs Photoshop {w:?}");
                }
            }
        }
    }

    #[test]
    fn classic_lab_stops_interpolate_in_lab() {
        // Lab documents blend Classic gradients in L*a*b* (psd-tools 4x4_16bit_lab): the half-way
        // colour is the Lab average, even without smoothness.
        let lab = |l: f32, a: f32, b: f32| Color { mode: ColorMode::Lab, c: [l / 100.0, (a + 128.0) / 255.0, (b + 128.0) / 255.0, 0.0], alpha: 1.0 };
        let s = bake(vec![(0.0, lab(19.07, 52.29, -85.08)), (1.0, lab(54.29, 80.81, 69.91))], &[0.5], 0.0, Method::Classic);
        assert!(s.len() > 2 && s.iter().all(|(_, c)| c.mode == ColorMode::Lab));
        let mid = s.iter().find(|(t, _)| (t - 0.5).abs() < 1e-6).map(|(_, c)| *c).unwrap();
        let want = lab(36.68, 66.55, -7.585);
        for i in 0..3 {
            assert!((mid.c[i] - want.c[i]).abs() < 1e-4, "{mid:?}");
        }
    }

    #[test]
    fn midpoint_shifts_the_half_way_colour() {
        let s = bake(vec![(0.0, Color::BLACK), (1.0, Color::WHITE)], &[0.25], 0.0, Method::Classic);
        assert!((at(&s, 0.25)[0] - 0.5).abs() < 0.01);
    }
}
