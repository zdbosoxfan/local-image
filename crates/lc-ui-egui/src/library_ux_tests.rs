//! Final-review Library UX: actual buttons plus model/preservation controls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::{
    Services,
    headless::Headless,
    state::{StartupView, UiState},
};
use lightcraft_catalog::{GroupBy, Op, Photo, PhotoId, Source};
use lightcraft_engine::Session;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
const T: Duration = Duration::from_secs(20);
fn photos(n: u64) -> Session {
    let mut session = Session::new();
    for index in 1..=n {
        let photo = Photo::new(
            PhotoId(index),
            Source::Demo { scene: 1 },
            &format!("photo-{index}.jpg"),
            "JPEG",
            60,
            40,
            &format!("2026-09-{index:02}T12:00:00"),
        );
        session.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    }
    if n > 0 {
        session.execute("library.select", &json!({"ids":[1]})).unwrap();
    }
    session
}
fn headless(session: Session, services: Services, size: [f32; 2]) -> Headless {
    let mut app = LightcraftApp::new(session, services);
    app.ui.settings.gpu = false;
    app.ui.right = RightPanel::None;
    let mut h = Headless::new(app, size, 1.0);
    for _ in 0..4 {
        h.step();
    }
    h
}
fn rect(h: &Headless, id: &str) -> egui::Rect {
    h.app.widgets.iter().find(|(key, _)| key == id).map(|(_, rect)| *rect).unwrap_or_else(|| panic!("missing {id}"))
}
fn click(h: &mut Headless, id: &str) {
    let hit = rect(h, id);
    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, h.size).contains_rect(hit), "{id} must be visibly hittable: {hit:?}");
    if id == "icon:more" {
        let inner = rect(h, "panel:toolStripInner");
        assert!(inner.contains_rect(hit), "More must fit the actual framed strip interior: {hit:?} vs {inner:?}");
    }
    assert_eq!(h.request("ui.clickWidget", json!({"id":id}), T)["ok"], true);
    h.step();
    h.step();
}
fn psd() -> Vec<u8> {
    let mut bytes = b"8BPS".to_vec();
    bytes.extend_from_slice(&1u16.to_be_bytes());
    bytes.extend_from_slice(&[0; 6]);
    bytes.extend_from_slice(&3u16.to_be_bytes());
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.extend_from_slice(&8u16.to_be_bytes());
    bytes.extend_from_slice(&3u16.to_be_bytes());
    for _ in 0..3 {
        bytes.extend_from_slice(&0u32.to_be_bytes());
    }
    bytes.extend_from_slice(&0u16.to_be_bytes());
    bytes.extend_from_slice(&[128; 12]);
    bytes
}
struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn library_ux_empty_startup_import_click_reviews_before_adding_and_cancel_preserves_source() {
    let dir = Temp(std::env::temp_dir().join(format!(
        "li-library-ux-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    )));
    std::fs::create_dir(&dir.0).unwrap();
    let file = dir.0.join("tiny.psd");
    let source = psd();
    std::fs::write(&file, &source).unwrap();
    let accept = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let (pick, count, path) = (accept.clone(), calls.clone(), file.to_string_lossy().into_owned());
    let services = Services {
        pick_files: Some(Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
            if pick.load(Ordering::SeqCst) { vec![path.clone()] } else { vec![] }
        })),
        ..Default::default()
    };
    let mut h = headless(Session::new().with_fs(), services, [1200., 800.]);
    assert_eq!(h.app.ui.view, ViewMode::PhotoGrid);
    assert_eq!(h.app.ui.right, RightPanel::None);
    click(&mut h, "button:emptyImport");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(h.app.scan.is_none() && h.app.session.catalog.is_empty());
    accept.store(true, Ordering::SeqCst);
    click(&mut h, "button:emptyImport");
    let until = Instant::now() + T;
    while !matches!(h.app.ui.dialog, Some(Dialog::Import { .. })) {
        assert!(Instant::now() < until, "import review did not appear");
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(h.app.session.catalog.is_empty(), "Import button opens review, never adds without confirmation");
    click(&mut h, "button:dialogCancel");
    assert!(h.app.session.catalog.is_empty());
    assert_eq!(std::fs::read(&file).unwrap(), source);
    let mut nonempty = UiState::default();
    nonempty.settings.startup_view = StartupView::Detail;
    nonempty = nonempty.sanitized();
    nonempty.empty_library_startup(false);
    assert_eq!(nonempty.view, ViewMode::Detail, "nonempty Develop startup preference remains intact");
}
#[test]
fn library_ux_sparse_automatic_grid_flows_but_explicit_dates_remain_grouped() {
    let mut h = headless(photos(6), Services::default(), [1400., 900.]);
    h.app.run("view.photoGrid", json!({})).unwrap();
    h.step();
    h.step();
    let ids = h.app.session.visible_cloned();
    assert_eq!(rect(&h, &format!("thumb:{}", ids[0].0)).top(), rect(&h, &format!("thumb:{}", ids[1].0)).top());
    let builds = h.app.caches.grid_stats.layout_builds;
    h.step();
    h.step();
    assert_eq!(h.app.caches.grid_stats.layout_builds, builds, "unchanged frames retain cached layout");
    h.app.run("library.sort", json!({"group":"day"})).unwrap();
    h.step();
    h.step();
    assert!(rect(&h, &format!("thumb:{}", ids[1].0)).top() > rect(&h, &format!("thumb:{}", ids[0].0)).bottom());
    h.app.run("library.sort", json!({"group":"none"})).unwrap();
    h.step();
    h.step();
    assert_eq!(rect(&h, &format!("thumb:{}", ids[0].0)).top(), rect(&h, &format!("thumb:{}", ids[1].0)).top());
    let dense = vec![
        lightcraft_catalog::DateRun { key: "2026-09-01".into(), label: "one".into(), start: 0, count: 6 },
        lightcraft_catalog::DateRun { key: "2026-09-02".into(), label: "two".into(), start: 6, count: 6 },
    ];
    assert!(!crate::panels::grid::automatic_flow(GroupBy::Auto, &dense, 12, 1200., 220.));
    assert!(!crate::panels::grid::automatic_flow(GroupBy::Day, &dense, 2, 1200., 220.));
}
#[test]
fn library_ux_copy_and_paste_are_stable_independent_actions() {
    let mut h = headless(photos(2), Services::default(), [1440., 900.]);
    h.app.run("view.photoGrid", json!({})).unwrap();
    h.step();
    h.step();
    assert!(h.app.session.clipboard.is_none());
    click(&mut h, "button:pasteSettings");
    assert!(h.app.session.clipboard.is_none(), "disabled Paste does not copy");
    h.app.run("develop.set", json!({"control":"light.exposure","value":2.0})).unwrap();
    click(&mut h, "button:copySettings");
    let first = h.app.session.clipboard.clone();
    h.app.run("library.select", json!({"ids":[2]})).unwrap();
    h.app.run("develop.set", json!({"control":"light.exposure","value":-1.0})).unwrap();
    h.step();
    click(&mut h, "button:copySettings");
    assert_ne!(h.app.session.clipboard, first, "Copy remains Copy after a previous copy");
    h.app.run("library.select", json!({"ids":[1]})).unwrap();
    h.step();
    click(&mut h, "button:pasteSettings");
    assert_eq!(h.app.session.catalog.photo(PhotoId(1)).unwrap().develop.light.exposure, -1.0);
    assert!(rect(&h, "button:copySettings").right() < rect(&h, "button:pasteSettings").left());
}
#[test]
fn library_ux_navigation_status_history_and_more_have_real_destinations() {
    for size in [[1440., 900.], [760., 480.]] {
        let mut h = headless(photos(2), Services::default(), size);
        assert!(!h.app.widgets.iter().any(|(id, _)| id == "icon:forward"));
        click(&mut h, "icon:back");
        assert_eq!(h.app.ui.view, ViewMode::PhotoGrid);
        assert!(!can_go_back(&h.app));
        click(&mut h, "icon:cloud");
        assert!(rect(&h, "panel:libraryStatus").width() > 0.0);
        click(&mut h, "menu:libraryStatus:settings");
        assert!(matches!(h.app.ui.dialog, Some(Dialog::Settings { .. })));
        h.app.ui.dialog = None;
        h.step();
        click(&mut h, "icon:bell");
        assert_eq!(h.app.ui.right, RightPanel::Activity);
    }
    let mut h = headless(photos(2), Services::default(), [1440., 900.]);
    click(&mut h, "icon:more");
    assert!(!matches!(h.app.ui.dialog, Some(Dialog::About)));
    click(&mut h, "menu:more:develop.copy");
    assert!(h.app.session.clipboard.is_some());
}
#[test]
fn library_ux_shortcuts_are_human_readable_and_photo_actions_precede_delete() {
    let linux = shortcut_rows(false);
    assert!(linux.iter().any(|(label, key)| label == "Import from Folder…" && key == "Ctrl+Shift+I"));
    assert!(!linux.iter().any(|(label, key)| label.starts_with("file.") || label.starts_with("view.") || key.contains("Cmd")));
    assert!(shortcut_rows(true).iter().any(|(_, key)| key == "⌘⇧I"));
    if cfg!(target_os = "linux") {
        assert_eq!(REVEAL_LABEL, "Show in File Manager");
    }
    let app = LightcraftApp::new(photos(2), Services::default());
    let menu = crate::menubar::menu_bar(&app);
    let photo = &menu.iter().find(|(label, _)| label == "Photo").unwrap().1;
    assert!(matches!(photo.last(), Some(crate::menubar::MenuNode::Item{id,..}) if id=="photo.delete"));
    for command in ["photo.copyMetadata", "photo.pasteMetadata", "dialog.allMetadata", "dialog.cull", "photo.convertToDng"] {
        assert!(
            photo.iter().any(|node| matches!(node,crate::menubar::MenuNode::Item{id,..} if id==command)),
            "{command} must have an intentional menu position"
        );
    }
}

#[test]
fn library_ux_empty_filter_clear_keeps_search_and_results_consistent() {
    let mut h = headless(photos(2), Services::default(), [1200., 800.]);
    h.app.run("view.photoGrid", json!({})).unwrap();
    h.app.ui.search = "no matching photo".into();
    h.app.run("library.filter", json!({"text":"no matching photo"})).unwrap();
    h.step();
    h.step();
    assert!(h.app.session.visible_cloned().is_empty());
    click(&mut h, "button:emptyClearFilters");
    assert!(h.app.ui.search.is_empty(), "successful clear must also clear the visible search field");
    assert_eq!(h.app.session.filter, Default::default());
    assert_eq!(h.app.session.visible_cloned().len(), 2);
}

#[derive(Clone, Default)]
struct FailingLibrary {
    files: lightcraft_catalog::MemStore,
    fail: Arc<AtomicBool>,
}
impl lightcraft_catalog::Store for FailingLibrary {
    fn read(&mut self, name: &str) -> std::io::Result<Option<Vec<u8>>> {
        lightcraft_catalog::Store::read(&mut self.files, name)
    }
    fn write_atomic(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("injected Library write failure"));
        }
        lightcraft_catalog::Store::write_atomic(&mut self.files, name, data)
    }
    fn append(&mut self, name: &str, data: &[u8]) -> std::io::Result<()> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("injected Library write failure"));
        }
        lightcraft_catalog::Store::append(&mut self.files, name, data)
    }
    fn truncate(&mut self, name: &str, len: u64) -> std::io::Result<()> {
        lightcraft_catalog::Store::truncate(&mut self.files, name, len)
    }
    fn describe(&self) -> String {
        "Library UX failing store".into()
    }
}
#[test]
fn library_ux_status_open_failure_reports_error_and_keeps_unsaved_library() {
    let store = FailingLibrary::default();
    let mut session = Session::new();
    session
        .open_library_in(
            lightcraft_engine::library::LibraryStores {
                dir: "library-ux-original".into(),
                catalog: Box::new(store.clone()),
                files: Box::new(lightcraft_catalog::MemStore::new()),
                on_disk: false,
            },
            false,
        )
        .unwrap();
    session.execute("album.create", &json!({"name":"Saved collection"})).unwrap();
    let identity = session.library_identity().to_owned();
    store.fail.store(true, Ordering::SeqCst);
    assert!(session.execute("album.create", &json!({"name":"Keep unsaved collection"})).is_err());
    let temp = Temp(std::env::temp_dir().join(format!(
        "li-library-open-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    )));
    std::fs::create_dir(&temp.0).unwrap();
    let next = temp.0.join("must-not-open");
    let chosen = next.to_string_lossy().into_owned();
    let services = Services { pick_folder: Some(Box::new(move || Some(chosen.clone()))), ..Default::default() };
    let mut h = headless(session, services, [1440., 900.]);
    click(&mut h, "icon:cloud");
    h.app.ui.toast = None;
    click(&mut h, "menu:libraryStatus:open");
    assert!(h.app.ui.toast.as_ref().is_some_and(|(text, _)| text.contains("injected Library write failure")), "failed switch needs a visible error");
    assert_eq!(h.app.session.library_identity(), identity);
    assert_eq!(h.app.session.catalog.albums().count(), 2, "both saved and unsaved records remain");
    assert!(h.app.session.unsaved().is_some());
    assert!(!next.exists(), "new Library must not open after old Library cannot save");
    store.fail.store(false, Ordering::SeqCst);
}

#[test]
fn library_ux_shortcut_help_matches_effective_import_dispatch() {
    let rows = shortcut_rows(false);
    let import: Vec<_> = rows.iter().filter(|(_, key)| key == "Ctrl+Shift+I").collect();
    assert_eq!(import.len(), 1, "the engine import shortcut is shadowed by the UI review command");
    assert_eq!(import[0].0, "Import from Folder…");
    let picks = Arc::new(AtomicUsize::new(0));
    let count = picks.clone();
    let services = Services {
        pick_folder: Some(Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
            None
        })),
        ..Default::default()
    };
    let mut h = headless(Session::new(), services, [1200., 800.]);
    assert_eq!(h.request("ui.key", json!({"key":"i","cmd":true,"shift":true}), T)["ok"], true);
    assert_eq!(picks.load(Ordering::SeqCst), 1);
    assert!(h.app.session.catalog.is_empty());
    assert!(!h.app.session.journal.iter().any(|(id, _)| id == "library.import"), "cancelled review picker must not fall through to engine import");
}
#[test]
fn library_ux_hosted_more_keeps_filter_actions_local() {
    let mut h = headless(photos(1), Services::default(), [1440., 900.]);
    h.app.host_session = Some(crate::panels::host_session::HostSession::new("Temporary layer", vec!["crop".into()]));
    h.app.ui.view = ViewMode::Detail;
    h.app.ui.right = RightPanel::Edit;
    h.step();
    h.step();
    click(&mut h, "icon:more");
    for id in ["dialog.export", "dialog.syncSettings", "app.settings"] {
        assert!(!h.app.widgets.iter().any(|(key, _)| key == &format!("menu:more:{id}")), "{id} must not escape a filter session");
    }
    click(&mut h, "menu:more:develop.copy");
    assert!(h.app.session.clipboard.is_some());
    h.app.run("develop.set", json!({"control":"light.exposure","value":2.0})).unwrap();
    h.step();
    click(&mut h, "icon:more");
    click(&mut h, "menu:more:develop.paste");
    assert_eq!(h.app.session.catalog.photo(PhotoId(1)).unwrap().develop.light.exposure, 0.0);
    assert!(h.app.export.is_none() && h.app.session.last_export.is_none());
    assert_eq!(h.app.host_session.as_ref().unwrap().result, None);
    click(&mut h, "button:hostSessionCancel");
    assert_eq!(h.app.host_session.as_ref().unwrap().result, Some(false));
}
#[test]
fn library_ux_persisted_detail_switches_to_empty_grid_but_cancel_keeps_current_library() {
    let temp = Temp(std::env::temp_dir().join(format!(
        "li-library-switch-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    )));
    std::fs::create_dir(&temp.0).unwrap();
    let old = temp.0.join("old");
    let new = temp.0.join("new");
    let mut session = Session::new().with_fs();
    session.open_library(&old, false).unwrap();
    let photo = Photo::new(PhotoId(1), Source::Demo { scene: 1 }, "Kept.jpg", "JPEG", 60, 40, "2026-09-01");
    session.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    session.execute("library.select", &json!({"ids":[1]})).unwrap();
    let identity = session.library_identity().to_owned();
    let choose = Arc::new(AtomicBool::new(false));
    let pick = choose.clone();
    let path = new.to_string_lossy().into_owned();
    let services = Services { pick_folder: Some(Box::new(move || pick.load(Ordering::SeqCst).then(|| path.clone()))), ..Default::default() };
    let mut app = LightcraftApp::new(session, services);
    let saved = serde_json::to_vec(&UiState { view: ViewMode::Detail, right: RightPanel::Edit, ..Default::default() }).unwrap();
    app.ui = serde_json::from_slice::<UiState>(&saved).unwrap().sanitized();
    app.ui.settings.gpu = false;
    let mut h = Headless::new(app, [1440., 900.], 1.0);
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(h.app.ui.view, ViewMode::Detail);
    click(&mut h, "icon:cloud");
    click(&mut h, "menu:libraryStatus:open");
    assert_eq!(h.app.ui.view, ViewMode::Detail);
    assert_eq!(h.app.session.library_identity(), identity);
    assert!(!new.exists());
    choose.store(true, Ordering::SeqCst);
    click(&mut h, "icon:cloud");
    click(&mut h, "menu:libraryStatus:open");
    assert!(h.app.session.catalog.is_empty());
    assert_eq!(h.app.ui.view, ViewMode::PhotoGrid);
    assert_eq!(h.app.ui.right, RightPanel::None);
    assert_ne!(h.app.session.library_identity(), identity);
    let mut previous = Session::new().with_fs();
    previous.open_library(&old, false).unwrap();
    assert!(previous.catalog.photo(PhotoId(1)).is_some(), "switch preserves the saved old Library");
    previous.close_library().unwrap();
}
