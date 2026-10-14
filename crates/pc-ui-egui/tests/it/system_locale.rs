//! Use the actual platform UI-language API and the production (non-cfg(test)) Auto path.
#![cfg(not(target_arch = "wasm32"))]

use egui_kittest::{Harness, kittest::Queryable};
use photocraft_ui_egui::PhotocraftApp;

#[test]
fn native_first_launch_follows_os_ui_preferences() {
    let tags: Vec<String> = match std::env::var("PHOTOCRAFT_LOCALE").ok().filter(|tag| !tag.trim().is_empty()) {
        Some(tag) => vec![tag],
        None => sys_locale::get_locales().take(64).collect(),
    };
    let expected = tags
        .iter()
        .filter(|tag| tag.len() <= 128 && tag.trim().bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'@')))
        .find_map(|tag| li_i18n::Lang::from_tag(tag.trim()))
        .unwrap_or(li_i18n::Lang::En);
    eprintln!("native UI-language preferences: {tags:?}; resolved: {}", expected.code());
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(photocraft_engine::Session::new(), Default::default())
    });
    h.run_steps(4);
    assert_eq!(h.state().session.prefs().interface.language, "auto");
    assert_eq!(li_i18n::current(), expected, "native language preferences: {tags:?}");
    assert!(h.query_by_label(li_i18n::tr(expected, "File")).is_some(), "the first-launch menu must use the OS UI language");
}
