//! Real ComfyUI preservation check. No weight downloads, global interrupt or parallel jobs.
//! cargo run -p li-ai --example refine_check -- --host 127.0.0.1:8188
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use image::{Rgb, RgbImage, imageops::FilterType};
use li_ai::builders::{self, Params, Task};
use li_ai::catalog::{self, InstalledModel};
use li_ai::comfy::{ComfyClient, JobControl, ObjectInfo};
use li_ai::family::{Family, Loader, Role};
use li_ai::workflows::Files;

const WIDTH: u32 = 512;
const HEIGHT: u32 = 288;
const PROMPT: &str = "A simple landscape painting: blue sky, a yellow sun in the upper right, green rolling hills, a red house with a dark roof on the left and a blue pond on the right. Preserve the image composition, shapes and colors exactly; refine only subtle surface detail.";

/// Large recognizable regions make spatial/color loss visible at the requested 64x36 scale.
fn test_image() -> RgbImage {
    RgbImage::from_fn(WIDTH, HEIGHT, |x, y| {
        let u = x as f64 / WIDTH as f64;
        let v = y as f64 / HEIGHT as f64;
        let hill = 0.55 + 0.06 * (u * 9.0).sin();
        let color = if (u - 0.78).powi(2) + (v - 0.23).powi(2) < 0.075_f64.powi(2) {
            [244, 207, 80]
        } else if v < hill {
            [105 + (v * 25.0) as u8, 166 + (v * 22.0) as u8, 205 + (v * 25.0) as u8]
        } else if (u - 0.76).powi(2) / 0.15_f64.powi(2) + (v - 0.79).powi(2) / 0.09_f64.powi(2) < 1.0 {
            [73, 135, 177]
        } else if (0.19..0.38).contains(&u) && (0.57..0.77).contains(&v) {
            if (0.26..0.30).contains(&u) && v > 0.65 { [79, 57, 47] } else { [180, 82, 62] }
        } else if (0.17..0.40).contains(&u) && v > 0.44 + (u - 0.285).abs() * 1.05 && v < 0.58 {
            [76, 65, 62]
        } else {
            [86 + (v * 18.0) as u8, 136 + (v * 14.0) as u8, 65 + (v * 13.0) as u8]
        };
        Rgb(color)
    })
}

fn mean_abs_difference(a: &RgbImage, b: &RgbImage) -> f64 {
    let a = image::imageops::resize(a, 64, 36, FilterType::Triangle);
    let b = image::imageops::resize(b, 64, 36, FilterType::Triangle);
    let total: u64 = a.as_raw().iter().zip(b.as_raw()).map(|(a, b)| u64::from(a.abs_diff(*b))).sum();
    total as f64 / (64 * 36 * 3) as f64
}

fn queue_idle(client: &ComfyClient) -> Result<bool> {
    let q = client.queue_state()?;
    for field in ["queue_running", "queue_pending"] {
        let entries = q[field].as_array().with_context(|| format!("Invalid /queue: missing {field}"))?;
        if !entries.is_empty() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn wait_idle(client: &ComfyClient) -> Result<()> {
    let start = Instant::now();
    while !queue_idle(client)? {
        if start.elapsed() > Duration::from_secs(120) {
            bail!("ComfyUI queue stayed busy for 120s; retry when the owner is finished");
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    Ok(())
}

/// Preserve curated per-model overrides and matching encoder variants, even for tool weights
/// (Klein base). Run the main model alone; a task-specific removal LoRA is not a refine input.
fn recipe(installed: &InstalledModel, info: &ObjectInfo) -> Result<(Family, Files, li_ai::ModelId)> {
    let main = installed.files.get(&Role::Unet).or_else(|| installed.files.get(&Role::Checkpoint)).context("No main weights")?;
    let matching = catalog::presets().iter().filter(|p| !p.installed).find(|p| {
        p.files.iter().any(|f| matches!(f.role, Role::Unet | Role::Checkpoint) && li_ai::comfy::normalized_name(&f.name) == li_ai::comfy::normalized_name(main))
    });
    let model = matching.map_or_else(|| li_ai::ModelId::intern(&installed.key), |p| p.model);
    let model_info = model.try_info().context("Installed model is missing from the catalog")?;
    let mut family = if model_info.tool {
        // The main base weights are being checked without the removal LoRA: use their
        // general edit recipe, not the tool's task-specific 28-step/CFG-4 overrides.
        li_ai::family::registry().family(&installed.family).context("Missing installed family")?.resolved_for_file(main)
    } else {
        model_info.resolved.clone()
    };
    let mut files = installed.files.clone();
    if let Some(preset) = matching {
        for spec in &preset.files {
            if matches!(spec.role, Role::Lora | Role::Unet | Role::Checkpoint) {
                continue;
            }
            let (class, input) = spec.role.loader();
            if let Some(file) = info.find_file(class, input, &spec.name) {
                files.insert(spec.role, file);
            }
        }
    }
    for (role, file) in li_ai::inventory::resolve_components(&family, info) {
        files.entry(role).or_insert(file);
    }
    if installed.files.contains_key(&Role::Checkpoint) {
        family.pipeline.loader = Loader::Checkpoint;
    }
    let needed: &[Role] = match family.pipeline.clip.node.as_str() {
        "CLIPLoader" => &[Role::Clip],
        "DualCLIPLoader" => &[Role::Clip, Role::Clip2],
        "TripleCLIPLoader" => &[Role::Clip, Role::Clip2, Role::Clip3],
        "QuadrupleCLIPLoader" => &[Role::Clip, Role::Clip2, Role::Clip3, Role::Clip4],
        _ => &[],
    };
    for role in needed.iter().copied().chain((family.pipeline.vae == "file").then_some(Role::Vae)) {
        if !files.contains_key(&role) {
            bail!("Missing {role:?} for {}", installed.label);
        }
    }
    Ok((family, files, model))
}

fn main() -> Result<()> {
    let mut host = "127.0.0.1:8188".to_owned();
    let mut filter = String::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => host = args.next().context("--host requires host:port")?,
            "--model" => filter = args.next().context("--model requires a filename substring")?,
            "--help" | "-h" => {
                println!(
                    "refine_check [--host 127.0.0.1:8188] [--model filename-substring]\nRuns every recognized installed image model at its default denoise and at 0.4, sequentially.\nPASS: RGB mean absolute difference < 35 (0-255 at 64x36). Artifacts: target/refine-check/."
                );
                return Ok(());
            }
            _ => bail!("Unknown option {arg}"),
        }
    }
    let client = ComfyClient::new(host);
    let info = client.object_info()?;
    let (installed, _) = li_ai::inventory::scan(&info, None, li_ai::family::registry());
    catalog::set_installed(installed.clone());
    // Make omissions explicit: unknown models and non-image services cannot use this builder.
    for (class, input) in [("UNETLoader", "unet_name"), ("CheckpointLoaderSimple", "ckpt_name"), ("UnetLoaderGGUF", "unet_name")] {
        for name in info.choices(class, input) {
            if !installed.iter().any(|m| m.files.values().any(|v| v == &name)) {
                println!("SKIP\t{name}\tUnrecognized, non-image, or adapter weights");
            }
        }
    }
    let output_dir = PathBuf::from("target/refine-check");
    std::fs::create_dir_all(&output_dir)?;
    let source = test_image();
    source.save(output_dir.join("source.png"))?;
    let png = li_ai::imaging::encode_png(&source)?;
    println!("MODEL\tFAMILY\tDENOISE\tMAD_0_255\tSTATUS\tSECONDS");
    let (mut runs, mut failures) = (0, 0);
    for (index, installed) in installed.iter().enumerate().filter(|(_, m)| m.key.contains(&filter)) {
        let (family, files, model) = match recipe(installed, &info) {
            Ok(r) => r,
            Err(e) => {
                println!("ERROR\t{}\t{e:#}", installed.key);
                failures += 1;
                continue;
            }
        };
        for (label, denoise) in [("default", catalog::refine_default_denoise(model)), ("0.4", 0.4)] {
            wait_idle(&client)?;
            let mut params = Params::create(PROMPT, WIDTH, HEIGHT, 12345, family.sampling.steps.default as u32, family.sampling.cfg.default);
            params.task = Task::Refine { denoise };
            let built = builders::build(&family, &files, &params);
            let missing: Vec<_> =
                built.graph.as_object().context("Invalid graph")?.values().filter_map(|n| n["class_type"].as_str()).filter(|c| !info.has_node(c)).collect();
            if !missing.is_empty() {
                println!("ERROR\t{}\tmissing nodes {missing:?}", installed.key);
                failures += 1;
                break;
            }
            let stem = format!("{index:02}-{label}");
            std::fs::write(output_dir.join(format!("{stem}.json")), serde_json::to_vec_pretty(&built.graph)?)?;
            println!("RUN\t{}\t{label}={denoise}\tsteps={} cfg={}", installed.key, params.steps, params.cfg);
            std::io::stdout().flush()?;
            let started = Instant::now();
            let images = built.image_nodes.into_iter().map(|id| (id, png.clone())).collect::<Vec<_>>();
            let result = client.run(built.graph, &images, &JobControl::new()).and_then(|bytes| Ok(image::load_from_memory(&bytes)?.to_rgb8()));
            runs += 1;
            match result {
                Ok(out) => {
                    out.save(output_dir.join(format!("{stem}.png")))?;
                    let mad = mean_abs_difference(&source, &out);
                    let status = if mad < 35.0 {
                        "PASS"
                    } else {
                        failures += 1;
                        "FAIL"
                    };
                    println!("{}\t{}\t{denoise:.2}\t{mad:.3}\t{status}\t{:.2}", installed.key, family.id, started.elapsed().as_secs_f64());
                }
                Err(e) => {
                    failures += 1;
                    println!("{}\t{}\t{denoise:.2}\t-\tERROR\t{:.2}\t{e:#}", installed.key, family.id, started.elapsed().as_secs_f64());
                }
            }
        }
    }
    println!("Completed {runs} runs; {failures} failures. Outputs: {}", output_dir.display());
    if runs == 0 || failures > 0 {
        bail!("Refine check failed ({runs} runs, {failures} failures)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_detects_color_and_spatial_changes() {
        let source = test_image();
        assert_eq!(mean_abs_difference(&source, &source), 0.0);
        let shifted = RgbImage::from_fn(WIDTH, HEIGHT, |x, y| *source.get_pixel((x + WIDTH / 2) % WIDTH, y));
        assert!(mean_abs_difference(&source, &shifted) > 5.0);
        let black = RgbImage::new(64, 36);
        let white = RgbImage::from_pixel(128, 72, Rgb([255; 3]));
        assert_eq!(mean_abs_difference(&black, &white), 255.0);
    }
}
