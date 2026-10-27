//! Model-agnostic transforms that derive every tool from a template's own graph: create with
//! references (the edit graph on an empty latent), more reference images, refine (img2img),
//! fill (noise mask), LoRA styles, GGUF loaders, and the run's values. Each inserted node is
//! `li_<purpose>`; each inserted class must exist in `/object_info`.

use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

use super::{ImageRole, ImageSlot, LatentSpec, RefCapacity, TaskGraph, TemplateRecipe};
use crate::builders::{Params, Slot as Upload, Task};
use crate::comfy::ObjectInfo;
use crate::graph::{add_node, consumers, inputs, link, link_of, prune_to, rewire};
use crate::workflows::{Built, LoraUse};

fn need(info: &ObjectInfo, class: &str) -> Result<()> {
    if !info.has_node(class) {
        bail!("This ComfyUI doesn't have the node {class}. Update ComfyUI.");
    }
    Ok(())
}

fn set(g: &mut Map<String, Value>, node: &str, input: &str, v: Value) {
    if let Some(ins) = g.get_mut(node).and_then(|n| n.get_mut("inputs")).and_then(Value::as_object_mut) {
        ins.insert(input.to_owned(), v);
    }
}

fn final_sampler(tg: &TaskGraph) -> Result<&str> {
    tg.samplers.last().map(String::as_str).ok_or_else(|| anyhow::anyhow!("The {} template has no sampler.", tg.template))
}

/// Create with references: the edit graph sampling from an empty latent of the create graph's
/// class, its source image becoming reference 0.
pub fn create_with_refs(edit: &TaskGraph, create: &TaskGraph) -> Result<TaskGraph> {
    let LatentSpec::Empty { node, class } = &create.latent else { bail!("The create template has no empty latent to start from.") };
    let mut tg = edit.clone();
    let template_inputs = create.graph.get(node).and_then(inputs).cloned().unwrap_or_default();
    let mut latent_inputs = Map::new();
    for k in ["width", "height"] {
        latent_inputs.insert(k.into(), template_inputs.get(k).filter(|v| link_of(v).is_none()).cloned().unwrap_or(json!(1024)));
    }
    latent_inputs.insert("batch_size".into(), json!(1));
    let id = add_node(&mut tg.graph, "latent", class, Value::Object(latent_inputs));
    let sampler = final_sampler(&tg)?.to_owned();
    set(&mut tg.graph, &sampler, "latent_image", link(&id, 0));
    tg.latent = LatentSpec::Empty { node: id.clone(), class: class.clone() };
    tg.slots.width = vec![(id.clone(), "width".into())];
    tg.slots.height = vec![(id, "height".into())];
    for img in &mut tg.images {
        img.role = match img.role {
            ImageRole::Source => ImageRole::Reference(0),
            ImageRole::Reference(i) => ImageRole::Reference(i + 1),
            r => r,
        };
    }
    tg.template = format!("{}+refs", edit.template);
    Ok(tg)
}

/// Where a graph's denoise is set: the sampler's own input, its sigmas' scheduler, or a split of
/// fixed sigmas.
fn denoise_slot(tg: &mut TaskGraph, denoise: f32, info: &ObjectInfo) -> Result<()> {
    let sampler = final_sampler(tg)?.to_owned();
    let ins = tg.graph.get(&sampler).and_then(inputs).cloned().unwrap_or_default();
    if ins.contains_key("denoise") {
        tg.slots.denoise = vec![(sampler, "denoise".into())];
        return Ok(());
    }
    let sigmas = ins.get("sigmas").cloned().ok_or_else(|| anyhow::anyhow!("The {} template has no denoise control.", tg.template))?;
    if let Some((src, _)) = link_of(&sigmas)
        && tg.graph.get(src).and_then(inputs).is_some_and(|i| i.get("denoise").is_some_and(Value::is_number))
    {
        tg.slots.denoise = vec![(src.to_owned(), "denoise".into())];
        return Ok(());
    }
    need(info, "SplitSigmasDenoise")?;
    let split = add_node(&mut tg.graph, "refine_sigmas", "SplitSigmasDenoise", json!({ "sigmas": sigmas, "denoise": denoise }));
    set(&mut tg.graph, &sampler, "sigmas", link(&split, 1));
    tg.slots.denoise = vec![(split, "denoise".into())];
    Ok(())
}

/// Refine (img2img): the source image, encoded with the template's VAE, replaces the sampler's
/// starting latent; the sampler (or its scheduler) gets the strength.
pub fn refine(create: &TaskGraph, denoise: f32, info: &ObjectInfo) -> Result<TaskGraph> {
    need(info, "VAEEncode")?;
    need(info, "LoadImage")?;
    let mut tg = create.clone();
    let image = add_node(&mut tg.graph, "refine_image", "LoadImage", json!({ "image": "" }));
    let encode = add_node(&mut tg.graph, "refine_encode", "VAEEncode", json!({ "pixels": link(&image, 0), "vae": tg.vae.clone() }));
    let sampler = final_sampler(&tg)?.to_owned();
    set(&mut tg.graph, &sampler, "latent_image", link(&encode, 0));
    denoise_slot(&mut tg, denoise, info)?;
    tg.images.retain(|i| i.role != ImageRole::Source);
    tg.images.push(ImageSlot { node: image, role: ImageRole::Source });
    tg.latent = LatentSpec::FromNode { node: encode, output: 0 };
    tg.template = format!("{}+refine", create.template);
    Ok(tg)
}

/// Fill with a noise mask: refine, with the mask limiting where the latent is re-noised, and
/// differential diffusion for soft edges when this ComfyUI has it.
pub fn inpaint(create: &TaskGraph, denoise: f32, info: &ObjectInfo) -> Result<TaskGraph> {
    need(info, "ImageToMask")?;
    need(info, "SetLatentNoiseMask")?;
    let mut tg = refine(create, denoise, info)?;
    let LatentSpec::FromNode { node: encode, .. } = tg.latent.clone() else { bail!("refine produced no encoded latent") };
    let mask_image = add_node(&mut tg.graph, "mask_image", "LoadImage", json!({ "image": "" }));
    let mask = add_node(&mut tg.graph, "mask", "ImageToMask", json!({ "image": link(&mask_image, 0), "channel": "red" }));
    let noise = add_node(&mut tg.graph, "noise_mask", "SetLatentNoiseMask", json!({ "samples": link(&encode, 0), "mask": link(&mask, 0) }));
    let sampler = final_sampler(&tg)?.to_owned();
    set(&mut tg.graph, &sampler, "latent_image", link(&noise, 0));
    if info.has_node("DifferentialDiffusion") {
        let guider = tg.graph.get(&sampler).and_then(inputs).and_then(|i| i.get("guider")).and_then(link_of).map(|(n, _)| n.to_owned());
        let target = guider.unwrap_or(sampler);
        if let Some(model) = tg.graph.get(&target).and_then(inputs).and_then(|i| i.get("model")).cloned() {
            let dd = add_node(&mut tg.graph, "differential", "DifferentialDiffusion", json!({ "model": model }));
            set(&mut tg.graph, &target, "model", link(&dd, 0));
        }
    }
    tg.images.push(ImageSlot { node: mask_image, role: ImageRole::Mask });
    tg.template = format!("{}+fill", create.template);
    Ok(tg)
}

/// The GGUF loader that reads the same inputs, when the file is a `.gguf`.
fn gguf_class(class: &str) -> Option<&'static str> {
    match class {
        "UNETLoader" => Some("UnetLoaderGGUF"),
        "CLIPLoader" => Some("CLIPLoaderGGUF"),
        "DualCLIPLoader" => Some("DualCLIPLoaderGGUF"),
        _ => None,
    }
}

/// Points every loader at its resolved file; a `.gguf` file switches its loader to the GGUF one.
pub fn apply_files(tg: &mut TaskGraph, info: &ObjectInfo) -> Result<()> {
    for b in tg.files.clone() {
        let Some(used) = &b.used else {
            if b.optional {
                continue;
            }
            bail!("This model needs {} in {}/.", b.declared, b.folder);
        };
        if !tg.graph.contains_key(&b.node) {
            continue;
        }
        set(&mut tg.graph, &b.node, &b.input, json!(used));
        if used.to_ascii_lowercase().ends_with(".gguf") {
            let class = tg.graph[&b.node]["class_type"].as_str().unwrap_or("").to_owned();
            if let Some(gguf) = gguf_class(&class) {
                if !info.has_node(gguf) {
                    bail!("{used} needs the ComfyUI-GGUF custom nodes ({gguf}).");
                }
                let n = tg.graph.get_mut(&b.node).expect("checked above");
                n["class_type"] = json!(gguf);
                if let Some(ins) = n.get_mut("inputs").and_then(Value::as_object_mut) {
                    ins.retain(|k, _| info.input_spec(gguf, k).is_some());
                }
            }
        }
    }
    Ok(())
}

/// The next free entry of an autogrow input (`images.image3`, `images.image_4`).
fn autogrow_entry(input: &str, prefix: &Option<String>, names: &[String], existing: &[String]) -> Option<String> {
    match prefix {
        Some(p) => {
            let used: Vec<usize> = existing.iter().filter_map(|k| k.strip_prefix(&format!("{input}.{p}"))?.parse::<usize>().ok()).collect();
            let next = used.iter().max().map_or(0, |m| m + 1);
            Some(format!("{input}.{p}{next}"))
        }
        None => names.iter().map(|n| format!("{input}.{n}")).find(|e| !existing.contains(e)),
    }
}

/// Grows (or trims) the reference images to `n`, copying the template's per-image transforms.
pub fn grow_references(tg: &mut TaskGraph, n: usize, info: &ObjectInfo) -> Result<()> {
    let have: Vec<(usize, String)> =
        tg.images.iter().filter_map(|s| if let ImageRole::Reference(i) = s.role { Some((i, s.node.clone())) } else { None }).collect();
    if n < have.len() {
        for (_, node) in have.iter().filter(|(i, _)| *i >= n) {
            for (c, input) in consumers(&tg.graph, node, 0) {
                if let Some(ins) = tg.graph.get_mut(&c).and_then(|x| x.get_mut("inputs")).and_then(Value::as_object_mut) {
                    ins.remove(&input);
                }
            }
            tg.images.retain(|s| &s.node != node);
        }
        return Ok(());
    }
    if n > have.len() {
        need(info, "LoadImage")?;
    }
    for i in have.len()..n {
        let loader = add_node(&mut tg.graph, &format!("ref_{i}"), "LoadImage", json!({ "image": "" }));
        let mut src = link(&loader, 0);
        for t in tg.ref_transforms.clone() {
            let mut node = tg.graph.get(&t).cloned().unwrap_or(Value::Null);
            if let Some(ins) = node.get_mut("inputs").and_then(Value::as_object_mut)
                && let Some(k) = ins.iter().find(|(_, v)| link_of(v).is_some()).map(|(k, _)| k.clone())
            {
                ins.insert(k, src.clone());
            }
            let class = node["class_type"].as_str().unwrap_or("").to_owned();
            let id = add_node(&mut tg.graph, &format!("ref_{i}_transform"), &class, node["inputs"].clone());
            src = link(&id, 0);
        }
        let too_many = || anyhow::anyhow!("This model takes at most {} reference images.", tg.refs.max());
        let (node, entry) = match &tg.refs {
            RefCapacity::Autogrow { node, input, prefix, names, max } => {
                let existing: Vec<String> = tg
                    .graph
                    .get(node)
                    .and_then(inputs)
                    .map(|m| m.keys().filter(|k| k.starts_with(&format!("{input}."))).cloned().collect())
                    .unwrap_or_default();
                if existing.len() >= *max {
                    return Err(too_many());
                }
                (node.clone(), autogrow_entry(input, prefix, names, &existing).ok_or_else(too_many)?)
            }
            RefCapacity::Fixed { node, inputs: slots } => {
                let free = slots.iter().find(|k| tg.graph.get(node).and_then(inputs).is_some_and(|m| !m.contains_key(*k))).cloned();
                (node.clone(), free.ok_or_else(too_many)?)
            }
            RefCapacity::None => bail!("This model takes no reference images."),
        };
        set(&mut tg.graph, &node, &entry, src);
        tg.images.push(ImageSlot { node: loader, role: ImageRole::Reference(i) });
    }
    Ok(())
}

/// Chains LoRA loaders after the main loader (after the template's own LoRAs) and points every
/// model consumer at the last one.
pub fn chain_loras(tg: &mut TaskGraph, loras: &[LoraUse], info: &ObjectInfo) -> Result<()> {
    if loras.is_empty() {
        return Ok(());
    }
    let checkpoint = tg.graph.get(&tg.loader).is_some_and(|n| n["class_type"] == "CheckpointLoaderSimple");
    let class = if checkpoint { "LoraLoader" } else { "LoraLoaderModelOnly" };
    need(info, class)?;
    // Past the template's own LoRAs (the Krea style reference stays first).
    let mut src = tg.loader.clone();
    loop {
        let next: Vec<String> = consumers(&tg.graph, &src, 0)
            .into_iter()
            .filter(|(c, _)| matches!(tg.graph[c]["class_type"].as_str(), Some("LoraLoaderModelOnly" | "LoraLoader")))
            .map(|(c, _)| c)
            .collect();
        match next.as_slice() {
            [one] => src = one.clone(),
            _ => break,
        }
    }
    let (mut model, mut clip) = (link(&src, 0), link(&src, 1));
    let mut added = Vec::new();
    for (i, l) in loras.iter().enumerate() {
        let inputs = if checkpoint {
            json!({ "model": model, "clip": clip, "lora_name": l.name, "strength_model": l.strength, "strength_clip": l.strength })
        } else {
            json!({ "model": model, "lora_name": l.name, "strength_model": l.strength })
        };
        let id = add_node(&mut tg.graph, &format!("lora_{i}"), class, inputs);
        model = link(&id, 0);
        clip = link(&id, 1);
        added.push(id);
    }
    let except: Vec<&str> = added.iter().map(String::as_str).collect();
    rewire(&mut tg.graph, (&src, 0), &model, &except);
    if checkpoint {
        rewire(&mut tg.graph, (&src, 1), &clip, &except);
    }
    Ok(())
}

fn snap(v: u32, grid: u32) -> u32 {
    let g = grid.max(8);
    ((v + g / 2) / g).max(1) * g
}

/// Sets the run's prompt, negative, seed, sampling values, strength and size.
pub fn set_values(tg: &mut TaskGraph, recipe: &TemplateRecipe, p: &Params) {
    let g = &mut tg.graph;
    for (n, i) in &tg.slots.prompt {
        set(g, n, i, json!(p.prompt));
    }
    if let super::NegativeSlot::Text(slots) = &tg.slots.negative {
        for (n, i) in slots {
            set(g, n, i, json!(p.negative));
        }
    }
    for (k, (n, i)) in tg.slots.seeds.iter().enumerate() {
        set(g, n, i, json!(p.seed.wrapping_add(k as u64) & ((1 << 48) - 1)));
    }
    if recipe.controls.steps.is_some() && tg.fixed_steps.is_none() {
        for (n, i) in &tg.slots.steps {
            set(g, n, i, json!(p.steps));
        }
    }
    if recipe.controls.guidance.is_some() {
        for (n, i) in &tg.slots.cfg {
            set(g, n, i, json!(p.cfg));
        }
    }
    if let Some((sampler, scheduler)) = p.sampler {
        for (n, i) in &tg.slots.sampler_name {
            set(g, n, i, json!(sampler));
        }
        for (n, i) in &tg.slots.scheduler {
            set(g, n, i, json!(scheduler));
        }
    }
    if let Task::Refine { denoise } | Task::Inpaint { denoise } = p.task {
        for (n, i) in &tg.slots.denoise {
            set(g, n, i, json!(denoise.clamp(0.0, 1.0)));
        }
    }
    let grid = recipe.controls.size_grid.max(recipe.defaults.grid).max(8);
    for (n, i) in &tg.slots.width {
        set(g, n, i, json!(snap(p.width, grid)));
    }
    for (n, i) in &tg.slots.height {
        set(g, n, i, json!(snap(p.height, grid)));
    }
}

/// The graph pruned to its output, with its image inputs in upload order.
pub fn finish(mut tg: TaskGraph) -> Built {
    let output = tg.output.clone();
    prune_to(&mut tg.graph, &[output.as_str()]);
    let mut images: Vec<(String, Upload)> = tg
        .images
        .iter()
        .filter(|s| tg.graph.contains_key(&s.node))
        .map(|s| {
            let slot = match s.role {
                ImageRole::Source => Upload::Source,
                ImageRole::Mask => Upload::Mask,
                ImageRole::Reference(i) => Upload::Reference(i),
            };
            (s.node.clone(), slot)
        })
        .collect();
    images.sort_by_key(|(_, s)| match s {
        Upload::Source => 0,
        Upload::Mask => 1,
        Upload::Reference(i) => 2 + i,
    });
    let (image_nodes, slots) = images.into_iter().unzip();
    Built { graph: Value::Object(tg.graph), image_nodes, slots }
}
