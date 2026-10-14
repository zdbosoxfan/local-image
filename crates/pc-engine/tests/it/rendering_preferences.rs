//! Rendering policy defaults and command validation, without a graphics device.
use photocraft_engine::prefs::{GpuBackend, Preferences, RenderingMode};
use serde_json::json;

#[test]
fn rendering_mode_defaults_to_automatic() {
    assert_eq!(Preferences::default().performance.rendering_mode, RenderingMode::Auto);
    let p: Preferences = serde_json::from_value(json!({"performance": {"gpuBackend": "cpu"}})).unwrap();
    assert_eq!(p.performance.rendering_mode, RenderingMode::Auto, "the graphics backend is not the policy");
}

#[test]
fn explicit_choices_round_trip() {
    let mut p = Preferences::default();
    p.performance.gpu_backend = GpuBackend::Cpu;
    for mode in [RenderingMode::Auto, RenderingMode::Gpu, RenderingMode::Cpu] {
        p.set("performance.renderingMode", json!(mode.name())).unwrap();
        assert_eq!(p.performance.rendering_mode, mode);
        let restored: Preferences = serde_json::from_value(p.to_json()).unwrap();
        assert_eq!(restored.performance.rendering_mode, mode);
    }
}

#[test]
fn malformed_policy_is_rejected_without_changing_preferences() {
    let mut p = Preferences::default();
    let before = p.to_json();
    for invalid in [json!("quantum"), json!(7), json!({}), json!(true), json!(null)] {
        assert!(p.set("performance.renderingMode", invalid).is_err());
        assert_eq!(p.to_json(), before);
    }
    p.set("performance.renderingMode", json!("cpu")).unwrap();
    p.reset(Some("performance.renderingMode")).unwrap();
    assert_eq!(p.performance.rendering_mode, RenderingMode::Auto);
}
