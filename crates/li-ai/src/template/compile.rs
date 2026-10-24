//! Compiles the matched official templates of an installed file into a [`TemplateRecipe`]: each
//! template is converted and canonicalised, its roles read, its loaders bound to installed files
//! ([`crate::companions`]), and the architecture's profile polish applied (labels, ranges, value
//! fixes). [`family_for`] gives the recipe a family profile, so code that reads
//! `ModelInfo.resolved` keeps working.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::index::TemplateEntry;
use super::matching::{Capability, Match};
use super::roles::{self, NegativeSlot, Slot};
use super::{
    Controls, ImageRole, ImageSlot, InpaintWay, LatentSpec, RefCapacity, Slots, TaskGraph, TemplateDefaults, TemplateRecipe, TemplateSource, UpsamplerInfo,
};
use crate::comfy::ObjectInfo;
use crate::companions::{self, Binding, Runtime, Why};
use crate::family::{Capabilities, Commercial, Engine, Family, FamilyKind, InpaintMethod, Range, RefMethod, Sampling, Sizes};
use crate::graph::{self, ancestors, canon, class_of, consumers, inputs, link_of, ui};
use crate::identify::Identity;

/// A matched template with its body.
pub struct Source<'a> {
    pub entry: &'a TemplateEntry,
    pub body: &'a Value,
    pub matched: &'a Match,
    pub templates_version: &'a str,
}

/// What compilation needs besides the templates.
pub struct Input<'a> {
    pub key: &'a str,
    /// The installed main file, as its loader lists it.
    pub file: &'a str,
    pub identity: &'a Identity,
    pub info: &'a ObjectInfo,
    pub runtime: &'a Runtime,
    /// The architecture's family profile (its `template` polish, labels and licence).
    pub family: Option<&'a Family>,
    pub gpu: Option<&'a str>,
    /// `declared → installed` substitutions the user chose.
    pub user: &'a BTreeMap<String, String>,
    pub vae_channels: &'a dyn Fn(&str) -> Option<u32>,
}

pub use super::Unusable;

fn unusable(template: &str, capability: Capability, detail: String) -> Unusable {
    Unusable { template: template.to_owned(), capability: capability.id().into(), detail, ..Default::default() }
}

/// An unusable template with the classes and options this ComfyUI lacks.
fn unusable_from(template: &str, capability: Capability, problems: &[crate::graph::validate::Problem]) -> Unusable {
    use crate::graph::validate::ProblemKind;
    let mut u = unusable(template, capability, problems.iter().map(|p| p.describe()).collect::<Vec<_>>().join("; "));
    for p in problems {
        match &p.kind {
            ProblemKind::MissingClass if !u.missing_classes.contains(&p.class) => u.missing_classes.push(p.class.clone()),
            ProblemKind::NotOffered(v) => u.not_offered.push((p.class.clone(), p.input.clone(), v.clone())),
            _ => {}
        }
    }
    u
}

fn file_role(input: &str) -> &'static str {
    match input {
        "vae_name" => "vae",
        "lora_name" => "lora",
        i if i.starts_with("clip_name") => "clip",
        "style_model_name" => "style_model",
        "control_net_name" => "controlnet",
        _ => "file",
    }
}

const FILE_INPUTS: &[&str] =
    &["clip_name", "clip_name1", "clip_name2", "clip_name3", "clip_name4", "vae_name", "lora_name", "style_model_name", "control_net_name"];

/// The loader bindings of a graph: the main loader takes the installed file; every other file
/// input is resolved among the installed files.
fn bindings(g: &Map<String, Value>, main_loader: &str, input: &Input, optional: &BTreeSet<String>) -> Vec<Binding> {
    let ctx = companions::Ctx {
        info: input.info,
        runtime: input.runtime,
        gpu: input.gpu,
        main_quant: input.identity.quant.class.clone(),
        user: input.user,
        vae_channels: input.vae_channels,
    };
    let mut out = Vec::new();
    for id in graph::sorted_ids(g) {
        let class = class_of(g, &id).to_owned();
        for (k, v) in inputs(&g[&id]).into_iter().flatten() {
            let Some(declared) = v.as_str() else { continue };
            if id == main_loader && matches!(k.as_str(), "unet_name" | "ckpt_name") {
                out.push(Binding {
                    role: "main".into(),
                    node: id.clone(),
                    input: k.clone(),
                    loader: class.clone(),
                    declared: declared.to_owned(),
                    used: Some(input.file.to_owned()),
                    folder: companions::folder_of(&class, k).to_owned(),
                    why: Why::Exact,
                    ..Default::default()
                });
                continue;
            }
            let file = if k == "name" { declared.to_ascii_lowercase().ends_with(".safetensors") } else { FILE_INPUTS.contains(&k.as_str()) };
            if !file {
                continue;
            }
            let mut b = companions::resolve(&ctx, &class, k, declared);
            b.role = if optional.contains(&id) { "upsampler".into() } else { file_role(k).into() };
            b.node = id.clone();
            b.optional = optional.contains(&id);
            out.push(b);
        }
    }
    out
}

/// The default size behind a latent's width/height source (a `ResolutionSelector` is evaluated).
fn size_of(g: &Map<String, Value>, src: &str) -> (u32, u32) {
    let ins = inputs(&g[src]).cloned().unwrap_or_default();
    if class_of(g, src) != "ResolutionSelector" {
        return (1024, 1024);
    }
    let ratio = ins.get("aspect_ratio").and_then(Value::as_str).unwrap_or("1:1");
    let (w, h) = ratio
        .split_whitespace()
        .next()
        .and_then(|r| r.split_once(':'))
        .and_then(|(a, b)| Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?)))
        .unwrap_or((1.0, 1.0));
    let mp = ins.get("megapixels").and_then(Value::as_f64).unwrap_or(1.0);
    let multiple = ins.get("multiple").and_then(Value::as_f64).unwrap_or(8.0).max(1.0);
    let total = mp * 1024.0 * 1024.0;
    let width = (total * w / h).sqrt();
    let height = width * h / w;
    let snap = |v: f64| ((v / multiple).round() * multiple) as u32;
    (snap(width), snap(height))
}

/// Binds a graph's size inputs: a computed size source becomes literals, and every input that
/// read it (the latent, a sampling shift) becomes a size slot.
fn bind_size(g: &mut Map<String, Value>, latent: &str, sampling: &BTreeSet<String>) -> (Vec<Slot>, Vec<Slot>, (u32, u32), u32) {
    let ins = inputs(&g[latent]).cloned().unwrap_or_default();
    let mut width = vec![(latent.to_owned(), "width".to_owned())];
    let mut height = vec![(latent.to_owned(), "height".to_owned())];
    let mut grid = 0;
    let size = match (ins.get("width").and_then(link_of), ins.get("height").and_then(link_of)) {
        (Some((src, ws)), Some((hsrc, hs))) => {
            let (src, hsrc) = (src.to_owned(), hsrc.to_owned());
            let (dw, dh) = size_of(g, &src);
            if class_of(g, &src) == "ResolutionSelector" {
                grid = inputs(&g[&src]).and_then(|i| i.get("multiple")).and_then(Value::as_u64).unwrap_or(0) as u32;
            }
            width = consumers(g, &src, ws);
            height = consumers(g, &hsrc, hs);
            graph::rewire(g, (&src, ws), &json!(dw), &[]);
            graph::rewire(g, (&hsrc, hs), &json!(dh), &[]);
            (dw, dh)
        }
        _ => {
            let w = ins.get("width").and_then(Value::as_u64).unwrap_or(1024) as u32;
            let h = ins.get("height").and_then(Value::as_u64).unwrap_or(1024) as u32;
            // Sampling inputs sized like the latent (a FLUX shift, a FLUX.2 scheduler) follow it.
            for id in sampling {
                for (k, v) in inputs(&g[id]).into_iter().flatten() {
                    if id != latent && k == "width" && v.as_u64() == Some(u64::from(w)) {
                        width.push((id.clone(), k.clone()));
                    }
                    if id != latent && k == "height" && v.as_u64() == Some(u64::from(h)) {
                        height.push((id.clone(), k.clone()));
                    }
                }
            }
            (w, h)
        }
    };
    (width, height, size, grid)
}

/// The reference consumer reached from the image loaders, and how many images it takes.
fn ref_capacity(g: &Map<String, Value>, info: &ObjectInfo, loaders: &[String]) -> (RefCapacity, Vec<String>) {
    for l in loaders {
        for (consumer, input, chain) in roles::image_destinations(g, info, l) {
            let class = class_of(g, &consumer);
            let top = input.split('.').next().unwrap_or(&input);
            let Some(spec) = info.input_spec(class, top) else { continue };
            if spec.get(0).and_then(Value::as_str) == Some(ui::AUTOGROW) {
                let t = spec.get(1).and_then(|o| o.get("template"));
                let prefix = t.and_then(|t| t.get("prefix")).and_then(Value::as_str).map(str::to_owned);
                let names: Vec<String> = t
                    .and_then(|t| t.get("names"))
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
                    .unwrap_or_default();
                let max = t.and_then(|t| t.get("max")).and_then(Value::as_u64).map(|m| m as usize).unwrap_or(if names.is_empty() { 100 } else { names.len() });
                return (RefCapacity::Autogrow { node: consumer, input: top.to_owned(), prefix, names, max }, chain);
            }
            if spec.get(0).and_then(Value::as_str) == Some("IMAGE") {
                let digits = top.trim_start_matches(|c: char| !c.is_ascii_digit());
                if !digits.is_empty() {
                    let stem = &top[..top.len() - digits.len()];
                    let (req, opt) = info.input_names(class);
                    let inputs: Vec<String> = req
                        .into_iter()
                        .chain(opt)
                        .filter(|n| n.starts_with(stem) && n[stem.len()..].chars().all(|c| c.is_ascii_digit()) && !n[stem.len()..].is_empty())
                        .collect();
                    return (RefCapacity::Fixed { node: consumer, inputs }, chain);
                }
            }
        }
    }
    (RefCapacity::None, Vec::new())
}

/// The roles of one canonical graph.
fn task_graph(template: String, c: &canon::Canonical, main_stem: &str, input: &Input, edit: bool) -> Result<TaskGraph, String> {
    let info = input.info;
    let mut g = c.graph.clone();
    let up = ancestors(&g, &c.output);
    let samplers = roles::samplers(&g, info, &up);
    let last = samplers.last().cloned().ok_or("no sampler")?;
    let sampling = roles::sampling_nodes(&g, &samplers);
    let loader =
        canon::main_loaders(&g, Some(main_stem)).into_iter().next().or_else(|| canon::main_loaders(&g, None).into_iter().next()).ok_or("no model loader")?;
    let mut slots = Slots {
        prompt: c.prompt.clone(),
        negative: c.negative.clone(),
        seeds: roles::seed_slots(&g, info, &sampling),
        steps: roles::typed_inputs(&g, info, &sampling, "steps", "INT"),
        cfg: roles::typed_inputs(&g, info, &sampling, "cfg", "FLOAT"),
        sampler_name: roles::typed_inputs(&g, info, &sampling, "sampler_name", "COMBO"),
        scheduler: roles::typed_inputs(&g, info, &sampling, "scheduler", "COMBO"),
        denoise: roles::typed_inputs(&g, info, &sampling, "denoise", "FLOAT"),
        ..Default::default()
    };
    let fixed_steps = slots
        .steps
        .is_empty()
        .then(|| {
            sampling
                .iter()
                .find(|id| class_of(&g, id) == "ManualSigmas")
                .and_then(|id| inputs(&g[id])?.get("sigmas")?.as_str().map(|s| s.split(',').count().saturating_sub(1) as u32))
        })
        .flatten();
    let latent_link = inputs(&g[&last]).and_then(|i| i.get("latent_image")).cloned().ok_or("the sampler has no latent")?;
    let (lnode, lslot) = link_of(&latent_link).map(|(n, s)| (n.to_owned(), s)).ok_or("the sampler latent is not linked")?;
    let lclass = class_of(&g, &lnode).to_owned();
    let mut grid = 0;
    let latent = if lclass.starts_with("Empty") && inputs(&g[&lnode]).is_some_and(|i| i.contains_key("width")) {
        let (w, h, _, g_res) = bind_size(&mut g, &lnode, &sampling);
        slots.width = w;
        slots.height = h;
        grid = g_res;
        LatentSpec::Empty { node: lnode, class: lclass }
    } else {
        LatentSpec::FromNode { node: lnode, output: lslot }
    };
    let decoder = up.iter().find(|id| {
        inputs(&g[*id]).is_some_and(|i| i.get("samples").and_then(link_of).is_some_and(|(s, _)| s == last))
            && inputs(&g[*id]).is_some_and(|i| i.contains_key("vae"))
    });
    let vae = decoder.and_then(|d| inputs(&g[d])?.get("vae").cloned()).unwrap_or(Value::Null);
    let loaders = roles::image_loaders(&g, info);
    let (refs, chain) = ref_capacity(&g, info, &loaders);
    let mut images = Vec::new();
    let mut r = 0;
    for l in &loaders {
        let role = if !consumers(&g, l, 1).is_empty() && consumers(&g, l, 0).is_empty() {
            ImageRole::Mask
        } else if edit && images.iter().all(|s: &ImageSlot| s.role != ImageRole::Source) {
            ImageRole::Source
        } else {
            r += 1;
            ImageRole::Reference(r - 1)
        };
        images.push(ImageSlot { node: l.clone(), role });
    }
    let files = bindings(&g, &loader, input, &BTreeSet::new());
    Ok(TaskGraph {
        template,
        output: c.output.clone(),
        samplers,
        slots,
        latent,
        vae,
        loader,
        images,
        refs,
        ref_transforms: chain,
        fixed_steps,
        files,
        enhanced: None,
        grid,
        graph: g,
    })
}

/// The same task graph with the template's enhancer kept.
fn enhanced_graph(base: &TaskGraph, e: &canon::Enhanced, main_stem: &str, input: &Input, edit: bool) -> Option<TaskGraph> {
    let c = canon::Canonical {
        graph: e.graph.clone(),
        output: base.output.clone(),
        mode: None,
        prompt: e.prompt.clone(),
        negative: base.slots.negative.clone(),
        negative_default: None,
        enhanced: None,
    };
    let mut tg = task_graph(format!("{}+enhance", base.template), &c, main_stem, input, edit).ok()?;
    if let Some(seed) = &e.seed {
        tg.slots.seeds.push(seed.clone());
    }
    let upsamplers: BTreeSet<String> =
        tg.graph.keys().filter(|id| !base.graph.contains_key(*id) && class_of(&tg.graph, id).ends_with("Loader")).cloned().collect();
    tg.files = bindings(&tg.graph, &tg.loader, input, &upsamplers);
    Some(tg)
}

fn number(g: &Map<String, Value>, slots: &[Slot]) -> Option<f64> {
    roles::first_value(g, slots).and_then(|v| v.as_f64())
}

fn text(g: &Map<String, Value>, slots: &[Slot]) -> Option<String> {
    roles::first_value(g, slots).and_then(|v| v.as_str().map(str::to_owned))
}

fn sha256(v: &Value) -> String {
    hex::encode(Sha256::digest(graph::canonical_bytes(v)))
}

/// One matched template as a task graph, its default negative and its source record.
fn source_graph(
    input: &Input,
    s: &Source,
    polish: Option<&crate::family::TemplatePolish>,
) -> Result<(TaskGraph, Option<String>, TemplateSource), Box<Unusable>> {
    let cap = s.matched.capability;
    let fail = |detail: String| Box::new(unusable(&s.entry.name, cap, detail));
    let zero: Vec<&str> = input.runtime.zero_classes.iter().map(String::as_str).collect();
    let override_graph = polish.and_then(|p| p.graphs.get(cap.id()));
    let conv = match override_graph {
        Some(api) => ui::Converted { graph: api.as_object().cloned().unwrap_or_default(), ..Default::default() },
        None => ui::to_api(s.body, input.info).map_err(|e| fail(format!("{e:#}")))?,
    };
    if let Some((id, why)) = conv.problems.first() {
        return Err(fail(format!("node {id}: {why}")));
    }
    let opts = canon::Options { info: input.info, io_outputs: &s.entry.io_outputs, main_stem: Some(&s.matched.declared.stem), zero: &zero };
    let modes = canon::canonicalise(&conv, &opts).map_err(fail)?;
    let edit = cap == Capability::Edit;
    let want = if edit { "edit" } else { "create" };
    let c = modes.iter().find(|c| c.mode.as_deref().is_none_or(|m| m == want)).ok_or_else(|| fail(format!("no {want} mode")))?;
    let problems: Vec<_> = crate::graph::validate::validate(&c.graph, input.info)
        .into_iter()
        .filter(|p| !p.is_file() && !(p.class == "LoadImage" && p.input == "image"))
        .collect();
    if !problems.is_empty() {
        return Err(Box::new(unusable_from(&s.entry.name, cap, &problems)));
    }
    let name = match &c.mode {
        Some(m) => format!("{}#{m}", s.entry.name),
        None => s.entry.name.clone(),
    };
    let mut tg = task_graph(name, c, &s.matched.declared.stem, input, edit).map_err(fail)?;
    if let Some(e) = &c.enhanced {
        tg.enhanced = enhanced_graph(&tg, e, &s.matched.declared.stem, input, edit).map(Box::new);
    }
    let source = TemplateSource {
        name: s.entry.name.clone(),
        capability: cap.id().into(),
        sha256: sha256(s.body),
        templates_version: s.templates_version.to_owned(),
        tags: s.entry.tags.clone(),
        min_comfyui: s.entry.min_comfyui.map(|v| v.to_string()),
        declared_main: s.matched.declared.name.clone(),
        tier: format!("{:?}", s.matched.tier).to_ascii_lowercase(),
    };
    Ok((tg, c.negative_default.clone(), source))
}

/// Compiles the matched templates (at most one per capability) into a recipe. A template this
/// ComfyUI cannot run is skipped (recorded in `skipped`); the error lists every template when
/// none can run.
pub fn compile(input: &Input, sources: &[Source]) -> Result<TemplateRecipe, Vec<Unusable>> {
    let polish = input.family.and_then(|f| f.template.as_ref());
    let variant = input.identity.variant.as_deref().and_then(|v| polish.and_then(|p| p.variants.get(v)));
    let mut recipe = TemplateRecipe {
        model_key: input.key.to_owned(),
        main: super::FileRef { name: input.file.to_owned(), loader: String::new() },
        polish: input.family.filter(|_| polish.is_some()).map(|f| f.id.clone()),
        recipe_version: super::RECIPE_VERSION,
        ..Default::default()
    };
    let mut negative_default = None;
    for s in sources {
        let (tg, negative, source) = match source_graph(input, s, polish) {
            Ok(x) => x,
            Err(u) => {
                recipe.skipped.push(Unusable { min_comfyui: s.entry.min_comfyui.map(|v| v.to_string()), ..*u });
                continue;
            }
        };
        if negative_default.is_none() {
            negative_default = negative;
        }
        recipe.main.loader = class_of(&tg.graph, &tg.loader).to_owned();
        recipe.sources.push(source);
        match s.matched.capability {
            Capability::Create => recipe.create = Some(tg),
            Capability::Edit => recipe.edit = Some(tg),
            Capability::Style => recipe.style = Some(tg),
        }
    }
    if recipe.create.is_none() && recipe.edit.is_none() && recipe.style.is_none() {
        return Err(if recipe.skipped.is_empty() { vec![Unusable { detail: "no matching template".into(), ..Default::default() }] } else { recipe.skipped });
    }
    // Value fixes by role path (`edit.cfg`).
    for (path, value) in variant.map(|v| v.fix.clone()).unwrap_or_default() {
        let Some((cap, role)) = path.split_once('.') else { continue };
        let g = match cap {
            "create" => recipe.create.as_mut(),
            "edit" => recipe.edit.as_mut(),
            "style" => recipe.style.as_mut(),
            _ => None,
        };
        if let Some(g) = g {
            let slots = match role {
                "cfg" => g.slots.cfg.clone(),
                "steps" => g.slots.steps.clone(),
                "sampler_name" => g.slots.sampler_name.clone(),
                "scheduler" => g.slots.scheduler.clone(),
                "denoise" => g.slots.denoise.clone(),
                _ => Vec::new(),
            };
            for (n, i) in slots {
                if let Some(ins) = g.graph.get_mut(&n).and_then(|x| x.get_mut("inputs")).and_then(Value::as_object_mut) {
                    ins.insert(i, value.clone());
                }
            }
        }
    }
    let Some(main) = recipe.create.clone().or_else(|| recipe.edit.clone()).or_else(|| recipe.style.clone()) else {
        return Err(recipe.skipped);
    };
    let g = &main.graph;
    let steps = number(g, &main.slots.steps).map(|s| s as u32).or(main.fixed_steps).unwrap_or(20);
    let cfg = number(g, &main.slots.cfg).unwrap_or(1.0) as f32;
    let (w, h) = (number(g, &main.slots.width).unwrap_or(1024.0) as u32, number(g, &main.slots.height).unwrap_or(1024.0) as u32);
    let latent_step = match &main.latent {
        LatentSpec::Empty { class, .. } => {
            input.info.input_spec(class, "width").and_then(|s| s.get(1)).and_then(|o| o.get("step")).and_then(Value::as_u64).unwrap_or(8) as u32
        }
        _ => 8,
    };
    let res_grid = main.grid;
    let sizes = variant.and_then(|v| v.sizes);
    let grid = latent_step.max(res_grid).max(sizes.map(|s| s.multiple).unwrap_or(0)).max(8);
    let native = sizes.map(|s| s.native).unwrap_or((f64::from(w) * f64::from(h)).sqrt() as u32);
    let sampling = variant.and_then(|v| v.sampling);
    let negative_text = matches!(main.slots.negative, NegativeSlot::Text(_));
    let cfg_all_one = [&recipe.create, &recipe.edit, &recipe.style]
        .into_iter()
        .flatten()
        .all(|t| t.slots.cfg.iter().all(|s| number(&t.graph, std::slice::from_ref(s)) == Some(1.0)));
    let guidance = match sampling {
        Some(s) if s.cfg.max <= 1.0 && s.cfg.min >= 1.0 => None,
        Some(s) => Some(s.cfg),
        None if !negative_text && cfg_all_one => None,
        None if main.slots.cfg.is_empty() => None,
        None => Some(Range { default: cfg, min: 1.0, max: (2.0 * cfg).max(10.0) }),
    };
    let steps_range = match sampling {
        _ if main.fixed_steps.is_some() => None,
        Some(s) if s.steps.fixed() => None,
        Some(s) => Some(s.steps),
        None if main.slots.steps.is_empty() => None,
        None => Some(Range { default: steps as f32, min: 1.0, max: (2.0 * steps as f32).max(50.0) }),
    };
    let negative_shown = negative_text && guidance.is_some_and(|g| g.max > 1.0) && polish.and_then(|p| p.negative.as_deref()) != Some("hide");
    recipe.defaults = TemplateDefaults {
        steps,
        cfg: sampling.map(|s| s.cfg.default).unwrap_or(cfg),
        sampler: text(g, &main.slots.sampler_name).unwrap_or_default(),
        scheduler: text(g, &main.slots.scheduler).unwrap_or_default(),
        width: w,
        height: h,
        grid,
        negative: negative_default.unwrap_or_default(),
        denoise: sampling.map(|s| s.denoise).filter(|d| *d > 0.0).unwrap_or(if recipe.create.is_some() { 0.35 } else { 1.0 }),
    };
    let capacity = recipe.edit.as_ref().map(|g| g.refs.max()).unwrap_or(0).max(recipe.style.as_ref().map(|g| g.refs.max()).unwrap_or(0));
    recipe.controls = Controls {
        negative: negative_shown,
        guidance,
        steps: steps_range,
        sampler: !main.slots.sampler_name.is_empty(),
        size_grid: grid,
        size_native: native,
        size_min: sizes.map(|s| s.min).unwrap_or(256),
        size_max: sizes.map(|s| s.max).unwrap_or((2 * native).min(2048)),
        references: capacity.min(polish.and_then(|p| p.references_max).unwrap_or(4) as usize) as u32,
    };
    recipe.inpaint = match polish.and_then(|p| p.inpaint.as_deref()) {
        Some("instruction") if recipe.edit.is_some() => InpaintWay::Instruction,
        _ => InpaintWay::NoiseMask,
    };
    recipe.upsampler = [&recipe.create, &recipe.edit, &recipe.style].into_iter().flatten().find_map(|t| {
        let e = t.enhanced.as_ref()?;
        let b = e.files.iter().find(|b| b.optional)?;
        Some(UpsamplerInfo { template: t.template.clone(), declared_file: b.declared.clone(), url: b.url.clone(), installed: b.used.clone() })
    });
    Ok(recipe)
}

/// A family profile for a recipe (what `ModelInfo.resolved` readers see). It is never buildable.
pub fn family_for(recipe: &TemplateRecipe, profile: Option<&Family>, id: &str, label: &str) -> Family {
    let c = &recipe.controls;
    let d = &recipe.defaults;
    let fixed = |v: f32| Range { default: v, min: v, max: v };
    let mut f = Family {
        schema: crate::family::SCHEMA,
        id: id.to_owned(),
        label: label.to_owned(),
        group: profile.map(|p| p.group.clone()).filter(|g| !g.is_empty()).unwrap_or_else(|| "Installed".into()),
        description: profile.map(|p| p.description.clone()).unwrap_or_default(),
        kind: FamilyKind::Image,
        engine: Engine::Template,
        sizes: Sizes { native: c.size_native.max(256), multiple: c.size_grid.max(8), min: c.size_min, max: c.size_max.max(c.size_min) },
        sampling: Sampling { steps: c.steps.unwrap_or(fixed(d.steps as f32)), cfg: c.guidance.unwrap_or(fixed(d.cfg)), denoise: d.denoise },
        capabilities: Capabilities {
            create: recipe.create.is_some() || recipe.style.is_some(),
            edit: recipe.edit.is_some(),
            references: recipe.reference_capacity() as u32,
            inpaint: recipe.create.is_some() || recipe.edit.is_some(),
            transparent: false,
            negative_prompt: c.negative,
            upscale: recipe.create.is_some(),
        },
        vram_gb: profile.map(|p| p.vram_gb).unwrap_or(0),
        license: profile.map(|p| p.license.clone()).unwrap_or_default(),
        license_url: profile.map(|p| p.license_url.clone()).unwrap_or_default(),
        license_summary: profile.and_then(|p| p.license_summary.clone()),
        commercial: profile.map(|p| p.commercial).unwrap_or(Commercial::Unknown),
        lora: profile.map(|p| p.lora.clone()).unwrap_or_default(),
        ..Default::default()
    };
    f.pipeline.sampler_name = d.sampler.clone();
    f.pipeline.scheduler = d.scheduler.clone();
    f.pipeline.inpaint = match recipe.inpaint {
        InpaintWay::Instruction => InpaintMethod::Instruction,
        InpaintWay::NoiseMask => InpaintMethod::NoiseMask,
    };
    f.pipeline.reference = if recipe.edit.is_some() { RefMethod::ReferenceLatent } else { RefMethod::None };
    if let Some(p) = profile {
        f.prompt.prefix = p.prompt.prefix.clone();
    }
    let negative_default = profile.and_then(|p| p.template.as_ref()).and_then(|t| t.negative.as_deref()) == Some("default_text");
    if negative_default || c.negative {
        f.prompt.negative = d.negative.clone();
    }
    f
}
