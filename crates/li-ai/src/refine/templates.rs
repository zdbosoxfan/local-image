//! Refine recipes from ComfyUI's official templates: a template whose declared main file is the
//! installed one (by exact name and its declared size and hash) becomes a custom workflow with the
//! branch for that file active and its output selected.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::{Defaults, Recipe, main_file};
use crate::catalog::InstalledModel;
use crate::comfy::{JobControl, ObjectInfo, normalized_name};
use crate::custom::{self, CustomWorkflow, Field, FieldKind};
use crate::family::Role;

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
