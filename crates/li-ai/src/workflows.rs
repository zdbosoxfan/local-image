//! The ComfyUI graphs that keep 0.7's exact node layout: SeedVR2 and the AI Remove graph.
//! Everything else is built per family by [`crate::builders`]. Each builder takes the loader file
//! names resolved by [`crate::catalog::availability`].
//! `LoadImage` nodes are created with an empty `image`; [`crate::comfy::ComfyClient::run`] uploads
//! the pixels and fills them in.

use crate::family::Role;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

pub type Files = BTreeMap<Role, String>;

/// A LoRA chosen for a generation: its ComfyUI `lora_name` and strength (−2..2).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    fn into_value(self) -> Value {
        Value::Object(self.0)
    }
}

fn f(files: &Files, role: Role) -> String {
    files.get(&role).cloned().unwrap_or_default()
}

/// Node ids of the `LoadImage` nodes a graph expects, in order, and what each one is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Built {
    pub graph: Value,
    pub image_nodes: Vec<String>,
    pub slots: Vec<crate::builders::Slot>,
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
    Built { graph: g.into_value(), image_nodes: vec!["3".into()], slots: Vec::new() }
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
    Built { graph: g.into_value(), image_nodes: vec!["30".into()], slots: Vec::new() }
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
    fn other_graphs_are_closed() {
        links_resolve(&seedvr2(&files(), 3).graph);
        let r = klein_remove(&files(), 768, 768, 5);
        links_resolve(&r.graph);
        assert_eq!(r.graph["2"]["inputs"]["lora_name"], json!("l"));
    }
}
