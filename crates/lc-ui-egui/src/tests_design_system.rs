//! Design-system screenshots of the new Develop UI (layer bar, whole-image notices, Skin Tone,
//! canvas layer pill and picker badge): dark/light, 250/400 pt panels, into
//! `target/design-system/`. `DESIGN_SYSTEM_PHASE` names the set (`before`/`after`). CPU raster
//! through the Library/Develop headless driver; run explicitly with `--ignored`.
use crate::{LightcraftApp, Services, headless::Headless, state::RightPanel, theme::Tokens};
use egui::{Rect, pos2, vec2};
use serde_json::{Value, json};
use std::time::Duration;

const T: Duration = Duration::from_secs(40);

fn request(h: &mut Headless, command: &str, params: Value) -> Value {
    let r = h.request(command, params, T);
    assert_eq!(r["ok"], true, "{command}: {r}");
    h.step();
    r["result"].clone()
}
fn click(h: &mut Headless, id: &str) {
    request(h, "ui.clickWidget", json!({"id":id}));
}
fn rect(h: &Headless, id: &str) -> Rect {
    h.app.widgets.iter().rev().find(|(name, _)| name == id).unwrap_or_else(|| panic!("missing {id}")).1
}
fn setup(session: lightcraft_engine::Session, width: f32, dark: bool) -> Headless {
    let mut app = LightcraftApp::new(session, Services { png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    app.ui.view = crate::state::ViewMode::Detail;
    app.ui.right = RightPanel::Edit;
    app.ui.histogram = false;
    app.ui.left_panel = false;
    app.ui.filmstrip = false;
    app.ui.right_width = width;
    app.set_host_theme(if dark { Tokens::default() } else { super::tests_develop_c::light() });
    let mut h = Headless::new(app, [1200.0, 1500.0], 1.0);
    h.step();
    h.step();
    assert!(h.settle(T));
    h
}
fn save(h: &Headless, name: &str, r: Rect) {
    let image = h.paint();
    let scale = h.pixels_per_point;
    let x0 = (r.left() * scale).max(0.0) as usize;
    let y0 = (r.top() * scale).max(0.0) as usize;
    let x1 = ((r.right() * scale).ceil() as usize).min(image.size[0]);
    let y1 = ((r.bottom() * scale).ceil() as usize).min(image.size[1]);
    assert!(x1 > x0 && y1 > y0, "{name}: {r:?}");
    let out = li_raster::Rgba8::from_fn(x1 - x0, y1 - y0, |x, y| image.pixels[(y + y0) * image.size[0] + x + x0].to_array());
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&out), &Default::default()).unwrap();
    let phase = std::env::var("DESIGN_SYSTEM_PHASE").unwrap_or_else(|_| "after".into());
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/design-system");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{phase}-develop-{name}.png")), bytes).unwrap();
}
fn panel_top(h: &Headless, height: f32) -> Rect {
    let panel = rect(h, "panel:right_panel");
    Rect::from_min_size(panel.min, vec2(panel.width(), height.min(panel.height())))
}

fn focus(h: &mut Headless, name: &str) {
    for _ in 0..256 {
        let wanted = rect(h, name);
        let focused = h.view.ctx.memory(|memory| memory.focused());
        if focused.and_then(|id| h.view.ctx.read_response(id)).is_some_and(|response| response.rect == wanted) {
            return;
        }
        request(h, "ui.key", json!({"key":"tab"}));
    }
    panic!("keyboard could not focus {name}");
}

#[test]
#[ignore = "design-system screenshots: run explicitly with --ignored"]
fn design_system_develop_screens() {
    for dark in [true, false] {
        for width in [250.0, 400.0] {
            let tag = format!("{}-{}", if dark { "dark" } else { "light" }, width as u32);
            // Layer bar: Background row, a filled and a radial layer, the selected layer's controls.
            let mut h = setup(lightcraft_engine::Session::with_demo(), width, dark);
            click(&mut h, "button:layerNew");
            click(&mut h, "button:maskNew:full");
            click(&mut h, "button:layerNew");
            click(&mut h, "button:maskNew:colorRange");
            request(&mut h, "ui.key", json!({"key":"escape"}));
            click(&mut h, "mask:1");
            assert!(h.settle(T));
            save(&h, &format!("layers-{tag}"), panel_top(&h, 330.0));
            // The canvas layer pill.
            let canvas = h.app.canvas_rect.expect("canvas");
            save(&h, &format!("pill-{tag}"), Rect::from_min_size(canvas.min, vec2(260.0, 60.0)));
            // Whole-image notices in a layer's Color and Effects sections, and the group line.
            h.app.ui.open_sections = vec!["layer-color".into(), "layer-effects".into()];
            h.step();
            if h.app.widgets.iter().any(|(id, _)| id == "layerGlobalGroup") {
                click(&mut h, "layerGlobalGroup");
            }
            h.step();
            let bw = rect(&h, "layerBackground:bw");
            save(
                &h,
                &format!("global-only-{tag}"),
                Rect::from_min_max(
                    pos2(rect(&h, "panel:right_panel").left(), bw.top() - 40.0),
                    pos2(rect(&h, "panel:right_panel").right(), bw.bottom() + 40.0),
                ),
            );
            let group = rect(&h, "layerGlobalGroup");
            save(
                &h,
                &format!("global-group-{tag}"),
                Rect::from_min_max(
                    pos2(rect(&h, "panel:right_panel").left(), group.top() - 10.0),
                    pos2(rect(&h, "panel:right_panel").right(), group.bottom() + 140.0),
                ),
            );

            // Skin Tone (Background): the picker, swatch with readout, range strips.
            let mut h = setup(lightcraft_engine::Session::with_demo(), width, dark);
            h.app.ui.open_sections = vec!["color".into()];
            h.app.ui.open_flyouts.clear();
            h.step();
            h.step();
            click(&mut h, "flyout:skinTone");
            click(&mut h, "icon:skinTone");
            request(&mut h, "ui.pointer", json!({"events":[{"kind":"down","x":0.45,"y":0.4},{"kind":"up","x":0.45,"y":0.4}]}));
            assert!(h.settle(T));
            let header = rect(&h, "flyout:skinTone");
            let end = rect(&h, "skinProtectLips");
            save(
                &h,
                &format!("skin-{tag}"),
                Rect::from_min_max(
                    pos2(rect(&h, "panel:right_panel").left(), header.top()),
                    pos2(rect(&h, "panel:right_panel").right(), end.bottom() + 6.0),
                ),
            );
            // The picker's canvas badge under the pointer.
            click(&mut h, "icon:skinTone");
            let canvas = h.app.canvas_rect.expect("canvas");
            let at = canvas.center();
            h.app.synthetic.push(egui::Event::PointerMoved(at));
            h.step();
            h.step();
            save(&h, &format!("picker-{tag}"), Rect::from_center_size(at + vec2(14.0, 14.0), vec2(64.0, 64.0)));

            for scale in [1.0, 1.5] {
                let tag = format!("{tag}-{}ppp", scale);
                let mut h = setup(lightcraft_engine::Session::with_demo(), width, dark);
                h.pixels_per_point = scale;
                let mut palette = Tokens::get(&h.view.ctx);
                palette.panel_w = width;
                h.app.set_host_theme(palette);
                h.app.restyle(&h.view.ctx);
                request(&mut h, "engine.execute", json!({"command":"panel.presets"}));
                focus(&mut h, "preset:lc.warm-glow");
                request(&mut h, "ui.key", json!({"key":"space"}));
                h.step();
                focus(&mut h, "value:presetAmount");
                request(&mut h, "ui.key", json!({"key":"space"}));
                request(&mut h, "ui.key", json!({"key":"a","cmd":true}));
                request(&mut h, "ui.text", json!({"text":"75"}));
                let commits = h.app.session.commits();
                request(&mut h, "ui.key", json!({"key":"enter"}));
                assert_eq!(h.app.session.commits(), commits + 1, "Amount keyboard replacement");
                assert!(h.settle(T));
                let right = rect(&h, "panel:right_panel").left();
                save(&h, &format!("presets-{tag}"), Rect::from_min_max(pos2(right - width, 0.0), pos2(right, 900.0)));

                request(&mut h, "engine.execute", json!({"command":"panel.presets"}));
                request(&mut h, "engine.execute", json!({"command":"photo.setReference"}));
                request(&mut h, "ui.key", json!({"key":"r","shift":true}));
                assert!(h.settle(T));
                save(&h, &format!("reference-{tag}"), h.app.canvas_rect.unwrap());
                let pinned = h.app.ui.reference.unwrap();
                request(&mut h, "engine.execute", json!({"command":"photo.delete","params":{"ids":[pinned]}}));
                assert!(h.settle(T));
                save(&h, &format!("reference-missing-{tag}"), h.app.canvas_rect.unwrap());
                request(&mut h, "ui.key", json!({"key":"escape"}));
                assert_eq!(h.app.ui.view, crate::state::ViewMode::Detail);

                request(&mut h, "ui.key", json!({"key":"r"}));
                request(&mut h, "engine.execute", json!({"command":"crop.straighten","params":{"angle":6}}));
                request(&mut h, "engine.execute", json!({"command":"crop.aspect","params":{"aspect":"current"}}));
                assert!(h.settle(T));
                save(&h, &format!("crop-{tag}"), Rect::from_min_max(pos2(0.0, 0.0), pos2(h.size[0], h.size[1])));

                request(&mut h, "ui.key", json!({"key":"escape"}));
                request(&mut h, "engine.execute", json!({"command":"view.photoGrid"}));
                let active = h.app.session.active().unwrap();
                request(&mut h, "engine.execute", json!({"command":"photo.delete","params":{"ids":[active.0]}}));
                request(&mut h, "engine.execute", json!({"command":"library.source","params":{"kind":"recentlyDeleted"}}));
                assert!(h.settle(T));
                let thumbnail = rect(&h, &format!("thumb:{}", active.0)).center();
                request(&mut h, "ui.click", json!({"x":thumbnail.x,"y":thumbnail.y,"button":"right"}));
                h.step();
                save(&h, &format!("deleted-reference-menu-{tag}"), Rect::from_min_max(pos2(0.0, 0.0), pos2(h.size[0], h.size[1])));

                let (session, directory) = super::tests_develop_a::flat_session();
                let mut flat = setup(session, width, dark);
                flat.pixels_per_point = scale;
                request(&mut flat, "ui.key", json!({"key":"r"}));
                focus(&mut flat, "button:straightenAuto");
                request(&mut flat, "ui.key", json!({"key":"space"}));
                assert!(flat.settle(T));
                assert!(flat.app.ui.toast.as_ref().is_some_and(|toast| toast.0 == "No horizon or vertical lines to level by"));
                save(&flat, &format!("straighten-empty-{tag}"), Rect::from_min_size(pos2(0.0, 0.0), vec2(flat.size[0], flat.size[1])));
                std::fs::remove_dir_all(directory).unwrap();
            }
            for scale in [1.0, 1.5] {
                let tag = format!("{tag}-{scale}ppp");
                let mut h = setup(lightcraft_engine::Session::with_demo(), width, dark);
                h.pixels_per_point = scale;
                request(&mut h, "ui.key", json!({"key":"g", "shift":true}));
                request(&mut h, "engine.execute", json!({"command":"geometry.guides", "params":{"guides":[[0.2,0.3,0.8,0.3],[0.2,0.7,0.8,0.7]]}}));
                assert!(h.settle(T));
                let guide = rect(&h, "uprightGuide:0").center();
                request(&mut h, "ui.click", json!({"x":guide.x,"y":guide.y}));
                let full = Rect::from_min_size(pos2(0.0, 0.0), h.size);
                save(&h, &format!("guides-selected-{tag}"), full);

                let photo = h.app.session.active().unwrap();
                let target = egui::Id::new(("uprightGuide", photo.0, 1_usize));
                for _ in 0..256 {
                    if h.view.ctx.memory(|memory| memory.focused()) == Some(target) {
                        break;
                    }
                    request(&mut h, "ui.key", json!({"key":"tab"}));
                }
                assert_eq!(h.view.ctx.memory(|memory| memory.focused()), Some(target), "guide keyboard focus");
                request(&mut h, "ui.key", json!({"key":"space"}));
                assert_eq!(h.app.ui.selected_guide, Some((photo.0, 1)));
                save(&h, &format!("guides-keyboard-{tag}"), full);
                request(&mut h, "ui.key", json!({"key":"Delete"}));
                assert_eq!(h.app.session.develop_of(photo).unwrap().geometry.guides.len(), 1);
                request(&mut h, "ui.key", json!({"key":"z","cmd":true}));
                assert_eq!(h.app.session.develop_of(photo).unwrap().geometry.guides.len(), 2);
                request(&mut h, "ui.click", json!({"x":guide.x,"y":guide.y}));
                request(
                    &mut h,
                    "ui.pointer",
                    json!({"events":[{"kind":"down","x":0.2,"y":0.3},{"kind":"drag","x":0.25,"y":0.4},{"kind":"up","x":0.25,"y":0.4}]}),
                );
                assert!(h.settle(T));
                assert_eq!(h.app.ui.selected_guide, Some((photo.0, 0)));
                save(&h, &format!("guides-endpoint-{tag}"), full);

                let mut pan = setup(lightcraft_engine::Session::with_demo(), width, dark);
                pan.pixels_per_point = scale;
                request(&mut pan, "engine.execute", json!({"command":"panel.remove"}));
                request(&mut pan, "engine.execute", json!({"command":"view.zoom100"}));
                assert!(pan.settle(T));
                let at = pan.app.canvas_rect.unwrap().center();
                pan.events.push(egui::Event::PointerMoved(at));
                pan.events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Middle,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
                pan.step();
                pan.events.push(egui::Event::PointerMoved(at - vec2(80.0, 40.0)));
                pan.step();
                assert_eq!(pan.last_cursor, egui::CursorIcon::Grabbing);
                pan.step();
                save(&pan, &format!("middle-pan-{tag}"), Rect::from_min_size(pos2(0.0, 0.0), pan.size));
                pan.events.push(egui::Event::PointerButton {
                    pos: at - vec2(80.0, 40.0),
                    button: egui::PointerButton::Middle,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
                pan.step();
            }
            for scale in [1.0, 1.5] {
                let tag = format!("{}-{width}-{scale}ppp", if dark { "dark" } else { "light" });
                let mut h = setup(lightcraft_engine::Session::with_demo(), width, dark);
                h.size.y = 1900.0;
                h.pixels_per_point = scale;
                h.app.ui.histogram = true;
                h.app.ui.open_sections = vec!["light".into()];
                h.app.ui.open_flyouts = vec!["curve".into()];
                h.app.ui.curve_channel = "parametric".into();
                request(
                    &mut h,
                    "engine.execute",
                    json!({"command":"develop.curve", "params":{"channel":"master", "points":[[0.0,0.0],[0.25,0.15],[0.75,0.85],[1.0,1.0]]}}),
                );
                assert!(h.settle(T));
                let graph = rect(&h, "curve");
                let photo = h.app.session.active().unwrap();
                let settings = h.app.session.develop_of(photo).unwrap();
                let level = lightcraft_pipeline::finish::curve_luts(&settings.curve).unwrap().tables[0].eval(0.625);
                let at = pos2(graph.left() + graph.width() * 0.625, graph.bottom() - graph.height() * level);
                h.events.push(egui::Event::PointerMoved(at));
                h.step();
                assert_eq!(h.last_cursor, egui::CursorIcon::Grab);
                assert!(
                    panel_top(&h, 1850.0).contains_rect(rect(&h, "slider:curve.refineSaturation")),
                    "capture includes the curve and all its sliders"
                );
                save(&h, &format!("panel-curve-hover-{tag}"), panel_top(&h, 1850.0));
                h.events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
                h.step();
                let dragged = at - vec2(0.0, 10.0);
                h.events.push(egui::Event::PointerMoved(dragged));
                h.step();
                h.step();
                assert_eq!(h.last_cursor, egui::CursorIcon::Grabbing);
                save(&h, &format!("panel-curve-drag-{tag}"), panel_top(&h, 1850.0));
                h.events.push(egui::Event::PointerButton {
                    pos: dragged,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
                h.step();
                let plot = li_ui_kit::histogram::library_plot_rect(rect(&h, "histogram"));
                let at = plot.center();
                h.events.push(egui::Event::PointerMoved(at));
                h.step();
                save(&h, &format!("panel-histogram-hover-{tag}"), panel_top(&h, 1850.0));
                h.events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
                h.step();
                let dragged = at + vec2(plot.width() * 0.05, 0.0);
                h.events.push(egui::Event::PointerMoved(dragged));
                h.step();
                h.step();
                save(&h, &format!("panel-histogram-drag-{tag}"), panel_top(&h, 1850.0));
                h.events.push(egui::Event::PointerButton {
                    pos: dragged,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
                h.step();
                for control in ["light.exposure", "curve.lights"] {
                    focus(&mut h, &format!("value:{control}"));
                    request(&mut h, "ui.key", json!({"key":"space"}));
                    request(&mut h, "ui.text", json!({"text":"1"}));
                    request(&mut h, "ui.key", json!({"key":"enter"}));
                    assert_eq!(lightcraft_develop::controls::get(&h.app.session.develop_of(photo).unwrap(), control), Some(1.0));
                }
                save(&h, &format!("panel-keyboard-{tag}"), panel_top(&h, 1850.0));
            }
        }
    }
}
