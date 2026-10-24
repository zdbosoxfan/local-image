//! Deterministic refine discovery and preservation checks, on worker threads only.
//! ComfyUI's CheckpointLoaderSimple detects checkpoint architectures; split weights require
//! a family or an official template.
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail};
use image::{Rgb, RgbImage, RgbaImage};
use lightcraft_sysmem::jobs::Cancelled;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::builders::{self, Params, Task};
use crate::catalog::{self, InstalledModel, ModelId};
use crate::comfy::{Busy, ComfyClient, JobControl, ObjectInfo, normalized_name};
use crate::custom::{self, CustomWorkflow, Field, FieldKind};
use crate::family::{Family, Loader, Range, RefMethod, Role};
use crate::workflows::{Files, LoraUse};

pub const PROMPT: &str = "A simple landscape painting: blue sky, a yellow sun in the upper right, green rolling hills, a red house with a dark roof on the left and a blue pond on the right. Preserve the image composition, shapes and colors exactly; refine only subtle surface detail.";
pub const PASS_LIMIT: f64 = 35.0;

/// Shared with the real refine checker. Large regions expose lost color and composition.
pub fn test_image() -> RgbImage {
    RgbImage::from_fn(512, 288, |x, y| {
        let (u, v) = (x as f64 / 512.0, y as f64 / 288.0);
        let hill = 0.55 + 0.06 * (u * 9.0).sin();
        Rgb(if (u - 0.78).powi(2) + (v - 0.23).powi(2) < 0.075_f64.powi(2) {
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
        })
    })
}

pub fn mean_abs_difference(a: &RgbImage, b: &RgbImage) -> f64 {
    let a = li_raster::resize_rgb8(a, 64, 36, li_raster::Filter::Triangle);
    let b = li_raster::resize_rgb8(b, 64, 36, li_raster::Filter::Triangle);
    a.as_raw().iter().zip(b.as_raw()).map(|(a, b)| f64::from(a.abs_diff(*b))).sum::<f64>() / (64 * 36 * 3) as f64
}

static PREFERENCES: Mutex<BTreeMap<String, (Defaults, bool)>> = Mutex::new(BTreeMap::new());

pub fn model_defaults(model: ModelId) -> Option<Defaults> {
    PREFERENCES.lock().ok().and_then(|p| p.get(model.key()).map(|(d, _)| d.clone()))
}

// Published with the verified recipe, including explicit unsupported capabilities.
static LORA_PROFILES: Mutex<BTreeMap<(String, String), Option<Family>>> = Mutex::new(BTreeMap::new());

/// Nonblocking picker capability. Before discovery, the family is a provisional recommendation;
/// the worker validates the actual recipe before submitting either generation pass.
pub fn lora_profile_for(model: ModelId, variant: &str) -> Option<Family> {
    if let Some(profile) = LORA_PROFILES.lock().ok().and_then(|p| p.get(&(model.key().into(), variant.into())).cloned()) {
        return profile;
    }
    model.try_info().map(|i| i.resolved.clone()).filter(|f| !f.lora.loader.is_empty())
}

pub fn model_is_reference(model: ModelId) -> Option<bool> {
    PREFERENCES.lock().ok().and_then(|p| p.get(model.key()).map(|(_, reference)| *reference))
}

pub fn unknown_profile_for(key: &str) -> Family {
    let mut profile = unknown_profile();
    if let Some((d, reference)) = PREFERENCES.lock().ok().and_then(|p| p.get(key).cloned()) {
        profile.sampling.steps.default = d.steps as f32;
        profile.sampling.cfg.default = d.cfg;
        profile.sampling.denoise = d.denoise;
        profile.sizes.multiple = d.grid;
        if reference {
            profile.pipeline.reference = RefMethod::ReferenceLatent;
        }
    }
    profile
}

/// Unknown files remain visible in the refine picker, without claiming create/edit support.
pub fn unknown_profile() -> Family {
    let mut f = Family { id: "unknown".into(), label: "Auto-detect".into(), group: "Installed".into(), ..Default::default() };
    f.sampling.steps = Range { default: 20.0, min: 1.0, max: 10000.0 };
    f.sampling.cfg = Range { default: 8.0, min: 0.0, max: 100.0 };
    f.sampling.denoise = 0.35;
    f
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Defaults {
    pub steps: u32,
    pub cfg: f32,
    pub denoise: f32,
    pub sampler: String,
    pub scheduler: String,
    pub grid: u32,
    #[serde(default)]
    pub resolution: Option<(u32, u32)>,
    #[serde(default)]
    pub encoder_layer: Option<i32>,
}

/// The successful recipe is saved with the verdict, so offline reuse needs no discovery.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Recipe {
    Family {
        profile: Box<Family>,
        files: Files,
        denoise: f32,
    },
    Template {
        workflow: CustomWorkflow,
        defaults: Defaults,
    },
    Universal {
        checkpoint: String,
        defaults: Defaults,
    },
    /// A model set up from its official templates (refine is derived from its create graph).
    Compiled {
        recipe: Box<crate::template::TemplateRecipe>,
    },
}

impl Recipe {
    pub fn label(&self) -> String {
        match self {
            Self::Family { profile, .. } => format!("family:{}", profile.id),
            Self::Template { workflow, .. } => format!("template:{}", workflow.name),
            Self::Universal { .. } => "universal".into(),
            Self::Compiled { recipe } => format!("template:{}", recipe.sources.first().map(|s| s.name.as_str()).unwrap_or("")),
        }
    }
    pub fn is_reference(&self) -> bool {
        match self {
            Self::Family { profile, .. } => matches!(profile.pipeline.reference, RefMethod::ReferenceLatent | RefMethod::TextEncoder),
            Self::Template { workflow, .. } => workflow.graph.as_object().is_some_and(|g| {
                g.values().any(|n| {
                    n["class_type"] == "ReferenceLatent"
                        || n["class_type"].as_str().is_some_and(|c| c.starts_with("TextEncode") && (c.contains("Edit") || c.contains("QwenImage21")))
                })
            }),
            Self::Universal { .. } => false,
            Self::Compiled { recipe } => recipe.create.is_none(),
        }
    }
    pub fn defaults(&self) -> Defaults {
        match self {
            Self::Family { profile, denoise, .. } => Defaults {
                steps: profile.sampling.steps.default as u32,
                cfg: profile.sampling.cfg.default,
                denoise: *denoise,
                sampler: profile.pipeline.sampler_name.clone(),
                scheduler: profile.pipeline.scheduler.clone(),
                grid: profile.sizes.multiple.max(8),
                resolution: Some((profile.sizes.native, profile.sizes.native)),
                encoder_layer: None,
            },
            Self::Template { defaults, .. } | Self::Universal { defaults, .. } => defaults.clone(),
            Self::Compiled { recipe } => {
                let d = &recipe.defaults;
                Defaults {
                    steps: d.steps,
                    cfg: d.cfg,
                    denoise: d.denoise,
                    sampler: d.sampler.clone(),
                    scheduler: d.scheduler.clone(),
                    grid: d.grid.max(8),
                    resolution: Some((d.width, d.height)),
                    encoder_layer: None,
                }
            }
        }
    }
    /// A recipe must explicitly support styles; nominal model families are insufficient.
    pub fn lora_profile(&self) -> Option<&Family> {
        match self {
            Self::Family { profile, .. } if !profile.lora.loader.is_empty() => Some(profile),
            _ => None,
        }
    }
    pub fn validate_loras(&self, loras: &[LoraUse]) -> Result<()> {
        if loras.is_empty() {
            return Ok(());
        }
        let profile = self
            .lora_profile()
            .context("This refinement recipe does not support styles (LoRAs). Remove the selected styles or choose another refinement model.")?;
        for lora in loras {
            let family = crate::inventory::loras()
                .into_iter()
                .find(|i| i.name == lora.name)
                .and_then(|i| i.family)
                .or_else(|| crate::arch::guess_from_name(&lora.name, crate::arch::FileKind::Lora));
            if family.is_some_and(|f| !profile.lora_fits(&f)) {
                bail!("The selected style is incompatible with this refinement model: {}", lora.name);
            }
        }
        Ok(())
    }
    /// The graph for `p`; a compiled recipe also needs the server's node list (`info`).
    pub fn graph_for(&self, p: &Params, info: Option<&ObjectInfo>) -> Result<(Value, Vec<String>)> {
        match (self, info) {
            (Self::Compiled { recipe }, Some(info)) => {
                self.validate_loras(p.loras)?;
                let b = recipe.build(p, info)?;
                Ok((b.graph, b.image_nodes))
            }
            _ => self.graph(p),
        }
    }
    pub fn graph(&self, p: &Params) -> Result<(Value, Vec<String>)> {
        self.validate_loras(p.loras)?;
        match self {
            Self::Compiled { .. } => bail!("A template model's graph needs ComfyUI's node list."),
            Self::Family { profile, files, .. } => {
                let b = builders::build(profile, files, p);
                Ok((b.graph, b.image_nodes))
            }
            Self::Template { workflow, .. } => {
                let denoise = match p.task {
                    Task::Refine { denoise } => Some(denoise),
                    _ => None,
                };
                let (mut g, slots) = custom::prepare(
                    workflow,
                    &custom::Inputs {
                        prompt: p.prompt.into(),
                        negative: p.negative.into(),
                        seed: p.seed,
                        steps: Some(p.steps),
                        cfg: Some(p.cfg),
                        denoise,
                        width: Some(p.width),
                        height: Some(p.height),
                    },
                );
                if let Some((sampler, scheduler)) = p.sampler
                    && let Some(nodes) = g.as_object_mut()
                {
                    for node in nodes.values_mut() {
                        match node["class_type"].as_str() {
                            Some("KSampler" | "KSamplerAdvanced") => {
                                node["inputs"]["sampler_name"] = json!(sampler);
                                node["inputs"]["scheduler"] = json!(scheduler);
                            }
                            Some("KSamplerSelect") => node["inputs"]["sampler_name"] = json!(sampler),
                            Some("BasicScheduler") => node["inputs"]["scheduler"] = json!(scheduler),
                            _ => {}
                        }
                    }
                }
                // Full-denoise reference templates start from an empty latent. At partial
                // denoise, ComfyUI img2img starts from the source's encoded latent instead.
                if denoise.is_some_and(|d| d < 1.0) {
                    let nodes = g.as_object().context("template graph is not an object")?.clone();
                    let vae = nodes.values().find(|n| n["class_type"] == "VAEDecode").and_then(|n| n["inputs"].get("vae")).cloned();
                    let image = slots.iter().find(|(k, _)| *k == FieldKind::Image(0)).map(|(_, n)| n.clone());
                    if let (Some(vae), Some(image)) = (vae, image) {
                        for (id, n) in &nodes {
                            if !matches!(n["class_type"].as_str(), Some("KSampler" | "SamplerCustomAdvanced" | "SamplerCustom")) {
                                continue;
                            }
                            let latent = &n["inputs"]["latent_image"];
                            if latent[0]
                                .as_str()
                                .and_then(|id| nodes.get(id))
                                .is_some_and(|n| n["class_type"].as_str().is_some_and(|c| c.starts_with("Empty") && c.contains("Latent")))
                            {
                                let enc = format!("li_partial_encode_{id}");
                                g[&enc] = json!({"class_type":"VAEEncode","inputs":{"pixels":[image,0],"vae":vae}});
                                g[id]["inputs"]["latent_image"] = json!([enc, 0]);
                            }
                        }
                    }
                }
                Ok((g, slots.into_iter().filter_map(|(k, id)| matches!(k, FieldKind::Image(_)).then_some(id)).collect()))
            }
            Self::Universal { checkpoint, defaults } => {
                let denoise = match p.task {
                    Task::Refine { denoise } => denoise,
                    _ => defaults.denoise,
                };
                let (sampler, scheduler) = p.sampler.unwrap_or((&defaults.sampler, &defaults.scheduler));
                let mut graph = json!({
                    "1": {"class_type":"CheckpointLoaderSimple", "inputs":{"ckpt_name":checkpoint}},
                    "2": {"class_type":"LoadImage", "inputs":{"image":""}},
                    "3": {"class_type":"VAEEncode", "inputs":{"pixels":["2",0], "vae":["1",2]}},
                    "4": {"class_type":"CLIPTextEncode", "inputs":{"text":p.prompt,"clip":["1",1]}},
                    "5": {"class_type":"CLIPTextEncode", "inputs":{"text":p.negative,"clip":["1",1]}},
                    "6": {"class_type":"KSampler", "inputs":{"model":["1",0],"positive":["4",0],"negative":["5",0],"latent_image":["3",0],"seed":p.seed,"steps":p.steps,"cfg":p.cfg,"sampler_name":sampler,"scheduler":scheduler,"denoise":denoise}},
                    "7": {"class_type":"VAEDecode", "inputs":{"samples":["6",0],"vae":["1",2]}},
                    "8": {"class_type":"SaveImage", "inputs":{"images":["7",0],"filename_prefix":"LocalImage-Refine"}}
                });
                if let Some(layer) = defaults.encoder_layer {
                    graph["9"] = json!({"class_type":"CLIPSetLastLayer", "inputs":{"clip":["1",1],"stop_at_clip_layer":-layer.abs()}});
                    graph["4"]["inputs"]["clip"] = json!(["9", 0]);
                    graph["5"]["inputs"]["clip"] = json!(["9", 0]);
                }
                Ok((graph, vec!["2".into()]))
            }
        }
    }
    /// This is also the real application path: upload only the draft/source, snapped to this
    /// recipe's grid, then resize the returned image to the requested output dimensions.
    pub fn run(&self, client: &ComfyClient, source: &RgbaImage, p: &Params, ctl: &JobControl) -> Result<RgbaImage> {
        self.run_with_admission(client, source, p, ctl, false)
    }
    fn run_with_admission(&self, client: &ComfyClient, source: &RgbaImage, p: &Params, ctl: &JobControl, idle_only: bool) -> Result<RgbaImage> {
        ctl.check()?;
        let requested = source.dimensions();
        let defaults = self.defaults();
        let (mut w, mut h) = source.dimensions();
        if let Some((rw, rh)) = defaults.resolution {
            let scale = ((f64::from(rw) * f64::from(rh)) / (f64::from(w) * f64::from(h))).sqrt().min(1.0);
            w = (f64::from(w) * scale) as u32;
            h = (f64::from(h) * scale) as u32;
        }
        let (w, h) = crate::imaging::snap_size(w, h, defaults.grid);
        let source = li_raster::resize_rgba8(&crate::imaging::over_white(source), w, h, li_raster::Filter::Lanczos3);
        let png = crate::imaging::encode_png(&source)?;
        self.validate_loras(p.loras)?;
        let p = Params { width: w, height: h, references: 0, ..p.clone() };
        let info = matches!(self, Self::Compiled { .. }).then(|| client.object_info()).transpose()?;
        let (graph, slots) = self.graph_for(&p, info.as_ref())?;
        let uploads = slots.into_iter().map(|n| (n, png.clone())).collect::<Vec<_>>();
        let bytes = if idle_only { client.run_when_idle(graph, &uploads, ctl)? } else { client.run(graph, &uploads, ctl)? };
        let out = crate::imaging::decode_rgba(&bytes)?;
        Ok(li_raster::resize_rgba8(&out, requested.0, requested.1, li_raster::Filter::Lanczos3))
    }
}

fn node_default(info: &ObjectInfo, key: &str) -> Option<Value> {
    let spec = &info.0["KSampler"]["input"]["required"][key];
    spec[1].get("default").cloned().or_else(|| spec[0].as_array().and_then(|v| v.first()).cloned())
}

/// ComfyUI provides sampler/steps/CFG defaults. Metadata overrides only documented numeric
/// inference parameters; training step counts are deliberately not inference step counts.
pub fn universal(checkpoint: &str, info: &ObjectInfo, metadata: &BTreeMap<String, String>) -> Result<Recipe> {
    for class in ["CheckpointLoaderSimple", "LoadImage", "VAEEncode", "CLIPTextEncode", "KSampler", "VAEDecode", "SaveImage"] {
        if !info.has_node(class) {
            bail!("ComfyUI is missing {class}");
        }
    }
    let steps = node_default(info, "steps").and_then(|v| v.as_u64()).context("KSampler has no steps default")?;
    let cfg = node_default(info, "cfg").and_then(|v| v.as_f64()).context("KSampler has no cfg default")?;
    // KSampler defaults to 1.0 (txt2img); the official img2img tutorial lowers it.
    let denoise = node_default(info, "denoise").and_then(|v| v.as_f64()).unwrap_or(1.0).min(0.35) as f32;
    let choice = |key: &str| -> Result<String> {
        node_default(info, key).and_then(|v| v.as_str().map(str::to_owned)).with_context(|| format!("KSampler has no {key} default"))
    };
    // ModelSpec defines resolution and encoder_layer, not inference steps/CFG/sampler.
    let resolution = metadata.get("modelspec.resolution").or_else(|| metadata.get("ss_resolution")).and_then(|s| {
        let s = s.trim_matches(['(', ')', '[', ']']);
        let (w, h) = s.split_once('x').or_else(|| s.split_once(','))?;
        let (w, h) = (w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?);
        ((8..=8192).contains(&w) && (8..=8192).contains(&h)).then_some((w, h))
    });
    let encoder_layer =
        metadata.get("modelspec.encoder_layer").or_else(|| metadata.get("ss_clip_skip")).and_then(|s| s.parse::<i32>().ok()).filter(|n| (1..=32).contains(n));
    if encoder_layer.is_some() && !info.has_node("CLIPSetLastLayer") {
        bail!("ComfyUI is missing CLIPSetLastLayer");
    }
    Ok(Recipe::Universal {
        checkpoint: checkpoint.into(),
        defaults: Defaults {
            steps: steps.clamp(1, 10000) as u32,
            cfg: cfg.clamp(0.0, 100.0) as f32,
            denoise,
            sampler: choice("sampler_name")?,
            scheduler: choice("scheduler")?,
            grid: 8,
            resolution,
            encoder_layer,
        },
    })
}

/// Exact normalized basename, plus all declared size/hash constraints (no substring guessing).
pub fn template_matches(template: &Value, name: &str, bytes: u64, sha256: &str, checkpoint: bool) -> bool {
    let mut found = false;
    let nodes = template["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(template["definitions"]["subgraphs"].as_array().into_iter().flatten().flat_map(|g| g["nodes"].as_array().into_iter().flatten()));
    for model in nodes.flat_map(|n| n["properties"]["models"].as_array().into_iter().flatten()) {
        let folder = model["directory"].as_str().unwrap_or("");
        let role_matches = if checkpoint { folder == "checkpoints" } else { matches!(folder, "diffusion_models" | "unet" | "unet_gguf") };
        if !role_matches || !model["name"].as_str().is_some_and(|n| normalized_name(n) == normalized_name(name)) {
            continue;
        }
        found = true;
        // A declared constraint must be satisfied, including malformed/unsupported hashes.
        // Matching declarations do not require a download URL, and none can hide a stricter one.
        for key in ["size", "bytes"] {
            if let Some(size) = model.get(key)
                && size.as_u64().or_else(|| size.as_str().and_then(|s| s.parse().ok())) != Some(bytes)
            {
                return false;
            }
        }
        if let Some(hash) = model.get("hash")
            && (!model.get("hash_type").is_none_or(|t| t.as_str().is_some_and(|t| t.eq_ignore_ascii_case("sha256")))
                || !hash.as_str().is_some_and(|h| h.eq_ignore_ascii_case(sha256)))
        {
            return false;
        }
    }
    found
}

fn mark(w: &mut CustomWorkflow, kind: FieldKind, node: &str, input: &str) {
    if let Some(field) = w.fields.iter_mut().find(|f| f.node == node && f.input == input) {
        field.default = w.graph[node]["inputs"][input].clone();
    } else {
        w.fields.push(Field { kind, node: node.into(), input: input.into(), default: w.graph[node]["inputs"][input].clone() });
    }
}

fn reachable(graph: &serde_json::Map<String, Value>, start: &str) -> std::collections::BTreeSet<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut todo = vec![start.to_owned()];
    while let Some(id) = todo.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(inputs) = graph.get(&id).and_then(|n| n["inputs"].as_object()) {
            for input in inputs.values() {
                if let Some(link) = input.as_array()
                    && link.len() == 2
                    && link[1].is_number()
                    && let Some(id) = link[0].as_str()
                {
                    todo.push(id.into());
                }
            }
        }
    }
    seen
}

/// Templates can compare several models. Keep the output branch using these exact weights,
/// rather than loading unrelated models or sending the wrong output to the preservation metric.
fn select_template_output(w: &mut CustomWorkflow, installed: &InstalledModel) -> Result<()> {
    let (_, main) = main_file(installed)?;
    let graph = w.graph.as_object().context("template graph is not an object")?;
    let mains: Vec<_> = graph
        .iter()
        .filter(|(_, n)| {
            let input = match n["class_type"].as_str() {
                Some("CheckpointLoaderSimple") => "ckpt_name",
                Some("UNETLoader" | "UnetLoaderGGUF") => "unet_name",
                _ => return false,
            };
            n["inputs"][input].as_str().is_some_and(|file| normalized_name(file) == normalized_name(main))
        })
        .map(|(id, _)| id.clone())
        .collect();
    if mains.is_empty() {
        bail!("Template has no loader for the declared model file");
    }
    let branch = graph
        .iter()
        .filter(|(_, n)| matches!(n["class_type"].as_str(), Some("SaveImage" | "PreviewImage")))
        .map(|(id, _)| reachable(graph, id))
        .find(|ids| mains.iter().any(|id| ids.contains(id)))
        .context("Template has no image output using these weights")?;
    w.graph.as_object_mut().context("missing graph")?.retain(|id, _| branch.contains(id));
    w.fields.retain(|f| branch.contains(&f.node));
    Ok(())
}

/// Official comparison templates sometimes bypass the variant we have installed. Activate
/// its model/subgraph instance and downstream image outputs, retaining optional adapter bypasses.
fn activate_template_branch(template: &Value, installed: &InstalledModel) -> Value {
    if !template.get("nodes").is_some_and(Value::is_array) {
        return template.clone();
    }
    let Ok((role, main)) = main_file(installed) else { return template.clone() };
    let defs: BTreeMap<String, &Value> =
        template["definitions"]["subgraphs"].as_array().into_iter().flatten().filter_map(|g| Some((g["id"].as_str()?.into(), g))).collect();
    fn declares(node: &Value, defs: &BTreeMap<String, &Value>, main: &str, role: Role, depth: usize) -> bool {
        if depth > 16 {
            return false;
        }
        let direct = node["properties"]["models"].as_array().into_iter().flatten().any(|m| {
            let folder = m["directory"].as_str().unwrap_or("");
            let fits = if role == Role::Checkpoint { folder == "checkpoints" } else { matches!(folder, "diffusion_models" | "unet" | "unet_gguf") };
            fits && m["name"].as_str().is_some_and(|n| normalized_name(n) == normalized_name(main))
        });
        direct
            || node["type"]
                .as_str()
                .and_then(|c| defs.get(c))
                .is_some_and(|g| g["nodes"].as_array().into_iter().flatten().any(|n| declares(n, defs, main, role, depth + 1)))
    }
    let mut active: std::collections::BTreeSet<i64> =
        template["nodes"].as_array().into_iter().flatten().filter(|n| declares(n, &defs, main, role, 0)).filter_map(|n| n["id"].as_i64()).collect();
    let seeds = active.clone();
    let links: Vec<(i64, i64)> = template["links"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|link| {
            if let Some(a) = link.as_array() {
                Some((a.get(1)?.as_i64()?, a.get(3)?.as_i64()?))
            } else {
                Some((link["origin_id"].as_i64()?, link["target_id"].as_i64()?))
            }
        })
        .collect();
    loop {
        let before = active.len();
        for (from, to) in &links {
            if active.contains(from) {
                active.insert(*to);
            }
        }
        if before == active.len() {
            break;
        }
    }
    let mut template = template.clone();
    if let Some(nodes) = template["nodes"].as_array_mut() {
        for n in nodes {
            if n["id"]
                .as_i64()
                .is_some_and(|id| seeds.contains(&id) || (active.contains(&id) && matches!(n["type"].as_str(), Some("SaveImage" | "PreviewImage"))))
            {
                n["mode"] = json!(0);
            }
        }
    }
    template
}

/// Retain the template's loaders and sampling/conditioning. A text-only KSampler template
/// becomes img2img by replacing its empty latent with VAEEncode using its actual decoder VAE.
pub fn template_recipe(name: &str, template: &Value, info: &ObjectInfo, installed: &InstalledModel) -> Result<Recipe> {
    let template = activate_template_branch(template, installed);
    let mut w = custom::read(name, &template, Some(info))?;
    select_template_output(&mut w, installed)?;
    let grid = if w.graph.as_object().is_some_and(|g| g.values().any(|n| n["class_type"] == "TextEncodeQwenImage21")) {
        32
    } else if w.graph.as_object().is_some_and(|g| g.values().any(|n| n["class_type"] == "EmptyFlux2LatentImage" || n["class_type"] == "EmptySD3LatentImage")) {
        16
    } else {
        8
    };
    if w.fields.is_empty() {
        w.fields = custom::basic_fields(&w.graph);
    }
    let g = w.graph.as_object_mut().context("template is not an API graph")?;
    // Bind ALL declared loader choices to the installed inventory, preserving subfolders.
    for n in g.values_mut() {
        let class = n["class_type"].as_str().unwrap_or("").to_owned();
        let mut installed_main = false;
        if !info.has_node(&class) {
            bail!("Template {name} needs {class}");
        }
        let Some(inputs) = n["inputs"].as_object_mut() else { continue };
        for (key, val) in inputs {
            if key.ends_with("_name") || key.starts_with("clip_name") {
                let Some(file) = val.as_str() else { continue };
                let choices = info.choices(&class, key);
                if choices.is_empty() {
                    continue;
                }
                let main = match (class.as_str(), key.as_str()) {
                    ("CheckpointLoaderSimple", "ckpt_name") => installed.files.get(&Role::Checkpoint),
                    ("UNETLoader" | "UnetLoaderGGUF", "unet_name") => installed.files.get(&Role::Unet),
                    _ => None,
                };
                let main = main.filter(|main| normalized_name(main) == normalized_name(file));
                installed_main |= main.is_some();
                let target = main.map_or(file, String::as_str);
                *val = json!(info.find_file(&class, key, target).with_context(|| format!("Template {name} needs installed {target}"))?);
            }
        }
        if installed_main {
            if !n.get("_meta").is_some_and(Value::is_object) {
                n["_meta"] = json!({});
            }
            n["_meta"]["li_refine_main"] = json!(true);
        }
    }
    let added_source = !w.has(&FieldKind::Image(0));
    if added_source {
        let g = w.graph.as_object().context("missing graph")?;
        let sampler = g
            .iter()
            .find(|(_, n)| matches!(n["class_type"].as_str(), Some("KSampler" | "SamplerCustomAdvanced" | "SamplerCustom")))
            .map(|(id, _)| id.clone())
            .context("Text-only template has no img2img sampler")?;
        let vae = g.values().find(|n| n["class_type"] == "VAEDecode").and_then(|n| n["inputs"].get("vae")).cloned().context("Template has no decoder VAE")?;
        let old = w.graph[&sampler]["inputs"]["latent_image"].clone();
        let old_node = old[0].as_str().and_then(|id| w.graph.get(id)).context("Missing initial latent")?;
        if !old_node["class_type"].as_str().is_some_and(|c| c.starts_with("Empty") && c.contains("Latent")) {
            bail!("Template initial latent isn't an empty image latent");
        }
        // Unique names cannot collide with numeric IDs from UI templates.
        w.graph["li_refine_image"] = json!({"class_type":"LoadImage","inputs":{"image":""}});
        w.graph["li_refine_encode"] = json!({"class_type":"VAEEncode","inputs":{"pixels":["li_refine_image",0],"vae":vae}});
        w.graph[&sampler]["inputs"]["latent_image"] = json!(["li_refine_encode", 0]);
        if w.graph[&sampler]["class_type"] == "KSampler" {
            w.graph[&sampler]["inputs"]["denoise"] = json!(0.35);
        }
        mark(&mut w, FieldKind::Image(0), "li_refine_image", "image");
    }
    // Single-source self-tests must not invent masks or unrelated reference images.
    if w.has(&FieldKind::Mask) || w.fields.iter().any(|f| matches!(f.kind, FieldKind::Image(i) if i > 0)) {
        bail!("Template requires additional images or a mask");
    }
    select_template_output(&mut w, installed)?;
    let nodes = w.graph.as_object().context("missing graph")?.clone();
    for (id, n) in &nodes {
        if matches!(n["class_type"].as_str(), Some("SamplerCustomAdvanced" | "SamplerCustom")) {
            let sigmas = n["inputs"]["sigmas"].clone();
            let has_denoise = nodes.values().any(|n| n["inputs"]["denoise"].is_number());
            if sigmas.is_array() && !has_denoise && !w.has(&FieldKind::Denoise) {
                if !info.has_node("SplitSigmasDenoise") {
                    bail!("Template needs SplitSigmasDenoise for refinement");
                }
                let split = format!("li_refine_sigmas_{id}");
                w.graph[&split] = json!({"class_type":"SplitSigmasDenoise","inputs":{"sigmas":sigmas,"denoise":if added_source {0.35} else {1.0}}});
                w.graph[id]["inputs"]["sigmas"] = json!([split, 1]);
                mark(&mut w, FieldKind::Denoise, &split, "denoise");
            }
        }
    }
    let nodes = w.graph.as_object().context("missing graph")?.clone();
    for (id, n) in &nodes {
        for (key, kind) in
            [("steps", FieldKind::Steps), ("cfg", FieldKind::Cfg), ("denoise", FieldKind::Denoise), ("width", FieldKind::Width), ("height", FieldKind::Height)]
        {
            if n["inputs"][key].is_number() {
                if added_source && key == "denoise" {
                    w.graph[id]["inputs"][key] = json!(0.35);
                }
                mark(&mut w, kind, id, key);
            }
        }
    }
    if !w.has(&FieldKind::Denoise) {
        bail!("Template has no supported denoise control");
    }
    if !w.has(&FieldKind::Image(0)) || !w.has(&FieldKind::Prompt) {
        bail!("Template has no source/prompt controls");
    }
    let field_num = |kind: FieldKind| w.fields.iter().find(|f| f.kind == kind).and_then(|f| f.default.as_f64());
    let reference = nodes
        .values()
        .any(|n| n["class_type"] == "ReferenceLatent" || n["class_type"].as_str().is_some_and(|c| c.starts_with("TextEncode") && c.contains("Edit")));
    let defaults = Defaults {
        steps: field_num(FieldKind::Steps).unwrap_or(20.0) as u32,
        cfg: field_num(FieldKind::Cfg).unwrap_or(1.0) as f32,
        denoise: field_num(FieldKind::Denoise).unwrap_or(if reference { 1.0 } else { 0.35 }) as f32,
        sampler: nodes.values().find_map(|n| n["inputs"]["sampler_name"].as_str()).unwrap_or("").into(),
        scheduler: nodes.values().find_map(|n| n["inputs"]["scheduler"].as_str()).unwrap_or("").into(),
        // Core VAEEncode crops to eight; image-aware encoders can enforce their own larger grid.
        grid,
        resolution: None,
        encoder_layer: None,
    };
    Ok(Recipe::Template { workflow: w, defaults })
}

pub fn queue_idle(client: &ComfyClient) -> Result<bool> {
    let q = client.queue_state()?;
    for key in ["queue_running", "queue_pending"] {
        let items = q[key].as_array().with_context(|| format!("Invalid /queue: missing {key}"))?;
        if !items.is_empty() {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Known recipes always precede edit/img2img templates, then the checkpoint fallback.
pub fn fallback_order(family: Option<Recipe>, templates: Vec<Recipe>, universal: Option<Recipe>) -> Vec<Recipe> {
    family.into_iter().chain(templates).chain(universal).collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Verdict {
    Verified { recipe: Box<Recipe>, difference: f64 },
    Failed { reason: String },
}

/// Library AI state. Old/empty state loads without verdicts; transient testing isn't persisted.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiState {
    pub refine_verdicts: BTreeMap<String, Verdict>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    #[default]
    Unchecked,
    Testing,
    Verified,
    Failed(String),
}

// Upload selectors change when this check uploads its own source image. Keep model
// choices and every other schema detail, while excluding those temporary image choices.
fn inventory_signature(info: &ObjectInfo) -> Value {
    let mut signature = info.0.clone();
    if let Some(nodes) = signature.as_object_mut() {
        for node in nodes.values_mut() {
            for group in ["required", "optional"] {
                if let Some(inputs) = node["input"][group].as_object_mut() {
                    for spec in inputs.values_mut() {
                        if spec[1]["image_upload"] == true && spec[0].is_array() {
                            spec[0] = Value::Null;
                        }
                    }
                }
            }
        }
    }
    signature
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: SystemTime,
    #[cfg(unix)]
    change: (u64, u64, i64, i64),
}
impl FileStamp {
    fn read(path: &Path) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified()?,
            #[cfg(unix)]
            change: (metadata.dev(), metadata.ino(), metadata.ctime(), metadata.ctime_nsec()),
        })
    }
}

struct RuntimeEntry {
    status: Status,
    ctl: JobControl,
    next_attempt: Instant,
    signature: Option<(PathBuf, FileStamp)>,
}

type HashCache = BTreeMap<PathBuf, (FileStamp, String)>;

pub struct Checker {
    path: PathBuf,
    state: Mutex<Option<AiState>>,
    hashes: Mutex<HashCache>,
    runtime: Mutex<BTreeMap<String, RuntimeEntry>>,
    session_verdicts: Mutex<BTreeMap<String, Verdict>>,
}

// Across all checkers in this process, including manual commands and background UI requests.
pub(crate) static TEST_JOB: Mutex<()> = Mutex::new(());

impl Checker {
    /// Construction does no disk I/O; first worker use loads the saved state.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            state: Mutex::new(None),
            hashes: Mutex::new(BTreeMap::new()),
            runtime: Mutex::new(BTreeMap::new()),
            session_verdicts: Mutex::new(BTreeMap::new()),
        }
    }
    fn key(client: &ComfyClient, model: ModelId, variant: &str) -> String {
        format!("{}:{}:{variant}", client.host(), model.key())
    }
    pub fn status(&self, client: &ComfyClient, model: ModelId) -> Status {
        self.status_for(client.host(), model)
    }
    /// Cheap UI snapshot; never reads settings, files or the network.
    pub fn status_for(&self, host: &str, model: ModelId) -> Status {
        self.status_for_variant(host, model, catalog::default_variant(model))
    }
    pub fn status_for_variant(&self, host: &str, model: ModelId, variant: &str) -> Status {
        self.runtime.lock().ok().and_then(|r| r.get(&format!("{host}:{}:{variant}", model.key())).map(|e| e.status.clone())).unwrap_or_default()
    }
    pub fn cancel(&self, client: &ComfyClient, model: ModelId) {
        self.cancel_variant(client, model, catalog::default_variant(model));
    }
    pub fn cancel_variant(&self, client: &ComfyClient, model: ModelId, variant: &str) {
        if let Ok(r) = self.runtime.lock()
            && let Some(e) = r.get(&Self::key(client, model, variant))
        {
            e.ctl.cancel();
        }
    }
    /// Called by the existing background inventory poller, never by the UI.
    pub fn invalidate_changed_files(&self) {
        if let Ok(mut r) = self.runtime.lock() {
            for e in r.values_mut() {
                if let Some((p, stamp)) = &e.signature {
                    let changed = FileStamp::read(p).map_or(true, |current| current != *stamp);
                    if changed && e.status != Status::Testing {
                        e.status = Status::Unchecked;
                        e.next_attempt = Instant::now();
                    }
                }
            }
        }
    }
    pub fn hash_file(&self, path: &Path, ctl: &JobControl) -> Result<(String, u64)> {
        ctl.check()?;
        let stamp = FileStamp::read(path)?;
        let len = stamp.len;
        if let Some((cached, hash)) = self.hashes.lock().ok().and_then(|c| c.get(path).cloned())
            && cached == stamp
        {
            return Ok((hash, len));
        }
        let mut file = std::fs::File::open(path)?;
        let mut sha = Sha256::new();
        let mut bytes = vec![0u8; 1 << 20];
        loop {
            ctl.check()?;
            let n = file.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            sha.update(&bytes[..n]);
        }
        if FileStamp::read(path)? != stamp {
            bail!("Model file changed while hashing; retry");
        }
        let hash = hex::encode(sha.finalize());
        if let Ok(mut c) = self.hashes.lock() {
            c.insert(path.into(), (stamp, hash.clone()));
        }
        Ok((hash, len))
    }
    fn saved(&self, hash: &str) -> Option<Verdict> {
        let mut state = self.state.lock().ok()?;
        let state = state.get_or_insert_with(|| std::fs::read(&self.path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default());
        state.refine_verdicts.get(hash).cloned()
    }
    fn store(&self, hash: &str, verdict: &Verdict) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| anyhow::anyhow!("AI state lock poisoned"))?;
        let state = state.get_or_insert_with(|| std::fs::read(&self.path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default());
        state.refine_verdicts.insert(hash.into(), verdict.clone());
        li_fs::atomic_write(&self.path, &serde_json::to_vec_pretty(state)?)?;
        Ok(())
    }
    fn publish(&self, client: &ComfyClient, model: ModelId, variant: &str, verdict: &Verdict) {
        if let Ok(mut profiles) = LORA_PROFILES.lock() {
            match verdict {
                Verdict::Verified { recipe, .. } => {
                    profiles.insert((model.key().into(), variant.into()), recipe.lora_profile().cloned());
                }
                Verdict::Failed { .. } => {
                    profiles.remove(&(model.key().into(), variant.into()));
                }
            }
        }
        let mut changed = false;
        if variant == catalog::default_variant(model)
            && let Ok(mut preferences) = PREFERENCES.lock()
        {
            match verdict {
                Verdict::Verified { recipe, .. } => {
                    let preference = (recipe.defaults(), recipe.is_reference());
                    if preferences.get(model.key()) != Some(&preference) {
                        preferences.insert(model.key().into(), preference);
                        changed = true;
                    }
                }
                Verdict::Failed { .. } => changed = preferences.remove(model.key()).is_some(),
            }
        }
        if changed {
            catalog::invalidate();
        }
        if let Ok(mut r) = self.runtime.lock()
            && let Some(e) = r.get_mut(&Self::key(client, model, variant))
        {
            e.status = match verdict {
                Verdict::Verified { .. } => Status::Verified,
                Verdict::Failed { reason } => Status::Failed(reason.clone()),
            };
        }
    }
    /// UI scheduling is cheap: no hashing, discovery, network or disk on the caller thread.
    /// A failed/busy/offline attempt can be explicitly re-tested; queue contention retries later.
    pub fn request(self: &Arc<Self>, client: ComfyClient, model: ModelId, variant: String, retest: bool) {
        let key = Self::key(&client, model, &variant);
        let ctl = JobControl::new();
        if let Ok(mut r) = self.runtime.lock() {
            if r.values().any(|e| e.status == Status::Testing) {
                return;
            }
            if !retest && r.get(&key).is_some_and(|e| e.status != Status::Unchecked || e.next_attempt > Instant::now()) {
                return;
            }
            r.insert(key.clone(), RuntimeEntry { status: Status::Testing, ctl: ctl.clone(), next_attempt: Instant::now(), signature: None });
        }
        let checker = self.clone();
        let _ = std::thread::Builder::new().name("refine-check".into()).spawn(move || {
            let result = checker.check(&client, model, &variant, retest, &ctl);
            if let Ok(mut r) = checker.runtime.lock()
                && let Some(e) = r.get_mut(&key)
                && e.status == Status::Testing
            {
                e.status = match result {
                    Ok(Some(Verdict::Verified { .. })) => Status::Verified,
                    Ok(Some(Verdict::Failed { reason })) => Status::Failed(reason),
                    _ => Status::Unchecked, // Cancellation/offline/busy is not an unsupported verdict.
                };
                e.next_attempt = Instant::now() + Duration::from_secs(5);
            }
        });
    }
    /// Returns None when busy. Each failed recipe is followed by a fresh queue check. A
    /// cancellation or connection failure never becomes a persisted model failure.
    pub fn check(&self, client: &ComfyClient, model: ModelId, variant: &str, retest: bool, ctl: &JobControl) -> Result<Option<Verdict>> {
        self.check_in(client, model, variant, &crate::AiSettings::load().model_dir(), retest, ctl)
    }
    /// Explicit model root for isolated libraries, tests and the live checker.
    pub fn check_in(&self, client: &ComfyClient, model: ModelId, variant: &str, model_dir: &Path, retest: bool, ctl: &JobControl) -> Result<Option<Verdict>> {
        let Ok(_job) = TEST_JOB.try_lock() else { return Ok(None) };
        if let Ok(mut r) = self.runtime.lock() {
            r.entry(Self::key(client, model, variant)).or_insert_with(|| RuntimeEntry {
                status: Status::Unchecked,
                ctl: ctl.clone(),
                next_attempt: Instant::now(),
                signature: None,
            });
        }
        let result = match self.check_inner(client, model, variant, model_dir, retest, ctl) {
            Err(e) if e.is::<Busy>() => Ok(None),
            result => result,
        };
        if let Ok(mut r) = self.runtime.lock()
            && let Some(e) = r.get_mut(&Self::key(client, model, variant))
        {
            e.status = match &result {
                Ok(Some(Verdict::Verified { .. })) => Status::Verified,
                Ok(Some(Verdict::Failed { reason })) => Status::Failed(reason.clone()),
                _ => Status::Unchecked,
            };
        }
        result
    }
    fn check_inner(&self, client: &ComfyClient, model: ModelId, variant: &str, model_dir: &Path, retest: bool, ctl: &JobControl) -> Result<Option<Verdict>> {
        ctl.check()?;
        let info = client.object_info()?;
        let settings = crate::AiSettings::load();
        // Normalize the initial catalogue before capturing its generation. A concurrent UI
        // refresh of these same loader files must not look like a model replacement.
        crate::inventory::refresh(&info, model_dir.is_dir().then_some(model_dir));
        let generation = catalog::inventory_generation();
        let inventory = inventory_signature(&info);
        let target = target(model, variant, &info)?;
        let (role, name) = main_file(&target)?;
        let folders = if role == Role::Checkpoint { vec!["checkpoints"] } else { vec!["diffusion_models", "unet", "unet_gguf"] };
        let path = crate::inventory::local_path(Some(model_dir), &folders, name)
            .or_else(|| crate::inventory::local_path(Some(&PathBuf::from(&settings.comfy_directory).join("models")), &folders, name));
        let stamp = path.as_ref().map(|path| FileStamp::read(path)).transpose()?;
        let (hash, bytes) = match &path {
            Some(path) => {
                let (h, b) = self.hash_file(path, ctl)?;
                (Some(h), b)
            }
            None => (None, 0),
        };
        let session_key = format!("{}:{name}", Self::key(client, model, variant));
        if let Some(path) = &path
            && let Ok(mut runtime) = self.runtime.lock()
            && let Some(entry) = runtime.get_mut(&Self::key(client, model, variant))
        {
            entry.signature = Some((path.clone(), FileStamp::read(path)?));
        }
        // Recheck publication identity, even after a successful asynchronous prompt. This does
        // not claim to identify a remote loader's cached weights or close an unobserved ABA swap.
        let revalidate = |fresh_hash: bool| -> Result<()> {
            ctl.check()?;
            if catalog::inventory_generation() != generation || inventory_signature(&client.object_info()?) != inventory {
                bail!("Model inventory changed during refinement testing; retry.");
            }
            if let (Some(path), Some(hash)) = (&path, &hash) {
                if Some(FileStamp::read(path)?) != stamp {
                    bail!("Model file changed during refinement testing; retry.");
                }
                if fresh_hash && let Ok(mut hashes) = self.hashes.lock() {
                    hashes.remove(path);
                }
                let (current, _) = self.hash_file(path, ctl)?;
                if current != *hash {
                    bail!("Model file changed during refinement testing; retry.");
                }
            }
            Ok(())
        };
        if !retest {
            let saved = match &hash {
                Some(h) => self.saved(h),
                None => self.session_verdicts.lock().ok().and_then(|v| v.get(&session_key).cloned()),
            };
            if let Some(mut v) = saved {
                if let Verdict::Verified { recipe, .. } = &mut v {
                    match recipe.as_mut() {
                        Recipe::Universal { checkpoint, .. } => *checkpoint = name.into(),
                        Recipe::Compiled { .. } => {}
                        Recipe::Family { files, .. } => {
                            files.insert(role, name.into());
                        }
                        Recipe::Template { workflow, .. } => {
                            if let Some(g) = workflow.graph.as_object_mut() {
                                for n in g.values_mut() {
                                    let input = match n["class_type"].as_str() {
                                        Some("CheckpointLoaderSimple") if role == Role::Checkpoint => Some("ckpt_name"),
                                        Some("UNETLoader" | "UnetLoaderGGUF") if role == Role::Unet => Some("unet_name"),
                                        _ => None,
                                    };
                                    if let Some(input) = input
                                        && (n["_meta"]["li_refine_main"] == true
                                            || n["inputs"][input].as_str().is_some_and(|file| normalized_name(file) == normalized_name(name)))
                                    {
                                        n["inputs"][input] = json!(name);
                                    }
                                }
                            }
                        }
                    }
                }
                revalidate(true)?;
                self.publish(client, model, variant, &v);
                return Ok(Some(v));
            }
        }
        if let Ok(mut r) = self.runtime.lock() {
            let e = r.entry(Self::key(client, model, variant)).or_insert_with(|| RuntimeEntry {
                status: Status::Unchecked,
                ctl: ctl.clone(),
                next_attempt: Instant::now(),
                signature: None,
            });
            e.ctl = ctl.clone();
            e.status = Status::Testing;
        }
        let save = |v: &Verdict| -> Result<()> {
            revalidate(true)?;
            if let Some(h) = &hash {
                self.store(h, v)?;
            } else if let Ok(mut verdicts) = self.session_verdicts.lock() {
                verdicts.insert(session_key.clone(), v.clone());
            }
            self.publish(client, model, variant, v);
            Ok(())
        };
        if !queue_idle(client)? {
            return Ok(None);
        }
        let mut failures = Vec::new();
        let try_one = |recipe: Recipe, failures: &mut Vec<String>| -> Result<Option<Verdict>> {
            ctl.check()?;
            if !queue_idle(client)? {
                return Ok(None);
            }
            revalidate(false)?;
            match self.test_recipe(client, &recipe, ctl) {
                Ok(difference) if difference < PASS_LIMIT => Ok(Some(Verdict::Verified { recipe: Box::new(recipe), difference })),
                Ok(d) => {
                    failures.push(format!("{}: preservation difference {d:.3} >= {PASS_LIMIT}", recipe.label()));
                    Ok(None)
                }
                Err(e) if e.is::<Busy>() => Err(e),
                Err(e) if e.is::<Cancelled>() || ctl.is_cancelled() => Err(anyhow::anyhow!(Cancelled)),
                Err(e) => {
                    // Confirm the server is still reachable before calling this a recipe failure.
                    client.queue_state()?;
                    failures.push(format!("{}: {e:#}", recipe.label()));
                    Ok(None)
                }
            }
        };
        if model.try_info().is_some_and(|m| m.family != "unknown")
            && let Some(preset) = catalog::preset(model, variant)
        {
            let available = catalog::availability(preset, &info);
            if !available.available {
                failures.push(format!("family unavailable: {}", available.reason));
            }
        }
        if let Some(recipe) = family_recipe(model, variant, &target, &info) {
            if let Some(v) = try_one(recipe, &mut failures)? {
                save(&v)?;
                return Ok(Some(v));
            }
            if !queue_idle(client)? {
                return Ok(None);
            }
        }
        if model.try_info().is_some_and(|m| m.recipe.is_some()) {
            let v = Verdict::Failed { reason: failures.join("; ") };
            save(&v)?;
            return Ok(Some(v));
        }
        let mut cfg = crate::browser::Config::from_env(&settings);
        // The connected installation ships version-matched Comfy-Org templates.
        if std::env::var_os("LOCAL_IMAGE_TEMPLATES_BASE").is_none() {
            cfg.templates = format!("http://{}/templates", client.host());
        }
        let cache = crate::browser::Cache::new(crate::browser::Cache::default_dir());
        let net = crate::browser::HttpNet::new(cfg.clone());
        match discover_templates(&cfg, &cache, &net, true, &target, bytes, hash.as_deref().unwrap_or(""), &info, ctl) {
            Ok(recipes) => {
                for recipe in recipes {
                    if let Some(v) = try_one(recipe, &mut failures)? {
                        save(&v)?;
                        return Ok(Some(v));
                    }
                    if !queue_idle(client)? {
                        return Ok(None);
                    }
                }
            }
            Err(_) if ctl.is_cancelled() => return Err(anyhow::anyhow!(Cancelled)),
            Err(e) => failures.push(format!("templates: {e:#}")),
        }
        if role == Role::Checkpoint {
            let metadata = path.as_ref().and_then(|p| crate::arch::read_header(p).ok()).map(|h| h.metadata).unwrap_or_default();
            match universal(name, &info, &metadata) {
                Ok(recipe) => {
                    if let Some(v) = try_one(recipe, &mut failures)? {
                        save(&v)?;
                        return Ok(Some(v));
                    }
                }
                Err(e) => failures.push(format!("universal: {e:#}")),
            }
        } else {
            failures.push("Split diffusion weights need a matching family profile or image template".into());
        }
        if !queue_idle(client)? {
            return Ok(None);
        }
        let v = Verdict::Failed { reason: failures.join("; ") };
        save(&v)?;
        Ok(Some(v))
    }
    pub fn test_recipe(&self, client: &ComfyClient, recipe: &Recipe, ctl: &JobControl) -> Result<f64> {
        preservation(client, recipe, 42, ctl).map(|(d, _)| d)
    }
}

/// Refines the test picture with `recipe` at its default strength (only on an idle queue) and
/// returns how much it changed ([`mean_abs_difference`]; [`PASS_LIMIT`] passes) and the result.
pub fn preservation(client: &ComfyClient, recipe: &Recipe, seed: u64, ctl: &JobControl) -> Result<(f64, RgbaImage)> {
    let source = test_image();
    let src = image::DynamicImage::ImageRgb8(source.clone()).to_rgba8();
    let d = recipe.defaults();
    let p = Params { task: Task::Refine { denoise: d.denoise }, ..Params::create(PROMPT, 512, 288, seed, d.steps, d.cfg) };
    let out = recipe.run_with_admission(client, &src, &p, ctl, true)?;
    Ok((mean_abs_difference(&source, &image::DynamicImage::ImageRgba8(out.clone()).to_rgb8()), out))
}

fn main_file(target: &InstalledModel) -> Result<(Role, &str)> {
    [Role::Checkpoint, Role::Unet].into_iter().find_map(|r| target.files.get(&r).map(|n| (r, n.as_str()))).context("Model has no main weights")
}

pub fn target(model: ModelId, variant: &str, info: &ObjectInfo) -> Result<InstalledModel> {
    let m = model.try_info().context("Model is no longer installed")?;
    let preset = catalog::preset(model, variant).context("Model preset is no longer installed")?;
    let a = catalog::availability(preset, info);
    // A missing companion is a recipe failure, not a reason to hide the installed main file.
    let mut files = a.files;
    for f in &preset.files {
        if matches!(f.role, Role::Checkpoint | Role::Unet) {
            let (class, input) = f.role.loader();
            let found = info
                .find_file(class, input, &f.name)
                .or_else(|| (f.role == Role::Unet).then(|| info.find_file("UnetLoaderGGUF", "unet_name", &f.name)).flatten());
            files.insert(f.role, found.with_context(|| format!("{} is not installed", f.name))?);
        }
    }
    Ok(InstalledModel { key: model.key().into(), family: m.family.clone(), label: m.label.clone(), files, ..Default::default() })
}

pub fn family_recipe(model: ModelId, variant: &str, target: &InstalledModel, info: &ObjectInfo) -> Option<Recipe> {
    let m = model.try_info()?;
    if let Some(recipe) = &m.recipe {
        return Some(Recipe::Compiled { recipe: Box::new((**recipe).clone()) });
    }
    let mut profile = if m.tool {
        crate::family::registry().family(&target.family)?.resolved_for_file(main_file(target).ok()?.1, None)
    } else {
        if m.family == "unknown" {
            return None;
        }
        m.resolved.clone()
    };
    if target.files.contains_key(&Role::Checkpoint) {
        profile.pipeline.loader = Loader::Checkpoint;
    }
    let a = catalog::availability(catalog::preset(model, variant)?, info);
    if !a.available {
        return None;
    }
    let denoise = if matches!(profile.pipeline.reference, RefMethod::ReferenceLatent | RefMethod::TextEncoder) { 1.0 } else { 0.35 };
    Some(Recipe::Family { profile: Box::new(profile), files: target.files.clone(), denoise })
}

pub fn discover_templates(
    cfg: &crate::browser::Config,
    cache: &crate::browser::Cache,
    net: &dyn crate::browser::Net,
    online: bool,
    target: &InstalledModel,
    bytes: u64,
    hash: &str,
    info: &ObjectInfo,
    ctl: &JobControl,
) -> Result<Vec<Recipe>> {
    let index_url = format!("{}/index.json", cfg.templates.trim_end_matches('/'));
    let fresh = cache.get(&index_url).is_some_and(|(_, age)| age < 86400);
    let (index, _) = cache.fetch(net, &index_url, online && !fresh)?;
    let mut entries = crate::browser::parse_template_index(&serde_json::from_slice(&index)?);
    entries.sort_by_key(|e| {
        (
            !e.tags.iter().any(|t| {
                let t = t.to_ascii_lowercase();
                t.contains("edit") || t.contains("image to image") || t.contains("img2img")
            }),
            e.name.clone(),
        )
    });
    let (role, name) = main_file(target)?;
    let mut recipes = Vec::new();
    let mut rejected = Vec::new();
    for e in entries {
        ctl.check()?;
        // Entries are untrusted path components, not URLs or filesystem paths.
        if !e.name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
            continue;
        }
        let url = format!("{}/{}.json", cfg.templates.trim_end_matches('/'), e.name);
        let fresh = cache.get(&url).is_some_and(|(_, age)| age < 86400);
        let Ok((body, _)) = cache.fetch(net, &url, online && !fresh) else { continue };
        let Ok(v) = serde_json::from_slice::<Value>(&body) else { continue };
        if template_matches(&v, name, bytes, hash, role == Role::Checkpoint) {
            match template_recipe(&e.name, &v, info, target) {
                Ok(recipe) => recipes.push(recipe),
                Err(error) => rejected.push(format!("{}: {error:#}", e.name)),
            }
        }
    }
    if recipes.is_empty() && !rejected.is_empty() {
        bail!("Matched templates cannot refine: {}", rejected.join("; "));
    }
    Ok(recipes)
}

pub fn checker() -> &'static Arc<Checker> {
    static CHECKER: OnceLock<Arc<Checker>> = OnceLock::new();
    CHECKER.get_or_init(|| Arc::new(Checker::new(crate::settings::state_dir().join("ai-refine.json"))))
}

/// Called only on an operation worker. Busy checks defer; the user's job remains cancellable.
pub fn ensure(client: &ComfyClient, model: ModelId, variant: &str, ctl: &JobControl) -> Result<Recipe> {
    let start = Instant::now();
    loop {
        ctl.check()?;
        if let Some(v) = checker().check(client, model, variant, false, ctl)? {
            return match v {
                Verdict::Verified { recipe, .. } => Ok(*recipe),
                Verdict::Failed { reason } => bail!("Refine not supported ({reason})"),
            };
        }
        if start.elapsed() > Duration::from_secs(120) {
            bail!("ComfyUI is busy; retry refinement when it is idle");
        }
        ctl.set_message("Testing refine support when ComfyUI is idle…");
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests;
