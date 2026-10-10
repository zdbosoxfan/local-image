use li_ai::comfy::{self, Cancelled, ComfyClient, JobControl, ObjectInfo, Progress, Stage};
use serde_json::{Value, json};

#[test]
fn normalized_name_empty() {
    assert_eq!(comfy::normalized_name(""), "");
}

#[test]
fn normalized_name_lowercase_alnum_only() {
    assert_eq!(comfy::normalized_name("A/B\\C.TXT"), "ctxt");
    assert_eq!(comfy::normalized_name("  Hello, World!  "), "helloworld");
}

#[test]
fn normalized_name_uses_basename_after_subfolder() {
    assert_eq!(comfy::normalized_name("dir/sub\\file.PNG"), "filepng");
}

#[test]
fn stage_labels() {
    assert_eq!(Stage::Preparing.label(), "Preparing");
    assert_eq!(Stage::Uploading.label(), "Uploading");
    assert_eq!(Stage::Queued.label(), "Waiting in the ComfyUI queue");
    assert_eq!(Stage::Loading.label(), "Loading models");
    assert_eq!(Stage::Conditioning.label(), "Reading the prompt");
    assert_eq!(Stage::Sampling.label(), "Sampling");
    assert_eq!(Stage::Decoding.label(), "Decoding");
    assert_eq!(Stage::Saving.label(), "Saving");
    assert_eq!(Stage::Running.label(), "Running");
    assert_eq!(Stage::Finishing.label(), "Finishing");
    assert_eq!(Stage::Done.label(), "Done");
    assert_eq!(Stage::Failed.label(), "Failed");
    assert_eq!(Stage::Cancelled.label(), "Cancelled");
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
fn progress_default() {
    let p = Progress::default();
    assert_eq!(p.stage, Stage::Preparing);
    assert_eq!(p.step, None);
    assert_eq!(p.message, "");
    assert_eq!(p.fraction(), None);
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
fn job_control_new_and_cancel() {
    let jc = JobControl::new();
    assert!(!jc.is_cancelled());
    jc.cancel();
    assert!(jc.is_cancelled());
}

#[test]
fn job_control_check_returns_cancelled_error() {
    let jc = JobControl::new();
    assert!(jc.check().is_ok());
    jc.cancel();
    let err = jc.check().unwrap_err();
    assert!(err.is::<Cancelled>());
    assert_eq!(err.to_string(), "Cancelled");
}

#[test]
fn job_control_set_stage_updates_progress() {
    let jc = JobControl::new();
    jc.set_stage(Stage::Sampling);
    assert_eq!(jc.progress().stage, Stage::Sampling);
    jc.set_stage(Stage::Done);
    assert_eq!(jc.progress().stage, Stage::Done);
}

#[test]
fn job_control_set_message_updates_progress() {
    let jc = JobControl::new();
    jc.set_message("hello");
    assert_eq!(jc.progress().message, "hello");
    jc.set_message("world");
    assert_eq!(jc.progress().message, "world");
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
fn object_info_choices_v3_shape() {
    let info = ObjectInfo(json!({
        "CLIPLoader": {"input": {"required": {"type": ["COMBO", {"options": ["flux2", "qwen_image"]}]}}}
    }));
    assert_eq!(info.choices("CLIPLoader", "type"), vec!["flux2", "qwen_image"]);
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
fn object_info_find_file_normalizes_and_returns_exact() {
    let info = ObjectInfo(json!({
        "UNETLoader": {"input": {"required": {"unet_name": [["a/Qwen_Image_2.1_BF16.safetensors", "b.safetensors"]]}}}
    }));
    assert_eq!(info.find_file("UNETLoader", "unet_name", "qwen_image_2.1_bf16.safetensors").as_deref(), Some("a/Qwen_Image_2.1_BF16.safetensors"));
    assert_eq!(info.find_file("UNETLoader", "unet_name", "B.SAFETENSORS").as_deref(), Some("b.safetensors"));
    assert!(info.find_file("UNETLoader", "unet_name", "missing.safetensors").is_none());
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
fn first_output_image_prefers_output_over_temp() {
    let e = json!({
        "outputs": {
            "10": {"images": [{"filename": "temp.png", "type": "temp"}]},
            "2": {"images": [{"filename": "out.png", "type": "output"}]}
        }
    });
    let (file, sub, kind) = comfy::first_output_image(&e).unwrap();
    assert_eq!((file.as_str(), sub.as_str(), kind.as_str()), ("out.png", "", "output"));
}

#[test]
fn first_output_image_falls_back_to_temp() {
    let e = json!({
        "outputs": {
            "10": {"images": [{"filename": "preview.png", "type": "temp"}]}
        }
    });
    let (file, sub, kind) = comfy::first_output_image(&e).unwrap();
    assert_eq!((file.as_str(), sub.as_str(), kind.as_str()), ("preview.png", "", "temp"));
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

#[test]
fn cancelled_display_and_error_trait() {
    let c = Cancelled;
    assert_eq!(c.to_string(), "Cancelled");
    // std::error::Error is implemented; we can verify by downcasting from anyhow using JobControl.
    let jc = JobControl::new();
    jc.cancel();
    let err = jc.check().unwrap_err();
    assert!(err.is::<Cancelled>());
}

#[test]
fn comfy_client_host() {
    let client = ComfyClient::new("127.0.0.1:8188");
    assert_eq!(client.host(), "127.0.0.1:8188");
    let debug = format!("{:?}", client);
    assert!(debug.contains("127.0.0.1:8188"));
}
