#![allow(clippy::unwrap_used)]
use super::*;
use lightcraft_develop::{ClarityMode, Mask, MaskComponent, MaskOp, MaskShape};
use lightcraft_geom::Point;
fn rows(name: &str) -> Vec<Vec<f32>> {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sliders").join(name))
        .unwrap()
        .lines()
        .map(|l| l.split(',').map(|v| v.parse().unwrap()).collect())
        .collect()
}
#[test]
fn extracted_c_eigf_both_paths_and_blends() {
    let input = GrayFrame::from_fn(37, 29, |x, y| {
        0.002 + 0.013 * x as f32 + 0.0007 * y as f32 + if x > 18 { 0.7 } else { 0.0 } + 0.002 * (x as f32 * 0.7 + y as f32 * 0.3).sin()
    });
    let fixtures = rows("eigf.csv");
    for mode in 0..4 {
        let mut p = eigf::Params::new(if mode == 0 { 1.7 } else { 6.3 }, 0.08);
        p.iterations = 3;
        p.quantization = if mode > 1 { 0.5 } else { 0. };
        p.geometric = mode == 3;
        let out = eigf::filter(&input, p);
        let max = fixtures.iter().filter(|r| r[0] as usize == mode).map(|r| (out.data[r[1] as usize] - r[2]).abs()).fold(0., f32::max);
        assert!(max < 3e-6, "EIGF mode {mode}: {max}");
    }
}
#[test]
fn extracted_c_local_laplacian_full_pyramids() {
    let input = GrayFrame::from_fn(50, 37, |x, y| 0.3 + 0.2 * ((x as f32 * 0.3).sin() * (y as f32 * 0.2).cos()) + if x > 25 { 0.3 } else { 0. });
    let fixtures = rows("llf.csv");
    for mode in 0..3 {
        let out =
            llf::local_laplacian(&input, 0.2, if mode == 1 { 0.6 } else { 1. }, if mode == 1 { 1.4 } else { 1. }, if mode == 2 { 0.8 } else { 0. });
        let max = fixtures.iter().filter(|r| r[0] as usize == mode).map(|r| (out.data[r[1] as usize] - r[2]).abs()).fold(0., f32::max);
        assert!(max < 3e-6, "LLF mode {mode}: {max}");
    }
}
#[test]
fn extracted_c_ucs_hdr_roundtrips() {
    for r in rows("ucs.csv") {
        let i = r[0];
        let xy = [0.25 + 0.005 * i, 0.23 + 0.003 * i, (-12. + i * 0.5).exp2()];
        let lw = ucs::y_to_l_star(1.);
        let j = ucs::xyy_to_jch(xy, lw);
        let h = ucs::jch_to_hsb(j);
        let back = ucs::jch_to_xyy(ucs::hsb_to_jch(h), lw);
        for (actual, &expected) in j.into_iter().chain(h).chain(back).zip(&r[1..]) {
            assert!((actual - expected).abs() < 4e-5 * expected.abs().max(1.), "{actual} != {expected}");
        }
    }
}
#[test]
fn extracted_c_colour_equalizer_uv_covariance_filters() {
    let (w, h) = (37, 29);
    let uv: Vec<_> = (0..w * h)
        .flat_map(|i| {
            let (x, y) = ((i % w) as f32, (i / w) as f32);
            [0.03 * (0.13 * x).sin() + if x > 18. { 0.01 } else { 0. }, 0.04 * (0.17 * y).cos()]
        })
        .collect();
    let pre = colorequal::guided_uv(&uv, &uv, w, h, 7.3, 1e-5, false);
    let wt = 1. / (1. + (-60f32 * 0.1).exp());
    let uv: Vec<_> = uv.iter().zip(pre).map(|(a, b)| a + wt * (b - a)).collect();
    let input: Vec<_> = (0..w * h).flat_map(|i| [1. + 0.2 * ((i % w) as f32 * 0.1).sin(), 0.03 * ((i / w) as f32 * 0.1).cos()]).collect();
    let corr = colorequal::guided_uv(&uv, &input, w, h, 8.5, 1e-6, true);
    let mut max = 0f32;
    for row in rows("colorequal-filter.csv") {
        let i = row[1] as usize;
        let actual = if row[0] == 0. { [uv[2 * i], uv[2 * i + 1]] } else { [1. + (corr[2 * i] - 1.) * wt, corr[2 * i + 1] * 0.95 * wt] };
        for (v, &refv) in actual.into_iter().zip(&row[2..]) {
            max = max.max((v - refv).abs());
        }
    }
    assert!(max < 5e-6, "colour filter max {max}");
}
#[test]
fn candidate_step_order_detail_hue_and_clip_protection() {
    let src = RgbFrame::from_fn(100, 60, |x, y| {
        let v = if x < 50 { 0.025 } else { 3. };
        let t = 1. + 0.01 * (y as f32 * 0.8).sin();
        [v * t; 3]
    });
    for method in [primary::HsMethod::LiTone, primary::HsMethod::Eigf] {
        for value in [-100., -50., 50., 100.] {
            let mut s = DevelopSettings::default();
            s.light.highlights = value;
            s.light.shadows = -value;
            let info = SourceInfo::default();
            let plan = crate::plan(&src, &info, &s, &RenderRequest::fit(100, 60));
            let out = primary::process(&src, &src, &info, &plan, method);
            assert!(out.get(49, 30)[0] < out.get(50, 30)[0], "gradient reversal {method:?} {value}");
            let det0 = (src.get(75, 10)[0] / src.get(75, 11)[0]).log2();
            let det1 = (out.get(75, 10)[0] / out.get(75, 11)[0]).log2();
            assert!((det0 - det1).abs() < 0.005, "detail {method:?} {value}: {det0} {det1}");
        }
    }
    let src = RgbFrame::filled(24, 16, [8.; 3]);
    let mut s = DevelopSettings::default();
    s.light.highlights = -100.;
    let info = SourceInfo {
        clip_confidence: Some(Arc::new(ClipConfidence { width: 24, height: 16, data: vec![1.; 384] })),
        raw_clip_level: Some(0.99),
        ..Default::default()
    };
    let plan = crate::plan(&src, &info, &s, &RenderRequest::fit(24, 16));
    for method in [primary::HsMethod::LiTone, primary::HsMethod::Eigf] {
        let out = primary::process(&src, &src, &info, &plan, method);
        assert!(out.data.iter().all(|c| (c[0] - 8.).abs() < 1e-5));
    }
    let mut s = DevelopSettings::default();
    s.light.highlights = -100.;
    s.light.shadows = 100.;
    let src = RgbFrame::filled(32, 24, [1.5, 0.8, 0.3]);
    let info = SourceInfo::default();
    let plan = crate::plan(&src, &info, &s, &RenderRequest::fit(32, 24));
    let out = primary::process(&src, &src, &info, &plan, primary::DEFAULT_HS_METHOD);
    assert!((out.data[0][0] / out.data[0][1] - 1.5 / 0.8).abs() < 1e-5);
}
#[test]
fn modes_structure_skin_layers_and_cache_changes() {
    let src = Arc::new(RgbFrame::from_fn(80, 60, |x, y| [0.3 + 0.02 * (x as f32 * 0.6).sin(), 0.15 + 0.007 * (y as f32 * 0.7).cos(), 0.08]));
    let info = SourceInfo::default();
    let req = RenderRequest::fit(80, 60);
    let cache = StageCache::default();
    let mut s = DevelopSettings::default();
    s.effects.clarity = 70.;
    s.effects.structure = 60.;
    s.effects.texture = 50.;
    let natural = render_cached(&src, &info, &s, &req, &cache).image;
    for mode in [ClarityMode::Punch, ClarityMode::Neutral] {
        s.effects.clarity_mode = mode;
        let a = render_cached(&src, &info, &s, &req, &cache).image;
        assert_ne!(a, natural);
        assert_eq!(a, render(&src, &info, &s, &req).image);
    }
    s.effects = Default::default();
    let reference = skin_reference_sample(&src, &info, &s, &req, Point::new(0.5, 0.5)).unwrap();
    s.skin_tone.reference = Some(reference);
    s.skin_tone.uniformity = 100.;
    let skin = render(&src, &info, &s, &req).image;
    assert_ne!(skin, render(&src, &info, &DevelopSettings::default(), &req).image);
    let mut layer = DevelopSettings::default();
    let tools = lightcraft_develop::LayerTools {
        skin_tone: Some(s.skin_tone),
        effects: Some(lightcraft_develop::Effects { structure: 60., ..Default::default() }),
        ..Default::default()
    };
    layer.masks.push(Mask {
        tools,
        components: vec![MaskComponent {
            name: None,
            op: MaskOp::Add,
            invert: false,
            shape: MaskShape::Linear { start: Point::new(1., 0.), end: Point::new(1., 1.) },
        }],
        ..Default::default()
    });
    assert!(!layers_need_cpu(&layer));
    assert_ne!(render(&src, &info, &layer, &req).image, render(&src, &info, &DevelopSettings::default(), &req).image);
}
#[test]
fn old_settings_default_new_controls_and_layer_roundtrip() {
    let mut s = DevelopSettings::from_json(&serde_json::json!({"effects":{"texture":12,"clarity":20}})).unwrap();
    assert_eq!(s.effects.clarity_mode, ClarityMode::Natural);
    assert_eq!(s.effects.structure, 0.);
    assert!(lightcraft_develop::controls::set(&mut s, "effects.structure", 400.));
    assert_eq!(s.effects.structure, 100.);
    let t = lightcraft_develop::LayerTools::default()
        .merged((6500., 0.), &serde_json::json!({"skin_tone":{"uniformity":60.},"effects":{"structure":30.,"clarity_mode":"punch"}}))
        .unwrap();
    assert!(t.is_set("skin_tone"));
    assert_eq!(t.view((6500., 0.)).effects.clarity_mode, ClarityMode::Punch);
}

#[test]
fn extracted_c_balance_complete_pixel_ucs_and_alternate_jz() {
    for row in rows("balance.csv") {
        let mode = row[0] as usize;
        let i = row[1];
        let v = (-12. + i * 0.5).exp2();
        let mut b = balance::Balance::new(&DevelopSettings::default());
        b.white = 1.;
        b.grey = 0.18;
        b.mask_grey = 0.18f32.powf(0.410_120_58);
        b.shadows_weight = 4.;
        b.highlights_weight = 4.;
        b.midtones_weight = 8.;
        b.midtones_y = 0.93;
        b.contrast = 1.08;
        for c in 0..3 {
            b.global[c] = 0.002 * (c + 1) as f32;
            b.shadows[c] = 0.9 + 0.05 * c as f32;
            b.highlights[c] = 1.15 - 0.04 * c as f32;
            b.midtones[c] = 0.94 + 0.04 * c as f32;
            b.chroma_masks[c] = 0.03 * (c as f32 - 1.);
            b.saturation_masks[c] = 0.05 * (c as f32 - 1.);
            b.brilliance_masks[c] = 0.02 * (c as f32 - 1.);
        }
        b.vibrance = 0.35;
        b.chroma = 0.08;
        b.saturation = 0.25;
        b.brilliance = 0.04;
        b.hue_angle = 0.1;
        b.formula = if mode == 0 { balance::Formula::Ucs } else { balance::Formula::JzAzBz };
        let lut = std::array::from_fn(|i| (if mode == 0 { 0.025 } else { 0.7 }) * (1. + 0.1 * (i as f32 * 0.03).sin()));
        let out = b.apply_with_gamut([v * (0.3 + 0.01 * i), v * 0.7, v * (0.9 - 0.01 * i)], &lut);
        for (a, e) in out.into_iter().zip(&row[2..]) {
            assert!((a - e).abs() < 4e-4 * e.abs().max(1.), "balance {mode} {i}: {a} != {e}");
        }
    }
}
#[test]
fn extracted_c_jz_hdr_roundtrips_and_neutral_stability() {
    for row in rows("jz.csv") {
        let i = row[0];
        let v = (-12. + i * 0.5).exp2();
        let j = balance::xyz_to_jz([v * 0.95, v, v * 1.08]);
        let back = balance::jz_to_xyz(j);
        for (a, e) in j.into_iter().zip(&row[1..4]) {
            assert!((a - e).abs() < 6e-6 * e.abs().max(1.), "Jz {i}: {a} != {e}");
        }
        for (a, e) in back.into_iter().zip(&row[4..]) {
            assert!((a - e).abs() < 3e-4 * e.abs().max(1.), "Jz inverse {i}: {a} != {e}");
        }
    }
    assert!(balance::Balance::new(&DevelopSettings::default()).is_identity());
}

#[test]
fn clipping_metadata_compresses_losslessly_and_defaults_for_old_sources() {
    let info =
        SourceInfo { clip_confidence: Some(Arc::new(ClipConfidence { width: 512, height: 384, data: vec![0.; 512 * 384] })), ..Default::default() };
    let bytes = serde_json::to_vec(&info).unwrap();
    assert!(bytes.len() < 4096);
    assert!(serde_json::from_slice::<SourceInfo>(&bytes).unwrap() == info);
    assert!(serde_json::from_str::<SourceInfo>("{\"raw\":true}").unwrap().clip_confidence.is_none());
    assert!(serde_json::from_str::<ClipConfidence>("{\"width\":1,\"height\":1,\"data\":[[100000001,0.0]]}").is_err());
}

#[test]
fn skin_uniformity_retains_pore_chroma_and_protects_lips() {
    let lw = ucs::y_to_l_star(1.);
    let jch_rgb = |j| ucs::hsb_to_rgb(ucs::jch_to_hsb(j), lw);
    let src = RgbFrame::from_fn(160, 80, |x, _| jch_rgb([0.5, 0.04 + 0.003 * (x as f32 * 0.03).sin() + 0.0007 * (x as f32 * 1.8).sin(), 0.8]));
    let mut out = src.clone();
    let mut skin = lightcraft_develop::SkinTone {
        reference: Some([0.5, 0.04, 0.8f64.to_degrees()]),
        uniformity: 100.,
        hue_range: 90.,
        chroma_range: 100.,
        lightness_range: 100.,
        protect_lips: false,
        ..Default::default()
    };
    primary::skin(&mut out, &skin, 6000.);
    let chroma = |img: &RgbFrame, x| ucs::xyy_to_jch(ucs::xyz_to_xyy(ucs::mul(&ucs::mats().rgb_to_xyz, img.get(x, 40))), lw)[1];
    // Second differences isolate fine pores from the low-frequency chroma unevenness.
    let energy = |img: &RgbFrame| {
        (10..150)
            .map(|x| {
                let v = chroma(img, x - 1) - 2. * chroma(img, x) + chroma(img, x + 1);
                v * v
            })
            .sum::<f32>()
    };
    let ratio = energy(&out) / energy(&src);
    assert!((0.85..1.15).contains(&ratio), "pore chroma ratio {ratio}");
    let lip = RgbFrame::filled(20, 12, jch_rgb([0.5, 0.04, 0.2]));
    skin.protect_lips = true;
    let mut protected = lip.clone();
    primary::skin(&mut protected, &skin, 6000.);
    assert!(protected == lip);
    skin.protect_lips = false;
    let mut unprotected = lip.clone();
    primary::skin(&mut unprotected, &skin, 6000.);
    assert!(unprotected != lip);
}

#[test]
fn every_neutral_primary_control_skips_its_stage() {
    let mut s = DevelopSettings::default();
    assert_eq!(primary::Stages::of(&s), primary::Stages::default());
    assert!(!primary::active(&s));
    assert!(!primary::needs_proxy(&s));
    // Modes, hue picks and window widths do not activate a neutral tool.
    s.effects.clarity_mode = ClarityMode::Punch;
    s.grading.global.hue = 147.;
    s.grading.shadows.hue = 203.;
    s.grading.blending = 75.;
    s.skin_tone.reference = Some([0.5, 0.13, 45.]);
    s.skin_tone.hue_range = 30.;
    assert_eq!(primary::Stages::of(&s), primary::Stages::default());
    assert!(!primary::active(&s));
    let controls: [fn(&mut DevelopSettings, f64); 11] = [
        |s, v| s.light.highlights = v,
        |s, v| s.light.shadows = v,
        |s, v| s.effects.clarity = v,
        |s, v| s.effects.texture = v,
        |s, v| s.effects.structure = v,
        |s, v| s.color.vibrance = v,
        |s, v| s.color.saturation = v,
        |s, v| s.mixer.orange.hue = v,
        |s, v| s.grading.shadows.sat = v,
        |s, v| s.skin_tone.uniformity = v,
        |s, v| s.skin_tone.lightness = v,
    ];
    for set in controls {
        set(&mut s, 30.);
        assert!(primary::Stages::of(&s).any());
        set(&mut s, 0.);
        assert_eq!(primary::Stages::of(&s), primary::Stages::default());
        assert!(!primary::active(&s));
    }
    // Whites and Blacks reshape the global tone map in the finish stage, never this one.
    s.light.whites = 30.;
    s.light.blacks = -30.;
    assert!(!primary::active(&s));
    s.light.whites = 0.;
    s.light.blacks = 0.;
    let src = RgbFrame::from_fn(19, 13, |x, y| [0.002 + (x as f32 / 18.).exp2(), 0.02 + y as f32 / 40., 0.1]);
    let info = SourceInfo::default();
    let req = RenderRequest::fit(19, 13);
    assert_eq!(render(&src, &info, &s, &req).image, render(&src, &info, &DevelopSettings::default(), &req).image);
    // A zero tool section on a layer also leaves the entire primary stage inactive.
    s.masks.push(Mask {
        components: vec![MaskComponent {
            name: None,
            op: MaskOp::Add,
            invert: false,
            shape: MaskShape::Linear { start: Point::new(0., 0.), end: Point::new(1., 0.) },
        }],
        tools: lightcraft_develop::LayerTools {
            light: Some(Default::default()),
            effects: Some(Default::default()),
            color: Some(Default::default()),
            mixer: Some(Default::default()),
            grading: Some(Default::default()),
            skin_tone: Some(s.skin_tone),
            ..Default::default()
        },
        ..Default::default()
    });
    assert!(!primary::active(&s));
    assert!(!primary::needs_proxy(&s));
    s.masks[0].components.clear();
    s.masks[0].adjust.clarity = 70.;
    assert!(!primary::active(&s));
    assert!(!primary::needs_proxy(&s));
}

/// Mean 8-bit green level of column `x` (averages the dither over the rows).
fn column_level(img: &Rgba8, x: usize) -> f32 {
    (0..img.height).map(|y| img.get(x, y)[1] as f32).sum::<f32>() / img.height as f32
}

#[test]
fn whites_and_blacks_move_the_white_and_black_points() {
    let (w, h) = (512, 8);
    let (lo, hi) = (0.003f32.ln(), 1.2f32.ln());
    let src = RgbFrame::from_fn(w, h, |x, _| [(lo + (hi - lo) * x as f32 / (w - 1) as f32).exp(); 3]);
    let column = |v: f32| ((v.ln() - lo) / (hi - lo) * (w - 1) as f32).round() as usize;
    let shot = |info: &SourceInfo, highlights: f64, whites: f64, blacks: f64| {
        let mut s = DevelopSettings::default();
        s.light.highlights = highlights;
        s.light.whites = whites;
        s.light.blacks = blacks;
        render(&src, info, &s, &RenderRequest::fit(w, h)).image
    };
    let level = |img: &Rgba8, v: f32| column_level(img, column(v));
    let raw = SourceInfo { raw: true, ..Default::default() };
    // With Highlights set too, the primary stage runs and must leave Whites to the finish.
    for highlights in [0., -30.] {
        let neutral = shot(&raw, highlights, 0., 0.);
        let up = shot(&raw, highlights, 100., 0.);
        let down = shot(&raw, highlights, -100., 0.);
        for v in [0.7, 1.0] {
            assert!(level(&up, v) - level(&neutral, v) >= 6., "Whites +100 at {v} (highlights {highlights})");
            assert!(level(&neutral, v) - level(&down, v) >= 10., "Whites -100 at {v} (highlights {highlights})");
        }
        for img in [&up, &down] {
            assert!((level(img, 0.05) - level(&neutral, 0.05)).abs() <= 1., "Whites moved the shadows (highlights {highlights})");
        }
        let crushed = shot(&raw, highlights, 0., -100.);
        assert!(level(&neutral, 0.01) - level(&crushed, 0.01) >= 8., "Blacks -100 (highlights {highlights})");
        assert!((level(&crushed, 0.7) - level(&neutral, 0.7)).abs() <= 1., "Blacks moved the highlights (highlights {highlights})");
    }
    let rendered = SourceInfo::default();
    let neutral = shot(&rendered, 0., 0., 0.);
    assert!(level(&shot(&rendered, 0., 100., 0.), 0.7) > level(&neutral, 0.7) + 2.);
    assert!(level(&shot(&rendered, 0., -100., 0.), 0.7) < level(&neutral, 0.7) - 2.);
    assert!(level(&shot(&rendered, 0., 0., -100.), 0.01) < level(&neutral, 0.01) - 2.);
}

#[test]
fn clarity_strength_is_even_across_tones() {
    let (w, h) = (1000, 48);
    let mut s = DevelopSettings::default();
    s.effects.clarity = 100.;
    s.effects.clarity_mode = ClarityMode::Neutral;
    let info = SourceInfo::default();
    let amplitude = |img: &RgbFrame| {
        let row = &img.data[(h / 2) * w + 200..(h / 2) * w + 800];
        let l: Vec<f32> = row.iter().map(|c| c[1].log2()).collect();
        l.iter().copied().fold(f32::MIN, f32::max) - l.iter().copied().fold(f32::MAX, f32::min)
    };
    let mut excess = Vec::new();
    for k in -8..=8 {
        let e = k as f32 * 0.25;
        let src = RgbFrame::from_fn(w, h, |x, _| [0.18 * (e + 0.15 * (std::f32::consts::TAU * x as f32 / 48.).sin()).exp2(); 3]);
        let plan = crate::plan(&src, &info, &s, &RenderRequest::fit(w, h));
        let out = primary::process(&src, &src, &info, &plan, primary::DEFAULT_HS_METHOD);
        let ratio = amplitude(&out) / amplitude(&src);
        assert!(ratio > 1.08, "Clarity +100 at {e:+} EV: local contrast ratio {ratio}");
        excess.push((ratio - 1.) / (-(e / 3.5).powi(2)).exp());
    }
    let (lo, hi) = excess.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    assert!((hi - lo) / hi < 0.1, "Clarity strength varies with tone: {excess:?}");
}

/// Linear Rec.2020 of an 8-bit sRGB output pixel.
fn decoded_2020(c: [u8; 4]) -> [f32; 3] {
    let lin = [c[0], c[1], c[2]].map(|v| li_color::transfer::srgb_to_linear(v as f32 / 255.));
    li_color::SRGB.to_space(&li_color::REC2020).apply_f32(lin)
}

#[test]
fn hsl_luminance_darkens_saturated_blues_without_reaching_black() {
    let info = SourceInfo { raw: true, ..Default::default() };
    let deep_blue = li_color::SRGB.to_space(&li_color::REC2020).apply_f32([40., 90., 200.].map(|v| li_color::transfer::srgb_to_linear(v / 255.)));
    for patch in [[0.08, 0.14, 0.32], [0.03, 0.07, 0.30], deep_blue] {
        let src = RgbFrame::filled(32, 24, patch);
        let shot = |lum: f64| {
            let mut s = DevelopSettings::default();
            s.mixer.blue.lum = lum;
            decoded_2020(render(&src, &info, &s, &RenderRequest::fit(32, 24)).image.get(16, 12))
        };
        let lab = |c: [f32; 3]| li_color::perceptual::oklab_from_2020(c);
        let neutral = shot(0.);
        let mut previous = li_color::luminance_2020(neutral);
        for lum in [-30., -60., -100.] {
            let c = shot(lum);
            let y = li_color::luminance_2020(c);
            assert!(y > 0. && y < previous, "{patch:?} Blue Luminance {lum}: Y {y} after {previous}");
            let (a, b) = (lab(c), lab(neutral));
            let dh = (a[2].atan2(a[1]) - b[2].atan2(b[1])).abs();
            assert!(dh < 0.05, "{patch:?} Blue Luminance {lum} turned the hue by {dh} rad");
            previous = y;
        }
        assert!(previous >= li_color::luminance_2020(neutral) * 2f32.powf(-2.5), "{patch:?} Blue Luminance -100 is darker than -2.5 EV");
        let up = li_color::luminance_2020(shot(100.)) / li_color::luminance_2020(neutral);
        assert!((1.5..=2f32.powf(1.5)).contains(&up), "{patch:?} Blue Luminance +100 gives {up}x luminance");
    }
}

#[test]
fn hue_band_curves_stay_between_their_nodes() {
    let hues = colorequal::hues();
    let at = |lut: &[f32], hue: f32| crate::ucs::lookup_gamut(lut, hue);
    for band in 0..8 {
        for value in [-100., 100.] {
            let mut s = DevelopSettings::default();
            let set: [&mut lightcraft_develop::Hsl; 8] = [
                &mut s.mixer.red,
                &mut s.mixer.orange,
                &mut s.mixer.yellow,
                &mut s.mixer.green,
                &mut s.mixer.aqua,
                &mut s.mixer.blue,
                &mut s.mixer.purple,
                &mut s.mixer.magenta,
            ];
            set.into_iter().nth(band).unwrap().lum = value;
            let [_, _, bright, _] = colorequal::luts(&s);
            let node = 1. + value as f32 / 200.;
            assert!((at(&bright, hues[band]) - node).abs() < 2e-3, "band {band} {value}: its own hue misses the node");
            let (lo, hi) = (node.min(1.), node.max(1.));
            assert!(bright.iter().all(|v| (lo - 1e-6..=hi + 1e-6).contains(v)), "band {band} {value} overshoots its node");
            // Beyond the neighbouring bands the curve is untouched.
            let mut sorted: Vec<f32> = hues.to_vec();
            sorted.sort_by(f32::total_cmp);
            let k = sorted.iter().position(|h| *h == hues[band]).unwrap();
            let (before, after) = (sorted[(k + 7) % 8], sorted[(k + 1) % 8]);
            for i in 0..512 {
                let hue = i as f32 * std::f32::consts::TAU / 512. - std::f32::consts::PI;
                let inside = |a: f32, b: f32| (hue - a).rem_euclid(std::f32::consts::TAU) <= (b - a).rem_euclid(std::f32::consts::TAU);
                if !inside(before, after) {
                    assert!((bright[i] - 1.).abs() < 1e-6, "band {band} {value} changed hue {hue}");
                }
            }
        }
    }
}

#[test]
fn calibration_precedes_the_colour_tools() {
    let src = RgbFrame::filled(24, 16, [0.05, 0.08, 0.3]);
    let info = SourceInfo::default();
    let mut s = DevelopSettings::default();
    s.calibration.blue_sat = 60.;
    s.mixer.blue.sat = -60.;
    let plan = crate::plan(&src, &info, &s, &RenderRequest::fit(24, 16));
    let out = primary::process(&src, &src, &info, &plan, primary::DEFAULT_HS_METHOD);
    let m = colorops::calibration_matrix(&s.calibration);
    let mut expected = src.map(|c| colorops::calibrate(c, m.as_ref(), 0.));
    colorequal::apply(&mut expected, &s, plan.px_per_long);
    for (a, b) in out.data.iter().zip(&expected.data) {
        for k in 0..3 {
            assert!((a[k] - b[k]).abs() <= 1e-3 * b[k].abs().max(1e-3), "{a:?} != calibrated then HSL {b:?}");
        }
    }
    // The finish stage must not calibrate a second time.
    assert!(primary::remaining(&s).calibration == Default::default());
}

#[test]
fn calibration_shapes_black_and_white_and_its_shadows_tint_stays_neutral() {
    let info = SourceInfo { raw: true, ..Default::default() };
    let req = RenderRequest::fit(24, 16);
    let mut s = DevelopSettings { treatment: lightcraft_develop::Treatment::Bw, ..Default::default() };
    let blue = RgbFrame::filled(24, 16, [0.04, 0.06, 0.25]);
    let grey = |s: &DevelopSettings, src: &RgbFrame| render(src, &info, s, &req).image.get(12, 8);
    let plain = grey(&s, &blue);
    s.calibration.blue_sat = 60.;
    let calibrated = grey(&s, &blue);
    assert!(plain[1].abs_diff(calibrated[1]) >= 2, "Blue Primary Saturation does not shape the B&W mix: {plain:?} {calibrated:?}");
    s.calibration = Default::default();
    s.calibration.shadows_tint = 100.;
    let dark = grey(&s, &RgbFrame::filled(24, 16, [0.01; 3]));
    assert!(dark[0].abs_diff(dark[1]) <= 1 && dark[2].abs_diff(dark[1]) <= 1, "B&W shadows picked up a tint: {dark:?}");
}

#[test]
fn calibration_edits_reach_cached_renders() {
    let src = Arc::new(RgbFrame::from_fn(40, 30, |x, y| [0.05 + 0.01 * x as f32, 0.1 + 0.005 * y as f32, 0.2]));
    let info = SourceInfo::default();
    let req = RenderRequest::fit(40, 30);
    let cache = StageCache::default();
    let mut s = DevelopSettings::default();
    s.color.vibrance = 30.;
    let before = render_cached(&src, &info, &s, &req, &cache).image;
    s.calibration.red_hue = 60.;
    let after = render_cached(&src, &info, &s, &req, &cache).image;
    assert_ne!(before, after);
    assert_eq!(after, render(&src, &info, &s, &req).image);
}

#[test]
fn highlights_and_shadows_ignore_unrelated_dark_corners() {
    let (w, h) = (512, 256);
    let scene = |patch: Option<(usize, f32)>| {
        RgbFrame::from_fn(w, h, |x, y| match patch {
            Some((size, v)) if x < size && y < size => [v; 3],
            _ => [if x < w / 2 { 0.015 } else { 0.9 }; 3],
        })
    };
    let mut s = DevelopSettings::default();
    s.light.highlights = -100.;
    s.light.shadows = 100.;
    let info = SourceInfo::default();
    let develop = |src: &RgbFrame| {
        let plan = crate::plan(src, &info, &s, &RenderRequest::fit(w, h));
        primary::process(src, src, &info, &plan, primary::DEFAULT_HS_METHOD)
    };
    let clean = develop(&scene(None));
    // One black or blown pixel must not move the tone grid. A dark area only shifts the
    // pyramid's global level in proportion to its share of the frame (0.05% here).
    for (patch, bound) in [((1, 0.), 0.01), ((1, 1000.), 0.01), ((8, 2e-5), 0.04)] {
        let out = develop(&scene(Some(patch)));
        let worst = (0..w * h).filter(|i| i % w >= 32 || i / w >= 32).map(|i| (out.data[i][1] / clean.data[i][1]).log2().abs()).fold(0f32, f32::max);
        assert!(worst < bound, "a {patch:?} corner moved Highlights/Shadows elsewhere by {worst} EV");
    }
}

fn mask_of(shape: MaskShape, adjust: lightcraft_develop::LocalAdjustments) -> Mask {
    Mask { id: 1, components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: false, shape }], adjust, ..Default::default() }
}

#[test]
fn range_masks_select_before_the_primary_stage_in_every_stage() {
    // A dark interior beside a bright window; global Shadows lifts the interior out of the range.
    let (w, h) = (96, 48);
    let src = RgbFrame::from_fn(w, h, |x, _| [if x < w / 2 { 0.02 } else { 0.9 }; 3]);
    let info = SourceInfo::default();
    let req = RenderRequest::fit(w, h);
    let mut s = DevelopSettings::default();
    s.light.shadows = 80.;
    let shape = MaskShape::LuminanceRange { lo: 0.3, hi: 0.45, lo_feather: 0.05, hi_feather: 0.05 };
    s.masks.push(mask_of(shape, lightcraft_develop::LocalAdjustments { shadows: 50., exposure: 0.5, ..Default::default() }));
    let plan = crate::plan(&src, &info, &s, &req);
    let mut lin = plan.frame.sample(&src, plan.w, plan.h);
    lin_cpu(&mut lin, &info, &plan);
    let log = lin.map(local::log_lum);
    let expected = masks::evaluate_with_tone(&s.masks, &plan.frame, plan.w, plan.h, &lin, &log, 0., &masks::selection_tone(&s, &info));
    let overlay = Overlay::Mask { id: 1, view: crate::visualize::MaskView::WhiteOnBlack, color: [255; 3], opacity: 100 };
    let shown = render(&src, &info, &s, &RenderRequest { overlay, ..req }).image;
    for y in 0..h {
        for x in 0..w {
            let a = (expected[0].alpha.at(y * w + x) * 255.).round() as u8;
            assert!(
                shown.get(x, y)[1].abs_diff(a) <= 1,
                "({x}, {y}): finish-stage alpha {} differs from the primary stage's {a}",
                shown.get(x, y)[1]
            );
        }
    }
    assert_eq!(shown.get(10, 24)[1], 255, "the interior is selected");
}

#[test]
fn colour_range_masks_keep_their_selection_after_global_colour_changes() {
    // A blue sky above a grey wall; B&W and Saturation change colours in the primary stage.
    let src = RgbFrame::from_fn(64, 48, |_, y| if y < 24 { [0.05, 0.09, 0.3] } else { [0.12; 3] });
    let info = SourceInfo { raw: true, ..Default::default() };
    let req = RenderRequest::fit(64, 48);
    let mut s = DevelopSettings::default();
    let sample = color_range_sample(&src, &info, &s, &req, Point::new(0.5, 0.2)).unwrap();
    s.treatment = Treatment::Bw;
    s.color.saturation = 40.;
    let base = render(&src, &info, &s, &req).image;
    s.masks.push(mask_of(
        MaskShape::ColorRange { samples: vec![sample], refine: 50. },
        lightcraft_develop::LocalAdjustments { exposure: 1.0, ..Default::default() },
    ));
    let masked = render(&src, &info, &s, &req).image;
    assert!(masked.get(32, 10)[1] >= base.get(32, 10)[1] + 20, "the picked sky lost its selection: {:?} {:?}", masked.get(32, 10), base.get(32, 10));
    assert!(masked.get(32, 40)[1].abs_diff(base.get(32, 40)[1]) <= 1, "the wall was selected");
}

#[test]
fn hidden_masks_overlay_the_selection_they_make_when_shown() {
    // A blue sky over a grey wall; Shadows runs the primary stage and Contrast, Whites and Blacks
    // shape the tone colour selection compares under.
    let src = RgbFrame::from_fn(64, 48, |x, y| if y < 24 { [0.05, 0.09, 0.3].map(|v| v * (0.3 + x as f32 / 32.)) } else { [0.12; 3] });
    let info = SourceInfo { raw: true, ..Default::default() };
    let req = RenderRequest::fit(64, 48);
    let mut s = DevelopSettings::default();
    s.light.shadows = 60.;
    s.light.contrast = 40.;
    s.light.whites = 60.;
    s.light.blacks = -40.;
    let sample = color_range_sample(&src, &info, &s, &req, Point::new(0.5, 0.2)).unwrap();
    s.masks.push(mask_of(
        MaskShape::ColorRange { samples: vec![sample], refine: 30. },
        lightcraft_develop::LocalAdjustments { exposure: 0.5, ..Default::default() },
    ));
    let overlay = Overlay::Mask { id: 1, view: crate::visualize::MaskView::WhiteOnBlack, color: [255; 3], opacity: 100 };
    let req = RenderRequest { overlay, ..req };
    for stacked in [false, true] {
        if stacked {
            let light = lightcraft_develop::Light { exposure: 2., ..Default::default() };
            s.masks.push(Mask {
                id: 2,
                components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: false, shape: MaskShape::Full }],
                tools: lightcraft_develop::LayerTools { light: Some(light), ..Default::default() },
                ..Default::default()
            });
        }
        s.masks[0].visible = true;
        let shown = render(&src, &info, &s, &req).image;
        assert!(shown.data.iter().any(|p| p[1] > 200) && shown.data.iter().any(|p| p[1] < 50), "the sample must select part of the sky");
        s.masks[0].visible = false;
        let hidden = render(&src, &info, &s, &req).image;
        let worst = shown.data.iter().zip(&hidden.data).map(|(a, b)| a[1].abs_diff(b[1])).max().unwrap();
        assert!(worst <= 1, "stacked {stacked}: the hidden mask's overlay differs from the shown one by {worst}");
    }
}
