//! Serial, real-model evaluation of core-node variety candidates. No custom nodes or downloads.
//! Artifacts stay in target/zimage-variety; run with LOCAL_IMAGE_DATA_DIR in that scratch dir.
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use image::{RgbImage, imageops};
use li_ai::builders::{self, Params};
use li_ai::comfy::{ComfyClient, JobControl};
use li_ai::family::{Role, registry};
use li_ai::workflows::Files;
use serde_json::{Value, json};

const PROMPTS: &[&str] = &[
    "A red fox sitting beside a mossy fallen log in a green forest, soft morning sunlight, wildlife photograph",
    "A woman chef wearing a white chef hat and blue apron, holding a bowl of oranges in a rustic kitchen, candid photograph",
];
const SEEDS: &[u64] = &[42, 314159, 987654321, 20261010];

fn alternate(seed: u64) -> String {
    let mut state = seed;
    (0..96)
        .map(|_| {
            state = state.wrapping_add(0x9e3779b97f4a7c15);
            let mut n = state;
            n = (n ^ (n >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            n = (n ^ (n >> 27)).wrapping_mul(0x94d049bb133111eb);
            format!("{:08x}", (n ^ (n >> 31)) as u32)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn insert(g: &mut Value, id: &str, class: &str, inputs: Value) {
    g[id] = json!({"class_type": class, "inputs": inputs});
}

fn candidate(mut g: Value, method: &str, level: f32, seed: u64) -> Result<Value> {
    if level == 0.0 {
        return Ok(g);
    }
    let sampler = g.as_object().context("graph")?.iter().find(|(_, n)| n["class_type"] == "KSampler").context("sampler")?.0.clone();
    let pos = g[&sampler]["inputs"]["positive"].clone();
    if method == "blend-early" || method == "blend-all" {
        let clip = g[pos[0].as_str().context("positive id")?]["inputs"]["clip"].clone();
        insert(&mut g, "901", "CLIPTextEncode", json!({"clip": clip, "text": alternate(seed)}));
        insert(&mut g, "902", "ConditioningAverage", json!({"conditioning_to": pos, "conditioning_from": ["901", 0], "conditioning_to_strength": 1.0-level}));
        if method == "blend-early" {
            insert(&mut g, "903", "ConditioningSetTimestepRange", json!({"conditioning": ["902",0], "start": 0.0, "end": 0.2}));
            insert(&mut g, "904", "ConditioningSetTimestepRange", json!({"conditioning": pos, "start": 0.2, "end": 1.0}));
            insert(&mut g, "905", "ConditioningCombine", json!({"conditioning_1": ["903",0], "conditioning_2": ["904",0]}));
            g[&sampler]["inputs"]["positive"] = json!(["905", 0]);
        } else {
            g[&sampler]["inputs"]["positive"] = json!(["902", 0]);
        }
    } else {
        let model = g[&sampler]["inputs"]["model"].clone();
        let latent = g[&sampler]["inputs"]["latent_image"].clone();
        insert(&mut g, "901", "RandomNoise", json!({"noise_seed": seed ^ 0x5a5a5a5a5a5a5a5au64}));
        insert(&mut g, "902", "BasicScheduler", json!({"model": model, "scheduler": "simple", "steps": 8, "denoise": 1.0}));
        insert(&mut g, "903", "AddNoise", json!({"model": model, "noise": ["901",0], "sigmas": ["902",0], "latent_image": latent}));
        insert(&mut g, "904", "LatentMultiply", json!({"samples": ["903",0], "multiplier": level}));
        g[&sampler]["inputs"]["latent_image"] = json!(["904", 0]);
    }
    Ok(g)
}

fn difference(a: &RgbImage, b: &RgbImage) -> f64 {
    let small = |im: &RgbImage| imageops::resize(im, 64, 36, imageops::FilterType::Triangle);
    let (a, b) = (small(a), small(b));
    a.as_raw().iter().zip(b.as_raw()).map(|(a, b)| (*a as f64 - *b as f64).abs()).sum::<f64>() / a.as_raw().len() as f64
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let method = args.get(1).map(String::as_str).unwrap_or("blend-early");
    let client = ComfyClient::new("127.0.0.1:8188");
    if method == "app" {
        let ai = li_ai::Ai::new(client.host());
        let mut times = Vec::new();
        for (seed, variety) in [(4242424242, 0.0), (4343434343, 0.0), (4444444444, 100.0), (4545454545, 100.0)] {
            let queue = client.queue_state()?;
            ensure!(["queue_running", "queue_pending"].iter().all(|key| queue[*key].as_array().is_some_and(Vec::is_empty)), "ComfyUI is busy");
            let mut req = li_ai::GenerateRequest::new(li_ai::ModelId::ZImageTurbo, PROMPTS[0]);
            (req.width, req.height, req.seed) = (512, 288, seed);
            req.variety = variety;
            let start = Instant::now();
            let image = ai.generate(&req, &JobControl::new())?;
            let elapsed = start.elapsed().as_secs_f64();
            image.save(format!("target/zimage-variety/app-{seed}.png"))?;
            times.push(json!({"seed": seed,"variety":variety,"seconds": elapsed}));
            println!("APP\t{seed}\t{variety}\t{elapsed:.3}s");
        }
        std::fs::write("target/zimage-variety/app-times.json", serde_json::to_vec_pretty(&times)?)?;
        return Ok(());
    }
    ensure!(["blend-early", "blend-all", "latent", "shipped"].contains(&method), "unknown method");
    let levels: &[f32] = if method == "latent" {
        &[0.0, 0.5, 1.0, 2.0]
    } else if method == "shipped" {
        &[0.0, 40.0, 100.0]
    } else {
        &[0.0, 0.15, 0.35, 0.6]
    };
    let width = args.get(2).map(|s| s.parse::<u32>()).transpose()?.unwrap_or(512);
    let height = width * 9 / 16;
    let prompt_filter = args.get(3).map(|s| s.parse::<usize>()).transpose()?;
    let info = client.object_info()?;
    let stats = client.system_stats()?;
    let dir = Path::new("target/zimage-variety").join(if width == 512 { method.to_owned() } else { format!("{method}-{width}") });
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("system.json"), serde_json::to_vec_pretty(&stats)?)?;
    let family = registry().family("z-image").context("z-image")?;
    let mut files = Files::new();
    for (role, name) in [(Role::Unet, "z_image_turbo_bf16.safetensors"), (Role::Clip, "qwen_3_4b.safetensors"), (Role::Vae, "ae.safetensors")] {
        let (loader, input) = role.loader();
        files.insert(role, info.find_file(loader, input, name).with_context(|| format!("missing {name}"))?);
    }
    let mut measurements = Vec::new();
    for (pi, prompt) in PROMPTS.iter().enumerate() {
        if prompt_filter.is_some_and(|filter| pi != filter) {
            continue;
        }
        let mut montage = RgbImage::new(width * SEEDS.len() as u32, height * levels.len() as u32);
        for (li, level) in levels.iter().copied().enumerate() {
            let mut images = Vec::new();
            let mut seconds = Vec::new();
            for (si, seed) in SEEDS.iter().copied().enumerate() {
                let mut params = Params::create(prompt, width, height, seed, 8, 1.0);
                let graph = if method == "shipped" {
                    params.variety = level;
                    builders::build(family, &files, &params).graph
                } else {
                    candidate(builders::build(family, &files, &params).graph, method, level, seed)?
                };
                ensure!(graph.as_object().context("graph")?.values().all(|n| info.has_node(n["class_type"].as_str().unwrap_or(""))), "missing core node");
                let stem = format!("p{pi}-l{li}-s{si}");
                std::fs::write(dir.join(format!("{stem}.json")), serde_json::to_vec_pretty(&graph)?)?;
                let path = dir.join(format!("{stem}.png"));
                let image = if path.exists() {
                    image::open(&path)?.to_rgb8()
                } else {
                    let start = Instant::now();
                    let bytes = client.run_when_idle(graph, &[], &JobControl::new())?;
                    seconds.push(start.elapsed().as_secs_f64());
                    let im = image::load_from_memory(&bytes)?.to_rgb8();
                    im.save(&path)?;
                    println!("{method}\tp{pi}\t{level}\t{seed}\t{:.3}s", start.elapsed().as_secs_f64());
                    im
                };
                imageops::replace(&mut montage, &image, si as i64 * width as i64, li as i64 * height as i64);
                images.push(image);
            }
            let pairs: Vec<f64> =
                (0..images.len()).flat_map(|a| (a + 1..images.len()).map(move |b| (a, b))).map(|(a, b)| difference(&images[a], &images[b])).collect();
            let mean = pairs.iter().sum::<f64>() / pairs.len() as f64;
            println!("MEAN\t{method}\tp{pi}\t{level}\t{mean:.3}");
            measurements.push(json!({"prompt":prompt,"prompt_index":pi,"level":level,"pairwise_mad":pairs,"mean_mad":mean,"seconds":seconds}));
            std::fs::write(dir.join("measurements.json"), serde_json::to_vec_pretty(&measurements)?)?;
        }
        montage.save(dir.join(format!("p{pi}-montage.png")))?;
    }
    if method == "shipped" {
        // A different core sampler class forces fresh sampling instead of reusing the cached
        // KSampler output. Both nodes call common_ksampler with the same full-denoise settings.
        let pi = prompt_filter.unwrap_or(0);
        let mut params = Params::create(PROMPTS[pi], width, height, SEEDS[0], 8, 1.0);
        params.variety = 100.0;
        let mut graph = builders::build(family, &files, &params).graph;
        let sampler = graph.as_object_mut().context("graph")?.values_mut().find(|n| n["class_type"] == "KSampler").context("sampler")?;
        sampler["class_type"] = json!("KSamplerAdvanced");
        let inputs = sampler["inputs"].as_object_mut().context("inputs")?;
        let seed = inputs.remove("seed").context("seed")?;
        inputs.remove("denoise");
        inputs.insert("noise_seed".into(), seed);
        inputs.insert("add_noise".into(), json!("enable"));
        inputs.insert("start_at_step".into(), json!(0));
        inputs.insert("end_at_step".into(), json!(8));
        inputs.insert("return_with_leftover_noise".into(), json!("disable"));
        std::fs::write(dir.join("replay.json"), serde_json::to_vec_pretty(&graph)?)?;
        let bytes = client.run_when_idle(graph, &[], &JobControl::new())?;
        let replay = image::load_from_memory(&bytes)?.to_rgb8();
        replay.save(dir.join("replay.png"))?;
        let original = image::open(dir.join(format!("p{pi}-l2-s0.png")))?.to_rgb8();
        ensure!(original == replay, "fresh sampler replay changed pixels");
        std::fs::write(
            dir.join("replay-result.json"),
            serde_json::to_vec_pretty(&json!({"prompt_index":pi,"seed":SEEDS[0],"variety":100,"fresh_sampler":"KSamplerAdvanced","identical_pixels":true}))?,
        )?;
        println!("REPLAY\tidentical pixels\tseed={}\tvariety=100", SEEDS[0]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(seed: u64) -> Value {
        builders::build(registry().family("z-image").unwrap(), &Files::new(), &Params::create(PROMPTS[0], 512, 288, seed, 8, 1.0)).graph
    }

    #[test]
    fn variety_candidates_off_is_original_and_on_is_seed_deterministic() {
        let base = graph(42);
        for method in ["blend-early", "blend-all", "latent"] {
            assert_eq!(candidate(base.clone(), method, 0.0, 42).unwrap(), base);
            let on = candidate(base.clone(), method, 0.35, 42).unwrap();
            assert_ne!(on, base);
            assert_eq!(candidate(base.clone(), method, 0.35, 42).unwrap(), on);
            assert_ne!(candidate(base.clone(), method, 0.35, 43).unwrap(), on);
        }
    }

    #[test]
    fn variety_candidates_use_core_blend_and_restore_original_after_early_steps() {
        let g = candidate(graph(42), "blend-early", 0.35, 42).unwrap();
        assert_eq!(g["902"]["class_type"], "ConditioningAverage");
        assert!((g["902"]["inputs"]["conditioning_to_strength"].as_f64().unwrap() - 0.65).abs() < 1e-6);
        assert_eq!(g["903"]["inputs"]["end"], 0.2);
        assert_eq!(g["904"]["inputs"]["start"], 0.2);
        assert_eq!(g["904"]["inputs"]["conditioning"], g["902"]["inputs"]["conditioning_to"]);
        assert_eq!(g["905"]["class_type"], "ConditioningCombine");
    }
}
