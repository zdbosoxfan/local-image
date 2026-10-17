//! Design-system screenshots of the new editor UI: the Swatches panel, the shortcut editor's
//! scope/profile row, and the batch Remove Backgrounds window, in a dark and a light theme at a
//! narrow and a normal width, into `target/design-system/`. `DESIGN_SYSTEM_PHASE` names the set
//! (`before`/`after`). CPU `li-testkit` renderer (any machine, no GPU or window); run explicitly
//! with `--ignored`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_engine::Session;

/// The shared deterministic CPU renderer (`li-testkit`): the same pixels on any machine, no
/// GPU or window. These screens draw only meshes (no wgpu paint callbacks).
pub(crate) fn renderer() -> li_testkit::SoftpaintRenderer {
    li_testkit::SoftpaintRenderer::new()
}

fn output(name: &str) -> std::path::PathBuf {
    let phase = std::env::var("DESIGN_SYSTEM_PHASE").unwrap_or_else(|_| "after".into());
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/design-system");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{phase}-editor-{name}.png"))
}

const THEMES: [(crate::theme::ThemeKind, &str); 2] = [(crate::theme::ThemeKind::Pro, "dark"), (crate::theme::ThemeKind::StudioLight, "light")];

pub(crate) fn fonts_ready(ui: &egui::Ui) -> bool {
    let ready = egui::Id::new("design-system-fonts-ready");
    if ui.ctx().data(|d| d.get_temp::<bool>(ready)).unwrap_or(false) {
        return true;
    }
    crate::theme::install_fonts(ui.ctx());
    crate::theme::apply(ui.ctx(), crate::theme::ThemeKind::Pro);
    ui.ctx().data_mut(|d| d.insert_temp(ready, true));
    ui.ctx().request_repaint();
    false
}

#[test]
#[ignore = "design-system screenshots: run explicitly with --ignored"]
fn design_system_editor_screens() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // Swatches panel, a swatch selected (footer Delete enabled).
    let mut app = PhotocraftApp::new(Session::new(), crate::Services::default());
    app.run("file.new", serde_json::json!({"width":96,"height":64})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(250., 760.)).renderer(renderer()).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            if !fonts_ready(ui) {
                return;
            }
            egui_extras::install_image_loaders(ui.ctx());
            ui.set_min_height(ui.available_height());
            crate::panels::swatches_panel::panel(app, ui);
        },
        app,
    );
    h.run_steps(4);
    h.get_by_label("Starter 2").click();
    h.run();
    for (theme, name) in THEMES {
        crate::theme::apply(&h.ctx, theme);
        for width in [250., 400.] {
            h.set_size(egui::vec2(width, 760.));
            h.run();
            h.render().unwrap().save(output(&format!("swatches-{name}-{}", width as u32))).unwrap();
        }
    }
    drop(h);

    // Shortcut editor: scope, profile and its buttons.
    let mut app = PhotocraftApp::new(Session::new(), Default::default());
    let id = open_shortcuts(&mut app, 0);
    let fields = app.ui.dialog_mut(id).unwrap().fields.clone();
    app.ui.dialogs.clear();
    let mut h = Harness::builder().with_size(egui::vec2(480., 260.)).renderer(renderer()).build_ui_state(
        |ui, (app, fields): &mut (PhotocraftApp, Map<String, Value>)| {
            if fonts_ready(ui) {
                shortcuts_body(app, ui, fields);
            }
        },
        (app, fields),
    );
    h.run_steps(4);
    h.state_mut().1.insert("filter".into(), json!("edit.search"));
    for (theme, name) in THEMES {
        crate::theme::apply(&h.ctx, theme);
        for width in [480., 900.] {
            h.set_size(egui::vec2(width, 260.));
            h.run();
            h.render().unwrap().save(output(&format!("shortcuts-{name}-{}", width as u32))).unwrap();
        }
    }
    drop(h);

    // Batch Remove Backgrounds window (shared background-removal settings and the batch section).
    let mut h = Harness::builder().with_size(egui::vec2(640., 640.)).renderer(renderer()).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            if fonts_ready(ui) {
                crate::generate_ui::batch_window(app, ui.ctx());
            }
        },
        PhotocraftApp::new(Session::new(), Default::default()),
    );
    crate::generate_ui::open_batch();
    h.run_steps(4);
    for (theme, name) in THEMES {
        crate::theme::apply(&h.ctx, theme);
        for width in [520., 640.] {
            h.set_size(egui::vec2(width, 640.));
            h.run();
            h.render().unwrap().save(output(&format!("batch-bg-{name}-{}", width as u32))).unwrap();
        }
    }
}

#[test]
#[ignore = "offscreen hardware visual QA, run explicitly"]
fn display_output_preferences_shots() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let info = crate::gpu_canvas::GpuInfo {
        output_depth: 8,
        output_note: "X11 / XWayland uses 8-bit output; pen tablet compatibility mode forces this backend".into(),
        ..Default::default()
    };
    let lines = json!(info.lines());
    let mut h = Harness::builder().with_size(egui::vec2(400.0, 600.0)).renderer(renderer()).build_ui_state(
        move |ui, values: &mut Map<String, Value>| {
            if fonts_ready(ui) {
                gpu_status_rows(ui, Some(&lines), None, values);
            }
        },
        Map::new(),
    );
    for ppp in [1.0, 1.5] {
        h.set_pixels_per_point(ppp);
        for (theme, name) in THEMES {
            crate::theme::apply(&h.ctx, theme);
            for width in [250.0, 400.0] {
                h.set_size(egui::vec2(width, 600.0));
                h.run_steps(3);
                h.render().unwrap().save(output(&format!("display-output-{name}-{}-{ppp}x", width as u32))).unwrap();
            }
        }
    }
}
