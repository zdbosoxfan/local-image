use li_ai::comfy::{self, ObjectInfo, Progress, Stage};
use serde_json::{Value, json};

#[test]
fn normalized_name_lowercase_alnum_only() {
    assert_eq!(comfy::normalized_name("A/B\\C.TXT"), "ctxt");
    assert_eq!(comfy::normalized_name("  Hello, World!  "), "helloworld");
}

#[test]
fn stage_for_class_various() {
    assert_eq!(comfy::stage_for_class("KSampler"), Stage::Sampling);
    assert_eq!(comfy::stage_for_class("SamplerCustomAdvanced"), Stage::Sampling);
    assert_eq!(comfy::stage_for_class("VAEDecodeTiled"), Stage::Decoding);
    assert_eq!(comfy::stage_for_class("CheckpointLoaderSimple"), Stage::Loading);
    assert_eq!(comfy::stage_for_class("LoadImage"), Stage::Loading);
    assert_eq!(comfy::stage_for_class("CLIPTextEncode"), Stage::Conditioning);
    assert_eq!(comfy::stage_for_class("SaveImage"), Stage::Saving);
    assert_eq!(comfy::stage_for_class("PreviewImage"), Stage::Saving);
    assert_eq!(comfy::stage_for_class("AnythingElse"), Stage::Running);
    assert_eq!(comfy::stage_for_class("ksampler"), Stage::Sampling);
    assert_eq!(comfy::stage_for_class("loader"), Stage::Loading);
}

#[test]
fn progress_fraction_some_and_edge() {
    let p = Progress { stage: Stage::Sampling, step: Some((2, 5)), message: "".into() };
    assert_eq!(p.fraction(), Some(2.0 / 5.0));
    let p = Progress { stage: Stage::Preparing, step: Some((0, 0)), message: "".into() };
    assert_eq!(p.fraction(), None);
    let p = Progress { stage: Stage::Preparing, step: Some((3, 0)), message: "".into() };
    assert_eq!(p.fraction(), None);
}

#[test]
fn object_info_has_node() {
    let info = ObjectInfo(json!({"KSampler": {}}));
    assert!(info.has_node("KSampler"));
    assert!(!info.has_node("Missing"));
    let empty = ObjectInfo(Value::Null);
    assert!(!empty.has_node("any"));
}

#[test]
fn object_info_choices_legacy_shape() {
    let info = ObjectInfo(json!({
        "UNETLoader": {"input": {"required": {"unet_name": [["a.safetensors", "b.safetensors"]]}}}
    }));
    assert_eq!(info.choices("UNETLoader", "unet_name"), vec!["a.safetensors", "b.safetensors"]);
}

#[test]
fn object_info_choices_missing_or_wrong_shape() {
    let info = ObjectInfo(json!({}));
    assert_eq!(info.choices("A", "B"), Vec::<String>::new());
    let info = ObjectInfo(json!({"Node": {"input": {"required": {"x": 42}}}}));
    assert_eq!(info.choices("Node", "x"), Vec::<String>::new());
    let info = ObjectInfo(json!({"Node": {"input": {"optional": {}}}}));
    assert_eq!(info.choices("Node", "x"), Vec::<String>::new());
}

#[test]
fn first_output_image_empty_and_malformed() {
    assert_eq!(comfy::first_output_image(&Value::Null), None);
    assert_eq!(comfy::first_output_image(&json!({})), None);
    assert_eq!(comfy::first_output_image(&json!({"outputs": {}})), None);
    assert_eq!(comfy::first_output_image(&json!({"outputs": {"1": {}}})), None);
    assert_eq!(comfy::first_output_image(&json!({"outputs": {"1": {"images": []}}})), None);
    assert_eq!(comfy::first_output_image(&json!({"outputs": {"1": {"images": [{}]}}})), None);
}

#[test]
fn first_output_image_sorts_by_numeric_node_id() {
    let e = json!({
        "outputs": {
            "10": {"images": [{"filename": "late.png", "type": "output"}]},
            "2": {"images": [{"filename": "early.png", "type": "output"}]}
        }
    });
    let (file, _, _) = comfy::first_output_image(&e).unwrap();
    assert_eq!(file, "early.png");
}

#[test]
fn first_output_image_output_beats_temp_regardless_of_order() {
    let e = json!({
        "outputs": {
            "1": {"images": [{"filename": "a.png", "type": "temp"}]},
            "2": {"images": [{"filename": "b.png", "type": "output"}]}
        }
    });
    let (file, _, _) = comfy::first_output_image(&e).unwrap();
    assert_eq!(file, "b.png");
}

#[test]
fn friendly_error_oom_and_generic() {
    assert_eq!(comfy::friendly_error("CUDA out of memory"), "The GPU ran out of memory. Try a smaller size, the compact model, or close other GPU work.");
    assert_eq!(comfy::friendly_error("Allocation on device"), "The GPU ran out of memory. Try a smaller size, the compact model, or close other GPU work.");
    assert_eq!(comfy::friendly_error("Some other error"), "ComfyUI could not finish: Some other error");
}
