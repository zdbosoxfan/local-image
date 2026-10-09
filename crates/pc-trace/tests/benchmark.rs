mod common;
use common::*;
use pc_trace::{Fitter, Image, Params, Preset, trace};
use std::{path::PathBuf, time::Instant};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn compare_references(cases: &[Case], scores: &[Score], preset: &str, strict: bool) -> TestResult {
    let mut competitors = Vec::new();
    for tool in ["vtracer-alpha4", "vtracer-065", "potrace-116"] {
        if tool == "potrace-116" && preset != "black_white" {
            continue;
        }
        let dir = root().join("tests/references").join(tool).join(preset);
        if cases.iter().any(|c| !dir.join(format!("{}.svg", c.id)).is_file()) {
            eprintln!("skipped: {tool}/{preset} competitor gates — committed reference SVGs missing");
            continue;
        }
        let mut result = Vec::new();
        for c in cases {
            result.push(reference_score(c, &dir.join(format!("{}.svg", c.id)))?);
        }
        competitors.push(result);
    }
    if competitors.is_empty() {
        eprintln!("skipped: {preset} B.4 fidelity/win/node gates — no complete competitor reference set");
    } else {
        gates(scores, &competitors, strict)?;
    }
    Ok(())
}
#[test]
fn fast_six_image_benchmark() -> TestResult {
    let mut cases = Vec::new();
    for (i, variant) in [Variant::Clean, Variant::Jpeg, Variant::Blur, Variant::Noise, Variant::Resample, Variant::Scan].into_iter().enumerate() {
        cases.push(case(i, 256, variant)?);
    }
    // The specified absolute quality gates require competitors. Without those
    // goldens, use an independent empty-canvas control for structural smoke QA.
    let empty = image::RgbImage::from_pixel(256, 256, image::Rgb([255; 3]));
    let controls: Vec<f64> = cases.iter().map(|c| photocraft_testkit::vector::ssim_ms(&c.clean, &empty)).collect::<Result<_, _>>()?;
    for (preset, name) in [
        (Preset::Logo, "logo"),
        (Preset::BlackWhite, "black_white"),
        (Preset::FewColors, "few_colors"),
        (Preset::Silhouette, "silhouette"),
        (Preset::Photo, "photo"),
        (Preset::PixelArt, "pixel_art"),
    ] {
        let mut scores = Vec::new();
        let params = Params::for_preset(preset);
        for (c, &control) in cases.iter().zip(&controls) {
            let start = Instant::now();
            let out = trace(Image { width: 256, height: 256, rgba: c.input.as_raw() }, &params)?;
            let score = score(c, &out, start.elapsed().as_secs_f64())?;
            eprintln!("{name}/{} fidelity {:.5} MS-SSIM {:.5} nodes {} IoU {:?}", c.id, score.fidelity, score.ms_ssim, score.nodes, score.iou);
            assert!(score.fidelity.is_finite() && score.ms_ssim > control, "trace does not beat empty-canvas MS-SSIM {control}: {score:?}");
            assert!(out.layers.len() <= params.max_colors);
            for l in &out.layers {
                assert!(photocraft_testkit::vector::is_finite(&l.path));
                assert!(photocraft_testkit::vector::self_intersections(&l.path).is_empty(), "{} crosses itself", c.id);
            }
            scores.push(score);
        }
        compare_references(&cases, &scores, name, matches!(preset, Preset::Logo | Preset::BlackWhite))?;
    }
    Ok(())
}
#[test]
fn benchmark_gates_reject_regressions() -> TestResult {
    let c = case(0, 32, Variant::Clean)?;
    let baseline = score(&c, &c.truth, 0.)?;
    let mut bad = baseline.clone();
    bad.fidelity -= 0.006;
    assert!(gates(&[bad], &[vec![baseline.clone()]], true).is_err());
    let mut bad = baseline.clone();
    bad.nodes = baseline.nodes * 2;
    assert!(gates(&[bad], &[vec![baseline.clone()]], true).is_err());
    gates(std::slice::from_ref(&baseline), &[vec![baseline.clone()]], true)?;
    Ok(())
}
#[test]
fn reference_nodes_count_closed_anchors_once() -> TestResult {
    let c = case(0, 32, Variant::Clean)?;
    let dir = root().join("../../target/trace-bench");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("node-metric-fixture.svg");
    for (data, count) in [("M1 1L31 1L31 31L1 31L1 1Z", 4), ("M1 1L31 1L31 31L1 31Z", 4), ("M1 1C31 1 31 31 1 1Z", 1)] {
        std::fs::write(&file, format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><path d="{data}"/></svg>"#))?;
        assert_eq!(reference_score(&c, &file)?.nodes, count);
    }
    std::fs::remove_file(file)?;
    Ok(())
}
#[test]
#[ignore = "full release corpus and fitter decision; coordinator commits competitor SVGs first"]
fn full_quality_benchmark() -> TestResult {
    if cfg!(debug_assertions) {
        return Err("run with --release".into());
    }
    let mut report = serde_json::Map::new();
    let dir = root().join("../../target/trace-bench");
    std::fs::create_dir_all(&dir)?;
    for (preset, name) in [
        (Preset::Logo, "logo"),
        (Preset::BlackWhite, "black_white"),
        (Preset::FewColors, "few_colors"),
        (Preset::Silhouette, "silhouette"),
        (Preset::Photo, "photo"),
        (Preset::PixelArt, "pixel_art"),
    ] {
        let mut cases = Vec::new();
        let mut scores = Vec::new();
        let mut alternate = Vec::new();
        for i in 0..30 {
            if matches!(preset, Preset::BlackWhite | Preset::Silhouette) && i >= 6 {
                continue;
            }
            for size in [256, 512, 1024] {
                for variant in Variant::ALL {
                    let c = case(i, size, variant)?;
                    let params = Params::for_preset(preset);
                    let start = Instant::now();
                    let out = trace(Image { width: size, height: size, rgba: c.input.as_raw() }, &params)?;
                    scores.push(score(&c, &out, start.elapsed().as_secs_f64())?);
                    if preset != Preset::PixelArt {
                        let mut params = params;
                        params.fitter = if params.fitter == Fitter::Potrace { Fitter::Spline } else { Fitter::Potrace };
                        let start = Instant::now();
                        let out = trace(Image { width: size, height: size, rgba: c.input.as_raw() }, &params)?;
                        alternate.push(score(&c, &out, start.elapsed().as_secs_f64())?);
                    }
                    cases.push(c);
                }
            }
        }
        for c in external_cases_for(preset)? {
            let params = Params::for_preset(preset);
            let start = Instant::now();
            let out = trace(Image { width: c.input.width(), height: c.input.height(), rgba: c.input.as_raw() }, &params)?;
            scores.push(score(&c, &out, start.elapsed().as_secs_f64())?);
            if preset != Preset::PixelArt {
                let mut params = params;
                params.fitter = if params.fitter == Fitter::Potrace { Fitter::Spline } else { Fitter::Potrace };
                let start = Instant::now();
                let out = trace(Image { width: c.input.width(), height: c.input.height(), rgba: c.input.as_raw() }, &params)?;
                alternate.push(score(&c, &out, start.elapsed().as_secs_f64())?);
            }
            cases.push(c);
        }
        let summarize = |scores: &[Score]| {
            serde_json::json!({
                "median_fidelity": median(&scores.iter().map(|s| s.fidelity).collect::<Vec<_>>()),
                "median_nodes": median(&scores.iter().map(|s| s.nodes as f64).collect::<Vec<_>>()),
                "median_seconds": median(&scores.iter().map(|s| s.seconds).collect::<Vec<_>>()),
            })
        };
        report.insert(
            name.into(),
            serde_json::json!({
                "default_fitter": Params::for_preset(preset).fitter,
                "default_summary": summarize(&scores), "alternate_summary": summarize(&alternate),
                "default":scores,"alternate":alternate
            }),
        );
        // Keep measured evidence even when the competitor gate reports a regression.
        std::fs::write(dir.join("scores.json"), serde_json::to_vec_pretty(&report)?)?;
        compare_references(&cases, &scores, name, matches!(preset, Preset::Logo | Preset::BlackWhite))?;
    }
    Ok(())
}
#[test]
#[ignore = "release timing gate runs on coordinator machine"]
fn release_performance_gates() -> TestResult {
    if cfg!(debug_assertions) {
        return Err("run with --release".into());
    }
    for (preset, size, budget) in [(Preset::Logo, 1024, 1.5), (Preset::Photo, 2048, 8.)] {
        let c = case(18, size, Variant::Jpeg)?;
        let start = Instant::now();
        let out = trace(Image { width: size, height: size, rgba: c.input.as_raw() }, &Params::for_preset(preset))?;
        std::hint::black_box(out);
        assert!(start.elapsed().as_secs_f64() < budget, "{preset:?} exceeds {budget}s");
    }
    Ok(())
}
#[test]
fn external_corpus() -> TestResult {
    let cases = external_cases()?;
    if cases.is_empty() {
        eprintln!("skipped: external corpus — coordinator CC0/PD SVGs and licensed scans have not been supplied");
        return Ok(());
    }
    for c in cases {
        let start = Instant::now();
        let out = trace(Image { width: c.input.width(), height: c.input.height(), rgba: c.input.as_raw() }, &Params::default())?;
        let result = score(&c, &out, start.elapsed().as_secs_f64())?;
        assert!(result.fidelity.is_finite());
        for l in &out.layers {
            assert!(photocraft_testkit::vector::is_finite(&l.path));
            assert!(photocraft_testkit::vector::self_intersections(&l.path).is_empty());
        }
    }
    Ok(())
}
