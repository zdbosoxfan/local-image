//! Canonicalisation of an official template's converted graph into the graph Local Image runs:
//!
//! 1. unset widget inputs get their defaults (what the frontend does for older templates);
//! 2. the output is the saved image that uses a sampler and the template's main model
//!    (comparison branches and bypassed variants drop out);
//! 3. a switch that chooses between a text-only and an image-conditioned branch splits the
//!    template into a create and an edit mode;
//! 4. the prompt and negative encoders' text inputs become placeholders, which cuts whatever fed
//!    them: an LLM prompt enhancer, layout prompts with example boxes, trigger-word concatenation,
//!    system prompts. The enhancer is kept in a separate `enhanced` variant, with the user's
//!    prompt going into it instead;
//! 5. primitives, previews, reroutes and switches with a literal value are folded away;
//! 6. everything the output doesn't read is pruned;
//! 7. required image inputs left open get a `LoadImage`.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use super::ui::{Converted, DYNAMIC_COMBO, combo_option, is_widget};
use super::{add_node, ancestors, class_of, consumers, inputs, link, link_of, prune_to, rewire, sorted_ids};
use crate::comfy::ObjectInfo;
use crate::template::roles::{self, NegativeSlot, Slot};

const PRIMITIVE_MODULE: &str = "comfy_extras.nodes_primitive";

/// What canonicalisation needs to know about the template.
pub struct Options<'a> {
    pub info: &'a ObjectInfo,
    /// The template index's `io.outputs` node ids, preferred output first.
    pub io_outputs: &'a [i64],
    /// The stem key of the template's main model file: the output must use that loader.
    pub main_stem: Option<&'a str>,
    /// Classes whose output is a zeroed conditioning.
    pub zero: &'a [&'a str],
}

/// The template with its LLM prompt enhancer kept: the user's prompt goes to `prompt`.
#[derive(Clone, Debug, PartialEq)]
pub struct Enhanced {
    pub graph: Map<String, Value>,
    pub prompt: Vec<Slot>,
    /// The enhancer's own seed, bound to the image seed.
    pub seed: Option<Slot>,
}

/// One canonical graph.
#[derive(Clone, Debug, PartialEq)]
pub struct Canonical {
    pub graph: Map<String, Value>,
    pub output: String,
    /// `create` or `edit` when the template switches between the two.
    pub mode: Option<String>,
    pub prompt: Vec<Slot>,
    pub negative: NegativeSlot,
    /// The template's own negative text, when it has one.
    pub negative_default: Option<String>,
    pub enhanced: Option<Enhanced>,
}

fn widget_default(spec: &Value, required: bool) -> Option<Value> {
    if let Some(d) = spec.get(1).and_then(|o| o.get("default")) {
        return Some(d.clone());
    }
    if !required {
        return None;
    }
    match spec.get(0) {
        Some(Value::Array(a)) => a.first().cloned(),
        Some(Value::String(t)) if t == "COMBO" => spec.get(1)?.get("options")?.as_array()?.first().cloned(),
        _ => None,
    }
}

fn fill_input(ins: &mut Map<String, Value>, path: &str, spec: &Value, required: bool) {
    if spec.get(0).and_then(Value::as_str) == Some(DYNAMIC_COMBO) {
        if !ins.contains_key(path) {
            let first = spec.get(1).and_then(|o| o.get("options")).and_then(Value::as_array).and_then(|a| a.first()).and_then(|o| o.get("key")).cloned();
            match spec.get(1).and_then(|o| o.get("default")).cloned().or(first) {
                Some(d) if required => {
                    ins.insert(path.to_owned(), d);
                }
                _ => return,
            }
        }
        let key = ins[path].as_str().unwrap_or("").to_owned();
        if let Some(option) = combo_option(spec, &key) {
            for (section, req) in [("required", true), ("optional", false)] {
                for (name, nspec) in option.get(section).and_then(Value::as_object).into_iter().flatten() {
                    fill_input(ins, &format!("{path}.{name}"), nspec, req);
                }
            }
        }
        return;
    }
    if is_widget(spec)
        && !ins.contains_key(path)
        && let Some(d) = widget_default(spec, required)
    {
        ins.insert(path.to_owned(), d);
    }
}

/// Fills unset widget inputs with their defaults (the first choice of a required combo).
pub fn fill_defaults(g: &mut Map<String, Value>, info: &ObjectInfo) {
    for id in sorted_ids(g) {
        let class = class_of(g, &id).to_owned();
        let Some(spec) = info.spec(&class).and_then(|s| s.get("input")).cloned() else { continue };
        let Some(ins) = g.get_mut(&id).and_then(|n| n.get_mut("inputs")).and_then(Value::as_object_mut) else { continue };
        for (section, required) in [("required", true), ("optional", false)] {
            for (name, ispec) in spec.get(section).and_then(Value::as_object).into_iter().flatten() {
                fill_input(ins, name, ispec, required);
            }
        }
    }
}

/// Main model loaders: nodes with a `unet_name` or `ckpt_name` file.
pub fn main_loaders(g: &Map<String, Value>, stem: Option<&str>) -> Vec<String> {
    sorted_ids(g)
        .into_iter()
        .filter(|id| {
            inputs(&g[id]).is_some_and(|i| {
                ["unet_name", "ckpt_name"]
                    .iter()
                    .any(|k| i.get(*k).and_then(Value::as_str).is_some_and(|f| stem.is_none_or(|s| crate::stem::stem_key(f).key == s)))
            })
        })
        .collect()
}

/// The saved image the template produces with its main model.
pub fn choose_output(g: &Map<String, Value>, opts: &Options) -> Option<String> {
    let info = opts.info;
    let mut candidates: Vec<String> =
        sorted_ids(g).into_iter().filter(|id| info.is_output_node(class_of(g, id)) && info.input_spec(class_of(g, id), "images").is_some()).collect();
    let rank = |id: &String| {
        let preferred = opts.io_outputs.first().is_some_and(|n| n.to_string() == *id);
        let class = class_of(g, id);
        (!preferred, class != "SaveImage", class != "SaveImageAdvanced")
    };
    candidates.sort_by_key(rank);
    let mains = main_loaders(g, opts.main_stem);
    candidates.into_iter().find(|id| {
        let up = ancestors(g, id);
        !roles::samplers(g, info, &up).is_empty() && (mains.is_empty() && opts.main_stem.is_none() || mains.iter().any(|m| up.contains(m)))
    })
}

fn is_switch(g: &Map<String, Value>, info: &ObjectInfo, id: &str) -> bool {
    let class = class_of(g, id);
    info.input_spec(class, "switch").and_then(|s| s.get(0)).and_then(Value::as_str) == Some("BOOLEAN")
        && info.input_spec(class, "on_true").is_some()
        && info.input_spec(class, "on_false").is_some()
}

fn branch(g: &Map<String, Value>, id: &str, side: &str) -> BTreeSet<String> {
    inputs(&g[id]).and_then(|i| i.get(side)).and_then(link_of).map(|(n, _)| ancestors(g, n)).unwrap_or_default()
}

/// A switch whose one branch conditions on a loaded image: `(switch, the edit side's value)`.
fn mode_switch(g: &Map<String, Value>, info: &ObjectInfo, among: &BTreeSet<String>) -> Option<(String, bool)> {
    let loaders = roles::image_loaders(g, info);
    let conditions = |set: &BTreeSet<String>| {
        loaders.iter().any(|l| set.contains(l)) && set.iter().any(|n| info.outputs(class_of(g, n)).iter().any(|o| o == "CONDITIONING"))
    };
    among.iter().filter(|id| is_switch(g, info, id)).find_map(|id| match (conditions(&branch(g, id, "on_true")), conditions(&branch(g, id, "on_false"))) {
        (true, false) => Some((id.clone(), true)),
        (false, true) => Some((id.clone(), false)),
        _ => None,
    })
}

/// Sets a switch: through the primitive that drives it when there is one (other switches share it).
fn set_switch(g: &mut Map<String, Value>, info: &ObjectInfo, switch: &str, on: bool) {
    let driver = inputs(&g[switch]).and_then(|i| i.get("switch")).and_then(link_of).map(|(n, _)| n.to_owned());
    match driver.filter(|d| is_primitive(g, info, d)) {
        Some(d) => set_input(g, &(d, "value".into()), json!(on)),
        None => set_input(g, &(switch.to_owned(), "switch".into()), json!(on)),
    }
}

fn is_primitive(g: &Map<String, Value>, info: &ObjectInfo, id: &str) -> bool {
    info.spec(class_of(g, id)).and_then(|s| s.get("python_module")).and_then(Value::as_str) == Some(PRIMITIVE_MODULE)
}

/// Primitive string nodes upstream of `slot` through STRING links, not passing `stop`.
fn string_primitives(g: &Map<String, Value>, info: &ObjectInfo, slot: &Slot, stop: Option<&str>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut todo: Vec<String> = inputs(&g[&slot.0]).and_then(|i| i.get(&slot.1)).and_then(link_of).map(|(n, _)| vec![n.to_owned()]).unwrap_or_default();
    let mut seen = BTreeSet::new();
    while let Some(id) = todo.pop() {
        if Some(id.as_str()) == stop || !seen.insert(id.clone()) || seen.len() > 128 {
            continue;
        }
        if is_primitive(g, info, &id) && info.outputs(class_of(g, &id)).first().is_some_and(|o| o == "STRING") {
            out.insert(id);
            continue;
        }
        for v in inputs(&g[&id]).into_iter().flat_map(|i| i.values()) {
            if let Some((n, _)) = link_of(v) {
                todo.push(n.to_owned());
            }
        }
    }
    out
}

fn set_input(g: &mut Map<String, Value>, (node, input): &Slot, v: Value) {
    if let Some(ins) = g.get_mut(node).and_then(|n| n.get_mut("inputs")).and_then(Value::as_object_mut) {
        ins.insert(input.clone(), v);
    }
}

/// Folds primitives, previews, reroutes and literal switches until nothing changes.
pub fn fold(g: &mut Map<String, Value>, info: &ObjectInfo) {
    for _ in 0..64 {
        let mut changed = false;
        for id in sorted_ids(g) {
            let class = class_of(g, &id).to_owned();
            let ins = inputs(&g[&id]).cloned().unwrap_or_default();
            let module = info.spec(&class).and_then(|s| s.get("python_module")).and_then(Value::as_str).unwrap_or("");
            let replacement = if module == PRIMITIVE_MODULE {
                ins.get("value").filter(|v| link_of(v).is_none()).cloned().map(Some)
            } else if class == "PreviewAny" || class == "Reroute" {
                ins.values().next().cloned().map(Some)
            } else if is_switch(g, info, &id) {
                ins.get("switch").and_then(Value::as_bool).map(|on| ins.get(if on { "on_true" } else { "on_false" }).cloned())
            } else {
                None
            };
            let Some(to) = replacement else { continue };
            let users = consumers(g, &id, 0);
            if users.is_empty() {
                continue;
            }
            changed = true;
            match to {
                Some(v) => rewire(g, (&id, 0), &v, &[]),
                None => {
                    for (c, input) in users {
                        if let Some(ci) = g.get_mut(&c).and_then(|n| n.get_mut("inputs")).and_then(Value::as_object_mut) {
                            ci.remove(&input);
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// A switch the output still needs whose value isn't a literal.
fn driven_switch(g: &Map<String, Value>, info: &ObjectInfo) -> Option<String> {
    sorted_ids(g).into_iter().find(|id| is_switch(g, info, id) && inputs(&g[id]).and_then(|i| i.get("switch")).is_some_and(|v| link_of(v).is_some())).map(
        |id| {
            let src = inputs(&g[&id]).and_then(|i| i.get("switch")).and_then(link_of).map(|(n, _)| class_of(g, n).to_owned()).unwrap_or_default();
            format!("switch driven by {src}")
        },
    )
}

/// Gives every required IMAGE input left open its own `LoadImage`.
pub fn open_images(g: &mut Map<String, Value>, info: &ObjectInfo) {
    for id in sorted_ids(g) {
        let class = class_of(g, &id).to_owned();
        let (req, _) = info.input_names(&class);
        for name in req {
            let is_image = info.input_spec(&class, &name).and_then(|s| s.get(0)).and_then(Value::as_str) == Some("IMAGE");
            if is_image && inputs(&g[&id]).is_some_and(|i| !i.contains_key(&name)) {
                let l = add_node(g, "image", "LoadImage", json!({ "image": "" }));
                set_input(g, &(id.clone(), name), link(&l, 0));
            }
        }
    }
}

/// The literal text a slot reads once the graph is folded.
fn folded_text(g: &Map<String, Value>, info: &ObjectInfo, slot: &Slot) -> Option<String> {
    let mut copy = g.clone();
    fold(&mut copy, info);
    inputs(copy.get(&slot.0)?)?.get(&slot.1)?.as_str().map(str::to_owned)
}

/// The switches between `slot` and the enhancer node, each set to the enhancer's side.
fn enhancer_path_switches(g: &Map<String, Value>, info: &ObjectInfo, slot: &Slot, enhancer: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut todo: Vec<String> = inputs(&g[&slot.0]).and_then(|i| i.get(&slot.1)).and_then(link_of).map(|(n, _)| vec![n.to_owned()]).unwrap_or_default();
    let mut seen = BTreeSet::new();
    while let Some(id) = todo.pop() {
        if !seen.insert(id.clone()) || seen.len() > 64 {
            continue;
        }
        if is_switch(g, info, &id) {
            let on_true = branch(g, &id, "on_true").contains(enhancer);
            out.push((id.clone(), on_true));
        }
        for v in inputs(&g[&id]).into_iter().flat_map(|i| i.values()) {
            if let Some((n, _)) = link_of(v) {
                todo.push(n.to_owned());
            }
        }
    }
    out
}

fn finish(mut g: Map<String, Value>, output: &str, info: &ObjectInfo) -> Result<Map<String, Value>, String> {
    fold(&mut g, info);
    prune_to(&mut g, &[output]);
    if let Some(why) = driven_switch(&g, info) {
        return Err(why);
    }
    open_images(&mut g, info);
    Ok(g)
}

fn rewrite_output(g: &mut Map<String, Value>, output: &str) {
    if class_of(g, output) == "SaveImageAdvanced" {
        let images = inputs(&g[output]).and_then(|i| i.get("images")).cloned().unwrap_or(Value::Null);
        g.insert(output.to_owned(), json!({ "class_type": "SaveImage", "inputs": { "images": images, "filename_prefix": "LocalImage" } }));
    }
}

fn one_mode(mut g: Map<String, Value>, output: &str, mode: Option<String>, opts: &Options) -> Result<Canonical, String> {
    let info = opts.info;
    // Roles are read on the folded graph (switches resolved); the enhancer is found on the
    // unfolded one, where a switched-off branch still shows it.
    let mut folded = g.clone();
    fold(&mut folded, info);
    let up = ancestors(&folded, output);
    let samplers = roles::samplers(&folded, info, &up);
    let (prompt, negative) = roles::prompt_slots(&folded, info, &samplers, opts.zero);
    if prompt.is_empty() {
        return Err("no prompt input found".into());
    }
    let negative_default = match &negative {
        NegativeSlot::Text(slots) => slots.first().and_then(|s| folded_text(&g, info, s)).filter(|t| !t.trim().is_empty()),
        _ => None,
    };
    let enhancer = prompt.iter().find_map(|s| roles::upsampler_for(&g, info, s));
    let enhanced = match &enhancer {
        Some(tg) => {
            let mut e = g.clone();
            for slot in &prompt {
                for (sw, on) in enhancer_path_switches(&e, info, slot, tg) {
                    set_input(&mut e, &(sw, "switch".into()), json!(on));
                }
            }
            // The user's text enters the enhancer where the template's own prompt primitive does
            // (it may be wrapped in instructions first); else straight into its prompt input.
            let plain: BTreeSet<String> = prompt.iter().flat_map(|s| string_primitives(&g, info, s, Some(tg))).collect();
            let into_enhancer = string_primitives(&g, info, &(tg.clone(), "prompt".into()), None);
            let user: Vec<String> = plain.intersection(&into_enhancer).cloned().collect();
            let slots: Vec<Slot> = match user.as_slice() {
                [p] => {
                    let feeds = ancestors(&e, tg);
                    let slots: Vec<Slot> = consumers(&e, p, 0).into_iter().filter(|(c, _)| feeds.contains(c)).collect();
                    for s in &slots {
                        set_input(&mut e, s, json!(""));
                    }
                    slots
                }
                _ => {
                    let slot: Slot = (tg.clone(), "prompt".into());
                    set_input(&mut e, &slot, json!(""));
                    vec![slot]
                }
            };
            if let NegativeSlot::Text(slots) = &negative {
                for s in slots {
                    set_input(&mut e, s, json!(""));
                }
            }
            let seed = inputs(&e[tg]).and_then(|i| i.get("sampling_mode.seed")).map(|_| (tg.clone(), "sampling_mode.seed".to_owned()));
            finish(e, output, info).ok().map(|graph| Enhanced { graph, prompt: slots, seed })
        }
        None => None,
    };
    for slot in &prompt {
        set_input(&mut g, slot, json!(""));
    }
    if let NegativeSlot::Text(slots) = &negative {
        for s in slots {
            set_input(&mut g, s, json!(""));
        }
    }
    let graph = finish(g, output, info)?;
    Ok(Canonical { graph, output: output.to_owned(), mode, prompt, negative, negative_default, enhanced })
}

/// The canonical graphs of a converted template: one, or a create and an edit graph when the
/// template switches between them.
pub fn canonicalise(conv: &Converted, opts: &Options) -> Result<Vec<Canonical>, String> {
    let info = opts.info;
    let mut g = conv.graph.clone();
    fill_defaults(&mut g, info);
    let output = choose_output(&g, opts).ok_or("no saved image uses a sampler and the model")?;
    rewrite_output(&mut g, &output);
    let up = ancestors(&g, &output);
    match mode_switch(&g, info, &up) {
        Some((sw, edit_side)) => {
            let mut create = g.clone();
            set_switch(&mut create, info, &sw, !edit_side);
            let create = one_mode(create, &output, Some("create".into()), opts)?;
            // Only a real text-only mode counts: a switch between two image-conditioned
            // variants (a fixed size or the source's size) is left at the template's value.
            if !roles::image_loaders(&create.graph, info).is_empty() {
                return Ok(vec![one_mode(g, &output, None, opts)?]);
            }
            let mut edit = g;
            set_switch(&mut edit, info, &sw, edit_side);
            Ok(vec![create, one_mode(edit, &output, Some("edit".into()), opts)?])
        }
        None => Ok(vec![one_mode(g, &output, None, opts)?]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::ui::Converted;

    fn info() -> ObjectInfo {
        ObjectInfo(json!({
            "UNETLoader": {"input": {"required": {"unet_name": [["m.safetensors"]], "weight_dtype": [["default", "fp8"]]}}, "input_order": {"required": ["unet_name", "weight_dtype"]}, "output": ["MODEL"]},
            "CLIPLoader": {"input": {"required": {"clip_name": [["c.safetensors"]], "type": [["flux3"]]}}, "output": ["CLIP"]},
            "CLIPTextEncode": {"input": {"required": {"text": ["STRING", {"multiline": true}], "clip": ["CLIP"]}}, "input_order": {"required": ["text", "clip"]}, "output": ["CONDITIONING"]},
            "TextGenerate": {"input": {"required": {"clip": ["CLIP"], "prompt": ["STRING", {"multiline": true}]}}, "input_order": {"required": ["clip", "prompt"]}, "output": ["STRING", "STRING"]},
            "PrimitiveStringMultiline": {"input": {"required": {"value": ["STRING", {"multiline": true}]}}, "output": ["STRING"], "python_module": "comfy_extras.nodes_primitive"},
            "ComfySwitchNode": {"input": {"required": {"switch": ["BOOLEAN"]}, "optional": {"on_false": ["COMFY_MATCHTYPE_V3"], "on_true": ["COMFY_MATCHTYPE_V3"]}}, "output": ["COMFY_MATCHTYPE_V3"]},
            "PreviewAny": {"input": {"required": {"source": ["*"]}}, "output": ["STRING"], "output_node": true},
            "ConditioningZeroOut": {"input": {"required": {"conditioning": ["CONDITIONING"]}}, "output": ["CONDITIONING"]},
            "EmptyLatentImage": {"input": {"required": {"width": ["INT", {"default": 1024}], "height": ["INT", {"default": 1024}], "batch_size": ["INT", {"default": 1}]}}, "output": ["LATENT"]},
            "KSampler": {"input": {"required": {"model": ["MODEL"], "seed": ["INT", {"control_after_generate": true}], "steps": ["INT", {"default": 20}], "cfg": ["FLOAT", {"default": 8.0}],
                "sampler_name": [["euler"]], "scheduler": [["simple"]], "positive": ["CONDITIONING"], "negative": ["CONDITIONING"], "latent_image": ["LATENT"], "denoise": ["FLOAT", {"default": 1.0}]}}, "output": ["LATENT"]},
            "VAEDecode": {"input": {"required": {"samples": ["LATENT"], "vae": ["VAE"]}}, "output": ["IMAGE"]},
            "VAELoader": {"input": {"required": {"vae_name": [["v.safetensors"]]}}, "output": ["VAE"]},
            "SaveImage": {"input": {"required": {"images": ["IMAGE"], "filename_prefix": ["STRING", {"default": "ComfyUI"}]}}, "output": [], "output_node": true}
        }))
    }

    fn template() -> Map<String, Value> {
        json!({
            "1": {"class_type": "UNETLoader", "inputs": {"unet_name": "m.safetensors"}},
            "2": {"class_type": "CLIPLoader", "inputs": {"clip_name": "c.safetensors", "type": "flux3"}},
            "3": {"class_type": "PrimitiveStringMultiline", "inputs": {"value": "a fox"}},
            "4": {"class_type": "CLIPLoader", "inputs": {"clip_name": "llm.safetensors", "type": "flux3"}},
            "5": {"class_type": "TextGenerate", "inputs": {"clip": ["4", 0], "prompt": ["3", 0]}},
            "6": {"class_type": "ComfySwitchNode", "inputs": {"switch": true, "on_false": ["3", 0], "on_true": ["5", 0]}},
            "7": {"class_type": "PreviewAny", "inputs": {"source": ["6", 0]}},
            "8": {"class_type": "CLIPTextEncode", "inputs": {"text": ["7", 0], "clip": ["2", 0]}},
            "9": {"class_type": "ConditioningZeroOut", "inputs": {"conditioning": ["8", 0]}},
            "10": {"class_type": "EmptyLatentImage", "inputs": {}},
            "11": {"class_type": "KSampler", "inputs": {"model": ["1", 0], "seed": 3, "positive": ["8", 0], "negative": ["9", 0], "latent_image": ["10", 0]}},
            "12": {"class_type": "VAELoader", "inputs": {"vae_name": "v.safetensors"}},
            "13": {"class_type": "VAEDecode", "inputs": {"samples": ["11", 0], "vae": ["12", 0]}},
            "14": {"class_type": "SaveImage", "inputs": {"images": ["13", 0]}},
            "15": {"class_type": "PreviewAny", "inputs": {"source": ["5", 0]}}
        })
        .as_object()
        .unwrap()
        .clone()
    }

    #[test]
    fn prompt_chains_are_cut_and_the_enhancer_kept_aside() {
        let conv = Converted { graph: template(), ..Default::default() };
        let opts = Options { info: &info(), io_outputs: &[], main_stem: Some("m"), zero: roles::ZERO_CLASSES };
        let c = canonicalise(&conv, &opts).unwrap().remove(0);
        assert_eq!(c.output, "14");
        assert_eq!(c.prompt, vec![("8".to_owned(), "text".to_owned())]);
        assert_eq!(c.negative, NegativeSlot::Zeroed);
        assert_eq!(c.graph["8"]["inputs"]["text"], "", "the switch-on enhancer no longer feeds the encoder");
        for gone in ["3", "4", "5", "6", "7", "15"] {
            assert!(!c.graph.contains_key(gone), "{gone} pruned");
        }
        assert_eq!(c.graph["11"]["inputs"]["steps"], 20, "defaults filled");
        assert_eq!(c.graph["10"]["inputs"]["width"], 1024);
        let e = c.enhanced.unwrap();
        assert_eq!(e.prompt, vec![("5".to_owned(), "prompt".to_owned())], "the template's prompt primitive fed the enhancer here");
        assert_eq!(e.graph["8"]["inputs"]["text"], json!(["5", 0]), "the enhancer feeds the encoder directly");
        assert_eq!(e.graph["5"]["inputs"]["prompt"], "");
        assert!(e.graph.contains_key("4") && !e.graph.contains_key("6"));
    }
}
