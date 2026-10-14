//! A2–A6 acceptance uses the application's actual widgets and canvas input.
use crate::{LightcraftApp, Services, headless::Headless, state::RightPanel};
use egui::Rect;
use lightcraft_develop::MaskShape;
use serde_json::{Value, json};
use std::time::Duration;
const T: Duration = Duration::from_secs(40);
fn request(h: &mut Headless, command: &str, params: Value) -> Value {
    let r = h.request(command, params, T);
    assert_eq!(r["ok"], true, "{command}: {r}");
    h.step();
    r["result"].clone()
}
fn exec(h: &mut Headless, command: &str, params: Value) -> Value {
    request(h, "engine.execute", json!({"command":command,"params":params}))
}
fn click(h: &mut Headless, id: &str) {
    request(h, "ui.clickWidget", json!({"id":id}));
}
fn rect(h: &Headless, id: &str) -> Rect {
    h.app.widgets.iter().rev().find(|(name, _)| name == id).unwrap_or_else(|| panic!("missing {id}")).1
}
fn d(h: &Headless) -> lightcraft_develop::DevelopSettings {
    (*h.app.session.develop_of(h.app.session.active().unwrap()).unwrap()).clone()
}
fn setup(width: f32, dark: bool, ppp: f32) -> Headless {
    let mut s = lightcraft_engine::Session::with_demo();
    if let Ok(path) = std::env::var("LC_DEVELOP_A_REFERENCE") {
        s = lightcraft_engine::Session::new().with_fs();
        s.execute("library.import", &json!({"paths":[path]})).unwrap();
    }
    let mut app = LightcraftApp::new(s, Services { png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    app.ui.view = crate::state::ViewMode::Detail;
    app.ui.right = RightPanel::Edit;
    app.ui.histogram = false;
    app.ui.left_panel = false;
    app.ui.filmstrip = false;
    app.ui.right_width = width;
    app.set_host_theme(if dark { Default::default() } else { super::tests_develop_c::light() });
    let mut h = Headless::new(app, [1200.0, 1800.0], ppp);
    h.step();
    h.step();
    assert!(h.settle(T));
    h
}
fn new_layer(h: &mut Headless, kind: &str) {
    click(h, "button:layerNew");
    click(h, &format!("button:maskNew:{kind}"));
}
fn pointer(h: &mut Headless, events: Value) {
    request(h, "ui.pointer", json!({"events":events}));
}
fn drag(h: &mut Headless, id: &str, dx: f32) {
    request(h, "ui.dragWidget", json!({"id":id,"dx":dx,"steps":8}));
}
fn text(h: &Headless) -> String {
    super::tests_develop_c::texts(h)
}
#[test]
fn develop_a_filled_background_and_global_only_links() {
    let mut h = setup(400.0, true, 1.0);
    assert!(d(&h).masks.is_empty());
    assert!(text(&h).contains("Background"));
    new_layer(&mut h, "full");
    assert_eq!(h.app.session.active_mask, Some(1));
    assert!(matches!(d(&h).masks[0].components[0].shape, MaskShape::Full));
    assert!(matches!(crate::panels::detail::view_overlay(&h.app, &d(&h)), lightcraft_pipeline::Overlay::Mask { id: 1, .. }));
    click(&mut h, "section:layer-light");
    let slider = rect(&h, "slider:light.exposure");
    drag(&mut h, "slider:light.exposure", slider.width() * 0.1);
    let edited = d(&h);
    assert_eq!(edited.masks[0].tools.light.as_ref().unwrap().exposure, 1.0);
    assert_eq!(edited.light.exposure, 0.0);
    assert!(h.app.session.undo_entry().unwrap().label.contains("Exposure – Filled Layer 1"));
    let photo = h.app.session.active().unwrap();
    let layer_pixels = h.app.session.render_now(photo, 160, 160).unwrap().image;
    click(&mut h, "section:layer-effects");
    // TEST-CHANGED (UX polish): the three-line notice is one dimmed line per tool now.
    assert!(text(&h).contains("whole image only"));
    click(&mut h, "layerBackground:vignette");
    assert_eq!(h.app.session.active_mask, None);
    assert!(h.app.widgets.iter().any(|(id, _)| id == "slider:light.exposure"));
    click(&mut h, "icon:maskVisible:1");
    let slider = rect(&h, "slider:light.exposure");
    drag(&mut h, "slider:light.exposure", slider.width() * 0.1);
    assert_eq!(d(&h).light.exposure, 1.0);
    let global_pixels = h.app.session.render_now(photo, 160, 160).unwrap().image;
    // TEST-CHANGED: a RAW Background exposure precedes its camera tone map; a layer
    // exposure multiplies the completed image. Rendered sources remain equivalent here.
    let same = layer_pixels.data.as_flattened().iter().zip(global_pixels.data.as_flattened()).all(|(a, b)| a.abs_diff(*b) <= 1);
    if h.app.session.source_info(photo).raw {
        assert!(!same, "RAW Background and developed-image exposure have distinct meanings");
    } else {
        assert!(same);
    }
    click(&mut h, "icon:maskVisible:1");
    click(&mut h, "mask:1");
    click(&mut h, "section:layer-color");
    click(&mut h, "layerBackground:bw");
    assert_eq!(h.app.session.active_mask, None);
}
#[test]
fn develop_a_components_canvas_opacity_history_undo_redo() {
    let mut h = setup(400.0, true, 1.0);
    new_layer(&mut h, "full");
    click(&mut h, "section:layer-light");
    drag(&mut h, "slider:light.exposure", 18.0);
    click(&mut h, "button:maskPlus:1");
    // Direct luminance intersect is intentionally a single visible action.
    click(&mut h, "button:layerIntersectLuma");
    assert_eq!(d(&h).masks[0].components[1].op, lightcraft_develop::MaskOp::Intersect);
    exec(&mut h, "edit.undo", json!({}));
    click(&mut h, "button:maskPlus:1");
    let menu = h.app.widgets.iter().find(|(id, _)| id == "button:layerEditMask").unwrap().1;
    request(&mut h, "ui.move", json!({"x":menu.left()+25.0,"y":menu.top()-45.0}));
    // Submenus also have registered rows so hover/click is reproducible.
    click(&mut h, "layerOp:subtract");
    click(&mut h, "maskComp:subtract:radial");
    pointer(
        &mut h,
        json!([{"kind":"down","x":0.35,"y":0.35},{"kind":"drag","x":0.4,"y":0.4},{"kind":"drag","x":0.65,"y":0.6},{"kind":"up","x":0.65,"y":0.6}]),
    );
    let current = d(&h);
    assert_eq!(current.masks[0].components.len(), 2);
    assert_eq!(current.masks[0].components[1].op, lightcraft_develop::MaskOp::Subtract);
    let MaskShape::Radial { rx, ry, .. } = current.masks[0].components[1].shape else { panic!("radial") };
    assert!(rx > 0.1 && ry > 0.1);
    click(&mut h, "button:layerMenu:1");
    click(&mut h, "button:layerMenu:Invert");
    assert!(d(&h).masks[0].invert);
    drag(&mut h, "layerOpacity:1", -50.0);
    assert_eq!(d(&h).masks[0].opacity, 50.0);
    let opacity = d(&h).masks[0].opacity;
    exec(&mut h, "edit.undo", json!({}));
    assert_eq!(d(&h).masks[0].opacity, 100.0);
    exec(&mut h, "edit.undo", json!({}));
    assert!(!d(&h).masks[0].invert);
    exec(&mut h, "edit.undo", json!({}));
    assert_eq!(d(&h).masks[0].components.len(), 1);
    assert_eq!(h.app.session.active_mask, Some(1));
    exec(&mut h, "edit.undo", json!({}));
    assert!(d(&h).masks[0].tools.light.is_none());
    exec(&mut h, "edit.undo", json!({}));
    assert!(d(&h).masks.is_empty());
    assert_eq!(h.app.session.active_mask, None);
    for _ in 0..5 {
        exec(&mut h, "edit.redo", json!({}));
    }
    assert_eq!(d(&h).masks[0].opacity, opacity);
    assert!(d(&h).masks[0].invert);
    click(&mut h, "mask:1");
    request(&mut h, "ui.clickWidget", json!({"id":"mask:1","count":2}));
    request(&mut h, "ui.key", json!({"key":"a","cmd":true}));
    request(&mut h, "ui.text", json!({"text":"Sky"}));
    request(&mut h, "ui.key", json!({"key":"enter"}));
    assert_eq!(d(&h).masks[0].name, "Sky");
    assert!(text(&h).contains("Sky"));
    rect(&h, "activeLayerPill");
    let before = h.app.session.undo.len();
    click(&mut h, "icon:maskVisible:1");
    assert!(!d(&h).masks[0].visible);
    click(&mut h, "icon:maskVisible:1");
    assert!(d(&h).masks[0].visible);
    assert_eq!(h.app.session.undo.len(), before + 2);
}
#[test]
fn develop_a_empty_brush_overlay_reorder_delete_and_legacy() {
    let mut h = setup(400.0, true, 1.0);
    new_layer(&mut h, "empty");
    assert!(text(&h).contains("(no mask)"));
    assert!(h.app.widgets.iter().any(|(id, _)| id == "button:layerIntersectLuma"));
    request(&mut h, "ui.key", json!({"key":"escape"}));
    request(&mut h, "ui.key", json!({"key":"k"}));
    assert_eq!(h.app.ui.right, RightPanel::Edit);
    pointer(
        &mut h,
        json!([{"kind":"down","x":0.3,"y":0.6},{"kind":"drag","x":0.4,"y":0.6},{"kind":"drag","x":0.5,"y":0.6},{"kind":"up","x":0.5,"y":0.6}]),
    );
    assert_eq!(d(&h).masks.len(), 1);
    assert!(matches!(d(&h).masks[0].components[0].shape, MaskShape::Brush { .. }));
    h.app.ui.mask_overlay_drawing_only = true;
    h.step();
    assert_eq!(crate::panels::detail::view_overlay(&h.app, &d(&h)), lightcraft_pipeline::Overlay::None);
    exec(&mut h, "mask.adjust", json!({"values":{"exposure":0.4}}));
    assert!(text(&h).contains("Lightroom local adjustments"));
    new_layer(&mut h, "full");
    let to = rect(&h, "mask:1").center();
    request(&mut h, "ui.dragWidget", json!({"id":"mask:2","toX":to.x,"toY":to.y,"steps":8}));
    assert_eq!(d(&h).masks[0].id, 2);
    request(&mut h, "ui.key", json!({"key":"delete"}));
    click(&mut h, "button:layerDeleteConfirm");
    assert_eq!(d(&h).masks.len(), 1);
    assert_eq!(h.app.session.active_mask, None);
    click(&mut h, "button:layerBackground");
    request(&mut h, "ui.key", json!({"key":"down"}));
    assert_eq!(h.app.session.active_mask, Some(1));
    request(&mut h, "ui.key", json!({"key":"up"}));
    assert_eq!(h.app.session.active_mask, None);
    exec(&mut h, "mask.select", json!({"id":1}));
    exec(&mut h, "panel.masking", json!({}));
    assert!(!text(&h).contains("Lightroom local adjustments"));
    assert!(!h.app.widgets.iter().any(|(id, _)| id == "slider:light.exposure"));
}
#[test]
fn develop_a_layer_wb_curve_and_subtract_brush() {
    let mut h = setup(400.0, true, 1.0);
    new_layer(&mut h, "full");
    exec(&mut h, "mask.rename", json!({"name":"Face"}));
    click(&mut h, "section:layer-light");
    let global = d(&h).wb;
    click(&mut h, "icon:wbPicker");
    pointer(&mut h, json!([{"kind":"down","x":0.4,"y":0.5},{"kind":"up","x":0.4,"y":0.5}]));
    assert!(h.settle(T));
    assert!(d(&h).masks[0].tools.wb.is_some());
    assert_eq!(d(&h).wb, global);
    assert_eq!(h.app.session.undo_entry().unwrap().label, "White Balance – Face");
    if !h.app.ui.section_open("layer-light") {
        click(&mut h, "section:layer-light");
    }
    click(&mut h, "flyout:layer-curve");
    let global = d(&h).curve;
    click(&mut h, "dropdown:curvePreset");
    click(&mut h, "curvePreset:Medium Contrast");
    assert!(d(&h).masks[0].tools.curve.is_some());
    assert_eq!(d(&h).curve, global);
    click(&mut h, "button:maskPlus:1");
    click(&mut h, "layerOp:subtract");
    click(&mut h, "maskComp:subtract:brush");
    pointer(&mut h, json!([{"kind":"down","x":0.3,"y":0.3},{"kind":"drag","x":0.4,"y":0.3},{"kind":"up","x":0.4,"y":0.3}]));
    let m = &d(&h).masks[0];
    assert_eq!(m.components[1].op, lightcraft_develop::MaskOp::Subtract);
    let MaskShape::Brush { strokes } = &m.components[1].shape else { panic!("brush") };
    assert!(!strokes[0].erase);
}
#[test]
fn develop_a_sync_copy_ui_preserves_other_photos_layers() {
    let mut h = setup(400.0, true, 1.0);
    // Demo has many independent photos; reference run imports extra copies inside target only.
    if std::env::var_os("LC_DEVELOP_A_REFERENCE").is_some() {
        let src = std::env::var("LC_DEVELOP_A_REFERENCE").unwrap();
        let parent = std::path::Path::new(&src).parent().unwrap();
        let mut paths = vec![];
        for i in 2..=4 {
            let dest = parent.join(format!("sync-{i}.dng"));
            std::fs::copy(&src, &dest).unwrap();
            std::io::Write::write_all(&mut std::fs::OpenOptions::new().append(true).open(&dest).unwrap(), &[i as u8]).unwrap();
            paths.push(dest);
        }
        exec(&mut h, "library.import", json!({"paths":paths}));
    }
    let ids: Vec<_> = h.app.session.visible_cloned().into_iter().take(4).collect();
    assert_eq!(ids.len(), 4);
    exec(&mut h, "library.select", json!({"ids":ids.iter().map(|id|id.0).collect::<Vec<_>>(),"active":ids[0].0}));
    new_layer(&mut h, "full");
    exec(&mut h, "mask.rename", json!({"name":"Sky"}));
    exec(&mut h, "develop.autoSync", json!({"on":true}));
    click(&mut h, "section:layer-light");
    let before: Vec<_> = ids[1..].iter().map(|id| h.app.session.develop_of(*id).unwrap().masks.clone()).collect();
    drag(&mut h, "slider:light.contrast", 20.0);
    for (i, id) in ids[1..].iter().enumerate() {
        assert_eq!(h.app.session.develop_of(*id).unwrap().masks, before[i]);
    }
    exec(&mut h, "dialog.syncSettings", json!({}));
    click(&mut h, "pasteGroup:masks");
    click(&mut h, "button:dialogOk");
    for id in &ids[1..] {
        let p = h.app.session.catalog.photo(*id).unwrap();
        assert_eq!(p.develop.masks, d(&h).masks);
        assert_eq!(p.history.last().unwrap().label, "Sync Settings");
    }
    exec(&mut h, "dialog.copySettings", json!({}));
    click(&mut h, "button:dialogOk");
    let target_before = h.app.session.develop_of(ids[1]).unwrap().masks.clone();
    exec(&mut h, "library.select", json!({"ids":[ids[1].0],"active":ids[1].0}));
    exec(&mut h, "develop.paste", json!({}));
    assert_eq!(d(&h).masks, target_before);
}
#[test]
fn develop_a_layer_bar_rasters() {
    for dark in [true, false] {
        for width in [250.0, 400.0] {
            for ppp in [1.0, 1.5] {
                let mut h = setup(width, dark, ppp);
                let theme = if dark { "dark" } else { "light" };
                save(&h, &format!("compact-{theme}-{width}-{ppp}"), None);
                click(&mut h, "button:layerNew");
                save(&h, &format!("plus-{theme}-{width}-{ppp}"), None);
                click(&mut h, "button:maskNew:full");
                save(&h, &format!("expanded-{theme}-{width}-{ppp}"), None);
                click(&mut h, "button:layerMenu:1");
                save(&h, &format!("kebab-{theme}-{width}-{ppp}"), None);
                request(&mut h, "ui.key", json!({"key":"escape"}));
                click(&mut h, "section:layer-effects");
                save(&h, &format!("global-only-{theme}-{width}-{ppp}"), None);
                let bg = rect(&h, "layerBackground:grain");
                let panel = rect(&h, "panel:right_panel");
                assert!(panel.contains_rect(bg), "{panel:?} {bg:?}");
            }
        }
    }
}
fn save(h: &Headless, name: &str, _crop: Option<Rect>) {
    let image = h.paint();
    let scale = h.pixels_per_point;
    let x0 = ((1200.0 - h.app.ui.right_width - 200.0) * scale) as usize;
    let x1 = image.size[0];
    let y1 = (800.0 * scale) as usize;
    let out = lightcraft_raster::Rgba8::from_fn(x1 - x0, y1, |x, y| image.pixels[y * image.size[0] + x + x0].to_array());
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&out), &Default::default()).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/develop-a-ux");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.png")), bytes).unwrap();
}

#[test]
#[ignore = "RTX 5090 interactive layer benchmark, run explicitly with LC_REQUIRE_GPU=1"]
fn develop_a_layer_drag_interactive_gpu_budget() {
    use crate::render::Slot;
    use egui::pos2;
    use lightcraft_develop::{BrushStroke, Effects, LayerTools, Light, Mask, MaskComponent, MaskShape};
    use lightcraft_geom::Point;
    use std::time::Instant;
    let _guard = crate::gpu_test_support::lock();
    let path = std::env::var("LC_DEVELOP_A_RAW").expect("set LC_DEVELOP_A_RAW to a 45 MP RAW");
    let mut s = lightcraft_engine::Session::new().with_fs();
    s.execute("library.import", &json!({"paths":[path]})).unwrap();
    let photo = s.catalog.photo(s.active().unwrap()).unwrap();
    assert!(photo.develops_raw() && photo.width as u64 * photo.height as u64 >= 45_000_000);
    eprintln!("develop-a RAW source: {}x{}", photo.width, photo.height);
    let mut app = LightcraftApp::new(s, Services { png: None, ..Default::default() });
    app.ui.settings.gpu = true;
    app.ui.view = crate::state::ViewMode::Detail;
    app.ui.right = RightPanel::Edit;
    app.ui.left_panel = false;
    app.ui.filmstrip = false;
    app.ui.right_width = 400.0;
    app.ui.layer_used_only = true;
    app.ui.open_sections = vec!["layer-light".into(), "layer-local-collapsed".into()];
    let mut h = Headless::new(app, [1800.0, 1800.0], 1.0);
    h.step();
    h.step();
    assert!(lightcraft_engine::gpu::available(), "real hardware GPU required");
    let adapter = lightcraft_engine::gpu::adapter_name().unwrap();
    assert!(adapter.contains("RTX 5090") && adapter.contains("Vulkan"), "RTX 5090 / Vulkan required: {adapter}");
    eprintln!("develop-a interactive adapter: {adapter}");
    let wait = |h: &mut Headless| {
        assert!(
            h.step_until(T, |h| h.app.renderer.textures.get(&Slot::Main).is_some_and(|tex| Some(tex.key) == h.app.renderer.wanted(Slot::Main))
                && !h.app.renderer.is_pending(Slot::Main)),
            "latest preview failed: {:?}",
            h.app.renderer.failure(Slot::Main)
        )
    };
    for (count, full) in [(1, false), (4, false), (8, false), (8, true)] {
        eprintln!("develop-a CASE {count} {} start", if full { "Full" } else { "Brush" });
        let masks: Vec<_> = (1..=count)
            .map(|i| Mask {
                id: i,
                name: format!("Benchmark {i}"),
                components: vec![MaskComponent {
                    name: None,
                    op: Default::default(),
                    invert: false,
                    shape: if full {
                        MaskShape::Full
                    } else {
                        MaskShape::Brush {
                            strokes: vec![BrushStroke { points: vec![Point::new(0.3, 0.5), Point::new(0.7, 0.5)], size: 0.3, ..Default::default() }],
                        }
                    },
                }],
                tools: LayerTools {
                    light: Some(Light { exposure: 0.1, ..Default::default() }),
                    effects: (!full).then_some(Effects { clarity: 20.0, ..Default::default() }),
                    ..Default::default()
                },
                ..Default::default()
            })
            .collect();
        exec(&mut h, "develop.merge", json!({"settings":{"masks":masks}}));
        exec(&mut h, "mask.select", json!({"id":count.min(4)}));
        h.step();
        wait(&mut h);
        let slider = rect(&h, "slider:light.exposure");
        let start = pos2(slider.left() + slider.width() * 0.5, slider.center().y);
        h.app.synthetic.push(egui::Event::PointerMoved(start));
        h.step();
        h.app.synthetic.push(egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        h.step();
        let mut latency = vec![];
        let mut ui_ms = vec![];
        for step in 1..=6 {
            let q = pos2(slider.left() + slider.width() * (0.5 + step as f32 * 0.025), slider.center().y);
            let time = Instant::now();
            h.app.synthetic.push(egui::Event::PointerMoved(q));
            h.step();
            ui_ms.push(time.elapsed().as_secs_f64() * 1000.0);
            wait(&mut h);
            latency.push(time.elapsed().as_secs_f64() * 1000.0);
            let tex = &h.app.renderer.textures[&Slot::Main];
            assert!(tex.size[0].max(tex.size[1]) <= crate::render::DRAFT_EDGE);
        }
        let q = pos2(slider.left() + slider.width() * 0.65, slider.center().y);
        h.app.synthetic.push(egui::Event::PointerButton {
            pos: q,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        h.step();
        wait(&mut h);
        latency.sort_by(f64::total_cmp);
        ui_ms.sort_by(f64::total_cmp);
        let memory = h.app.renderer.memory();
        let gpu_bytes = memory["stageCaches"]["gpuBytes"].as_u64().unwrap();
        assert!(gpu_bytes > 0, "GPU stage cache required");
        eprintln!(
            "develop-a {} {count} {} layers: UI p95={:.2}ms, change-to-texture p95={:.2}ms, GPU stage cache={} bytes",
            if std::env::var_os("LC_DEVELOP_A_RAW").is_some() { "RAW" } else { "45MP procedural" },
            if full { "Full" } else { "Brush" },
            ui_ms[5],
            latency[5],
            gpu_bytes
        );
        assert!(ui_ms[5] < 16.7, "interactive UI frame budget");
        // After timing, leave the retained allocations visible to the external NVML sampler.
        // This idle interval is excluded from UI and change-to-texture latencies above.
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// UX-polish screenshots of the right panel (layer bar and the layer view's whole-image tools):
/// 250/400 pt, dark/light, 1×/1.5×, into `target/ux-polish/`. `UX_POLISH_PHASE` names the set.
#[test]
fn ux_polish_layer_bar_screens() {
    let phase = std::env::var("UX_POLISH_PHASE").unwrap_or_else(|_| "after".into());
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ux-polish");
    std::fs::create_dir_all(&dir).unwrap();
    let shot = |h: &Headless, name: &str| {
        let image = h.paint();
        let scale = h.pixels_per_point;
        let panel = rect(h, "panel:right_panel");
        let x0 = ((panel.left() * scale) as usize).min(image.size[0] - 1);
        let y1 = ((panel.bottom().min(1500.0) * scale) as usize).min(image.size[1]);
        let w = image.size[0] - x0;
        let out = lightcraft_raster::Rgba8::from_fn(w, y1, |x, y| image.pixels[y * image.size[0] + x + x0].to_array());
        let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&out), &Default::default()).unwrap();
        std::fs::write(dir.join(format!("{phase}-layers-{name}.png")), bytes).unwrap();
    };
    for dark in [true, false] {
        for width in [250.0, 400.0] {
            for ppp in [1.0, 1.5] {
                let theme = if dark { "dark" } else { "light" };
                let tag = format!("{theme}-{width}-{ppp}x");
                let mut h = setup(width, dark, ppp);
                shot(&h, &format!("none-{tag}"));
                new_layer(&mut h, "full");
                new_layer(&mut h, "radial");
                request(&mut h, "ui.key", json!({"key":"escape"}));
                click(&mut h, "mask:1");
                shot(&h, &format!("two-{tag}"));
                click(&mut h, "section:layer-color");
                click(&mut h, "section:layer-effects");
                shot(&h, &format!("sections-{tag}"));
                if h.app.widgets.iter().any(|(id, _)| id == "layerGlobalGroup") {
                    click(&mut h, "layerGlobalGroup");
                    shot(&h, &format!("group-open-{tag}"));
                }
            }
        }
    }
}

/// UX polish: no layers → one line (Background … +); with layers, Background is the first row,
/// every layer row has a mask thumbnail; the five whole-image sections are ONE collapsed line
/// whose link and expanded items select Background.
#[test]
fn ux_layer_bar_one_line_thumbnails_and_whole_image_group() {
    let mut h = setup(250.0, true, 1.0);
    let bar = rect(&h, "layerBar");
    let bg = rect(&h, "button:layerBackground");
    let plus = rect(&h, "button:layerNew");
    assert!((bg.center().y - plus.center().y).abs() < 2.0, "Background and + share one line");
    assert!(bar.height() < 50.0, "one line without layers: {bar:?}");
    assert!(!h.app.widgets.iter().any(|(id, _)| id == "button:layerCollapse"));
    new_layer(&mut h, "full");
    new_layer(&mut h, "empty");
    request(&mut h, "ui.key", json!({"key":"escape"}));
    // Background is the first row, then the layers in order, each with its thumbnail.
    let bg = rect(&h, "button:layerBackground");
    let (m1, m2) = (rect(&h, "mask:1"), rect(&h, "mask:2"));
    assert!(bg.bottom() <= m1.top() + 1.0 && m1.bottom() <= m2.top() + 1.0);
    for id in [1, 2] {
        let thumb = rect(&h, &format!("layerThumb:{id}"));
        let row = rect(&h, &format!("mask:{id}"));
        assert!(thumb.right() <= row.left() && (thumb.center().y - row.center().y).abs() < 3.0, "thumbnail sits before the name");
    }
    assert!(rect(&h, "button:layerMenu:1").right() <= rect(&h, "panel:right_panel").right());
    // the thumbnails are painted from the mask data: Filled is white, Empty is black
    let image = h.paint();
    let px = |id: &str| {
        let c = rect(&h, id).center();
        image.pixels[c.y as usize * image.size[0] + c.x as usize]
    };
    assert!(px("layerThumb:1").r() > 200, "filled layer thumbnail is white: {:?}", px("layerThumb:1"));
    assert!(px("layerThumb:2").r() < 40, "empty layer thumbnail is black: {:?}", px("layerThumb:2"));
    // the one header "+" (no per-row "+" any more; mask components live under the list)
    assert_eq!(h.app.widgets.iter().filter(|(id, _)| id == "button:layerNew").count(), 1);
    assert!(h.app.widgets.iter().any(|(id, _)| id == "button:maskPlus:2"));
    click(&mut h, "mask:1");
    assert!(text(&h).contains("5 whole-image tools"));
    assert!(!text(&h).contains("This tool applies to the whole image."));
    for key in ["profile", "negative", "toneEq", "optics", "calibration"] {
        assert!(!h.app.widgets.iter().any(|(id, _)| id == &format!("layerBackground:{key}")), "collapsed: {key}");
    }
    click(&mut h, "layerBackground:global");
    assert_eq!(h.app.session.active_mask, None);
    click(&mut h, "mask:1");
    click(&mut h, "layerGlobalGroup");
    for name in ["Profile & RAW", "Negative", "Tone Equalizer", "Optics", "Calibration"] {
        assert!(text(&h).contains(name), "{name}");
    }
    click(&mut h, "layerBackground:optics");
    assert_eq!(h.app.session.active_mask, None);
    // collapsing the list keeps one line naming the edited layer
    click(&mut h, "mask:2");
    click(&mut h, "button:layerCollapse");
    assert!(!h.app.widgets.iter().any(|(id, _)| id == "mask:1"));
    let name = d(&h).masks[1].name.clone();
    assert!(text(&h).contains(&format!("Layers · {name}")), "{}", text(&h));
}

/// The UI's stored order must drive the renderer, including its warm caches.
#[test]
fn develop_a_stack_reorder_drag_changes_displayed_pixels() {
    use crate::render::Slot;
    let mut h = setup(400.0, true, 1.0);
    h.app.ui.mask_overlay = false;
    h.app.renderer.keep_pixels = true;
    exec(&mut h, "develop.merge", json!({"settings":{"light":{"exposure":0.6,"contrast":40.0,"highlights":-50.0,"shadows":45.0}}}));
    new_layer(&mut h, "full");
    exec(&mut h, "mask.setTools", json!({"id":1,"tools":{"curve":{"master":[{"x":0.0,"y":0.12},{"x":1.0,"y":1.0}]},"mixer":{"blue":{"hue":50.0}}}}));
    new_layer(&mut h, "full");
    click(&mut h, "section:layer-light");
    let slider = rect(&h, "slider:light.exposure");
    drag(&mut h, "slider:light.exposure", -slider.width() * 0.1);
    assert!(d(&h).masks[1].tools.light.unwrap().exposure < 0.0);
    assert!(h.settle(T));
    let before = h.app.renderer.textures[&Slot::Main].pixels.as_ref().unwrap().clone();
    let to = rect(&h, "mask:1").center();
    request(&mut h, "ui.dragWidget", json!({"id":"mask:2","toX":to.x,"toY":to.y,"steps":8}));
    assert_eq!(d(&h).masks[0].id, 2);
    assert!(h.settle(T));
    let after = h.app.renderer.textures[&Slot::Main].pixels.as_ref().unwrap().clone();
    assert!(before.pixels != after.pixels, "reordering curve and exposure must change the actual preview");
    exec(&mut h, "edit.undo", json!({}));
    assert!(h.settle(T));
    let restored = h.app.renderer.textures[&Slot::Main].pixels.as_ref().unwrap();
    assert!(restored.pixels == before.pixels, "undo must restore the pixels, including cached prefixes");
}
