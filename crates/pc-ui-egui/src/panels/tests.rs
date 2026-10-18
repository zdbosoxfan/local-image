use super::*;

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn srgb_hsva_roundtrip() {
        for c in [[0.847, 0.271, 0.180, 1.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0], [0.2, 0.6, 0.4, 1.0]] {
            let back = hsva_srgb(srgb_hsva(c));
            for i in 0..3 {
                assert!((back[i] - c[i]).abs() <= 1.0 / 255.0, "{c:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn hue_and_saturation_fields_take_values_on_black() {
        use egui_kittest::kittest::Queryable;
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.session.tools.foreground = [0.0, 0.0, 0.0, 1.0];
        let mut h = egui_kittest::Harness::builder().with_size(vec2(300.0, 300.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                    color_picker(app, ui);
                }
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(2);
        // Hue, Saturation, Brightness, typed one key per frame. Black has no hue or saturation of its
        // own, so the first two used to reset to 0 and this ended on white.
        for (field, typed) in [(0, "120"), (1, "100"), (2, "100")] {
            h.query_all_by_role(egui::accesskit::Role::SpinButton).nth(field).unwrap().click();
            h.run_steps(1);
            for ch in typed.chars() {
                h.event(egui::Event::Text(ch.to_string()));
                h.run_steps(1);
            }
            h.key_press(egui::Key::Tab);
            h.run_steps(2);
        }
        assert_eq!(srgb_bytes(h.state().session.tools.foreground), [0, 255, 0]);
    }

    fn field_harness(app: PhotocraftApp) -> egui_kittest::Harness<'static, PhotocraftApp> {
        // 60 fps steps, so two clicks a frame apart are a double-click.
        let mut h = egui_kittest::Harness::builder().with_size(vec2(300.0, 200.0)).with_step_dt(1.0 / 60.0).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                    color_field(app, ui);
                }
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(2);
        h
    }

    fn click(h: &mut egui_kittest::Harness<'_, PhotocraftApp>, p: egui::Pos2) {
        h.hover_at(p);
        h.run_steps(1);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
    }

    /// A click on the background chip makes the field edit the background, not the foreground.
    #[test]
    fn background_chip_retargets_the_color_field() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.session.tools.foreground = [0.2, 0.4, 0.6, 1.0];
        app.session.tools.background = [0.0, 0.0, 0.0, 1.0];
        let mut h = field_harness(app);
        let min = h.ctx.input(|i| i.viewport_rect()).min;
        // The background chip's corner that the foreground chip doesn't cover.
        click(&mut h, min + vec2(40.0, 40.0));
        // The field's top-left: no saturation, full brightness.
        click(&mut h, min + vec2(60.0, 10.0));
        assert!(h.state().ui.color_panel.background);
        let tools = &h.state().session.tools;
        assert_eq!(tools.foreground, [0.2, 0.4, 0.6, 1.0]);
        assert!(tools.background[..3].iter().all(|&v| v > 0.9), "{:?}", tools.background);
    }

    #[test]
    fn double_clicking_a_chip_opens_its_color_picker() {
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let mut h = field_harness(app);
        let min = h.ctx.input(|i| i.viewport_rect()).min;
        // The foreground point is where the chips overlap: the foreground is on top there.
        for (p, target) in [(vec2(40.0, 40.0), "background"), (vec2(30.0, 30.0), "foreground")] {
            h.state_mut().ui.dialogs.clear();
            // Past the last double-click, so this one isn't counted as a triple-click.
            h.run_steps(40);
            click(&mut h, min + p);
            assert!(h.state().ui.dialogs.is_empty(), "a single click only picks the chip");
            click(&mut h, min + p);
            let [d] = h.state().ui.dialogs.as_slice() else { panic!("one Color Picker") };
            assert_eq!(d.fields.get("__colorPicker").and_then(Value::as_str), Some(target));
        }
    }

    /// Black has no hue of its own, so each chip remembers the hue last picked for it.
    #[test]
    fn each_chip_keeps_its_own_hue_on_black() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.session.tools.foreground = [0.0, 0.0, 0.0, 1.0];
        app.session.tools.background = [0.0, 0.0, 0.0, 1.0];
        let mut h = field_harness(app);
        let min = h.ctx.input(|i| i.viewport_rect()).min;
        // The hue strip runs from red at the top through blue (a third down) and green (two thirds).
        let (strip_x, green, blue) = (285.0, 88.0, 48.0);
        click(&mut h, min + vec2(strip_x, green));
        click(&mut h, min + vec2(40.0, 40.0));
        click(&mut h, min + vec2(strip_x, blue));
        click(&mut h, min + vec2(16.0, 16.0));
        // Near the field's top-right: high saturation and brightness.
        click(&mut h, min + vec2(265.0, 10.0));
        let [r, g, b] = srgb_bytes(h.state().session.tools.foreground);
        assert!(g > 200 && r < 64 && b < 64, "the foreground's green, not the background's blue: {:?}", [r, g, b]);
    }
}

#[cfg(test)]
mod history_transform_tests {
    use super::*;
    use egui_kittest::Harness;

    fn click(h: &mut Harness<'static, PhotocraftApp>, p: egui::Pos2) {
        h.hover_at(p);
        h.run_steps(1);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
    }

    /// While Free Transform is open, clicking a History state leaves the document alone: the
    /// transform owns Undo. Without a transform the same click steps back as usual.
    #[test]
    fn history_rows_wait_for_an_open_transform() {
        for transforming in [true, false] {
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
            app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
            app.run("layer.new.layer", json!({})).unwrap();
            app.run("select.rect", json!({"x": 20, "y": 20, "width": 60, "height": 40})).unwrap();
            app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
            app.run("select.deselect", json!({})).unwrap();
            let mut h = Harness::builder().with_size(vec2(300.0, 400.0)).build_ui_state(|ui, app: &mut PhotocraftApp| history(app, ui), app);
            h.run_steps(2);
            if transforming {
                let ctx = h.ctx.clone();
                crate::transform_tool::begin(h.state_mut(), &ctx).unwrap();
            }
            let steps = h.state().session.active().unwrap().history.entries().len();
            // The first state's row, below the 38 pt snapshot row.
            let row = h.ctx.input(|i| i.viewport_rect()).min + vec2(60.0, 60.0);
            click(&mut h, row);
            let after = h.state().session.active().unwrap().history.entries().len();
            if transforming {
                assert_eq!(after, steps, "the document's history is untouched");
                assert!(h.state().ui.transform.is_some());
            } else {
                assert!(after < steps, "the click steps back: {steps} -> {after}");
            }
        }
    }
}

#[cfg(test)]
mod lock_tests {
    use super::*;
    use crate::canvas::{ToolEvent, tool_event};
    use egui::Modifiers;

    fn click_lock(app: &mut PhotocraftApp) {
        let st = app.session.active().unwrap();
        let l = st.active_layer.and_then(|id| st.doc.layer(id)).unwrap();
        let (cmd, p) = simple_lock_toggle(crate::doc_props_ui::is_background(&st.doc, l), l, &json!({"layer": l.id.0}));
        app.run(&cmd, p).unwrap();
    }

    fn erase_line(app: &mut PhotocraftApp) {
        app.ui.tool = Tool::Eraser;
        let m = Modifiers::NONE;
        tool_event(app, ToolEvent::Down { x: 10.0, y: 40.0, pressure: 1.0 }, m);
        tool_event(app, ToolEvent::Move { x: 100.0, y: 40.0, pressure: 1.0 }, m);
        tool_event(app, ToolEvent::Up { x: 100.0, y: 40.0 }, m);
    }

    #[test]
    fn simple_lock_button_never_locks_the_eraser_out_of_the_background() {
        // #76: the simple themes' lock button used to toggle "lock all" on the Background, which
        // kept showing as locked and then refused every paint tool.
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 120, "height": 80})).unwrap();
        app.run("tools.setColors", json!({"background": "#ff0000"})).unwrap();
        app.run("tools.setBrush", json!({"brush": {"size": 12, "hardness": 1.0}})).unwrap();
        // Clicking the Background's lock converts it to a normal layer (Photoshop).
        click_lock(&mut app);
        let l = &app.session.active().unwrap().doc.layers[0];
        assert_eq!(l.name, "Layer 0");
        assert!(!(l.locks.transparency || l.locks.pixels || l.locks.position || l.locks.all));
        erase_line(&mut app);
        assert!(!app.ui.status_error, "{}", app.ui.status);
        assert_eq!(app.session.active().unwrap().doc.layers[0].surface().unwrap().rgba(50, 40)[3], 0.0, "erased to transparency");
        // On a normal layer the button locks all, then unlocks everything.
        click_lock(&mut app);
        assert!(app.session.active().unwrap().doc.layers[0].locks.all);
        click_lock(&mut app);
        let k = app.session.active().unwrap().doc.layers[0].locks;
        assert!(!(k.transparency || k.pixels || k.position || k.artboard || k.all));
        // A layer with only transparency locked unlocks in one click.
        app.run("layer.setProps", json!({"locks": {"transparency": true}})).unwrap();
        click_lock(&mut app);
        assert!(!app.session.active().unwrap().doc.layers[0].locks.transparency);
    }
}

#[cfg(test)]
mod swatch_type_tests {
    use super::*;
    use egui_kittest::Harness;

    /// Clicking a swatch while characters are selected recolours them, not just the foreground.
    #[test]
    fn clicking_a_swatch_recolours_selected_type() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 400, "height": 200})).unwrap();
        let id = app.run("type.create", json!({"text": "Hello world", "size": 40, "x": 20, "y": 100, "color": "#ffffff"})).unwrap()["layer"].as_u64().unwrap();
        app.ui.tool = Tool::Type;
        app.ui.text_edit =
            Some(crate::state::TextEdit { layer: id, caret: 0, anchor: 5, session: "s".into(), created: false, dragging: false, resize: None, preedit: None });
        let mut h = Harness::builder().with_size(vec2(300.0, 600.0)).build_ui_state(|ui, app: &mut PhotocraftApp| swatches(app, ui), app);
        h.run_steps(2);
        use egui_kittest::kittest::Queryable;
        h.get_by_label("Starter 1").click();
        h.run_steps(2);
        let s = SWATCHES[0];
        assert_eq!(h.state().session.tools.foreground, [s[0] as f32 / 255.0, s[1] as f32 / 255.0, s[2] as f32 / 255.0, 1.0]);
        let st = h.state().session.active().unwrap();
        let Some(photocraft_doc::LayerContent::Text(t)) = st.doc.layer(photocraft_doc::LayerId(id)).map(|l| &l.content) else { panic!("type layer") };
        let runs = t.char_runs();
        assert_eq!(runs[0].len, 5, "the selection is its own run");
        assert_eq!(runs[0].style.color.to_rgba8(), [s[0], s[1], s[2], 255]);
        assert_eq!(runs[1].style.color.to_rgba8(), [255, 255, 255, 255]);
    }

    /// The HSB sliders only act on an edit. Their h/s/v round trip isn't exact for every colour
    /// (#D8452E isn't), and comparing values used to rewrite the foreground every frame, which
    /// would recolour selected type the moment it was selected.
    #[test]
    fn idle_hsb_sliders_leave_the_foreground_and_selected_type_alone() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 400, "height": 200})).unwrap();
        let id = app.run("type.create", json!({"text": "Hello world", "size": 40, "x": 20, "y": 100, "color": "#ffffff"})).unwrap()["layer"].as_u64().unwrap();
        app.run("tools.setColors", json!({"foreground": "#d8452e"})).unwrap();
        let fg = app.session.tools.foreground;
        app.ui.tool = Tool::Type;
        app.ui.text_edit =
            Some(crate::state::TextEdit { layer: id, caret: 0, anchor: 5, session: "s".into(), created: false, dragging: false, resize: None, preedit: None });
        let mut h = Harness::builder().with_size(vec2(300.0, 300.0)).build_ui_state(|ui, app: &mut PhotocraftApp| color_picker(app, ui), app);
        h.run_steps(4);
        assert_eq!(h.state().session.tools.foreground, fg);
        let st = h.state().session.active().unwrap();
        let Some(photocraft_doc::LayerContent::Text(t)) = st.doc.layer(photocraft_doc::LayerId(id)).map(|l| &l.content) else { panic!("type layer") };
        assert_eq!(t.char_runs().len(), 1, "still one white run");
    }
}

#[cfg(test)]
mod type_flyout_tests {
    use super::*;

    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn long_press_type_button_selects_vertical_without_selecting_on_release() {
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let initial = app.ui.tool;
        let mut h = Harness::builder().with_size(vec2(1200.0, 1800.0)).with_step_dt(0.1).build_ui_state(|ui, app: &mut PhotocraftApp| toolbar(app, ui), app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.run_steps(2);
        let index = TOOL_SECTIONS.iter().flat_map(|section| section.iter()).position(|slot| slot.contains(&Tool::Type)).unwrap();
        let at = h.get_by_role_and_label(egui::accesskit::Role::Button, "Horizontal Type Tool").rect().center();
        let pointer = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        h.event(egui::Event::PointerMoved(at));
        h.event(pointer(at, true));
        h.run_steps(5);
        assert_eq!(h.ctx.data(|d| d.get_temp::<(egui::Id, Rect)>(egui::Id::new("tool-flyout"))).map(|(id, _)| id), Some(egui::Id::new(("tool-slot", index))));
        h.event(pointer(at, false));
        h.run_steps(2);
        assert_eq!(h.state().ui.tool, initial);
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Vertical Type Tool").click();
        h.run_steps(2);
        assert_eq!(h.state().ui.tool, Tool::VerticalType);
        assert!(h.ctx.data(|d| d.get_temp::<(egui::Id, Rect)>(egui::Id::new("tool-flyout"))).is_none());
    }
}

#[cfg(test)]
mod properties_card_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    /// Arithmetic left uncommitted in one fill layer's card never lands on the layer selected next.
    #[test]
    fn uncommitted_arithmetic_stays_with_its_layer() {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 64, "height": 64})).unwrap();
        let mut ids = Vec::new();
        for color in ["#00ff00", "#0000ff"] {
            s.execute("layer.newFillLayer.solidColor", json!({"color": color})).unwrap();
            ids.push(s.active().unwrap().active_layer.unwrap());
        }
        // The layer selected next already shows 25, the value the edited one has when `25*` stops it.
        s.execute("layer.setProps", json!({"layer": ids[0].0, "opacity": 0.25})).unwrap();
        let mut app = PhotocraftApp::new(s, crate::Services::default());
        app.ui.panels.properties = true;
        let mut h = Harness::builder().with_size(vec2(800.0, 600.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                    properties_window(app, ui.ctx());
                }
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(2);
        h.query_all_by_role(egui::accesskit::Role::SpinButton).next().unwrap().click();
        h.run_steps(1);
        for ch in "25*2".chars() {
            h.event(egui::Event::Text(ch.to_string()));
            h.run_steps(1);
        }
        // The Layers panel selects the other layer in the frame the click leaves the field.
        h.state_mut().session.execute("layer.select", json!({"layer": ids[0].0})).unwrap();
        h.ctx.memory_mut(|m| m.stop_text_input());
        h.run_steps(3);
        let doc = &h.state().session.active().unwrap().doc;
        assert_eq!(ids.iter().map(|&id| doc.layer(id).unwrap().opacity).collect::<Vec<_>>(), [0.25, 0.25]);
    }
}

#[cfg(test)]
mod title_tests {
    use super::title_x;

    #[test]
    fn title_never_overlaps_menus_or_controls() {
        // Wide window: centred.
        assert_eq!(title_x(800.0, 420.0, 1300.0, 60.0), Some(770.0));
        // Centre would hit the menus: slid right into the gap.
        assert_eq!(title_x(400.0, 420.0, 1300.0, 60.0), Some(436.0));
        // Centre would hit the controls: slid left.
        assert_eq!(title_x(1280.0, 420.0, 1300.0, 60.0), Some(1224.0));
        // No room for the whole title: the caller elides or drops it.
        assert_eq!(title_x(400.0, 420.0, 480.0, 60.0), None);
        assert_eq!(title_x(f32::NAN, 420.0, 1300.0, 60.0), Some(436.0));
        assert_eq!(title_x(400.0, f32::INFINITY, 1300.0, 60.0), None);
    }
}

#[cfg(test)]
mod several_layer_tests {
    //! The Layers panel's blend mode, Opacity, Fill and lock controls change every selected layer,
    //! as in Photoshop, not only the active one.
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;

    fn harness(theme: crate::theme::ThemeKind) -> Harness<'static, PhotocraftApp> {
        let mut h = Harness::builder().with_size(vec2(1400.0, 900.0)).with_max_steps(64).build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, theme);
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
            app.run("prefs.set", json!({"path": "interface.theme", "value": theme.id()})).unwrap();
            app.ui.theme = theme;
            app.run("file.new", json!({"width": 64, "height": 64, "background": "white"})).unwrap();
            app
        });
        h.run_steps(6);
        h
    }

    fn locked(h: &Harness<'static, PhotocraftApp>, id: u64) -> bool {
        h.state().session.active().unwrap().doc.layer(photocraft_doc::LayerId(id)).unwrap().locks.all
    }

    #[test]
    fn the_layers_panel_locks_every_selected_layer() {
        let mut h = harness(crate::theme::ThemeKind::default());
        let background = h.state().session.active().unwrap().doc.layers[0].id.0;
        let a = h.state_mut().run("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap();
        let b = h.state_mut().run("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap();
        h.state_mut().run("layer.select", json!({"layer": a, "mode": "add"})).unwrap();
        h.run_steps(3);
        assert_eq!(h.state().session.active().unwrap().selected_layers().len(), 2);
        h.query_by_label_contains("Lock all").expect("the Lock all button").click();
        h.run_steps(3);
        assert!(locked(&h, a) && locked(&h, b), "both selected layers lock");
        assert!(!locked(&h, background), "the unselected Background doesn't");
    }

    #[test]
    fn the_basic_lock_toggle_locks_every_selected_layer() {
        let mut h = harness(crate::theme::ThemeKind::Studio);
        let a = h.state_mut().run("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap();
        let b = h.state_mut().run("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap();
        h.state_mut().run("layer.select", json!({"layer": a, "mode": "add"})).unwrap();
        h.run_steps(4);
        h.get_by_label("Lock layer").click();
        h.run_steps(3);
        assert!(locked(&h, a) && locked(&h, b), "both selected layers lock");
    }

    /// A selected Background is left out of a several-layer change: an Opacity, Fill or blend
    /// change would turn it into a normal layer.
    #[test]
    fn a_selected_background_is_left_out_of_a_several_layer_change() {
        let mut h = harness(crate::theme::ThemeKind::default());
        let background = h.state().session.active().unwrap().doc.layers[0].id.0;
        let top = h.state_mut().run("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap();
        h.state_mut().run("layer.select", json!({"layer": background})).unwrap();
        h.state_mut().run("layer.select", json!({"layer": top, "mode": "add"})).unwrap();
        h.run_steps(3);
        h.get_by_label("Lock all").click();
        h.run_steps(3);
        assert!(locked(&h, top), "the layer locks");
        let st = h.state().session.active().unwrap();
        assert!(crate::doc_props_ui::is_background(&st.doc, st.doc.layer(photocraft_doc::LayerId(background)).unwrap()));
        assert!(!locked(&h, background), "the Background is left as it was");
    }
}

#[cfg(test)]
mod crop_ratio_tests {
    use super::*;

    #[test]
    fn the_crop_ratio_fields_show_three_decimals_and_showing_them_changes_nothing() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.ui.tool = Tool::Crop;
        app.ui.tool_options.crop_ratio = "1.414:1".into();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1600.0, 90.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                    options_bar(app, ui);
                }
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(4);
        fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.job.text.clone()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
                _ => {}
            }
        }
        let mut shown = Vec::new();
        h.output().shapes.iter().for_each(|s| texts(&s.shape, &mut shown));
        assert!(shown.iter().any(|t| t == "1.414"), "{shown:?}");
        assert_eq!(h.state().ui.tool_options.crop_ratio, "1.414:1");
    }

    #[test]
    fn number_text_keeps_the_requested_decimals_without_trailing_zeros() {
        assert_eq!(widgets::fmt_num_prec(1.414, 3), "1.414");
        assert_eq!(widgets::fmt_num_prec(16.0, 3), "16");
        assert_eq!(widgets::fmt_num_prec(0.7995, 2), "0.8");
        assert_eq!(widgets::fmt_num_prec(-0.0001, 3), "0");
        assert_eq!(widgets::fmt_num_prec(12.0, 0), "12");
    }
}
