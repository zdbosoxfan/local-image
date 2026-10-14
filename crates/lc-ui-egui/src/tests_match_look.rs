//! Real-click acceptance, screenshots and app-path CPU-job/GPU-preview measurements.
use crate::{
    LightcraftApp, Services,
    headless::Headless,
    state::{RightPanel, ViewMode},
    theme::Tokens,
};
use lightcraft_catalog::{MemStore, PhotoId};
use lightcraft_engine::cmd::match_look as engine;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
const T: Duration = Duration::from_secs(60);
fn request(h: &mut Headless, id: &str, p: Value) -> Value {
    let r = if id.starts_with("ui.") { h.request(id, p, T) } else { h.request("engine.execute", json!({"command":id,"params":p}), T) };
    assert_eq!(r["ok"], true, "{id}: {r}");
    h.step();
    r["result"].clone()
}
fn click(h: &mut Headless, id: &str) {
    h.step();
    h.step();
    request(h, "ui.clickWidget", json!({"id":id}));
    while !h.app.synthetic.is_empty() {
        h.step();
    }
}
fn rect(h: &Headless, id: &str) -> egui::Rect {
    h.app.widgets.iter().rev().find(|(n, _)| n == id).unwrap_or_else(|| panic!("missing {id}")).1
}
fn wait(h: &mut Headless) {
    while !h.app.synthetic.is_empty() {
        h.step();
    }
    assert!(crate::tasks::wait(&mut h.app, &h.view.ctx, T));
    h.step();
    h.step();
    assert!(h.app.match_look.error.is_empty(), "{}", h.app.match_look.error);
}
fn setup(width: f32, dark: bool, ppp: f32) -> Headless {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services::default());
    app.ui.settings.gpu = false;
    app.ui.view = ViewMode::Detail;
    app.ui.right = RightPanel::Edit;
    app.ui.left_panel = false;
    app.ui.filmstrip = false;
    app.ui.histogram = false;
    app.ui.right_width = width;
    app.ui.open_sections = vec!["matchLook".into()];
    app.set_host_theme(if dark { Tokens::default() } else { super::tests_develop_c::light() });
    let mut h = Headless::new(app, [1200.0, 1200.0], ppp);
    h.step();
    h.step();
    assert!(h.settle(T));
    h
}
fn analyze(h: &mut Headless, count: usize) {
    let ids = h.app.session.catalog.photos().take(count).map(|p| p.id.0).collect::<Vec<_>>();
    request(h, "library.select", json!({"ids":ids}));
    click(h, "button:matchLookAdd");
    assert_eq!(h.app.session.match_look.references.len(), count);
    click(h, "button:matchLook");
    assert!(h.app.ui.dialog.is_none(), "Analyze gates matching");
    click(h, "button:matchLookAnalyze");
    assert!(h.app.tasks.is_running("Analyze Match Look"));
    wait(h);
    assert!(engine::current_ready(&h.app.session));
}
fn target(h: &mut Headless) -> PhotoId {
    let id = h.app.session.catalog.photos().find(|p| !h.app.session.match_look.references.contains(&p.id)).unwrap().id;
    request(h, "library.select", json!({"ids":[id.0]}));
    id
}
#[test]
fn match_look_ui_analyze_preview_toggles_strength_layer_apply_undo() {
    for ppp in [1.0, 1.5] {
        let mut h = setup(250.0, true, ppp);
        analyze(&mut h, 2);
        let id = target(&mut h);
        let before = h.app.session.develop_of(id).unwrap();
        let undo = h.app.session.undo.len();
        let revision = h.app.session.catalog.revision;
        click(&mut h, "button:matchLook");
        assert!(h.app.ui.dialog.is_some());
        click(&mut h, "check:matchNormalize");
        click(&mut h, "check:matchNormalize");
        click(&mut h, "check:matchTone");
        click(&mut h, "check:matchColor");
        click(&mut h, "check:matchLight");
        click(&mut h, "check:matchLight");
        click(&mut h, "check:matchColor");
        click(&mut h, "check:matchNewLayer");
        click(&mut h, "check:matchConsistent");
        request(&mut h, "ui.dragWidget", json!({"id":"slider:matchLook.strength","dx":-35.0,"steps":5}));
        let options = match h.app.ui.dialog.as_ref().unwrap() {
            crate::state::Dialog::MatchLook { options } => *options,
            _ => panic!("wrong dialog"),
        };
        assert!(options.strength < 100.0 && options.strength > 0.0);
        assert!(options.new_layer && options.consistent_batch);
        assert!(!options.color_tone);

        click(&mut h, "button:matchLookPreview");
        wait(&mut h);
        assert!(crate::match_look::preview_ready(&h.app, options));
        assert!(rect(&h, "matchLook:preview").width() > 100.0);
        assert_eq!(h.app.session.catalog.revision, revision, "Preview is nondestructive");
        click(&mut h, "button:dialogOk");
        wait(&mut h);
        assert!(h.app.ui.dialog.is_none());
        assert_eq!(h.app.session.undo.len(), undo + 1);
        let after = h.app.session.develop_of(id).unwrap();
        assert_eq!(after.masks.len(), before.masks.len() + 1);
        assert_eq!(after.light, before.light);
        request(&mut h, "ui.key", json!({"key":"z","cmd":true}));
        assert_eq!(h.app.session.develop_of(id).unwrap(), before);
    }
}
#[test]
fn match_look_ui_saved_picker_reload_and_stale_analysis() {
    let store = MemStore::new();
    let mut h = setup(400.0, false, 1.0);
    h.app.session.saved_looks.open(Box::new(store.clone())).unwrap();
    analyze(&mut h, 10);
    click(&mut h, "button:matchLookSave");
    click(&mut h, "field:dialogText");
    request(&mut h, "ui.text", json!({"text":"Exteriors"}));
    click(&mut h, "button:dialogOk");
    assert_eq!(h.app.session.saved_looks.file.looks[0].name, "Exteriors");
    let id = target(&mut h);
    click(&mut h, "dropdown:matchLookPicker");
    click(&mut h, "button:matchLookPick-0");
    assert_eq!(h.app.session.match_look.selected, None);
    let reference = h.app.session.match_look.references[0];
    request(&mut h, "develop.set", json!({"ids":[reference.0],"control":"light.contrast","value":37.0}));
    assert!(!engine::current_ready(&h.app.session));
    click(&mut h, "button:matchLook");
    assert!(h.app.ui.dialog.is_none());
    click(&mut h, "dropdown:matchLookPicker");
    click(&mut h, "button:matchLookPick-1");
    assert!(engine::selected_looks(&h.app.session).is_some());
    click(&mut h, "button:matchLook");
    click(&mut h, "button:matchLookPreview");
    wait(&mut h);
    click(&mut h, "button:dialogOk");
    wait(&mut h);
    let saved = h.app.session.develop_of(id).unwrap();
    assert!(h.app.session.smart.tagger.is_none());
    assert!(h.app.session.smart.people.tagger.is_none());
    drop(h);
    let mut h = setup(400.0, false, 1.0);
    h.app.session.saved_looks.open(Box::new(store)).unwrap();
    request(&mut h, "library.select", json!({"ids":[id.0]}));
    click(&mut h, "dropdown:matchLookPicker");
    click(&mut h, "button:matchLookPick-1");
    click(&mut h, "button:matchLook");
    click(&mut h, "button:matchLookPreview");
    wait(&mut h);
    click(&mut h, "button:dialogOk");
    wait(&mut h);
    assert_eq!(h.app.session.develop_of(id).unwrap(), saved, "Reloading the saved look gives the same sliders");
}
#[test]
fn match_look_ui_individual_adjustments_group_masters_and_no_op() {
    for width in [250.0, 400.0] {
        let mut h = setup(width, true, 1.0);
        analyze(&mut h, 2);
        let id = target(&mut h);
        let before = h.app.session.develop_of(id).unwrap();
        let revision = h.app.session.catalog.revision;
        let undo = h.app.session.undo.len();
        click(&mut h, "button:matchLook");
        for (group, tools) in [
            ("matchNormalize", vec!["matchExposure", "matchWhiteBalance"]),
            ("matchLight", vec!["matchContrast", "matchHighlights", "matchShadows", "matchWhites", "matchBlacks", "matchLumaCurve"]),
            ("matchColor", vec!["matchSaturation", "matchVibrance", "matchColorEditor", "matchBlackWhite"]),
            (
                "matchTone",
                vec![
                    "matchMasterBalance",
                    "matchShadowsBalance",
                    "matchMidtonesBalance",
                    "matchHighlightsBalance",
                    "matchRedCurve",
                    "matchGreenCurve",
                    "matchBlueCurve",
                ],
            ),
        ] {
            click(&mut h, &format!("icon:{group}Tools"));
            let window = rect(&h, "dialog:window");
            for tool in &tools {
                let r = rect(&h, &format!("check:{tool}"));
                assert!(r.left() >= window.left() && r.right() <= window.right(), "{tool} overflowed {window:?}: {r:?}");
                click(&mut h, &format!("check:{tool}"));
            }
            click(&mut h, &format!("icon:{group}Tools"));
        }
        let options = match h.app.ui.dialog.as_ref().unwrap() {
            crate::state::Dialog::MatchLook { options } => *options,
            _ => panic!("wrong dialog"),
        };
        assert!(!crate::match_look::any_adjustment_enabled(options));
        // Masters retain individual exclusions; disabled children cannot re-enable a tool.
        click(&mut h, "check:matchLight");
        click(&mut h, "icon:matchLightTools");
        click(&mut h, "check:matchContrast");
        click(&mut h, "icon:matchLightTools");
        click(&mut h, "check:matchLight");
        let remembered = match h.app.ui.dialog.as_ref().unwrap() {
            crate::state::Dialog::MatchLook { options } => *options,
            _ => panic!("wrong dialog"),
        };
        assert_eq!(options, remembered);
        click(&mut h, "button:matchLookPreview");
        assert!(h.app.match_look.control.is_none());
        assert!(h.app.match_look.preview.is_none());
        click(&mut h, "button:dialogOk");
        wait(&mut h);
        assert_eq!(h.app.session.catalog.revision, revision, "No enabled adjustments must not write to the catalog");
        assert_eq!(h.app.session.undo.len(), undo);
        assert_eq!(h.app.session.develop_of(id).unwrap(), before);
    }
}

fn narrow_body(h: &mut Headless, width: f32) -> Vec<(String, egui::Rect)> {
    let mut options = h.app.session.match_look.options;
    crate::widgets::take_registry(&h.view.ctx);
    let time = h.view.ctx.input(|input| input.time) + 1.0;
    h.view.run(crate::headless::HeadlessView::raw_input(egui::vec2(width, 1800.0), 1.0, time, vec![]), |ui| {
        ui.set_max_width(width);
        ui.painter().rect_filled(ui.max_rect(), 0.0, Tokens::get(ui.ctx()).chrome);
        crate::match_look::body(&mut h.app, ui, &mut options);
    });
    crate::widgets::take_registry(&h.view.ctx)
}
#[test]
fn match_look_ui_expanded_adjustments_fit_actual_narrow_content_width() {
    for width in [250.0, 400.0] {
        let mut h = setup(width, true, 1.0);
        analyze(&mut h, 2);
        target(&mut h);
        click(&mut h, "button:matchLook");
        for group in ["matchNormalize", "matchLight", "matchColor", "matchTone"] {
            click(&mut h, &format!("icon:{group}Tools"));
        }
        let controls = narrow_body(&mut h, width);
        let adjustment_controls = controls.iter().filter(|(id, _)| id.starts_with("check:match") || id.ends_with("Tools")).collect::<Vec<_>>();
        assert!(adjustment_controls.len() >= 27, "All expanded adjustment controls must render");
        for (id, rect) in adjustment_controls {
            assert!(rect.left() >= 0.0 && rect.right() <= width + 0.1, "{id} must fit {width}px: {rect:?}");
        }
    }
}

#[test]
#[ignore = "explicit visual QA: all individual adjustment controls at actual 250/400px content widths"]
fn match_look_ui_expanded_adjustment_screenshots() {
    for dark in [true, false] {
        for width in [250.0, 400.0] {
            let mut h = setup(width, dark, 1.0);
            analyze(&mut h, 2);
            target(&mut h);
            click(&mut h, "button:matchLook");
            for group in ["matchNormalize", "matchLight", "matchColor", "matchTone"] {
                click(&mut h, &format!("icon:{group}Tools"));
            }
            let controls = narrow_body(&mut h, width);
            let bottom = controls.iter().map(|(_, rect)| rect.bottom()).fold(0.0, f32::max);
            save(
                &h,
                &format!("tools-{}-{}", if dark { "dark" } else { "light" }, width as u32),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(width, bottom + 8.0)),
            );
        }
    }
}

#[test]
fn match_look_ui_saved_previews_options_and_layer_survive_without_source_catalog() {
    let store = MemStore::new();
    let mut h = setup(250.0, false, 1.0);
    h.app.session.saved_looks.open(Box::new(store.clone())).unwrap();
    analyze(&mut h, 2);
    let reference_ids = h.app.session.match_look.references.clone();
    let target_id = target(&mut h);
    click(&mut h, "button:matchLook");
    click(&mut h, "icon:matchNormalizeTools");
    click(&mut h, "check:matchWhiteBalance");
    click(&mut h, "icon:matchNormalizeTools");
    click(&mut h, "icon:matchToneTools");
    click(&mut h, "check:matchRedCurve");
    click(&mut h, "icon:matchToneTools");
    click(&mut h, "check:matchNewLayer");
    request(&mut h, "ui.dragWidget", json!({"id":"slider:matchLook.strength","dx":-28.0,"steps":4}));
    let expected = match h.app.ui.dialog.as_ref().unwrap() {
        crate::state::Dialog::MatchLook { options } => *options,
        _ => panic!("wrong dialog"),
    };
    assert!(expected.new_layer && expected.strength < 100.0);
    click(&mut h, "button:dialogCancel");
    click(&mut h, "button:matchLookSave");
    click(&mut h, "field:dialogText");
    request(&mut h, "ui.text", json!({"text":"Portable exterior"}));
    click(&mut h, "button:dialogOk");
    let saved = h.app.session.saved_looks.file.looks[0].clone();
    assert_eq!(saved.options, Some(expected));
    assert_eq!(saved.previews.len(), 2);
    assert!(saved.previews.iter().all(|preview| preview.image().is_some()));
    drop(h);

    let mut h = setup(250.0, false, 1.0);
    h.app.session.saved_looks.open(Box::new(store)).unwrap();
    for id in &reference_ids {
        h.app.session.catalog.apply(lightcraft_catalog::Op::RemovePhoto { id: *id }).unwrap();
    }
    request(&mut h, "library.select", json!({"ids":[target_id.0]}));
    let before = h.app.session.develop_of(target_id).unwrap();
    click(&mut h, "dropdown:matchLookPicker");
    click(&mut h, "button:matchLookPick-1");
    assert!(h.app.session.match_look.references.is_empty());
    assert_eq!(h.app.session.match_look.options, expected);
    assert!(h.app.widgets.iter().any(|(id, _)| id == "reference:0"));
    assert!(!h.app.widgets.iter().any(|(id, _)| id.starts_with("icon:matchReferenceRemove-")), "Saved previews cannot remove current references");
    let images = crate::match_look::preview_pixels(&h.app);
    assert_eq!(images.len(), 2, "Saved thumbnails decode without the source catalog");
    h.step();
    h.step();
    assert_eq!(images, crate::match_look::preview_pixels(&h.app), "Stable saved references must reuse GPU textures");
    click(&mut h, "button:matchLook");
    click(&mut h, "button:matchLookPreview");
    wait(&mut h);
    click(&mut h, "button:matchCompare-2");
    assert!(h.app.match_look.compare_reference);
    assert_eq!(h.app.match_look.reference_index, 1);
    assert!(crate::match_look::preview_ready(&h.app, expected));
    assert_eq!(crate::match_look::preview_pixels(&h.app).len(), 4);
    click(&mut h, "button:dialogOk");
    wait(&mut h);
    let applied = h.app.session.develop_of(target_id).unwrap();
    assert_eq!(applied.masks.len(), before.masks.len() + 1);
    assert_eq!(applied.wb, before.wb, "The saved WB exclusion is preserved on layers");
    assert_eq!(applied.curve.red, before.curve.red);
    let layer = applied.masks.last().unwrap();
    assert_eq!(layer.adjust.temp, 0.0);
    assert_eq!(layer.adjust.tint, 0.0);
    assert!(layer.tools.curve.as_ref().is_none_or(|curve| curve.red.is_empty()), "The excluded RGB curve remains neutral in the Match Look layer");
    request(&mut h, "ui.dragWidget", json!({"id":"slider:matchLook.photoStrength","dx":-20.0,"steps":4}));
    assert!(h.app.session.match_look.applied[&target_id].options.strength < expected.strength);
    assert!(!h.app.session.match_look.applied[&target_id].options.adjustments.white_balance);
    assert!(!h.app.session.match_look.applied[&target_id].options.adjustments.red_curve);
    // Reapply into the owned layer while preserving a later, unrelated user mask.
    request(&mut h, "mask.add", json!({"kind":"linear","name":"User gradient"}));
    request(&mut h, "mask.select", json!({"id":null}));
    let user_mask = h.app.session.develop_of(target_id).unwrap().masks.last().unwrap().clone();
    click(&mut h, "button:matchLook");
    click(&mut h, "button:matchLookPreview");
    wait(&mut h);
    assert!(
        h.app.widgets.iter().any(|(id, _)| id.starts_with("button:matchPreviewReset-")),
        "Replacing an owned layer must preview the real changes against its neutral template"
    );
    click(&mut h, "button:dialogOk");
    wait(&mut h);
    let reapplied = h.app.session.develop_of(target_id).unwrap();
    assert_eq!(reapplied.masks.len(), before.masks.len() + 2, "Reapplication must reuse the Match Look layer");
    assert_eq!(reapplied.masks.last(), Some(&user_mask));
    assert_eq!(reapplied.masks[before.masks.len()].id, layer.id);
    // Save an adjusted recipe from the portable look while every original reference is gone.
    click(&mut h, "button:matchLook");
    click(&mut h, "check:matchSkin");
    let modified = match h.app.ui.dialog.as_ref().unwrap() {
        crate::state::Dialog::MatchLook { options } => *options,
        _ => panic!("wrong dialog"),
    };
    click(&mut h, "button:dialogCancel");
    click(&mut h, "button:matchLookSave");
    assert!(
        matches!(h.app.ui.dialog, Some(crate::state::Dialog::TextPrompt { .. })),
        "A portable saved recipe can be saved without original references"
    );
    click(&mut h, "field:dialogText");
    request(&mut h, "ui.text", json!({"text":"Portable exterior without skin protection"}));
    click(&mut h, "button:dialogOk");
    let resaved = h.app.session.saved_looks.file.looks.last().unwrap();
    assert_eq!(resaved.options, Some(modified));
    assert_eq!(resaved.previews, saved.previews);
    assert_eq!(resaved.looks, saved.looks);
}

#[test]
fn match_look_ui_external_reference_picker_analyze_pin_and_remove_without_import() {
    let mut h = setup(250.0, true, 1.0);
    let count = h.app.session.catalog.photos().count();
    let dir = std::env::temp_dir()
        .join(format!("lc-match-reference-ui-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("finished-exterior.png");
    let image = lightcraft_raster::Rgba8::from_fn(96, 64, |x, y| [(80 + x) as u8, (100 + y) as u8, (140 + x / 2) as u8, 255]);
    std::fs::write(&path, lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&image), &Default::default()).unwrap()).unwrap();
    let picked = path.to_string_lossy().into_owned();
    h.app.services.pick_files = Some(Box::new(move || vec![picked.clone()]));
    click(&mut h, "button:matchLookAddFiles");
    assert_eq!(h.app.session.catalog.photos().count(), count, "Finished reference files must not be imported as target photos");
    assert!(h.app.session.match_look.references.is_empty());
    assert_eq!(h.app.session.match_look.external_references.len(), 1);
    assert!(crate::menus::ui_enabled(&h.app, "develop.analyzeLook"), "External-only references must enable Analyze from the menu too");
    let reference = h.app.session.match_look.external_references[0].id;
    assert!(
        h.view.shapes().iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("finished-exterior.png"))),
        "An external reference shows its filename while its image awaits analysis"
    );
    click(&mut h, &format!("icon:matchReferencePin-{reference}"));
    assert_eq!(h.app.session.match_look.external_references[0].importance, 2.0);
    click(&mut h, "button:matchLookAnalyze");
    assert!(!crate::menus::ui_enabled(&h.app, "develop.analyzeLook"), "Analyze stays disabled while its reference job runs");
    wait(&mut h);
    assert!(engine::current_ready(&h.app.session));
    assert_eq!(h.app.session.match_look.analysis.as_ref().unwrap().previews.len(), 1);
    click(&mut h, &format!("reference:{reference}"));
    assert!(h.app.match_look.compare_reference);
    let id = target(&mut h);
    click(&mut h, "button:matchLook");
    click(&mut h, "button:matchLookPreview");
    wait(&mut h);
    click(&mut h, "button:matchCompare-1");
    assert!(h.app.match_look.compare_reference);
    assert_eq!(h.app.session.active(), Some(id));
    click(&mut h, "button:dialogCancel");
    click(&mut h, &format!("icon:matchReferenceRemove-{reference}"));
    assert!(h.app.session.match_look.external_references.is_empty());
    assert!(!engine::current_ready(&h.app.session));
    assert_eq!(h.app.session.catalog.photos().count(), count);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn match_look_ui_consistent_preview_uses_full_batch_and_rejects_stale_other_targets() {
    let mut h = setup(400.0, true, 1.0);
    analyze(&mut h, 2);
    let ids = h
        .app
        .session
        .catalog
        .photos()
        .filter(|photo| !h.app.session.match_look.references.contains(&photo.id))
        .take(3)
        .map(|photo| photo.id)
        .collect::<Vec<_>>();
    request(&mut h, "library.select", json!({"ids":ids}));
    let active = h.app.session.active().unwrap();
    let other = *ids.iter().find(|id| **id != active).unwrap();
    let before = ids.iter().map(|id| h.app.session.develop_of(*id).unwrap()).collect::<Vec<_>>();
    let revision = h.app.session.catalog.revision;
    let undo = h.app.session.undo.len();
    click(&mut h, "button:matchLook");
    click(&mut h, "check:matchConsistent");
    let options = match h.app.ui.dialog.as_ref().unwrap() {
        crate::state::Dialog::MatchLook { options } => *options,
        _ => panic!("wrong dialog"),
    };
    click(&mut h, "button:matchLookPreview");
    assert_eq!(
        h.app.match_look.control.as_ref().unwrap().total,
        ids.len() + 1,
        "Consistent Preview fits the complete batch then renders the active target"
    );
    wait(&mut h);
    assert!(crate::match_look::preview_ready(&h.app, options));
    assert_eq!(h.app.session.catalog.revision, revision);
    assert_eq!(h.app.session.undo.len(), undo);
    assert_eq!(ids.iter().map(|id| h.app.session.develop_of(*id).unwrap()).collect::<Vec<_>>(), before);
    request(&mut h, "develop.set", json!({"ids":[other.0],"control":"light.contrast","value":37.0}));
    assert_eq!(h.app.session.active(), Some(active));
    assert!(!crate::match_look::preview_ready(&h.app, options), "A shared-style preview depends on the other batch targets too");
    click(&mut h, "button:matchLookPreview");
    wait(&mut h);
    assert!(crate::match_look::preview_ready(&h.app, options));
    request(&mut h, "library.select", json!({"ids":[active.0]}));
    assert!(!crate::match_look::preview_ready(&h.app, options), "Changing batch membership requires a fresh consistent Preview");
}

#[test]
fn match_look_ui_cancel_analyze_and_target_batch() {
    let mut h = setup(400.0, true, 1.0);
    let ids = h.app.session.catalog.photos().take(10).map(|p| p.id.0).collect::<Vec<_>>();
    request(&mut h, "library.select", json!({"ids":ids}));
    click(&mut h, "button:matchLookAdd");
    click(&mut h, "button:matchLookAnalyze");
    click(&mut h, "button:matchLookCancel");
    wait(&mut h);
    assert!(!engine::current_ready(&h.app.session));
    click(&mut h, "button:matchLookAnalyze");
    wait(&mut h);
    target(&mut h);
    // Start a sufficiently large batch to click Cancel before completion.
    let ids = h.app.session.catalog.photos().map(|p| p.id.0).collect::<Vec<_>>();
    request(&mut h, "library.select", json!({"ids":ids}));
    let revision = h.app.session.catalog.revision;
    request(&mut h, "develop.matchLook", json!({}));
    click(&mut h, "button:matchLookCancel");
    wait(&mut h);
    assert_eq!(h.app.session.catalog.revision, revision, "Cancelled batch adds no undo and changes no photos");
}
fn save(h: &Headless, name: &str, r: egui::Rect) {
    let image = h.paint();
    let ppp = h.pixels_per_point;
    let x0 = (r.left() * ppp).max(0.0) as usize;
    let y0 = (r.top() * ppp).max(0.0) as usize;
    let x1 = ((r.right() * ppp).ceil() as usize).min(image.size[0]);
    let y1 = ((r.bottom() * ppp).ceil() as usize).min(image.size[1]);
    let out = lightcraft_raster::Rgba8::from_fn(x1 - x0, y1 - y0, |x, y| image.pixels[(y + y0) * image.size[0] + x + x0].to_array());
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/matchlook-ux");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{name}.png")),
        lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&out), &Default::default()).unwrap(),
    )
    .unwrap();
}
#[test]
#[ignore = "explicit visual QA: renders panel/dialog in dark/light at both requested widths"]
fn match_look_ui_screenshots() {
    for dark in [true, false] {
        for width in [250.0, 400.0] {
            let mut h = setup(width, dark, 1.0);
            analyze(&mut h, 10);
            target(&mut h);
            click(&mut h, "button:matchLookSave");
            click(&mut h, "field:dialogText");
            request(&mut h, "ui.text", json!({"text":"Exteriors"}));
            click(&mut h, "button:dialogOk");
            assert!(h.settle(T));
            let tag = format!("{}-{}", if dark { "dark" } else { "light" }, width as u32);
            let panel = rect(&h, "panel:right_panel");
            let start = rect(&h, "section:matchLook").top();
            let end = rect(&h, "button:matchLook").bottom();
            let sections = h.app.ui.open_sections.clone();
            h.app.ui.open_sections = vec!["light".into(), "color".into(), "effects".into()];
            h.step();
            h.step();
            save(&h, &format!("develop-{tag}"), panel);
            h.app.ui.open_sections = sections;
            h.step();
            h.step();
            save(&h, &format!("panel-{tag}"), egui::Rect::from_min_max(egui::pos2(panel.left(), start), egui::pos2(panel.right(), end + 8.0)));
            click(&mut h, "icon:matchLookMenu");
            save(&h, &format!("menu-{tag}"), egui::Rect::from_min_max(egui::pos2(panel.left(), start), egui::pos2(panel.right(), end + 80.0)));
            request(&mut h, "ui.key", json!({"key":"Escape"}));
            click(&mut h, "button:matchLook");
            assert!(h.settle(T));
            click(&mut h, "button:matchLookPreview");
            wait(&mut h);
            assert!(h.app.widgets.iter().any(|(id, _)| id == "matchLook:preview"), "Preview must be visible in screenshots");
            save(&h, &format!("dialog-{tag}"), rect(&h, "dialog:window"));
            click(&mut h, "button:matchCompare-1");
            save(&h, &format!("reference-{tag}"), rect(&h, "dialog:window"));
            request(&mut h, "ui.hoverWidget", json!({"id":"matchLook:preview"}));
            request(&mut h, "ui.scroll", json!({"dy":-1200.0}));
            h.step();
            h.step();
            save(&h, &format!("summary-{tag}"), rect(&h, "dialog:window"));
            click(&mut h, "button:dialogOk");
            wait(&mut h);
            let last =
                h.app.widgets.iter().filter(|(id, _)| id.starts_with("button:matchAppliedReset-")).map(|(_, r)| r.bottom()).fold(end, f32::max);
            save(&h, &format!("applied-{tag}"), egui::Rect::from_min_max(egui::pos2(panel.left(), start), egui::pos2(panel.right(), last + 8.0)));
            click(&mut h, "dropdown:matchLookPicker");
            click(&mut h, "button:matchLookPick-0");
            let reference = h.app.session.match_look.references[0];
            request(&mut h, "develop.set", json!({"ids":[reference.0],"control":"light.contrast","value":37.0}));
            assert!(!engine::current_ready(&h.app.session));
            save(
                &h,
                &format!("stale-{tag}"),
                egui::Rect::from_min_max(egui::pos2(panel.left(), start), egui::pos2(panel.right(), rect(&h, "button:matchLook").bottom() + 8.0)),
            );
        }
    }
}
#[test]
#[ignore = "500-photo app-path performance acceptance; run separately with low-end profile override"]
fn match_look_ui_500_photo_app_path_benchmark() {
    run_match_look_app_benchmark(false);
}

#[test]
#[ignore = "500-photo direct-Apply acceptance; no prior Preview or shared-style cache"]
fn match_look_ui_500_photo_direct_apply_benchmark() {
    run_match_look_app_benchmark(true);
}

fn run_match_look_app_benchmark(direct_apply: bool) {
    lightcraft_sysmem::init(Default::default());
    let mut h = setup(400.0, true, 1.0);
    h.app.ui.settings.gpu = true;
    h.step();
    assert!(lightcraft_engine::gpu::available(), "Real hardware GPU required");
    let adapter = lightcraft_engine::gpu::adapter_name().unwrap();
    assert!(!adapter.to_lowercase().contains("llvmpipe") && !adapter.to_lowercase().contains("software"), "{adapter}");
    let raw_dir = std::env::var("LOCAL_IMAGE_TEST_RAW_DIR").ok();
    let reference_count = std::env::var("LC_MATCH_LOOK_BENCH_REFERENCES")
        .map(|value| value.parse::<usize>().expect("LC_MATCH_LOOK_BENCH_REFERENCES must be an integer"))
        .unwrap_or(if raw_dir.is_some() { 2 } else { 10 });
    assert!((1..=20).contains(&reference_count), "Benchmark requires 1–20 references");
    let consistent_batch = match std::env::var("LC_MATCH_LOOK_BENCH_MODE").as_deref() {
        Ok("independent") => false,
        Ok("consistent") | Err(_) => true,
        Ok(other) => panic!("Unknown LC_MATCH_LOOK_BENCH_MODE {other:?}; use independent or consistent"),
    };
    if let Some(dir) = &raw_dir {
        h.app.session = lightcraft_engine::Session::new().with_fs();
        let mut paths = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.extension().is_some_and(|e| ["arw", "cr2", "nef", "raf", "dng", "rw2"].contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
            })
            .collect::<Vec<_>>();
        paths.sort();
        request(&mut h, "library.import", json!({"paths":paths,"mode":"add"}));
        assert!(h.app.session.catalog.photos().all(|p| p.develops_raw()));
    }
    assert!(h.app.session.catalog.photos().count() > reference_count, "Benchmark needs references plus at least one target source");
    analyze(&mut h, reference_count);
    let seeds = h
        .app
        .session
        .catalog
        .photos()
        .filter(|p| !h.app.session.match_look.references.contains(&p.id))
        .map(|p| p.as_ref().clone())
        .collect::<Vec<_>>();
    let seed = &seeds[0];
    if raw_dir.is_none() {
        let source = h.app.session.source_now(seed.id, lightcraft_engine::media::SourceLevel::Thumb).unwrap();
        let info = h.app.session.source_info(seed.id);
        h.app.session.media.file_loader = Some(std::sync::Arc::new(move |_, _| Ok(((*source).clone(), info.clone()))));
    }
    let count = std::env::var("LC_MATCH_LOOK_BENCH_COUNT")
        .map(|value| value.parse::<usize>().expect("LC_MATCH_LOOK_BENCH_COUNT must be an integer"))
        .unwrap_or(500);
    assert!(count > 0, "Benchmark needs at least one target instance");
    let mut ids = Vec::new();
    let mut ops = Vec::new();
    for n in 0..count {
        let mut p = seeds[n % seeds.len()].clone();
        p.id = h.app.session.catalog.alloc_photo_id();
        p.file_name = format!("benchmark-{n}-{}", p.file_name);
        if raw_dir.is_none() {
            p.source = lightcraft_catalog::Source::File { path: format!("match-look-proxy-{n}.jpg") };
        }
        ids.push(p.id);
        ops.push(lightcraft_catalog::Op::AddPhoto { photo: Box::new(p) });
    }
    h.app.session.commit("Benchmark inputs", lightcraft_catalog::Op::Batch { ops }).unwrap();
    request(&mut h, "library.select", json!({"ids":ids,"active":ids[0]}));
    h.app.ui.open_sections = vec!["matchLook".into()];
    h.step();
    click(&mut h, "button:matchLook");
    if h.app.session.match_look.options.consistent_batch != consistent_batch {
        click(&mut h, "check:matchConsistent");
    }
    assert_eq!(h.app.session.match_look.options.consistent_batch, consistent_batch);
    let (preview_metrics, preview_peak_rss) = if direct_apply {
        assert!(h.app.match_look.preview.is_none(), "Direct Apply must not inherit a Preview");
        assert!(h.app.match_look.control.is_none(), "Direct Apply must begin without a running Preview");
        (None, 0)
    } else {
        let preview_start = Instant::now();
        click(&mut h, "button:matchLookPreview");
        // Consistent Preview now analyzes the complete batch. Record that work rather
        // than hiding it behind the normal small-selection helper's 60-second wait.
        let mut preview_frames = Vec::new();
        let mut preview_peak_rss = 0;
        while h.app.match_look.control.is_some() {
            let frame = Instant::now();
            h.step();
            preview_frames.push(frame.elapsed().as_secs_f64() * 1000.0);
            preview_peak_rss = preview_peak_rss.max(lightcraft_sysmem::permits::process_rss().unwrap_or(0));
            assert!(preview_start.elapsed() < Duration::from_secs(300), "Preview watchdog expired");
            std::thread::sleep(Duration::from_millis(10));
        }
        h.step();
        h.step();
        assert!(h.app.match_look.error.is_empty(), "{}", h.app.match_look.error);
        let preview_wall = preview_start.elapsed().as_secs_f64();
        preview_frames.sort_by(f64::total_cmp);
        let preview_p95 = preview_frames.get(((preview_frames.len().saturating_sub(1)) as f64 * 0.95) as usize).copied().unwrap_or(0.0);
        println!("MATCH_LOOK_PREVIEW photos={count} wall_seconds={preview_wall:.3} ui_frame_p95_ms={preview_p95:.3}");
        assert!(
            h.app.widgets.iter().any(|(id, _)| id == "matchLook:preview"),
            "preview exists={}, dialog={:?}, last={:?}",
            h.app.match_look.preview.is_some(),
            h.app.ui.dialog,
            h.app.match_look.last_result
        );
        (Some((preview_wall, preview_p95)), preview_peak_rss)
    };
    let before = h.app.session.undo.len();
    let start = Instant::now();
    click(&mut h, "button:dialogOk");
    let mut frames = Vec::new();
    let mut peak_rss = preview_peak_rss;
    let mut reported = 0;
    while h.app.match_look.control.is_some() {
        let frame = Instant::now();
        h.step();
        frames.push(frame.elapsed().as_secs_f64() * 1000.0);
        peak_rss = peak_rss.max(lightcraft_sysmem::permits::process_rss().unwrap_or(0));
        let completed = h.app.match_look.control.as_ref().map_or(count, |c| c.completed.load(std::sync::atomic::Ordering::Relaxed));
        if completed >= reported + 100 {
            eprintln!("MATCH_LOOK_PROGRESS completed={completed} elapsed_seconds={:.3}", start.elapsed().as_secs_f64());
            reported = completed;
        }
        assert!(start.elapsed() < Duration::from_secs(300));
        std::thread::sleep(Duration::from_millis(10));
    }
    let wall = start.elapsed().as_secs_f64();
    assert_eq!(h.app.session.undo.len(), before + 1);
    assert_eq!(h.app.match_look.last_result.as_ref().unwrap()["changed"], count);
    frames.sort_by(f64::total_cmp);
    let p95 = frames[((frames.len() - 1) as f64 * 0.95) as usize];
    let solve = h.app.match_look.last_result.as_ref().unwrap()["elapsedMs"].as_f64().unwrap() / 1000.0;
    println!(
        "MATCH_LOOK_BENCH direct_apply={direct_apply} consistent_batch={consistent_batch} references={reference_count} adapter={adapter} raw={} workers={} profile={:?} memory_override={:?} photos={count} solve_seconds={solve:.3} wall_seconds={wall:.3} per_photo_ms={:.3} ui_frame_p95_ms={p95:.3} frames={}",
        raw_dir.is_some(),
        h.app.match_look.last_result.as_ref().unwrap()["workers"],
        lightcraft_sysmem::profile(),
        std::env::var("LOCAL_IMAGE_MEMORY_GB"),
        solve * 1000.0 / count as f64,
        frames.len()
    );
    let mut metrics = h.app.match_look.last_result.as_ref().unwrap().clone();
    println!("MATCH_LOOK_PHASES load_ms={} solve_ms={} renders={}", metrics["loadMs"], metrics["solveMs"], metrics["renders"]);
    metrics["benchmark"] = json!({
        "photos":count,"nativeRaw":raw_dir.is_some(),"directApply":direct_apply,
        "references":reference_count,"consistentBatch":consistent_batch,
        "uniqueTargetSources":if raw_dir.is_some() { count.min(seeds.len()) } else { 1 },
        "sourceSetRepeated":count > if raw_dir.is_some() { seeds.len() } else { 1 },
        "memoryPlanningOverrideGb":std::env::var("LOCAL_IMAGE_MEMORY_GB").ok(),
        "adapter":adapter,"profileThreads":lightcraft_sysmem::profile().threads,
        "previewWallSeconds":preview_metrics.map(|(wall, _)| wall),
        "applyWallSeconds":wall,"previewFrameP95Ms":preview_metrics.map(|(_, p95)| p95),"applyFrameP95Ms":p95,
        "peakRssBytes":peak_rss,"proxy500WallLimitSeconds":90.0,
        "proxy500WallGateApplies":count == 500 && raw_dir.is_none(),
        "wall500LimitSeconds":90.0,"wall500GateApplies":count == 500
    });
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/matchlook-revision3");
    std::fs::create_dir_all(&out).unwrap();
    let configuration = if raw_dir.is_none() && reference_count == 10 && consistent_batch && std::env::var("LOCAL_IMAGE_MEMORY_GB").is_err() {
        String::new()
    } else {
        let memory = std::env::var("LOCAL_IMAGE_MEMORY_GB")
            .ok()
            .map(|value| value.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.').collect::<String>());
        format!(
            "-r{reference_count}-{}{}{}",
            if consistent_batch { "consistent" } else { "independent" },
            if raw_dir.is_some() { "-raw" } else { "" },
            memory.map_or(String::new(), |gb| format!("-mem{gb}gb"))
        )
    };
    std::fs::write(
        out.join(format!(
            "benchmark-{count}-{}{}{configuration}.json",
            lightcraft_sysmem::profile().threads,
            if direct_apply { "-direct" } else { "" }
        )),
        serde_json::to_vec_pretty(&metrics).unwrap(),
    )
    .unwrap();
    if let Some((_, preview_p95)) = preview_metrics {
        assert!(preview_p95 < 16.7, "Preview must leave interactive frames responsive: {preview_p95}");
    }
    assert!(p95 < 16.7, "CPU job must leave interactive frames responsive: {p95}");
    assert!(h.settle(T));
    let texture_seconds = start.elapsed().as_secs_f64();
    let gpu_bytes = h.app.renderer.memory()["stageCaches"]["gpuBytes"].as_u64().unwrap();
    assert!(gpu_bytes > 0, "The app preview must use native GPU stages");
    println!(
        "MATCH_LOOK_APP_TEXTURE seconds={texture_seconds:.3} gpu_stage_bytes={gpu_bytes} peak_rss_mib={:.1}",
        peak_rss as f64 / (1 << 20) as f64
    );
    request(&mut h, "edit.undo", json!({}));
    if count == 500 {
        assert!(
            preview_metrics.is_none_or(|(wall, _)| wall <= 90.0) && wall <= 90.0,
            "500-photo throughput gate: Preview {preview_metrics:?}, Apply {wall:.3}s; each measured phase must be <=90s"
        );
    }
}

#[test]
fn match_look_ui_context_reference_remove_and_profile_catalog() {
    let mut h = setup(400.0, true, 1.0);
    let id = h.app.session.active().unwrap();
    let at = h.app.image_rect.unwrap().center();
    request(&mut h, "ui.click", json!({"x":at.x,"y":at.y,"button":"right"}));
    h.step();
    click(&mut h, "button:matchLookSetReference");
    assert_eq!(h.app.session.match_look.references, [id]);
    click(&mut h, "button:matchLookAnalyze");
    wait(&mut h);
    assert!(engine::current_ready(&h.app.session));
    request(&mut h, "ui.hoverWidget", json!({"id":format!("reference:{}",id.0)}));
    click(&mut h, &format!("icon:matchReferenceRemove-{}", id.0));
    assert!(h.app.session.match_look.references.is_empty());
    assert!(!engine::current_ready(&h.app.session));
    let catalog = crate::shortcuts::catalog::catalog();
    for command in ["develop.matchLookReferences", "develop.analyzeLook", "dialog.matchLook"] {
        let binding = catalog.iter().find(|b| b.command == command).unwrap();
        for profile in ["Lightroom", "Capture One"] {
            assert!(crate::keymaps::default(profile, command, true, binding.default.as_deref()).is_none());
        }
    }
    let menus = crate::menubar::menu_bar(&h.app);
    assert!(!menus.iter().any(|(title, _)| title == "Develop"));
    assert!(menus.iter().any(|(title, _)| title == "Photo"));
}

#[test]
fn match_look_ui_apply_direct_locks_colour_only_remember_amount_and_reset() {
    let mut h = setup(400.0, true, 1.0);
    analyze(&mut h, 2);
    let id = target(&mut h);
    click(&mut h, "button:matchLook");
    click(&mut h, "icon:matchNormalizeTools");
    click(&mut h, "check:matchWhiteBalance");
    click(&mut h, "check:matchExposure");
    click(&mut h, "button:matchColourOnly");
    click(&mut h, "check:matchSkin");
    let before = h.app.session.develop_of(id).unwrap();
    let opts = match h.app.ui.dialog.as_ref().unwrap() {
        crate::state::Dialog::MatchLook { options } => *options,
        _ => panic!(),
    };
    assert!(!opts.normalize && !opts.light_contrast && opts.color_adjust && opts.color_tone, "{opts:?}");
    assert!(!opts.adjustments.white_balance && !opts.adjustments.exposure && !opts.protect_skin);
    assert!(h.app.match_look.preview.is_none());
    click(&mut h, "button:dialogOk");
    wait(&mut h);
    let after = h.app.session.develop_of(id).unwrap();
    assert_eq!(after.wb, before.wb);
    assert_eq!(after.light, before.light);
    click(&mut h, "button:matchLook");
    let remembered = match h.app.ui.dialog.as_ref().unwrap() {
        crate::state::Dialog::MatchLook { options } => *options,
        _ => panic!(),
    };
    assert_eq!(opts, remembered);
    click(&mut h, "button:dialogCancel");
    if h.app.session.match_look.applied.contains_key(&id) {
        let undo = h.app.session.undo.len();
        request(&mut h, "ui.dragWidget", json!({"id":"slider:matchLook.photoStrength","dx":-40.0,"steps":6}));
        assert_eq!(h.app.session.undo.len(), undo + 1);
        assert!(h.app.session.match_look.applied[&id].options.strength < 100.0);
        if h.app.widgets.iter().any(|(id, _)| id == "button:matchAppliedReset-Color Adjust") {
            click(&mut h, "button:matchAppliedReset-Color Adjust");
            assert_eq!(h.app.session.develop_of(id).unwrap().color, before.color);
        }
    } else {
        panic!("Colour only should yield a per-photo editable result");
    }
}
#[test]
fn match_look_ui_wipe_update_confirm_import_export_picker_helpers() {
    let mut h = setup(400.0, false, 1.0);
    analyze(&mut h, 2);
    target(&mut h);
    click(&mut h, "button:matchLookSave");
    click(&mut h, "field:dialogText");
    request(&mut h, "ui.text", json!({"text":"Interiors"}));
    click(&mut h, "button:dialogOk");
    let look = h.app.session.saved_looks.file.looks[0].clone();
    click(&mut h, "icon:matchLookMenu");
    click(&mut h, "button:matchLookUpdate");
    assert!(matches!(h.app.ui.dialog, Some(crate::state::Dialog::UpdateMatchLook { .. })));
    click(&mut h, "button:dialogCancel");
    assert_eq!(h.app.session.saved_looks.file.looks[0], look);
    let reference = h.app.session.match_look.references[0];
    request(&mut h, "develop.set", json!({"ids":[reference.0],"control":"light.contrast","value":-23.0}));
    click(&mut h, "icon:matchLookMenu");
    click(&mut h, "button:matchLookUpdate");
    click(&mut h, "button:dialogOk");
    wait(&mut h);
    assert_ne!(h.app.session.saved_looks.file.looks[0].looks, look.looks);
    let look = h.app.session.saved_looks.file.looks[0].clone();
    click(&mut h, "button:matchLook");
    click(&mut h, "button:matchLookPreview");
    wait(&mut h);
    let original = h.app.match_look.wipe;
    request(&mut h, "ui.dragWidget", json!({"id":"wipe:matchLook:preview","dx":45.0,"steps":4}));
    assert!((h.app.match_look.wipe - original).abs() > 0.02);
    click(&mut h, "button:dialogCancel");
    let dir = std::env::temp_dir()
        .join(format!("lc-look-picker-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("interiors.json");
    let export_path = path.clone();
    h.app.services.save_look_file = Some(Box::new(move |_| Some(export_path.to_string_lossy().into_owned())));
    click(&mut h, "icon:matchLookMenu");
    click(&mut h, "button:matchLookExport");
    assert!(path.exists());
    let import_path = path.clone();
    h.app.services.pick_look_files = Some(Box::new(move || vec![import_path.to_string_lossy().into_owned()]));
    click(&mut h, "icon:matchLookMenu");
    click(&mut h, "button:matchLookImport");
    assert_eq!(h.app.session.saved_looks.file.looks.len(), 1);
    assert_eq!(h.app.session.saved_looks.file.looks[0].looks, look.looks);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn match_look_ui_revision2_compact_dialog_overflow_picker_drag_and_undo() {
    let mut h = setup(250.0, true, 1.0);
    assert!(!crate::menus::ui_enabled(&h.app, "develop.analyzeLook"));
    analyze(&mut h, 2);
    let id = target(&mut h);
    click(&mut h, "button:matchLook");
    let window = rect(&h, "dialog:window");
    assert!(window.height() < 700.0 && window.height() <= 1200.0 * 0.8, "{window:?}");
    click(&mut h, "button:dialogOk");
    wait(&mut h);
    let undo = h.app.session.undo.len();
    let history = h.app.session.catalog.photo(id).unwrap().history.len();
    request(&mut h, "ui.dragWidget", json!({"id":"slider:matchLook.photoStrength","dx":-30.0,"steps":6}));
    assert_eq!(h.app.session.undo.len(), undo + 1);
    assert_eq!(h.app.session.catalog.photo(id).unwrap().history.len(), history + 1);
    request(&mut h, "edit.undo", json!({}));
    request(&mut h, "edit.undo", json!({}));
    assert!(!h.app.session.match_look.applied.contains_key(&id));
    assert!(!h.app.widgets.iter().any(|(id, _)| id == "slider:matchLook.photoStrength"));
    click(&mut h, "button:matchLookSave");
    click(&mut h, "field:dialogText");
    request(&mut h, "ui.text", json!({"text":"Facade"}));
    click(&mut h, "button:dialogOk");
    let seed = h.app.session.saved_looks.file.looks[0].clone();
    h.app.session.saved_looks.file.looks = (0..60)
        .map(|index| {
            let mut l = seed.clone();
            l.id = format!("look-{index}");
            l.name = format!("Saved look {index}");
            l
        })
        .collect();
    h.app.session.match_look.selected = Some("look-0".into());
    h.step();
    assert!(!h.app.widgets.iter().any(|(id, _)| id == "button:matchLookRename"));
    click(&mut h, "icon:matchLookMenu");
    click(&mut h, "button:matchLookRename");
    assert!(h.app.ui.dialog.is_some());
    click(&mut h, "button:dialogCancel");
    click(&mut h, "dropdown:matchLookPicker");
    request(&mut h, "ui.hoverWidget", json!({"id":"button:matchLookPick-1"}));
    request(&mut h, "ui.scroll", json!({"dy":-5000.0}));
    for _ in 0..20 {
        h.step();
    }
    click(&mut h, "button:matchLookPick-60");
    assert_eq!(h.app.session.match_look.selected.as_deref(), Some("look-59"));
}
#[test]
fn match_look_ui_revision2_debounced_preview_reopen_and_reference_cycle() {
    let mut h = setup(400.0, false, 1.5);
    analyze(&mut h, 2);
    let id = target(&mut h);
    click(&mut h, "button:matchLook");
    click(&mut h, "button:matchLookPreview");
    wait(&mut h);
    click(&mut h, "button:matchCompare-1");
    click(&mut h, "button:matchCompare-2");
    assert_eq!(h.app.match_look.reference_index, 1);
    click(&mut h, "check:matchContrastGuard");
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(350) {
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    wait(&mut h);
    let options = match h.app.ui.dialog.as_ref().unwrap() {
        crate::state::Dialog::MatchLook { options } => *options,
        _ => panic!(),
    };
    assert!(crate::match_look::preview_ready(&h.app, options));
    let source = h.app.session.source_now(id, lightcraft_engine::media::SourceLevel::Thumb).unwrap();
    let info = h.app.session.source_info(id);
    let mut photo = h.app.session.catalog.photo(id).unwrap().as_ref().clone();
    photo.source = lightcraft_catalog::Source::File { path: "slow-preview-fixture".into() };
    h.app.session.catalog.apply(lightcraft_catalog::Op::RemovePhoto { id }).unwrap();
    h.app.session.catalog.apply(lightcraft_catalog::Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    h.app.session.media.forget(id);
    h.app.session.media.file_loader = Some(std::sync::Arc::new(move |_, _| {
        std::thread::sleep(Duration::from_millis(100));
        Ok(((*source).clone(), info.clone()))
    }));
    crate::match_look::start(&mut h.app, json!(options), true).unwrap();
    assert!(h.app.match_look.control.is_some());
    // Reopening invalidates the previous control, so its callback cannot clear the new slot.
    request(&mut h, "dialog.matchLook", json!({}));
    click(&mut h, "button:matchLookPreview");
    wait(&mut h);
    assert!(crate::match_look::preview_ready(&h.app, options));
    crate::match_look::start(&mut h.app, json!(options), true).unwrap();
    click(&mut h, "button:dialogOk");
    wait(&mut h);
    assert!(h.app.ui.dialog.is_none(), "Apply cancels its optional preview instead of waiting for it");
}

#[test]
fn match_look_ui_revision3_quality_pin_wrap_and_weak_review() {
    let mut h = setup(250.0, true, 1.0);
    analyze(&mut h, 20);
    let panel = rect(&h, "panel:right_panel");
    for id in h.app.session.match_look.references.clone() {
        let tile = rect(&h, &format!("reference:{}", id.0));
        assert!(tile.bottom() <= h.size.y, "all twenty reference tiles must be on screen: {tile:?}");
        assert!(tile.left() >= panel.left() && tile.right() <= panel.right(), "reference tile must fit the narrow panel: {tile:?}, {panel:?}");
    }
    let first = h.app.session.match_look.references[0];
    click(&mut h, &format!("icon:matchReferencePin-{}", first.0));
    assert_eq!(h.app.session.match_look.reference_weights[&first], 2.0);
    click(&mut h, &format!("reference:{}", first.0));
    assert!(h.app.match_look.compare_reference);
    let id = target(&mut h);
    request(&mut h, "develop.set", json!({"ids":[id.0],"control":"light.exposure","value":-1.0}));
    click(&mut h, "button:matchLook");
    click(&mut h, "check:matchRegions");
    click(&mut h, "button:matchLookPreview");
    wait(&mut h);
    click(&mut h, "button:matchCompare-2");
    assert_eq!(h.app.match_look.reference_index, 1);
    click(&mut h, "button:dialogOk");
    wait(&mut h);
    assert!(h.app.session.match_look.applied.contains_key(&id), "underexposed target must receive a match: {:?}", h.app.match_look.last_result);
    // Exercise the actual panel scroll with the full twenty-reference tray
    // before checking that the result remains reachable at narrow width.
    let last_reference = *h.app.session.match_look.references.last().unwrap();
    request(&mut h, "ui.hoverWidget", json!({"id":format!("reference:{}", last_reference.0)}));
    request(&mut h, "ui.scroll", json!({"dy":-600.0}));
    for _ in 0..8 {
        h.step();
    }
    assert!(h.app.widgets.iter().any(|(name, _)| name == "matchLook:quality"));
    let text = h
        .view
        .shapes()
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    for chip in ["Fit ", "WB shift ", "Decoded photo"] {
        assert!(text.contains(chip), "missing {chip}: {text}");
    }
    assert!(!text.contains("Tonal range "), "curve slope comfort must not be presented as full-pipeline reachability");
    assert_eq!(h.app.match_look.last_result.as_ref().unwrap()["perPhoto"][0]["id"], json!(id));
    // Stub the result metrics only; clicking runs the real grid-filter command.
    let other = h.app.session.catalog.photos().find(|photo| photo.id != id).unwrap().id;
    h.app.match_look.last_result =
        Some(json!({"weak":1,"previewOnly":1,"perPhoto":[{"id":id,"fit":0.2,"reach":0.4},{"id":other,"fit":0.9,"reach":0.1}]}));
    assert_eq!(crate::match_look::batch_details(h.app.match_look.last_result.as_ref().unwrap()), "1 photos matched weakly; 1 on embedded previews");
    h.step();
    click(&mut h, "button:matchWeak");
    assert_eq!(h.app.session.filter.only, vec![id]);
    assert_eq!(h.app.session.visible().len(), 1);
}

#[test]
#[ignore = "compose the final labelled reference/before/after proof sheets after RAW and JPEG audits"]
fn match_look_final_contact_sheets() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/matchlook-final-sheets");
    let mut directories =
        std::fs::read_dir(root.join("data")).unwrap().map(|e| e.unwrap().path()).filter(|p| p.join("sheet.json").is_file()).collect::<Vec<_>>();
    directories.sort();
    assert!(directories.len() >= 46, "22 RAW + 24 JPEG rows required");
    let escape = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let mut html = String::from(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Match Look final proof sheets</title><style>body{margin:2rem;background:#202020;color:#eee;font:16px system-ui}h1{font-size:1.6rem}a{color:#b9d8ff}figure{margin:2rem 0;border-top:1px solid #555;padding-top:1rem}img{display:block;width:100%;height:auto}figcaption{margin:0.5rem 0}nav{display:flex;flex-wrap:wrap;gap:1rem}</style><h1>Match Look — final photographer proof sheets</h1><p>Every row includes all finished analyzed references, the target before, and the target after. Click a sheet for full resolution. RAW all-camera rows use the expanded error; RAW mode gates retain the original 30-feature distribution error. JPEG rows report distribution distance against the solver goal. These historical diagnostics do not establish matched materials, perceptual quality or full-pipeline attainability.</p><nav><a href=\"#RAW-all-cameras\">All RAW cameras</a><a href=\"#RAW-modes\">RAW modes</a><a href=\"#JPEG-looks\">JPEG looks</a></nav>",
    );
    let mut records = Vec::new();
    for dir in directories {
        let metadata: Value = serde_json::from_slice(&std::fs::read(dir.join("sheet.json")).unwrap()).unwrap();
        let name = metadata["name"].as_str().unwrap();
        let mut view = crate::headless::HeadlessView::new();
        let t = Tokens::get(&view.ctx);
        let load = |path: &std::path::Path| {
            let pixels = lightcraft_codecs::decode(&std::fs::read(path).unwrap(), Default::default()).unwrap().to_srgb8();
            egui::ColorImage::from_rgba_unmultiplied([pixels.width, pixels.height], &pixels.data.iter().flatten().copied().collect::<Vec<_>>())
        };
        let mut references = Vec::new();
        for r in metadata["references"].as_array().unwrap() {
            let texture = view.ctx.load_texture(
                format!("{name}-{}", r["file"].as_str().unwrap()),
                load(&dir.join(r["file"].as_str().unwrap())),
                egui::TextureOptions::LINEAR,
            );
            references.push((r["label"].as_str().unwrap().to_string(), texture));
        }
        let before =
            view.ctx.load_texture(format!("{name}-before"), load(&dir.join(metadata["before"].as_str().unwrap())), egui::TextureOptions::LINEAR);
        let after =
            view.ctx.load_texture(format!("{name}-after"), load(&dir.join(metadata["after"].as_str().unwrap())), egui::TextureOptions::LINEAR);
        let reference_width = t.panel_w;
        let target_width = t.panel_w * 3.0;
        let width = (t.panel_w * 7.0 + 64.0).ceil();
        let bottom = std::cell::Cell::new(0.0);
        let title = format!("{} · {}", metadata["camera"].as_str().unwrap(), metadata["mode"].as_str().unwrap());
        let caption = format!(
            "Distribution fit {:.0}% · curve regularity {:.0}% · diagnostic error {:.6} → {:.6}{}",
            metadata["fit"].as_f64().unwrap() * 100.0,
            metadata["reach"].as_f64().unwrap() * 100.0,
            metadata["errorBefore"].as_f64().unwrap(),
            metadata["errorAfter"].as_f64().unwrap(),
            if metadata["alreadyClose"] == true { " · already close (protected)" } else { "" }
        );
        let mut draw = |ui: &mut egui::Ui| {
            ui.label(egui::RichText::new(&title).font(t.semibold(16.0)).color(t.text));
            ui.label(egui::RichText::new(&caption).font(t.font(14.0)).color(t.text_label));
            ui.separator();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(reference_width);
                    ui.label(egui::RichText::new("Analyzed references").font(t.semibold(14.0)));
                    for (label, texture) in &references {
                        ui.label(egui::RichText::new(label).font(t.font(12.0)).color(t.text_label));
                        let size = texture.size_vec2() * (reference_width / texture.size_vec2().x);
                        ui.add(egui::Image::new((texture.id(), size)).fit_to_exact_size(size));
                    }
                });
                for (label, texture) in [("Target BEFORE", &before), ("Target AFTER", &after)] {
                    ui.vertical(|ui| {
                        ui.set_width(target_width);
                        ui.label(egui::RichText::new(label).font(t.semibold(14.0)));
                        let size = texture.size_vec2() * (target_width / texture.size_vec2().x);
                        ui.add(egui::Image::new((texture.id(), size)).fit_to_exact_size(size));
                    });
                }
            });
            ui.separator();
            ui.label(
                egui::RichText::new("CC0 evaluation photographs · all references are rendered with their complete analyzed edits")
                    .font(t.font(11.5))
                    .color(t.text_dim),
            );
            bottom.set(ui.cursor().top() + ui.spacing().item_spacing.y);
        };
        for frame in 0..2 {
            view.run(crate::headless::HeadlessView::raw_input(egui::vec2(width, 2400.0), 1.0, frame as f64 / 60.0, vec![]), &mut draw);
        }
        let height = bottom.get().ceil();
        view.run(crate::headless::HeadlessView::raw_input(egui::vec2(width, height), 1.0, 0.05, vec![]), &mut draw);
        let pixels = view.paint(&Default::default());
        assert!(pixels.size[0] >= 1600);
        let image =
            lightcraft_raster::Rgba8 { width: pixels.size[0], height: pixels.size[1], data: pixels.pixels.iter().map(|p| p.to_array()).collect() };
        let file = format!("{name}.png");
        std::fs::write(root.join(&file), lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&image), &Default::default()).unwrap())
            .unwrap();
        if name.starts_with("raw-") {
            std::fs::copy(root.join(&file), root.parent().unwrap().join("matchlook-revision3").join(&file)).unwrap();
        }
        if let Some(legacy) = name.strip_prefix("jpeg-") {
            let dir = root.parent().unwrap().join("matchlook-quality/sheets");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::copy(root.join(&file), dir.join(format!("{legacy}.png"))).unwrap();
        }
        let kind = metadata["kind"].as_str().unwrap();
        records.push((kind.to_string(), title, caption, file));
    }
    for kind in ["RAW all cameras", "RAW modes", "JPEG looks"] {
        html.push_str(&format!("<section id=\"{}\"><h2>{}</h2>", escape(&kind.replace(' ', "-")), escape(kind)));
        for (_, title, caption, file) in records.iter().filter(|r| r.0 == kind) {
            html.push_str(&format!(
                "<figure><figcaption><strong>{}</strong><br>{}</figcaption><a href=\"{}\"><img loading=\"lazy\" src=\"{}\" alt=\"{}\"></a></figure>",
                escape(title),
                escape(caption),
                escape(file),
                escape(file),
                escape(title)
            ));
        }
        html.push_str("</section>");
    }
    html.push_str("</html>");
    std::fs::write(root.join("index.html"), html).unwrap();
    println!("MATCH_LOOK_FINAL_SHEETS count={} index={}", records.len(), root.join("index.html").display());
}
