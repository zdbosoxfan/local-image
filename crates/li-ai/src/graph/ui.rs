//! The editor's UI format (`nodes` and `links`, subgraph definitions) to the API format, the way
//! the ComfyUI frontend serialises a workflow for `/prompt` (Krita AI Diffusion does the same):
//!
//! - Subgraphs are flattened: inner nodes get ids `instance:node`, the instance's inputs and
//!   outputs are wired through, and an instance's own values (`widgets_values_named`, its ordered
//!   `widgets_values`, or `proxyWidgets`) override the inner widgets they feed.
//! - Reroutes and legacy primitive nodes are inlined; bypassed nodes (mode 4) pass their first
//!   input of the same type through; muted nodes (mode 2) and notes are dropped.
//! - Widget values go to widget inputs in definition order (`input_order`); a dynamic combo
//!   (`COMFY_DYNAMICCOMBO_V3`) takes one value for its option, then its option's inputs in their
//!   document order ([`ObjectInfo::nested_order`]) as dotted names (`sampling_mode.temperature`);
//!   a seed's `control_after_generate` value is skipped. `widgets_values_named` wins over order.
//! - A class this ComfyUI doesn't have is kept with its links and reported, so a branch that will
//!   be pruned never fails the conversion.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

use crate::comfy::ObjectInfo;

const WIDGET_TYPES: &[&str] = &["INT", "FLOAT", "STRING", "BOOLEAN", "COMBO"];
const CONTROL_VALUES: &[&str] = &["fixed", "increment", "decrement", "randomize"];
pub const DYNAMIC_COMBO: &str = "COMFY_DYNAMICCOMBO_V3";
pub const AUTOGROW: &str = "COMFY_AUTOGROW_V3";
pub const MATCHTYPE: &str = "COMFY_MATCHTYPE_V3";

/// Whether an input spec (`[type, options]`) takes a widget value.
pub fn is_widget(spec: &Value) -> bool {
    let opts = spec.get(1);
    let flag = |k: &str| opts.and_then(|o| o.get(k)).and_then(Value::as_bool) == Some(true);
    if flag("forceInput") || flag("rawLink") || flag("socketless") {
        return false;
    }
    match spec.get(0) {
        Some(Value::Array(_)) => true,
        Some(Value::String(t)) => WIDGET_TYPES.contains(&t.as_str()),
        _ => false,
    }
}

fn type_of(spec: &Value) -> &str {
    spec.get(0).and_then(Value::as_str).unwrap_or("")
}

/// Whether the value after this widget may be a `control_after_generate` mode.
fn seedlike(name: &str, spec: &Value) -> bool {
    let control = spec.get(1).and_then(|o| o.get("control_after_generate"));
    let truthy = match control {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        _ => false,
    };
    let last = name.rsplit('.').next().unwrap_or(name);
    truthy || last == "seed" || last == "noise_seed"
}

/// The `inputs` object of a dynamic combo's option `key`.
pub fn combo_option<'a>(spec: &'a Value, key: &str) -> Option<&'a Value> {
    spec.get(1)?.get("options")?.as_array()?.iter().find(|o| o.get("key").and_then(Value::as_str) == Some(key))?.get("inputs")
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
    named: Option<Map<String, Value>>,
    properties: Value,
}

#[derive(Default)]
struct Flattener<'a> {
    defs: Vec<&'a Value>,
    nodes: Vec<Flat>,
    aliases: HashMap<(String, u64), Option<Src>>,
    /// Promoted widget values from subgraph instances (`proxyWidgets`): `(scoped node, input) → value`.
    overrides: Vec<(String, String, Value)>,
}

fn node_id(n: &Value) -> Option<String> {
    n.get("id").and_then(|v| v.as_i64().map(|i| i.to_string()).or_else(|| v.as_str().map(str::to_owned)))
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
            let Some(id) = node_id(n) else { continue };
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
                self.instance(n, def, &inputs, &src_of, &sid, depth)?;
                continue;
            }
            let linked = inputs.iter().filter_map(|i| Some((i.get("name")?.as_str()?.to_owned(), src_of(i)?))).collect();
            self.nodes.push(Flat {
                id: sid,
                class: class.to_owned(),
                title: n.get("title").and_then(Value::as_str).map(str::to_owned),
                linked,
                widgets: n.get("widgets_values").and_then(Value::as_array).cloned().unwrap_or_default(),
                named: n.get("widgets_values_named").and_then(Value::as_object).filter(|m| !m.is_empty()).cloned(),
                properties: n.get("properties").cloned().unwrap_or(Value::Null),
            });
        }
        Ok(())
    }

    /// A subgraph instance: its inputs feed the definition's input slots (matched by name, then
    /// label), its own values feed unlinked widget slots, its outputs alias whatever the
    /// definition routes to them.
    fn instance(&mut self, n: &'a Value, def: &'a Value, inputs: &[&'a Value], src_of: &dyn Fn(&Value) -> Option<Src>, sid: &str, depth: usize) -> Result<()> {
        let def_inputs: Vec<&Value> = def.get("inputs").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
        let named = n.get("widgets_values_named").and_then(Value::as_object).filter(|m| !m.is_empty());
        let proxies = n.get("properties").and_then(|p| p.get("proxyWidgets")).and_then(Value::as_array);
        let ordered: Vec<Value> = n.get("widgets_values").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut ordered = ordered.into_iter().peekable();
        let use_order = named.is_none() && proxies.is_none() && ordered.peek().is_some();
        let mut used = vec![false; inputs.len()];
        for (k, di) in def_inputs.iter().enumerate() {
            let name = di.get("name").and_then(Value::as_str);
            let label = di.get("label").and_then(Value::as_str);
            let ty = di.get("type").and_then(Value::as_str).unwrap_or("");
            let widget = WIDGET_TYPES.contains(&ty);
            // Ordered instance values cover every widget-typed definition input, linked or not.
            let own = if use_order && widget {
                let v = ordered.next();
                let control = name.is_some_and(|nm| nm == "seed" || nm == "noise_seed");
                if control && ordered.peek().and_then(Value::as_str).is_some_and(|s| CONTROL_VALUES.contains(&s)) {
                    ordered.next();
                }
                v
            } else {
                None
            };
            let pick = inputs.iter().enumerate().position(|(j, i)| {
                !used[j]
                    && i.get("name").and_then(Value::as_str) == name
                    && (label.is_none() || i.get("label").and_then(Value::as_str).is_none_or(|l| Some(l) == label))
            });
            let linked = pick.and_then(|j| {
                used[j] = true;
                src_of(inputs[j])
            });
            let src = linked.or_else(|| named.and_then(|m| name.and_then(|nm| m.get(nm))).cloned().map(Src::Value)).or_else(|| own.map(Src::Value));
            self.aliases.insert((format!("in:{sid}"), k as u64), src);
        }
        let inner = format!("{sid}:");
        self.scope(&def["nodes"], &def["links"], &inner, sid, depth + 1)?;
        let def_links = ui_links(&def["links"]);
        for (k, out) in def.get("outputs").and_then(Value::as_array).into_iter().flatten().enumerate() {
            let src = out
                .get("linkIds")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(Value::as_i64)
                .and_then(|l| Self::link_src(&def_links, l, &inner, sid));
            self.aliases.insert((sid.to_owned(), k as u64), src);
        }
        if named.is_none()
            && let Some(px) = proxies
        {
            let values = n.get("widgets_values").and_then(Value::as_array).cloned().unwrap_or_default();
            for (p, v) in px.iter().zip(values) {
                if let (Some(node), Some(w)) = (p.get(0).and_then(Value::as_str), p.get(1).and_then(Value::as_str))
                    && w != "control_after_generate"
                {
                    self.overrides.push((format!("{inner}{node}"), w.to_owned(), v));
                }
            }
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

/// Assigns ordered widget values to `class`'s inputs; `Err` names a dynamic combo whose nested
/// order this inventory doesn't know.
fn assign_ordered(info: &ObjectInfo, class: &str, values: &[Value], linked: &BTreeSet<String>, out: &mut Map<String, Value>) -> Result<(), String> {
    let mut values = values.iter().cloned().peekable();
    let (req, opt) = info.input_names(class);
    for name in req.iter().chain(&opt) {
        let Some(spec) = info.input_spec(class, name) else { continue };
        walk(info, class, name, spec, &mut values, linked, out)?;
    }
    Ok(())
}

fn walk(
    info: &ObjectInfo,
    class: &str,
    path: &str,
    spec: &Value,
    values: &mut std::iter::Peekable<impl Iterator<Item = Value>>,
    linked: &BTreeSet<String>,
    out: &mut Map<String, Value>,
) -> Result<(), String> {
    match type_of(spec) {
        DYNAMIC_COMBO => {
            let Some(v) = values.next() else { return Ok(()) };
            let key = v.as_str().unwrap_or("").to_owned();
            if !linked.contains(path) {
                out.insert(path.to_owned(), v);
            }
            let Some(option) = combo_option(spec, &key) else { return Ok(()) };
            let has_nested = ["required", "optional"].iter().any(|s| option.get(*s).and_then(Value::as_object).is_some_and(|o| !o.is_empty()));
            if !has_nested {
                return Ok(());
            }
            let (req, opt) = info.nested_order(class, path, &key).ok_or_else(|| format!("unknown input order for {class}.{path}"))?;
            for (section, names) in [("required", req), ("optional", opt)] {
                for nested in names {
                    if let Some(nspec) = option.get(section).and_then(|s| s.get(&nested)) {
                        walk(info, class, &format!("{path}.{nested}"), nspec, values, linked, out)?;
                    }
                }
            }
        }
        AUTOGROW | MATCHTYPE => {}
        _ if is_widget(spec) => {
            let Some(v) = values.next() else { return Ok(()) };
            if !linked.contains(path) {
                out.insert(path.to_owned(), v);
            }
            if seedlike(path, spec) && values.peek().and_then(Value::as_str).is_some_and(|s| CONTROL_VALUES.contains(&s)) {
                values.next();
            }
        }
        _ => {}
    }
    Ok(())
}

/// Whether `key` (maybe dotted) names an input of `class`.
fn declared(info: &ObjectInfo, class: &str, key: &str, named: &Map<String, Value>) -> bool {
    let mut parts = key.split('.');
    let Some(first) = parts.next() else { return false };
    let Some(mut spec) = info.input_spec(class, first) else { return false };
    let mut path = first.to_owned();
    for part in parts {
        let Some(option) = named.get(&path).and_then(Value::as_str) else { return false };
        let Some(inputs) = combo_option(spec, option) else { return false };
        let Some(next) = ["required", "optional"].iter().find_map(|s| inputs.get(*s).and_then(|o| o.get(part))) else { return false };
        spec = next;
        path = format!("{path}.{part}");
    }
    true
}

/// The result of converting a UI-format workflow.
#[derive(Clone, Debug, Default)]
pub struct Converted {
    pub graph: Map<String, Value>,
    /// Nodes whose class this ComfyUI doesn't have: `(id, class)`.
    pub unknown: Vec<(String, String)>,
    /// Nodes whose values could not be placed: `(id, why)`.
    pub problems: Vec<(String, String)>,
    /// Each converted node's UI `properties` (declared model files, node pack and version).
    pub properties: BTreeMap<String, Value>,
}

/// Converts a UI-format workflow, keeping unknown classes and unreadable nodes (reported).
pub fn to_api(ui: &Value, info: &ObjectInfo) -> Result<Converted> {
    ui.get("nodes").and_then(Value::as_array).context("no nodes")?;
    let mut f = Flattener {
        defs: ui.get("definitions").and_then(|d| d.get("subgraphs")).and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default(),
        ..Default::default()
    };
    f.scope(&ui["nodes"], &ui["links"], "", "", 0)?;
    let mut out = Converted::default();
    for n in &f.nodes {
        let mut inputs = Map::new();
        for (name, src) in &n.linked {
            if let Some(v) = f.resolve(src) {
                inputs.insert(name.clone(), v);
            }
        }
        let linked: BTreeSet<String> = inputs.keys().cloned().collect();
        if !info.has_node(&n.class) {
            out.unknown.push((n.id.clone(), n.class.clone()));
        } else if let Some(named) = &n.named {
            for (k, v) in named {
                if k == "control_after_generate" || linked.contains(k) {
                    continue;
                }
                if declared(info, &n.class, k, named) {
                    inputs.insert(k.clone(), v.clone());
                } else {
                    log::debug!("{} ({}): value for undeclared input {k} ignored", n.id, n.class);
                }
            }
        } else if let Err(why) = assign_ordered(info, &n.class, &n.widgets, &linked, &mut inputs) {
            out.problems.push((n.id.clone(), why));
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
        out.graph.insert(n.id.clone(), node);
        if !n.properties.is_null() {
            out.properties.insert(n.id.clone(), n.properties.clone());
        }
    }
    Ok(out)
}

/// Converts a UI-format workflow for running as a whole: every class must exist and every value
/// must be placed.
pub fn ui_to_api(ui: &Value, info: &ObjectInfo) -> Result<Value> {
    let c = to_api(ui, info)?;
    if let Some((_, class)) = c.unknown.first() {
        bail!("ComfyUI has no node “{class}”. Update ComfyUI, or install the custom node pack that provides it.");
    }
    if let Some((id, why)) = c.problems.first() {
        bail!("node {id}: {why}");
    }
    Ok(Value::Object(c.graph))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> ObjectInfo {
        let raw = br#"{
            "TextGenerate": {"input": {"required": {"clip": ["CLIP", {}], "prompt": ["STRING", {"multiline": true}], "max_length": ["INT", {"default": 512}],
                "sampling_mode": ["COMFY_DYNAMICCOMBO_V3", {"options": [{"key": "on", "inputs": {"required": {"temperature": ["FLOAT", {}], "top_k": ["INT", {}], "seed": ["INT", {}]},
                    "optional": {"presence_penalty": ["FLOAT", {}]}}}, {"key": "off", "inputs": {"required": {}}}]}]},
                "optional": {"thinking": ["BOOLEAN", {}], "system_prompt": ["STRING", {"forceInput": true}]}},
                "input_order": {"required": ["clip", "prompt", "max_length", "sampling_mode"], "optional": ["thinking", "system_prompt"]}, "output": ["STRING", "STRING"]},
            "PrimitiveInt": {"input": {"required": {"value": ["INT", {"control_after_generate": "fixed"}]}}, "input_order": {"required": ["value"]}, "output": ["INT"]},
            "ResolutionSelector": {"input": {"required": {"aspect_ratio": ["COMBO", {"options": ["1:1"]}], "megapixels": ["FLOAT", {}], "multiple": ["INT", {}]},
                "optional": {"preview": ["RESOLUTION_PREVIEW", {"socketless": true}]}}, "input_order": {"required": ["aspect_ratio", "megapixels", "multiple"], "optional": ["preview"]}, "output": ["INT", "INT"]}
        }"#;
        ObjectInfo::from_bytes(raw).unwrap()
    }

    #[test]
    fn dynamic_combo_values_follow_the_nested_document_order() {
        let ui = json!({"nodes": [
            {"id": 1, "type": "TextGenerate", "widgets_values": ["hi", 64, "on", 0.7, 40, 5, "randomize", 1.5, true]},
            {"id": 2, "type": "PrimitiveInt", "widgets_values": [7, "fixed"]},
            {"id": 3, "type": "ResolutionSelector", "widgets_values": ["1:1", 1.0, 16]}
        ], "links": []});
        let api = ui_to_api(&ui, &info()).unwrap();
        let tg = &api["1"]["inputs"];
        assert_eq!((&tg["sampling_mode"], &tg["sampling_mode.temperature"], &tg["sampling_mode.top_k"]), (&json!("on"), &json!(0.7), &json!(40)));
        assert_eq!((&tg["sampling_mode.seed"], &tg["sampling_mode.presence_penalty"], &tg["thinking"]), (&json!(5), &json!(1.5), &json!(true)));
        assert!(tg.get("system_prompt").is_none(), "forceInput consumes no value");
        assert_eq!(api["2"]["inputs"]["value"], 7);
        assert_eq!(api["3"]["inputs"]["multiple"], 16);
        assert!(api["3"]["inputs"].get("preview").is_none(), "socketless consumes no value");
        // Without the nested order the node is refused instead of guessed.
        let mut bare = info();
        bare.0.as_object_mut().unwrap().remove(crate::comfy::ORDER_KEY);
        let c = to_api(&ui, &bare).unwrap();
        assert!(c.problems.iter().any(|(id, why)| id == "1" && why.contains("unknown input order for TextGenerate.sampling_mode")), "{:?}", c.problems);
    }

    #[test]
    fn named_values_win_and_unknown_classes_are_kept() {
        let ui = json!({"nodes": [
            {"id": 1, "type": "TextGenerate", "widgets_values": ["wrong"], "widgets_values_named": {"prompt": "right", "sampling_mode": "on", "sampling_mode.top_k": 3, "bogus": 1}},
            {"id": 2, "type": "FutureNode", "inputs": [{"name": "x", "link": 5}], "widgets_values": [1]}
        ], "links": [[5, 1, 0, 2, 0, "STRING"]]});
        let c = to_api(&ui, &info()).unwrap();
        assert_eq!(c.graph["1"]["inputs"]["prompt"], "right");
        assert_eq!(c.graph["1"]["inputs"]["sampling_mode.top_k"], 3);
        assert!(c.graph["1"]["inputs"].get("bogus").is_none());
        assert_eq!(c.unknown, vec![("2".to_owned(), "FutureNode".to_owned())]);
        assert_eq!(c.graph["2"]["inputs"]["x"], json!(["1", 0]));
        assert!(ui_to_api(&ui, &info()).unwrap_err().to_string().contains("Update ComfyUI"));
    }
}
