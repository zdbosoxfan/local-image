//! ComfyUI API-format graphs for every model, ported node-for-node from Local Image 0.7 so results
//! match. Each builder takes the loader file names resolved by [`crate::catalog::availability`].
//! `LoadImage` nodes are created with an empty `image`; [`crate::comfy::ComfyClient::run`] uploads
//! the pixels and fills them in.

use crate::catalog::Role;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

pub type Files = BTreeMap<Role, String>;

/// A LoRA chosen for a generation: its ComfyUI `lora_name` and strength (−2..2).
#[derive(Clone, Debug, PartialEq)]
pub struct LoraUse {
    pub name: String,
    pub strength: f32,
}

#[derive(Default)]
struct Graph(Map<String, Value>);

impl Graph {
    fn node(&mut self, id: impl ToString, class: &str, inputs: Value) {
        self.0.insert(id.to_string(), json!({ "class_type": class, "inputs": inputs }));
    }
    fn set(&mut self, id: &str, key: &str, value: Value) {
        if let Some(n) = self.0.get_mut(id).and_then(|n| n.get_mut("inputs")) {
            n[key] = value;
        }
    }
    /// Chains up to three model-only LoRA loaders (ids 100..102) after `model`; returns the link.
    fn lora_chain(&mut self, mut model: Value, loras: &[LoraUse]) -> Value {
        for (i, l) in loras.iter().take(3).enumerate() {
            let id = (100 + i).to_string();
            self.node(&id, "LoraLoaderModelOnly", json!({ "model": model, "lora_name": l.name, "strength_model": l.strength }));
            model = json!([id, 0]);
        }
        model
    }
    fn into_value(self) -> Value {
        Value::Object(self.0)
    }
}

fn f(files: &Files, role: Role) -> String {
    files.get(&role).cloned().unwrap_or_default()
}

/// Node ids of the `LoadImage` nodes a graph expects, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Built {
    pub graph: Value,
    pub image_nodes: Vec<String>,
}

pub struct QwenParams<'a> {
    pub prompt: &'a str,
    pub negative: &'a str,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub steps: u32,
    pub cfg: f32,
    pub references: usize,
    pub use_cache: bool,
    pub loras: &'a [LoraUse],
}

/// Qwen Image 2.1: text-to-image (EmptyLatentImage) or editing from references (the encoder's
/// latent, so the canvas follows reference 1).
pub fn qwen(files: &Files, p: &QwenParams) -> Built {
    let mut g = Graph::default();
    g.node(1, "UNETLoader", json!({ "unet_name": f(files, Role::Unet), "weight_dtype": "default" }));
    g.node(2, "CLIPLoader", json!({ "clip_name": f(files, Role::Clip), "type": "qwen_image", "device": "default" }));
    g.node(3, "VAELoader", json!({ "vae_name": f(files, Role::Vae) }));
    let refs = p.references > 0;
    g.node(
        4,
        "TextEncodeQwenImage21",
        json!({ "clip": ["2", 0], "prompt": p.prompt, "negative_prompt": p.negative, "resolution": if refs { 0 } else { 1024 } }),
    );
    g.node(
        6,
        "KSampler",
        json!({ "model": ["1", 0], "seed": p.seed, "steps": p.steps, "cfg": p.cfg, "sampler_name": "euler", "scheduler": "simple",
                "denoise": 1.0, "positive": ["4", 0], "negative": ["4", 1], "latent_image": if refs { json!(["4", 2]) } else { json!(["5", 0]) } }),
    );
    g.node(7, "VAEDecode", json!({ "samples": ["6", 0], "vae": ["3", 0] }));
    g.node(8, "SaveImage", json!({ "images": ["7", 0], "filename_prefix": "LocalImage_Qwen21" }));
    let model = g.lora_chain(json!(["1", 0]), p.loras);
    g.set("6", "model", model.clone());
    if p.use_cache {
        g.node(9, "QwenImage21Cache", json!({ "model": model, "device": "auto", "dtype": "default" }));
        g.set("6", "model", json!(["9", 0]));
    }
    let mut image_nodes = Vec::new();
    if refs {
        g.set("4", "vae", json!(["3", 0]));
        for i in 1..=p.references {
            let (load, alpha) = ((20 + i * 2).to_string(), (21 + i * 2).to_string());
            g.node(&load, "LoadImage", json!({ "image": "" }));
            // LoadImage's MASK is inverted alpha; JoinImageWithAlpha expects that convention.
            g.node(&alpha, "JoinImageWithAlpha", json!({ "image": [load, 0], "alpha": [load, 1] }));
            g.set("4", &format!("images.image_{i}"), json!([alpha, 0]));
            image_nodes.push(load);
        }
    } else {
        g.node(5, "EmptyLatentImage", json!({ "width": p.width, "height": p.height, "batch_size": 1 }));
    }
    Built { graph: g.into_value(), image_nodes }
}

pub struct ZImageParams<'a> {
    pub prompt: &'a str,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub steps: u32,
    /// Variation strength when a starting image is given.
    pub denoise: f32,
    pub init_image: bool,
    pub loras: &'a [LoraUse],
}

pub fn z_image(files: &Files, p: &ZImageParams) -> Built {
    let mut g = Graph::default();
    g.node(1, "UNETLoader", json!({ "unet_name": f(files, Role::Unet), "weight_dtype": "default" }));
    g.node(2, "CLIPLoader", json!({ "clip_name": f(files, Role::Clip), "type": "lumina2", "device": "default" }));
    g.node(3, "VAELoader", json!({ "vae_name": f(files, Role::Vae) }));
    g.node(4, "CLIPTextEncode", json!({ "clip": ["2", 0], "text": p.prompt }));
    g.node(5, "ConditioningZeroOut", json!({ "conditioning": ["4", 0] }));
    g.node(6, "ModelSamplingAuraFlow", json!({ "model": ["1", 0], "shift": 3.0 }));
    g.node(
        7,
        "KSampler",
        json!({ "model": ["6", 0], "seed": p.seed, "steps": p.steps, "cfg": 1.0, "sampler_name": "res_multistep", "scheduler": "simple",
                "positive": ["4", 0], "negative": ["5", 0], "latent_image": ["10", 0], "denoise": if p.init_image { p.denoise } else { 1.0 } }),
    );
    g.node(8, "VAEDecode", json!({ "samples": ["7", 0], "vae": ["3", 0] }));
    g.node(9, "SaveImage", json!({ "images": ["8", 0], "filename_prefix": "LocalImage_ZImageTurbo" }));
    let model = g.lora_chain(json!(["1", 0]), p.loras);
    g.set("6", "model", model);
    let mut image_nodes = Vec::new();
    if p.init_image {
        g.node(11, "LoadImage", json!({ "image": "" }));
        g.node(10, "VAEEncode", json!({ "pixels": ["11", 0], "vae": ["3", 0] }));
        image_nodes.push("11".into());
    } else {
        g.node(10, "EmptySD3LatentImage", json!({ "width": p.width, "height": p.height, "batch_size": 1 }));
    }
    Built { graph: g.into_value(), image_nodes }
}

pub struct Flux2Params<'a> {
    pub prompt: &'a str,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub steps: u32,
    pub references: usize,
    pub loras: &'a [LoraUse],
}

/// FLUX.2 Klein (4B / 9B, distilled): CFG 1 with zeroed negative conditioning and a
/// ReferenceLatent per reference image on both chains.
pub fn flux2_klein(files: &Files, p: &Flux2Params) -> Built {
    let mut g = Graph::default();
    g.node(1, "UNETLoader", json!({ "unet_name": f(files, Role::Unet), "weight_dtype": "default" }));
    g.node(2, "CLIPLoader", json!({ "clip_name": f(files, Role::Clip), "type": "flux2", "device": "default" }));
    g.node(3, "VAELoader", json!({ "vae_name": f(files, Role::Vae) }));
    g.node(4, "CLIPTextEncode", json!({ "clip": ["2", 0], "text": p.prompt }));
    g.node(7, "EmptyFlux2LatentImage", json!({ "width": p.width, "height": p.height, "batch_size": 1 }));
    g.node(8, "Flux2Scheduler", json!({ "steps": p.steps, "width": p.width, "height": p.height }));
    g.node(9, "RandomNoise", json!({ "noise_seed": p.seed }));
    g.node(10, "KSamplerSelect", json!({ "sampler_name": "euler" }));
    g.node(11, "SamplerCustomAdvanced", json!({ "noise": ["9", 0], "guider": ["6", 0], "sampler": ["10", 0], "sigmas": ["8", 0], "latent_image": ["7", 0] }));
    g.node(12, "VAEDecode", json!({ "samples": ["11", 0], "vae": ["3", 0] }));
    g.node(13, "SaveImage", json!({ "images": ["12", 0], "filename_prefix": "LocalImage_Flux2" }));
    g.node(5, "ConditioningZeroOut", json!({ "conditioning": ["4", 0] }));
    let (mut pos, mut neg) = (json!(["4", 0]), json!(["5", 0]));
    let mut image_nodes = Vec::new();
    for i in 0..p.references {
        let ids: Vec<String> = (0..4).map(|k| (20 + i * 4 + k).to_string()).collect();
        g.node(&ids[0], "LoadImage", json!({ "image": "" }));
        g.node(&ids[1], "VAEEncode", json!({ "pixels": [ids[0], 0], "vae": ["3", 0] }));
        g.node(&ids[2], "ReferenceLatent", json!({ "conditioning": pos, "latent": [ids[1], 0] }));
        pos = json!([ids[2], 0]);
        g.node(&ids[3], "ReferenceLatent", json!({ "conditioning": neg, "latent": [ids[1], 0] }));
        neg = json!([ids[3], 0]);
        image_nodes.push(ids[0].clone());
    }
    g.node(6, "CFGGuider", json!({ "model": ["1", 0], "positive": pos, "negative": neg, "cfg": 1.0 }));
    let model = g.lora_chain(json!(["1", 0]), p.loras);
    g.set("6", "model", model);
    Built { graph: g.into_value(), image_nodes }
}

pub struct ErnieParams<'a> {
    pub prompt: &'a str,
    pub negative: &'a str,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub steps: u32,
    pub cfg: f32,
}

pub fn ernie(files: &Files, p: &ErnieParams) -> Built {
    let mut g = Graph::default();
    g.node(1, "UNETLoader", json!({ "unet_name": f(files, Role::Unet), "weight_dtype": "default" }));
    g.node(2, "CLIPLoader", json!({ "clip_name": f(files, Role::Clip), "type": "flux2", "device": "default" }));
    g.node(3, "VAELoader", json!({ "vae_name": f(files, Role::Vae) }));
    g.node(4, "CLIPTextEncode", json!({ "clip": ["2", 0], "text": p.prompt }));
    g.node(5, "CLIPTextEncode", json!({ "clip": ["2", 0], "text": p.negative }));
    g.node(6, "EmptyFlux2LatentImage", json!({ "width": p.width, "height": p.height, "batch_size": 1 }));
    g.node(
        7,
        "KSampler",
        json!({ "model": ["1", 0], "seed": p.seed, "steps": p.steps, "cfg": p.cfg, "sampler_name": "euler", "scheduler": "simple",
                "positive": ["4", 0], "negative": ["5", 0], "latent_image": ["6", 0], "denoise": 1.0 }),
    );
    g.node(8, "VAEDecode", json!({ "samples": ["7", 0], "vae": ["3", 0] }));
    g.node(9, "SaveImage", json!({ "images": ["8", 0], "filename_prefix": "LocalImage_ERNIE" }));
    Built { graph: g.into_value(), image_nodes: Vec::new() }
}

/// SeedVR2 one-step upscale; the input is already resized to the target size.
pub fn seedvr2(files: &Files, seed: u64) -> Built {
    let mut g = Graph::default();
    let tile = |extra: Value| {
        let mut v = json!({ "tile_size": 512, "overlap": 128, "temporal_size": 4096, "temporal_overlap": 8 });
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        v
    };
    g.node(1, "UNETLoader", json!({ "unet_name": f(files, Role::Unet), "weight_dtype": "default" }));
    g.node(2, "VAELoader", json!({ "vae_name": f(files, Role::Vae) }));
    g.node(3, "LoadImage", json!({ "image": "" }));
    g.node(4, "SeedVR2Preprocess", json!({ "resized_images": ["3", 0] }));
    g.node(5, "VAEEncodeTiled", tile(json!({ "pixels": ["4", 0], "vae": ["2", 0] })));
    g.node(6, "SeedVR2Conditioning", json!({ "model": ["1", 0], "vae_conditioning": ["5", 0] }));
    g.node(
        7,
        "KSampler",
        json!({ "model": ["1", 0], "seed": seed, "steps": 1, "cfg": 1.0, "sampler_name": "euler", "scheduler": "simple",
                "positive": ["6", 0], "negative": ["6", 1], "latent_image": ["5", 0], "denoise": 1.0 }),
    );
    g.node(8, "VAEDecodeTiled", tile(json!({ "samples": ["7", 0], "vae": ["2", 0] })));
    g.node(9, "SeedVR2PostProcessing", json!({ "images": ["8", 0], "original_resized_images": ["3", 0], "color_correction_method": "lab" }));
    g.node(10, "SaveImage", json!({ "images": ["9", 0], "filename_prefix": "LocalImage_SeedVR2" }));
    Built { graph: g.into_value(), image_nodes: vec!["3".into()] }
}

/// The prompt fal's object-removal LoRA was trained with.
pub const KLEIN_REMOVE_PROMPT: &str = "Remove the highlighted object from the scene";

/// FLUX.2 Klein base 4B + object-removal LoRA, reading a red-outlined input image.
pub fn klein_remove(files: &Files, width: u32, height: u32, seed: u64) -> Built {
    let mut g = Graph::default();
    g.node(1, "UNETLoader", json!({ "unet_name": f(files, Role::Unet), "weight_dtype": "default" }));
    g.node(2, "LoraLoaderModelOnly", json!({ "model": ["1", 0], "lora_name": f(files, Role::Lora), "strength_model": 1.1 }));
    g.node(3, "CLIPLoader", json!({ "clip_name": f(files, Role::Clip), "type": "flux2", "device": "default" }));
    g.node(4, "VAELoader", json!({ "vae_name": f(files, Role::Vae) }));
    g.node(5, "CLIPTextEncode", json!({ "clip": ["3", 0], "text": KLEIN_REMOVE_PROMPT }));
    g.node(6, "CLIPTextEncode", json!({ "clip": ["3", 0], "text": "" }));
    g.node(30, "LoadImage", json!({ "image": "" }));
    g.node(8, "VAEEncode", json!({ "pixels": ["30", 0], "vae": ["4", 0] }));
    g.node(9, "ReferenceLatent", json!({ "conditioning": ["5", 0], "latent": ["8", 0] }));
    g.node(10, "ReferenceLatent", json!({ "conditioning": ["6", 0], "latent": ["8", 0] }));
    g.node(11, "CFGGuider", json!({ "model": ["2", 0], "positive": ["9", 0], "negative": ["10", 0], "cfg": 4.0 }));
    g.node(12, "RandomNoise", json!({ "noise_seed": seed }));
    g.node(13, "KSamplerSelect", json!({ "sampler_name": "euler" }));
    g.node(14, "Flux2Scheduler", json!({ "steps": 28, "width": width, "height": height }));
    g.node(15, "EmptyFlux2LatentImage", json!({ "width": width, "height": height, "batch_size": 1 }));
    g.node(
        16,
        "SamplerCustomAdvanced",
        json!({ "noise": ["12", 0], "guider": ["11", 0], "sampler": ["13", 0], "sigmas": ["14", 0], "latent_image": ["15", 0] }),
    );
    g.node(17, "VAEDecode", json!({ "samples": ["16", 0], "vae": ["4", 0] }));
    g.node(41, "PreviewImage", json!({ "images": ["17", 0] }));
    Built { graph: g.into_value(), image_nodes: vec!["30".into()] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Files {
        [(Role::Unet, "u".to_owned()), (Role::Clip, "c".to_owned()), (Role::Vae, "v".to_owned()), (Role::Lora, "l".to_owned())].into()
    }

    fn links_resolve(g: &Value) {
        let o = g.as_object().unwrap();
        for (id, n) in o {
            for (k, v) in n["inputs"].as_object().unwrap() {
                if let Some(a) = v.as_array()
                    && a.len() == 2
                    && a[0].is_string()
                    && a[1].is_u64()
                {
                    assert!(o.contains_key(a[0].as_str().unwrap()), "node {id}.{k} links to missing {}", a[0]);
                }
            }
        }
    }

    #[test]
    fn qwen_text_to_image_and_edit() {
        let loras = [LoraUse { name: "a.safetensors".into(), strength: 0.8 }];
        let t2i = qwen(
            &files(),
            &QwenParams { prompt: "p", negative: "", width: 1024, height: 768, seed: 7, steps: 40, cfg: 1.0, references: 0, use_cache: true, loras: &loras },
        );
        links_resolve(&t2i.graph);
        assert_eq!(t2i.graph["6"]["inputs"]["latent_image"], json!(["5", 0]));
        assert_eq!(t2i.graph["9"]["inputs"]["model"], json!(["100", 0]));
        assert_eq!(t2i.graph["6"]["inputs"]["model"], json!(["9", 0]));
        let edit = qwen(
            &files(),
            &QwenParams { prompt: "p", negative: "", width: 0, height: 0, seed: 7, steps: 25, cfg: 1.0, references: 2, use_cache: false, loras: &[] },
        );
        links_resolve(&edit.graph);
        assert_eq!(edit.image_nodes, vec!["22", "24"]);
        assert_eq!(edit.graph["4"]["inputs"]["images.image_2"], json!(["25", 0]));
        assert_eq!(edit.graph["4"]["inputs"]["resolution"], json!(0));
        assert!(edit.graph.get("5").is_none());
    }

    #[test]
    fn klein_references_chain_both_conditionings() {
        let b = flux2_klein(&files(), &Flux2Params { prompt: "p", width: 1024, height: 1024, seed: 1, steps: 4, references: 2, loras: &[] });
        links_resolve(&b.graph);
        assert_eq!(b.graph["6"]["inputs"]["positive"], json!(["26", 0]));
        assert_eq!(b.graph["6"]["inputs"]["negative"], json!(["27", 0]));
        assert_eq!(b.image_nodes, vec!["20", "24"]);
    }

    #[test]
    fn other_graphs_are_closed() {
        links_resolve(
            &z_image(&files(), &ZImageParams { prompt: "p", width: 512, height: 512, seed: 1, steps: 8, denoise: 0.6, init_image: true, loras: &[] }).graph,
        );
        links_resolve(
            &z_image(&files(), &ZImageParams { prompt: "p", width: 512, height: 512, seed: 1, steps: 8, denoise: 0.6, init_image: false, loras: &[] }).graph,
        );
        links_resolve(&ernie(&files(), &ErnieParams { prompt: "p", negative: "n", width: 512, height: 512, seed: 1, steps: 50, cfg: 4.0 }).graph);
        links_resolve(&seedvr2(&files(), 3).graph);
        let r = klein_remove(&files(), 768, 768, 5);
        links_resolve(&r.graph);
        assert_eq!(r.graph["2"]["inputs"]["lora_name"], json!("l"));
    }
}
