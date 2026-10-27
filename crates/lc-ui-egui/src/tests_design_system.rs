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
    app.set_host_theme(crate::theme::host_palette(if dark { li_ui_kit::theme::ThemeKind::Pro } else { li_ui_kit::theme::ThemeKind::StudioLight }));
    let mut h = Headless::new(app, [1200.0, 1500.0], 1.0);
    h.app.restyle(&h.view.ctx);
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
            for scale in [1.0, 1.5] {
                use lightcraft_catalog::{Op, Photo, PhotoId, Source};
                use std::sync::{
                    Arc,
                    atomic::{AtomicBool, Ordering},
                };
                let tag = format!("{tag}-{scale}ppp");
                let hold = Arc::new(AtomicBool::new(false));
                let held = hold.clone();
                let mut session = lightcraft_engine::Session::new();
                session.media.file_loader = Some(Arc::new(move |path: &str, _| {
                    while path != "p1.tif" && held.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Ok((li_raster::RgbFrame::filled(96, 64, [0.2, 0.3, 0.4]), Default::default()))
                }));
                for id in [PhotoId(1), PhotoId(2), PhotoId(3)] {
                    let name = format!("p{}.tif", id.0);
                    let photo = Photo::new(id, Source::File { path: name.clone() }, &name, "TIFF", 96, 64, "2026-10-24");
                    session.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
                }
                session.execute("library.select", &json!({"ids":[1,2,3], "active":1})).unwrap();
                let mut upright = setup(session, width, dark);
                upright.pixels_per_point = scale;
                hold.store(true, Ordering::Relaxed);
                upright.app.session.execute("geometry.upright", &json!({"mode":"vertical"})).unwrap();
                upright.app.session.execute("develop.sync", &json!({"groups":["geometry"]})).unwrap();
                for _ in 0..12 {
                    upright.step();
                }
                let full = Rect::from_min_size(pos2(0.0, 0.0), upright.size);
                save(&upright, &format!("upright-start-toast-{tag}"), full);
                for (name, panel) in [
                    ("edit", RightPanel::Edit),
                    ("profiles", RightPanel::Profiles),
                    ("crop", RightPanel::Crop),
                    ("remove", RightPanel::Remove),
                    ("masking", RightPanel::Masking),
                    ("red-eye", RightPanel::RedEye),
                    ("info", RightPanel::Info),
                    ("keywords", RightPanel::Keywords),
                    ("versions", RightPanel::Versions),
                    ("activity", RightPanel::Activity),
                ] {
                    upright.app.ui.right = panel;
                    upright.events.push(egui::Event::PointerMoved(pos2(0.0, 0.0)));
                    upright.step();
                    upright.step();
                    save(&upright, &format!("upright-running-{name}-{tag}"), panel_top(&upright, 350.0));
                    eprintln!(
                        "UPRIGHT_GEOMETRY {name}-{tag} requested={width} actual={} panel={:?} cancel={:?}",
                        rect(&upright, "panel:right_panel").width(),
                        rect(&upright, "panel:right_panel"),
                        rect(&upright, "button:uprightCancel")
                    );
                    let cancel = rect(&upright, "button:uprightCancel").center();
                    upright.events.push(egui::Event::PointerMoved(cancel));
                    upright.step();
                    upright.step();
                    save(&upright, &format!("upright-cancel-hover-{name}-{tag}"), panel_top(&upright, 350.0));
                }
                upright.app.ui.right = RightPanel::Edit;
                upright.events.push(egui::Event::PointerMoved(pos2(0.0, 0.0)));
                upright.step();
                upright.step();
                focus(&mut upright, "button:uprightCancel");
                upright.step();
                eprintln!(
                    "FOCUS_GEOMETRY upright-cancel-{tag} panel={:?} button={:?}",
                    rect(&upright, "panel:right_panel"),
                    rect(&upright, "button:uprightCancel")
                );
                save(&upright, &format!("upright-cancel-focused-{tag}"), panel_top(&upright, 350.0));
                save(&upright, &format!("upright-cancel-focused-full-{tag}"), full);
                hold.store(false, Ordering::Relaxed);
                let stored = upright.step_until(T, |h| {
                    [PhotoId(2), PhotoId(3)].iter().all(|id| h.app.session.develop_of(*id).unwrap().geometry.upright_transform.is_some())
                });
                assert!(stored);
                assert!(upright.app.ui.toast.as_ref().is_some_and(|toast| toast.0 == "Upright complete"));
                upright.step();
                save(&upright, &format!("upright-finish-toast-{tag}"), full);
                eprintln!("IDLE_GEOMETRY edit-{tag} requested={width} actual={}", rect(&upright, "panel:right_panel").width());
                hold.store(true, Ordering::Relaxed);
                let photo = Photo::new(PhotoId(4), Source::File { path: "p4.tif".into() }, "p4.tif", "TIFF", 96, 64, "2026-10-24");
                upright.app.session.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
                upright.app.session.execute("library.select", &json!({"ids":[1,4], "active":1})).unwrap();
                upright.app.session.execute("develop.sync", &json!({"groups":["geometry"]})).unwrap();
                upright.step();
                upright.step();
                let cancelled = upright.app.session.upright.jobs().last().unwrap().clone();
                focus(&mut upright, "button:uprightCancel");
                request(&mut upright, "ui.key", json!({"key":"space"}));
                assert!(cancelled.cancel.load(Ordering::Relaxed), "keyboard Cancel reaches the running analysis");
                hold.store(false, Ordering::Relaxed);
                assert!(upright.step_until(T, |h| !h.app.session.upright.busy()));
                eprintln!("KEYBOARD_CANCEL {tag} cancelled=true drained=true");
                for (name, panel, target) in
                    [("text-button", RightPanel::Crop, "button:straightenAuto"), ("segmented", RightPanel::Versions, "button:versionsTab-auto")]
                {
                    let mut h = setup(lightcraft_engine::Session::with_demo(), width, dark);
                    h.pixels_per_point = scale;
                    h.app.ui.right = panel;
                    h.step();
                    h.step();
                    focus(&mut h, target);
                    h.step();
                    eprintln!("FOCUS_GEOMETRY {name}-{tag} panel={:?} button={:?}", rect(&h, "panel:right_panel"), rect(&h, target));
                    save(&h, &format!("{name}-focused-{tag}"), panel_top(&h, 800.0));
                    h.events.push(egui::Event::PointerMoved(rect(&h, target).center()));
                    h.step();
                    h.step();
                    save(&h, &format!("{name}-hovered-{tag}"), panel_top(&h, 800.0));
                }
            }
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

                let (session, directory) = super::tests_develop_a::flat_session("design_system_develop_screens");
                let mut flat = setup(session, width, dark);
                flat.pixels_per_point = scale;
                request(&mut flat, "ui.key", json!({"key":"r"}));
                focus(&mut flat, "button:straightenAuto");
                request(&mut flat, "ui.key", json!({"key":"space"}));
                assert!(flat.settle(T));
                assert!(flat.app.ui.toast.as_ref().is_some_and(|toast| toast.0 == "No horizon or vertical lines to level by"));
                save(&flat, &format!("straighten-empty-{tag}"), Rect::from_min_size(pos2(0.0, 0.0), vec2(flat.size[0], flat.size[1])));
                std::fs::remove_dir_all(directory).unwrap();

                for (view, command) in [("grid", "view.photoGrid"), ("develop", "view.develop")] {
                    let mut filters = setup(lightcraft_engine::Session::with_demo(), width, dark);
                    filters.pixels_per_point = scale;
                    filters.app.ui.filter_bar = true;
                    filters.app.ui.filmstrip = true;
                    request(&mut filters, "engine.execute", json!({"command":command}));
                    let text = filters.app.session.catalog.photos().find(|photo| photo.rating >= 4).unwrap().file_name.clone();
                    request(&mut filters, "engine.execute", json!({"command":"library.filter","params":{"text":text,"rating":4}}));
                    filters.app.ui.search = text;
                    let expected = filters.app.session.visible_cloned();
                    for state in ["enabled", "disabled", "restored"] {
                        if state != "enabled" {
                            request(&mut filters, "ui.key", json!({"key":"l","cmd":true}));
                        }
                        assert_eq!(filters.app.session.filters_off(), state == "disabled");
                        if state != "disabled" {
                            assert_eq!(filters.app.session.visible_cloned(), expected);
                        }
                        assert!(filters.settle(T));
                        click(&mut filters, "menu:View");
                        save(
                            &filters,
                            &format!("filters-{view}-{state}-{tag}"),
                            Rect::from_min_size(pos2(0.0, 0.0), vec2(filters.size[0], filters.size[1])),
                        );
                        request(&mut filters, "ui.key", json!({"key":"escape"}));
                    }
                }
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

                request(
                    &mut h,
                    "engine.execute",
                    json!({"command":"geometry.guides", "params":{"guides":[[0.25,0.02,0.25,0.98],[0.75,0.02,0.75,0.98]]}}),
                );
                h.app.ui.selected_guide = None;
                assert!(h.settle(T));
                save(&h, &format!("guides-contrast-{tag}"), full);
                request(&mut h, "engine.execute", json!({"command":"geometry.guides", "params":{"guides":[]}}));
                assert!(h.settle(T));
                save(&h, &format!("guides-photo-reference-{tag}"), full);

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
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    if let Some(path) = std::env::var_os("PHOTOCRAFT_THEME_FILE") {
        let proposals: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        for kind in [li_ui_kit::theme::ThemeKind::Pro, li_ui_kit::theme::ThemeKind::StudioLight] {
            for proposed in [false, true] {
                let mut palette = li_ui_kit::library::for_kind(kind);
                if proposed {
                    let overrides = proposals.get(kind.id()).expect("the proposal contains both themes");
                    assert!(overrides.is_object(), "theme overrides must be an object");
                    let unknown = li_ui_kit::theme::live::apply_overrides(&mut palette, overrides);
                    assert!(unknown.is_empty(), "unknown proposal tokens: {unknown:?}");
                    palette.library.text_disabled = palette.text_faint;
                    palette.library.caution = crate::theme::readable_text(palette.warning, palette.chrome);
                    palette.library.reject = crate::theme::readable_text(palette.danger, palette.chrome);
                }
                for width in [250.0, 400.0] {
                    for scale in [1.0, 1.5] {
                        let mut session = lightcraft_engine::Session::with_demo();
                        let id = session.active().unwrap();
                        let photo = session.catalog.photo(id).unwrap();
                        let content = lightcraft_catalog::Op::SetContent {
                            id,
                            width: photo.width,
                            height: photo.height,
                            file_size: photo.file_size,
                            content_hash: photo.content_hash.clone(),
                            preview_only: Some("Synthetic RAW preview fixture".into()),
                        };
                        let source = lightcraft_catalog::Op::Relink {
                            id,
                            file_name: "SYNTHETIC-PREVIEW.RAW".into(),
                            source: photo.source.clone(),
                            format: Some("RAW".into()),
                        };
                        session.catalog.apply(content).unwrap();
                        session.catalog.apply(source).unwrap();
                        let mut app = LightcraftApp::new(session, Services { png: None, ..Default::default() });
                        app.ui.settings.gpu = false;
                        app.ui.right_width = width;
                        app.ui.left_panel = false;
                        app.ui.filmstrip = false;
                        app.ui.histogram = false;
                        app.set_host_theme(palette.into());
                        let mut h = Headless::new(app, [1200.0, 900.0], scale);
                        let phase = if proposed { "proposed" } else { "before" };
                        let tag = format!("{}-{width}-{scale}", kind.id());
                        for (screen, view, panel) in [
                            ("library", crate::state::ViewMode::PhotoGrid, RightPanel::Info),
                            ("develop", crate::state::ViewMode::Detail, RightPanel::Edit),
                        ] {
                            h.app.ui.view = view;
                            h.app.ui.right = panel;
                            h.app.ui.open_sections = vec!["light".into()];
                            h.step();
                            h.step();
                            if screen == "develop" {
                                click(&mut h, "button:layerNew");
                                click(&mut h, "button:maskNew:full");
                                h.app.ui.open_sections = vec!["layer-color".into(), "layer-effects".into()];
                                h.step();
                                h.step();
                            }
                            assert!(h.settle(T));
                            save(&h, &format!("contrast-{phase}-{screen}-{tag}"), Rect::from_min_size(pos2(0.0, 0.0), h.size));
                        }
                    }
                }
            }
        }
    }
}
