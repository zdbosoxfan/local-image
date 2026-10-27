//! Refreshes `tests/fixtures/comfy-<version>/` from a ComfyUI release: the minified official
//! templates the template-runtime tests use, the template index trimmed to them, `system_stats`
//! without local paths, and `/object_info` reduced to the classes they need with the nested
//! dynamic-combo order recorded (`__li_order`).
//!
//! ```sh
//! LI_AI_FIXTURES_OBJECT_INFO=object_info.json LI_AI_FIXTURES_TEMPLATES=<comfyui_workflow_templates_json>/templates \
//! LI_AI_FIXTURES_INDEX=index.json LI_AI_FIXTURES_STATS=system_stats.json \
//!   cargo test -p li-ai --test ai_fixtures -- --ignored
//! ```

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::{Map, Value, json};

/// The templates the tests use.
const TEMPLATES: &[&str] = &[
    "image_flux3_klein_t2i",
    "image_flux3_klein_distilled_t2i",
    "image_flux3_klein_image_edit",
    "image_flux3_klein_distilled_image_edit",
    "image_moss_image_t2i",
    "image_moss_image_base_t2i",
    "image_krea2_turbo_t2i",
    "image_krea2_turbo_t2i_int8",
    "image_krea2_turbo_int8_image_style_reference",
    "image_qwen_image_2_1_turbo_t2i",
    "image_qwen_image_2_1_turbo_image_edit",
    "image_flux2_klein_image_edit_9b_base",
    "hidream_e1_1",
    "image_hidream_o1",
];

/// Classes the graph transforms and builders insert.
const EXTRA_CLASSES: &[&str] = &[
    "VAEEncode",
    "VAEDecode",
    "SetLatentNoiseMask",
    "ImageToMask",
    "DifferentialDiffusion",
    "SplitSigmasDenoise",
    "LoraLoader",
    "LoraLoaderModelOnly",
    "LoadImage",
    "SaveImage",
    "ImageScaleToTotalPixels",
    "EmptyLatentImage",
    "EmptyFlux2LatentImage",
    "ConditioningZeroOut",
    "CLIPTextEncode",
    "KSampler",
    "UNETLoader",
    "CLIPLoader",
    "VAELoader",
    "CheckpointLoaderSimple",
];

const NODE_KEYS: &[&str] = &["id", "type", "mode", "title", "inputs", "outputs", "widgets_values", "widgets_values_named", "properties"];

fn env_path(k: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(k).unwrap_or_else(|| panic!("set {k}")))
}

fn read(p: &std::path::Path) -> Value {
    serde_json::from_slice(&std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).unwrap()
}

fn cut_strings(v: &mut Value) {
    match v {
        Value::String(s) if s.chars().count() > 200 => *s = s.chars().take(200).collect(),
        Value::Array(a) => a.iter_mut().for_each(cut_strings),
        Value::Object(o) => o.values_mut().for_each(cut_strings),
        _ => {}
    }
}

fn minify_nodes(nodes: &Value, classes: &mut BTreeSet<String>) -> Value {
    let kept = nodes.as_array().into_iter().flatten().filter(|n| !matches!(n["type"].as_str(), Some("Note" | "MarkdownNote"))).map(|n| {
        let mut m = Map::new();
        for k in NODE_KEYS {
            if let Some(v) = n.get(*k) {
                m.insert((*k).to_owned(), v.clone());
            }
        }
        if let Some(ins) = m.get_mut("inputs").and_then(Value::as_array_mut) {
            for i in ins.iter_mut().filter_map(Value::as_object_mut) {
                i.retain(|k, _| matches!(k.as_str(), "name" | "type" | "link" | "label" | "widget"));
            }
        }
        if let Some(outs) = m.get_mut("outputs").and_then(Value::as_array_mut) {
            for o in outs.iter_mut().filter_map(Value::as_object_mut) {
                o.retain(|k, _| matches!(k.as_str(), "name" | "type" | "links"));
            }
        }
        if let Some(p) = m.get_mut("properties").and_then(Value::as_object_mut) {
            p.retain(|k, _| matches!(k.as_str(), "models" | "proxyWidgets" | "cnr_id" | "ver"));
        }
        for k in ["widgets_values", "widgets_values_named"] {
            if let Some(v) = m.get_mut(k) {
                cut_strings(v);
            }
        }
        if let Some(t) = n["type"].as_str() {
            classes.insert(t.to_owned());
        }
        Value::Object(m)
    });
    Value::Array(kept.collect())
}

fn minify_template(t: &Value, classes: &mut BTreeSet<String>) -> Value {
    let mut out = json!({ "nodes": minify_nodes(&t["nodes"], classes), "links": t["links"].clone() });
    if let Some(defs) = t.get("definitions").and_then(|d| d.get("subgraphs")).and_then(Value::as_array) {
        let defs: Vec<Value> = defs
            .iter()
            .map(|d| {
                let slots = |k: &str| -> Value {
                    Value::Array(
                        d[k].as_array()
                            .into_iter()
                            .flatten()
                            .map(|s| {
                                let mut s = s.as_object().cloned().unwrap_or_default();
                                s.retain(|k, _| matches!(k.as_str(), "name" | "type" | "label" | "linkIds"));
                                Value::Object(s)
                            })
                            .collect(),
                    )
                };
                json!({ "id": d["id"], "name": d["name"], "inputs": slots("inputs"), "outputs": slots("outputs"),
                        "nodes": minify_nodes(&d["nodes"], classes), "links": d["links"].clone() })
            })
            .collect();
        out["definitions"] = json!({ "subgraphs": defs });
    }
    out
}

fn strip_spec(v: &mut Value) {
    match v {
        Value::Object(o) => {
            o.remove("tooltip");
            o.remove("description");
            o.values_mut().for_each(strip_spec);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_spec),
        _ => {}
    }
}

/// Empties a loader's file list (the tests list their own files).
fn empty_files(spec: &mut Value) {
    for section in ["required", "optional"] {
        for (_, s) in spec["input"][section].as_object_mut().into_iter().flatten() {
            let file_like = |c: &Value| {
                c.as_str().is_some_and(|c| {
                    [".safetensors", ".gguf", ".pt", ".pth", ".ckpt", ".png", ".jpg", ".jpeg", ".webp", ".sft", ".bin"].iter().any(|e| c.ends_with(e))
                })
            };
            if s[0].as_array().is_some_and(|a| a.iter().any(file_like)) {
                s[0] = json!([]);
            }
            if s[0] == "COMBO" && s[1]["options"].as_array().is_some_and(|a| a.iter().any(file_like)) {
                s[1]["options"] = json!([]);
            }
        }
    }
}

#[test]
#[ignore = "regenerates tests/fixtures/comfy-<version> from a ComfyUI release (see the module docs)"]
fn refresh_template_fixtures() {
    let raw = std::fs::read(env_path("LI_AI_FIXTURES_OBJECT_INFO")).unwrap();
    let info: Value = serde_json::from_slice(&raw).unwrap();
    let order = li_ai::comfy::nested_order(&raw).unwrap();
    let stats = read(&env_path("LI_AI_FIXTURES_STATS"));
    let version = stats["system"]["comfyui_version"].as_str().unwrap().to_owned();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/comfy-{version}"));
    std::fs::create_dir_all(out.join("templates")).unwrap();
    let templates = env_path("LI_AI_FIXTURES_TEMPLATES");
    let mut classes: BTreeSet<String> = EXTRA_CLASSES.iter().map(|c| (*c).to_owned()).collect();
    for name in TEMPLATES {
        let t = read(&templates.join(format!("{name}.json")));
        let min = minify_template(&t, &mut classes);
        std::fs::write(out.join("templates").join(format!("{name}.json")), serde_json::to_string(&min).unwrap() + "\n").unwrap();
    }
    let mut min_info = Map::new();
    let mut min_order = Map::new();
    for class in &classes {
        if let Some(spec) = info.get(class) {
            let mut spec = spec.clone();
            strip_spec(&mut spec);
            empty_files(&mut spec);
            if let Some(o) = spec.as_object_mut() {
                o.retain(|k, _| matches!(k.as_str(), "input" | "input_order" | "output" | "output_name" | "output_node" | "python_module"));
            }
            min_info.insert(class.clone(), spec);
        }
        if let Some(o) = order.get(class) {
            min_order.insert(class.clone(), o.clone());
        }
    }
    min_info.insert(li_ai::comfy::ORDER_KEY.into(), Value::Object(min_order));
    std::fs::write(out.join("object_info.min.json"), serde_json::to_string(&Value::Object(min_info)).unwrap() + "\n").unwrap();
    let mut index = read(&env_path("LI_AI_FIXTURES_INDEX"));
    for cat in index.as_array_mut().into_iter().flatten() {
        if let Some(ts) = cat.get_mut("templates").and_then(Value::as_array_mut) {
            ts.retain(|t| t["name"].as_str().is_some_and(|n| TEMPLATES.contains(&n)));
            for t in ts.iter_mut().filter_map(Value::as_object_mut) {
                t.remove("thumbnail");
                t.remove("logos");
            }
        }
    }
    index.as_array_mut().unwrap().retain(|c| c["templates"].as_array().is_some_and(|t| !t.is_empty()));
    std::fs::write(out.join("templates/index.json"), serde_json::to_string_pretty(&index).unwrap() + "\n").unwrap();
    let mut stats = stats;
    stats["system"]["argv"] = json!(["main.py"]);
    stats["system"]["python_version"] = json!("3.12");
    std::fs::write(out.join("system_stats.json"), serde_json::to_string_pretty(&stats).unwrap() + "\n").unwrap();
    let mut old = stats.clone();
    old["system"]["comfyui_version"] = json!("0.38.0");
    old["system"]["installed_templates_version"] = json!("0.10.50");
    old["system"]["required_templates_version"] = json!("0.10.50");
    std::fs::write(out.join("system_stats-0.38.0.json"), serde_json::to_string_pretty(&old).unwrap() + "\n").unwrap();
}
