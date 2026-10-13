//! Preset files opened like documents (File › Open, drag-and-drop, the Brushes panel's
//! Import Brushes…, the Preset Manager's Load…): Photoshop brushes (`.abr`) go to the brush
//! library through `brush.presets.importAbr`, gradients (`.grd`) to the Gradients panel through
//! `gradient.presets.importGrd`, instead of opening as documents.

use serde_json::{Value, json};

use crate::PhotocraftApp;

/// Extensions handled here rather than by the document importer.
pub const PRESET_EXTS: &[&str] = &["abr", "grd", "gbr", "gih", "vbr", "bundle", "kpp", "brush", "brushset", "myb", "afbrushes"];

fn ext(name: &str) -> String {
    std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

/// Is `name` a preset file this module imports?
pub fn is_preset_file(name: &str) -> bool {
    PRESET_EXTS.contains(&ext(name).as_str())
}

/// Command params for the file: a real path keeps the journal small; browser files arrive as bytes.
fn source(name: &str, bytes: &[u8]) -> Value {
    #[cfg(not(target_arch = "wasm32"))]
    if std::path::Path::new(name).is_absolute() && std::path::Path::new(name).is_file() {
        return json!({"path": name});
    }
    let _ = name;
    json!({"data": photocraft_engine::paint::tile::b64_encode(bytes)})
}

/// Import a preset file; `None` when `name` isn't one.
pub fn open(app: &mut PhotocraftApp, name: &str, bytes: &[u8]) -> Option<Result<(), String>> {
    let (cmd, what) = match ext(name).as_str() {
        "abr" | "gbr" | "gih" | "vbr" | "bundle" | "kpp" | "brush" | "brushset" | "myb" | "afbrushes" => ("brush.presets.importAbr", "brushes"),
        "grd" => ("gradient.presets.importGrd", "gradients"),
        _ => return None,
    };
    let stem = std::path::Path::new(name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| tl!("Imported").into());
    let mut p = source(name, bytes);
    p["group"] = json!(stem);
    if what == "brushes" {
        p["select"] = json!(true);
        p["format"] = json!(ext(name));
    }
    Some(app.run(cmd, p).map(|r| {
        // A background import (#210): `jobs_ui` reports the count when it lands.
        if r.get("pending").and_then(Value::as_bool) == Some(true) {
            app.ui.status_error = false;
            app.ui.status = format!("Importing {what} from {stem}…");
            return;
        }
        let n = r.get("count").and_then(Value::as_u64).unwrap_or(0);
        let warnings: Vec<String> = r.get("warnings").and_then(|w| serde_json::from_value(w.clone()).ok()).unwrap_or_default();
        app.ui.status_error = false;
        app.ui.status = if what == "brushes" {
            summary(&r)
        } else {
            match warnings.first() {
                Some(w) => format!("Imported {n} {what} from {stem} ({} notes: {w})", warnings.len()),
                None => format!("Imported {n} {what} from {stem}"),
            }
        };
        if what == "brushes" {
            // Show them: Brush Settings › Brushes tab.
            app.ui.panels.brush_settings = true;
            app.ui.brush_tab = 1;
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_preset_files() {
        assert!(is_preset_file("/a/b/Set.ABR"));
        assert!(is_preset_file("sunsets.grd"));
        assert!(!is_preset_file("photo.psd"));
        assert!(!is_preset_file("abr"));
    }
}

/// The same summary for synchronous imports and completed background jobs.
pub(crate) fn summary(result: &Value) -> String {
    let n = result["count"].as_u64().unwrap_or(0);
    let approximate = result["approximate"].as_u64().unwrap_or(0);
    let skipped = result["skipped"].as_array().cloned().unwrap_or_default();
    let reasons = skipped.iter().filter_map(Value::as_str).take(6).collect::<Vec<_>>().join("; ");
    let mut text = format!("Imported {n} brushes, {approximate} approximate, {} skipped", skipped.len());
    if !reasons.is_empty() {
        text.push_str(&format!(" ({reasons})"));
    }
    if let Some(warnings) = result["warnings"].as_array()
        && !warnings.is_empty()
    {
        let notes = warnings.iter().filter_map(Value::as_str).take(3).collect::<Vec<_>>().join("; ");
        if !notes.is_empty() {
            text.push_str(&format!("; notes: {notes}"));
        }
    }
    text
}

#[cfg(test)]
mod brush_pack_ui_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    const MYB: &[u8] = include_bytes!("../../pc-io/tests/fixtures/brushes/charcoal.myb");
    #[derive(Debug)]
    struct Drop(std::path::PathBuf);
    impl egui::DroppedFile for Drop {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            Ok(MYB.to_vec())
        }
    }
    #[test]
    fn brush_batch_import_button_and_drop_into_brushes_panel() {
        let mut picked = false;
        let services = crate::Services {
            pick_open: Some(Box::new(move || {
                if picked {
                    None
                } else {
                    picked = true;
                    Some(("Artist.myb".into(), Ok(MYB.to_vec())))
                }
            })),
            ..Default::default()
        };
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        app.ui.panels.brush_settings = true;
        app.ui.brush_tab = 1;
        let mut h = Harness::builder().with_size(egui::vec2(1000.0, 900.0)).build_ui_state(
            |ui, a: &mut PhotocraftApp| {
                if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                    return;
                }
                let files = ui.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
                if !files.is_empty() {
                    a.open_dropped(ui.ctx(), files, ui.ctx().pointer_latest_pos());
                }
                crate::brush_panel::window(a, ui.ctx());
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, Default::default());
        h.run_steps(4);
        let at = h.get_by_label("Import Brushes…").rect().center();
        h.get_by_label("Import Brushes…").click();
        h.run_steps(2);
        assert_eq!(h.state().session.tools.presets.iter().filter(|p| p.group == "Artist").count(), 1);
        assert!(h.state().ui.status.starts_with("Imported 1 brushes, 1 approximate, 0 skipped"));
        h.hover_at(at);
        h.input_mut().dropped_files.push(std::sync::Arc::new(Drop("Dropped.myb".into())));
        h.run_steps(1);
        assert_eq!(h.state().session.tools.presets.iter().filter(|p| p.group == "Dropped").count(), 1);
        assert!(h.state().session.documents().is_empty());
    }
    #[test]
    fn brush_batch_summary_keeps_skipped_reasons() {
        assert_eq!(
            summary(&json!({"count":46,"approximate":3,"skipped":["smudge: unavailable engine","broken tip"]})),
            "Imported 46 brushes, 3 approximate, 2 skipped (smudge: unavailable engine; broken tip)"
        );
    }
}
