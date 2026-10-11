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
    let input = Plane::from_fn(37, 29, |x, y| {
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
    let input = Plane::from_fn(50, 37, |x, y| 0.3 + 0.2 * ((x as f32 * 0.3).sin() * (y as f32 * 0.2).cos()) + if x > 25 { 0.3 } else { 0. });
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
    let src = Rgb32f::from_fn(100, 60, |x, y| {
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
    let src = Rgb32f::filled(24, 16, [8.; 3]);
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
    let src = Rgb32f::filled(32, 24, [1.5, 0.8, 0.3]);
    let info = SourceInfo::default();
    let plan = crate::plan(&src, &info, &s, &RenderRequest::fit(32, 24));
    let out = primary::process(&src, &src, &info, &plan, primary::DEFAULT_HS_METHOD);
    assert!((out.data[0][0] / out.data[0][1] - 1.5 / 0.8).abs() < 1e-5);
}
#[test]
fn modes_structure_skin_layers_and_cache_changes() {
    let src = Arc::new(Rgb32f::from_fn(80, 60, |x, y| [0.3 + 0.02 * (x as f32 * 0.6).sin(), 0.15 + 0.007 * (y as f32 * 0.7).cos(), 0.08]));
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
fn extracted_c_colour_equalizer_periodic_rbf() {
    let positions = std::array::from_fn(|i| -std::f32::consts::PI + i as f32 * std::f32::consts::TAU / 8.);
    for mode in 0..2 {
        let nodes = std::array::from_fn(|i| 0.2 * (i as f32 * 1.1).cos() + if mode == 1 { 1. } else { 0. });
        let lut = colorequal::rbf(nodes, positions, std::f32::consts::PI, mode == 1);
        let max = rows("colorequal-rbf.csv").iter().filter(|r| r[0] as usize == mode).map(|r| (lut[r[1] as usize] - r[2]).abs()).fold(0., f32::max);
        assert!(max < 8e-6, "RBF {mode}: {max}");
    }
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
    let src = Rgb32f::from_fn(160, 80, |x, _| jch_rgb([0.5, 0.04 + 0.003 * (x as f32 * 0.03).sin() + 0.0007 * (x as f32 * 1.8).sin(), 0.8]));
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
    let chroma = |img: &Rgb32f, x| ucs::xyy_to_jch(ucs::xyz_to_xyy(ucs::mul(&ucs::mats().rgb_to_xyz, img.get(x, 40))), lw)[1];
    // Second differences isolate fine pores from the low-frequency chroma unevenness.
    let energy = |img: &Rgb32f| {
        (10..150)
            .map(|x| {
                let v = chroma(img, x - 1) - 2. * chroma(img, x) + chroma(img, x + 1);
                v * v
            })
            .sum::<f32>()
    };
    let ratio = energy(&out) / energy(&src);
    assert!((0.85..1.15).contains(&ratio), "pore chroma ratio {ratio}");
    let lip = Rgb32f::filled(20, 12, jch_rgb([0.5, 0.04, 0.2]));
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
    let controls: [fn(&mut DevelopSettings, f64); 13] = [
        |s, v| s.light.highlights = v,
        |s, v| s.light.shadows = v,
        |s, v| s.light.whites = v,
        |s, v| s.light.blacks = v,
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
    let src = Rgb32f::from_fn(19, 13, |x, y| [0.002 + (x as f32 / 18.).exp2(), 0.02 + y as f32 / 40., 0.1]);
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
