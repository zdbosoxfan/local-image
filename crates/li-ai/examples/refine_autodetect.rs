//! Run the same checker and image-refine path the app uses, serially on installed files.
use anyhow::{Context, Result, bail};
use li_ai::catalog::{self, ModelId};
use li_ai::comfy::{ComfyClient, JobControl, normalized_name};
use li_ai::family::Role;
use li_ai::refine::{Checker, Verdict, queue_idle};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut host = "127.0.0.1:8188".to_owned();
    let mut root = li_ai::AiSettings::load().model_dir();
    let mut filter = String::new();
    let mut app_refine = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => host = args.next().context("--host needs host:port")?,
            "--models" => root = PathBuf::from(args.next().context("--models needs a model root")?),
            "--model" => filter = args.next().context("--model needs a substring")?,
            "--app-refine" => app_refine = true,
            _ => bail!("Unknown option {arg}"),
        }
    }
    let client = ComfyClient::new(host);
    let info = client.object_info()?;
    let (installed, _) = li_ai::inventory::scan(&info, Some(&root), li_ai::family::registry());
    catalog::set_installed(installed.clone());
    std::fs::create_dir_all("target/refine-autodetect")?;
    std::fs::write("target/refine-autodetect/system.json", serde_json::to_vec_pretty(&client.system_stats()?)?)?;
    let checker = if app_refine { li_ai::refine::checker().clone() } else { Arc::new(Checker::new("target/refine-autodetect/ai-refine.json")) };
    println!("file\tverdict\trecipe\tdifference\tseconds");
    for installed in installed {
        let main = installed.files.get(&Role::Checkpoint).or_else(|| installed.files.get(&Role::Unet)).context("missing main file")?;
        if !main.contains(&filter) {
            continue;
        }
        let preset = catalog::presets()
            .iter()
            .find(|p| p.files.iter().any(|f| matches!(f.role, Role::Checkpoint | Role::Unet) && normalized_name(&f.name) == normalized_name(main)));
        let model = preset.map(|p| p.model).unwrap_or_else(|| ModelId::intern(&installed.key));
        let variant = preset.map(|p| p.variant.as_str()).unwrap_or("installed");
        let start = Instant::now();
        while !queue_idle(&client)? {
            if start.elapsed() > Duration::from_secs(120) {
                bail!("ComfyUI remains busy; resume later");
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let t = Instant::now();
        let verdict = checker.check_in(&client, model, variant, &root, !app_refine, &JobControl::new())?;
        match verdict {
            Some(Verdict::Verified { recipe, difference }) => {
                println!("{main}\tVerified\t{}\t{difference:.3}\t{:.3}", recipe.label(), t.elapsed().as_secs_f64());
                if app_refine {
                    let source = image::DynamicImage::ImageRgb8(li_ai::refine::test_image()).to_rgba8();
                    let defaults = recipe.defaults();
                    let mut request = li_ai::GenerateRequest::new(model, li_ai::refine::PROMPT);
                    request.variant = variant.into();
                    request.mode = li_ai::GenerateMode::Refine;
                    request.source = Some(source);
                    request.steps = defaults.steps;
                    request.guidance = defaults.cfg;
                    request.denoise = defaults.denoise;
                    let ai = li_ai::Ai::new(client.host());
                    for seed in [42, 43] {
                        if !queue_idle(&client)? {
                            bail!("ComfyUI became busy before app refinement");
                        }
                        request.seed = seed;
                        let started = Instant::now();
                        let output = ai.generate(&request, &JobControl::new())?;
                        let seconds = started.elapsed().as_secs_f64();
                        let difference =
                            li_ai::refine::mean_abs_difference(&li_ai::refine::test_image(), &image::DynamicImage::ImageRgba8(output.clone()).to_rgb8());
                        output.save(format!("target/refine-autodetect/app-{}-{seed}.png", model.key().replace([':', '/', '\\'], "_")))?;
                        println!("{main}\tApp refine\tseed={seed}\t{difference:.3}\t{seconds:.3}");
                    }
                }
            }
            Some(Verdict::Failed { reason }) => println!("{main}\tFailed\t{reason}\t-\t{:.3}", t.elapsed().as_secs_f64()),
            None => bail!("Queue became busy; resume later"),
        }
    }
    // List excluded non-image services and adapters so every installed main file is accounted for.
    for (class, input) in [("CheckpointLoaderSimple", "ckpt_name"), ("UNETLoader", "unet_name"), ("UnetLoaderGGUF", "unet_name")] {
        for name in info.choices(class, input) {
            if name.contains(&filter)
                && !li_ai::inventory::scan(&info, Some(&root), li_ai::family::registry()).0.iter().any(|m| m.files.values().any(|f| *f == name))
            {
                println!("{name}\tUnsupported\tNon-image service or adapter weights\t-\t0");
            }
        }
    }
    Ok(())
}
