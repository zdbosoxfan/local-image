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
fn setup(width: f32, dark: bool) -> Headless {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
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

#[test]
#[ignore = "design-system screenshots: run explicitly with --ignored"]
fn design_system_develop_screens() {
    for dark in [true, false] {
        for width in [250.0, 400.0] {
            let tag = format!("{}-{}", if dark { "dark" } else { "light" }, width as u32);
            // Layer bar: Background row, a filled and a radial layer, the selected layer's controls.
            let mut h = setup(width, dark);
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
            let mut h = setup(width, dark);
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
        }
    }
}
