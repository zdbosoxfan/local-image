//! Detail tools (`crate::detail`): sharpening's frequency response, Detail, Masking,
//! halo control and preview scaling; luminance NR's Contrast; colour NR's edge awareness; dehaze
//! without halos; previews against full size. Measurements are printed (`--nocapture`).

use lightcraft_develop::DevelopSettings;
use lightcraft_raster::{Rgb32f, Rgba8};

use crate::{RenderRequest, SourceInfo, StageCache, render, render_cached};

/// A deterministic hash in −1..1.
fn noise(x: usize, y: usize, k: u32) -> f32 {
    let mut v = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ k.wrapping_mul(0xcb1a_b31f);
    v ^= v >> 13;
    v = v.wrapping_mul(0x5bd1_e995);
    v ^= v >> 15;
    (v & 0xffff) as f32 / 32768.0 - 1.0
}

fn settings() -> DevelopSettings {
    DevelopSettings::default()
}

fn shot(src: &Rgb32f, s: &DevelopSettings) -> Rgba8 {
    render(src, &SourceInfo::default(), s, &RenderRequest::fit(src.width, src.height)).image
}

fn shot_at(src: &Rgb32f, s: &DevelopSettings, w: usize) -> Rgba8 {
    render(src, &SourceInfo::default(), s, &RenderRequest::fit(w, w)).image
}

fn luma(p: [u8; 4]) -> f32 {
    0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32
}

/// Gaussian-blurred (σ px, the "lens") linear image of `f(x, y)` (supersampled ×4 per axis).
fn optics(w: usize, h: usize, sigma: f32, f: impl Fn(f32, f32) -> f32 + Sync) -> Rgb32f {
    let img = Rgb32f::from_fn(w, h, |x, y| {
        let mut acc = 0.0;
        for j in 0..4 {
            for i in 0..4 {
                acc += f(x as f32 + (i as f32 + 0.5) / 4.0, y as f32 + (j as f32 + 0.5) / 4.0);
            }
        }
        [acc / 16.0; 3]
    });
    lightcraft_raster::blur::gaussian_fine(&img, sigma)
}

// ---------------------------------------------------------------------------------------------
// Slanted edge: ESF → LSF → MTF (ISO 12233-style, 4× oversampled projection)

const EDGE_W: usize = 160;
const EDGE_H: usize = 160;
/// Edge slope (dx per row): about 5°.
const SLANT: f32 = 0.0875;

fn edge_scene() -> Rgb32f {
    optics(EDGE_W, EDGE_H, 0.9, |x, y| {
        let d = x - (EDGE_W as f32 / 2.0 + (y - EDGE_H as f32 / 2.0) * SLANT);
        if d < 0.0 { 0.04 } else { 0.32 }
    })
}

struct Edge {
    esf: Vec<f32>,
    mtf: Vec<f32>,
}

impl Edge {
    /// Spatial frequency (cycles / px) of bin `k`.
    fn freq(&self, k: usize) -> f32 {
        k as f32 / (self.esf.len() as f32 / 4.0)
    }

    fn mtf_at(&self, f: f32) -> f32 {
        let n = self.esf.len() as f32 / 4.0;
        let x = f * n;
        let k = x.floor() as usize;
        let t = x - k as f32;
        self.mtf[k] + (self.mtf[k + 1] - self.mtf[k]) * t
    }

    fn mtf50(&self) -> f32 {
        for k in 1..self.mtf.len() {
            if self.mtf[k] < 0.5 {
                let t = (self.mtf[k - 1] - 0.5) / (self.mtf[k - 1] - self.mtf[k]);
                return self.freq(k - 1) + t * (self.freq(k) - self.freq(k - 1));
            }
        }
        self.freq(self.mtf.len() - 1)
    }

    /// Largest overshoot past either plateau, as a share of the step (%).
    fn overshoot(&self) -> f32 {
        let n = self.esf.len();
        let lo = self.esf[..n / 8].iter().sum::<f32>() / (n / 8) as f32;
        let hi = self.esf[n - n / 8..].iter().sum::<f32>() / (n / 8) as f32;
        let mx = self.esf.iter().copied().fold(f32::MIN, f32::max);
        let mn = self.esf.iter().copied().fold(f32::MAX, f32::min);
        100.0 * (mx - hi).max(lo - mn).max(0.0) / (hi - lo)
    }
}

/// ESF of the slanted edge in `img` (luma, ±24 px around it, 4 bins / px) and its MTF.
fn measure(img: &Rgba8) -> Edge {
    const HALF: usize = 24;
    let bins = 2 * HALF * 4;
    let (mut sum, mut cnt) = (vec![0f32; bins], vec![0u32; bins]);
    for y in 16..EDGE_H - 16 {
        let cx = EDGE_W as f32 / 2.0 + (y as f32 + 0.5 - EDGE_H as f32 / 2.0) * SLANT;
        for x in 0..EDGE_W {
            // distance along the edge normal
            let d = (x as f32 + 0.5 - cx) / (1.0 + SLANT * SLANT).sqrt();
            let b = ((d + HALF as f32) * 4.0).floor();
            if b >= 0.0 && (b as usize) < bins {
                sum[b as usize] += luma(img.data[y * EDGE_W + x]);
                cnt[b as usize] += 1;
            }
        }
    }
    let mut esf: Vec<f32> = sum.iter().zip(&cnt).map(|(s, c)| s / (*c).max(1) as f32).collect();
    for i in 1..bins {
        if cnt[i] == 0 {
            esf[i] = esf[i - 1];
        }
    }
    // LSF with a Hann window, then |DFT| normalized at DC
    let lsf: Vec<f32> =
        (0..bins - 1).map(|i| (esf[i + 1] - esf[i]) * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (bins - 2) as f32).cos())).collect();
    let n = lsf.len();
    let dft = |k: usize| {
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for (i, v) in lsf.iter().enumerate() {
            let a = std::f32::consts::TAU * k as f32 * i as f32 / n as f32;
            re += v * a.cos();
            im -= v * a.sin();
        }
        re.hypot(im)
    };
    let dc = dft(0).max(1e-6);
    let mtf = (0..=n / 8 + 1).map(|k| dft(k) / dc).collect();
    Edge { esf, mtf }
}

fn sharpened(amount: f64, radius: f64, detail: f64, masking: f64) -> DevelopSettings {
    let mut s = settings();
    s.detail.sharpen_amount = amount;
    s.detail.sharpen_radius = radius;
    s.detail.sharpen_detail = detail;
    s.detail.sharpen_masking = masking;
    s
}

#[test]
fn sharpening_raises_mtf50_and_radius_moves_the_response() {
    let src = edge_scene();
    let base = measure(&shot(&src, &settings()));
    eprintln!("unsharpened: MTF50 {:.3} cy/px, overshoot {:.1} %", base.mtf50(), base.overshoot());
    let gains = |e: &Edge| [0.1f32, 0.2, 0.3, 0.4].map(|f| e.mtf_at(f) / base.mtf_at(f));
    let mut rows = Vec::new();
    for radius in [0.5, 1.0, 2.0, 3.0] {
        let e = measure(&shot(&src, &sharpened(100.0, radius, 100.0, 0.0)));
        let g = gains(&e);
        eprintln!("radius {radius}: MTF50 {:.3}, overshoot {:.1} %, gain at 0.1/0.2/0.3/0.4 cy/px {g:.2?}", e.mtf50(), e.overshoot());
        assert!(e.mtf50() > base.mtf50() * 1.1, "radius {radius}: sharpening raises MTF50");
        rows.push(g);
    }
    // a small radius works on the finest detail, a large one on coarser detail
    let (r05, r3) = (rows[0], rows[3]);
    assert!(r3[0] > r05[0] + 0.05, "radius 3 lifts 0.1 cy/px more than radius 0.5: {r3:?} vs {r05:?}");
    assert!(r05[3] > r05[0], "small radius preferentially boosts fine frequencies");
    // Amount scales the response
    let weak = measure(&shot(&src, &sharpened(40.0, 1.0, 100.0, 0.0)));
    let strong = measure(&shot(&src, &sharpened(150.0, 1.0, 100.0, 0.0)));
    assert!(strong.mtf50() > weak.mtf50() && weak.mtf50() > base.mtf50(), "{} {} {}", base.mtf50(), weak.mtf50(), strong.mtf50());
}

#[test]
fn detail_trades_halo_control_for_fine_detail() {
    let src = edge_scene();
    let lo = measure(&shot(&src, &sharpened(120.0, 1.0, 0.0, 0.0)));
    let hi = measure(&shot(&src, &sharpened(120.0, 1.0, 100.0, 0.0)));
    eprintln!(
        "detail 0: MTF50 {:.3} overshoot {:.1} %; detail 100: MTF50 {:.3} overshoot {:.1} %",
        lo.mtf50(),
        lo.overshoot(),
        hi.mtf50(),
        hi.overshoot()
    );
    // Detail 0 suppresses the halo, Detail 100 doesn't
    assert!(lo.overshoot() < 0.5 * hi.overshoot(), "{} vs {}", lo.overshoot(), hi.overshoot());
    assert!(lo.overshoot() < 8.0, "halo control keeps overshoot small: {}", lo.overshoot());
    // and still sharpens
    let base = measure(&shot(&src, &settings()));
    assert!(lo.mtf50() > base.mtf50() * 1.05);
    // Detail 100 weighs the finest frequencies more (deconvolution-like)
    let hf = |e: &Edge| e.mtf_at(0.4) / e.mtf_at(0.1);
    assert!(
        hi.mtf50() > lo.mtf50(),
        "Detail increases the recoverable frequency: {} vs {}; high-frequency ratios {} vs {}",
        hi.mtf50(),
        lo.mtf50(),
        hf(&hi),
        hf(&lo)
    );
}

/// An edge in the middle and noise everywhere: Masking keeps the noise in flat areas unsharpened
/// while the edge is still sharpened.
#[test]
fn masking_spares_flat_areas() {
    let w = 160;
    let src = Rgb32f::from_fn(w, w, |x, y| {
        let base = if x < w / 2 { 0.06 } else { 0.3 };
        [base * (1.0 + 0.04 * noise(x, y, 3)); 3]
    });
    let flat_std = |img: &Rgba8| {
        let v: Vec<f32> = (10..w - 10).flat_map(|y| (10..60).map(move |x| (x, y))).map(|(x, y)| luma(img.data[y * w + x])).collect();
        let m = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|a| (a - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
    };
    let edge_step = |img: &Rgba8| (10..w - 10).map(|y| luma(img.data[y * w + w / 2]) - luma(img.data[y * w + w / 2 - 1])).sum::<f32>();
    let plain = shot(&src, &settings());
    let m0 = shot(&src, &sharpened(100.0, 1.0, 25.0, 0.0));
    let m100 = shot(&src, &sharpened(100.0, 1.0, 25.0, 100.0));
    let (s_plain, s0, s100) = (flat_std(&plain), flat_std(&m0), flat_std(&m100));
    eprintln!("flat noise σ: none {s_plain:.2}, masking 0 {s0:.2}, masking 100 {s100:.2}");
    assert!(s0 > 1.15 * s_plain, "without a mask the noise is sharpened too");
    assert!(s100 < 1.08 * s_plain, "masking 100 leaves flat areas alone");
    assert!(edge_step(&m100) > 1.05 * edge_step(&plain), "the edge is still sharpened");
    // the Alt preview: white at the edge, black on the flat part
    let req = RenderRequest { overlay: crate::Overlay::SharpenMask, ..RenderRequest::fit(w, w) };
    let ov = render(&src, &SourceInfo::default(), &sharpened(100.0, 1.0, 25.0, 60.0), &req).image;
    assert!(ov.data[80 * w + w / 2][0] > 200 && ov.data[80 * w + 20][0] < 40, "{:?} {:?}", ov.data[80 * w + w / 2], ov.data[80 * w + 20]);
}

/// Average 2 × 2 blocks of an 8-bit image.
fn half(img: &Rgba8) -> Rgba8 {
    let (w, h) = (img.width / 2, img.height / 2);
    let mut data = vec![[0u8; 4]; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = img.data[(2 * y + dy) * img.width + 2 * x + dx];
                for c in 0..4 {
                    acc[c] += p[c] as u32;
                }
            }
            data[y * w + x] = acc.map(|v| ((v + 2) / 4) as u8);
        }
    }
    Rgba8 { width: w, height: h, data }
}

fn mean_abs(a: &Rgba8, b: &Rgba8) -> f32 {
    a.data.iter().zip(&b.data).map(|(p, q)| (0..3).map(|c| p[c].abs_diff(q[c]) as f32).sum::<f32>()).sum::<f32>() / (a.data.len() * 3) as f32
}

#[test]
fn sharpening_preview_matches_the_downscaled_full_size_render() {
    // fine texture on the scale of the radius
    let src = optics(320, 320, 0.7, |x, y| 0.12 + 0.06 * ((x * 0.45).sin() * (y * 0.37).cos()) + if x > 160.0 { 0.15 } else { 0.0 });
    let s = sharpened(120.0, 2.0, 25.0, 0.0);
    let full = half(&shot(&src, &s));
    let full0 = half(&shot(&src, &settings()));
    let prev = shot_at(&src, &s, 160);
    let prev0 = shot_at(&src, &settings(), 160);
    let (effect_full, effect_prev) = (mean_abs(&full, &full0), mean_abs(&prev, &prev0));
    // the radius at preview scale (1 px instead of 2): without it the preview would sharpen
    // twice as coarse a band
    let mut unscaled = s.clone();
    unscaled.detail.sharpen_radius = 3.0;
    let effect_unscaled = mean_abs(&shot_at(&src, &unscaled, 160), &prev0);
    eprintln!(
        "sharpening effect: full size (downscaled) {effect_full:.2} LSB, preview {effect_prev:.2}, preview at a 1.5× radius {effect_unscaled:.2}"
    );
    assert!(effect_prev > 0.5 * effect_full && effect_prev < 1.6 * effect_full);
    assert!((effect_prev - effect_full).abs() < (effect_unscaled - effect_full).abs());
}

// ---------------------------------------------------------------------------------------------
// Noise reduction

/// Flat patches (left) and a fine texture (right, period 6 px, ±0.25 EV), with noise everywhere.
fn texture_scene(w: usize) -> (Rgb32f, Rgb32f) {
    let clean = Rgb32f::from_fn(w, w, |x, y| {
        let t = if x >= w / 2 { 0.25 * ((x as f32 * std::f32::consts::TAU / 6.0).sin() * (y as f32 * 0.6).cos()) } else { 0.0 };
        [0.12 * t.exp2(); 3]
    });
    let noisy = Rgb32f::from_fn(w, w, |x, y| {
        let c = clean.get(x, y);
        let n = 0.08 * noise(x, y, 7);
        [c[0] * (1.0 + n), c[1] * (1.0 + n), c[2] * (1.0 + n)]
    });
    (clean, noisy)
}

fn region_std(img: &Rgba8, x0: usize, x1: usize) -> f32 {
    let w = img.width;
    let v: Vec<f32> = (8..img.height - 8).flat_map(|y| (x0..x1).map(move |x| (x, y))).map(|(x, y)| luma(img.data[y * w + x])).collect();
    let m = v.iter().sum::<f32>() / v.len() as f32;
    (v.iter().map(|a| (a - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
}

/// Correlation of the texture region with the clean render's (texture retention).
fn texture_corr(img: &Rgba8, clean: &Rgba8) -> f32 {
    let w = img.width;
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for y in 8..img.height - 8 {
        for x in w / 2 + 8..w - 8 {
            a.push(luma(img.data[y * w + x]));
            b.push(luma(clean.data[y * w + x]));
        }
    }
    let (ma, mb) = (a.iter().sum::<f32>() / a.len() as f32, b.iter().sum::<f32>() / b.len() as f32);
    let cov: f32 = a.iter().zip(&b).map(|(p, q)| (p - ma) * (q - mb)).sum();
    let va: f32 = a.iter().map(|p| (p - ma).powi(2)).sum();
    let vb: f32 = b.iter().map(|q| (q - mb).powi(2)).sum();
    cov / (va * vb).sqrt()
}

#[test]
fn luminance_nr_contrast_keeps_texture() {
    let w = 160;
    let (clean, noisy) = texture_scene(w);
    let c = shot(&clean, &settings());
    let n = shot(&noisy, &settings());
    let nr = |contrast: f64| {
        let mut s = settings();
        s.detail.nr_luminance = 60.0;
        s.detail.nr_contrast = contrast;
        shot(&noisy, &s)
    };
    let (c0, c100) = (nr(0.0), nr(100.0));
    let (sn, s0, s100) = (region_std(&n, 8, w / 2 - 8), region_std(&c0, 8, w / 2 - 8), region_std(&c100, 8, w / 2 - 8));
    let (tn, t0, t100) = (texture_corr(&n, &c), texture_corr(&c0, &c), texture_corr(&c100, &c));
    eprintln!("flat noise σ: noisy {sn:.2}, NR contrast 0 {s0:.2}, contrast 100 {s100:.2}");
    eprintln!("texture correlation: noisy {tn:.3}, contrast 0 {t0:.3}, contrast 100 {t100:.3}");
    assert!(s0 < 0.5 * sn && s100 < 0.6 * sn, "noise is reduced either way");
    assert!(c0.data != c100.data, "Contrast is read");
    assert!(t100 > t0 + 0.01, "Contrast keeps more of the texture");
    // the variance-stabilised filter smooths deep shadows too
    let dark = Rgb32f::from_fn(w, w, |x, y| [0.01 * (1.0 + 0.25 * noise(x, y, 9)); 3]);
    let mut s = settings();
    s.detail.nr_luminance = 60.0;
    s.light.exposure = 3.0;
    let mut none = s.clone();
    none.detail.nr_luminance = 0.0;
    let (sd, sv) = (region_std(&shot(&dark, &none), 8, w - 8), region_std(&shot(&dark, &s), 8, w - 8));
    eprintln!("shadow noise σ (+3 EV): none {sd:.2}, wavelets {sv:.2}");
    assert!(sv < 0.6 * sd, "shadows: {sv} vs {sd}");
}

/// A sharp red | green edge (about equal luminance, so a luminance guide alone can't hold it)
/// with chroma noise on both sides.
fn colour_edge(w: usize) -> Rgb32f {
    Rgb32f::from_fn(w, w, |x, y| {
        let (r, g) = (0.5 + 0.3 * noise(x, y, 11), 0.5 + 0.3 * noise(x, y, 12));
        let b = 0.5 + 0.3 * noise(x, y, 13);
        let base = if x < w / 2 { [0.30, 0.09, 0.07] } else { [0.06, 0.13, 0.05] };
        [base[0] * (0.7 + 0.6 * r), base[1] * (0.7 + 0.6 * g), base[2] * (0.7 + 0.6 * b)]
    })
}

/// Mean red − green (encoded) per column.
fn redness(img: &Rgba8) -> Vec<f32> {
    let w = img.width;
    (0..w)
        .map(|x| (8..img.height - 8).map(|y| img.data[y * w + x][0] as f32 - img.data[y * w + x][1] as f32).sum::<f32>() / (img.height - 16) as f32)
        .collect()
}

#[test]
fn colour_nr_does_not_bleed_across_a_colour_edge() {
    let w = 160;
    let src = colour_edge(w);
    let mut s = settings();
    s.detail.nr_color = 100.0;
    s.detail.nr_color_smoothness = 100.0;
    let (none, l26) = (shot(&src, &settings()), shot(&src, &s));
    let (r0, r26) = (redness(&none), redness(&l26));
    // leak: the colour difference lost within 6 px of the edge, relative to far away
    let leak = |r: &[f32]| {
        let far = r[10..40].iter().sum::<f32>() / 30.0 - r[w - 40..w - 10].iter().sum::<f32>() / 30.0;
        let near = r[w / 2 - 6..w / 2 - 2].iter().sum::<f32>() / 4.0 - r[w / 2 + 2..w / 2 + 6].iter().sum::<f32>() / 4.0;
        1.0 - near / far
    };
    let chroma_noise = |img: &Rgba8| {
        let v: Vec<f32> = (8..w - 8)
            .flat_map(|y| (8..w / 2 - 12).map(move |x| (x, y)))
            .map(|(x, y)| img.data[y * w + x][0] as f32 - img.data[y * w + x][1] as f32)
            .collect();
        let m = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|a| (a - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
    };
    eprintln!("chroma leak at 2..6 px: none {:.3}, wavelets {:.3}", leak(&r0), leak(&r26));
    eprintln!("chroma noise σ: none {:.2}, wavelets {:.2}", chroma_noise(&none), chroma_noise(&l26));
    assert!(leak(&r26) < 0.08, "Wavelet colour NR keeps the edge: {}", leak(&r26));
    assert!(chroma_noise(&l26) < 0.5 * chroma_noise(&none), "chroma noise is reduced");
    // luminance is untouched by colour NR
    let lum = |img: &Rgba8| img.data.iter().map(|p| luma(*p)).sum::<f32>() / img.data.len() as f32;
    assert!((lum(&l26) - lum(&none)).abs() < 1.5, "{} vs {}", lum(&l26), lum(&none));
}

// ---------------------------------------------------------------------------------------------
// Dehaze

/// A dark tower on a bright hazy sky over a hazy ground getting hazier with distance: J·t + A(1−t).
fn hazy_scene(w: usize, h: usize) -> Rgb32f {
    let air = [0.75, 0.78, 0.85];
    Rgb32f::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        let tower = (0.42..0.58).contains(&fx) && fy > 0.2;
        let (j, t) = if tower {
            ([0.03, 0.03, 0.035], 0.55)
        } else if fy < 0.5 {
            ([0.55, 0.65, 0.8], 0.2) // sky
        } else {
            ([0.12 + 0.05 * (fx * 40.0).sin(), 0.16, 0.08], 0.25 + 0.6 * (fy - 0.5) * 2.0) // ground, nearer below
        };
        [0, 1, 2].map(|k| j[k] * t + air[k] * (1.0 - t))
    })
}

#[test]
fn dehaze_clears_haze_without_halos() {
    let (w, h) = (240, 160);
    let src = hazy_scene(w, h);
    let with = |dz: f64| {
        let mut s = DevelopSettings::default();
        s.effects.dehaze = dz;
        shot(&src, &s)
    };
    let plain = with(0.0);
    let d = with(70.0);
    // contrast between tower and sky rises
    let at = |img: &Rgba8, x: usize, y: usize| luma(img.data[y * w + x]);
    let contrast = |img: &Rgba8| at(img, 30, 40) - at(img, w / 2, 40);
    assert!(contrast(&d) > 1.3 * contrast(&plain), "{} vs {}", contrast(&d), contrast(&plain));
    // halo: along row 40 the sky next to the tower must not differ from the sky far from it
    let ring = |img: &Rgba8| {
        let far = (8..30).map(|x| at(img, x, 40)).sum::<f32>() / 22.0;
        let x0 = (0.42 * w as f32) as usize;
        (x0 - 24..x0 - 1).map(|x| (at(img, x, 40) - far).abs()).fold(0.0, f32::max)
    };
    let (r_plain, r26) = (ring(&plain), ring(&d));
    eprintln!(
        "dehaze 70: sky ring next to the tower: none {r_plain:.1} LSB, guided {r26:.1}; tower/sky contrast {:.1} → {:.1}",
        contrast(&plain),
        contrast(&d)
    );
    assert!(r26 < 3.0, "no halo: {r26}");
    // negative dehaze adds haze consistently: lower contrast, towards the airlight
    let neg = with(-70.0);
    assert!(contrast(&neg) < 0.7 * contrast(&plain));
    let ground = |img: &Rgba8| at(img, 30, h - 10) - at(img, 30, h / 2 + 10);
    assert!(ground(&neg).abs() < ground(&plain).abs() + 1.0);
}

#[test]
fn previews_approximate_full_size() {
    let (clean, noisy) = texture_scene(240);
    let _ = clean;
    let hazy = hazy_scene(240, 240);
    let mut s = settings();
    s.detail.nr_luminance = 40.0;
    s.detail.nr_contrast = 50.0;
    s.detail.nr_color = 50.0;
    for (name, src, s) in [
        ("noise reduction", &noisy, s),
        ("dehaze", &hazy, {
            let mut s = settings();
            s.effects.dehaze = 60.0;
            s
        }),
    ] {
        let full = half(&shot(src, &s));
        let prev = shot_at(src, &s, 120);
        let d = mean_abs(&full, &prev);
        // the same without the tool: the resampling difference alone
        let base = mean_abs(&half(&shot(src, &settings())), &shot_at(src, &settings(), 120));
        eprintln!("{name}: preview vs downscaled full size {d:.2} LSB (resampling alone {base:.2})");
        assert!(d < base + 1.5, "{name}: {d} vs {base}");
    }
}

#[test]
fn cached_renders_match_uncached() {
    let src = std::sync::Arc::new(hazy_scene(200, 140));
    let cache = StageCache::default();
    let info = SourceInfo::default();
    let req = RenderRequest::fit(200, 200);
    let mut s = settings();
    for step in 0..6 {
        match step {
            0 => s.detail.sharpen_amount = 80.0,
            1 => s.detail.sharpen_radius = 2.0,
            2 => s.effects.dehaze = 40.0,
            3 => s.detail.nr_contrast = 40.0,
            4 => s.detail.nr_luminance = 30.0,
            _ => s.detail.nr_color = 40.0,
        }
        let a = render_cached(&src, &info, &s, &req, &cache).image;
        let b = render(&src, &info, &s, &req).image;
        assert_eq!(a.data, b.data, "step {step}");
    }
}

#[test]
fn obsolete_process_field_is_ignored() {
    let mut json = settings().to_json();
    json["process"] = serde_json::json!("v2026");
    assert_eq!(DevelopSettings::from_json(&json).unwrap(), settings());
    json["process"] = serde_json::json!("legacy");
    assert_eq!(DevelopSettings::from_json(&json).unwrap(), settings());
    assert!(settings().to_json_full().get("process").is_none());
}
#[test]
fn tiny_black_negative_and_hdr_detail_stays_finite() {
    for (w, h) in [(1, 1), (2, 7), (7, 2), (19, 13)] {
        for value in [-0.1, 0.0, 1e-7, 0.18, 8.0] {
            let mut src = Rgb32f::filled(w, h, [value; 3]);
            let mut s = settings();
            s.detail.nr_luminance = 100.0;
            s.detail.nr_color = 100.0;
            let p = crate::detail::nr_params(&s, 1.0).unwrap();
            crate::detail::denoise(&mut src, &p);
            assert!(src.data.iter().flatten().all(|v| v.is_finite()));
            let hz = crate::detail::haze_plane(&src, 1.0);
            for strength in [-1.0, 1.0] {
                for (i, c) in src.data.iter().enumerate() {
                    assert!(crate::detail::dehaze_px(*c, strength, hz.at(i, strength), hz.air, hz.distance).iter().all(|v| v.is_finite()));
                }
            }
        }
    }
}
#[test]
fn layer_sharpen_radius_and_sensor_scale_are_used() {
    use lightcraft_develop::{LayerTools, Mask, MaskComponent, MaskShape};
    use lightcraft_geom::Point;
    let src = std::sync::Arc::new(edge_scene());
    let cache = StageCache::default();
    let mut s = settings();
    let mut d = s.detail;
    d.sharpen_amount = 100.0;
    s.masks = vec![Mask {
        components: vec![MaskComponent {
            shape: MaskShape::Radial { center: Point::new(0.5, 0.5), rx: 2.0, ry: 2.0, angle: 0.0, feather: 0.0, invert: false },
            name: None,
            op: lightcraft_develop::MaskOp::Add,
            invert: false,
        }],
        tools: LayerTools { detail: Some(d), ..Default::default() },
        ..Default::default()
    }];
    let req = RenderRequest::fit(EDGE_W, EDGE_H);
    let mut prior = None;
    for (radius, scale) in [(0.5, 1.0), (3.0, 1.0), (3.0, 2.0)] {
        s.masks[0].tools.detail.as_mut().unwrap().sharpen_radius = radius;
        let info = SourceInfo { sensor_scale: scale, ..Default::default() };
        let cached = render_cached(&src, &info, &s, &req, &cache).image;
        assert_eq!(cached, render(&src, &info, &s, &req).image);
        if let Some(prior) = prior {
            assert!(cached != prior, "layer radius and sensor scale alter sharpening");
        }
        prior = Some(cached);
    }
}

#[test]
fn masking_preview_is_grey_even_when_sharpening_is_subpixel() {
    let src = edge_scene();
    let mut s = settings();
    s.detail.sharpen_radius = 0.5;
    let req = RenderRequest { overlay: crate::Overlay::SharpenMask, ..RenderRequest::fit(16, 16) };
    let image = render(&src, &SourceInfo::default(), &s, &req).image;
    // Zero masking is an all-white mask, including when Amount is zero.
    assert!(image.data.iter().all(|p| *p == [255; 4]));
}
