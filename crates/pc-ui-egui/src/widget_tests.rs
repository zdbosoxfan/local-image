//! Windowless integration of the editor facades and shared Library widget profile.
//! The CPU renderer is the shared `li-testkit` software painter; no GPU/window is created.
use egui::{Color32, Rect, Sense, Vec2, vec2};
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
use li_ui_kit::widgets::{self as kit, Profile, SelectionStyle};

/// The shared CPU kittest renderer (`li-testkit`): no GPU or window.
fn cpu_renderer() -> li_testkit::SoftpaintRenderer {
    li_testkit::SoftpaintRenderer::with_clear_from(|ctx: &egui::Context| crate::theme::Tokens::get(ctx).chrome)
}

#[derive(Default)]
struct Gallery {
    editor_checked: bool,
    library_checked: bool,
    editor_locked: bool,
    library_locked: bool,
    editor_tab: bool,
    library_tab: bool,
    editor_clicks: usize,
    library_clicks: usize,
    choice: u8,
    library_choice: u8,
    value: f32,
    bounds: Vec<(&'static str, Rect)>,
}

fn gallery(ui: &mut egui::Ui, state: &mut Gallery) {
    use crate::widgets as editor;
    state.bounds.clear();
    editor::section_label_translated(ui, "Editor profile");
    let response = editor::pill_tab_translated(ui, "Editor tab", state.editor_tab);
    state.bounds.push(("editor tab", response.rect));
    if response.clicked() {
        state.editor_tab = !state.editor_tab;
    }
    let response = editor::primary_button(ui, "Editor action", 100.0);
    state.bounds.push(("editor button", response.rect));
    if response.clicked() {
        state.editor_clicks += 1;
    }
    let response = editor::checkbox(ui, &mut state.editor_checked, "Editor checkbox");
    state.bounds.push(("editor checkbox", response.rect));
    ui.add_enabled_ui(false, |ui| {
        editor::checkbox(ui, &mut state.editor_locked, "Editor locked");
    });
    editor::icon_action(ui, "plus", 24.0, "Editor icon", "Add item", None);
    let (tile, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
    editor::color_tile(ui, tile, Color32::from_rgba_unmultiplied(120, 60, 160, 160));
    let available = ui.available_width();
    editor::dropdown(ui, "choice", &mut state.choice, &[(0, "First choice"), (1, "Second choice")], available);
    let response = editor::slider(ui, &mut state.value, 0.0..=100.0, None);
    response.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), f64::from(state.value), "Editor slider"));
    state.bounds.push(("editor slider", response.rect));
    editor::hairline(ui);
    kit::heading(ui, "Library profile", Profile::Library, true);
    let response = kit::selection(ui, "Library tab", state.library_tab, SelectionStyle::Segment, None);
    state.bounds.push(("library tab", response.rect));
    if response.clicked() {
        state.library_tab = !state.library_tab;
    }
    let response = kit::selection(ui, "Library action", false, SelectionStyle::Text, None);
    state.bounds.push(("library button", response.rect));
    if response.clicked() {
        state.library_clicks += 1;
    }
    let response = kit::checkbox(ui, &mut state.library_checked, "Library checkbox", Profile::Library);
    state.bounds.push(("library checkbox", response.rect));
    ui.add_enabled_ui(false, |ui| {
        kit::checkbox(ui, &mut state.library_locked, "Library locked", Profile::Library);
    });
    kit::icon_button(ui, Vec2::splat(24.0), false, true, "Library icon", Profile::Library, |ui, rect, color| {
        crate::icons::paint(ui, rect, "plus", 16.0, color);
    });
    let (tile, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
    kit::color_tile(ui, tile, Color32::from_rgba_unmultiplied(120, 60, 160, 160), Profile::Library);
    let available = ui.available_width();
    kit::choice_menu(ui, "library-choice", if state.library_choice == 0 { "Library first" } else { "Library second" }, available, |ui| {
        for (value, label) in [(0, "Library first"), (1, "Library second")] {
            if ui.selectable_label(state.library_choice == value, label).clicked() {
                state.library_choice = value;
            }
        }
    });
    let library = Profile::Library.tokens(ui.ctx());
    let (track, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    li_ui_kit::slider::paint_hollow(
        ui,
        track,
        None,
        track.center().x,
        true,
        false,
        li_ui_kit::slider::HollowStyle {
            track: library.library.track,
            background: library.chrome,
            thumb: library.library.thumb,
            thumb_hover: library.library.thumb_hover,
            disabled: library.library.text_disabled,
        },
    );
    kit::separator(ui, Profile::Library, None);
}

fn harness(kind: crate::theme::ThemeKind, width: f32) -> Harness<'static, Gallery> {
    let mut initialized = false;
    let mut harness = Harness::builder().with_size(vec2(width, 650.0)).renderer(cpu_renderer()).build_ui_state(
        move |ui, state| {
            if !initialized {
                // Named Inter faces become available on the next egui pass.
                li_ui_kit::fonts::install_editor(ui.ctx());
                crate::theme::apply(ui.ctx(), kind);
                ui.ctx().data_mut(|data| data.insert_temp(egui::Id::NULL, li_ui_kit::library::for_kind(kind)));
                initialized = true;
                ui.ctx().request_repaint();
                return;
            }
            gallery(ui, state);
        },
        Gallery::default(),
    );
    harness.run_steps(3);
    harness
}

#[test]
fn widget_facades_real_clicks_keyboard_and_cpu_gallery() {
    for kind in [crate::theme::ThemeKind::ProMedium, crate::theme::ThemeKind::StudioLight] {
        for width in [180.0, 270.0] {
            let mut h = harness(kind, width);
            for (label, rect) in &h.state().bounds {
                assert!(h.ctx.content_rect().expand(1.0).contains_rect(*rect), "{kind:?}/{width}: {label} exceeds viewport: {rect:?}");
            }
            let started = std::time::Instant::now();
            h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Editor checkbox").click();
            h.run_steps(2);
            h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Library checkbox").click();
            h.run_steps(2);
            h.run_steps(3);
            assert!(h.state().editor_checked && h.state().library_checked);
            h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Editor checkbox").focus();
            h.run_steps(1);
            h.key_press(egui::Key::Space);
            h.run_steps(2);
            assert!(!h.state().editor_checked, "focused editor checkbox must toggle with Space");
            h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Library checkbox").focus();
            h.run_steps(1);
            h.key_press(egui::Key::Space);
            h.run_steps(2);
            assert!(!h.state().library_checked, "focused Library checkbox must toggle with Space");
            h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Editor locked").click();
            h.run_steps(2);
            h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Library locked").click();
            h.run_steps(2);
            h.run_steps(2);
            assert!(!h.state().editor_locked && !h.state().library_locked);
            h.get_by_label("Editor tab").click();
            h.run_steps(2);
            h.get_by_label("Library tab").click();
            h.run_steps(2);
            h.get_by_label("Editor action").click();
            h.run_steps(2);
            h.get_by_label("Library action").click();
            h.run_steps(2);
            h.run_steps(3);
            assert!(h.state().editor_tab && h.state().library_tab);
            assert_eq!((h.state().editor_clicks, h.state().library_clicks), (1, 1));
            h.get(By::new().role(egui::accesskit::Role::ComboBox).value("First choice")).click();
            h.run_steps(2);
            h.run_steps(2);
            h.get_by_label("Second choice").click();
            h.run_steps(2);
            h.run_steps(2);
            assert_eq!(h.state().choice, 1);
            h.get(By::new().role(egui::accesskit::Role::ComboBox).value("Library first")).click();
            h.run_steps(2);
            h.get_by_label("Library second").click();
            h.run_steps(2);
            assert_eq!(h.state().library_choice, 1);
            for label in ["Editor action", "Library action", "Editor tab", "Library tab"] {
                h.get_by_label(label).focus();
                h.run_steps(1);
                h.key_press(egui::Key::Space);
                h.run_steps(2);
            }
            assert_eq!((h.state().editor_clicks, h.state().library_clicks), (2, 2), "Space activates both button profiles");
            assert!(!h.state().editor_tab && !h.state().library_tab, "Space activates both selection profiles");
            let slider = h.get_by_label("Editor slider").rect();
            h.hover_at(slider.center());
            h.drag_at(slider.center());
            h.run_steps(2);
            h.hover_at(slider.right_center());
            h.run_steps(2);
            h.drop_at(slider.right_center());
            h.run_steps(2);
            assert_eq!(h.state().value, 100.0, "the real editor slider facade drag reaches its upper endpoint");
            let image = h.render().expect("CPU gallery render");
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            println!(
                "real shared-widget pointer/keyboard sample {kind:?}/{width}: {elapsed_ms:.3} ms incl final CPU frame; {}x{} pixels",
                image.width(),
                image.height()
            );
            assert_eq!(image.width(), width as u32);
            assert_eq!(image.height(), 650);
            let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/design-system/widgets");
            std::fs::create_dir_all(&target).unwrap();
            image.save(target.join(format!("{kind:?}-{}.png", width as u32))).unwrap();
        }
    }
}
