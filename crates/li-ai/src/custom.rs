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
    /// Fields found without markers (an official template): the panel shows a "basic controls"
    /// badge.
    #[serde(default)]
    pub basic: bool,
    /// The family the template is for, when known.
    #[serde(default)]
    pub family: Option<String>,
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
        let mut t = vec![if self.basic { "Basic controls" } else { "Custom" }.to_owned()];
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
    Ok(CustomWorkflow { name: name.to_owned(), graph, fields, basic: false, family: None })
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

/// A link in either of the editor's encodings: `[id, from, from_slot, to, to_slot, type]` at the
/// top level, objects inside subgraph definitions.
#[derive(Clone, Copy, Debug)]
struct UiLink {
    id: i64,
    from: i64,
    from_slot: u64,
}

fn ui_links(v: &Value) -> Vec<UiLink> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| {
            if let Some(a) = l.as_array() {
                Some(UiLink { id: a.first()?.as_i64()?, from: a.get(1)?.as_i64()?, from_slot: a.get(2)?.as_u64()? })
            } else {
                Some(UiLink { id: l.get("id")?.as_i64()?, from: l.get("origin_id")?.as_i64()?, from_slot: l.get("origin_slot")?.as_u64()? })
            }
        })
        .collect()
}

/// Where an input's value comes from, before aliases are resolved.
#[derive(Clone, Debug, PartialEq)]
enum Src {
    /// `(scoped node id, output slot)`: a real node, or an alias (reroute, primitive, bypassed
    /// node, subgraph instance output or a subgraph's input slot).
    Out(String, u64),
    Value(Value),
}

/// A flattened node.
struct Flat {
    id: String,
    class: String,
    title: Option<String>,
    linked: Vec<(String, Src)>,
    widgets: Vec<Value>,
}

#[derive(Default)]
struct Flattener<'a> {
    defs: Vec<&'a Value>,
    nodes: Vec<Flat>,
    aliases: std::collections::HashMap<(String, u64), Option<Src>>,
    /// Promoted widget values from subgraph instances: `(scoped node, input) → value`.
    overrides: Vec<(String, String, Value)>,
}

impl<'a> Flattener<'a> {
    fn link_src(links: &[UiLink], link: i64, prefix: &str, instance: &str) -> Option<Src> {
        let l = links.iter().find(|l| l.id == link)?;
        Some(if l.from == -10 { Src::Out(format!("in:{instance}"), l.from_slot) } else { Src::Out(format!("{prefix}{}", l.from), l.from_slot) })
    }

    /// Flattens one graph level; `prefix` scopes its node ids (`"76:"` inside instance 76).
    fn scope(&mut self, nodes: &'a Value, links: &'a Value, prefix: &str, instance: &str, depth: usize) -> Result<()> {
        if depth > 8 {
            bail!("subgraphs are nested too deeply");
        }
        let links = ui_links(links);
        for n in nodes.as_array().into_iter().flatten() {
            let class = n.get("type").and_then(Value::as_str).unwrap_or("");
            let mode = n.get("mode").and_then(Value::as_u64).unwrap_or(0);
            let Some(id) = n.get("id").and_then(|v| v.as_i64().map(|i| i.to_string()).or_else(|| v.as_str().map(str::to_owned))) else { continue };
            let sid = format!("{prefix}{id}");
            let inputs: Vec<&Value> = n.get("inputs").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
            let src_of = |inp: &Value| inp.get("link").and_then(Value::as_i64).and_then(|l| Self::link_src(&links, l, prefix, instance));
            if matches!(class, "Note" | "MarkdownNote") || mode == 2 {
                continue;
            }
            if class == "Reroute" {
                self.aliases.insert((sid, 0), inputs.first().and_then(|i| src_of(i)));
                continue;
            }
            if class == "PrimitiveNode" {
                self.aliases.insert((sid, 0), n.get("widgets_values").and_then(|w| w.get(0)).cloned().map(Src::Value));
                continue;
            }
            if mode == 4 {
                // Bypassed: each output passes the first input of the same type through.
                for (slot, out) in n.get("outputs").and_then(Value::as_array).into_iter().flatten().enumerate() {
                    let ty = out.get("type").and_then(Value::as_str).unwrap_or("");
                    let through = inputs.iter().find(|i| i.get("type").and_then(Value::as_str) == Some(ty)).and_then(|i| src_of(i));
                    self.aliases.insert((sid.clone(), slot as u64), through);
                }
                continue;
            }
            if let Some(def) = self.defs.iter().copied().find(|d| d.get("id").and_then(Value::as_str) == Some(class)) {
                // A subgraph instance: its inputs feed the definition's input slots (matched by
                // name, then label), its outputs alias whatever the definition routes to them.
                let def_inputs: Vec<&Value> = def.get("inputs").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
                let mut used = vec![false; inputs.len()];
                for (k, di) in def_inputs.iter().enumerate() {
                    let name = di.get("name").and_then(Value::as_str);
                    let label = di.get("label").and_then(Value::as_str);
                    let pick = inputs.iter().enumerate().position(|(j, i)| {
                        !used[j] && i.get("name").and_then(Value::as_str) == name && (label.is_none() || i.get("label").and_then(Value::as_str).is_none_or(|l| Some(l) == label))
                    });
                    let src = pick.and_then(|j| {
                        used[j] = true;
                        src_of(inputs[j])
                    });
                    self.aliases.insert((format!("in:{sid}"), k as u64), src);
                }
                let inner = format!("{sid}:");
                self.scope(&def["nodes"], &def["links"], &inner, &sid, depth + 1)?;
                let def_links = ui_links(&def["links"]);
                for (k, out) in def.get("outputs").and_then(Value::as_array).into_iter().flatten().enumerate() {
                    let src = out
                        .get("linkIds")
                        .and_then(Value::as_array)
                        .and_then(|a| a.first())
                        .and_then(Value::as_i64)
                        .and_then(|l| Self::link_src(&def_links, l, &inner, &sid));
                    self.aliases.insert((sid.clone(), k as u64), src);
                }
                // Promoted widgets with values on the instance.
                let proxies = n.get("properties").and_then(|p| p.get("proxyWidgets")).and_then(Value::as_array);
                let values = n.get("widgets_values").and_then(Value::as_array);
                if let (Some(px), Some(vals)) = (proxies, values) {
                    for (p, v) in px.iter().zip(vals) {
                        if let (Some(node), Some(w)) = (p.get(0).and_then(Value::as_str), p.get(1).and_then(Value::as_str))
                            && w != "control_after_generate"
                        {
                            self.overrides.push((format!("{inner}{node}"), w.to_owned(), v.clone()));
                        }
                    }
                }
                continue;
            }
            let linked = inputs
                .iter()
                .filter_map(|i| Some((i.get("name")?.as_str()?.to_owned(), src_of(i)?)))
                .collect();
            self.nodes.push(Flat {
                id: sid,
                class: class.to_owned(),
                title: n.get("title").and_then(Value::as_str).map(str::to_owned),
                linked,
                widgets: n.get("widgets_values").and_then(Value::as_array).cloned().unwrap_or_default(),
            });
        }
        Ok(())
    }

    /// The API value of a source after following aliases (`None`: unconnected).
    fn resolve(&self, src: &Src) -> Option<Value> {
        let mut cur = src.clone();
        for _ in 0..64 {
            match cur {
                Src::Value(v) => return Some(v),
                Src::Out(id, slot) => match self.aliases.get(&(id.clone(), slot)) {
                    Some(Some(next)) => cur = next.clone(),
                    Some(None) => return None,
                    None => return self.nodes.iter().any(|n| n.id == id).then(|| json!([id, slot])),
                },
            }
        }
        None
    }
}

/// Converts the editor's UI format to the API format using `/object_info`, flattening subgraphs
/// (the official templates wrap their graphs in them): inner nodes get ids `instance:node`, the
/// instance's inputs and outputs are wired through, promoted widget values are applied, and
/// bypassed nodes pass their input through.
pub fn ui_to_api(ui: &Value, info: &ObjectInfo) -> Result<Value> {
    ui.get("nodes").and_then(Value::as_array).context("no nodes")?;
    let mut f = Flattener { defs: ui.get("definitions").and_then(|d| d.get("subgraphs")).and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default(), ..Default::default() };
    f.scope(&ui["nodes"], &ui["links"], "", "", 0)?;
    let mut out = Map::new();
    for n in &f.nodes {
        let def = info.0.get(&n.class).with_context(|| format!("ComfyUI has no node “{}” (install its custom node pack)", n.class))?;
        let mut inputs = Map::new();
        for (name, src) in &n.linked {
            if let Some(v) = f.resolve(src) {
                inputs.insert(name.clone(), v);
            }
        }
        // Widget values, in definition order.
        let mut values = n.widgets.clone().into_iter();
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
        for (node, w, v) in &f.overrides {
            if node == &n.id && inputs.get(w).is_none_or(|cur| !cur.is_array()) {
                inputs.insert(w.clone(), v.clone());
            }
        }
        let mut node = json!({ "class_type": n.class, "inputs": Value::Object(inputs) });
        if let Some(t) = &n.title {
            node["_meta"] = json!({ "title": t });
        }
        out.insert(n.id.clone(), node);
    }
    Ok(Value::Object(out))
}

const TEXT_KEYS: [&str; 5] = ["text", "prompt", "text_g", "value", "string"];

/// The text node feeding a sampler's `positive`/`negative`, followed back through conditioning
/// nodes, switches and string primitives. A prompt primitive wins over an encoder's own text;
/// text-processing nodes (an enhancer's system prompt, a string replace) never count.
fn text_source(g: &Map<String, Value>, start: &Value) -> Option<(String, String)> {
    let mut queue: std::collections::VecDeque<String> = start.get(0).and_then(Value::as_str).map(str::to_owned).into_iter().collect();
    let mut seen = std::collections::HashSet::new();
    let mut best: Option<(u8, String, String)> = None;
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id.clone()) || seen.len() > 64 {
            continue;
        }
        let Some(node) = g.get(&id) else { continue };
        let class = node["class_type"].as_str().unwrap_or("");
        let Some(inputs) = node.get("inputs").and_then(Value::as_object) else { continue };
        let processing = matches!(class, "StringReplace" | "StringConcatenate" | "TextGenerate") || class.contains("System");
        for k in TEXT_KEYS {
            if let Some(Value::String(_)) = inputs.get(k)
                && !processing
            {
                let rank = if class.starts_with("PrimitiveString") { 0 } else if class.contains("TextEncode") { 1 } else { 2 };
                if best.as_ref().is_none_or(|(r, _, _)| rank < *r) {
                    best = Some((rank, id.clone(), k.to_owned()));
                }
                break;
            }
        }
        for v in inputs.values() {
            if let Some(src) = v.as_array().and_then(|a| a.first()).and_then(Value::as_str) {
                queue.push_back(src.to_owned());
            }
        }
    }
    best.map(|(_, id, k)| (id, k))
}

/// Fields found without markers, for an official template run with basic controls: the prompt
/// and negative feeding the samplers, every seed, the empty latent's size and the image loaders.
pub fn basic_fields(graph: &Value) -> Vec<Field> {
    let Some(g) = graph.as_object() else { return Vec::new() };
    let mut fields = Vec::new();
    let push = |kind: FieldKind, node: &str, input: &str, fields: &mut Vec<Field>| {
        if !fields.iter().any(|f: &Field| f.node == node && f.input == input) {
            fields.push(Field { kind, node: node.to_owned(), input: input.to_owned(), default: g[node]["inputs"].get(input).cloned().unwrap_or(Value::Null) });
        }
    };
    for (id, n) in g {
        let inputs = n.get("inputs").and_then(Value::as_object);
        let Some(inputs) = inputs else { continue };
        for (key, kind) in [("positive", FieldKind::Prompt), ("cond1", FieldKind::Prompt), ("conditioning", FieldKind::Prompt), ("negative", FieldKind::Negative)] {
            if let Some(start) = inputs.get(key).filter(|v| v.is_array())
                && let Some((node, input)) = text_source(g, start)
                && !fields.iter().any(|f: &Field| f.kind == kind)
            {
                push(kind, &node, &input, &mut fields);
            }
        }
        for key in ["seed", "noise_seed"] {
            if inputs.get(key).is_some_and(Value::is_number) {
                push(FieldKind::Seed, id, key, &mut fields);
            }
        }
        let class = n["class_type"].as_str().unwrap_or("");
        if class.contains("Latent") && inputs.get("width").is_some_and(Value::is_number) && inputs.get("height").is_some_and(Value::is_number) {
            push(FieldKind::Width, id, "width", &mut fields);
            push(FieldKind::Height, id, "height", &mut fields);
        }
    }
    let mut loaders: Vec<&String> = g.iter().filter(|(_, n)| n["class_type"] == "LoadImage").map(|(id, _)| id).collect();
    loaders.sort_by_key(|id| id.split(':').map(|p| p.parse::<u64>().unwrap_or(u64::MAX)).collect::<Vec<_>>());
    for (i, id) in loaders.into_iter().enumerate() {
        push(FieldKind::Image(i as u32), id, "image", &mut fields);
    }
    // The prompt and negative can't be the same node (a sampler fed one encoder twice).
    if let (Some(p), Some(n)) = (fields.iter().find(|f| f.kind == FieldKind::Prompt), fields.iter().find(|f| f.kind == FieldKind::Negative))
        && p.node == n.node
    {
        fields.retain(|f| f.kind != FieldKind::Negative);
    }
    fields
}

/// An official template as a custom workflow with basic controls (its markers, if it has any,
/// else [`basic_fields`]).
pub fn from_template(name: &str, ui: &Value, info: &ObjectInfo) -> Result<CustomWorkflow> {
    let graph = ui_to_api(ui, info)?;
    let mut w = from_api(name, graph)?;
    if w.fields.is_empty() {
        w.fields = basic_fields(&w.graph);
        w.basic = true;
    }
    if !w.has(&FieldKind::Prompt) {
        bail!("Local Image couldn't find the prompt in this template");
    }
    Ok(w)
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

    /// Every official template fixture converts (subgraphs flattened), finds its prompt and seed,
    /// and leaves no dangling link.
    #[test]
    fn official_templates_convert_with_basic_controls() {
        let info = ObjectInfo(crate::mock::object_info());
        let dir = crate::mock_hub::fixtures_dir();
        let mut checked = 0;
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            let name = p.file_stem().unwrap().to_string_lossy().to_string();
            if p.extension().is_none_or(|x| x != "json") || name == "index" {
                continue;
            }
            let ui: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
            let w = from_template(&name, &ui, &info).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            let g = w.graph.as_object().unwrap();
            for (id, n) in g {
                for (k, v) in n["inputs"].as_object().unwrap() {
                    if let Some(src) = v.as_array().and_then(|a| a.first()).and_then(Value::as_str) {
                        assert!(g.contains_key(src), "{name}: {id}.{k} → missing {src}");
                    }
                }
            }
            assert!(w.has(&FieldKind::Seed), "{name}: no seed");
            let prompt = w.fields.iter().find(|f| f.kind == FieldKind::Prompt).unwrap();
            let (run, _) = prepare(&w, &Inputs { prompt: "a lighthouse at dusk".into(), seed: 7, width: Some(1024), height: Some(768), ..Default::default() });
            assert_eq!(run[&prompt.node]["inputs"][&prompt.input], "a lighthouse at dusk", "{name}");
            checked += 1;
        }
        assert!(checked >= 10, "{checked}");
    }

    #[test]
    fn subgraph_instances_wire_through() {
        let info = ObjectInfo(crate::mock::object_info());
        let ui: Value = serde_json::from_slice(&std::fs::read(crate::mock_hub::fixtures_dir().join("image_qwen_image.json")).unwrap()).unwrap();
        let api = ui_to_api(&ui, &info).unwrap();
        // Inner nodes are scoped by the instance id; the outer SaveImage reads the inner decode.
        assert_eq!(api["76:37"]["class_type"], "UNETLoader");
        assert_eq!(api["76:37"]["inputs"]["unet_name"], "qwen_image_fp8_e4m3fn.safetensors");
        assert_eq!(api["60"]["inputs"]["images"], json!(["76:8", 0]));
        assert_eq!(api["76:3"]["inputs"]["seed"].as_u64().is_some(), true);
        assert_eq!(api["76:3"]["inputs"]["sampler_name"], "euler");
        let w = from_template("Qwen", &ui, &info).unwrap();
        assert!(w.basic);
        assert_eq!(w.fields.iter().find(|f| f.kind == FieldKind::Prompt).map(|f| f.node.as_str()), Some("76:6"));
        assert_eq!(w.fields.iter().find(|f| f.kind == FieldKind::Negative).map(|f| f.node.as_str()), Some("76:7"));
        assert!(w.fields.iter().any(|f| f.kind == FieldKind::Width && f.node == "76:58"));
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
