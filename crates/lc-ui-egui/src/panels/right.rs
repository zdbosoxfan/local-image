//! The right-hand panel next to the tool strip: Edit, Crop, Remove, Masking, Red Eye, Info,
//! Keywords, Versions, Activity.

use egui::{Align2, Rect, Sense, pos2, vec2};
use lightcraft_catalog::PhotoId;
use serde_json::json;

use crate::LightcraftApp;
use crate::icons::{Icon, paint};
use crate::state::RightPanel;
use crate::theme::Tokens;
use crate::widgets::{divider, register, text_button};

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let frame = egui::Frame::NONE.fill(t.chrome).stroke(egui::Stroke::new(1.0, t.divider));
    // the presets column and the left sidebar are laid out after this panel: leave them their room
    let reserve = if app.ui.presets { t.panel_w } else { 0.0 } + if app.ui.left_panel { crate::state::LEFT_WIDTH.min } else { 0.0 };
    let width = app.ui.right_width;
    let resized = super::resizable_side(ui, false, "right_panel", frame, width, crate::state::RIGHT_WIDTH, reserve, |ui| {
        let Some(id) = app.session.active() else {
            let r = ui.max_rect();
            super::empty_message(ui, r, "No photo selected", "Select a photo to edit");
            return;
        };
        egui::ScrollArea::vertical().id_salt("right-scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            match app.ui.right {
                RightPanel::Edit => super::edit::show(app, ui, id),
                RightPanel::Profiles => super::profiles::show(app, ui, id),
                RightPanel::Crop => crop(app, ui, id),
                RightPanel::Remove => remove(app, ui, id),
                RightPanel::Masking => super::masking::show(app, ui, id),
                RightPanel::RedEye => red_eye(app, ui, id),
                RightPanel::Info => info(app, ui, id),
                RightPanel::Keywords => keywords(app, ui, id),
                RightPanel::Versions => versions(app, ui, id),
                RightPanel::Activity => activity(app, ui, id),
                RightPanel::None => {}
            }
        });
    });
    if let Some(w) = resized {
        app.ui.right_width = w;
    }
}

pub fn header(ui: &mut egui::Ui, title: &str) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
    ui.painter().text(pos2(r.left() + 24.0, r.center().y + 2.0), Align2::LEFT_CENTER, li_i18n::library::t(title), t.semibold(15.0), t.text);
}

pub fn label_row(ui: &mut egui::Ui, label: &str, value: &str) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    ui.painter().text(pos2(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, li_i18n::library::t(label), t.font(12.5), t.text_dim);
    ui.painter().text(pos2(r.left() + 110.0, r.center().y), Align2::LEFT_CENTER, value, t.font(12.5), t.text_label);
}

fn padded(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 6, bottom: 6 }).show(ui, add);
}

fn crop(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let t = Tokens::get(ui.ctx());
    let d = app.session.develop_of(id).unwrap_or_default();
    header(ui, "Crop");
    padded(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(li_i18n::library::t("Aspect Ratio"));
            let cur = d.crop.aspect.map(|(w, h)| format!("{} × {}", w as f64 / 100.0, h as f64 / 100.0)).unwrap_or_else(|| "Free".into());
            let r = ui.add(egui::Button::new(li_i18n::library::t(&cur)).frame(false));
            register(ui.ctx(), "button:cropAspect", r.rect);
            egui::Popup::menu(&r).show(|ui| {
                for (label, a) in [
                    ("Free", "free"),
                    ("Original", "original"),
                    ("1 × 1", "1x1"),
                    ("4 × 5 / 8 × 10", "4x5"),
                    ("8.5 × 11", "8.5x11"),
                    ("5 × 7", "5x7"),
                    ("2 × 3 / 4 × 6", "2x3"),
                    ("4 × 3", "4x3"),
                    ("16 × 9", "16x9"),
                    ("16 × 10", "16x10"),
                ] {
                    if ui.button(li_i18n::library::t(label)).clicked() {
                        let _ = app.run("crop.aspect", json!({"aspect": a}));
                    }
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if text_button(ui, "cropRotateLeft", li_i18n::library::t("Rotate Left"), false).clicked() {
                let _ = app.run("photo.rotateLeft", json!({}));
            }
            if text_button(ui, "cropRotateRight", li_i18n::library::t("Rotate Right"), false).clicked() {
                let _ = app.run("photo.rotateRight", json!({}));
            }
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if text_button(ui, "cropFlipH", li_i18n::library::t("Flip H"), d.crop.flip_h).clicked() {
                let _ = app.run("photo.flipHorizontal", json!({}));
            }
            if text_button(ui, "cropFlipV", li_i18n::library::t("Flip V"), d.crop.flip_v).clicked() {
                let _ = app.run("photo.flipVertical", json!({}));
            }
            if text_button(ui, "cropSwapAspect", li_i18n::library::t("Swap Aspect (X)"), false).clicked() {
                let _ = app.run("crop.rotateAspect", json!({}));
            }
            if text_button(ui, "cropReset", li_i18n::library::t("Reset"), false).clicked() {
                let _ = app.run("crop.reset", json!({}));
            }
        });
    });
    let ang = lightcraft_develop::controls::find("crop.angle").copied();
    if let Some(spec) = ang {
        let out = crate::widgets::numeric_slider(
            ui,
            &spec,
            d.crop.geometry.angle,
            true,
            Some("Straighten"),
            crate::widgets::numeric_owner(app, ("photo-tools", app.session.active_spot, app.ui.eye)),
        );
        super::edit::apply_slider_out(app, &spec, out, |app, v| app.run("crop.straighten", json!({"angle": v})));
    }
    padded(ui, |ui| {
        ui.horizontal(|ui| {
            let on = app.ui.tool == "straighten";
            let (icon_rect, icon_response) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
            crate::icons::paint_tool(ui, icon_rect, "straighten", Icon::Rotate, t.icon, true);
            let resp = text_button(ui, "straightenTool", li_i18n::library::t("Straighten Tool"), on).union(icon_response);
            super::tool_tips::attach(app.ui.settings.tool_tips, ui, &resp, super::tool_tips::StripTool::Straighten);
            if resp.clicked() {
                app.ui.tool = if on { String::new() } else { "straighten".into() };
            }
            if text_button(ui, "straightenAuto", li_i18n::library::t("Auto"), false).clicked() {
                let _ = app.run("crop.autoStraighten", json!({}));
            }
        });
    });
    padded(ui, |ui| {
        ui.label(li_i18n::library::t("Overlay"));
        {
            use crate::state::CropOverlay as O;
            let opts = [
                (O::Thirds, "Thirds", "thirds"),
                (O::Grid, "Grid", "grid"),
                (O::Golden, "Golden", "golden"),
                (O::Diagonal, "Diagonal", "diagonal"),
                (O::Triangle, "Triangle", "triangle"),
                (O::Spiral, "Spiral", "spiral"),
                (O::None, "None", "none"),
            ];
            let items: Vec<(&str, &str)> = opts.iter().map(|(_, l, k)| (*l, *k)).collect();
            let active = opts.iter().position(|(o, _, _)| *o == app.ui.crop_overlay);
            if let Some(i) = crate::widgets::segmented(ui, "cropOverlay", &items, active, 4) {
                app.ui.crop_overlay = opts[i].0;
            }
            if matches!(app.ui.crop_overlay, O::Triangle | O::Spiral)
                && text_button(ui, "cropOverlayOrient", li_i18n::library::t("Flip Overlay (⇧O)"), false).clicked()
            {
                let _ = app.run("view.cropOverlayOrientation", json!({}));
            }
        }
    });
    divider(ui);
    header(ui, "Geometry");
    padded(ui, |ui| {
        ui.label(li_i18n::library::t("Upright"));
        {
            use lightcraft_develop::Upright;
            let modes = [
                ("Off", Upright::Off, "off"),
                ("Auto", Upright::Auto, "auto"),
                ("Guided", Upright::Guided, "guided"),
                ("Level", Upright::Level, "level"),
                ("Vertical", Upright::Vertical, "vertical"),
                ("Full", Upright::Full, "full"),
            ];
            let items: Vec<(&str, &str)> = modes.iter().map(|(l, _, k)| (*l, *k)).collect();
            let active = modes.iter().position(|(_, m, _)| *m == d.geometry.upright);
            if let Some(i) = crate::widgets::segmented(ui, "upright", &items, active, 3) {
                let (_, mode, key) = modes[i];
                let _ = app.run("geometry.upright", json!({"mode": key}));
                app.ui.tool = if mode == Upright::Guided { "guidedUpright".into() } else { String::new() };
            }
        }
        let guided = d.geometry.upright == lightcraft_develop::Upright::Guided;
        ui.horizontal_wrapped(|ui| {
            if !guided
                && d.geometry.upright != lightcraft_develop::Upright::Off
                && text_button(ui, "uprightUpdate", li_i18n::library::t("Update"), false).clicked()
            {
                let mode = serde_json::to_value(d.geometry.upright).unwrap_or_default();
                let _ = app.run("geometry.upright", json!({"mode": mode}));
            }
            if guided {
                let drawing = app.ui.tool == "guidedUpright";
                let (icon_rect, icon_response) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                crate::icons::paint_tool(ui, icon_rect, "guided-upright", Icon::Crop, t.icon, true);
                let resp = text_button(ui, "uprightDraw", li_i18n::library::t("Draw Guides"), drawing).union(icon_response);
                super::tool_tips::attach(app.ui.settings.tool_tips, ui, &resp, super::tool_tips::StripTool::Guided);
                if resp.clicked() {
                    app.ui.tool = if drawing { String::new() } else { "guidedUpright".into() };
                }
                if !d.geometry.guides.is_empty() && text_button(ui, "uprightClear", li_i18n::library::t("Clear Guides"), false).clicked() {
                    let _ = app.run("geometry.guides", json!({"guides": []}));
                }
            }
        });
        if guided {
            ui.label(
                egui::RichText::new(li_i18n::tr_format!(
                    "{} of 4 guides — drag along lines that should be vertical or horizontal.",
                    d.geometry.guides.len()
                ))
                .size(11.0)
                .color(Tokens::get(ui.ctx()).text_dim),
            );
        }
        ui.add_space(4.0);
        let mut c = d.geometry.constrain_crop;
        if ui.checkbox(&mut c, li_i18n::library::t("Constrain Crop")).changed() {
            let _ = app.run("develop.merge", json!({"settings": {"geometry": {"constrain_crop": c}}, "label": "Constrain Crop"}));
        }
    });
    for spec in lightcraft_develop::controls::in_section(lightcraft_develop::Section::Geometry).filter(|c| c.id != "crop.angle") {
        let v = lightcraft_develop::controls::get(&d, spec.id).unwrap_or(spec.default);
        let out = crate::widgets::numeric_slider(
            ui,
            spec,
            v,
            true,
            None,
            crate::widgets::numeric_owner(app, ("photo-tools", app.session.active_spot, app.ui.eye)),
        );
        super::edit::apply_slider_out(app, spec, out, |app, v| app.run("develop.set", json!({"control": spec.id, "value": v})));
    }
}

fn remove(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let d = app.session.develop_of(id).unwrap_or_default();
    header(ui, "Remove");
    padded(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (label, tool) in [("AI", "ai"), ("Remove", "remove"), ("Heal", "heal"), ("Clone", "clone")] {
                let tip_tool = match tool {
                    "ai" => super::tool_tips::StripTool::RemoveAi,
                    "remove" => super::tool_tips::StripTool::RemovePixels,
                    "heal" => super::tool_tips::StripTool::Heal,
                    _ => super::tool_tips::StripTool::Clone,
                };
                let response = ui
                    .vertical(|ui| {
                        let icon = crate::widgets::tool_button(
                            ui,
                            &format!("removeMode-{tool}"),
                            tip_tool.icon(),
                            vec2(32.0, 28.0),
                            app.ui.tool == tool,
                            true,
                            "",
                        );
                        let text = text_button(ui, &format!("removeMode-{tool}"), label, app.ui.tool == tool);
                        icon.union(text)
                    })
                    .inner;
                super::tool_tips::attach(app.ui.settings.tool_tips, ui, &response, tip_tool);
                if response.clicked() {
                    app.ui.tool = tool.into();
                    // a new brush: the spot selected before is no longer what the sliders edit
                    if app.session.active_spot.is_some() {
                        let _ = app.run("spot.select", json!({"index": null}));
                    }
                }
            }
        });
        if app.ui.tool == "heal" {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(li_i18n::library::t("Paint over a blemish: it is filled from the texture around it, on this computer."))
                    .size(11.0)
                    .color(Tokens::get(ui.ctx()).text_dim),
            );
        }
        ui.add_space(6.0);
        // Lightroom's Tool Overlay: when pins and outlines show
        ui.horizontal(|ui| {
            ui.label(li_i18n::library::t("Overlay"));
            use crate::state::SpotOverlay;
            for (label, key, v) in
                [("Auto", "auto", SpotOverlay::Auto), ("Always", "always", SpotOverlay::Always), ("Never", "never", SpotOverlay::Never)]
            {
                if text_button(ui, &format!("removeOverlay-{key}"), label, app.ui.remove_overlay == v).clicked() {
                    app.ui.remove_overlay = v;
                }
            }
        });
        ui.add_space(8.0);
        ui.label(li_i18n::tr_format!("{} spot(s) on this photo", d.spots.len()));
        ui.add_space(4.0);
        if text_button(ui, "findDust", li_i18n::library::t("Find Dust Spots"), false)
            .on_hover_text(li_i18n::library::t("Heal the small, soft dark spots sensor dust leaves on smooth areas"))
            .clicked()
        {
            match app.run("spot.findDust", json!({})) {
                Ok(r) => {
                    let n = r["added"].as_u64().unwrap_or(0);
                    app.toast(
                        ui.ctx(),
                        if n == 0 {
                            "No dust spots found".to_string()
                        } else {
                            li_i18n::tr_format!("Healed {n} dust spot{}", if n == 1 { "" } else { "s" }, n = n)
                        },
                    );
                }
                Err(e) => app.toast(ui.ctx(), e),
            }
        }
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(li_i18n::library::t("Paint over a distraction on the photo to remove it.")).color(Tokens::get(ui.ctx()).text_dim),
        );
    });
    // local-image: generative AI removal (engine, brush or lasso, a layer's area)
    if app.ui.tool == "ai" {
        super::enhance::remove_controls(app, ui, &d);
    } else {
        // content-aware heals being made
        super::enhance::remove_jobs(app, ui);
    }
    // brush settings; with a spot selected they edit that spot too (not an AI removal's or a
    // content-aware heal's: their size and feather are fixed, the brush keeps its own)
    let sel = app.session.active_spot.and_then(|i| d.spots.get(i).map(|sp| (i, sp.clone())));
    if let Some((i, sp)) = &sel {
        if !sp.is_ai() {
            (app.ui.remove_size, app.ui.remove_feather, app.ui.remove_opacity) = (sp.size as f32, sp.feather as f32, sp.opacity as f32);
        }
        divider(ui);
        let mode = match sp.mode {
            lightcraft_develop::SpotMode::Heal => "Heal",
            lightcraft_develop::SpotMode::Clone => "Clone",
            lightcraft_develop::SpotMode::Remove => "Remove",
            lightcraft_develop::SpotMode::Ai if super::enhance::is_local(sp) => "Heal",
            lightcraft_develop::SpotMode::Ai => "AI",
        };
        super::edit::sub_title(ui, &li_i18n::tr_format!("{mode} spot {} of {}", i + 1, d.spots.len(), mode = mode));
    }
    let plain = |id: &'static str, label: &'static str, min: f64, max: f64, default: f64| lightcraft_develop::ControlSpec {
        id,
        label,
        section: lightcraft_develop::Section::Detail,
        min,
        max,
        default,
        step: 1.0,
        decimals: 0,
        track: lightcraft_develop::Track::Plain,
    };
    // (a selected AI removal's or heal's opacity is its own; size and feather stay the brush's)
    let ai_opacity = sel.as_ref().filter(|(_, sp)| sp.is_ai()).map(|(_, sp)| sp.opacity);
    let sliders = [
        (plain("ui.removeSize", "Size", 1.0, 250.0, 20.0), "size", (app.ui.remove_size * 1000.0) as f64),
        (plain("ui.removeFeather", "Feather", 0.0, 100.0, 50.0), "feather", app.ui.remove_feather as f64),
        (plain("ui.removeOpacity", "Opacity", 0.0, 100.0, 100.0), "opacity", ai_opacity.unwrap_or(app.ui.remove_opacity as f64)),
    ];
    for (spec, key, v) in sliders {
        let out = crate::widgets::numeric_slider(
            ui,
            &spec,
            v,
            true,
            None,
            crate::widgets::numeric_owner(app, ("photo-tools", app.session.active_spot, app.ui.eye)),
        );
        let scale = if key == "size" { 1000.0 } else { 1.0 };
        if let Some(v) = out.value {
            match key {
                "size" => app.ui.remove_size = (v / scale) as f32,
                "feather" => app.ui.remove_feather = v as f32,
                _ if ai_opacity.is_some() => {}
                _ => app.ui.remove_opacity = v as f32,
            }
        }
        // (an AI removal's pixels were generated for its shape: only its opacity changes)
        if sel.as_ref().is_some_and(|(_, sp)| !sp.is_ai() || key == "opacity") {
            super::edit::apply_slider_out(app, &spec, out, |app, v| app.run("spot.update", json!({key: v / scale})));
        }
    }
    if let Some((_, sp)) = sel.as_ref().filter(|(_, sp)| sp.is_ai()) {
        super::enhance::spot_info(app, ui, sp);
    } else if sel.is_some() {
        padded(ui, |ui| {
            ui.horizontal(|ui| {
                if text_button(ui, "spotRefresh", li_i18n::library::t("Refresh Source (/)"), false).clicked() {
                    let _ = app.run("spot.refreshSource", json!({}));
                }
                if text_button(ui, "spotDelete", li_i18n::library::t("Delete (⌫)"), false).clicked() {
                    let _ = app.run("spot.delete", json!({}));
                }
            });
        });
    }
    // Visualize Spots (A): a black/white high-pass view that makes dust and specks stand out
    padded(ui, |ui| {
        let mut v = app.ui.visualize_spots;
        if ui.checkbox(&mut v, li_i18n::library::t("Visualize Spots (A)")).changed() {
            app.ui.visualize_spots = v;
        }
    });
    let spec = lightcraft_develop::ControlSpec {
        id: "ui.spotsThreshold",
        label: "Threshold",
        section: lightcraft_develop::Section::Detail,
        min: 0.0,
        max: 100.0,
        default: 50.0,
        step: 1.0,
        decimals: 0,
        track: lightcraft_develop::Track::Plain,
    };
    let out = crate::widgets::numeric_slider(
        ui,
        &spec,
        app.ui.spots_threshold as f64,
        app.ui.visualize_spots,
        None,
        crate::widgets::numeric_owner(app, ("photo-tools", app.session.active_spot, app.ui.eye)),
    );
    if let Some(v) = out.value {
        app.ui.spots_threshold = v as f32;
    }
    padded(ui, |ui| {
        if !d.spots.is_empty() && text_button(ui, "removeClear", li_i18n::library::t("Delete all spots"), false).clicked() {
            for i in (0..d.spots.len()).rev() {
                let _ = app.run("spot.delete", json!({"index": i}));
            }
        }
    });
}

fn red_eye(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let d = app.session.develop_of(id).unwrap_or_default();
    header(ui, "Red Eye");
    padded(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (label, pet) in [("Red Eye", false), ("Pet Eye", true)] {
                let (icon_rect, icon_response) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                crate::icons::paint_tool(ui, icon_rect, "red-eye", Icon::Eye, Tokens::get(ui.ctx()).icon, true);
                let resp =
                    text_button(ui, &format!("eyeMode-{}", if pet { "pet" } else { "red" }), label, app.ui.eye_pet == pet).union(icon_response);
                super::tool_tips::attach(
                    app.ui.settings.tool_tips,
                    ui,
                    &resp,
                    if pet { super::tool_tips::StripTool::PetEye } else { super::tool_tips::StripTool::RedEye },
                );
                if resp.clicked() {
                    app.ui.eye_pet = pet;
                }
            }
        });
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(li_i18n::library::t("Drag over an eye on the photo; the pupil inside is found automatically."))
                .color(Tokens::get(ui.ctx()).text_dim),
        );
    });
    let n = d.red_eye.len();
    if n == 0 {
        return;
    }
    if app.ui.eye >= n {
        app.ui.eye = n - 1;
    }
    let i = app.ui.eye;
    let eye = d.red_eye[i];
    divider(ui);
    super::edit::sub_title(ui, &format!("{} {} of {n}", if eye.pet { "Pet Eye" } else { "Red Eye" }, i + 1));
    for k in ["pupilSize", "darken"] {
        let cid = format!("redEye.{i}.{k}");
        let Some(spec) = lightcraft_develop::controls::find(&cid) else { continue };
        let v = lightcraft_develop::controls::get(&d, &cid).unwrap_or(spec.default);
        let out = crate::widgets::numeric_slider(
            ui,
            spec,
            v,
            true,
            None,
            crate::widgets::numeric_owner(app, ("photo-tools", app.session.active_spot, app.ui.eye)),
        );
        super::edit::apply_slider_out(app, spec, out, |app, v| app.run("develop.set", json!({"control": cid, "value": v})));
    }
    padded(ui, |ui| {
        if eye.pet {
            let mut on = eye.catchlight.is_some();
            if ui.checkbox(&mut on, li_i18n::library::t("Add Catchlight")).changed() {
                let _ = app.run("redeye.catchlight", json!({"index": i, "on": on}));
            }
            ui.add_space(4.0);
        }
        ui.horizontal(|ui| {
            if text_button(ui, "eyeDelete", li_i18n::library::t("Delete"), false).clicked() {
                let _ = app.run("redeye.delete", json!({"index": i}));
            }
            if n > 1 && text_button(ui, "eyeDeleteAll", li_i18n::library::t("Delete all"), false).clicked() {
                for k in (0..n).rev() {
                    let _ = app.run("redeye.delete", json!({"index": k}));
                }
            }
        });
    });
}

fn info(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let Some(p) = app.session.catalog.photo(id).cloned() else { return };
    header(ui, "Info");
    let t = Tokens::get(ui.ctx());
    padded(ui, |ui| {
        camera_card(ui, &p);
        camera_metadata(ui, &p.meta);
        ui.add_space(6.0);
        if let Some(why) = &p.preview_only {
            crate::widgets::preview_only_notice(ui, "info", why);
            ui.add_space(6.0);
        }
        if let Some(name) = &p.copy_name {
            let of = p.copy_of.and_then(|m| app.session.catalog.photo(m)).map(|m| m.file_name.clone()).unwrap_or_else(|| "a removed photo".into());
            ui.label(egui::RichText::new(format!("Virtual copy “{name}” of {of}")).color(t.text_label));
        }
        ui.add_space(8.0);
        if let Some(r) = ui.horizontal(|ui| crate::widgets::stars(ui, "info", p.rating, 20.0)).inner {
            let _ = app.run("photo.rate", json!({"rating": r}));
        }
        ui.add_space(6.0);
        // colour label: one swatch per label (hover shows its name); clicking the current one clears it
        ui.horizontal(|ui| {
            for l in lightcraft_catalog::ColorLabel::ALL {
                let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                let key = format!("{l:?}").to_lowercase();
                register(ui.ctx(), format!("label:{key}"), r);
                let on = p.label == Some(l);
                ui.painter().circle_filled(r.center(), if on || resp.hovered() { 8.0 } else { 6.5 }, crate::panels::grid::label_color(l));
                if on {
                    ui.painter().circle_stroke(r.center(), 10.0, egui::Stroke::new(1.5, t.text));
                }
                let resp = resp.on_hover_text(app.session.catalog.label_name(l));
                if resp.clicked() {
                    let _ = app.run("photo.label", json!({"label": if on { "none".to_string() } else { key }}));
                }
            }
            if let Some(l) = p.label {
                ui.label(egui::RichText::new(app.session.catalog.label_name(l)).color(t.text_label));
            }
        });
    });
    divider(ui);
    let m = p.meta.clone();
    padded(ui, |ui| {
        for (label, key, value, lines) in [
            ("Title", "title", &m.title, 1),
            ("Caption", "caption", &m.caption, 2),
            ("Alt Text", "altText", &m.alt_text, 2),
            ("Extended Description", "extendedDescription", &m.extended_description, 3),
            ("Copyright", "copyright", &m.copyright, 1),
        ] {
            meta_field(app, ui, label, key, value, lines);
        }
        copyright_status(app, ui, m.copyright_status);
        for (label, key, value, lines) in [
            ("Rights Usage Terms", "usageTerms", &m.usage_terms, 2),
            ("Copyright Info URL", "copyrightUrl", &m.copyright_url, 1),
            ("Creator", "creator", &m.creator, 1),
        ] {
            meta_field(app, ui, label, key, value, lines);
        }
        // file: name (rename), path (reveal), capture time (edit)
        let small = |ui: &mut egui::Ui, text: &str| ui.label(egui::RichText::new(text).size(11.5).color(t.text_dim));
        small(ui, "File Name");
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 22.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::widgets::icon_button(ui, "infoRename", Icon::Pencil, vec2(20.0, 20.0), false, true, "Rename…").clicked() {
                let _ = app.run("dialog.rename", json!({}));
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(egui::RichText::new(&p.file_name).color(t.text)).truncate());
            });
        });
        ui.add_space(4.0);
        small(ui, "File Path");
        ui.horizontal(|ui| {
            let path = match &p.source {
                lightcraft_catalog::Source::File { path } => path.clone(),
                lightcraft_catalog::Source::Demo { .. } => "Generated demo photo".into(),
            };
            ui.add(egui::Label::new(egui::RichText::new(path).size(12.0).color(t.text_label)).truncate());
            if crate::menus::ui_enabled(app, "app.showInFinder") {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::widgets::icon_button(ui, "infoReveal", Icon::Folder, vec2(20.0, 20.0), false, true, crate::menus::REVEAL_LABEL)
                        .clicked()
                    {
                        let _ = app.run("app.showInFinder", json!({}));
                    }
                });
            }
        });
        ui.add_space(4.0);
        small(ui, "Captured");
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 22.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::widgets::icon_button(ui, "editCaptureTime", Icon::Pencil, vec2(20.0, 20.0), false, true, "Edit Capture Time…").clicked() {
                let _ = app.run("dialog.captureTime", json!({}));
            }
            let cap = p.captured.as_deref().map(crate::library_labels::display_time).unwrap_or_else(|| "—".into());
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(egui::RichText::new(cap).size(12.5).color(t.text)).truncate());
            });
        });
        ui.add_space(6.0);
        for (label, key, value) in [
            ("Location", "location", &m.location),
            ("City", "city", &m.city),
            ("State / Province", "state", &m.state),
            ("Country", "country", &m.country),
        ] {
            meta_field(app, ui, label, key, value, 1);
        }
        // GPS: typed as "lat, lon" (or degrees / minutes / seconds); empty clears it
        let gps = m.gps.map(|(la, lo)| format!("{la:.6}, {lo:.6}")).unwrap_or_default();
        let gid = egui::Id::new("info-gps");
        ui.label(egui::RichText::new("GPS").size(11.5).color(t.text_dim));
        let mut text = metadata_draft(app, ui, gid, &gps);
        let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text("latitude, longitude").desired_width(f32::INFINITY));
        register(ui.ctx(), "field:gps", r.rect);
        if metadata_blur(app, ui, gid, &r, &text)
            && text.trim() != gps
            && let Err(e) = app.run("photo.setMeta", json!({"gps": text.trim()}))
        {
            app.toast(ui.ctx(), e);
        }
        if let Some((la, lo)) = m.gps {
            let pretty = format!("{:.5}° {}, {:.5}° {}", la.abs(), if la >= 0.0 { "N" } else { "S" }, lo.abs(), if lo >= 0.0 { "E" } else { "W" });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(pretty).size(11.0).color(t.text_dim));
                if text_button(ui, "showOnMap", li_i18n::library::t("Show on Map"), false)
                    .on_hover_text(li_i18n::library::t("Open the place in OpenStreetMap"))
                    .clicked()
                {
                    let url = format!("https://www.openstreetmap.org/?mlat={la:.6}&mlon={lo:.6}#map=15/{la:.6}/{lo:.6}");
                    if let Err(e) = crate::links::open(app, &url) {
                        app.toast(ui.ctx(), e);
                    }
                }
            });
        }
        if let Some(a) = p.analysis {
            let mut line = format!("Focus {:.0}", a.sharpness);
            if a.clipped > 0.02 {
                line.push_str(&format!(" · {:.0}% clipped", a.clipped * 100.0));
            }
            if a.group.is_some() {
                line.push_str(if a.best { " · best of its burst" } else { " · in a burst" });
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new(line).color(t.text_dim));
        }
        // offline originals / smart previews
        // (cached answers, checked off the UI thread: unknown counts as online / no smart preview)
        if let lightcraft_catalog::Source::File { path } = &p.source {
            let avail = &app.session.media.availability;
            let online = !avail.is_offline(path);
            let smart = app
                .session
                .media
                .smart_dir
                .as_ref()
                .is_some_and(|d| avail.exists(&d.join(lightcraft_engine::smart::file_name(&p)).to_string_lossy()) == Some(true));
            if !online || smart {
                let text = match (online, smart) {
                    (false, true) => "Original offline · editing the smart preview",
                    (false, false) => "Original offline · no smart preview",
                    _ => "Smart preview available",
                };
                ui.add_space(6.0);
                ui.label(egui::RichText::new(text).color(if online { t.text_dim } else { t.accent }));
            }
        }
        ui.add_space(10.0);
        if text_button(ui, "allMetadata", li_i18n::library::t("All Metadata…"), false)
            .on_hover_text(li_i18n::library::t("Every EXIF, GPS and XMP field in the file"))
            .clicked()
        {
            let _ = app.run("dialog.allMetadata", json!({}));
        }
    });
    // local-image: a layered document's layers (read-only)
    ui.add_space(6.0);
    crate::panels::doc_layers::show(app, ui, id);
}

/// The camera card at the top of Info: camera, lens, size · file size and format, then the
/// capture settings.
fn camera_card(ui: &mut egui::Ui, p: &lightcraft_catalog::Photo) {
    let t = Tokens::get(ui.ctx());
    let m = &p.meta;
    egui::Frame::NONE.fill(t.canvas).corner_radius(6.0).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let dim = |s: &str| egui::RichText::new(s.to_string()).size(12.0).color(t.text_label);
        if !m.camera.is_empty() {
            ui.label(dim(&m.camera));
        }
        if !m.lens.is_empty() {
            ui.add(egui::Label::new(dim(&m.lens)).truncate());
        }
        ui.horizontal(|ui| {
            ui.label(dim(&format!("{} × {}  ·  {}", p.width, p.height, human_size(p.file_size))));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::Frame::NONE.fill(t.chrome).corner_radius(3.0).inner_margin(egui::Margin::symmetric(5, 1)).show(ui, |ui| {
                    ui.label(egui::RichText::new(p.format.to_uppercase()).size(10.5).color(t.text_label));
                });
            });
        });
        let rows = [
            ("Focal length", m.focal_mm.map(|f| format!("{f:.1} mm").replace(".0 mm", " mm"))),
            ("Shutter speed", (!m.shutter.is_empty()).then(|| format!("{} sec", m.shutter))),
            ("Aperture", m.aperture.map(|f| format!("f / {f:.1}").replace(".0", ""))),
            ("ISO", m.iso.map(|i| i.to_string())),
        ];
        if rows.iter().any(|r| r.1.is_some()) {
            ui.add_space(6.0);
            egui::Grid::new("info-capture").num_columns(2).spacing([24.0, 3.0]).show(ui, |ui| {
                for (k, v) in rows {
                    ui.label(dim(k));
                    ui.label(dim(&v.unwrap_or_else(|| "—".into())));
                    ui.end_row();
                }
            });
        }
    });
}

/// Read-only capture fields use the shared disclosure and the installed theme's text styles.
fn camera_metadata(ui: &mut egui::Ui, meta: &lightcraft_catalog::Meta) {
    let t = Tokens::get(ui.ctx());
    let mut camera = Vec::new();
    if let Some(serial) = &meta.serial_number {
        camera.push(("Body Serial Number", serial.clone()));
    }
    if let Some(sony) = &meta.sony {
        for (label, value) in [
            ("Internal Serial Number", sony.internal_serial_number.clone()),
            ("Shutter Count", sony.shutter_count.map(|v| v.to_string())),
            ("Lens ID", sony.lens_id.map(|v| v.to_string())),
            ("Creative Style / Look", sony.creative_style.clone()),
            ("Picture Profile", sony.picture_profile.clone()),
            ("Focus Mode", sony.focus_mode.clone()),
            ("Focus Area", sony.focus_area.clone()),
            ("Flexible Spot Position", sony.flexible_spot.map(|p| format!("{}, {} (640 × 480)", p[0], p[1]))),
            ("Pixel Shift", sony.pixel_shift.map(|p| format!("{} / {} · {}", p.shot_number, p.shot_count, p.group_id))),
        ] {
            if let Some(value) = value {
                camera.push((label, value));
            }
        }
    }
    let mut drone = Vec::new();
    if let Some(dji) = &meta.dji {
        for (label, value, unit) in [
            ("Absolute Altitude", dji.absolute_altitude, " m"),
            ("Relative Altitude", dji.relative_altitude, " m"),
            ("Gimbal Yaw", dji.gimbal_yaw, "°"),
            ("Gimbal Pitch", dji.gimbal_pitch, "°"),
            ("Gimbal Roll", dji.gimbal_roll, "°"),
            ("Flight Yaw", dji.flight_yaw, "°"),
            ("Flight Pitch", dji.flight_pitch, "°"),
            ("Flight Roll", dji.flight_roll, "°"),
            ("Speed X", dji.speed_x, " m/s"),
            ("Speed Y", dji.speed_y, " m/s"),
            ("Speed Z", dji.speed_z, " m/s"),
        ] {
            if let Some(value) = value {
                drone.push((label, format!("{value:.3}{unit}")));
            }
        }
        if let (Some(lat), Some(lon)) = (dji.latitude, dji.longitude) {
            drone.push(("Drone GPS", format!("{lat:.6}, {lon:.6}")));
        }
    }
    for (id, title, rows) in [("infoCameraMetadata", "Camera Metadata", camera), ("infoDroneTelemetry", "Drone Telemetry", drone)] {
        if rows.is_empty() {
            continue;
        }
        let state = egui::Id::new(id);
        let mut open = ui.data(|d| d.get_temp::<bool>(state)).unwrap_or(false);
        if crate::widgets::disclosure(ui, id, li_i18n::library::t(title), open, ui.available_width(), false).clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_temp(state, open));
        }
        if open {
            for (label, value) in rows {
                ui.label(egui::RichText::new(li_i18n::library::t(label)).color(t.text_dim));
                let response = ui.label(egui::RichText::new(value).color(t.text_label));
                register(ui.ctx(), format!("metadata:{label}"), response.rect);
            }
        }
    }
}

fn human_size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{} KB", b >> 10),
        b => format!("{b} B"),
    }
}

/// A labelled metadata text field: the typed text lives in egui memory while focused and is
/// saved (photo.setMeta `key`) when the field loses focus.
/// Copyright Status: Unknown / Copyrighted / Public Domain (`xmpRights:Marked`).
fn copyright_status(app: &mut LightcraftApp, ui: &mut egui::Ui, current: lightcraft_catalog::CopyrightStatus) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(li_i18n::library::t("Copyright Status")).size(11.5).color(t.text_dim));
    let r = egui::ComboBox::from_id_salt("info-copyright-status").selected_text(li_i18n::library::t(current.label())).show_ui(ui, |ui| {
        for st in lightcraft_catalog::CopyrightStatus::ALL {
            if ui.selectable_label(st == current, li_i18n::library::t(st.label())).clicked() && st != current {
                let _ = app.run("photo.setMeta", json!({"copyrightStatus": st.id()}));
            }
        }
    });
    register(ui.ctx(), "field:copyrightStatus", r.response.rect);
    ui.add_space(6.0);
}

// A host modal may remove TextEdit focus even though the user did not leave the
// Library field. Retain that draft through dismissal, without manufacturing a
// commit. The same field must regain focus before ordinary blur can publish it.
// Capture the actual metadata target set and source value so resumed drafts can
// never migrate to another photo/catalog or overwrite a changed base value.
fn metadata_draft(app: &LightcraftApp, ui: &egui::Ui, id: egui::Id, value: &str) -> String {
    let owner = egui::Id::new((app.session.library_identity(), app.session.library_generation, app.session.active()))
        .with(&app.session.selection.ids)
        .with(value);
    ui.data_mut(|d| {
        let state = id.with("metadata-owner");
        if d.get_temp::<egui::Id>(state) != Some(owner) {
            d.remove::<String>(id);
            d.remove::<bool>(id.with("metadata-suspended"));
            d.insert_temp(state, owner);
        }
        d.get_temp::<String>(id).unwrap_or_else(|| value.to_owned())
    })
}

fn metadata_blur(app: &LightcraftApp, ui: &egui::Ui, id: egui::Id, response: &egui::Response, text: &str) -> bool {
    // Response focus access reads Context memory; do it before taking its data lock.
    let (has_focus, lost_focus) = (response.has_focus(), response.lost_focus());
    ui.data_mut(|d| {
        let suspended = id.with("metadata-suspended");
        if app.host_modal {
            if d.get_temp::<String>(id).is_some() {
                d.insert_temp(suspended, true);
            }
            return false;
        }
        if has_focus {
            d.insert_temp(id, text.to_owned());
            d.remove::<bool>(suspended);
            return false;
        }
        if d.get_temp::<bool>(suspended).unwrap_or(false) {
            return false;
        }
        d.remove::<String>(id);
        lost_focus
    })
}

fn meta_field(app: &mut LightcraftApp, ui: &mut egui::Ui, label: &str, key: &str, value: &str, lines: usize) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(li_i18n::library::t(label)).size(11.5).color(t.text_dim));
    let id = egui::Id::new(("info-field", key));
    let mut text = metadata_draft(app, ui, id, value);
    let edit = if lines > 1 { egui::TextEdit::multiline(&mut text).desired_rows(lines) } else { egui::TextEdit::singleline(&mut text) };
    let r = ui.add(edit.desired_width(f32::INFINITY));
    register(ui.ctx(), format!("field:{key}"), r.rect);
    if metadata_blur(app, ui, id, &r, &text) && text != value {
        let _ = app.run("photo.setMeta", json!({key: text}));
    }
    ui.add_space(6.0);
}

fn keywords(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let Some(p) = app.session.catalog.photo(id).cloned() else { return };
    header(ui, "Keywords");
    let t = Tokens::get(ui.ctx());
    padded(ui, |ui| {
        let kid = egui::Id::new("kw-input");
        let mut text = ui.data_mut(|d| d.get_temp::<String>(kid).unwrap_or_default());
        let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text(li_i18n::library::t("Add keyword")).desired_width(f32::INFINITY));
        register(ui.ctx(), "field:keyword", r.rect);
        if !app.host_modal && r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !text.trim().is_empty() {
            let kws: Vec<String> = text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            let _ = app.run("photo.setMeta", json!({"addKeywords": kws}));
            text.clear();
        }
        ui.data_mut(|d| d.insert_temp(kid, text));
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for k in &p.meta.keywords {
                let r =
                    ui.add(egui::Button::new(egui::RichText::new(format!("{}  ×", k.replace('|', " › "))).color(t.text_label)).corner_radius(10.0));
                register(ui.ctx(), format!("keywordChip:{k}"), r.rect);
                if r.clicked() {
                    let _ = app.run("photo.setMeta", json!({"removeKeywords": [k]}));
                }
                r.context_menu(|ui| {
                    if ui.button(li_i18n::library::t("Remove from Photo")).clicked() {
                        let _ = app.run("photo.setMeta", json!({"removeKeywords": [k]}));
                    }
                    if ui.button(li_i18n::library::t("Show Photos with Keyword")).clicked() {
                        let _ = app.run("library.filter", json!({"keyword": k}));
                    }
                    ui.separator();
                    if ui.button(li_i18n::library::t("Rename Keyword…")).clicked() {
                        app.ui.dialog = Some(crate::state::Dialog::RenameKeyword { from: k.clone(), to: k.clone() });
                    }
                    if ui.button(li_i18n::library::t("Delete Keyword")).clicked() {
                        let _ = app.run("keyword.delete", json!({"keyword": k}));
                    }
                });
            }
        });
        ui.add_space(10.0);
        keyword_set(app, ui, &p.meta.keywords);
        ui.add_space(8.0);
        // the painter: click photos in the grid to give them (or take away) a keyword
        ui.horizontal(|ui| {
            let pid = egui::Id::new("kw-painter");
            let painting = app.ui.keyword_painter.clone();
            let mut k: String = ui.data(|d| d.get_temp(pid)).unwrap_or_else(|| painting.clone().unwrap_or_default());
            let r = ui.add_enabled(
                painting.is_none(),
                egui::TextEdit::singleline(&mut k).hint_text(li_i18n::library::t("Keyword to paint")).desired_width(130.0),
            );
            register(ui.ctx(), "field:keywordPainter", r.rect);
            ui.data_mut(|d| d.insert_temp(pid, k.clone()));
            let label = if painting.is_some() { "Stop" } else { "Paint" };
            if text_button(ui, "keywordPaint", label, painting.is_some())
                .on_hover_text(li_i18n::library::t("Click photos in the grid to toggle the keyword; Esc stops"))
                .clicked()
            {
                let _ = app.run("tool.keywordPainter", json!({"keyword": if painting.is_some() { serde_json::Value::Null } else { json!(k) }}));
            }
        });
        ui.add_space(10.0);
        // suggestions: completions of the typed text, else keywords used together with this
        // photo's keywords, else the most used ones
        let typed = ui.data(|d| d.get_temp::<String>(kid).unwrap_or_default());
        let last = typed.rsplit(',').next().unwrap_or("").trim().to_string();
        let suggestions = (*app.caches.suggestions(&app.session.catalog, &p.meta.keywords, &last, 12)).clone();
        if !suggestions.is_empty() {
            ui.label(egui::RichText::new(li_i18n::library::t("Suggestions")).color(t.text_dim));
            ui.horizontal_wrapped(|ui| {
                for k in suggestions {
                    let r = ui
                        .add(egui::Button::new(egui::RichText::new(format!("+ {}", k.replace('|', " › "))).color(t.text_label)).corner_radius(10.0));
                    register(ui.ctx(), format!("kwSuggest:{k}"), r.rect);
                    if r.clicked() {
                        let _ = app.run("photo.setMeta", json!({"addKeywords": [k]}));
                        if !last.is_empty() {
                            ui.data_mut(|d| d.insert_temp(kid, String::new()));
                        }
                    }
                }
            });
        }
    });
}

/// The keyword set: pick a set, then nine buttons (⌥1–⌥9) that toggle its keywords on the
/// selected photos; "Save as Set…" keeps the current nine under a name.
fn keyword_set(app: &mut LightcraftApp, ui: &mut egui::Ui, have: &[String]) {
    let t = Tokens::get(ui.ctx());
    let sets = lightcraft_engine::cmd::keywords::keyword_sets_json(&app.session);
    let current = sets["current"].as_str().unwrap_or_default().to_string();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(li_i18n::library::t("Keyword Set")).color(t.text_dim));
        egui::ComboBox::from_id_salt("kw-set").selected_text(&current).show_ui(ui, |ui| {
            for set in sets["sets"].as_array().into_iter().flatten() {
                let name = set["name"].as_str().unwrap_or_default();
                if ui.selectable_label(name == current, name).clicked() {
                    let _ = app.run("keyword.useSet", json!({"name": name}));
                }
            }
            ui.separator();
            if ui.button(li_i18n::library::t("Save Current Keywords as Set…")).clicked() {
                app.ui.dialog = Some(crate::state::Dialog::TextPrompt {
                    title: "Save Keyword Set".into(),
                    hint: "Set name".into(),
                    value: String::new(),
                    command: "keyword.saveSet".into(),
                    params: json!({}),
                    key: "name".into(),
                });
            }
            if current != lightcraft_engine::cmd::keywords::RECENT
                && ui.button(li_i18n::tr_format!("Delete “{current}”", current = current)).clicked()
            {
                let _ = app.run("keyword.deleteSet", json!({"name": current}));
            }
        });
    });
    let kws: Vec<String> = sets["keywords"].as_array().into_iter().flatten().filter_map(|k| k.as_str().map(str::to_string)).collect();
    if kws.is_empty() {
        ui.label(egui::RichText::new(li_i18n::library::t("Keywords you add appear here; ⌥1–⌥9 apply them.")).color(t.text_dim));
        return;
    }
    let bw = ((ui.available_width() - 8.0) / 3.0).floor().max(40.0);
    egui::Grid::new("kw-set-grid").num_columns(3).spacing([4.0, 4.0]).show(ui, |ui| {
        for (i, k) in kws.iter().enumerate() {
            let on = have.iter().any(|x| x.eq_ignore_ascii_case(k));
            let short = k.rsplit('|').next().unwrap_or(k);
            let r = ui
                .add_sized(
                    [bw, 22.0],
                    egui::Button::new(egui::RichText::new(short).size(11.5).color(if on { t.text } else { t.text_label })).selected(on).truncate(),
                )
                .on_hover_text(format!("{} — ⌥{}", k.replace('|', " › "), i + 1));
            register(ui.ctx(), format!("kwSet:{}", i + 1), r.rect);
            if r.clicked() {
                let _ = app.run("keyword.toggleFromSet", json!({"index": i + 1}));
            }
            if i % 3 == 2 {
                ui.end_row();
            }
        }
    });
}

fn versions(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let Some(p) = app.session.catalog.photo(id).cloned() else { return };
    header(ui, "Versions");
    let t = Tokens::get(ui.ctx());
    padded(ui, |ui| {
        if text_button(ui, "versionCreate", li_i18n::library::t("Create Version"), false).clicked() {
            let _ = app.run("version.create", json!({}));
        }
        ui.add_space(8.0);
        // Named (made by you) and Auto (made by LightCraft) versions
        let tab_id = egui::Id::new("versions-tab");
        let mut auto: bool = ui.data(|d| d.get_temp(tab_id)).unwrap_or(false);
        let named_n = p.versions.iter().filter(|v| !v.auto).count();
        let items = [("Named", "named"), ("Auto", "auto")];
        let labels = [li_i18n::tr_format!("Named ({named_n})", named_n = named_n), li_i18n::tr_format!("Auto ({})", p.versions.len() - named_n)];
        let items: Vec<(&str, &str)> = items.iter().zip(&labels).map(|((_, k), l)| (l.as_str(), *k)).collect();
        if let Some(i) = crate::widgets::segmented(ui, "versionsTab", &items, Some(auto as usize), 2) {
            auto = i == 1;
            ui.data_mut(|d| d.insert_temp(tab_id, auto));
        }
        ui.add_space(6.0);
        let shown: Vec<(usize, &lightcraft_catalog::Version)> = p.versions.iter().enumerate().filter(|(_, v)| v.auto == auto).collect();
        if shown.is_empty() {
            let msg = if auto { "No automatic versions." } else { "No versions yet. Create one to keep this look." };
            ui.label(egui::RichText::new(li_i18n::library::t(msg)).color(t.text_dim));
        }
        // newest first
        for (i, v) in shown.into_iter().rev() {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
            register(ui.ctx(), format!("version:{i}"), r);
            let current = *v.settings == *p.develop;
            if resp.hovered() {
                ui.painter().rect_filled(r, 4.0, t.hover.gamma_multiply(0.7));
            }
            // thumbnail of the version's look
            let tr = Rect::from_min_size(r.min + vec2(4.0, 5.0), vec2(72.0, 48.0));
            ui.painter().rect_filled(tr, 2.0, t.canvas);
            if let Some(job) = app.session.variant_job(id, &v.settings, 160)
                && let Some(tex) = app.renderer.variant(job)
            {
                ui.painter().image(tex.tex.id(), tr, crate::panels::presets::cover_uv(tr, tex.size), egui::Color32::WHITE);
            }
            let x = tr.right() + 10.0;
            ui.painter().with_clip_rect(r.with_max_x(r.right() - 24.0)).text(
                pos2(x, r.top() + 18.0),
                Align2::LEFT_CENTER,
                &v.name,
                t.semibold(13.0),
                t.text,
            );
            let painter = ui.painter().with_clip_rect(r);
            painter.text(pos2(x, r.top() + 38.0), Align2::LEFT_CENTER, short_time(&v.created), t.font(11.0), t.text_dim);
            if current {
                paint(ui.painter(), Rect::from_center_size(pos2(r.right() - 14.0, r.center().y), vec2(14.0, 14.0)), Icon::Check, t.accent);
            }
            // resting on a version shows it in the loupe (nothing is changed)
            if resp.hovered() && !current {
                app.hover_preview = Some(crate::HoverPreview { label: li_i18n::tr_format!("Version: {}", v.name), settings: (*v.settings).clone() });
            }
            if resp.double_clicked() {
                crate::panels::dialogs::prompt(app, "Rename Version", "Version name", &v.name, "version.rename", json!({"index": i}), "name");
            } else if resp.clicked() && !current {
                let _ = app.run("version.restore", json!({"index": i}));
            }
            resp.on_hover_text(if current { "The photo has these settings" } else { "Click to restore · double-click to rename" }).context_menu(
                |ui| {
                    if ui.button(li_i18n::library::t("Restore")).clicked() {
                        let _ = app.run("version.restore", json!({"index": i}));
                    }
                    if ui.button(li_i18n::library::t("Update with Current Settings")).clicked() {
                        let _ = app.run("version.update", json!({"index": i}));
                    }
                    if ui.button(li_i18n::library::t("Rename…")).clicked() {
                        crate::panels::dialogs::prompt(app, "Rename Version", "Version name", &v.name, "version.rename", json!({"index": i}), "name");
                    }
                    if ui.button(li_i18n::library::t("Set as Before")).clicked() {
                        let _ = app.run("beforeAfter.setBefore", json!({"source": "version", "name": v.name}));
                    }
                    ui.separator();
                    if ui.button(li_i18n::library::t("Delete")).clicked() {
                        let _ = app.run("version.delete", json!({"index": i}));
                    }
                },
            );
        }
    });
}

/// "Sep 30, 2026, 12:00 PM" from an ISO time.
fn short_time(iso: &str) -> String {
    // Other languages use their own date patterns (`library_labels::display_time`).
    if li_i18n::current() != li_i18n::Lang::En {
        return crate::library_labels::display_time(iso);
    }
    let long = lightcraft_catalog::dates::display_time(iso);
    // "September 30, 2026 at 12:00:00 PM" → month abbreviated, seconds dropped
    let (date, time) = long.split_once(" at ").map_or((long.as_str(), None), |(d, t)| (d, Some(t)));
    let date = match date.split_once(' ') {
        Some((m, rest)) => format!("{} {rest}", m.chars().take(3).collect::<String>()),
        None => date.to_string(),
    };
    match time.and_then(|t| t.rsplit_once(' ')) {
        Some((hms, ampm)) => format!("{date}, {} {ampm}", hms.rsplit_once(':').map_or(hms, |x| x.0)),
        None => date,
    }
}

fn activity(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let Some(p) = app.session.catalog.photo(id).cloned() else { return };
    header(ui, "History");
    let t = Tokens::get(ui.ctx());
    padded(ui, |ui| {
        if p.history.is_empty() {
            ui.label(egui::RichText::new(li_i18n::library::t("No edits yet.")).color(t.text_dim));
        }
        for (i, h) in p.history.iter().enumerate().rev() {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
            register(ui.ctx(), format!("history:{i}"), r);
            if resp.hovered() {
                ui.painter().rect_filled(r, 3.0, t.hover);
            }
            paint(ui.painter(), Rect::from_min_size(r.min + vec2(0.0, 4.0), vec2(16.0, 16.0)), Icon::Clock, t.icon);
            ui.painter().text(
                pos2(r.left() + 24.0, r.center().y),
                Align2::LEFT_CENTER,
                crate::library_labels::history_label(&h.label, &app.session.presets),
                t.font(12.5),
                t.text_label,
            );
            if resp.clicked() {
                let _ = app.run("history.restore", json!({"index": i}));
            }
            resp.context_menu(|ui| {
                if ui.button(li_i18n::library::t("Copy History Step Settings to Before")).clicked() {
                    let _ = app.run("beforeAfter.setBefore", json!({"source": "history", "index": i}));
                }
                if ui.button(li_i18n::library::t("Create Version from Step")).clicked() {
                    let _ = app.run("history.restore", json!({"index": i}));
                    let _ = app.run("version.create", json!({"name": h.label}));
                }
                ui.separator();
                if ui.button(li_i18n::library::t("Clear History")).clicked() {
                    let _ = app.run("history.clear", json!({}));
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn short_times() {
        assert_eq!(super::short_time("2026-09-30T12:00:05"), "Sep 30, 2026, 12:00 PM");
        assert_eq!(super::short_time("2026-01-02"), "Jan 2, 2026");
    }
}

#[cfg(test)]
mod shared_shell_metadata_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn shared_shell_suspended_metadata_draft_cannot_change_owner_or_base() {
        let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), crate::Services::default());
        let ctx = egui::Context::default();
        let id = egui::Id::new(("info-field", "title"));
        let original = app.session.active().unwrap();
        let other = app.session.catalog.photos().find(|photo| photo.id != original).unwrap().id;
        let undo = app.session.undo.len();
        for change in 0..4 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                assert_eq!(metadata_draft(&app, ui, id, "base"), "base");
                ui.data_mut(|d| {
                    d.insert_temp(id, "retained".to_owned());
                    d.insert_temp(id.with("metadata-suspended"), true);
                });
            })
            .textures_delta
            .clear();
            match change {
                0 => app.session.library_generation += 1,
                1 => app.session.selection.active = Some(other),
                2 => app.session.selection.ids = vec![other],
                _ => {}
            }
            let base = if change == 3 { "external edit" } else { "base" };
            ctx.run_ui(egui::RawInput::default(), |ui| {
                assert_eq!(metadata_draft(&app, ui, id, base), base);
                assert!(ui.data(|d| d.get_temp::<String>(id)).is_none());
                assert!(ui.data(|d| d.get_temp::<bool>(id.with("metadata-suspended"))).is_none());
            })
            .textures_delta
            .clear();
        }
        assert_eq!(app.session.undo.len(), undo);
    }
}
