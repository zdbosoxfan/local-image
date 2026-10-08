//! Custom ComfyUI workflows: any workflow becomes a model in the Generate panel when its nodes
//! are titled with Local Image's markers. The panel shows a field for each marker found:
//!
//! | node title | what Local Image sets |
//! |---|---|
//! | `li:prompt` | the prompt (the node's `text`, `prompt` or first text input) |
//! | `li:negative` | the negative prompt |
//! | `li:image`, `li:image2`… | an image: the open document or a reference (a `LoadImage` node) |
//! | `li:mask` | the selection, as a grayscale image (a `LoadImage` node; white = selected) |
//! | `li:seed` | the seed (`seed`, `noise_seed` or `value`) |
//! | `li:steps`, `li:cfg`, `li:denoise`, `li:width`, `li:height` | those numbers |
//! | `li:output` | which output image is the result (else the first saved image) |
//!
//! Both of ComfyUI's formats are read: the API format (`{"id": {"class_type", "inputs",
//! "_meta": {"title"}}}`) directly, and the editor's UI format (`nodes` and `links`) converted
//! with the server's `/object_info`, the way Krita AI Diffusion does it: widget values are given
//! to the widget inputs in definition order (`input_order`; skipping a seed's
//! `control_after_generate` value),
//! links are followed through reroutes, primitive nodes are inlined.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::comfy::ObjectInfo;

/// What a marked node is for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Prompt,
    Negative,
    /// `li:image` (0), `li:image2` (1)…
    Image(u32),
    Mask,
    Seed,
    Steps,
    Cfg,
    Denoise,
    Width,
    Height,
    Output,
}

impl FieldKind {
    fn from_title(t: &str) -> Option<Self> {
        let t = t.trim().to_ascii_lowercase();
        let t = t.strip_prefix("li:")?;
        Some(match t {
            "prompt" | "positive" => FieldKind::Prompt,
            "negative" => FieldKind::Negative,
            "image" => FieldKind::Image(0),
            "mask" | "selection" => FieldKind::Mask,
            "seed" => FieldKind::Seed,
            "steps" => FieldKind::Steps,
            "cfg" | "guidance" => FieldKind::Cfg,
            "denoise" | "strength" => FieldKind::Denoise,
            "width" => FieldKind::Width,
            "height" => FieldKind::Height,
            "output" => FieldKind::Output,
            t if t.starts_with("image") => FieldKind::Image(t["image".len()..].parse::<u32>().ok()?.checked_sub(1)?),
            _ => return None,
        })
    }
}

/// One marked node and the input Local Image sets on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub kind: FieldKind,
    pub node: String,
    pub input: String,
    /// The workflow's own value (shown as the field's default).
    pub default: Value,
}

/// An imported workflow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomWorkflow {
    pub name: String,
    /// API format.
    pub graph: Value,
    pub fields: Vec<Field>,
}

impl CustomWorkflow {
    pub fn has(&self, kind: &FieldKind) -> bool {
        self.fields.iter().any(|f| &f.kind == kind)
    }
    pub fn images(&self) -> usize {
        self.fields.iter().filter(|f| matches!(f.kind, FieldKind::Image(_))).count()
    }
    /// Capability tags for the model picker.
    pub fn tags(&self) -> Vec<String> {
        let mut t = vec!["Custom".to_owned()];
        if self.has(&FieldKind::Prompt) {
            t.push("Prompt".into());
        }
        if self.images() > 0 {
            t.push(format!("{} image{}", self.images(), if self.images() == 1 { "" } else { "s" }));
        }
        if self.has(&FieldKind::Mask) {
            t.push("Selection".into());
        }
        t
    }
}

/// Values for one run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inputs {
    pub prompt: String,
    pub negative: String,
    pub seed: u64,
    pub steps: Option<u32>,
    pub cfg: Option<f32>,
    pub denoise: Option<f32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

fn text_input(node: &Value) -> Option<String> {
    let inputs = node.get("inputs")?.as_object()?;
    for k in ["text", "prompt", "value", "string", "text_g"] {
        if inputs.get(k).is_some_and(Value::is_string) {
            return Some(k.to_owned());
        }
    }
    inputs.iter().find(|(_, v)| v.is_string()).map(|(k, _)| k.clone())
}

fn number_input(node: &Value, names: &[&str]) -> Option<String> {
    let inputs = node.get("inputs")?.as_object()?;
    names.iter().find(|k| inputs.get(**k).is_some_and(Value::is_number)).map(|k| (*k).to_owned())
}

/// Reads an API-format workflow's markers.
pub fn from_api(name: &str, graph: Value) -> Result<CustomWorkflow> {
    let nodes = graph.as_object().context("not a ComfyUI workflow (expected an object of nodes)")?;
    let mut fields = Vec::new();
    for (id, node) in nodes {
        if node.get("class_type").and_then(Value::as_str).is_none() {
            bail!("node {id} has no class_type: not an API-format workflow");
        }
        let title = node.get("_meta").and_then(|m| m.get("title")).and_then(Value::as_str).unwrap_or("");
        let Some(kind) = FieldKind::from_title(title) else { continue };
        let input = match &kind {
            FieldKind::Prompt | FieldKind::Negative => text_input(node),
            FieldKind::Image(_) | FieldKind::Mask => Some("image".to_owned()),
            FieldKind::Seed => number_input(node, &["seed", "noise_seed", "value"]),
            FieldKind::Steps => number_input(node, &["steps", "value"]),
            FieldKind::Cfg => number_input(node, &["cfg", "guidance", "value"]),
            FieldKind::Denoise => number_input(node, &["denoise", "strength", "value"]),
            FieldKind::Width => number_input(node, &["width", "value"]),
            FieldKind::Height => number_input(node, &["height", "value"]),
            FieldKind::Output => Some(String::new()),
        };
        let Some(input) = input else { bail!("the node titled “{title}” has no input Local Image can set") };
        let default = if input.is_empty() { Value::Null } else { node["inputs"].get(&input).cloned().unwrap_or(Value::Null) };
        fields.push(Field { kind, node: id.clone(), input, default });
    }
    fields.sort_by(|a, b| a.node.cmp(&b.node));
    let has_output = nodes.values().any(|n| n["class_type"].as_str().is_some_and(|c| c.contains("SaveImage") || c.contains("PreviewImage")));
    if !has_output {
        bail!("the workflow saves no image (add a Save Image node)");
    }
    Ok(CustomWorkflow { name: name.to_owned(), graph, fields })
}

/// Reads a workflow file in either format (`info` is needed for the UI format).
pub fn import(name: &str, json: &Value, info: Option<&ObjectInfo>) -> Result<CustomWorkflow> {
    if json.get("nodes").is_some_and(Value::is_array) {
        let info = info.context("start ComfyUI to import a workflow saved from its editor (or export it in API format)")?;
        return from_api(name, ui_to_api(json, info)?);
    }
    from_api(name, json.clone())
}

/// Widget types: inputs whose values come from `widgets_values`.
fn is_widget(spec: &Value) -> bool {
    match spec.get(0) {
        Some(Value::Array(_)) => true,
        Some(Value::String(t)) => matches!(t.as_str(), "INT" | "FLOAT" | "STRING" | "BOOLEAN" | "COMBO"),
        _ => false,
    }
}

/// Converts the editor's UI format to the API format using `/object_info`.
pub fn ui_to_api(ui: &Value, info: &ObjectInfo) -> Result<Value> {
    let nodes = ui.get("nodes").and_then(Value::as_array).context("no nodes")?;
    let links: Vec<(u64, u64, u64)> = ui
        .get("links")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|l| {
            let a = l.as_array()?;
            Some((a.first()?.as_u64()?, a.get(1)?.as_u64()?, a.get(2)?.as_u64()?))
        })
        .collect();
    let by_id = |id: u64| nodes.iter().find(|n| n.get("id").and_then(Value::as_u64) == Some(id));
    // Follows a link back through reroutes; a primitive node yields its value.
    let resolve = |mut link: u64| -> Option<Value> {
        for _ in 0..32 {
            let &(_, from, slot) = links.iter().find(|(id, _, _)| *id == link)?;
            let n = by_id(from)?;
            match n.get("type").and_then(Value::as_str)? {
                "Reroute" => {
                    link = n.get("inputs")?.get(0)?.get("link")?.as_u64()?;
                }
                "PrimitiveNode" => return n.get("widgets_values")?.get(0).cloned(),
                _ => return Some(json!([from.to_string(), slot])),
            }
        }
        None
    };
    let mut out = Map::new();
    for n in nodes {
        let class = n.get("type").and_then(Value::as_str).unwrap_or("");
        let mode = n.get("mode").and_then(Value::as_u64).unwrap_or(0);
        if matches!(class, "Reroute" | "PrimitiveNode" | "Note" | "MarkdownNote") || mode == 2 || mode == 4 {
            continue;
        }
        let id = n.get("id").and_then(Value::as_u64).context("a node without an id")?;
        let def = info.0.get(class).with_context(|| format!("ComfyUI has no node “{class}” (install its custom node pack)"))?;
        let mut inputs = Map::new();
        // Linked inputs.
        for inp in n.get("inputs").and_then(Value::as_array).into_iter().flatten() {
            if let (Some(name), Some(link)) = (inp.get("name").and_then(Value::as_str), inp.get("link").and_then(Value::as_u64))
                && let Some(v) = resolve(link)
            {
                inputs.insert(name.to_owned(), v);
            }
        }
        // Widget values, in definition order.
        let mut values = n.get("widgets_values").and_then(Value::as_array).cloned().unwrap_or_default().into_iter();
        for section in ["required", "optional"] {
            let Some(spec) = def.get("input").and_then(|i| i.get(section)).and_then(Value::as_object) else { continue };
            // ComfyUI lists the definition order in `input_order` (the JSON object's own order is
            // not kept); without it, the object's order is the best guess.
            let order: Vec<String> = def
                .get("input_order")
                .and_then(|o| o.get(section))
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
                .unwrap_or_else(|| spec.keys().cloned().collect());
            for name in &order {
                let Some(s) = spec.get(name) else { continue };
                if !is_widget(s) {
                    continue;
                }
                let Some(v) = values.next() else { break };
                if !inputs.contains_key(name.as_str()) {
                    inputs.insert(name.clone(), v);
                }
                let seedlike = s.get(1).and_then(|o| o.get("control_after_generate")).and_then(Value::as_bool) == Some(true) || name == "seed" || name == "noise_seed";
                if seedlike {
                    let next = values.clone().next();
                    if next.as_ref().and_then(Value::as_str).is_some_and(|t| matches!(t, "fixed" | "increment" | "decrement" | "randomize")) {
                        values.next();
                    }
                }
            }
        }
        let mut node = json!({ "class_type": class, "inputs": Value::Object(inputs) });
        if let Some(t) = n.get("title").and_then(Value::as_str) {
            node["_meta"] = json!({ "title": t });
        }
        out.insert(id.to_string(), node);
    }
    Ok(Value::Object(out))
}

/// Sets the run's values and returns the graph plus the `LoadImage` node of each image field
/// (`Image(i)` and `Mask`, in field order). Image outputs other than `li:output` are dropped so
/// that one is the result.
pub fn prepare(w: &CustomWorkflow, inputs: &Inputs) -> (Value, Vec<(FieldKind, String)>) {
    let mut g = w.graph.clone();
    let mut images = Vec::new();
    for f in &w.fields {
        let set = |g: &mut Value, v: Value| {
            if let Some(n) = g.get_mut(&f.node).and_then(|n| n.get_mut("inputs")) {
                n[&f.input] = v;
            }
        };
        match &f.kind {
            FieldKind::Prompt => set(&mut g, json!(inputs.prompt)),
            FieldKind::Negative => set(&mut g, json!(inputs.negative)),
            FieldKind::Seed => set(&mut g, json!(inputs.seed)),
            FieldKind::Steps => {
                if let Some(v) = inputs.steps {
                    set(&mut g, json!(v));
                }
            }
            FieldKind::Cfg => {
                if let Some(v) = inputs.cfg {
                    set(&mut g, json!(v));
                }
            }
            FieldKind::Denoise => {
                if let Some(v) = inputs.denoise {
                    set(&mut g, json!(v));
                }
            }
            FieldKind::Width => {
                if let Some(v) = inputs.width {
                    set(&mut g, json!(v));
                }
            }
            FieldKind::Height => {
                if let Some(v) = inputs.height {
                    set(&mut g, json!(v));
                }
            }
            FieldKind::Image(_) | FieldKind::Mask => images.push((f.kind.clone(), f.node.clone())),
            FieldKind::Output => {}
        }
    }
    if let Some(out) = w.fields.iter().find(|f| f.kind == FieldKind::Output)
        && let Some(o) = g.as_object_mut()
    {
        o.retain(|id, n| {
            let c = n["class_type"].as_str().unwrap_or("");
            id == &out.node || !(c.contains("SaveImage") || c.contains("PreviewImage"))
        });
    }
    (g, images)
}

/// Where imported workflows are kept (`<data>/workflows/*.json`, API format plus fields).
pub fn dir() -> PathBuf {
    crate::settings::data_root().join("workflows")
}

pub fn save(w: &CustomWorkflow, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stem: String = w.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let path = dir.join(format!("{stem}.json"));
    crate::settings::write_atomic(&path, &serde_json::to_vec_pretty(w)?)?;
    Ok(path)
}

pub fn list(dir: &Path) -> Vec<CustomWorkflow> {
    let mut v: Vec<CustomWorkflow> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok())
        .collect();
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api() -> Value {
        json!({
            "3": {"class_type": "KSampler", "_meta": {"title": "li:seed"}, "inputs": {"seed": 5, "steps": 20, "cfg": 7.0, "model": ["4", 0], "positive": ["6", 0], "negative": ["7", 0], "latent_image": ["10", 0], "denoise": 0.7}},
            "4": {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": "x.safetensors"}},
            "6": {"class_type": "CLIPTextEncode", "_meta": {"title": "li:prompt"}, "inputs": {"text": "a cat", "clip": ["4", 1]}},
            "7": {"class_type": "CLIPTextEncode", "_meta": {"title": "li:negative"}, "inputs": {"text": "", "clip": ["4", 1]}},
            "8": {"class_type": "LoadImage", "_meta": {"title": "li:image"}, "inputs": {"image": "a.png"}},
            "10": {"class_type": "VAEEncode", "inputs": {"pixels": ["8", 0], "vae": ["4", 2]}},
            "11": {"class_type": "SaveImage", "_meta": {"title": "li:output"}, "inputs": {"images": ["12", 0]}},
            "13": {"class_type": "PreviewImage", "inputs": {"images": ["12", 0]}}
        })
    }

    #[test]
    fn markers_become_fields_and_values_are_set() {
        let w = from_api("Img2img", api()).unwrap();
        let kinds: Vec<_> = w.fields.iter().map(|f| f.kind.clone()).collect();
        assert!(kinds.contains(&FieldKind::Prompt) && kinds.contains(&FieldKind::Seed) && kinds.contains(&FieldKind::Image(0)) && kinds.contains(&FieldKind::Output));
        assert_eq!(w.tags(), vec!["Custom", "Prompt", "1 image"]);
        let (g, images) = prepare(&w, &Inputs { prompt: "a dog".into(), negative: "blur".into(), seed: 42, ..Default::default() });
        assert_eq!(g["6"]["inputs"]["text"], "a dog");
        assert_eq!(g["7"]["inputs"]["text"], "blur");
        assert_eq!(g["3"]["inputs"]["seed"], 42);
        assert_eq!(images, vec![(FieldKind::Image(0), "8".to_owned())]);
        assert!(g.get("13").is_none() && g.get("11").is_some());
        assert!(FieldKind::from_title("li:image3") == Some(FieldKind::Image(2)));
        assert!(from_api("x", json!({"1": {"class_type": "CLIPTextEncode", "inputs": {}}})).is_err());
    }

    #[test]
    fn ui_format_converts_with_object_info() {
        let info = ObjectInfo(json!({
            "KSampler": {"input": {"required": {"model": ["MODEL"], "seed": ["INT", {"control_after_generate": true}], "steps": ["INT", {}], "cfg": ["FLOAT", {}], "sampler_name": [["euler"]], "scheduler": [["normal"]], "positive": ["CONDITIONING"], "negative": ["CONDITIONING"], "latent_image": ["LATENT"], "denoise": ["FLOAT", {}]}},
                         "input_order": {"required": ["model", "seed", "steps", "cfg", "sampler_name", "scheduler", "positive", "negative", "latent_image", "denoise"]}},
            "CLIPTextEncode": {"input": {"required": {"text": ["STRING", {"multiline": true}], "clip": ["CLIP"]}}},
            "SaveImage": {"input": {"required": {"images": ["IMAGE"], "filename_prefix": ["STRING", {}]}}}
        }));
        let ui = json!({
            "nodes": [
                {"id": 3, "type": "KSampler", "title": "li:seed", "inputs": [{"name": "positive", "link": 1}], "widgets_values": [123, "randomize", 25, 6.5, "euler", "normal", 1.0]},
                {"id": 6, "type": "CLIPTextEncode", "title": "li:prompt", "inputs": [{"name": "clip", "link": null}], "widgets_values": ["a fox"]},
                {"id": 9, "type": "Reroute", "inputs": [{"name": "", "link": 2}]},
                {"id": 12, "type": "SaveImage", "inputs": [{"name": "images", "link": 3}], "widgets_values": ["out"]},
                {"id": 20, "type": "Note", "widgets_values": ["hello"]}
            ],
            "links": [[1, 9, 0, 3, 4, "CONDITIONING"], [2, 6, 0, 9, 0, "*"], [3, 3, 0, 12, 0, "IMAGE"]]
        });
        let api = ui_to_api(&ui, &info).unwrap();
        assert_eq!(api["3"]["inputs"]["seed"], 123);
        assert_eq!(api["3"]["inputs"]["steps"], 25);
        assert_eq!(api["3"]["inputs"]["sampler_name"], "euler");
        assert_eq!(api["3"]["inputs"]["positive"], json!(["6", 0]));
        assert!(api.get("9").is_none() && api.get("20").is_none());
        let w = import("From editor", &ui, Some(&info)).unwrap();
        assert!(w.has(&FieldKind::Prompt) && w.has(&FieldKind::Seed));
        assert!(import("x", &ui, None).is_err());
    }

    #[test]
    fn workflows_save_and_list() {
        let dir = std::env::temp_dir().join(format!("li-wf-{}", uuid::Uuid::new_v4().simple()));
        let w = from_api("My Upscaler / v2", api()).unwrap();
        save(&w, &dir).unwrap();
        assert_eq!(list(&dir), vec![w]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
