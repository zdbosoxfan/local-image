//! Offline summaries of Comfy-Org UI templates (including subgraphs) beside our API graphs.
//! cargo run -p li-ai --example refine_compare -- target/refine-templates > target/refine-comparison.md
//! A single JSON file argument summarizes that template alone. This is an inspection tool,
//! not a UI-to-API executor: widget arrays are retained verbatim, not guessed from positions.
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use li_ai::builders::{self, Params, Task};
use li_ai::family::{FamilyKind, Role};
use serde_json::Value;
use sha2::{Digest, Sha256};

const TEMPLATES: &[(&str, &[&str])] = &[
    ("sd15", &[]),
    ("sdxl", &["image_sdxl_simple"]),
    ("pony", &["image_sdxl_simple"]),
    ("illustrious", &["image_sdxl_simple"]),
    ("sd35", &["sd3.5_simple_example"]),
    ("flux1", &["flux_dev_full_text_to_image", "flux_schnell_full_text_to_image"]),
    ("flux1-kontext", &["flux_kontext_dev_basic"]),
    ("flux1-fill", &["flux_fill_inpaint_example"]),
    ("flux2", &["image_flux2"]),
    (
        "flux2-klein",
        &[
            "image_flux2_klein_image_edit_4b_distilled",
            "image_flux2_klein_image_edit_9b_distilled",
            "image_flux2_klein_image_edit_4b_base",
            "image_flux2_klein_image_edit_9b_base",
        ],
    ),
    ("qwen-image", &["image_qwen_image", "image_qwen_Image_2512"]),
    ("qwen-edit", &["image_qwen_image_edit", "image_qwen_image_edit_2509", "image_qwen_image_edit_2511"]),
    ("qwen-image-21", &["image_qwen_image_2_1_image_edit"]),
    ("z-image", &["image_z_image_turbo", "image_z_image"]),
    ("ernie", &["image_ernie_image"]),
    ("hidream", &["hidream_i1_full", "hidream_i1_dev", "hidream_i1_fast"]),
];

fn id(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_owned)
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    let mut chars = s.chars();
    let mut out: String = chars.by_ref().take(220).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

/// Old LiteGraph tuple links and current subgraph object links are both in official files.
fn link_origin(link: &Value) -> Option<(&Value, &Value)> {
    if link.is_array() { Some((link.get(1)?, link.get(2)?)) } else { Some((link.get("origin_id")?, link.get("origin_slot")?)) }
}

fn summarize_scope(scope: &Value, name: &str, out: &mut String, counts: &mut BTreeMap<String, usize>) -> Result<()> {
    let nodes = scope["nodes"].as_array().context("UI workflow has no nodes array")?;
    let links: BTreeMap<_, _> = scope["links"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| {
            let key = if l.is_array() { l.get(0) } else { l.get("id") }?;
            Some((id(key), l))
        })
        .collect();
    let by_id: BTreeMap<_, _> = nodes.iter().map(|n| (id(&n["id"]), n)).collect();
    writeln!(out, "Scope {name}:")?;
    for node in nodes {
        let class = node["type"].as_str().context("UI node has no type")?;
        if matches!(class, "Note" | "MarkdownNote") {
            continue;
        }
        *counts.entry(class.to_owned()).or_default() += 1;
        writeln!(out, "  {} {class} mode={} widgets={}", id(&node["id"]), node["mode"].as_u64().unwrap_or(0), short(&node["widgets_values"]))?;
        for input in node["inputs"].as_array().into_iter().flatten() {
            if input["link"].is_null() {
                continue;
            }
            let key = id(&input["link"]);
            let Some((origin, slot)) = links.get(&key).and_then(|l| link_origin(l)) else {
                writeln!(out, "    {} <- unresolved link {key}", input["name"])?;
                continue;
            };
            let origin_id = id(origin);
            let label = if let Some(n) = by_id.get(&origin_id) {
                format!("{}:{origin_id}[{slot}]", n["type"].as_str().unwrap_or("?"))
            } else if scope["inputNode"]["id"] == *origin || origin.as_i64() == Some(-10) {
                let name = slot.as_u64().and_then(|i| scope["inputs"].get(i as usize)).and_then(|v| v["name"].as_str()).unwrap_or("?");
                format!("subgraph-input:{name}[{slot}]")
            } else {
                format!("boundary:{origin_id}[{slot}]")
            };
            writeln!(out, "    {} <- {label}", input["name"].as_str().unwrap_or("?"))?;
        }
    }
    // Definitions can live at the root or inside another definition.
    for subgraph in scope.pointer("/definitions/subgraphs").and_then(Value::as_array).into_iter().flatten() {
        summarize_scope(subgraph, &format!("{name}/{} ({})", id(&subgraph["id"]), subgraph["name"].as_str().unwrap_or("")), out, counts)?;
    }
    Ok(())
}

fn template_summary(path: &Path) -> Result<(String, BTreeMap<String, usize>)> {
    let value: Value = serde_json::from_slice(&std::fs::read(path)?).with_context(|| path.display().to_string())?;
    let (mut summary, mut counts) = (String::new(), BTreeMap::new());
    summarize_scope(&value, "root", &mut summary, &mut counts)?;
    Ok((summary, counts))
}

fn graph_summary(graph: &Value) -> Result<(String, BTreeMap<String, usize>)> {
    let mut summary = String::new();
    let mut counts = BTreeMap::new();
    for (id, node) in graph.as_object().context("API graph is not an object")? {
        let class = node["class_type"].as_str().context("API node has no class_type")?;
        *counts.entry(class.to_owned()).or_default() += 1;
        writeln!(summary, "  {id} {class} {}", node["inputs"])?;
    }
    Ok((summary, counts))
}

fn main() -> Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "target/refine-templates".into());
    let path = Path::new(&path);
    if path.is_file() {
        print!("{}", template_summary(path)?.0);
        return Ok(());
    }
    if !path.is_dir() {
        bail!("{} is not a template directory or JSON file", path.display());
    }
    let files = [Role::Unet, Role::Checkpoint, Role::Clip, Role::Clip2, Role::Clip3, Role::Clip4, Role::Vae]
        .into_iter()
        .map(|r| (r, format!("{r:?}.safetensors")))
        .collect();
    println!(
        "# Refine graph inspection\n\nUI scope counts include optional/bypassed branches and subgraph instances; they are evidence, not an equivalence verdict. Linked inputs override widget defaults. Mode 4 means bypass.\n"
    );
    for family in li_ai::family::registry().families.iter().filter(|f| f.kind == FamilyKind::Image) {
        println!("## {}\n", family.id);
        let names = TEMPLATES.iter().find(|(id, _)| *id == family.id).map(|(_, names)| *names).context("Image family has no comparison mapping")?;
        let mut official_counts = BTreeMap::<String, usize>::new();
        if names.is_empty() {
            println!("No family-specific UI template in this snapshot; use the official ComfyUI img2img example.\n");
        }
        for name in names {
            println!("### Official {name}.json\n");
            let file = path.join(format!("{name}.json"));
            if !file.is_file() {
                println!("Missing snapshot: download https://raw.githubusercontent.com/Comfy-Org/workflow_templates/main/templates/{name}.json\n");
                continue;
            }
            let (summary, counts) = template_summary(&file)?;
            println!("Snapshot SHA-256: `{}`.\n", hex::encode(Sha256::digest(std::fs::read(&file)?)));
            println!("```text\n{summary}```\n");
            for (class, count) in counts {
                *official_counts.entry(class).or_default() += count;
            }
        }
        let mut cases = vec![("family default", family.clone())];
        for rule in &family.file_overrides {
            if let Some(pattern) = rule.patterns.first() {
                cases.push((pattern, family.resolved_for_file(pattern)));
            }
        }
        for (case, family) in cases {
            let default_denoise =
                if matches!(family.pipeline.reference, li_ai::family::RefMethod::ReferenceLatent | li_ai::family::RefMethod::TextEncoder) { 1.0 } else { 0.35 };
            let mut tasks = vec![Task::Refine { denoise: default_denoise }, Task::Refine { denoise: 0.4 }];
            if family.capabilities.edit {
                tasks.push(Task::Edit);
            }
            for task in tasks {
                let mut p = Params::create("preserve the composition", 1360, 768, 12345, family.sampling.steps.default as u32, family.sampling.cfg.default);
                p.task = task;
                let b = builders::build(&family, &files, &p);
                let (summary, counts) = graph_summary(&b.graph)?;
                println!("### Ours {case}: {task:?}\n\n```text\n{summary}```\n");
                let only_ours: Vec<_> = counts.keys().filter(|c| !official_counts.contains_key(*c)).collect();
                let only_official: Vec<_> = official_counts.keys().filter(|c| !counts.contains_key(*c)).collect();
                println!("Classes only in ours: {only_ours:?}. Classes only in official scopes: {only_official:?}.\n");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summary_resolves_tuple_object_and_subgraph_boundary_links() {
        let workflow = json!({
            "nodes": [{"id": 1, "type": "LoadImage"}, {"id": 2, "type": "sub", "widgets_values": [false], "inputs": [{"name": "pixels", "link": 1}]}],
            "links": [[1, 1, 0, 2, 0, "IMAGE"]],
            "definitions": {"subgraphs": [{"id": "sub", "inputNode": {"id": -10}, "inputs": [{"name": "pixels"}],
                "nodes": [{"id": 3, "type": "VAEEncode", "inputs": [{"name": "pixels", "link": 2}]}],
                "links": [{"id": 2, "origin_id": -10, "origin_slot": 0, "target_id": 3, "target_slot": 0}]}]}
        });
        let (mut output, mut counts) = (String::new(), BTreeMap::new());
        summarize_scope(&workflow, "root", &mut output, &mut counts).unwrap();
        assert!(output.contains("pixels <- LoadImage:1[0]"));
        assert!(output.contains("pixels <- subgraph-input:pixels[0]"));
        assert!(output.contains("widgets=[false]"));
        assert_eq!(counts["VAEEncode"], 1);
    }

    #[test]
    fn summarizes_bundled_official_subgraphs() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/templates/flux_kontext_dev_basic.json");
        let (summary, counts) = template_summary(&path).unwrap();
        assert_eq!(counts["KSampler"], 1);
        assert_eq!(counts["VAEEncode"], 1);
        assert!(summary.contains("latent_image <- VAEEncode:"));
    }
}
