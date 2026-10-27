//! Role detection on an API graph, from its structure and `/object_info` metadata only (input
//! types, `multiline`, `forceInput`, `control_after_generate`, output names): which nodes sample,
//! where the prompt and negative text go, which inputs are seeds, steps, guidance, sampler,
//! scheduler and denoise, where the latent and the images come from. Class names are used only
//! for the zeroed-negative classes (`runtime.json` `zero_classes`).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::comfy::ObjectInfo;
use crate::graph::{ancestors, class_of, consumers, inputs, link_of};

pub type Slot = (String, String);

/// Classes whose output is a zeroed conditioning (the negative of distilled models).
pub const ZERO_CLASSES: &[&str] = &["ConditioningZeroOut"];

/// What feeds a sampler's negative conditioning.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "slots")]
pub enum NegativeSlot {
    Text(Vec<Slot>),
    Zeroed,
    #[default]
    Absent,
}

/// Sampler nodes among `among`, ordered by distance from the decoder (final pass last).
pub fn samplers(g: &Map<String, Value>, info: &ObjectInfo, among: &BTreeSet<String>) -> Vec<String> {
    let mut found: Vec<String> = among
        .iter()
        .filter(|id| {
            let class = class_of(g, id);
            info.input_spec(class, "latent_image").is_some() && info.outputs(class).iter().any(|o| o == "LATENT")
        })
        .cloned()
        .collect();
    // A sampler that feeds another (directly or through latent nodes) runs first.
    found.sort_by_key(|id| std::cmp::Reverse(found_downstream(g, id)));
    found
}

/// How many nodes consume this node's outputs, transitively (more = earlier pass).
fn found_downstream(g: &Map<String, Value>, id: &str) -> usize {
    let mut seen = BTreeSet::new();
    let mut todo = vec![id.to_owned()];
    while let Some(n) = todo.pop() {
        for (c, _) in (0..4).flat_map(|slot| consumers(g, &n, slot)) {
            if seen.insert(c.clone()) {
                todo.push(c);
            }
        }
    }
    seen.len()
}

/// The conditioning roots of a sampler: `(positive, negative)` links, through its guider.
pub fn conditioning_roots(g: &Map<String, Value>, sampler: &str) -> (Option<Value>, Option<Value>) {
    let ins = inputs(&g[sampler]);
    let get = |ins: Option<&Map<String, Value>>, k: &str| ins.and_then(|i| i.get(k)).filter(|v| link_of(v).is_some()).cloned();
    if let Some((guider, _)) = ins.and_then(|i| i.get("guider")).and_then(link_of) {
        let gi = g.get(guider).and_then(inputs);
        let pos = get(gi, "positive").or_else(|| get(gi, "conditioning")).or_else(|| get(gi, "cond1"));
        return (pos, get(gi, "negative"));
    }
    (get(ins, "positive"), get(ins, "negative"))
}

/// The text inputs of an encoder node: STRING inputs with `multiline` and no `forceInput`, in
/// definition order.
pub fn text_inputs(info: &ObjectInfo, class: &str) -> Vec<String> {
    let (req, opt) = info.input_names(class);
    req.into_iter()
        .chain(opt)
        .filter(|name| {
            info.input_spec(class, name).is_some_and(|s| {
                let o = s.get(1);
                s.get(0).and_then(Value::as_str) == Some("STRING")
                    && o.and_then(|o| o.get("multiline")).and_then(Value::as_bool) == Some(true)
                    && o.and_then(|o| o.get("forceInput")).and_then(Value::as_bool) != Some(true)
            })
        })
        .collect()
}

fn has_clip_input(info: &ObjectInfo, class: &str) -> bool {
    let (req, opt) = info.input_names(class);
    req.iter().chain(&opt).any(|n| info.input_spec(class, n).and_then(|s| s.get(0)).and_then(Value::as_str) == Some("CLIP"))
}

/// Where a conditioning link's text comes from: `Some(Some(slot))` an encoder text input,
/// `Some(None)` a zeroed conditioning, `None` nothing found.
pub fn trace_encoder(g: &Map<String, Value>, info: &ObjectInfo, root: &Value, zero: &[&str]) -> Option<Option<Slot>> {
    let (mut node, mut slot) = link_of(root).map(|(n, s)| (n.to_owned(), s))?;
    for _ in 0..64 {
        let class = class_of(g, &node).to_owned();
        if zero.contains(&class.as_str()) {
            return Some(None);
        }
        if has_clip_input(info, &class) {
            let texts = text_inputs(info, &class);
            let i = if texts.len() > 1 { (slot as usize).min(texts.len() - 1) } else { 0 };
            return texts.get(i).map(|t| Some((node.clone(), t.clone())));
        }
        let out_name = info.spec(&class).and_then(|s| s.get("output_name")).and_then(|o| o.get(slot as usize)).and_then(Value::as_str).unwrap_or("").to_owned();
        let ins = inputs(g.get(&node)?)?;
        let conditioning = |name: &str| info.input_spec(&class, name).and_then(|s| s.get(0)).and_then(Value::as_str) == Some("CONDITIONING");
        let next = ins.get(&out_name).filter(|v| link_of(v).is_some() && conditioning(&out_name)).or_else(|| {
            let (req, opt) = info.input_names(&class);
            req.iter().chain(&opt).filter(|n| conditioning(n)).find_map(|n| ins.get(n).filter(|v| link_of(v).is_some()))
        })?;
        let (n, s) = link_of(next)?;
        (node, slot) = (n.to_owned(), s);
    }
    None
}

/// The prompt slots and the negative of the given samplers.
pub fn prompt_slots(g: &Map<String, Value>, info: &ObjectInfo, samplers: &[String], zero: &[&str]) -> (Vec<Slot>, NegativeSlot) {
    let mut prompt = Vec::new();
    let mut negative = Vec::new();
    let mut zeroed = false;
    for s in samplers {
        let (pos, neg) = conditioning_roots(g, s);
        if let Some(Some(slot)) = pos.as_ref().and_then(|p| trace_encoder(g, info, p, zero))
            && !prompt.contains(&slot)
        {
            prompt.push(slot);
        }
        match neg.as_ref().and_then(|n| trace_encoder(g, info, n, zero)) {
            Some(Some(slot)) if !prompt.contains(&slot) && !negative.contains(&slot) => negative.push(slot),
            Some(None) => zeroed = true,
            _ => {}
        }
    }
    let neg = if !negative.is_empty() {
        NegativeSlot::Text(negative)
    } else if zeroed {
        NegativeSlot::Zeroed
    } else {
        NegativeSlot::Absent
    };
    (prompt, neg)
}

/// Inputs of `among` nodes named `name` whose spec type is `ty` (`COMBO` also matches legacy
/// list combos).
pub fn typed_inputs(g: &Map<String, Value>, info: &ObjectInfo, among: &BTreeSet<String>, name: &str, ty: &str) -> Vec<Slot> {
    among
        .iter()
        .filter_map(|id| {
            let class = class_of(g, id);
            let spec = info.input_spec(class, name)?;
            let t = match spec.get(0)? {
                Value::Array(_) => "COMBO",
                Value::String(s) => s.as_str(),
                _ => return None,
            };
            (t == ty && inputs(&g[id]).is_some_and(|i| i.get(name).is_some_and(|v| link_of(v).is_none()))).then(|| (id.clone(), name.to_owned()))
        })
        .collect()
}

/// Seed inputs (`control_after_generate`) among `among`.
pub fn seed_slots(g: &Map<String, Value>, info: &ObjectInfo, among: &BTreeSet<String>) -> Vec<Slot> {
    let mut out = Vec::new();
    for id in among {
        let class = class_of(g, id);
        for (k, v) in inputs(&g[id]).into_iter().flatten() {
            let control = info.input_spec(class, k).and_then(|s| s.get(1)).and_then(|o| o.get("control_after_generate"));
            let truthy = matches!(control, Some(Value::Bool(true))) || matches!(control, Some(Value::String(s)) if !s.is_empty());
            if truthy && v.is_number() {
                out.push((id.clone(), k.clone()));
            }
        }
    }
    out
}

/// The union of the samplers' ancestors (where sampling settings live).
pub fn sampling_nodes(g: &Map<String, Value>, samplers: &[String]) -> BTreeSet<String> {
    samplers.iter().flat_map(|s| ancestors(g, s)).collect()
}

/// A text generator that rewrites the prompt (an LLM upsampler): a node with a CLIP input and a
/// STRING output feeding `slot` through STRING links. Returns its id.
pub fn upsampler_for(g: &Map<String, Value>, info: &ObjectInfo, slot: &Slot) -> Option<String> {
    let start = inputs(g.get(&slot.0)?)?.get(&slot.1)?;
    let mut todo: Vec<String> = link_of(start).map(|(n, _)| vec![n.to_owned()]).unwrap_or_default();
    let mut seen = BTreeSet::new();
    while let Some(id) = todo.pop() {
        if !seen.insert(id.clone()) || seen.len() > 64 {
            continue;
        }
        let class = class_of(g, &id);
        if has_clip_input(info, class) && info.outputs(class).iter().any(|o| o == "STRING") {
            return Some(id);
        }
        for v in inputs(&g[&id]).into_iter().flat_map(|i| i.values()) {
            if let Some((src, s)) = link_of(v) {
                let src_class = class_of(g, src);
                let out = info.outputs(src_class);
                if out.get(s as usize).is_some_and(|t| t == "STRING" || t == "*" || t == "COMFY_MATCHTYPE_V3") {
                    todo.push(src.to_owned());
                }
            }
        }
    }
    None
}

/// Image loaders (`LoadImage`-shaped: an `image_upload` combo and an IMAGE output).
pub fn image_loaders(g: &Map<String, Value>, info: &ObjectInfo) -> Vec<String> {
    crate::graph::sorted_ids(g)
        .into_iter()
        .filter(|id| {
            let class = class_of(g, id);
            info.input_spec(class, "image").and_then(|s| s.get(1)).and_then(|o| o.get("image_upload")).and_then(Value::as_bool) == Some(true)
                && info.outputs(class).first().is_some_and(|o| o == "IMAGE")
        })
        .collect()
}

/// For each consumer input fed by `loader`'s image (directly or through image-only transforms),
/// the consumer and input, with the transforms in between.
pub fn image_destinations(g: &Map<String, Value>, info: &ObjectInfo, loader: &str) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    let mut todo: Vec<(String, u64, Vec<String>)> = vec![(loader.to_owned(), 0, Vec::new())];
    let mut steps = 0;
    while let Some((node, slot, chain)) = todo.pop() {
        steps += 1;
        if steps > 256 {
            break;
        }
        for (c, input) in consumers(g, &node, slot) {
            let class = class_of(g, &c);
            let outs = info.outputs(class);
            let image_only = outs.len() == 1 && outs[0] == "IMAGE" && !class.starts_with("Save") && !class.starts_with("Preview");
            if image_only {
                let mut next = chain.clone();
                next.push(c.clone());
                todo.push((c, 0, next));
            } else {
                out.push((c, input, chain.clone()));
            }
        }
    }
    out.sort();
    out
}

/// The first literal value among `(node, input)` slots.
pub fn first_value(g: &Map<String, Value>, slots: &[Slot]) -> Option<Value> {
    slots.iter().find_map(|(n, i)| g.get(n).and_then(inputs).and_then(|x| x.get(i)).filter(|v| link_of(v).is_none()).cloned())
}
