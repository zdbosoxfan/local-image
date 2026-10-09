//! ComfyUI API graphs for any model family, built from the family profile's pipeline recipe
//! ([`crate::family::Pipeline`]). The approach follows Krita AI Diffusion's workflow builders
//! (`ai_diffusion/workflow.py`, GPL-3.0; see docs/TOOLSET.md): one loader stage per load style
//! (checkpoint, diffusion model + text encoders + VAE, GGUF), the family's conditioning recipe
//! (CLIP text, FluxGuidance, Qwen Edit's image-aware encoder, reference latents), an optional
//! starting latent with a noise mask for refine and inpaint, and the family's sampler. Only core
//! ComfyUI nodes are used (IP-Adapter references need ComfyUI_IPAdapter_plus); masks are grown,
//! feathered and composited by Local Image itself.
//!
//! Qwen Image 2.1 keeps its own builder ([`crate::workflows::qwen`]), node for node as in 0.7.

use serde_json::{Map, Value, json};

use crate::family::{Encode, Family, InpaintMethod, Loader, RefMethod, Role, SamplerStyle};
use crate::workflows::{Built, Files, LoraUse};

/// What a graph does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Task {
    /// Text (and references) to new images.
    Create,
    /// Re-noise an image by `denoise` (0..1) and resample: img2img, refine, Draft → Refine.
    Refine { denoise: f32 },
    /// Regenerate the masked part of an image (`denoise` 1 = from scratch).
    Inpaint { denoise: f32 },
    /// Instruction edit of the first image (native edit families).
    Edit,
}

/// Everything a graph needs besides the files.
#[derive(Clone, Debug)]
pub struct Params<'a> {
    pub task: Task,
    pub prompt: &'a str,
    pub negative: &'a str,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub steps: u32,
    pub cfg: f32,
    /// Reference images (beyond the image being refined, inpainted or edited).
    pub references: usize,
    pub loras: &'a [LoraUse],
    pub batch: u32,
    /// Overrides of the family's sampler and scheduler.
    pub sampler: Option<(&'a str, &'a str)>,
}

impl<'a> Params<'a> {
    pub fn create(prompt: &'a str, width: u32, height: u32, seed: u64, steps: u32, cfg: f32) -> Self {
        Self { task: Task::Create, prompt, negative: "", width, height, seed, steps, cfg, references: 0, loras: &[], batch: 1, sampler: None }
    }
}

/// What a `LoadImage` node of a built graph expects, in [`Built::image_nodes`] order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// The image being refined, inpainted or edited (RGB).
    Source,
    /// The noise mask (grayscale: white = regenerate).
    Mask,
    Reference(usize),
}

#[derive(Default)]
struct G {
    nodes: Map<String, Value>,
    next: u32,
    images: Vec<(String, Slot)>,
}

impl G {
    fn add(&mut self, class: &str, inputs: Value) -> String {
        self.next += 1;
        let id = self.next.to_string();
        self.nodes.insert(id.clone(), json!({ "class_type": class, "inputs": inputs }));
        id
    }
    fn load(&mut self, slot: Slot) -> String {
        let id = self.add("LoadImage", json!({ "image": "" }));
        self.images.push((id.clone(), slot));
        id
    }
}

fn link(id: &str, out: u32) -> Value {
    json!([id, out])
}

fn file(files: &Files, role: Role) -> String {
    files.get(&role).cloned().unwrap_or_default()
}

fn is_gguf(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".gguf")
}

/// The graph for `p` on family `f` (with the model's overrides applied) and the resolved files.
pub fn build(f: &Family, files: &Files, p: &Params) -> Built {
    if f.pipeline.encode == Encode::QwenImage21 {
        return qwen21(files, p);
    }
    let pl = &f.pipeline;
    let mut g = G::default();
    // ---- Loaders.
    let (mut model, mut clip, mut vae);
    let ckpt = matches!(pl.loader, Loader::Checkpoint);
    if ckpt {
        let c = g.add("CheckpointLoaderSimple", json!({ "ckpt_name": file(files, Role::Checkpoint) }));
        (model, clip, vae) = (link(&c, 0), link(&c, 1), link(&c, 2));
    } else {
        let unet = file(files, Role::Unet);
        let u = if is_gguf(&unet) {
            g.add("UnetLoaderGGUF", json!({ "unet_name": unet }))
        } else {
            g.add("UNETLoader", json!({ "unet_name": unet, "weight_dtype": "default" }))
        };
        (model, clip, vae) = (link(&u, 0), Value::Null, Value::Null);
    }
    let clips: Vec<String> = [Role::Clip, Role::Clip2, Role::Clip3, Role::Clip4].iter().map(|r| file(files, *r)).filter(|s| !s.is_empty()).collect();
    let gguf_clip = clips.iter().any(|c| is_gguf(c));
    let kind = pl.clip.kind.clone().unwrap_or_default();
    let clip_node = match pl.clip.node.as_str() {
        "CLIPLoader" => {
            Some((if gguf_clip { "CLIPLoaderGGUF" } else { "CLIPLoader" }, json!({ "clip_name": clips.first().cloned().unwrap_or_default(), "type": kind })))
        }
        "DualCLIPLoader" => Some((
            if gguf_clip { "DualCLIPLoaderGGUF" } else { "DualCLIPLoader" },
            json!({ "clip_name1": clips.first().cloned().unwrap_or_default(), "clip_name2": clips.get(1).cloned().unwrap_or_default(), "type": kind }),
        )),
        "TripleCLIPLoader" => Some((
            if gguf_clip { "TripleCLIPLoaderGGUF" } else { "TripleCLIPLoader" },
            json!({ "clip_name1": clips.first().cloned().unwrap_or_default(), "clip_name2": clips.get(1).cloned().unwrap_or_default(), "clip_name3": clips.get(2).cloned().unwrap_or_default() }),
        )),
        "QuadrupleCLIPLoader" => Some((
            "QuadrupleCLIPLoader",
            json!({ "clip_name1": clips.first().cloned().unwrap_or_default(), "clip_name2": clips.get(1).cloned().unwrap_or_default(),
                    "clip_name3": clips.get(2).cloned().unwrap_or_default(), "clip_name4": clips.get(3).cloned().unwrap_or_default() }),
        )),
        _ => None,
    };
    if let Some((class, mut inputs)) = clip_node {
        if class == "CLIPLoader" {
            inputs["device"] = json!("default");
        }
        let c = g.add(class, inputs);
        clip = link(&c, 0);
    }
    if pl.clip.skip < -1 && !clip.is_null() {
        let c = g.add("CLIPSetLastLayer", json!({ "clip": clip, "stop_at_clip_layer": pl.clip.skip }));
        clip = link(&c, 0);
    }
    if pl.vae == "file" || files.contains_key(&Role::Vae) {
        let v = g.add("VAELoader", json!({ "vae_name": file(files, Role::Vae) }));
        vae = link(&v, 0);
    }
    // ---- LoRAs.
    for l in p.loras.iter().take(f.lora.max.max(1) as usize) {
        if f.lora.loader == "LoraLoader" {
            let n =
                g.add("LoraLoader", json!({ "model": model, "clip": clip, "lora_name": l.name, "strength_model": l.strength, "strength_clip": l.strength }));
            (model, clip) = (link(&n, 0), link(&n, 1));
        } else if !f.lora.loader.is_empty() {
            let n = g.add("LoraLoaderModelOnly", json!({ "model": model, "lora_name": l.name, "strength_model": l.strength }));
            model = link(&n, 0);
        }
    }
    if let Some(ms) = &pl.model_sampling
        && !ms.node.is_empty()
    {
        let n = g.add(&ms.node, json!({ "model": model, "shift": ms.shift }));
        model = link(&n, 0);
    }
    let masked = matches!(p.task, Task::Inpaint { .. });
    if masked && pl.differential {
        let n = g.add("DifferentialDiffusion", json!({ "model": model }));
        model = link(&n, 0);
    }
    // ---- Source image and mask.
    let source = matches!(p.task, Task::Refine { .. } | Task::Inpaint { .. } | Task::Edit).then(|| g.load(Slot::Source));
    let mask = masked.then(|| {
        let m = g.load(Slot::Mask);
        g.add("ImageToMask", json!({ "image": link(&m, 0), "channel": "red" }))
    });
    // ---- Conditioning.
    let refs: Vec<String> = (0..p.references).map(|i| g.load(Slot::Reference(i))).collect();
    let (mut pos, mut neg);
    let mut encoded_latent = None;
    match pl.encode {
        Encode::QwenEditPlus => {
            let mut images: Vec<Value> = Vec::new();
            if let Some(s) = &source {
                images.push(link(s, 0));
            }
            images.extend(refs.iter().map(|r| link(r, 0)));
            let mut enc = |text: &str| {
                let mut inputs = json!({ "clip": clip, "prompt": text, "vae": vae });
                for (i, im) in images.iter().take(3).enumerate() {
                    inputs[format!("image{}", i + 1)] = im.clone();
                }
                g.add("TextEncodeQwenImageEditPlus", inputs)
            };
            let pe = enc(p.prompt);
            let ne = enc(p.negative);
            (pos, neg) = (link(&pe, 0), link(&ne, 0));
        }
        _ => {
            let pe = g.add("CLIPTextEncode", json!({ "clip": clip, "text": p.prompt }));
            pos = link(&pe, 0);
            neg = if pl.zero_negative || p.cfg <= 1.0 && !f.capabilities.negative_prompt {
                let z = g.add("ConditioningZeroOut", json!({ "conditioning": pos }));
                link(&z, 0)
            } else {
                let ne = g.add("CLIPTextEncode", json!({ "clip": clip, "text": p.negative }));
                link(&ne, 0)
            };
        }
    }
    if let Some(gv) = pl.flux_guidance {
        let fg = g.add(
            "FluxGuidance",
            json!({ "conditioning": pos, "guidance": if matches!(pl.inpaint, InpaintMethod::ModelConditioning) && masked { 30.0 } else { gv } }),
        );
        pos = link(&fg, 0);
    }
    // Reference latents: the edited image first, then each reference.
    if pl.reference == RefMethod::ReferenceLatent {
        let mut chain: Vec<String> = Vec::new();
        if matches!(p.task, Task::Edit)
            && let Some(s) = &source
        {
            chain.push(s.clone());
        }
        chain.extend(refs.iter().cloned());
        for img in chain {
            let px = if f.id.starts_with("flux1") {
                let s = g.add("FluxKontextImageScale", json!({ "image": link(&img, 0) }));
                link(&s, 0)
            } else {
                link(&img, 0)
            };
            let enc = g.add("VAEEncode", json!({ "pixels": px, "vae": vae }));
            let rp = g.add("ReferenceLatent", json!({ "conditioning": pos, "latent": link(&enc, 0) }));
            pos = link(&rp, 0);
            let rn = g.add("ReferenceLatent", json!({ "conditioning": neg, "latent": link(&enc, 0) }));
            neg = link(&rn, 0);
            if encoded_latent.is_none() && matches!(p.task, Task::Edit) && f.id.starts_with("flux1") {
                encoded_latent = Some(link(&enc, 0));
            }
        }
    }
    if pl.reference == RefMethod::Redux && !refs.is_empty() {
        let cv = g.add("CLIPVisionLoader", json!({ "clip_name": file(files, Role::ClipVision) }));
        let sm = g.add("StyleModelLoader", json!({ "style_model_name": file(files, Role::StyleModel) }));
        for r in &refs {
            let e = g.add("CLIPVisionEncode", json!({ "clip_vision": link(&cv, 0), "image": link(r, 0), "crop": "center" }));
            let a = g.add(
                "StyleModelApply",
                json!({ "conditioning": pos, "style_model": link(&sm, 0), "clip_vision_output": link(&e, 0), "strength": 1.0, "strength_type": "multiply" }),
            );
            pos = link(&a, 0);
        }
    }
    if pl.reference == RefMethod::IpAdapter && !refs.is_empty() {
        let loader = g.add("IPAdapterUnifiedLoader", json!({ "model": model, "preset": "PLUS (high strength)" }));
        model = link(&loader, 0);
        let ipa = link(&loader, 1);
        for r in &refs {
            let a = g.add(
                "IPAdapterAdvanced",
                json!({ "model": model, "ipadapter": ipa, "image": link(r, 0), "weight": 0.6, "weight_type": "linear",
                        "combine_embeds": "concat", "start_at": 0.0, "end_at": 1.0, "embeds_scaling": "V only" }),
            );
            model = link(&a, 0);
        }
    }
    // ---- Latent.
    let denoise = match p.task {
        Task::Refine { denoise } | Task::Inpaint { denoise } => denoise.clamp(0.01, 1.0),
        _ => 1.0,
    };
    let empty = |g: &mut G| {
        let l = g.add(&pl.latent, json!({ "width": p.width, "height": p.height, "batch_size": p.batch.max(1) }));
        link(&l, 0)
    };
    let mut latent = match (p.task, &source) {
        (Task::Create, _) => {
            // A starting image (Z-Image variations) is the first reference.
            if pl.reference == RefMethod::InitImage && !refs.is_empty() {
                let e = g.add("VAEEncode", json!({ "pixels": link(&refs[0], 0), "vae": vae }));
                link(&e, 0)
            } else {
                empty(&mut g)
            }
        }
        (Task::Edit, Some(s)) => match encoded_latent.take() {
            Some(l) => l,
            None if pl.encode == Encode::QwenEditPlus => {
                let e = g.add("VAEEncode", json!({ "pixels": link(s, 0), "vae": vae }));
                link(&e, 0)
            }
            None => empty(&mut g),
        },
        (_, Some(s)) if masked && pl.inpaint == InpaintMethod::ModelConditioning => {
            let ic = g.add(
                "InpaintModelConditioning",
                json!({ "positive": pos, "negative": neg, "vae": vae, "pixels": link(s, 0), "mask": link(mask.as_deref().unwrap_or(""), 0), "noise_mask": true }),
            );
            (pos, neg) = (link(&ic, 0), link(&ic, 1));
            link(&ic, 2)
        }
        (_, Some(s)) => {
            let e = g.add("VAEEncode", json!({ "pixels": link(s, 0), "vae": vae }));
            let mut l = link(&e, 0);
            if let Some(m) = &mask {
                let n = g.add("SetLatentNoiseMask", json!({ "samples": l, "mask": link(m, 0) }));
                l = link(&n, 0);
            }
            l
        }
        (_, None) => empty(&mut g),
    };
    if p.batch > 1 && !matches!(p.task, Task::Create) {
        let r = g.add("RepeatLatentBatch", json!({ "samples": latent, "amount": p.batch }));
        latent = link(&r, 0);
    }
    // ---- Sampler.
    let (sampler, scheduler) = p.sampler.map(|(a, b)| (a.to_owned(), b.to_owned())).unwrap_or((pl.sampler_name.clone(), pl.scheduler.clone()));
    let samples = match pl.sampler {
        SamplerStyle::Flux2Custom => {
            let mut sigmas = {
                let s = g.add("Flux2Scheduler", json!({ "steps": p.steps, "width": p.width, "height": p.height }));
                link(&s, 0)
            };
            if denoise < 1.0 {
                let s = g.add("SplitSigmasDenoise", json!({ "sigmas": sigmas, "denoise": denoise }));
                sigmas = link(&s, 1);
            }
            let noise = g.add("RandomNoise", json!({ "noise_seed": p.seed }));
            let ks = g.add("KSamplerSelect", json!({ "sampler_name": sampler }));
            let guider = g.add("CFGGuider", json!({ "model": model, "positive": pos, "negative": neg, "cfg": p.cfg }));
            let s = g.add(
                "SamplerCustomAdvanced",
                json!({ "noise": link(&noise, 0), "guider": link(&guider, 0), "sampler": link(&ks, 0), "sigmas": sigmas, "latent_image": latent }),
            );
            link(&s, 0)
        }
        _ => {
            let k = g.add(
                "KSampler",
                json!({ "model": model, "seed": p.seed, "steps": p.steps, "cfg": p.cfg, "sampler_name": sampler, "scheduler": scheduler,
                        "positive": pos, "negative": neg, "latent_image": latent, "denoise": denoise }),
            );
            link(&k, 0)
        }
    };
    let d = g.add("VAEDecode", json!({ "samples": samples, "vae": vae }));
    g.add("SaveImage", json!({ "images": link(&d, 0), "filename_prefix": format!("LocalImage_{}", f.id) }));
    finish(g)
}

fn finish(g: G) -> Built {
    // Inputs are ordered: source, mask, then references (what ops uploads).
    let mut images = g.images;
    images.sort_by_key(|(_, s)| match s {
        Slot::Source => 0,
        Slot::Mask => 1,
        Slot::Reference(i) => 2 + i,
    });
    let slots = images.iter().map(|(_, s)| *s).collect();
    Built { graph: Value::Object(g.nodes), image_nodes: images.into_iter().map(|(id, _)| id).collect(), slots }
}

/// Qwen Image 2.1 through 0.7's builder (create, edit, references); refine and inpaint pass the
/// source as reference 1.
fn qwen21(files: &Files, p: &Params) -> Built {
    let with_source = !matches!(p.task, Task::Create);
    let refs = p.references + usize::from(with_source);
    let mut b = crate::workflows::qwen(
        files,
        &crate::workflows::QwenParams {
            prompt: p.prompt,
            negative: p.negative,
            width: p.width,
            height: p.height,
            seed: p.seed,
            steps: p.steps,
            cfg: p.cfg,
            references: refs,
            use_cache: false,
            loras: p.loras,
        },
    );
    b.slots = (0..refs).map(|i| if with_source && i == 0 { Slot::Source } else { Slot::Reference(i - usize::from(with_source)) }).collect();
    b
}

/// The node classes a family's plain Create graph uses (what ComfyUI must offer).
pub fn required_nodes(f: &Family) -> Vec<String> {
    if f.kind != crate::family::FamilyKind::Image {
        return Vec::new();
    }
    let mut files = Files::new();
    for r in [Role::Unet, Role::Checkpoint, Role::Clip, Role::Clip2, Role::Clip3, Role::Clip4, Role::Vae] {
        files.insert(r, "x.safetensors".into());
    }
    if f.pipeline.vae != "file" {
        files.remove(&Role::Vae);
    }
    let p = Params::create("", 1024, 1024, 0, 1, 1.0);
    let b = build(f, &files, &p);
    let mut nodes: Vec<String> =
        b.graph.as_object().map(|o| o.values().filter_map(|n| n["class_type"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    nodes.sort();
    nodes.dedup();
    nodes
}

/// Extra nodes a task needs beyond Create (shown when a mode is unavailable).
pub fn task_nodes(f: &Family, task: Task, references: usize) -> Vec<String> {
    let mut files = Files::new();
    for r in [Role::Unet, Role::Checkpoint, Role::Clip, Role::Clip2, Role::Clip3, Role::Clip4, Role::Vae, Role::ClipVision, Role::StyleModel] {
        files.insert(r, "x.safetensors".into());
    }
    let mut p = Params::create("", 1024, 1024, 0, 1, 1.0);
    p.task = task;
    p.references = references;
    let b = build(f, &files, &p);
    let mut nodes: Vec<String> =
        b.graph.as_object().map(|o| o.values().filter_map(|n| n["class_type"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    nodes.sort();
    nodes.dedup();
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family::registry;

    fn closed(g: &Value) {
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

    fn classes(b: &Built) -> Vec<String> {
        b.graph.as_object().unwrap().values().map(|n| n["class_type"].as_str().unwrap().to_owned()).collect()
    }

    fn files() -> Files {
        [Role::Unet, Role::Checkpoint, Role::Clip, Role::Clip2, Role::Clip3, Role::Clip4, Role::Vae, Role::ClipVision, Role::StyleModel]
            .into_iter()
            .map(|r| (r, format!("{r:?}.safetensors")))
            .collect()
    }

    #[test]
    fn every_family_builds_closed_graphs_for_every_task() {
        for f in &registry().families {
            if f.kind != crate::family::FamilyKind::Image {
                continue;
            }
            for task in [Task::Create, Task::Refine { denoise: 0.5 }, Task::Inpaint { denoise: 1.0 }, Task::Edit] {
                for refs in [0, 2] {
                    let mut p = Params::create("a cat", 1024, 768, 3, 20, 4.0);
                    p.task = task;
                    p.references = refs;
                    p.loras = &[];
                    let b = build(f, &files(), &p);
                    closed(&b.graph);
                    assert_eq!(b.image_nodes.len(), b.slots.len(), "{} {task:?}", f.id);
                    assert!(classes(&b).iter().any(|c| c == "SaveImage"), "{} {task:?}", f.id);
                }
            }
        }
    }

    #[test]
    fn checkpoint_families_use_one_loader_and_clip_skip() {
        let f = registry().family("pony").unwrap();
        let mut fl = Files::new();
        fl.insert(Role::Checkpoint, "pony.safetensors".into());
        let lora = [LoraUse { name: "style.safetensors".into(), strength: 0.8 }];
        let mut p = Params::create("score_9, a fox", 1024, 1024, 1, 25, 7.0);
        p.loras = &lora;
        let b = build(f, &fl, &p);
        let c = classes(&b);
        assert!(c.contains(&"CheckpointLoaderSimple".into()) && c.contains(&"CLIPSetLastLayer".into()) && c.contains(&"LoraLoader".into()));
        assert!(!c.contains(&"VAELoader".into()));
    }

    #[test]
    fn flux_kontext_edit_chains_reference_latents() {
        let f = registry().family("flux1-kontext").unwrap();
        let mut p = Params::create("make it night", 1024, 1024, 1, 20, 1.0);
        p.task = Task::Edit;
        p.references = 1;
        let b = build(f, &files(), &p);
        let c = classes(&b);
        assert_eq!(c.iter().filter(|x| *x == "ReferenceLatent").count(), 4);
        assert!(c.contains(&"FluxGuidance".into()) && c.contains(&"DualCLIPLoader".into()));
        assert_eq!(b.slots, vec![Slot::Source, Slot::Reference(0)]);
    }

    #[test]
    fn inpaint_adds_a_noise_mask_and_differential_diffusion() {
        let f = registry().family("sdxl").unwrap();
        let mut p = Params::create("a vase", 1024, 1024, 1, 30, 6.0);
        p.task = Task::Inpaint { denoise: 0.8 };
        let b = build(f, &files(), &p);
        let c = classes(&b);
        assert!(c.contains(&"SetLatentNoiseMask".into()) && c.contains(&"DifferentialDiffusion".into()) && c.contains(&"ImageToMask".into()));
        let k = b.graph.as_object().unwrap().values().find(|n| n["class_type"] == "KSampler").unwrap();
        assert_eq!(k["inputs"]["denoise"], json!(0.8f32));
        assert_eq!(b.slots, vec![Slot::Source, Slot::Mask]);
        let fill = registry().family("flux1-fill").unwrap();
        let c = classes(&build(fill, &files(), &p));
        assert!(c.contains(&"InpaintModelConditioning".into()));
    }

    #[test]
    fn qwen_edit_feeds_images_to_the_encoder() {
        let f = registry().family("qwen-edit").unwrap();
        let mut p = Params::create("replace the sky", 1024, 1024, 1, 20, 2.5);
        p.task = Task::Edit;
        p.references = 1;
        let b = build(f, &files(), &p);
        let enc = b.graph.as_object().unwrap().values().find(|n| n["class_type"] == "TextEncodeQwenImageEditPlus").unwrap();
        assert!(enc["inputs"].get("image1").is_some() && enc["inputs"].get("image2").is_some());
        assert!(classes(&b).contains(&"ModelSamplingAuraFlow".into()));
    }

    #[test]
    fn original_models_keep_their_node_sets() {
        let (fam, m) = registry().model("flux2-klein-4b").unwrap();
        let f = fam.resolved_for(m);
        let mut p = Params::create("p", 1024, 1024, 1, 4, 1.0);
        p.references = 2;
        let c = classes(&build(&f, &files(), &p));
        for n in
            ["UNETLoader", "CLIPLoader", "VAELoader", "EmptyFlux2LatentImage", "Flux2Scheduler", "CFGGuider", "SamplerCustomAdvanced", "ConditioningZeroOut"]
        {
            assert!(c.contains(&n.to_owned()), "{n}");
        }
        assert_eq!(c.iter().filter(|x| *x == "ReferenceLatent").count(), 4);
        let (fam, m) = registry().model("z-image-turbo").unwrap();
        let req = required_nodes(&fam.resolved_for(m));
        assert!(req.contains(&"ModelSamplingAuraFlow".into()) && req.contains(&"EmptySD3LatentImage".into()));
        let q = registry().model("qwen").unwrap();
        assert!(required_nodes(&q.0.resolved_for(q.1)).contains(&"TextEncodeQwenImage21".into()));
    }
}
