use super::*;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use photocraft_doc::GradientMethod;

fn tab_to(h: &mut Harness<'_, PhotocraftApp>, role: egui::accesskit::Role, label: &str) {
    for _ in 0..32 {
        if h.get_by_role_and_label(role, label).accesskit_node().is_focused() {
            return;
        }
        h.key_press(egui::Key::Tab);
        h.run_steps(2);
    }
    assert!(h.get_by_role_and_label(role, label).accesskit_node().is_focused(), "Tab reaches {label}");
}

#[test]
fn properties_method_uses_keyboard_and_commits_one_undo_step() {
    let mut app = tests::app_with_gradient("linear");
    app.run(cmds::CREATE, json!({"from":[0,0],"to":[200,0],"dither":false})).unwrap();
    let mut h = Harness::builder().with_size(vec2(250.0, 700.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            if !ui.ctx().fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                return;
            }
            let (layer, ..) = active_gradient(app).unwrap();
            properties(app, ui, &layer);
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(3);
    let before = h.state().session.active().unwrap().history.past_len();
    let method = h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Method");
    assert_eq!(method.accesskit_node().value().as_deref(), Some("Classic"));
    tab_to(&mut h, egui::accesskit::Role::ComboBox, "Method");
    h.key_press(egui::Key::Space);
    h.run_steps(3);
    tab_to(&mut h, egui::accesskit::Role::Button, "Perceptual");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state_mut().run(cmds::GET, json!({})).unwrap()["method"], "perceptual");
    assert_eq!(h.state().session.active().unwrap().history.past_len(), before + 1);
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(h.state_mut().run(cmds::GET, json!({})).unwrap()["method"], "classic");
}

#[test]
fn options_method_reaches_live_and_destructive_drags() {
    let mut app = tests::app_with_gradient("linear");
    app.run(cmds::CREATE, json!({"from":[20,60],"to":[180,60],"method":"perceptual"})).unwrap();
    let before = app.ui.tool_options.clone();
    let history = app.session.active().unwrap().history.past_len();
    options_changed(&mut app, &before);
    assert_eq!(app.ui.tool_options.gradient_method, GradientMethod::Perceptual);
    assert_eq!(app.session.active().unwrap().history.past_len(), history);
    app.run("edit.undo", json!({})).unwrap();
    app.ui.tool_options.gradient_method = GradientMethod::Perceptual;
    tests::drag(&mut app, [20.0, 60.0], [180.0, 60.0]);
    assert_eq!(tests::get(&mut app)["method"], "perceptual");
    let before = app.ui.tool_options.clone();
    app.ui.tool_options.gradient_method = GradientMethod::Classic;
    let history = app.session.active().unwrap().history.past_len();
    options_changed(&mut app, &before);
    assert_eq!(tests::get(&mut app)["method"], "classic");
    assert_eq!(app.session.active().unwrap().history.past_len(), history + 1);
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(tests::get(&mut app)["method"], "perceptual");
    app.run("layer.new.layer", json!({})).unwrap();
    app.ui.tool_options.gradient_classic = true;
    app.ui.tool_options.gradient_method = GradientMethod::Perceptual;
    app.ui.tool_options.gradient_dither = false;
    tests::drag(&mut app, [20.0, 60.0], [180.0, 60.0]);
    let d = app.session.active().unwrap();
    let px = d.doc.layer(d.active_layer.unwrap()).unwrap().surface().unwrap().pixel(100, 60);
    // Foreground black to background white: a Perceptual midpoint is about 99/255.
    assert!((px[0] - 0.388573).abs() < 2.0 / 255.0, "{px:?}");
}

#[test]
fn options_bar_method_keyboard_choice_edits_the_selected_fill_once() {
    let mut app = tests::app_with_gradient("linear");
    app.run(cmds::CREATE, json!({"from":[20,60],"to":[180,60],"dither":false})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1440.0, 140.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            if !ui.ctx().fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                return;
            }
            crate::panels::options_bar(app, ui);
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(3);
    let history = h.state().session.active().unwrap().history.past_len();
    let method = h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Method");
    assert_eq!(method.accesskit_node().value().as_deref(), Some("Classic"));
    method.focus();
    h.key_press(egui::Key::Space);
    h.run_steps(3);
    h.get_by_label("Perceptual").focus();
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state_mut().run(cmds::GET, json!({})).unwrap()["method"], "perceptual");
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    assert_eq!(h.state().ui.tool_options.gradient_method, GradientMethod::Perceptual);
}
