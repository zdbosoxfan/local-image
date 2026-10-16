//! Full-size export and runtime palette regressions; no native window/model required.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::{
    LightcraftApp, Services,
    headless::Headless,
    panels::export_dialog,
    state::Dialog,
    theme::{self, Tokens},
};
use lightcraft_engine::{Session, export::ExportOptions};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn source_psd() -> Vec<u8> {
    // Independent uncompressed 4096x4 RGB source; only 16K pixels, above the old long-edge default.
    let mut data = b"8BPS".to_vec();
    data.extend_from_slice(&1u16.to_be_bytes());
    data.extend_from_slice(&[0; 6]);
    data.extend_from_slice(&3u16.to_be_bytes());
    data.extend_from_slice(&4u32.to_be_bytes());
    data.extend_from_slice(&4096u32.to_be_bytes());
    data.extend_from_slice(&8u16.to_be_bytes());
    data.extend_from_slice(&3u16.to_be_bytes());
    for _ in 0..3 {
        data.extend_from_slice(&0u32.to_be_bytes());
    }
    data.extend_from_slice(&0u16.to_be_bytes());
    for value in [40, 100, 180] {
        data.extend(std::iter::repeat_n(value, 4096 * 4));
    }
    data
}
#[test]
fn library_workflow_first_export_full_size_and_explicit_saved_resize() {
    let temp = Temp(std::env::temp_dir().join(format!(
        "li-workflow-export-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    )));
    std::fs::create_dir(&temp.0).unwrap();
    let source = temp.0.join("wide.psd");
    let original = source_psd();
    std::fs::write(&source, &original).unwrap();
    let mut session = Session::new().with_fs();
    session.execute("library.import", &json!({"paths":[source.to_string_lossy()]})).unwrap();
    let id = session.active().unwrap();
    let mut app = LightcraftApp::new(session, Services::default());
    app.ui.settings.gpu = false;
    let mut h = Headless::new(app, [1400., 1000.], 1.0);
    h.step();
    h.app.run("dialog.export", json!({})).unwrap();
    h.step();
    let dialog = h.app.ui.dialog.as_ref().unwrap();
    assert!(matches!(dialog, Dialog::Export { full_size: true, .. }));
    let options = ExportOptions::from_json(&export_dialog::export_dialog_params(dialog));
    assert!(options.resize.is_none());
    let exported = lightcraft_engine::export::export_photo(&mut h.app.session, id, &options, 1).unwrap();
    let decoded = lightcraft_codecs::decode(&exported.bytes, Default::default()).unwrap();
    assert_eq!((exported.width, exported.height), (4096, 4));
    assert_eq!((decoded.image.width, decoded.image.height), (4096, 4));
    // Explicit persisted size still wins, and survives the dialog's JSON round trip.
    h.app.ui.dialog = None;
    h.app.session.last_export = Some(json!({"format":"png","longEdge":2048,"quality":77}));
    h.app.run("dialog.export", json!({})).unwrap();
    let saved = export_dialog::export_dialog_params(h.app.ui.dialog.as_ref().unwrap());
    let options = ExportOptions::from_json(&saved);
    assert_eq!(options.resize.unwrap().value, 2048.0);
    let small = lightcraft_engine::export::export_photo(&mut h.app.session, id, &options, 1).unwrap();
    let decoded = lightcraft_codecs::decode(&small.bytes, Default::default()).unwrap();
    assert_eq!((decoded.image.width, decoded.image.height), (2048, 2));
    let mut restored = export_dialog::new_dialog("/not-written".into());
    export_dialog::apply_export_params(&mut restored, &saved, false);
    assert_eq!(ExportOptions::from_json(&export_dialog::export_dialog_params(&restored)).resize, options.resize);
    if let Dialog::Export { full_size, resize, .. } = &mut restored {
        *full_size = true;
        assert_eq!(resize.value, 2048.0);
    } else {
        panic!("export");
    }
    assert!(ExportOptions::from_json(&export_dialog::export_dialog_params(&restored)).resize.is_none());
    if let Dialog::Export { full_size, .. } = &mut restored {
        *full_size = false;
    }
    assert_eq!(ExportOptions::from_json(&export_dialog::export_dialog_params(&restored)).resize, options.resize);
    assert_eq!(std::fs::read(&source).unwrap(), original, "export must leave the source unchanged");
}
#[test]
fn library_workflow_empty_export_options_and_preset_sizes_are_explicit() {
    for value in [serde_json::Value::Null, json!({}), json!({"quality":80})] {
        for preset in [false, true] {
            let mut dialog = export_dialog::new_dialog("/not-written".into());
            export_dialog::apply_export_params(&mut dialog, &value, preset);
            assert!(ExportOptions::from_json(&export_dialog::export_dialog_params(&dialog)).resize.is_none());
        }
    }
    let mut dialog = export_dialog::new_dialog("/not-written".into());
    export_dialog::apply_export_params(&mut dialog, &json!({"longEdge":1500}), true);
    assert_eq!(ExportOptions::from_json(&export_dialog::export_dialog_params(&dialog)).resize.unwrap().value, 1500.0);
}
fn light() -> Tokens {
    let gray = egui::Color32::from_gray;
    Tokens {
        dark: false,
        chrome: gray(246),
        canvas: gray(225),
        grid_bg: gray(225),
        field: gray(252),
        button: gray(240),
        hover: gray(230),
        pressed: gray(215),
        text: gray(20),
        text_label: gray(20),
        text_dim: gray(75),
        icon: gray(40),
        selection: gray(215),
        selection_text: gray(0),
        ..Default::default()
    }
}
#[test]
fn library_workflow_runtime_palette_survives_initialization_and_shadow_render() {
    let mut app = LightcraftApp::new(Session::new(), Default::default());
    app.ui.settings.gpu = false;
    let palette = light();
    app.set_host_theme(palette);
    let mut h = Headless::new(app, [1200., 800.], 1.0);
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(Tokens::get(&h.view.ctx), palette);
    assert!(!h.view.ctx.global_style().visuals.dark_mode);
    let image = h.paint();
    assert_eq!(image[(100, 300)], palette.grid_bg, "actual empty grid body uses host palette");
    let shadow = h.app.headless_screenshot(&h.view.ctx, true).unwrap();
    assert_eq!(shadow[(100, 300)], palette.grid_bg);
    let value = serde_json::to_value(&h.app.ui).unwrap();
    assert!(value.get("theme").is_none() && value.get("hostTheme").is_none(), "no second persisted theme authority");
    h.app.set_host_theme(Tokens::default());
    h.app.restyle(&h.view.ctx);
    h.step();
    assert_eq!(Tokens::get(&h.view.ctx), Tokens::default());
    assert!(h.view.ctx.global_style().visuals.dark_mode);
}
#[test]
fn library_workflow_accent_and_warning_foregrounds_have_contrast() {
    let lum = |color: egui::Color32| {
        let c = color.to_array();
        let linear = |v: u8| {
            let x = f64::from(v) / 255.;
            if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * linear(c[0]) + 0.7152 * linear(c[1]) + 0.0722 * linear(c[2])
    };
    for fill in [
        egui::Color32::from_rgb(10, 36, 106),
        egui::Color32::from_rgb(128, 96, 0),
        egui::Color32::from_rgb(108, 92, 231),
        Tokens::default().accent,
        Tokens::default().caution,
    ] {
        let a = lum(theme::text_on(fill));
        let b = lum(fill);
        assert!((a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5);
    }
}
