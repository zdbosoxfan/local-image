use li_ai::comfy::ObjectInfo;
use li_ai::custom::{CustomWorkflow, Field, FieldKind, Inputs, basic_fields, from_api, import, list, prepare, save, ui_to_api};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("li-ai-custom-{}-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(), tag));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn minimal_api() -> Value {
    json!({
        "1": {"class_type": "SaveImage", "inputs": {"images": ["2", 0]}},
        "2": {"class_type": "CLIPTextEncode", "_meta": {"title": "li:prompt"}, "inputs": {"text": "a cat"}},
        "3": {"class_type": "CLIPTextEncode", "_meta": {"title": "li:negative"}, "inputs": {"text": "blur"}},
        "4": {"class_type": "SeedNode", "_meta": {"title": "li:seed"}, "inputs": {"seed": 5}},
        "5": {"class_type": "LoadImage", "_meta": {"title": "li:image"}, "inputs": {"image": "a.png"}}
    })
}

fn object_info() -> ObjectInfo {
    ObjectInfo(json!({
        "CLIPTextEncode": {
            "input": {"required": {"text": ["STRING", {"multiline": true}], "clip": ["CLIP"]}},
            "input_order": {"required": ["text", "clip"]}
        },
        "SaveImage": {
            "input": {"required": {"images": ["IMAGE"], "filename_prefix": ["STRING", {}]}},
            "input_order": {"required": ["images", "filename_prefix"]}
        }
    }))
}

#[test]
fn from_api_rejects_non_object_graph() {
    for bad in [json!(null), json!([]), json!("x")] {
        let err = from_api("bad", bad).unwrap_err();
        assert!(err.to_string().contains("not a ComfyUI workflow"));
    }
}

#[test]
fn from_api_rejects_missing_class_type() {
    let g = json!({"1": {"inputs": {}}});
    let err = from_api("x", g).unwrap_err();
    assert!(err.to_string().contains("no class_type"));
}

#[test]
fn from_api_rejects_no_image_output() {
    let g = json!({"1": {"class_type": "KSampler", "inputs": {}}});
    let err = from_api("x", g).unwrap_err();
    assert!(err.to_string().contains("saves no image"));
}

#[test]
fn unknown_markers_are_ignored_but_output_required() {
    let g = json!({
        "1": {"class_type": "SaveImage", "inputs": {}},
        "2": {"class_type": "X", "_meta": {"title": "li:unknown"}, "inputs": {"text": "x"}}
    });
    let w = from_api("x", g).unwrap();
    assert!(w.fields.is_empty());
    assert!(!w.basic);
}

#[test]
fn from_api_parses_aliases_and_image_indices() {
    let g = json!({
        "1": {"class_type": "SaveImage", "_meta": {"title": "li:output"}, "inputs": {"images": ["p", 0]}},
        "2": {"class_type": "X", "_meta": {"title": "li:positive"}, "inputs": {"text": "pos"}},
        "3": {"class_type": "X", "_meta": {"title": "li:negative"}, "inputs": {"text": "neg"}},
        "4": {"class_type": "X", "_meta": {"title": "li:seed"}, "inputs": {"noise_seed": 7}},
        "5": {"class_type": "X", "_meta": {"title": "li:steps"}, "inputs": {"value": 20}},
        "6": {"class_type": "X", "_meta": {"title": "li:guidance"}, "inputs": {"value": 3.5}},
        "7": {"class_type": "X", "_meta": {"title": "li:strength"}, "inputs": {"value": 0.7}},
        "8": {"class_type": "X", "_meta": {"title": "li:width"}, "inputs": {"value": 512}},
        "9": {"class_type": "X", "_meta": {"title": "li:height"}, "inputs": {"value": 768}},
        "10": {"class_type": "X", "_meta": {"title": "li:image"}, "inputs": {"image": "a.png"}},
        "11": {"class_type": "X", "_meta": {"title": "li:image2"}, "inputs": {"image": "b.png"}},
        "12": {"class_type": "X", "_meta": {"title": "li:selection"}, "inputs": {"image": "mask.png"}}
    });
    let w = from_api("alias", g).unwrap();
    let kinds: Vec<FieldKind> = w.fields.iter().map(|f| f.kind.clone()).collect();
    for k in [
        FieldKind::Prompt,
        FieldKind::Negative,
        FieldKind::Seed,
        FieldKind::Steps,
        FieldKind::Cfg,
        FieldKind::Denoise,
        FieldKind::Width,
        FieldKind::Height,
        FieldKind::Image(0),
        FieldKind::Image(1),
        FieldKind::Mask,
        FieldKind::Output,
    ] {
        assert!(kinds.contains(&k), "missing {:?}", k);
    }
    assert_eq!(w.images(), 2);
}

#[test]
fn text_input_falls_back_to_other_string_keys() {
    let g = json!({
        "1": {"class_type": "SaveImage", "inputs": {}},
        "2": {"class_type": "X", "_meta": {"title": "li:prompt"}, "inputs": {"value": "pos"}},
        "3": {"class_type": "X", "_meta": {"title": "li:negative"}, "inputs": {"string": "neg"}}
    });
    let w = from_api("fallback", g).unwrap();
    let p = w.fields.iter().find(|f| f.kind == FieldKind::Prompt).unwrap();
    assert_eq!(p.input, "value");
    assert_eq!(p.default, json!("pos"));
    let n = w.fields.iter().find(|f| f.kind == FieldKind::Negative).unwrap();
    assert_eq!(n.input, "string");
    assert_eq!(n.default, json!("neg"));
}

#[test]
fn marker_without_settable_input_errors() {
    let g = json!({
        "1": {"class_type": "SaveImage", "inputs": {}},
        "2": {"class_type": "X", "_meta": {"title": "li:prompt"}, "inputs": {"clip": ["3", 0]}}
    });
    let err = from_api("x", g).unwrap_err();
    assert!(err.to_string().contains("no input Local Image can set"));
}

#[test]
fn number_input_prefers_primary_name() {
    let g = json!({
        "1": {"class_type": "SaveImage", "inputs": {}},
        "2": {"class_type": "X", "_meta": {"title": "li:seed"}, "inputs": {"noise_seed": 5, "seed": 9}}
    });
    let w = from_api("x", g).unwrap();
    let f = w.fields.iter().find(|f| f.kind == FieldKind::Seed).unwrap();
    assert_eq!(f.input, "seed");
    assert_eq!(f.default, json!(9));
}

#[test]
fn import_api_format_needs_no_object_info() {
    let g = minimal_api();
    let direct = from_api("name", g.clone()).unwrap();
    let via_import = import("name", &g, None).unwrap();
    assert_eq!(direct, via_import);
}

#[test]
fn import_ui_format_without_object_info_errors() {
    let ui = json!({"nodes": []});
    let err = import("x", &ui, None).unwrap_err();
    assert!(err.to_string().contains("start ComfyUI"));
}

#[test]
fn ui_to_api_requires_nodes_array() {
    let info = object_info();
    let err = ui_to_api(&json!({"links": []}), &info).unwrap_err();
    assert!(err.to_string().contains("no nodes"));
}

#[test]
fn ui_to_api_rejects_unknown_node_class() {
    let info = ObjectInfo(json!({}));
    let ui = json!({
        "nodes": [{"id": 1, "type": "DoesNotExist", "inputs": [], "widgets_values": []}],
        "links": []
    });
    let err = ui_to_api(&ui, &info).unwrap_err();
    assert!(err.to_string().contains("ComfyUI has no node"));
}

#[test]
fn ui_to_api_skips_notes_and_resolves_reroutes() {
    let info = object_info();
    let ui = json!({
        "nodes": [
            {"id": 6, "type": "CLIPTextEncode", "inputs": [{"name": "clip", "link": null}], "widgets_values": ["a fox"]},
            {"id": 9, "type": "Reroute", "inputs": [{"name": "", "link": 1}]},
            {"id": 20, "type": "Note", "widgets_values": ["hi"]},
            {"id": 12, "type": "SaveImage", "inputs": [{"name": "images", "link": 2}], "widgets_values": ["out"]}
        ],
        "links": [[1, 6, 0, 9, 0, "*"], [2, 9, 0, 12, 0, "IMAGE"]]
    });
    let out = ui_to_api(&ui, &info).unwrap();
    assert_eq!(out["6"]["inputs"]["text"], "a fox");
    assert_eq!(out["12"]["inputs"]["images"], json!(["6", 0]));
    assert_eq!(out["12"]["inputs"]["filename_prefix"], "out");
    assert!(out.get("9").is_none());
    assert!(out.get("20").is_none());
}

#[test]
fn basic_fields_finds_prompt_seed_size_and_loaders() {
    let g = json!({
        "1": {"class_type": "Sampler", "inputs": {"positive": ["2", 0], "negative": ["3", 0], "seed": 5}},
        "2": {"class_type": "CLIPTextEncode", "inputs": {"text": "prompt"}},
        "3": {"class_type": "CLIPTextEncode", "inputs": {"text": "negative"}},
        "4": {"class_type": "EmptyLatentImage", "inputs": {"width": 512, "height": 768}},
        "5": {"class_type": "LoadImage", "inputs": {"image": "a.png"}},
        "6": {"class_type": "LoadImage", "inputs": {"image": "b.png"}}
    });
    let fields = basic_fields(&g);
    assert!(fields.iter().any(|f| f.kind == FieldKind::Prompt && f.node == "2" && f.input == "text"));
    assert!(fields.iter().any(|f| f.kind == FieldKind::Negative && f.node == "3"));
    assert!(fields.iter().any(|f| f.kind == FieldKind::Seed && f.node == "1" && f.input == "seed"));
    assert!(fields.iter().any(|f| f.kind == FieldKind::Width && f.node == "4" && f.input == "width"));
    assert!(fields.iter().any(|f| f.kind == FieldKind::Height && f.node == "4" && f.input == "height"));
    let images: Vec<FieldKind> = fields
        .iter()
        .filter_map(|f| match f.kind {
            FieldKind::Image(i) => Some(FieldKind::Image(i)),
            _ => None,
        })
        .collect();
    assert_eq!(images, vec![FieldKind::Image(0), FieldKind::Image(1)]);
}

#[test]
fn basic_fields_removes_negative_when_same_node_as_prompt() {
    let g = json!({
        "1": {"class_type": "Sampler", "inputs": {"positive": ["2", 0], "negative": ["2", 0]}},
        "2": {"class_type": "CLIPTextEncode", "inputs": {"text": "shared"}}
    });
    let fields = basic_fields(&g);
    assert!(fields.iter().any(|f| f.kind == FieldKind::Prompt));
    assert!(fields.iter().all(|f| f.kind != FieldKind::Negative));
}

#[test]
fn basic_fields_returns_empty_for_non_object() {
    assert!(basic_fields(&json!([])).is_empty());
    assert!(basic_fields(&json!("x")).is_empty());
}

#[test]
fn prepare_sets_provided_values_and_leaves_optionals_unchanged() {
    let g = json!({
        "1": {"class_type": "SaveImage", "inputs": {}},
        "2": {"class_type": "X", "_meta": {"title": "li:prompt"}, "inputs": {"text": "old prompt"}},
        "3": {"class_type": "X", "_meta": {"title": "li:negative"}, "inputs": {"text": "old neg"}},
        "4": {"class_type": "X", "_meta": {"title": "li:seed"}, "inputs": {"seed": 5}},
        "5": {"class_type": "X", "_meta": {"title": "li:steps"}, "inputs": {"steps": 20}},
        "6": {"class_type": "X", "_meta": {"title": "li:cfg"}, "inputs": {"cfg": 7.5}},
        "7": {"class_type": "X", "_meta": {"title": "li:denoise"}, "inputs": {"denoise": 0.8}},
        "8": {"class_type": "X", "_meta": {"title": "li:width"}, "inputs": {"width": 512}},
        "9": {"class_type": "X", "_meta": {"title": "li:height"}, "inputs": {"height": 768}}
    });
    let w = from_api("rich", g).unwrap();
    let (run, _) = prepare(&w, &Inputs { prompt: "new prompt".into(), negative: "new neg".into(), seed: 9, ..Default::default() });
    assert_eq!(run["2"]["inputs"]["text"], "new prompt");
    assert_eq!(run["3"]["inputs"]["text"], "new neg");
    assert_eq!(run["4"]["inputs"]["seed"], 9);
    assert_eq!(run["5"]["inputs"]["steps"], 20);
    assert_eq!(run["6"]["inputs"]["cfg"], 7.5);
    assert_eq!(run["7"]["inputs"]["denoise"], 0.8);
    assert_eq!(run["8"]["inputs"]["width"], 512);
    assert_eq!(run["9"]["inputs"]["height"], 768);
}

#[test]
fn prepare_returns_images_and_masks_in_field_order() {
    let g = json!({
        "1": {"class_type": "SaveImage", "inputs": {}},
        "2": {"class_type": "X", "_meta": {"title": "li:image"}, "inputs": {"image": "a.png"}},
        "3": {"class_type": "X", "_meta": {"title": "li:mask"}, "inputs": {"image": "mask.png"}},
        "4": {"class_type": "X", "_meta": {"title": "li:image2"}, "inputs": {"image": "b.png"}}
    });
    let w = from_api("img", g).unwrap();
    let (_, images) = prepare(&w, &Inputs::default());
    assert_eq!(images, vec![(FieldKind::Image(0), "2".to_owned()), (FieldKind::Mask, "3".to_owned()), (FieldKind::Image(1), "4".to_owned())]);
}

#[test]
fn prepare_output_keeps_only_selected_image_saver() {
    let g = json!({
        "out": {"class_type": "SaveImage", "_meta": {"title": "li:output"}, "inputs": {"images": ["src", 0]}},
        "other": {"class_type": "SaveImage", "inputs": {"images": ["src", 0]}},
        "preview": {"class_type": "PreviewImage", "inputs": {"images": ["src", 0]}}
    });
    let w = from_api("out", g).unwrap();
    let (run, _) = prepare(&w, &Inputs::default());
    let obj = run.as_object().unwrap();
    assert!(obj.contains_key("out"));
    assert!(!obj.contains_key("other"));
    assert!(!obj.contains_key("preview"));
}

#[test]
fn prepare_does_not_mutate_workflow_graph() {
    let w = from_api("no-mutate", minimal_api()).unwrap();
    let before = w.graph.clone();
    let _ = prepare(&w, &Inputs { prompt: "new".into(), seed: 123, ..Default::default() });
    assert_eq!(w.graph, before);
}

#[test]
fn save_and_list_roundtrip_with_sanitized_filename() {
    let dir = temp_dir("save-list");
    let w = from_api("My Upscaler / v2?", minimal_api()).unwrap();
    let path = save(&w, &dir).unwrap();
    assert_eq!(path.extension().and_then(|e| e.to_str()), Some("json"));
    assert!(!path.file_name().unwrap().to_string_lossy().contains('/'));
    let loaded = list(&dir);
    assert_eq!(loaded, vec![w]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn list_ignores_non_json_files_and_sorts_case_insensitively() {
    let dir = temp_dir("list-ignore");
    let a = from_api("A", minimal_api()).unwrap();
    let b = from_api("b", minimal_api()).unwrap();
    save(&a, &dir).unwrap();
    save(&b, &dir).unwrap();
    std::fs::write(dir.join("notes.txt"), b"not json").unwrap();
    std::fs::create_dir(dir.join("subdir")).unwrap();
    let got = list(&dir);
    assert_eq!(got, vec![a, b]);

    let missing =
        std::env::temp_dir().join(format!("li-ai-custom-missing-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    let _ = std::fs::remove_dir_all(&missing);
    assert!(list(&missing).is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn tags_reflect_basic_prompt_image_and_selection() {
    let w = CustomWorkflow {
        name: "x".into(),
        graph: json!({}),
        fields: vec![
            Field { kind: FieldKind::Prompt, node: "p".into(), input: "text".into(), default: Value::Null },
            Field { kind: FieldKind::Image(0), node: "i".into(), input: "image".into(), default: Value::Null },
            Field { kind: FieldKind::Mask, node: "m".into(), input: "image".into(), default: Value::Null },
        ],
        basic: true,
        family: None,
    };
    assert_eq!(w.tags(), vec!["Basic controls", "Prompt", "1 image", "Selection"]);
    assert!(w.has(&FieldKind::Prompt));
    assert!(w.has(&FieldKind::Mask));
    assert_eq!(w.images(), 1);
}

#[test]
fn workflow_serde_roundtrip() {
    let w = from_api("round", minimal_api()).unwrap();
    let encoded = serde_json::to_value(&w).unwrap();
    let decoded: CustomWorkflow = serde_json::from_value(encoded).unwrap();
    assert_eq!(w, decoded);
}
