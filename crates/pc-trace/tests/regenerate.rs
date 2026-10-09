//! Coordinator-only CLI harness. No external tracer runs in normal tests.
mod common;
use common::*;
use pc_trace::{Params, Preset};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn run(command: &mut Command) -> TestResult {
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!("{command:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(())
}
fn version(tool: &str) -> TestResult<String> {
    let result = Command::new(tool).arg("--version").output()?;
    if !result.status.success() {
        return Err(format!("{tool} --version failed").into());
    }
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}
fn pbm(path: &Path, c: &Case, p: &Params) -> TestResult {
    let mut out = format!("P1\n{} {}\n", c.input.width(), c.input.height());
    for pixel in c.input.pixels() {
        let intensity = (u32::from(pixel[0]) + u32::from(pixel[1]) + u32::from(pixel[2])) / 3;
        out.push_str(if pixel[3] >= 128 && intensity < u32::from(p.threshold) { "1 " } else { "0 " });
    }
    std::fs::write(path, out)?;
    Ok(())
}
#[test]
#[ignore = "coordinator only: requires vtracer 1.0.0-alpha.4, potrace 1.16 and Inkscape on PATH"]
fn regenerate_competitor_references() -> TestResult {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = root.join("../../target/refvec/trace");
    std::fs::create_dir_all(&scratch)?;
    let mut versions = serde_json::Map::new();
    for (tool, pin) in [("vtracer", "1.0.0-alpha.4"), ("potrace", "1.16"), ("inkscape", "")] {
        let v = version(tool)?;
        if !v.contains(pin) {
            return Err(format!("expected {tool} {pin}; found {v}").into());
        }
        versions.insert(tool.into(), v.into());
    }
    let old = std::env::var("VTRACER_065").unwrap_or_else(|_| "vtracer-0.6.5".into());
    let old_version = version(&old).ok();
    if old_version.as_ref().is_some_and(|v| !v.contains("0.6.5")) {
        return Err("VTRACER_065 is not 0.6.5".into());
    }
    if let Some(v) = old_version.as_ref() {
        versions.insert("vtracer-065".into(), v.clone().into());
    } else {
        eprintln!("skipped: vtracer 0.6.5 references; set VTRACER_065 to that binary");
    }
    let fast = std::env::var_os("TRACE_REF_FAST").is_some();
    let mut manifest = serde_json::Map::new();
    for (preset, name) in [
        (Preset::Logo, "logo"),
        (Preset::BlackWhite, "black_white"),
        (Preset::FewColors, "few_colors"),
        (Preset::Silhouette, "silhouette"),
        (Preset::Photo, "photo"),
        (Preset::PixelArt, "pixel_art"),
    ] {
        let p = Params::for_preset(preset);
        let mut scores = serde_json::Map::new();
        let mut list = Vec::new();
        let mut cases = Vec::new();
        for index in 0..if fast { 6 } else { 30 } {
            if matches!(preset, Preset::BlackWhite | Preset::Silhouette) && index >= 6 {
                continue;
            }
            for size in if fast { vec![256] } else { vec![256, 512, 1024] } {
                let variants = if fast { vec![Variant::ALL[index]] } else { Variant::ALL.to_vec() };
                for variant in variants {
                    cases.push(case(index, size, variant)?);
                }
            }
        }
        if !fast {
            cases.extend(external_cases_for(preset)?);
        }
        for c in cases {
            let input = scratch.join(format!("{}.png", c.id));
            if matches!(preset, Preset::BlackWhite | Preset::Silhouette) {
                let mut binary = c.input.clone();
                for px in binary.pixels_mut() {
                    let v = if px[3] >= 128 && (u32::from(px[0]) + u32::from(px[1]) + u32::from(px[2])) / 3 < u32::from(p.threshold) { 0 } else { 255 };
                    px.0 = [v, v, v, 255];
                }
                binary.save(&input)?;
            } else {
                c.input.save(&input)?;
            }
            list.push(c.id.clone());
            for (tool, label) in [("vtracer", "vtracer-alpha4"), (old.as_str(), "vtracer-065"), ("potrace", "potrace-116")] {
                if label == "vtracer-065" && old_version.is_none() {
                    continue;
                }
                if tool == "potrace" && preset != Preset::BlackWhite {
                    continue;
                }
                let dir = root.join("tests/references").join(label).join(name);
                std::fs::create_dir_all(&dir)?;
                let raw = scratch.join(format!("{label}-{}-raw.svg", c.id));
                let dest = dir.join(format!("{}.svg", c.id));
                let mut cmd = Command::new(tool);
                if tool == "potrace" {
                    let input = scratch.join(format!("{}.pbm", c.id));
                    pbm(&input, &c, &p)?;
                    cmd.args(["--svg", "--resolution", "96", "--turdsize"])
                        .arg(p.speckle.to_string())
                        .arg("--alphamax")
                        .arg(p.alphamax.to_string())
                        .arg("--opttolerance")
                        .arg(p.opttolerance.to_string())
                        .arg("--output")
                        .arg(&raw)
                        .arg(input);
                } else {
                    cmd.arg("--input")
                        .arg(&input)
                        .arg("--output")
                        .arg(&raw)
                        .args(["--mode", if preset == Preset::PixelArt { "pixel" } else { "spline" }])
                        .arg("--filter_speckle")
                        .arg((p.speckle as f64).sqrt().ceil().to_string());
                    if label == "vtracer-alpha4" {
                        // clap converts these modern long names to kebab case.
                        cmd = Command::new(tool);
                        cmd.arg("--input")
                            .arg(&input)
                            .arg("--output")
                            .arg(&raw)
                            .args(["--mode", if preset == Preset::PixelArt { "pixel" } else { "spline" }])
                            .arg("--filter-speckle")
                            .arg((p.speckle as f64).sqrt().ceil().to_string())
                            .arg("--color-precision")
                            .arg((8 - p.color_precision_loss).to_string())
                            .arg("--gradient-step")
                            .arg(p.layer_difference.to_string())
                            .arg("--simplify")
                            .arg(p.detail.to_string())
                            .arg("--max-colors")
                            .arg(p.max_colors.to_string())
                            .args([
                                "--clustering",
                                if preset == Preset::Photo {
                                    "watershed"
                                } else if matches!(preset, Preset::BlackWhite | Preset::Silhouette) {
                                    "bw"
                                } else {
                                    "color-cluster"
                                },
                            ]);
                        if matches!(preset, Preset::BlackWhite | Preset::Silhouette) {
                            cmd.arg("--threshold").arg(p.threshold.to_string());
                        }
                    } else {
                        cmd.args(["--colormode", if matches!(preset, Preset::BlackWhite | Preset::Silhouette) { "binary" } else { "color" }]);
                    }
                }
                run(&mut cmd)?;
                run(Command::new("inkscape")
                    .arg("--batch-process")
                    .arg(&raw)
                    .arg("--export-type=svg")
                    .arg("--export-plain-svg")
                    .arg(format!("--export-filename={}", dest.display())))?;
                let score = reference_score(&c, &dest)?;
                scores.entry(label).or_insert_with(|| serde_json::json!([])).as_array_mut().ok_or("scores array")?.push(serde_json::to_value(score)?);
            }
        }
        manifest.insert(name.into(), serde_json::json!({"cases":list,"scores":scores,"parameters":p}));
    }
    std::fs::write(
        root.join("tests/references/scores.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"schema":1,"versions":versions,"suites":manifest,"metric":"our MS-SSIM + vtracer-bench fidelity"}))?,
    )?;
    Ok(())
}
