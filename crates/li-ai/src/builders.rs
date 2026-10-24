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
    /// Z-Image Turbo Create: 0 = Off, 100 = the tested maximum conditioning blend.
    pub variety: f32,
    /// Create with an InitImage reference: how much of the starting latent to re-noise.
    pub init_denoise: f32,
}

impl<'a> Params<'a> {
    pub fn create(prompt: &'a str, width: u32, height: u32, seed: u64, steps: u32, cfg: f32) -> Self {
        Self {
            task: Task::Create,
            prompt,
            negative: "",
            width,
            height,
            seed,
            steps,
            cfg,
            references: 0,
            loras: &[],
            batch: 1,
            sampler: None,
            variety: 0.0,
            init_denoise: 1.0,
        }
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

/// SplitMix64 integer steps keep alternate text independent of RNG crate versions and thread
/// order. This is an alternate conditioning blend, not Gaussian tensor noise: core ComfyUI
/// cannot create that conditioning. The model's own text encoder encodes the alternate text.
/// Reference: Sebastiano Vigna, <https://prng.di.unimi.it/splitmix64.c> (public domain;
/// pinned CC0 snapshot and notice in docs/PORTS.md and licenses/splitmix64-NOTICE.md).
fn variety_text(seed: u64) -> String {
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

/// The graph for `p` on family `f` (with the model's overrides applied) and the resolved files.
pub fn build(f: &Family, files: &Files, p: &Params) -> Built {
    if f.pipeline.encode == Encode::QwenImage21 {
        return qwen21(f, files, p);
    }
    let pl = &f.pipeline;
    let mut g = G::default();
    // Loaders.
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
    // LoRAs.
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
    if matches!(pl.encode, Encode::QwenEditPlus | Encode::QwenEdit) {
        // Both the 2509 and 2511 edit templates normalize CFG after the sampling shift.
        let n = g.add("CFGNorm", json!({ "model": model, "strength": 1.0 }));
        model = link(&n, 0);
    }
    let masked = matches!(p.task, Task::Inpaint { .. });
    if masked && pl.differential {
        let n = g.add("DifferentialDiffusion", json!({ "model": model }));
        model = link(&n, 0);
    }
    // Source image and mask.
    let source = matches!(p.task, Task::Refine { .. } | Task::Inpaint { .. } | Task::Edit).then(|| g.load(Slot::Source));
    let mask = masked.then(|| {
        let m = g.load(Slot::Mask);
        g.add("ImageToMask", json!({ "image": link(&m, 0), "channel": "red" }))
    });
    // Conditioning.
    let refs: Vec<String> = (0..p.references).map(|i| g.load(Slot::Reference(i))).collect();
    let (mut pos, mut neg);
    let mut encoded_latent = None;
    match pl.encode {
        Encode::QwenEditPlus | Encode::QwenEdit => {
            let mut images: Vec<Value> = Vec::new();
            if let Some(s) = &source {
                images.push(link(s, 0));
            }
            images.extend(refs.iter().map(|r| link(r, 0)));
            if pl.encode == Encode::QwenEdit
                && let Some(image) = images.first_mut()
            {
                let scale = g.add("ImageScaleToTotalPixels", json!({ "image": image, "upscale_method": "lanczos", "megapixels": 1.5, "resolution_steps": 1 }));
                *image = link(&scale, 0);
                if source.is_some() {
                    let enc = g.add("VAEEncode", json!({ "pixels": image, "vae": vae }));
                    encoded_latent = Some(link(&enc, 0));
                }
            }
            let mut enc = |text: &str| {
                let mut inputs = json!({ "clip": clip, "prompt": text, "vae": vae });
                if pl.encode == Encode::QwenEdit {
                    if let Some(image) = images.first() {
                        inputs["image"] = image.clone();
                    }
                    return g.add("TextEncodeQwenImageEdit", inputs);
                }
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
            neg = if p.cfg <= 1.0 && (pl.zero_negative || !f.capabilities.negative_prompt) {
                let z = g.add("ConditioningZeroOut", json!({ "conditioning": pos }));
                link(&z, 0)
            } else {
                let ne = g.add("CLIPTextEncode", json!({ "clip": clip, "text": p.negative }));
                link(&ne, 0)
            };
        }
    }
    // Perturb only early conditioning, then restore the original prompt. The tested maximum
    // blend is 35%, with a 20% schedule switchover. Off,
    // Base Z-Image and every untested task/family keep their original graphs.
    if f.id == "z-image" && file(files, Role::Unet).to_ascii_lowercase().contains("turbo") && p.task == Task::Create && p.variety.is_finite() && p.variety > 0.0
    {
        let alt = g.add("CLIPTextEncode", json!({"clip": clip, "text": variety_text(p.seed)}));
        let mix = g.add(
            "ConditioningAverage",
            json!({"conditioning_to": pos, "conditioning_from": link(&alt,0),
            "conditioning_to_strength": 1.0 - 0.35 * p.variety.clamp(0.0,100.0) / 100.0}),
        );
        let early = g.add("ConditioningSetTimestepRange", json!({"conditioning": link(&mix,0), "start": 0.0, "end": 0.2}));
        let late = g.add("ConditioningSetTimestepRange", json!({"conditioning": pos, "start": 0.2, "end": 1.0}));
        let combined = g.add("ConditioningCombine", json!({"conditioning_1": link(&early,0), "conditioning_2": link(&late,0)}));
        pos = link(&combined, 0);
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
        // Edit and Refine both give the model the source image as its first reference: FLUX.2 Klein refines by
        // reference (Comfy-Org's image_flux2_klein_image_edit templates), not by re-noising an encoded latent.
        if matches!(p.task, Task::Edit | Task::Refine { .. })
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
                // The templates scale every reference to about one megapixel first.
                let s =
                    g.add("ImageScaleToTotalPixels", json!({ "image": link(&img, 0), "upscale_method": "lanczos", "megapixels": 1.0, "resolution_steps": 1 }));
                link(&s, 0)
            };
            let enc = g.add("VAEEncode", json!({ "pixels": px, "vae": vae }));
            let rp = g.add("ReferenceLatent", json!({ "conditioning": pos, "latent": link(&enc, 0) }));
            pos = link(&rp, 0);
            let rn = g.add("ReferenceLatent", json!({ "conditioning": neg, "latent": link(&enc, 0) }));
            neg = link(&rn, 0);
            if encoded_latent.is_none() && matches!(p.task, Task::Edit | Task::Refine { .. }) && f.id.starts_with("flux1") {
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
    // Latent.
    let denoise = match p.task {
        Task::Refine { denoise } | Task::Inpaint { denoise } => denoise.clamp(0.01, 1.0),
        Task::Create if pl.reference == RefMethod::InitImage && !refs.is_empty() => p.init_denoise.clamp(0.0, 1.0),
        _ => 1.0,
    };
    let empty = |g: &mut G| {
        let l =
            g.add(&pl.latent, json!({ "width": p.width, "height": p.height, "batch_size": if matches!(p.task, Task::Create) { p.batch.max(1) } else { 1 } }));
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
            None if matches!(pl.encode, Encode::QwenEditPlus | Encode::QwenEdit) => {
                let scale = g.add("FluxKontextImageScale", json!({ "image": link(s, 0) }));
                let e = g.add("VAEEncode", json!({ "pixels": link(&scale, 0), "vae": vae }));
                link(&e, 0)
            }
            None => empty(&mut g),
        },
        // Reference refine (FLUX.2): at full denoise an empty latent like the official template; below it, start from
        // the encoded source as well (the sampler's sigmas are split to the denoise below) to keep even more of it.
        (Task::Refine { .. }, Some(_)) if pl.sampler == SamplerStyle::Flux2Custom && denoise == 1.0 => empty(&mut g),
        (_, Some(s)) if pl.inpaint == InpaintMethod::ModelConditioning && (masked || matches!(p.task, Task::Refine { .. })) => {
            // Fill models require image+mask concatenation even for whole-image refinement.
            // A white canvas mask refines everything while partial denoise keeps the source.
            let noise_mask = mask.clone().unwrap_or_else(|| g.add("SolidMask", json!({ "value": 1.0, "width": p.width, "height": p.height })));
            let ic = g.add(
                "InpaintModelConditioning",
                json!({ "positive": pos, "negative": neg, "vae": vae, "pixels": link(s, 0), "mask": link(&noise_mask, 0), "noise_mask": true }),
            );
            (pos, neg) = (link(&ic, 0), link(&ic, 1));
            link(&ic, 2)
        }
        (_, Some(s)) => {
            let mut l = encoded_latent.take().unwrap_or_else(|| {
                let pixels = if matches!(pl.encode, Encode::QwenEditPlus | Encode::QwenEdit) {
                    let scale = g.add("FluxKontextImageScale", json!({ "image": link(s, 0) }));
                    link(&scale, 0)
                } else {
                    link(s, 0)
                };
                let e = g.add("VAEEncode", json!({ "pixels": pixels, "vae": vae }));
                link(&e, 0)
            });
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
    // Sampler.
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

/// Qwen Image 2.1 through 0.7's builder (create, edit, references). Full refine uses the official
/// edit path; partial refine also encodes the source, since the text encoder's latent is empty.
fn qwen21(f: &Family, files: &Files, p: &Params) -> Built {
    let with_source = !matches!(p.task, Task::Create);
    let refs = p.references + usize::from(with_source);
    let mut b = crate::workflows::qwen(
        files,
        &crate::workflows::QwenParams {
            sampler_name: &f.pipeline.sampler_name,
            scheduler: &f.pipeline.scheduler,
            prompt: p.prompt,
            negative: p.negative,
            width: p.width,
            height: p.height,
            seed: p.seed,
            steps: p.steps,
            cfg: p.cfg,
            denoise: match p.task {
                Task::Refine { denoise } | Task::Inpaint { denoise } => denoise,
                _ => 1.0,
            },
            references: refs,
            use_cache: false,
            loras: p.loras,
        },
    );
    b.slots = (0..refs).map(|i| if with_source && i == 0 { Slot::Source } else { Slot::Reference(i - usize::from(with_source)) }).collect();
    if let Task::Refine { denoise } | Task::Inpaint { denoise } = p.task {
        let denoise = denoise.clamp(0.0, 1.0);
        b.graph["6"]["inputs"]["denoise"] = json!(denoise);
        if denoise < 1.0 {
            // Match TextEncodeQwenImage21's resolution=0 rounding. Both the vision reference
            // and the starting latent must describe the same canvas, including RGBA channels.
            let snap = |v: u32| ((f64::from(v) / 32.0).round_ties_even().max(1.0) as u32) * 32;
            let source = b.graph["4"]["inputs"]["images.image_1"].clone();
            b.graph["200"] = json!({ "class_type": "ImageScale", "inputs": {
                "image": source, "width": snap(p.width), "height": snap(p.height), "upscale_method": "lanczos", "crop": "disabled"
            }});
            b.graph["4"]["inputs"]["images.image_1"] = json!(["200", 0]);
            b.graph["201"] = json!({ "class_type": "VAEEncode", "inputs": { "pixels": ["200", 0], "vae": ["3", 0] }});
            b.graph["6"]["inputs"]["latent_image"] = json!(["201", 0]);
        }
    }
    if let Some((sampler, scheduler)) = p.sampler {
        b.graph["6"]["inputs"]["sampler_name"] = json!(sampler);
        b.graph["6"]["inputs"]["scheduler"] = json!(scheduler);
    }
    if p.batch > 1 {
        let latent = b.graph["6"]["inputs"]["latent_image"].clone();
        b.graph["202"] = json!({ "class_type": "RepeatLatentBatch", "inputs": { "samples": latent, "amount": p.batch }});
        b.graph["6"]["inputs"]["latent_image"] = json!(["202", 0]);
    }
    b
}

fn graph_classes(b: &Built) -> Vec<String> {
    let mut nodes: Vec<String> = b
        .graph
        .as_object()
        .map(|o| o.values().filter_map(|n| n["class_type"].as_str()).filter(|c| !c.is_empty()).map(str::to_owned).collect())
        .unwrap_or_default();
    nodes.sort();
    nodes.dedup();
    nodes
}

/// The node classes a family's plain Create graph uses (what ComfyUI must offer). Families
/// without a builder (unknown files, template-only families) need none here.
pub fn required_nodes(f: &Family) -> Vec<String> {
    if f.id == "restoration" {
        return crate::restoration::NODES.iter().map(|n| (*n).to_owned()).collect();
    }
    if f.id == "unknown" || !f.buildable() {
        return Vec::new();
    }
    let mut files = Files::new();
    for r in [Role::Unet, Role::Checkpoint, Role::Clip, Role::Clip2, Role::Clip3, Role::Clip4, Role::Vae] {
        files.insert(r, "x.safetensors".into());
    }
    if f.pipeline.vae != "file" {
        files.remove(&Role::Vae);
    }
    let mut p = Params::create("", 1024, 1024, 0, 1, 1.0);
    if f.id == "z-image" {
        files.insert(Role::Unet, "z_image_turbo.safetensors".into());
        p.variety = 100.0;
    }
    graph_classes(&build(f, &files, &p))
}

/// Extra nodes a task needs beyond Create (shown when a mode is unavailable).
pub fn task_nodes(f: &Family, task: Task, references: usize) -> Vec<String> {
    if f.id == "unknown" || !f.buildable() {
        return Vec::new();
    }
    let mut files = Files::new();
    for r in [Role::Unet, Role::Checkpoint, Role::Clip, Role::Clip2, Role::Clip3, Role::Clip4, Role::Vae, Role::ClipVision, Role::StyleModel] {
        files.insert(r, "x.safetensors".into());
    }
    let mut p = Params::create("", 1024, 1024, 0, 1, 1.0);
    p.task = task;
    p.references = references;
    graph_classes(&build(f, &files, &p))
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

    fn node<'a>(b: &'a Built, class: &str) -> &'a Value {
        b.graph.as_object().unwrap().values().find(|n| n["class_type"] == class).unwrap_or_else(|| panic!("missing {class}"))
    }

    fn upstream<'a>(b: &'a Built, n: &Value, input: &str) -> &'a Value {
        &b.graph[n["inputs"][input][0].as_str().unwrap()]
    }

    #[test]
    fn zimage_variety_graph_restores_prompt_and_is_seed_deterministic() {
        let f = registry().family("z-image").unwrap();
        let mut files = files();
        files.insert(Role::Unet, "z_image_turbo_bf16.safetensors".into());
        let mut p = Params::create("a fox in a forest", 512, 288, 42, 8, 1.0);
        assert_eq!(p.variety, 0.0);
        let off = build(f, &files, &p);
        assert!(!classes(&off).contains(&"ConditioningAverage".into()));
        p.variety = 100.0;
        let on = build(f, &files, &p);
        closed(&on.graph);
        assert_eq!(build(f, &files, &p).graph, on.graph);
        let mix = node(&on, "ConditioningAverage");
        assert!((mix["inputs"]["conditioning_to_strength"].as_f64().unwrap() - 0.65).abs() < 1e-6);
        let original = mix["inputs"]["conditioning_to"].clone();
        assert_eq!(upstream(&on, mix, "conditioning_to")["inputs"]["text"], p.prompt);
        assert_eq!(upstream(&on, mix, "conditioning_from")["inputs"]["text"], variety_text(42));
        let combined = upstream(&on, node(&on, "KSampler"), "positive");
        assert_eq!(combined["class_type"], "ConditioningCombine");
        let (early, late) = (upstream(&on, combined, "conditioning_1"), upstream(&on, combined, "conditioning_2"));
        assert_eq!(early["inputs"]["end"], 0.2);
        assert_eq!(late["inputs"]["start"], 0.2);
        assert_eq!(late["inputs"]["conditioning"], original);
        assert_eq!(node(&on, "ConditioningZeroOut")["inputs"]["conditioning"], original);
        assert_eq!(variety_text(42).split_whitespace().next(), Some("2feb6e95"));
        p.seed = 43;
        let other = build(f, &files, &p);
        assert_ne!(upstream(&other, node(&other, "ConditioningAverage"), "conditioning_from")["inputs"]["text"], variety_text(42));
        p.seed = 42;
        p.variety = 40.0;
        assert_ne!(build(f, &files, &p).graph, on.graph);
        p.variety = 0.0;
        assert_eq!(build(f, &files, &p).graph, off.graph);
    }

    #[test]
    fn zimage_variety_does_not_change_base_other_families_or_non_create_tasks() {
        for (id, main) in [("z-image", "z_image_bf16.safetensors"), ("flux2-klein", "flux-2-klein-4b.safetensors")] {
            let mut files = files();
            files.insert(Role::Unet, main.into());
            let f = registry().family(id).unwrap().resolved_for_file(main, None);
            let mut p = Params::create("a fox", 512, 288, 42, 8, 1.0);
            let off = build(&f, &files, &p);
            p.variety = 100.0;
            assert_eq!(build(&f, &files, &p).graph, off.graph);
        }
        let f = registry().family("z-image").unwrap();
        let mut files = files();
        files.insert(Role::Unet, "z_image_turbo_bf16.safetensors".into());
        for task in [Task::Refine { denoise: 0.4 }, Task::Inpaint { denoise: 1.0 }, Task::Edit] {
            let mut p = Params::create("a fox", 512, 288, 42, 8, 1.0);
            p.task = task;
            let off = build(f, &files, &p);
            p.variety = 100.0;
            assert_eq!(build(f, &files, &p).graph, off.graph);
        }
    }

    #[test]
    fn zimage_variety_request_defaults_off_and_rejects_invalid_levels() {
        let mut req = crate::GenerateRequest::new(crate::ModelId::ZImageTurbo, "a fox");
        assert_eq!(req.variety, 0.0);
        for level in [0.0, 40.0, 100.0] {
            req.variety = level;
            req.validate().unwrap();
        }
        for level in [-1.0, 101.0, f32::NAN, f32::INFINITY] {
            req.variety = level;
            assert!(req.validate().unwrap_err().to_string().contains("Variety"));
        }
    }

    #[test]
    fn qwen21_refines_like_the_official_edit_and_encodes_for_partial_denoise() {
        let f = registry().family("qwen-image-21").unwrap();
        for denoise in [1.0, 0.4] {
            let mut p = Params::create("keep the composition", 1360, 768, 3, 25, 1.0);
            p.task = Task::Refine { denoise };
            p.references = 1;
            let b = build(f, &files(), &p);
            closed(&b.graph);
            let k = node(&b, "KSampler");
            let enc = node(&b, "TextEncodeQwenImage21");
            assert_eq!(k["inputs"]["denoise"], json!(denoise));
            assert_eq!(k["inputs"]["positive"], json!(["4", 0]));
            assert_eq!(k["inputs"]["negative"], json!(["4", 1]));
            assert_eq!(enc["inputs"]["resolution"], 0);
            assert_eq!(b.slots, vec![Slot::Source, Slot::Reference(0)]);
            if denoise == 1.0 {
                // Official: encoder output 2 is EMPTY and follows reference 1's canvas.
                assert_eq!(k["inputs"]["latent_image"], json!(["4", 2]));
                assert!(!classes(&b).contains(&"VAEEncode".into()));
            } else {
                let vae = upstream(&b, k, "latent_image");
                assert_eq!(vae["class_type"], "VAEEncode");
                assert_eq!(vae["inputs"]["pixels"], enc["inputs"]["images.image_1"]);
                let scale = upstream(&b, vae, "pixels");
                assert_eq!(scale["class_type"], "ImageScale");
                assert_eq!(scale["inputs"]["width"], 1344); // Comfy's ties-to-even, 32px grid
                assert_eq!(scale["inputs"]["height"], 768);
                assert_eq!(upstream(&b, scale, "image")["class_type"], "JoinImageWithAlpha");
            }
        }
        assert_eq!(crate::catalog::refine_default_denoise(crate::ModelId::Qwen), 1.0);
    }

    #[test]
    fn qwen_edit_refine_and_edit_follow_the_official_scaled_cfg_norm_path() {
        let f = registry().family("qwen-edit").unwrap();
        for task in [Task::Edit, Task::Refine { denoise: 1.0 }, Task::Refine { denoise: 0.4 }] {
            let mut p = Params::create("keep the composition", 1024, 768, 3, 20, 2.5);
            p.task = task;
            p.references = 1;
            let b = build(f, &files(), &p);
            closed(&b.graph);
            let k = node(&b, "KSampler");
            let norm = upstream(&b, k, "model");
            assert_eq!(norm["class_type"], "CFGNorm");
            assert_eq!(norm["inputs"]["strength"], 1.0);
            assert_eq!(upstream(&b, norm, "model")["class_type"], "ModelSamplingAuraFlow");
            let latent = upstream(&b, k, "latent_image");
            assert_eq!(latent["class_type"], "VAEEncode");
            let scale = upstream(&b, latent, "pixels");
            assert_eq!(scale["class_type"], "FluxKontextImageScale");
            assert_eq!(scale["inputs"]["image"], json!([b.image_nodes[0], 0]));
            for input in ["positive", "negative"] {
                let enc = upstream(&b, k, input);
                assert_eq!(enc["class_type"], "TextEncodeQwenImageEditPlus");
                assert_eq!(enc["inputs"]["image1"], json!([b.image_nodes[0], 0]));
                assert_eq!(enc["inputs"]["image2"], json!([b.image_nodes[1], 0]));
                assert_eq!(enc["inputs"]["vae"], latent["inputs"]["vae"]);
            }
            assert_eq!(k["inputs"]["denoise"], json!(if let Task::Refine { denoise } = task { denoise } else { 1.0 }));
        }
    }

    #[test]
    fn flux1_kontext_refine_keeps_the_official_scaled_source_latent() {
        let f = registry().family("flux1-kontext").unwrap();
        for task in [Task::Edit, Task::Refine { denoise: 1.0 }, Task::Refine { denoise: 0.4 }] {
            let mut p = Params::create("keep the composition", 1360, 768, 3, 20, 1.0);
            p.task = task;
            let b = build(f, &files(), &p);
            closed(&b.graph);
            let k = node(&b, "KSampler");
            let latent = upstream(&b, k, "latent_image");
            assert_eq!(latent["class_type"], "VAEEncode");
            assert_eq!(upstream(&b, latent, "pixels")["class_type"], "FluxKontextImageScale");
            assert_eq!(node(&b, "ReferenceLatent")["inputs"]["latent"], k["inputs"]["latent_image"]);
            assert!(!classes(&b).contains(&"EmptySD3LatentImage".into()));
            assert_eq!(classes(&b).iter().filter(|c| *c == "VAEEncode").count(), 1);
            assert_eq!(node(&b, "FluxGuidance")["inputs"]["guidance"], 2.5);
        }
    }

    #[test]
    fn flux2_dev_refine_follows_the_official_reference_scheduler_path() {
        let f = registry().family("flux2").unwrap();
        for denoise in [1.0, 0.4, 0.9995] {
            let mut p = Params::create("keep the composition", 1360, 768, 3, 28, 1.0);
            p.task = Task::Refine { denoise };
            let b = build(f, &files(), &p);
            closed(&b.graph);
            assert_eq!(node(&b, "ImageScaleToTotalPixels")["inputs"]["resolution_steps"], 1);
            assert_eq!(node(&b, "FluxGuidance")["inputs"]["guidance"], 4.0);
            let s = node(&b, "SamplerCustomAdvanced");
            if denoise == 1.0 {
                assert_eq!(upstream(&b, s, "latent_image")["class_type"], "EmptyFlux2LatentImage");
                assert_eq!(upstream(&b, s, "sigmas")["class_type"], "Flux2Scheduler");
            } else {
                assert_eq!(upstream(&b, s, "latent_image")["class_type"], "VAEEncode");
                assert_eq!(upstream(&b, s, "sigmas")["class_type"], "SplitSigmasDenoise");
                assert_eq!(s["inputs"]["sigmas"][1], 1);
            }
            assert_eq!(node(&b, "CFGGuider")["inputs"]["cfg"], 1.0); // equivalent to official BasicGuider
        }
    }

    #[test]
    fn conventional_refine_families_keep_the_source_and_partial_denoise() {
        for family in ["sd15", "sdxl", "sd35", "pony", "illustrious", "hidream", "ernie", "z-image", "qwen-image", "flux1"] {
            let f = registry().family(family).unwrap();
            let mut p = Params::create("preserve the composition", 1024, 768, 3, 20, 4.0);
            p.task = Task::Refine { denoise: 0.4 };
            let b = build(f, &files(), &p);
            let k = node(&b, "KSampler");
            let latent = upstream(&b, k, "latent_image");
            assert_eq!(latent["class_type"], "VAEEncode", "{family}");
            assert_eq!(latent["inputs"]["pixels"], json!([b.image_nodes[0], 0]), "{family}");
            assert_eq!(k["inputs"]["denoise"], json!(0.4f32), "{family}");
            assert_eq!(b.slots, vec![Slot::Source], "{family}");
            closed(&b.graph);
        }
    }

    #[test]
    fn flux1_fill_refine_keeps_the_official_image_and_mask_conditioning() {
        let f = registry().family("flux1-fill").unwrap();
        let mut p = Params::create("preserve the composition", 1024, 768, 3, 20, 1.0);
        p.task = Task::Refine { denoise: 0.4 };
        let b = build(f, &files(), &p);
        let k = node(&b, "KSampler");
        let cond = upstream(&b, k, "latent_image");
        assert_eq!(cond["class_type"], "InpaintModelConditioning");
        assert_eq!(k["inputs"]["latent_image"][1], 2);
        assert_eq!(k["inputs"]["positive"][0], k["inputs"]["latent_image"][0]);
        assert_eq!(k["inputs"]["positive"][1], 0);
        assert_eq!(k["inputs"]["negative"][0], k["inputs"]["latent_image"][0]);
        assert_eq!(k["inputs"]["negative"][1], 1);
        assert_eq!(cond["inputs"]["pixels"], json!([b.image_nodes[0], 0]));
        let mask = upstream(&b, cond, "mask");
        assert_eq!(mask["class_type"], "SolidMask");
        assert_eq!(mask["inputs"]["value"], 1.0);
        assert_eq!(mask["inputs"]["width"], 1024);
        assert_eq!(mask["inputs"]["height"], 768);
        assert_eq!(k["inputs"]["denoise"], json!(0.4f32));
        assert_eq!(node(&b, "FluxGuidance")["inputs"]["guidance"], 30.0);
        assert_eq!(b.slots, vec![Slot::Source]);
        closed(&b.graph);
    }

    #[test]
    fn original_qwen_edit_refines_with_its_official_single_image_encoder() {
        let f = registry().family("qwen-edit").unwrap().resolved_for_file("qwen_image_edit_fp8_e4m3fn.safetensors", None);
        for task in [Task::Edit, Task::Refine { denoise: 0.4 }] {
            let mut p = Params::create("keep the composition", 1024, 768, 3, 20, 2.5);
            p.task = task;
            let b = build(&f, &files(), &p);
            let k = node(&b, "KSampler");
            let latent = upstream(&b, k, "latent_image");
            assert_eq!(latent["class_type"], "VAEEncode");
            let scale = upstream(&b, latent, "pixels");
            assert_eq!(scale["class_type"], "ImageScaleToTotalPixels");
            assert_eq!(scale["inputs"]["megapixels"], 1.5);
            for input in ["positive", "negative"] {
                let enc = upstream(&b, k, input);
                assert_eq!(enc["class_type"], "TextEncodeQwenImageEdit");
                assert_eq!(enc["inputs"]["image"], latent["inputs"]["pixels"]);
            }
            assert_eq!(upstream(&b, k, "model")["class_type"], "CFGNorm");
            assert!(!classes(&b).contains(&"TextEncodeQwenImageEditPlus".into()));
            closed(&b.graph);
        }
    }

    #[test]
    fn hidream_dev_and_fast_refine_use_the_official_distilled_samplers() {
        for (name, steps, shift) in [("hidream_i1_dev_fp8.safetensors", 28, 6.0), ("hidream_i1_fast_fp8.safetensors", 16, 3.0)] {
            let f = registry().family("hidream").unwrap().resolved_for_file(name, None);
            let mut p = Params::create("keep the composition", 1024, 768, 3, f.sampling.steps.default as u32, f.sampling.cfg.default);
            p.task = Task::Refine { denoise: 0.4 };
            let b = build(&f, &files(), &p);
            let k = node(&b, "KSampler");
            assert_eq!(k["inputs"]["cfg"], 1.0);
            assert_eq!(k["inputs"]["steps"], steps);
            assert_eq!(k["inputs"]["sampler_name"], "lcm");
            assert_eq!(k["inputs"]["scheduler"], "normal");
            assert_eq!(node(&b, "ModelSamplingSD3")["inputs"]["shift"], shift);
            assert_eq!(upstream(&b, k, "latent_image")["class_type"], "VAEEncode");
            closed(&b.graph);
        }
    }

    #[test]
    fn z_image_base_refine_uses_the_official_cfg_and_negative_text() {
        let f = registry().family("z-image").unwrap().resolved_for_file("z_image_bf16.safetensors", None);
        let mut p = Params::create("keep the composition", 1024, 768, 3, f.sampling.steps.default as u32, f.sampling.cfg.default);
        p.task = Task::Refine { denoise: 0.4 };
        let b = build(&f, &files(), &p);
        let k = node(&b, "KSampler");
        assert_eq!(k["inputs"]["steps"], 25);
        assert_eq!(k["inputs"]["cfg"], 4.0);
        assert_eq!(k["inputs"]["sampler_name"], "res_multistep");
        assert_eq!(upstream(&b, k, "negative")["class_type"], "CLIPTextEncode");
        assert_eq!(upstream(&b, k, "latent_image")["class_type"], "VAEEncode");
        assert_eq!(registry().family("z-image").unwrap().resolved_for_file("z_image_turbo_bf16.safetensors", None).sampling.cfg.default, 1.0);
        closed(&b.graph);
    }

    #[test]
    fn flux1_schnell_refine_keeps_its_official_four_step_path() {
        let f = registry().family("flux1").unwrap().resolved_for_file("flux1-schnell.safetensors", None);
        let mut p = Params::create("keep the composition", 1024, 768, 3, f.sampling.steps.default as u32, f.sampling.cfg.default);
        p.task = Task::Refine { denoise: 0.4 };
        let b = build(&f, &files(), &p);
        let k = node(&b, "KSampler");
        assert_eq!(k["inputs"]["steps"], 4);
        assert_eq!(k["inputs"]["cfg"], 1.0);
        assert_eq!(k["inputs"]["sampler_name"], "euler");
        assert_eq!(upstream(&b, k, "latent_image")["class_type"], "VAEEncode");
        assert!(!classes(&b).contains(&"FluxGuidance".into()));
        closed(&b.graph);
    }

    #[test]
    fn flux2_klein_base_refine_keeps_the_official_cfg_negative_encoding() {
        for name in ["flux-2-klein-base-4b.safetensors", "flux-2-klein-base-9b.safetensors"] {
            let f = registry().family("flux2-klein").unwrap().resolved_for_file(name, None);
            let mut p = Params::create("keep the composition", 1024, 768, 3, f.sampling.steps.default as u32, f.sampling.cfg.default);
            p.task = Task::Refine { denoise: 1.0 };
            let b = build(&f, &files(), &p);
            assert_eq!(node(&b, "Flux2Scheduler")["inputs"]["steps"], 20);
            let guider = node(&b, "CFGGuider");
            assert_eq!(guider["inputs"]["cfg"], 5.0);
            let neg_ref = upstream(&b, guider, "negative");
            assert_eq!(neg_ref["class_type"], "ReferenceLatent");
            let neg = upstream(&b, neg_ref, "conditioning");
            assert_eq!(neg["class_type"], "CLIPTextEncode");
            assert_eq!(neg["inputs"]["text"], "");
            assert_eq!(upstream(&b, node(&b, "SamplerCustomAdvanced"), "latent_image")["class_type"], "EmptyFlux2LatentImage");
            closed(&b.graph);
        }
        // A curated base tool with CFG > 1 must also use blank text rather than zeroed tokens.
        let (family, model) = registry().model("klein-remove").unwrap();
        let f = family.resolved_for(model);
        let mut p = Params::create("keep the composition", 1024, 768, 3, 28, 4.0);
        p.task = Task::Refine { denoise: 1.0 };
        assert!(!classes(&build(&f, &files(), &p)).contains(&"ConditioningZeroOut".into()));
    }

    /// FLUX.2 Klein refines like Comfy-Org's image_flux2_klein_image_edit templates: the source, scaled to ~1 MP, is a
    /// ReferenceLatent on both conditionings; at full denoise the sampler starts from an empty latent with the full
    /// Flux2Scheduler schedule; below it, from the encoded source with split sigmas.
    #[test]
    fn flux2_klein_refines_by_reference_like_the_official_template() {
        let f = registry().family("flux2-klein").unwrap();
        let count = |b: &Built, c: &str| classes(b).iter().filter(|x| *x == c).count();
        let mut p = Params::create("more realistic textures", 1360, 768, 3, 4, 1.0);
        p.loras = &[];
        p.task = Task::Refine { denoise: 1.0 };
        let b = build(f, &files(), &p);
        closed(&b.graph);
        assert_eq!(count(&b, "ReferenceLatent"), 2, "source on positive and negative");
        assert_eq!(count(&b, "ImageScaleToTotalPixels"), 1);
        assert_eq!(count(&b, "EmptyFlux2LatentImage"), 1, "official: empty latent at full denoise");
        assert_eq!(count(&b, "SplitSigmasDenoise"), 0);
        p.task = Task::Refine { denoise: 0.4 };
        let b = build(f, &files(), &p);
        closed(&b.graph);
        assert_eq!(count(&b, "ReferenceLatent"), 2);
        assert_eq!(count(&b, "EmptyFlux2LatentImage"), 0);
        assert_eq!(count(&b, "SplitSigmasDenoise"), 1, "partial: encoded source, split schedule");
        assert_eq!(crate::catalog::refine_default_denoise(crate::ModelId::Klein9B), 1.0);
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

#[cfg(test)]
#[test]
fn ds_finding_flux_refine_batch_is_not_squared() {
    let family = crate::family::registry().family("flux2-klein").unwrap();
    for task in [Task::Create, Task::Refine { denoise: 1.0 }, Task::Refine { denoise: 0.4 }] {
        let mut p = Params::create("a cat", 128, 128, 3, 4, 1.0);
        p.batch = 3;
        p.task = task;
        let built = build(family, &Files::default(), &p);
        let nodes: Vec<_> = built.graph.as_object().unwrap().values().collect();
        let empty = nodes.iter().find(|n| n["class_type"] == "EmptyFlux2LatentImage");
        if let Some(empty) = empty {
            assert_eq!(empty["inputs"]["batch_size"], if task == Task::Create { 3 } else { 1 });
        }
        let repeat: Vec<_> = nodes.iter().filter(|n| n["class_type"] == "RepeatLatentBatch").collect();
        assert_eq!(repeat.len(), usize::from(task != Task::Create));
        if let Some(repeat) = repeat.first() {
            assert_eq!(repeat["inputs"]["amount"], 3);
        }
    }
}

#[cfg(test)]
#[test]
fn ds2_variety_nodes_are_in_capability_requirements() {
    let (family, model) = crate::family::registry().model("z-image-turbo").unwrap();
    let requirements = required_nodes(&family.resolved_for(model));
    for node in ["CLIPTextEncode", "ConditioningAverage", "ConditioningZeroOut"] {
        assert!(requirements.contains(&node.into()), "{node}: {requirements:?}");
    }
}
