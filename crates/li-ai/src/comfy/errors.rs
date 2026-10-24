//! Plain-language ComfyUI errors. An execution or validation error is matched against the
//! `error_patterns` of `runtime.json` (message text, failing node class, whether the failing input
//! loads a file) and explained with the files the submitted graph names. The original message is
//! kept as `detail`; an unmatched error reads "ComfyUI could not finish: <message>".

use std::collections::BTreeMap;

use serde_json::Value;

use crate::companions::{ErrorPattern, folder_of};

/// What ComfyUI reported about a failed node.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecError {
    pub node_type: String,
    pub node_id: String,
    pub exception_type: String,
    pub message: String,
    /// The input a validation error is about (`unet_name`), and the value it received.
    pub input: String,
    pub value: String,
}

/// An explained error: a stable code, the sentence shown to people, and ComfyUI's own text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FriendlyError {
    pub code: String,
    pub message: String,
    pub detail: String,
    pub exec: ExecError,
    /// The submitted graph and server version, so a caller that knows the recipe can explain
    /// the error again with the template's declared files ([`FriendlyError::with_recipe`]).
    pub graph: Value,
    pub comfyui_version: Option<String>,
}

impl FriendlyError {
    /// The same error explained with a recipe's declared files (`role → file`) and template.
    pub fn with_recipe(&self, declared: BTreeMap<String, String>, template: Option<String>) -> FriendlyError {
        let graph = (!self.graph.is_null()).then_some(&self.graph);
        let again = translate(&self.exec, &RunContext { graph, declared, template, comfyui_version: self.comfyui_version.clone(), gpu: None });
        if again.code == "Fallback" { self.clone() } else { again }
    }
}

impl std::fmt::Display for FriendlyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FriendlyError {}

/// What a run knows besides the error.
#[derive(Clone, Debug, Default)]
pub struct RunContext<'a> {
    /// The submitted API graph (names the files).
    pub graph: Option<&'a Value>,
    /// Files the recipe's template declares, by role (`clip`, `vae`); empty for builder graphs.
    pub declared: BTreeMap<String, String>,
    /// The recipe's template, for "the recipe doesn't match" errors.
    pub template: Option<String>,
    pub comfyui_version: Option<String>,
    pub gpu: Option<String>,
}

pub const OUT_OF_MEMORY: &str = "The GPU ran out of memory. Try a smaller size, the compact model, or close other GPU work.";

fn pattern_matches(p: &ErrorPattern, e: &ExecError, lower: &str) -> bool {
    p.contains.iter().any(|c| lower.contains(&c.to_ascii_lowercase()))
        && (p.classes.is_empty() || e.node_type.is_empty() || p.classes.contains(&e.node_type))
        && p.file_input.is_none_or(|want| want == crate::graph::validate::is_file_slot(&e.input, &Value::String(e.value.clone())))
}

fn node_inputs<'a>(graph: Option<&'a Value>, id: &str) -> Option<&'a serde_json::Map<String, Value>> {
    graph?.get(id)?.get("inputs")?.as_object()
}

/// The first string value of `input` on a node of one of `classes`.
fn file_of(graph: Option<&Value>, classes: &[&str], input: &str) -> Option<String> {
    let g = graph?.as_object()?;
    let mut ids: Vec<&String> = g.keys().collect();
    ids.sort();
    ids.into_iter().find_map(|id| {
        let n = &g[id];
        classes.contains(&n.get("class_type")?.as_str()?).then(|| n["inputs"].get(input)?.as_str().map(str::to_owned)).flatten()
    })
}

fn main_file(graph: Option<&Value>) -> Option<String> {
    file_of(graph, &["UNETLoader", "UnetLoaderGGUF"], "unet_name").or_else(|| file_of(graph, &["CheckpointLoaderSimple"], "ckpt_name"))
}

fn text_encoders(graph: Option<&Value>) -> Vec<String> {
    let Some(g) = graph.and_then(Value::as_object) else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    for n in g.values() {
        let class = n.get("class_type").and_then(Value::as_str).unwrap_or("");
        if !class.starts_with("CLIPLoader") && !class.starts_with("DualCLIPLoader") && !class.starts_with("TripleCLIPLoader") {
            continue;
        }
        for (k, v) in n["inputs"].as_object().into_iter().flatten() {
            if let (true, Some(s)) = (k.starts_with("clip_name"), v.as_str())
                && !out.iter().any(|x| x == s)
            {
                out.push(s.to_owned());
            }
        }
    }
    out.sort();
    out
}

/// The file the failing node loads, else the graph's main model.
fn failing_file(e: &ExecError, ctx: &RunContext) -> Option<String> {
    let ins = node_inputs(ctx.graph, &e.node_id);
    ins.and_then(|ins| {
        ins.iter().find(|(k, v)| crate::graph::validate::is_file_slot(k, v) && k.as_str() != "image").and_then(|(_, v)| v.as_str().map(str::to_owned))
    })
    .or_else(|| main_file(ctx.graph))
}

fn mismatch(e: &ExecError, ctx: &RunContext, lower: &str) -> (String, String) {
    let model = main_file(ctx.graph).unwrap_or_else(|| "this model".into());
    let te = text_encoders(ctx.graph);
    let vae = file_of(ctx.graph, &["VAELoader"], "vae_name");
    let is_vae = e.node_type.starts_with("VAE") || lower.contains("channels, but got");
    let is_te = lower.contains("mat1 and mat2") || e.node_type.contains("CLIP") || e.node_type.contains("TextEncode");
    match (is_vae, is_te, ctx.declared.get("vae"), ctx.declared.get("clip"), &vae, te.first()) {
        (true, _, Some(declared), _, Some(vae), _) => {
            ("ComponentMismatch:vae".into(), format!("The VAE {vae} doesn't fit {model}'s images. Use {declared} (or an equivalent)."))
        }
        (false, true, _, Some(declared), _, Some(te)) => {
            ("ComponentMismatch:te".into(), format!("The text encoder {te} doesn't fit {model}. It needs the encoder its template names: {declared}."))
        }
        _ => {
            let files: Vec<String> = te.into_iter().chain(vae).collect();
            let files = if files.is_empty() { "its companion files".to_owned() } else { files.join(", ") };
            ("ComponentMismatch".into(), format!("The text encoder or VAE ({files}) doesn't match {model}. Open Set Up Models to pick matching files."))
        }
    }
}

/// Explains `e` (the original message stays in `detail`).
pub fn translate(e: &ExecError, ctx: &RunContext) -> FriendlyError {
    let lower = e.message.to_ascii_lowercase();
    let code = crate::companions::runtime().error_patterns.iter().find(|p| pattern_matches(p, e, &lower)).map(|p| p.code.as_str());
    let file = || failing_file(e, ctx).unwrap_or_else(|| "this model file".into());
    let (code, message) = match code {
        Some("OutOfMemory") => ("OutOfMemory".to_owned(), OUT_OF_MEMORY.to_owned()),
        Some("ComfyCantReadModel") => {
            let have = ctx.comfyui_version.as_ref().map(|v| format!(" ({v})")).unwrap_or_default();
            (
                "ComfyCantReadModel".into(),
                format!(
                    "This ComfyUI{have} doesn't recognise {}. Update ComfyUI, or check that the file is a diffusion model and not a LoRA or text encoder.",
                    file()
                ),
            )
        }
        Some("ComponentMismatch") => mismatch(e, ctx, &lower),
        Some("QuantUnsupported") => {
            let f = main_file(ctx.graph).unwrap_or_else(file);
            let quant = crate::stem::name_quant(&crate::stem::stem_key(&f).quant_tokens).map(|q| q.label()).unwrap_or_else(|| "its precision".into());
            let gpu = ctx.gpu.as_ref().map(|g| format!(" ({g})")).unwrap_or_default();
            ("QuantUnsupported:run".into(), format!("{f} uses {quant}, which this GPU{gpu} can't run. Use a BF16 or FP8 version."))
        }
        Some("FileDamaged") => ("FileDamaged".into(), format!("{} is damaged or incomplete. Delete it and download it again.", file())),
        Some("ComfyCantSeeFile") => {
            let folder = folder_of(&e.node_type, &e.input);
            let folder = if folder.is_empty() { "its model folder".to_owned() } else { format!("{folder}/") };
            let f = if e.value.is_empty() { file() } else { e.value.clone() };
            ("ComfyCantSeeFile".into(), format!("ComfyUI can't see {f} in {folder}. Wait a few seconds for it to rescan, or restart ComfyUI."))
        }
        Some("ValueNotOffered") => {
            ("ValueNotOffered:run".into(), format!("This ComfyUI doesn't offer {} {} \"{}\". Update ComfyUI.", e.node_type, e.input, e.value))
        }
        Some("TemplateUnusable") if ctx.template.is_some() => (
            "TemplateUnusable:run".into(),
            format!(
                "The {} recipe doesn't match this ComfyUI ({}). Open Set Up Models and press Re-check.",
                ctx.template.as_deref().unwrap_or_default(),
                e.message.trim()
            ),
        ),
        _ => ("Fallback".into(), format!("ComfyUI could not finish: {}", e.message)),
    };
    FriendlyError {
        code,
        message,
        detail: e.message.clone(),
        exec: e.clone(),
        graph: ctx.graph.cloned().unwrap_or(Value::Null),
        comfyui_version: ctx.comfyui_version.clone(),
    }
}

/// The error of a history entry (`execution_error` payload), if it failed.
pub fn from_history(entry: &Value) -> Option<ExecError> {
    let status = entry.get("status")?;
    for m in status.get("messages").and_then(Value::as_array).into_iter().flatten() {
        let kind = m.get(0).and_then(Value::as_str).unwrap_or("");
        if kind == "execution_error" || kind == "execution_interrupted" {
            let data = m.get(1).cloned().unwrap_or(Value::Null);
            let s = |k: &str| data.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
            let node_id = match data.get("node_id") {
                Some(Value::String(s)) => s.clone(),
                Some(v @ Value::Number(_)) => v.to_string(),
                _ => String::new(),
            };
            let message = data.get("exception_message").and_then(Value::as_str).unwrap_or(kind).trim().to_owned();
            return Some(ExecError { node_type: s("node_type"), node_id, exception_type: s("exception_type"), message, ..Default::default() });
        }
    }
    (status.get("status_str").and_then(Value::as_str) == Some("error")).then(|| ExecError { message: "ComfyUI reported an error".into(), ..Default::default() })
}

/// The node errors of a rejected `/prompt` (`node_errors`), one per failing input.
pub fn from_rejection(v: &Value) -> Vec<ExecError> {
    let mut out = Vec::new();
    for (id, e) in v.get("node_errors").and_then(Value::as_object).into_iter().flatten() {
        let class = e.get("class_type").and_then(Value::as_str).unwrap_or("");
        for err in e.get("errors").and_then(Value::as_array).into_iter().flatten() {
            let text = |k: &str| err.get(k).and_then(Value::as_str).unwrap_or("");
            let extra = err.get("extra_info");
            let input = extra.and_then(|x| x.get("input_name")).and_then(Value::as_str).unwrap_or("").to_owned();
            let value = match extra.and_then(|x| x.get("received_value")) {
                Some(Value::String(s)) => s.clone(),
                Some(v) if !v.is_null() => v.to_string(),
                _ => String::new(),
            };
            let message = [text("message"), text("details")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(": ");
            out.push(ExecError { node_type: class.to_owned(), node_id: id.clone(), exception_type: text("type").to_owned(), message, input, value });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn graph() -> Value {
        json!({
            "1": {"class_type": "UNETLoader", "inputs": {"unet_name": "flux3-image-klein-bf16.safetensors", "weight_dtype": "default"}},
            "2": {"class_type": "CLIPLoader", "inputs": {"clip_name": "qwen3vl_4b_bf16.safetensors", "type": "flux3"}},
            "3": {"class_type": "VAELoader", "inputs": {"vae_name": "flux2-vae.safetensors"}},
            "4": {"class_type": "SamplerCustomAdvanced", "inputs": {}},
            "5": {"class_type": "VAEDecode", "inputs": {}}
        })
    }

    fn exec(node_type: &str, node_id: &str, message: &str) -> ExecError {
        ExecError { node_type: node_type.into(), node_id: node_id.into(), message: message.into(), ..Default::default() }
    }

    #[test]
    fn a_model_comfyui_cannot_read_asks_for_an_update_and_names_the_file() {
        let g = graph();
        let ctx = RunContext { graph: Some(&g), comfyui_version: Some("0.38.0".into()), ..Default::default() };
        let f = translate(&exec("UNETLoader", "1", "ERROR: Could not detect model type of: /models/x.safetensors"), &ctx);
        assert_eq!(f.code, "ComfyCantReadModel");
        assert!(f.message.starts_with("This ComfyUI (0.38.0) doesn't recognise flux3-image-klein-bf16.safetensors. Update ComfyUI"), "{}", f.message);
        assert!(f.detail.contains("Could not detect model type"));
        let bare = translate(&exec("", "", "Could not detect model type of: x"), &RunContext::default());
        assert!(bare.message.contains("Update ComfyUI"), "{}", bare.message);
    }

    #[test]
    fn shape_mismatches_name_the_installed_and_declared_companions() {
        let g = graph();
        let declared = [("clip".to_owned(), "qwen3vl_4b_int8_convrot.safetensors".to_owned()), ("vae".to_owned(), "flux2-vae.safetensors".to_owned())];
        let ctx = RunContext { graph: Some(&g), declared: declared.into(), ..Default::default() };
        let te = translate(&exec("SamplerCustomAdvanced", "4", "mat1 and mat2 shapes cannot be multiplied (77x2560 and 7680x3072)"), &ctx);
        assert_eq!(te.code, "ComponentMismatch:te");
        assert_eq!(
            te.message,
            "The text encoder qwen3vl_4b_bf16.safetensors doesn't fit flux3-image-klein-bf16.safetensors. It needs the encoder its template names: qwen3vl_4b_int8_convrot.safetensors."
        );
        let vae = translate(&exec("VAEDecode", "5", "Error(s) in loading state_dict for AutoencodingEngine: size mismatch"), &ctx);
        assert_eq!(vae.code, "ComponentMismatch:vae");
        // A 16-channel VAE decoding a 128-channel latent (measured on ComfyUI 0.42.5).
        let channels = "Given groups=1, weight of size [512, 16, 3, 3], expected input[1, 128, 64, 64] to have 16 channels, but got 128 channels instead";
        assert_eq!(translate(&exec("VAEDecode", "5", channels), &ctx).code, "ComponentMismatch:vae");
        // A builder graph declares nothing: the generic sentence lists both companions.
        let plain = RunContext { graph: Some(&g), ..Default::default() };
        let generic = translate(&exec("SamplerCustomAdvanced", "4", "mat1 and mat2 shapes cannot be multiplied"), &plain);
        assert_eq!(generic.code, "ComponentMismatch");
        assert!(generic.message.contains("qwen3vl_4b_bf16.safetensors, flux2-vae.safetensors"), "{}", generic.message);
        // A recipe run explains the same error with its template's files.
        assert_eq!(generic.with_recipe(ctx.declared.clone(), None).message, te.message);
    }

    #[test]
    fn rejected_values_split_into_unseen_files_and_missing_options() {
        let rejection = json!({"error": {"message": "Prompt outputs failed validation"}, "node_errors": {
            "1": {"class_type": "UNETLoader", "errors": [{"type": "value_not_in_list", "message": "Value not in list",
                  "details": "unet_name: 'new.safetensors' not in [...]", "extra_info": {"input_name": "unet_name", "received_value": "new.safetensors"}}]},
            "2": {"class_type": "CLIPLoader", "errors": [{"type": "value_not_in_list", "message": "Value not in list",
                  "details": "type: 'flux3' not in [...]", "extra_info": {"input_name": "type", "received_value": "flux3"}}]}
        }});
        let errs = from_rejection(&rejection);
        let out: Vec<FriendlyError> = errs.iter().map(|e| translate(e, &RunContext::default())).collect();
        assert_eq!(out[0].message, "ComfyUI can't see new.safetensors in diffusion_models/. Wait a few seconds for it to rescan, or restart ComfyUI.");
        assert_eq!(out[1].message, "This ComfyUI doesn't offer CLIPLoader type \"flux3\". Update ComfyUI.");
    }

    #[test]
    fn history_errors_keep_the_failing_node() {
        let entry = json!({"status": {"status_str": "error", "messages": [["execution_error",
            {"node_id": "4", "node_type": "SamplerCustomAdvanced", "exception_type": "RuntimeError", "exception_message": " CUDA out of memory "}]]}});
        let e = from_history(&entry).unwrap();
        assert_eq!((e.node_id.as_str(), e.node_type.as_str(), e.message.as_str()), ("4", "SamplerCustomAdvanced", "CUDA out of memory"));
        assert_eq!(translate(&e, &RunContext::default()).message, OUT_OF_MEMORY);
        assert!(from_history(&json!({"status": {"status_str": "success", "messages": []}})).is_none());
    }
}
