//! Library & Develop language behaviour in the real window: fonts per script, menus, Preferences,
//! the control channel and translated headings. Engine and catalog tests live in `li-i18n`.

use li_i18n::Lang;

/// Every character a catalog paints has a glyph in the fonts the UI installs, so a translation
/// never shows as boxes. Without craft-fonts there are no CJK faces at all, and the CJK
/// catalogs are skipped (the same degradation the Japanese UI has always had).
#[test]
fn catalog_characters_are_paintable() {
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::font_definitions(lightcraft_engine::CRAFT_FONTS));
    let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
    out.textures_delta.clear();
    for language in Lang::ALL {
        let messages = li_i18n::catalog(language, li_i18n::Domain::Library);
        if messages.is_empty() {
            continue;
        }
        // The faces this language's script needs; a language the craft-fonts input does not
        // cover (a translation added ahead of its font) is reported, not failed.
        let mut faces: Vec<&str> =
            lightcraft_engine::CRAFT_FONTS.iter().filter(|font| font.serves(language.script())).map(|font| font.family).collect();
        faces.dedup();
        if faces.is_empty() {
            eprintln!("skipped {}: built without a craft-fonts face for {}", language.code(), language.script());
            continue;
        }
        ctx.fonts_mut(|fonts| {
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Name(crate::theme::FONT_SEMIBOLD.into())] {
                let font = egui::FontId::new(13.0, family.clone());
                let mut missing: Vec<char> = messages
                    .plain_entries()
                    .flat_map(|(_, message)| message.chars())
                    .filter(|ch| !ch.is_whitespace() && !fonts.has_glyph(&font, *ch))
                    .collect();
                missing.sort_unstable();
                missing.dedup();
                assert!(
                    missing.is_empty(),
                    "{} in {family:?} (faces: {faces:?}) lack glyphs: {}",
                    language.code(),
                    missing.iter().collect::<String>()
                );
            }
        });
    }
}

/// The interface's own symbols paint in every language: the CJK faces stop at Han and kana, so a
/// translation must not carry a symbol no installed font has (egui's defaults cover the rest).
#[test]
fn interface_symbols_are_paintable() {
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::font_definitions(lightcraft_engine::CRAFT_FONTS));
    let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
    out.textures_delta.clear();
    // ⌥ and ⌫ come from egui's bundled fonts; ▸ and ▾ are in neither (measured 2026-10-07).
    ctx.fonts_mut(|fonts| {
        for symbol in ['⌥', '⌫', '⇧', '⌘', '→', '…', '—', '“', '”', '·', '»', '⌄', '★'] {
            let font = egui::FontId::new(13.0, egui::FontFamily::Proportional);
            assert!(fonts.has_glyph(&font, symbol), "{symbol:?} has no glyph in any installed font");
        }
    });
}

/// The language menu covers every language, and a language's own command selects it.
#[test]
fn language_commands_cover_every_language() {
    let commands = [
        ("app.language.traditionalChinese", Lang::ZhHant),
        ("app.language.english", Lang::En),
        ("app.language.simplifiedChinese", Lang::ZhHans),
        ("app.language.japanese", Lang::Ja),
        ("app.language.portuguese", Lang::PtBr),
        ("app.language.korean", Lang::Ko),
        ("app.language.german", Lang::De),
        ("app.language.french", Lang::Fr),
        ("app.language.spanish", Lang::Es),
        ("app.language.italian", Lang::It),
        ("app.language.russian", Lang::Ru),
        ("app.language.czech", Lang::Cs),
        ("app.language.indonesian", Lang::Id),
    ];
    // One command per language, and every command reachable from the menu table.
    assert_eq!(commands.len(), Lang::ALL.len());
    for (id, language) in commands {
        assert_eq!(crate::menus::language_from_command(id), Some(language), "{id}");
        assert!(crate::menus::ui_commands().any(|command| command.0 == id), "{id} is not in the menu");
    }
    assert_eq!(crate::menus::language_from_command("app.language.klingon"), None);
    assert_eq!(crate::menus::language_from_command("view.detail"), None);
}

#[test]
fn preferences_round_trip_the_language() {
    let fresh: crate::state::UiState = serde_json::from_str("{}").unwrap();
    assert_eq!(fresh.language, Lang::En);
    let settings = crate::state::UiState { language: Lang::ZhHans, ..fresh };
    let saved = serde_json::to_string(&settings).unwrap();
    let restored: crate::state::UiState = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored.language, Lang::ZhHans);
}

/// User-named menu items (presets, albums, label sets) are never translated; built-in labels are.
#[test]
fn menu_labels_translate_but_user_names_survive() {
    for language in Lang::ALL {
        li_i18n::set_current(language);
        assert_eq!(crate::menubar::display_item_label("album.addPhotos", &serde_json::json!({"id": 1}), "Color"), "Color");
        assert_eq!(crate::menubar::display_item_label("app.export", &serde_json::json!({"preset": "Color"}), "Color"), "Color");
        assert_eq!(crate::menubar::display_item_label("view.photoGrid", &serde_json::Value::Null, "Color"), li_i18n::library::tr(language, "Color"));
    }
    li_i18n::set_current(Lang::ZhHans);
    assert_eq!(crate::menubar::display_item_label("view.photoGrid", &serde_json::Value::Null, "Color"), "颜色");
    li_i18n::set_current(Lang::Ja);
    assert_eq!(crate::menubar::display_item_label("view.photoGrid", &serde_json::Value::Null, "Color"), "カラー");
    li_i18n::set_current(Lang::En);
}

/// The text the whole window paints over a few frames in `language`.
fn painted_text(ctx: &egui::Context, app: &mut crate::LightcraftApp, language: Lang) -> String {
    fn collect(shape: &egui::epaint::Shape, text: &mut String) {
        match shape {
            egui::epaint::Shape::Text(shape) => {
                text.push_str(&shape.galley.job.text);
                text.push('\n');
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, text)),
            _ => {}
        }
    }
    app.ui.language = language;
    let mut text = String::new();
    for frame in 0..4 {
        let input = crate::headless::HeadlessView::raw_input(egui::vec2(1600.0, 1000.0), 1.0, frame as f64 / 60.0, vec![]);
        let mut out = ctx.run_ui(input, |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        // This inspects shapes without a renderer; discard texture uploads explicitly.
        out.textures_delta.clear();
        text.clear();
        for shape in out.shapes {
            collect(&shape.shape, &mut text);
        }
    }
    text
}

/// Every language is painted by the real window, switching at runtime reinstalls the fonts
/// (their CJK fallback order follows the language), and command ids never change with it.
#[test]
fn every_language_is_painted_and_menu_ids_stay_the_same() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
    app.ui.left_panel = true;
    let ids = |app: &crate::LightcraftApp| crate::menus::menu_entries(app).into_iter().map(|entry| entry.id).collect::<Vec<_>>();
    let english_ids = ids(&app);
    for language in Lang::ALL {
        let text = painted_text(&ctx, &mut app, language);
        for label in ["Catalog", "All Photos"] {
            assert!(
                text.contains(li_i18n::library::tr(language, label)),
                "{language:?}: {label} -> {:?} not in\n{text}",
                li_i18n::library::tr(language, label)
            );
        }
        assert_eq!(app.font_language, language, "the fonts follow the language");
        assert_eq!(ids(&app), english_ids, "{language:?}: command ids are presentation-independent");
    }
    let text = painted_text(&ctx, &mut app, Lang::Ja);
    assert!(text.contains(li_i18n::library::tr(Lang::Ja, "Catalog")) && text.contains("すべての写真"), "{text}");
    let text = painted_text(&ctx, &mut app, Lang::ZhHans);
    assert!(text.contains(li_i18n::library::tr(Lang::ZhHans, "Catalog")) && text.contains("所有照片"), "{text}");
    let text = painted_text(&ctx, &mut app, Lang::PtBr);
    assert!(text.contains(li_i18n::library::tr(Lang::PtBr, "Catalog")) && text.contains("Todas as fotos"), "{text}");
    li_i18n::set_current(Lang::En);
}

/// The UI families, after one frame so the font definitions are loaded.
fn fonts_ctx(craft: &'static [lightcraft_engine::CraftFont]) -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::font_definitions(craft));
    let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
    out.textures_delta.clear();
    ctx
}

/// Built with craft-fonts, every CJK language renders with real glyphs (no tofu) in every UI
/// family, Monospace included.
#[test]
fn craft_fonts_render_cjk_in_the_ui() {
    let samples = [(Lang::Ja, "日本語の文字"), (Lang::ZhHans, "简体中文字"), (Lang::ZhHant, "繁體中文字")];
    for (language, sample) in samples {
        if !lightcraft_engine::CRAFT_FONTS.iter().any(|font| font.serves(language.script())) {
            eprintln!("skipped {}: built without a craft-fonts face for {}", language.code(), language.script());
            continue;
        }
        li_i18n::set_current(language);
        let ctx = fonts_ctx(lightcraft_engine::CRAFT_FONTS);
        ctx.fonts_mut(|fonts| {
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Name(crate::theme::FONT_SEMIBOLD.into()), egui::FontFamily::Monospace] {
                let font = egui::FontId::new(13.0, family);
                for ch in sample.chars() {
                    assert!(fonts.has_glyph(&font, ch), "{language:?}: {ch} in {font:?}");
                }
                let galley = fonts.layout_no_wrap(sample.into(), font.clone(), egui::Color32::WHITE);
                let wide = 13.0 * (sample.chars().count() as f32 - 1.0);
                assert!(galley.size().x > wide, "{language:?} {font:?}: full-width glyphs, {:?}", galley.size());
            }
        });
    }
    li_i18n::set_current(Lang::En);
}

/// Built without craft-fonts, the UI (in every language) still installs its fonts and runs;
/// Latin text keeps Inter.
#[test]
fn the_ui_works_without_craft_fonts() {
    let ctx = fonts_ctx(&[]);
    ctx.fonts_mut(|fonts| {
        // (Not Monospace: egui's `has_glyph` reports false for glyphs of the family's
        // replacement-glyph face, which there is Hack, the face that draws Latin.)
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Name(crate::theme::FONT_SEMIBOLD.into())] {
            let font = egui::FontId::new(13.0, family);
            assert!("LightCraft".chars().all(|ch| fonts.has_glyph(&font, ch)), "{font:?}");
        }
    });
    let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
    for language in Lang::ALL {
        app.ui.language = language;
        for frame in 0..3 {
            let input = crate::headless::HeadlessView::raw_input(egui::vec2(1200.0, 800.0), 1.0, frame as f64 / 60.0, vec![]);
            let mut out = ctx.run_ui(input, |ui| {
                app.logic(ui.ctx());
                app.ui(ui);
            });
            out.textures_delta.clear();
        }
    }
    li_i18n::set_current(Lang::En);
}

/// Traditional Chinese through the menu command and the control channel, persisted, with
/// localised headings and dates; ids and user text are untouched.
#[test]
fn traditional_chinese_switches_through_menu_and_control_and_persists() {
    use crate::control::{ControlRequest, Outcome};
    let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
    app.run("app.language.traditionalChinese", serde_json::json!({})).unwrap();
    assert_eq!(app.ui.language, Lang::ZhHant);
    // Applied at once, not on the next frame.
    assert_eq!(li_i18n::library::t("File"), "檔案");
    assert_eq!(crate::library_labels::source_label(lightcraft_engine::LibrarySource::All, &app.session.catalog), "所有照片");
    assert_eq!(crate::library_labels::date_group_label("2026-09-20", false), "2026年9月20日 星期日");
    assert_eq!(crate::library_labels::date_group_label("2026-09-20", true), "20日 星期日");
    assert_eq!(crate::library_labels::date_group_label("2026-09", false), "2026年9月");
    assert_eq!(crate::library_labels::date_group_label("2026-09", true), "9月");
    assert_eq!(crate::library_labels::date_group_label("2026", false), "2026年");
    assert_eq!(crate::library_labels::date_group_label("", false), "未知日期");
    assert_eq!(crate::library_labels::display_time("2026-09-20T16:04:05"), "2026年9月20日 星期日 16:04:05");
    assert_eq!(crate::library_labels::display_time("a-user-value"), "a-user-value");
    assert_eq!(crate::menubar::checked(&app, "app.language.traditionalChinese"), Some(true));
    assert_eq!(crate::menubar::checked(&app, "app.language.english"), Some(false));
    assert_eq!(li_i18n::library::t("my-photo.jpg"), "my-photo.jpg");
    for id in ["album.addPhotos", "metadata.applyPreset", "label.applySet"] {
        assert_eq!(crate::menubar::display_item_label(id, &serde_json::json!({"id": 1}), "Color"), "Color");
    }
    assert_eq!(crate::menubar::display_item_label("view.photoGrid", &serde_json::Value::Null, "Color"), "色彩");
    assert_eq!(li_i18n::tr_format!("{n} photo{}", "s", n = 12), "12 張照片");
    assert_eq!(li_i18n::tr_format!("Exported {ok} of {total} photo{}", "s", ok = 4, total = 12), "已匯出 4／12 張照片");
    let saved = serde_json::to_string(&app.ui).unwrap();
    assert!(saved.contains(r#""language":"zh-hant""#), "{saved}");
    assert_eq!(serde_json::from_str::<crate::state::UiState>(&saved).unwrap().language, Lang::ZhHant);
    let ctx = egui::Context::default();
    app.ui.language = Lang::En;
    for code in ["zh-hant", "zh-Hant", "zh-TW", "zh_TW", "zh-HK"] {
        let (req, _) = ControlRequest::new("ui.set", serde_json::json!({"language": code}));
        let Outcome::Done(reply) = crate::control::handle(&mut app, &ctx, &req) else { panic!("expected a reply") };
        assert_eq!(reply["ok"], true, "{code}");
        assert_eq!(app.ui.language, Lang::ZhHant, "{code}");
        assert_eq!(li_i18n::current(), Lang::ZhHant, "{code} applies at once");
    }
    let (req, _) = ControlRequest::new("ui.set", serde_json::json!({"language": "xx"}));
    let Outcome::Done(reply) = crate::control::handle(&mut app, &ctx, &req) else { panic!("expected a reply") };
    assert_eq!(reply["ok"], false);
    assert_eq!(app.ui.language, Lang::ZhHant);
    let ids = |app: &crate::LightcraftApp| crate::menus::menu_entries(app).into_iter().map(|entry| entry.id).collect::<Vec<_>>();
    let chinese_ids = ids(&app);
    app.run("app.language.english", serde_json::json!({})).unwrap();
    assert_eq!(li_i18n::library::t("File"), "File");
    assert_eq!(chinese_ids, ids(&app));
}

/// Dates follow every language's own patterns; English keeps the catalog's wording.
#[test]
fn date_headings_in_every_language() {
    li_i18n::set_current(Lang::En);
    assert_eq!(crate::library_labels::date_group_label("2026-09-20", false), "Sunday, 20 September 2026");
    assert_eq!(crate::library_labels::date_group_label("2026-09-20", true), "Sunday, 20");
    assert_eq!(crate::library_labels::date_group_label("2026-09", true), "September");
    assert_eq!(crate::library_labels::display_time("2026-09-20T16:04:05"), lightcraft_catalog::dates::display_time("2026-09-20T16:04:05"));
    li_i18n::set_current(Lang::Ja);
    assert_eq!(crate::library_labels::date_group_label("2026-09-20", false), "2026年9月20日（日曜日）");
    assert_eq!(crate::library_labels::date_group_label("2026-09", true), "9月");
    li_i18n::set_current(Lang::ZhHans);
    assert_eq!(crate::library_labels::date_group_label("2026-09-20", false), "2026年9月20日 星期日");
    for language in Lang::ALL {
        li_i18n::set_current(language);
        // A key that is not a date never panics and never shows a raw pattern.
        for key in ["", "abcd", "2026-xx", "2026-13", "2026-09-xx", "２０２６-09"] {
            assert!(!crate::library_labels::date_group_label(key, false).contains('{'), "{language:?}: {key:?}");
            assert!(!crate::library_labels::date_group_label(key, true).contains('{'), "{language:?}: {key:?}");
        }
    }
    li_i18n::set_current(Lang::En);
}

/// Stock presets and panel headers are translated; a user preset keeps its name, also when it
/// shares a stock preset's name.
#[test]
fn traditional_chinese_presets_and_panel_headers_are_painted() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
    app.ui.presets = true;
    let builtin = app.session.presets.iter().find(|p| p.name == "Warm Glow").unwrap().clone();
    app.session.presets.push(lightcraft_develop::Preset { id: "user.test".into(), builtin: false, ..builtin });
    for (panel, title) in [(crate::state::RightPanel::Activity, "歷史紀錄"), (crate::state::RightPanel::Versions, "版本")] {
        app.ui.right = panel;
        let text = painted_text(&ctx, &mut app, Lang::ZhHant);
        assert!(text.contains(title), "{text}");
        assert!(!text.contains("History") && !text.contains("Versions"), "{text}");
        assert!(text.contains("暖光"), "stock preset: {text}");
        assert!(text.contains("Warm Glow"), "user preset: {text}");
        assert!(text.contains("色彩"), "stock group: {text}");
    }
    assert_eq!(crate::library_labels::history_label("Exposure", &app.session.presets), "曝光");
    assert_eq!(crate::library_labels::history_label("Preset: Warm Film", &app.session.presets), "預設集：暖調底片");
    assert_eq!(crate::library_labels::history_label("Preset: Warm Glow", &app.session.presets), "預設集：Warm Glow");
    li_i18n::set_current(Lang::En);
}
