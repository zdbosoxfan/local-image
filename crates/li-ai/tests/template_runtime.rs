//! Official templates (ComfyUI 0.42.5, `tests/fixtures/comfy-0.42.5`) through the graph engine:
//! UI-to-API conversion values, canonicalisation and validation of every committed template.

use std::path::PathBuf;

use li_ai::comfy::ObjectInfo;
use li_ai::graph::{self, canon, ui, validate};
use li_ai::template::roles;
use serde_json::{Map, Value, json};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/comfy-0.42.5")
}

fn info() -> ObjectInfo {
    ObjectInfo(serde_json::from_slice(&std::fs::read(fixtures().join("object_info.min.json")).unwrap()).unwrap())
}

fn template(name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(fixtures().join("templates").join(format!("{name}.json"))).unwrap()).unwrap()
}

const ALL: &[&str] = &[
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

/// The stem key of the template's first declared main model file.
fn main_stem(t: &Value) -> Option<String> {
    let nodes = t["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(t["definitions"]["subgraphs"].as_array().into_iter().flatten().flat_map(|d| d["nodes"].as_array().into_iter().flatten()));
    nodes
        .flat_map(|n| n["properties"]["models"].as_array().cloned().unwrap_or_default())
        .find(|m| matches!(m["directory"].as_str(), Some("diffusion_models" | "unet" | "checkpoints")))
        .and_then(|m| m["name"].as_str().map(|n| li_ai::stem::stem_key(n).key))
}

fn converted(name: &str) -> ui::Converted {
    ui::to_api(&template(name), &info()).unwrap_or_else(|e| panic!("{name}: {e:#}"))
}

fn canonical(name: &str) -> Vec<canon::Canonical> {
    let t = template(name);
    let stem = main_stem(&t);
    let info = info();
    let opts = canon::Options { info: &info, io_outputs: &[], main_stem: stem.as_deref(), zero: roles::ZERO_CLASSES };
    canon::canonicalise(&converted(name), &opts).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn node_of<'a>(g: &'a Map<String, Value>, class: &str) -> &'a Value {
    g.values().find(|n| n["class_type"] == class).unwrap_or_else(|| panic!("no {class}"))
}

#[test]
fn instance_values_override_stale_inner_widgets() {
    let base = converted("image_flux3_klein_t2i").graph;
    let k = &node_of(&base, "KSampler")["inputs"];
    assert_eq!((&k["steps"], &k["cfg"], &k["sampler_name"], &k["scheduler"]), (&json!(50), &json!(3.5), &json!("euler"), &json!("simple")));
    let distilled = converted("image_flux3_klein_distilled_t2i").graph;
    let k = &node_of(&distilled, "KSampler")["inputs"];
    assert_eq!((&k["steps"], &k["sampler_name"], &k["scheduler"]), (&json!(16), &json!("euler"), &json!("simple")));
    let qwen_edit = converted("image_qwen_image_2_1_turbo_image_edit").graph;
    assert_eq!(node_of(&qwen_edit, "CFGGuider")["inputs"]["cfg"], json!(1), "the instance's cfg 1, not the inner 8");
}

#[test]
fn ordered_instance_values_map_to_widget_inputs() {
    // The Krea style template's instance has no named values: 12 ordered values for its 12
    // widget-typed inputs (the three image inputs take none).
    let g = converted("image_krea2_turbo_int8_image_style_reference").graph;
    let ms = &node_of(&g, "ModelSamplingFlux")["inputs"];
    let latent = &node_of(&g, "EmptyLatentImage")["inputs"];
    assert_eq!((&ms["width"], &ms["height"]), (&latent["width"], &latent["height"]), "the shift follows the same size source as the latent");
    assert!(node_of(&g, "RandomNoise")["inputs"]["noise_seed"].as_u64().is_some());
    let lora = &node_of(&g, "LoraLoaderModelOnly")["inputs"];
    assert_eq!((&lora["lora_name"], &lora["strength_model"]), (&json!("krea2_style_reference.safetensors"), &json!(1)));
    assert_eq!(node_of(&g, "UNETLoader")["inputs"]["unet_name"], "krea2_turbo_int8_convrot.safetensors");
}

#[test]
fn dynamic_combos_take_their_option_and_nested_values() {
    let g = converted("image_flux3_klein_distilled_image_edit").graph;
    let r = &node_of(&g, "ReferenceMultiLatent")["inputs"];
    assert_eq!((&r["resolution"], r["resolution.megapixels"].as_f64()), (&json!("resize"), Some(1.0)));
    let t2i = converted("image_flux3_klein_distilled_t2i").graph;
    let tg = &node_of(&t2i, "TextGenerate")["inputs"];
    assert_eq!((&tg["sampling_mode"], &tg["sampling_mode.top_p"], &tg["sampling_mode.repetition_penalty"]), (&json!("on"), &json!(0.95), &json!(1.05)));
    assert!(tg.get("system_prompt").is_some_and(|v| graph::link_of(v).is_some()), "forceInput system_prompt stays a link");
}

#[test]
fn bypassed_instances_and_muted_branches_produce_no_nodes() {
    let g = converted("image_flux2_klein_image_edit_9b_base").graph;
    assert!(g.keys().all(|id| !id.starts_with("92:")), "the bypassed 2-reference instance has no inner nodes: {:?}", g.keys().collect::<Vec<_>>());
    assert!(g.keys().any(|id| id.starts_with("75:")));
}

#[test]
fn defaults_fill_inputs_older_templates_lack() {
    let mut g = converted("hidream_e1_1").graph;
    canon::fill_defaults(&mut g, &info());
    assert_eq!(node_of(&g, "ImageScaleToTotalPixels")["inputs"]["resolution_steps"], 1);
}

#[test]
fn every_template_canonicalises_and_validates() {
    let info = info();
    for name in ALL {
        for c in canonical(name) {
            let problems: Vec<String> = validate::validate(&c.graph, &info)
                .into_iter()
                .filter(|p| !p.is_file() && !(p.class == "LoadImage" && p.input == "image"))
                .map(|p| p.describe())
                .collect();
            assert!(problems.is_empty(), "{name} {:?}: {problems:?}", c.mode);
            for c2 in c.graph.values() {
                let class = c2["class_type"].as_str().unwrap();
                assert!(
                    !matches!(class, "TextGenerate" | "Flux3LayoutPrompt" | "MOSSImageLayoutPrompt" | "CreateBoundingBoxes" | "PreviewAny" | "ComfySwitchNode"),
                    "{name}: {class} survived"
                );
            }
            let titles: Vec<&str> = c.prompt.iter().filter_map(|(n, _)| c.graph[n]["_meta"]["title"].as_str()).collect();
            assert!(!titles.iter().any(|t| t.to_ascii_lowercase().contains("system")), "{name}: prompt slot is a system prompt");
            if let Some(e) = &c.enhanced {
                let problems: Vec<String> = validate::validate(&e.graph, &info).into_iter().filter(|p| !p.is_file()).map(|p| p.describe()).collect();
                assert!(problems.is_empty(), "{name} enhanced: {problems:?}");
                assert!(!e.prompt.is_empty(), "{name}");
            }
        }
    }
}

#[test]
fn upsamplers_are_kept_only_in_the_enhanced_variant() {
    for name in ["image_flux3_klein_distilled_t2i", "image_flux3_klein_t2i", "image_moss_image_t2i", "image_krea2_turbo_t2i", "image_qwen_image_2_1_turbo_t2i"]
    {
        let c = canonical(name).remove(0);
        assert!(c.enhanced.is_some(), "{name} offers a prompt enhancer");
    }
    assert!(canonical("hidream_e1_1").remove(0).enhanced.is_none());
}

#[test]
fn hidream_o1_splits_into_create_and_edit() {
    let modes: Vec<Option<String>> = canonical("image_hidream_o1").into_iter().map(|c| c.mode).collect();
    assert_eq!(modes, [Some("create".to_owned()), Some("edit".to_owned())]);
}

#[test]
fn unknown_classes_in_pruned_branches_do_not_fail() {
    let mut t = template("image_krea2_turbo_t2i");
    let extra = json!({"id": 9001, "type": "SomeCustomPreview", "mode": 0, "inputs": [], "outputs": [], "widgets_values": [1]});
    t["nodes"].as_array_mut().unwrap().push(extra);
    let info = info();
    let conv = ui::to_api(&t, &info).unwrap();
    assert_eq!(conv.unknown, vec![("9001".to_owned(), "SomeCustomPreview".to_owned())]);
    let stem = main_stem(&t);
    let c = canon::canonicalise(&conv, &canon::Options { info: &info, io_outputs: &[], main_stem: stem.as_deref(), zero: roles::ZERO_CLASSES }).unwrap();
    assert!(!c[0].graph.contains_key("9001"));
}
