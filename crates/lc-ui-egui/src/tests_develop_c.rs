//! C1–C3/A1 acceptance through the existing Library/Develop driver and real pointer input.
use crate::{LightcraftApp, Services, headless::Headless, state::RightPanel, theme::Tokens};
use egui::{Color32, Rect, pos2};
use serde_json::{Value, json};
use std::time::Duration;
const T: Duration = Duration::from_secs(30);
fn exec(h: &mut Headless, c: &str, p: Value) -> Value {
    let r = h.request("engine.execute", json!({"command":c,"params":p}), T);
    assert_eq!(r["ok"], true, "{c}: {r}");
    r["result"].clone()
}
fn click(h: &mut Headless, id: &str) {
    let r = h.request("ui.clickWidget", json!({"id":id}), T);
    assert_eq!(r["ok"], true, "{id}: {r}");
    h.step();
}
fn rect(h: &Headless, id: &str) -> Rect {
    h.app.widgets.iter().rev().find(|(name, _)| name == id).unwrap_or_else(|| panic!("missing {id}")).1
}
fn develop(h: &Headless) -> lightcraft_develop::DevelopSettings {
    (*h.app.session.develop_of(h.app.session.active().unwrap()).unwrap()).clone()
}
pub(super) fn light() -> Tokens {
    let g = Color32::from_gray;
    Tokens {
        dark: false,
        chrome: g(246),
        canvas: g(225),
        grid_bg: g(225),
        inset: g(238),
        field: g(252),
        field_border: g(180),
        button: g(240),
        button_border: g(135),
        hover: g(230),
        pressed: g(215),
        text: g(20),
        text_label: g(20),
        text_dim: g(75),
        icon: g(40),
        tool_active: g(205),
        ..Default::default()
    }
}
fn setup(width: f32, ppp: f32, dark: bool, layer: bool, tool: &str) -> Headless {
    let mut session = lightcraft_engine::Session::with_demo();
    if let Ok(path) = std::env::var("LC_DEVELOP_C_REFERENCE") {
        session = lightcraft_engine::Session::new().with_fs();
        session.execute("library.import", &json!({"paths":[path]})).unwrap();
        session.execute("develop.reset", &json!({})).unwrap();
    }
    let mut app = LightcraftApp::new(session, Services { png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    app.ui.right_width = width;
    app.ui.filmstrip = false;
    app.ui.left_panel = false;
    app.set_host_theme(if dark { Tokens::default() } else { light() });
    let mut h = Headless::new(app, [1000.0, 1800.0], ppp);
    let r = h.request("ui.set", json!({"view":"detail"}), T);
    assert_eq!(r["ok"], true, "{r}");
    if layer {
        h.app.ui.right = RightPanel::Edit;
        h.step();
        click(&mut h, "button:layerNew");
        click(&mut h, "button:maskNew:full");
        exec(&mut h, "mask.rename", json!({"id":1,"name":"Face"}));
        exec(&mut h, "mask.setTools", json!({"id":1,"tools":{if tool=="curve" {"curve"} else {"skin_tone"}: {}}}));
        h.app.ui.layer_used_only = true;
    } else {
        h.app.ui.right = RightPanel::Edit;
    }
    h.app.ui.open_sections = vec![format!("{}{}", if layer { "layer-" } else { "" }, if tool == "curve" { "light" } else { "color" })];
    if layer {
        h.app.ui.open_sections.push("layer-local-collapsed".into());
    }
    h.app.ui.open_flyouts.clear();
    h.app.ui.curve_channel = "master".into();
    h.step();
    h.step();
    click(&mut h, &format!("flyout:{}{tool}", if layer { "layer-" } else { "" }));
    assert!(h.settle(T));
    h
}
fn save(h: &Headless, name: &str, r: Rect) {
    let image = h.paint();
    let scale = h.pixels_per_point;
    let x0 = (r.left() * scale).max(0.0) as usize;
    let y0 = (r.top() * scale).max(0.0) as usize;
    let x1 = (r.right() * scale).ceil().min(image.size[0] as f32) as usize;
    let y1 = (r.bottom() * scale).ceil().min(image.size[1] as f32) as usize;
    assert!(x1 > x0 && y1 > y0);
    let out = lightcraft_raster::Rgba8::from_fn(x1 - x0, y1 - y0, |x, y| image.pixels[(y + y0) * image.size[0] + x + x0].to_array());
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&out), &Default::default()).unwrap();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/develop-c-ux");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.png")), bytes).unwrap();
}
pub(super) fn texts(h: &Headless) -> String {
    fn visit(s: &egui::Shape, out: &mut String) {
        match s {
            egui::Shape::Text(t) => {
                out.push_str(&t.galley.job.text);
                out.push('\n');
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    visit(s, out)
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    for s in h.view.shapes() {
        visit(&s.shape, &mut out);
    }
    out
}
fn luminance(c: Color32) -> f32 {
    let f = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}
fn contrast(a: Color32, b: Color32) -> f32 {
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
#[test]
fn develop_c_curve_rasters_background_and_layer_identity_edited_and_parametric() {
    for dark in [true, false] {
        for width in [250.0, 400.0] {
            for layer in [false, true] {
                let mut h = setup(width, 1.0, dark, layer, "curve");
                let target = if layer { "layer-" } else { "" };
                let header = rect(&h, &format!("flyout:{target}curve"));
                let plot = rect(&h, "curve");
                assert!(plot.width() <= width - 12.0, "plot fits {width}: {plot:?}, actual panel {}", h.app.ui.right_width);
                assert!(plot.bottom() < h.size.y);
                if layer {
                    assert!(texts(&h).contains("Curve - Face"));
                }
                let image = h.paint();
                let t = Tokens::get(&h.view.ctx);
                let mid = plot.center();
                let pixel = (-2..=2)
                    .flat_map(|dy| (-2..=2).map(move |dx| (dx, dy)))
                    .map(|(dx, dy)| image[((mid.x.round() as i32 + dx) as usize, (mid.y.round() as i32 + dy) as usize)])
                    .max_by(|a, b| contrast(*a, t.grid_bg).total_cmp(&contrast(*b, t.grid_bg)))
                    .unwrap();
                assert!(contrast(pixel, t.grid_bg) >= 4.5, "raster contrast {pixel:?} / {:?}", t.grid_bg);
                let panel = Rect::from_min_max(pos2(header.left() - 16.0, header.top()), pos2(header.right() + 16.0, plot.bottom() + 50.0));
                let name = format!("curve-{}-{width}-{}", if dark { "dark" } else { "light" }, if layer { "face" } else { "background" });
                save(&h, &format!("{name}-identity"), panel);
                let r =
                    h.request("ui.dragWidget", json!({"id":"curve","fx":0.5,"fy":0.5,"dx":-0.1*plot.width(),"dy":-0.2*plot.height(),"steps":8}), T);
                assert_eq!(r["ok"], true, "{r}");
                h.step();
                let d = develop(&h);
                let curve = if layer { d.masks[0].tools.curve.as_ref().unwrap() } else { &d.curve };
                assert_eq!(curve.master.len(), 3);
                assert!(curve.master[1].y > 0.65);
                if layer {
                    assert!(d.curve.master.is_empty());
                }
                save(&h, &format!("{name}-edited"), panel);
                click(&mut h, "curveChannel:parametric");
                let split = rect(&h, "curveSplit:1");
                let r = h.request("ui.dragWidget", json!({"id":"curveSplit:1","dx":18.0,"dy":0.0,"steps":6}), T);
                assert_eq!(r["ok"], true, "{r}");
                h.step();
                let d = develop(&h);
                let curve = if layer { d.masks[0].tools.curve.as_ref().unwrap() } else { &d.curve };
                assert!(curve.split_mid > 50.0, "{split:?}");
                save(&h, &format!("{name}-parametric"), panel);
            }
        }
    }
    // HiDPI narrow-panel coverage uses the same actual input/raster path.
    for dark in [true, false] {
        let h = setup(250.0, 1.5, dark, false, "curve");
        let r = rect(&h, "curve");
        save(&h, &format!("curve-{}-250-hidpi", if dark { "dark" } else { "light" }), r.expand(12.0));
    }
}
#[test]
fn develop_c_skin_pick_strips_overlay_clear_global_and_layer() {
    for dark in [true, false] {
        for width in [250.0, 400.0] {
            for layer in [false, true] {
                let mut h = setup(width, 1.0, dark, layer, "skinTone");
                let target = if layer { "layer-" } else { "" };
                assert!(texts(&h).contains("Pick a skin tone on the photo to enable"), "layer={layer} width={width} text: {}", texts(&h));
                let before = develop(&h);
                click(&mut h, "icon:skinTone");
                assert_eq!(h.app.ui.tool, "skinTone");
                let r = h.request("ui.pointer", json!({"events":[{"kind":"down","x":0.45,"y":0.4},{"kind":"up","x":0.45,"y":0.4}]}), T);
                assert_eq!(r["ok"], true, "{r}");
                h.step();
                let d = develop(&h);
                let skin = if layer { d.masks[0].tools.skin_tone.unwrap() } else { d.skin_tone };
                assert!(skin.reference.is_some());
                if layer {
                    assert_eq!(d.skin_tone, before.skin_tone);
                }
                let swatch = rect(&h, &format!("{target}skinToneSwatch"));
                let pixels = h.paint();
                assert_ne!(pixels[(swatch.center().x as usize, swatch.center().y as usize)], Tokens::get(&h.view.ctx).chrome);
                assert!(texts(&h).contains("J ") && texts(&h).contains(" · C "));
                for (id, label) in [("skinTone.hueRange", "Hue"), ("skinTone.chromaRange", "Saturation"), ("skinTone.lightnessRange", "Lightness")] {
                    let range = rect(&h, &format!("{target}skinToneRange:{label}"));
                    let before = h.paint();
                    let r = h.request("ui.dragWidget", json!({"id":format!("slider:{id}"),"dx":20.0,"dy":0.0,"steps":6}), T);
                    assert_eq!(r["ok"], true, "{r}");
                    h.step();
                    let after = h.paint();
                    let changed = (range.left() as usize..range.right() as usize)
                        .any(|x| before[(x, range.top() as usize + 1)] != after[(x, range.top() as usize + 1)]);
                    assert!(changed, "{label} selection marker updates");
                }
                click(&mut h, &format!("{target}skinToneViewRange"));

                assert_eq!(
                    crate::panels::detail::view_overlay(&h.app, &develop(&h)),
                    lightcraft_pipeline::Overlay::SkinToneRange(if layer { Some(1) } else { None })
                );
                if layer {
                    let selected = h.app.session.active_mask;
                    h.app.session.active_mask = None;
                    assert_eq!(
                        crate::panels::detail::view_overlay(&h.app, &develop(&h)),
                        lightcraft_pipeline::Overlay::None,
                        "a stale layer selection cannot keep its range diagnostic active"
                    );
                    h.app.session.active_mask = selected;
                }
                let header = rect(&h, &format!("flyout:{target}skinTone"));
                let end = rect(&h, &format!("{target}skinProtectLips"));
                save(
                    &h,
                    &format!("skin-{}-{width}-{}", if dark { "dark" } else { "light" }, if layer { "face" } else { "background" }),
                    Rect::from_min_max(pos2(header.left() - 16.0, header.top()), pos2(header.right() + 16.0, end.bottom() + 10.0)),
                );
                click(&mut h, &format!("button:{target}skinToneClear"));
                let d = develop(&h);
                assert!(if layer { d.masks[0].tools.skin_tone.unwrap().reference.is_none() } else { d.skin_tone.reference.is_none() });
                assert!(!h.app.ui.skin_tone_visualize);
            }
        }
    }
}
#[test]
fn develop_c_filled_empty_real_creation_and_opacity_exposure_drag() {
    let mut h = setup(400.0, 1.0, true, true, "curve");
    h.app.ui.open_flyouts.clear();
    h.app.ui.layer_used_only = false;
    h.step();
    let r = h.request("ui.dragWidget", json!({"id":"slider:light.exposure","dx":18.0,"steps":8}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.step();
    assert_ne!(develop(&h).masks[0].tools.light.unwrap().exposure, 0.0);
    assert_eq!(develop(&h).light.exposure, 0.0);
    let r = h.request("ui.dragWidget", json!({"id":"slider:opacity","dx":-40.0,"steps":6}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.step();
    assert!(develop(&h).masks[0].opacity < 100.0);
    click(&mut h, "button:layerNew");
    click(&mut h, "button:maskNew:empty");
    let d = develop(&h);
    assert_eq!(d.masks[1].name, "Empty Layer 1");
    assert!(d.masks[1].components.is_empty());
    assert_eq!(h.app.session.active_mask, Some(2));
}

#[test]
fn develop_c_picker_assets_and_cursor_glyphs_are_distinct() {
    use crate::icons::Icon;
    for (tool, glyph) in [("wbPicker", Icon::WhiteBalance), ("skinTone", Icon::SkinTone), ("defringe", Icon::Defringe)] {
        assert_eq!(crate::panels::detail::picker_glyph(tool), Some(glyph));
    }
    let rasters: Vec<_> =
        ["wb-picker", "skin-picker", "defringe-picker"].into_iter().map(|name| li_ui_kit::color_icons::raster(name, 40, false).unwrap()).collect();
    assert_ne!(rasters[0].pixels, rasters[1].pixels);
    assert_ne!(rasters[1].pixels, rasters[2].pixels);
    assert_ne!(rasters[0].pixels, rasters[2].pixels);
}
