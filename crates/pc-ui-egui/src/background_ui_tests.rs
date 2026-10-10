use std::cell::RefCell;

use egui::{Event, Modifiers, PointerButton, Pos2, Rect};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use photocraft_doc::{Color, ColorMode, Document, LayerId, SampleType, Size};
use photocraft_engine::{
    cutout_cmds::{BackgroundOutput, apply_output},
    jobs::JobCtx,
};
use photocraft_geom::Rect as ImageRect;
use photocraft_raster::Surface;

use super::*;
use crate::state::Tool;

thread_local! {
    static MATTE: RefCell<Option<Surface>> = const { RefCell::new(None) };
    static REQUESTS: RefCell<Vec<(String, Value)>> = const { RefCell::new(Vec::new()) };
}

/// Inject only the model's matte; the real shared engine output still edits the document.
/// This makes UI clicks and Qwen requests testable without ComfyUI, sockets or installed models.
pub(super) fn injected_result(app: &mut PhotocraftApp, cmd: &str, p: &Value) -> Option<Result<Value, String>> {
    let matte = MATTE.with(|m| m.borrow().clone())?;
    REQUESTS.with(|r| r.borrow_mut().push((cmd.to_owned(), p.clone())));
    let id = p["layer"].as_u64().map(LayerId).or(app.session.active()?.active_layer)?;
    Some((|| {
        let output = BackgroundOutput::from_params(p, cmd).map_err(|e| e.to_string())?;
        let result = app
            .session
            .edit("Remove Background", |doc, active| {
                let id = apply_output(doc, id, matte, &output, &JobCtx::new())?;
                *active = Some(id);
                Ok(id)
            })
            .map_err(|e| e.to_string())?;
        Ok(json!({"layer": result.0}))
    })())
}

struct Ready;
impl Ready {
    fn new() -> Self {
        let mut st = crate::ai_ui::EngineStatus { checked: true, connected: true, ..Default::default() };
        for variant in ["int8", "bf16"] {
            st.presets.insert(format!("{}:{variant}", ModelId::Qwen.key()), Ok(()));
        }
        crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(st));
        let mut matte = Surface::new(photocraft_color::PixelFormat::GRAY8);
        matte.write_region(ImageRect::new(2, 1, 8, 5), &[1.0; 24]);
        MATTE.with(|m| *m.borrow_mut() = Some(matte));
        REQUESTS.with(|r| r.borrow_mut().clear());
        Self
    }
}
impl Drop for Ready {
    fn drop(&mut self) {
        crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = None);
        MATTE.with(|m| *m.borrow_mut() = None);
        REQUESTS.with(|r| r.borrow_mut().clear());
    }
}

fn app() -> PhotocraftApp {
    let mut doc = Document::with_background("Photo", Size::new(12, 8), ColorMode::Rgb, SampleType::F32, Color::WHITE);
    doc.layers[0].name = "Photo".into();
    doc.layers[0].locks = Default::default();
    let data: Vec<_> = (0..96).flat_map(|i| [i as f32 / 96.0, 0.2, 0.7, 1.0]).collect();
    let bounds = doc.bounds();
    doc.layers[0].surface_mut().unwrap().write_region(bounds, &data);
    let mut s = photocraft_engine::Session::new();
    s.add_document(doc, None);
    let mut app = PhotocraftApp::new(s, crate::Services::default());
    app.ui.ai.cutout_engine = "qwen-int8".into();
    app.ui.tool = Tool::Move;
    for group in
        [crate::dock::Group::Color, crate::dock::Group::Character, crate::dock::Group::Navigator, crate::dock::Group::History, crate::dock::Group::Layers]
    {
        app.ui.dock.set_collapsed(group, true);
    }
    app.ui.dock.heights.insert(crate::dock::Group::Properties, 700.0);
    app
}

fn harness() -> Harness<'static, PhotocraftApp> {
    let app = app();
    let mut h = Harness::builder().with_size(vec2(1680.0, 1100.0)).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app
    });
    h.run_steps(6);
    h
}

fn click(h: &mut Harness<'_, PhotocraftApp>, at: Pos2) {
    h.hover_at(at);
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(4);
}

fn properties_rect(h: &Harness<'_, PhotocraftApp>) -> Rect {
    let headers = crate::props_layout::sections_drawn(&h.ctx);
    let start = headers.iter().find(|(n, _)| n == "Remove Background").unwrap().1;
    let end = headers.iter().find(|(n, _)| n == "Quick Actions").unwrap().1;
    Rect::from_min_max(start.left_bottom(), end.right_top())
}

fn in_rect(h: &Harness<'_, PhotocraftApp>, label: &str, area: Rect) -> Pos2 {
    h.query_all_by_label(label)
        .chain(h.query_all_by_value(label))
        .map(|n| n.rect())
        .find(|r| area.contains(r.center()))
        .unwrap_or_else(|| panic!("missing {label} in {area:?}"))
        .center()
}

fn choose(h: &mut Harness<'_, PhotocraftApp>, old: &str, new: &str, area: Rect) {
    let at = in_rect(h, old, area);
    click(h, at);
    let at = h
        .query_all_by_label(new)
        .map(|n| n.rect())
        .find(|r| h.ctx.layer_id_at(r.center()).is_some_and(|l| l.order == egui::Order::Foreground))
        .expect("dropdown option")
        .center();
    click(h, at);
}

#[test]
fn background_ui_properties_clicks_every_output_and_undoes_once() {
    let _ready = Ready::new();
    for (key, label) in OUTPUTS {
        let mut h = harness();
        let area = properties_rect(&h);
        choose(&mut h, "Qwen AI · Compact", "Qwen AI · Full", area);
        assert_eq!(h.state().ui.ai.cutout_engine, "qwen-bf16");
        if key != "mask" {
            let area = properties_rect(&h);
            choose(&mut h, "Layer mask", label, area);
        }
        assert_eq!(h.state().ui.ai.cutout_output, key);
        let before = h.state().session.active().unwrap().doc.clone();
        let id = before.layers[0].id;
        let history = h.state().session.active().unwrap().history.past_len();
        let at = in_rect(&h, "Remove Background", properties_rect(&h));
        click(&mut h, at);
        let (cmd, p) = REQUESTS.with(|r| r.borrow().last().unwrap().clone());
        assert_eq!(cmd, "ai.removeBackground");
        assert_eq!(p["engine"], "qwen-bf16");
        assert_eq!(p["output"], key);
        assert_eq!(p["layer"], id.0);
        let s = &h.state().session;
        let st = s.active().unwrap();
        assert_eq!(st.history.past_len(), history + 1);
        let layer = st.doc.layer(st.active_layer.unwrap()).unwrap();
        if key == "transparent" {
            assert!(layer.mask.is_none());
            assert_eq!(layer.surface().unwrap().rgba(0, 0)[3], 0.0);
            assert_eq!(layer.surface().unwrap().rgba(3, 2)[3], 1.0);
        } else {
            assert_eq!(layer.mask.as_ref().unwrap().value(0, 0), 0.0);
            assert_eq!(layer.mask.as_ref().unwrap().value(3, 2), 1.0);
        }
        assert_eq!(st.doc.layers.len(), if matches!(key, "mask" | "transparent") { 1 } else { 2 });
        if key == "newLayer" {
            assert!(!st.doc.layer(id).unwrap().visible);
        }
        h.state_mut().session.undo();
        assert_eq!(h.state().session.active().unwrap().doc, before);
    }
}

#[test]
fn background_ui_surfaces_share_settings_and_requests() {
    let _ready = Ready::new();
    let mut h = harness();
    let hint = in_rect(&h, "Keep… (optional)", properties_rect(&h));
    click(&mut h, hint);
    h.event(Event::Text("the red car".into()));
    h.run_steps(4);
    h.state_mut().ui.tool = Tool::AiCutout;
    h.run_steps(4);
    let area = properties_rect(&h);
    choose(&mut h, "Qwen AI · Compact", "Qwen AI · Full", area);
    let area = properties_rect(&h);
    choose(&mut h, "Layer mask", "White background", area);
    let toolbar = Rect::from_min_max(Pos2::ZERO, egui::pos2(1680.0, 80.0));
    in_rect(&h, "Qwen AI · Full", toolbar);
    in_rect(&h, "White background", toolbar);
    // Changing the options bar immediately updates Properties.
    choose(&mut h, "White background", "Blur background", toolbar);
    assert_eq!(h.state().ui.ai.cutout_output, "blur");
    in_rect(&h, "Blur background", properties_rect(&h));
    h.state_mut().ui.tool = Tool::Move;
    h.run_steps(4);
    let at = h.get_by_label("Background removal options").rect().center();
    click(&mut h, at);
    let popup = h
        .ctx
        .memory(|m| {
            m.areas()
                .visible_layer_ids()
                .into_iter()
                .filter(|l| l.order == egui::Order::Foreground && l.id != egui::Id::new("li-context-bar"))
                .find_map(|l| m.area_rect(l.id))
        })
        .unwrap();
    in_rect(&h, "Qwen AI · Full", popup);
    let at = in_rect(&h, "Qwen AI · Compact", popup);
    click(&mut h, at);
    in_rect(&h, "Qwen AI · Compact", properties_rect(&h));
    let at = h.get_by_label("Background removal options").rect().center();
    click(&mut h, at);
    let at = h.get_by_label("Cutout on a new layer").rect().center();
    click(&mut h, at);
    assert_eq!(h.state().ui.ai.cutout_output, "newLayer");
    in_rect(&h, "Cutout on a new layer", properties_rect(&h));
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    let bar = h.ctx.memory(|m| m.area_rect(egui::Id::new("li-context-bar"))).unwrap();
    let at = in_rect(&h, "Remove Background", bar);
    click(&mut h, at);
    let (_, p) = REQUESTS.with(|r| r.borrow().last().unwrap().clone());
    assert_eq!(p["output"], "newLayer");
    assert_eq!(p["engine"], "qwen-int8");
    assert_eq!(p["hint"], "the red car");
    let st = h.state().session.active().unwrap();
    assert_eq!(st.doc.layers.len(), 2);
    assert!(!st.doc.layers[0].visible);
    assert!(st.doc.layers[1].mask.is_some());
    h.state_mut().session.undo();
    h.state_mut().ui.tool = Tool::AiCutout;
    h.run_steps(4);
    in_rect(&h, "Qwen AI · Compact", toolbar);
    in_rect(&h, "Cutout on a new layer", toolbar);
    let at = in_rect(&h, "Remove Background", toolbar);
    click(&mut h, at);
    let (_, p) = REQUESTS.with(|r| r.borrow().last().unwrap().clone());
    assert_eq!(p["output"], "newLayer");
    assert_eq!(p["hint"], "the red car");
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 2);
}

#[test]
fn background_ui_colour_picker_and_blur_drag_reach_the_request() {
    let _ready = Ready::new();
    let mut h = harness();
    let area = properties_rect(&h);
    choose(&mut h, "Layer mask", "Colour background", area);
    let at = in_rect(&h, "Background colour", properties_rect(&h));
    click(&mut h, at);
    let picker = h
        .ctx
        .memory(|m| {
            m.areas()
                .visible_layer_ids()
                .into_iter()
                .filter(|l| l.order == egui::Order::Foreground && l.id != egui::Id::new("li-context-bar"))
                .find_map(|l| m.area_rect(l.id))
        })
        .unwrap();
    click(&mut h, picker.min + vec2(60.0, picker.height() * 0.3));
    assert_ne!(h.state().ui.ai.cutout_color, [1.0; 3]);
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    let at = in_rect(&h, "Remove Background", properties_rect(&h));
    click(&mut h, at);
    let p = REQUESTS.with(|r| r.borrow().last().unwrap().1.clone());
    assert_eq!(p["color"], json!(h.state().ui.ai.cutout_color));
    let area = properties_rect(&h);
    choose(&mut h, "Colour background", "Blur background", area);
    let at = in_rect(&h, "Background blur amount", properties_rect(&h));
    let before = h.state().ui.ai.cutout_blur;
    h.hover_at(at);
    h.step();
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    for i in 1..=5 {
        h.event(Event::PointerMoved(at + vec2(i as f32 * 5.0, 0.0)));
        h.step();
    }
    h.event(Event::PointerButton { pos: at + vec2(25.0, 0.0), button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(4);
    assert!(h.state().ui.ai.cutout_blur > before);
    let at = in_rect(&h, "Remove Background", properties_rect(&h));
    click(&mut h, at);
    let p = REQUESTS.with(|r| r.borrow().last().unwrap().1.clone());
    assert_eq!(p["amount"], json!(h.state().ui.ai.cutout_blur));
}

#[test]
fn background_ui_readiness_and_collapsing_are_clickable() {
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(Default::default()));
    let mut h = harness();
    let area = properties_rect(&h);
    let at = in_rect(&h, "Remove Background", area);
    assert!(h.query_all_by_label("Remove Background").find(|n| n.rect().contains(at)).unwrap().accesskit_node().is_disabled());
    let at = in_rect(&h, "Set up AI…", area);
    click(&mut h, at);
    // The setup window is egui-only; this does not start ComfyUI or a native window.
    assert!(h.query_by_label("Local AI").is_some());
    h.key_press(egui::Key::Escape);
    let at = h.get_by_label("Close window").rect().center();
    click(&mut h, at);
    h.run_steps(4);
    let header = crate::props_layout::sections_drawn(&h.ctx).into_iter().find(|(n, _)| n == "Remove Background").unwrap().1;
    click(&mut h, header.center());
    assert_eq!(h.ctx.data(|d| d.get_temp::<bool>(egui::Id::new(("props-section", "remove-background")))), Some(false));
    assert!(h.query_by_value("Layer mask").is_none());
    h.state_mut().ui.tool = Tool::AiCutout;
    h.run_steps(4);
    let toolbar = Rect::from_min_max(Pos2::ZERO, egui::pos2(1680.0, 80.0));
    in_rect(&h, "Set up AI…", toolbar);
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = None);
}

#[test]
fn background_ui_background_and_studio_properties_offer_removal() {
    let _ready = Ready::new();
    for theme in [crate::theme::ThemeKind::ProMedium, crate::theme::ThemeKind::Studio] {
        for background in [false, true] {
            let mut h = harness();
            if background {
                h.state_mut()
                    .session
                    .edit("Background", |doc, _| {
                        doc.layers[0].name = "Background".into();
                        doc.layers[0].locks.transparency = true;
                        doc.layers[0].locks.position = true;
                        Ok(())
                    })
                    .unwrap();
                h.ctx.data_mut(|d| {
                    for id in ["canvas", "rulers", "guides"] {
                        d.insert_temp(egui::Id::new(("doc-props-section", id)), false);
                    }
                });
            }
            let ctx = h.ctx.clone();
            h.state_mut().set_theme(&ctx, theme);
            h.run_steps(6);
            let header = crate::props_layout::sections_drawn(&h.ctx)
                .into_iter()
                .find(|(n, _)| n == "Remove Background")
                .expect("section in Studio and document Properties")
                .1;
            let at = h.query_all_by_label("Remove Background").map(|n| n.rect()).find(|r| r.top() > header.bottom()).unwrap().center();
            click(&mut h, at);
            let st = h.state().session.active().unwrap();
            assert!(st.doc.layers[0].mask.is_some());
            assert!(!st.doc.layers[0].locks.transparency || !background);
        }
    }
}

#[test]
fn background_ui_requests_keep_model_hint_and_output_parameters() {
    let mut ai = crate::ai_ui::AiOptions { cutout_hint: "the red car".into(), cutout_color: [0.2, 0.3, 0.4], cutout_blur: 23.0, ..Default::default() };
    for (model, _) in MODELS {
        ai.cutout_engine = model.into();
        for (output, _) in OUTPUTS {
            ai.cutout_output = output.into();
            let (cmd, p) = request(&ai, Some(7));
            assert_eq!(cmd, if model == "quick" { "layer.removeBackground" } else { "ai.removeBackground" });
            assert_eq!(p["engine"], if model == "quick" { "learned" } else { model });
            assert_eq!(p["output"], output);
            assert_eq!(p["layer"], 7);
            assert_eq!(p.get("hint").cloned(), if model == "quick" { None } else { Some(json!(ai.cutout_hint)) });
            assert_eq!(p.get("color").cloned(), if output == "color" { Some(json!(ai.cutout_color)) } else { None });
            assert_eq!(p.get("amount").cloned(), if output == "blur" { Some(json!(ai.cutout_blur)) } else { None });
        }
    }
}

#[test]
fn background_ui_old_settings_load_and_new_settings_roundtrip() {
    let ai: crate::ai_ui::AiOptions = serde_json::from_value(json!({"cutout_engine":"qwen-bf16","cutout_hint":"dog"})).unwrap();
    assert_eq!(ai.cutout_output, "mask");
    assert_eq!(ai.cutout_blur, 12.0);
    assert_eq!(ai.cutout_color, [1.0; 3]);
    assert_eq!(ai.cutout_hint, "dog");
    let ai = crate::ai_ui::AiOptions { cutout_output: "color".into(), cutout_color: [0.1, 0.2, 0.3], ..ai };
    assert_eq!(serde_json::from_value::<crate::ai_ui::AiOptions>(serde_json::to_value(&ai).unwrap()).unwrap(), ai);
}
