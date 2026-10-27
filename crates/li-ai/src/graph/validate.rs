//! Static validation of an API graph against `/object_info`, the checks ComfyUI's `/prompt` makes
//! before it runs anything: every class exists, required inputs are present (a dynamic combo's
//! chosen option included), links point at real outputs, and literal values have the right type,
//! range and choice. A combo value that names a file is a binding problem (the file is missing on
//! this server), not a graph problem; a combo with no listed choices accepts anything.

use serde_json::{Map, Value};

use super::ui::{AUTOGROW, DYNAMIC_COMBO, MATCHTYPE, combo_option};
use super::{inputs, link_of};
use crate::comfy::ObjectInfo;

const FILE_EXTENSIONS: &[&str] = &[".safetensors", ".gguf", ".sft", ".ckpt", ".pt", ".pth", ".bin", ".onnx", ".png", ".jpg", ".jpeg", ".webp"];

#[derive(Clone, Debug, PartialEq)]
pub enum ProblemKind {
    MissingClass,
    MissingInput,
    BadLink(String),
    WrongType(String),
    OutOfRange(String),
    /// A choice this ComfyUI doesn't offer.
    NotOffered(String),
    /// A file this ComfyUI doesn't list for this loader.
    FileNotListed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub node: String,
    pub class: String,
    pub input: String,
    pub kind: ProblemKind,
}

impl Problem {
    pub fn describe(&self) -> String {
        let at = if self.input.is_empty() { format!("{} ({})", self.node, self.class) } else { format!("{} ({}).{}", self.node, self.class, self.input) };
        match &self.kind {
            ProblemKind::MissingClass => format!("{at}: this ComfyUI has no node {}", self.class),
            ProblemKind::MissingInput => format!("{at}: required input missing"),
            ProblemKind::BadLink(d) => format!("{at}: {d}"),
            ProblemKind::WrongType(t) => format!("{at}: expected {t}"),
            ProblemKind::OutOfRange(d) => format!("{at}: {d}"),
            ProblemKind::NotOffered(v) => format!("{at}: \"{v}\" is not offered"),
            ProblemKind::FileNotListed(v) => format!("{at}: {v} is not listed"),
        }
    }
    /// A missing or misplaced file (resolved by binding, not by changing the graph).
    pub fn is_file(&self) -> bool {
        matches!(self.kind, ProblemKind::FileNotListed(_))
    }
}

/// Whether a combo input holds a file name (a loader's file input or a value with a file
/// extension).
pub fn is_file_slot(input: &str, value: &Value) -> bool {
    let last = input.rsplit('.').next().unwrap_or(input);
    matches!(last, "unet_name" | "ckpt_name" | "vae_name" | "lora_name" | "model_name" | "style_model_name" | "control_net_name" | "image")
        || last.starts_with("clip_name")
        || value.as_str().is_some_and(|v| FILE_EXTENSIONS.iter().any(|e| v.to_ascii_lowercase().ends_with(e)))
}

fn combo_choices(spec: &Value) -> Option<Vec<Value>> {
    match spec.get(0) {
        Some(Value::Array(a)) => Some(a.clone()),
        Some(Value::String(t)) if t == "COMBO" => Some(spec.get(1).and_then(|o| o.get("options")).and_then(Value::as_array).cloned().unwrap_or_default()),
        _ => None,
    }
}

fn check_value(node: &str, class: &str, input: &str, spec: &Value, v: &Value, out: &mut Vec<Problem>) {
    let problem = |kind| Problem { node: node.into(), class: class.into(), input: input.into(), kind };
    let opts = spec.get(1);
    let bound = |k: &str| opts.and_then(|o| o.get(k)).and_then(Value::as_f64);
    let ty = spec.get(0).and_then(Value::as_str).unwrap_or("");
    if let Some(choices) = combo_choices(spec) {
        if choices.is_empty() || choices.contains(v) {
            return;
        }
        let text = v.as_str().map_or_else(|| v.to_string(), str::to_owned);
        out.push(problem(if is_file_slot(input, v) { ProblemKind::FileNotListed(text) } else { ProblemKind::NotOffered(text) }));
        return;
    }
    match ty {
        "INT" | "FLOAT" => {
            let Some(x) = v.as_f64() else {
                out.push(problem(ProblemKind::WrongType(ty.to_owned())));
                return;
            };
            if ty == "INT" && x.fract() != 0.0 {
                out.push(problem(ProblemKind::WrongType("INT".into())));
            } else if bound("min").is_some_and(|m| x < m) || bound("max").is_some_and(|m| x > m) {
                out.push(problem(ProblemKind::OutOfRange(format!(
                    "{x} is outside {}..{}",
                    bound("min").unwrap_or(f64::MIN),
                    bound("max").unwrap_or(f64::MAX)
                ))));
            }
        }
        "BOOLEAN" if !v.is_boolean() => out.push(problem(ProblemKind::WrongType("BOOLEAN".into()))),
        "STRING" if !v.is_string() => out.push(problem(ProblemKind::WrongType("STRING".into()))),
        _ => {}
    }
}

/// The node being checked.
struct Node<'a> {
    info: &'a ObjectInfo,
    g: &'a Map<String, Value>,
    id: &'a str,
    class: &'a str,
}

/// Checks one input path (a top-level input or a dynamic combo's nested input).
fn check_input(n: &Node, path: &str, spec: &Value, required: bool, out: &mut Vec<Problem>) {
    let Node { info, g, id, class } = *n;
    let ins = inputs(&g[id]);
    let value = ins.and_then(|i| i.get(path));
    let ty = spec.get(0).and_then(Value::as_str).unwrap_or("");
    let problem = |kind| Problem { node: id.into(), class: class.into(), input: path.into(), kind };
    match ty {
        AUTOGROW | MATCHTYPE => {
            let min = spec.get(1).and_then(|o| o.get("template")).and_then(|t| t.get("min")).and_then(Value::as_u64).unwrap_or(0);
            let present = ins.map_or(0, |i| i.keys().filter(|k| k.starts_with(&format!("{path}."))).count() as u64);
            if required && ty == AUTOGROW && present < min {
                out.push(problem(ProblemKind::MissingInput));
            }
            if ty == MATCHTYPE && required && value.is_none() {
                out.push(problem(ProblemKind::MissingInput));
            }
            return;
        }
        _ => {}
    }
    let Some(v) = value else {
        if required {
            out.push(problem(ProblemKind::MissingInput));
        }
        return;
    };
    if let Some((src, slot)) = link_of(v) {
        match g.get(src) {
            None => out.push(problem(ProblemKind::BadLink(format!("links to missing node {src}")))),
            Some(n) => {
                let src_class = n["class_type"].as_str().unwrap_or("");
                let outputs = info.outputs(src_class);
                if info.has_node(src_class) && slot as usize >= outputs.len() {
                    out.push(problem(ProblemKind::BadLink(format!("{src} ({src_class}) has no output {slot}"))));
                }
            }
        }
        return;
    }
    if ty == DYNAMIC_COMBO {
        let key = v.as_str().unwrap_or("");
        let Some(option) = combo_option(spec, key) else {
            out.push(problem(ProblemKind::NotOffered(key.to_owned())));
            return;
        };
        for (section, req) in [("required", true), ("optional", false)] {
            for (name, nspec) in option.get(section).and_then(Value::as_object).into_iter().flatten() {
                check_input(n, &format!("{path}.{name}"), nspec, req, out);
            }
        }
        return;
    }
    check_value(id, class, path, spec, v, out);
}

/// Every problem ComfyUI would reject `g` for.
pub fn validate(g: &Map<String, Value>, info: &ObjectInfo) -> Vec<Problem> {
    let mut out = Vec::new();
    for id in super::sorted_ids(g) {
        let class = g[&id]["class_type"].as_str().unwrap_or("").to_owned();
        let Some(spec) = info.spec(&class) else {
            out.push(Problem { node: id.clone(), class, input: String::new(), kind: ProblemKind::MissingClass });
            continue;
        };
        let node = Node { info, g, id: &id, class: &class };
        for (section, required) in [("required", true), ("optional", false)] {
            for (name, ispec) in spec.get("input").and_then(|i| i.get(section)).and_then(Value::as_object).into_iter().flatten() {
                check_input(&node, name, ispec, required, &mut out);
            }
        }
        // Links on names the class doesn't declare (autogrow entries are checked by prefix).
        for (k, v) in inputs(&g[&id]).into_iter().flatten() {
            if let Some((src, _)) = link_of(v)
                && !g.contains_key(src)
                && info.input_spec(&class, k).is_none()
            {
                out.push(Problem {
                    node: id.clone(),
                    class: class.clone(),
                    input: k.clone(),
                    kind: ProblemKind::BadLink(format!("links to missing node {src}")),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn info() -> ObjectInfo {
        ObjectInfo(json!({
            "KSampler": {"input": {"required": {"model": ["MODEL"], "seed": ["INT", {"min": 0}], "steps": ["INT", {"min": 1, "max": 10000}], "cfg": ["FLOAT", {"min": 0.0, "max": 100.0}],
                "sampler_name": [["euler", "er_sde"]], "scheduler": [["simple"]], "positive": ["CONDITIONING"], "negative": ["CONDITIONING"], "latent_image": ["LATENT"], "denoise": ["FLOAT", {"min": 0.0, "max": 1.0}]}},
                "output": ["LATENT"]},
            "UNETLoader": {"input": {"required": {"unet_name": [["a.safetensors"]], "weight_dtype": [["default"]]}}, "output": ["MODEL"]},
            "CustomCombo": {"input": {"required": {"choice": ["COMBO", {"options": []}]}}, "output": ["STRING"]},
            "ReferenceMultiLatent": {"input": {"required": {"positive": ["CONDITIONING"], "resolution": ["COMFY_DYNAMICCOMBO_V3", {"options": [{"key": "keep", "inputs": {"required": {}}},
                {"key": "resize", "inputs": {"required": {"megapixels": ["FLOAT", {"min": 0.01, "max": 16.0}]}}}]}]}}, "output": ["CONDITIONING", "CONDITIONING", "LATENT"]}
        }))
    }

    #[test]
    fn graphs_are_checked_like_comfyui_checks_prompts() {
        let g = json!({
            "1": {"class_type": "UNETLoader", "inputs": {"unet_name": "b.safetensors", "weight_dtype": "default"}},
            "2": {"class_type": "KSampler", "inputs": {"model": ["1", 0], "seed": 1, "steps": 0, "cfg": "high", "sampler_name": "nope", "scheduler": "simple",
                "positive": ["9", 0], "negative": ["3", 2], "latent_image": ["3", 2]}},
            "3": {"class_type": "ReferenceMultiLatent", "inputs": {"positive": ["1", 1], "resolution": "resize"}},
            "4": {"class_type": "CustomCombo", "inputs": {"choice": "anything"}},
            "5": {"class_type": "Unheard", "inputs": {}}
        });
        let problems = validate(g.as_object().unwrap(), &info());
        let has = |node: &str, input: &str, kind: &dyn Fn(&ProblemKind) -> bool| problems.iter().any(|p| p.node == node && p.input == input && kind(&p.kind));
        assert!(has("1", "unet_name", &|k| matches!(k, ProblemKind::FileNotListed(v) if v == "b.safetensors")));
        assert!(has("2", "steps", &|k| matches!(k, ProblemKind::OutOfRange(_))));
        assert!(has("2", "cfg", &|k| matches!(k, ProblemKind::WrongType(t) if t == "FLOAT")));
        assert!(has("2", "sampler_name", &|k| matches!(k, ProblemKind::NotOffered(v) if v == "nope")));
        assert!(has("2", "positive", &|k| matches!(k, ProblemKind::BadLink(_))));
        assert!(has("2", "denoise", &|k| *k == ProblemKind::MissingInput));
        assert!(has("3", "positive", &|k| matches!(k, ProblemKind::BadLink(d) if d.contains("no output 1"))));
        assert!(has("3", "resolution.megapixels", &|k| *k == ProblemKind::MissingInput));
        assert!(!problems.iter().any(|p| p.node == "4"), "free combos accept any value");
        assert!(has("5", "", &|k| *k == ProblemKind::MissingClass));
        assert!(!problems.iter().any(|p| p.node == "2" && p.input == "negative"), "slot 2 of ReferenceMultiLatent exists");
    }
}
