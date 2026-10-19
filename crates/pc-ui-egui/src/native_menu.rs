//! The macOS menu bar: the same menus as the in-window bar ([`crate::menus::menu_items`]), laid
//! out the way a Mac app's are and handed to a platform [`Backend`] (the desktop app's muda one).
//!
//! Pure and platform-free, so it is tested everywhere and builds for the web; only the backend
//! touches AppKit. What the layout does ([`mac_layout`]):
//! - an **app menu** named Local Image: About (from Help), Settings (Edit › Preferences), Language,
//!   Appearance (Window › Theme), Services, Hide, Hide Others, Show All and Quit (File › Exit,
//!   still the app's own `file.exit` so the unsaved-changes prompt runs). The other Craft apps
//!   use the same app menu;
//! - **Window** gets Minimize, Zoom and Bring All to Front, and **Help** the system search field;
//! - **clashes** with the system's keys (⌘H, ⌘M, ⌘W, ⌘,) are resolved in Local Image's favour, and
//!   reported; Hide and Minimize move to ⌃⌘H and ⌃⌘M, as Photoshop does.
//!
//! **Keys stay with egui.** AppKit runs a menu's key equivalents before the window sees the key,
//! which would skip Local Image's key rules (text fields keep ⌘C/⌘V/⌘Z, dialogs and Camera Raw are
//! modal, Liquify has its own ⌘Z). So the backend reports a key equivalent as [`Event::Key`], and
//! [`NativeMenu::raw_input`] feeds it to egui as the key press it was: the shortcut runs through
//! [`crate::shortcuts::handle`] exactly as without a native menu. A click is [`Event::Click`] and
//! runs the command like an in-window menu click.

#[cfg(test)]
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::PhotocraftApp;
use li_commands::menu::MenuItem;
use li_i18n::{Lang, tr, tr_id};

use li_commands::menu::{Backend, Event, HIDE, Item, ItemRole, Layout, MINIMIZE, MenuBar, MenuRole, Node, from_items};
#[cfg(test)]
use li_commands::menu::{Menu, Standard, structure_key};
#[cfg(test)]
use li_keys::Chord;

/// Apply the shared macOS layout using the editor's translation catalog.
pub fn mac_layout(bar: &MenuBar, lang: Lang) -> Layout {
    li_commands::menu::mac_layout(bar, |text| tr(lang, text).to_string())
}

/// Command ids of the app menu's Language items: `app.language.<code>`, where `auto` follows the
/// system (the `interface.language` preference values).
pub const LANGUAGE_PREFIX: &str = "app.language.";

/// Language in the app menu, after Settings: Auto, then every UI language in its own name, with
/// the current preference checked.
fn language_node(pref: &str, lang: Lang) -> Node {
    let item = |code: &str, label: &str| Item {
        id: format!("{LANGUAGE_PREFIX}{code}"),
        label: label.into(),
        shortcut: None,
        enabled: true,
        checked: Some(pref == code),
        role: Some(ItemRole::Language),
    };
    let mut children = vec![Node::Item(item("auto", tr(lang, "Auto"))), Node::Separator];
    children.extend(Lang::ALL.into_iter().map(|l| Node::Item(item(l.code(), l.name()))));
    Node::Submenu { label: tr(lang, "Language").into(), children, role: Some(ItemRole::Language) }
}

/// Translate menu titles, submenus and items the layout didn't name itself.
fn translate(bar: &mut MenuBar, lang: Lang) {
    fn walk(nodes: &mut [Node], lang: Lang) {
        for n in nodes {
            match n {
                Node::Item(it) if it.role.is_none() => it.label = tr_id(lang, &it.id, &it.label).to_string(),
                Node::Submenu { label, children, role } => {
                    if role.is_none() {
                        *label = tr(lang, label).to_string();
                    }
                    walk(children, lang);
                }
                _ => {}
            }
        }
    }
    for m in &mut bar.menus {
        if m.role != MenuRole::App {
            m.title = tr(lang, &m.title).to_string();
        }
        walk(&mut m.children, lang);
    }
}

/// Local Image's menus as a Mac menu bar, in the UI language; `language` is the
/// `interface.language` preference (`auto` or a code), checked in the app menu's Language.
pub fn photocraft_layout(items: &[MenuItem], lang: Lang, language: &str) -> Layout {
    photocraft_layout_impl(items, lang, language, &crate::menus::TOP_MENUS, None)
}

/// App-wide language choices, including following the operating system.
pub fn language_menu(app: &PhotocraftApp) -> Node {
    let pref = &app.session.prefs().interface.language;
    language_node(pref, Lang::from_pref(pref))
}

/// App-wide appearance choices for a host whose active module has its own command set.
/// Labels and checked state use the editor's canonical command declarations. These rows have
/// no key equivalents outside the editor, where its scoped shortcut bindings do not dispatch.
pub fn appearance_menu(app: &PhotocraftApp) -> Node {
    let lang = Lang::from_pref(&app.session.prefs().interface.language);
    let item = |id: &str, label: &str| {
        Node::Item(Item {
            id: id.into(),
            label: tr_id(lang, id, label).into(),
            shortcut: None,
            enabled: true,
            checked: crate::menus::checked(app, id),
            role: None,
        })
    };
    let mut children: Vec<_> = crate::menus::UI_COMMANDS
        .iter()
        .filter(|(id, _, path, _)| id.starts_with("window.theme.") && path.last() == Some(&"Theme"))
        .map(|(id, label, _, _)| item(id, label))
        .collect();
    if let Some((id, label, _, _)) = crate::menus::UI_COMMANDS.iter().find(|(id, _, _, _)| *id == "window.theme.toggle") {
        children.extend([Node::Separator, item(id, label)]);
    }
    Node::Submenu { label: tr(lang, "Appearance").into(), children, role: Some(ItemRole::Appearance) }
}

/// Native menus use all active bindings, including commands hidden from the menu tree, so
/// system accelerators cannot steal a customized shortcut. The current persona sets the tops.
pub fn app_layout(app: &PhotocraftApp) -> Layout {
    let lang = Lang::from_pref(&app.session.prefs().interface.language);
    li_i18n::set_current(lang);
    let mut layout = photocraft_layout_impl(
        &crate::menus::menu_items(app),
        lang,
        &app.session.prefs().interface.language,
        crate::menus::persona_menus(app),
        Some(crate::shortcut_dispatch::binding_strings(app)),
    );
    if !commands_available(app) {
        disable_commands(&mut layout.bar);
    }
    layout
}

/// Modal ownership applies equally to native menu clicks and keyboard shortcuts. Check the
/// application state as well as egui's layer at dispatch: a prompt can open after clicks queue.
pub fn commands_available(app: &PhotocraftApp) -> bool {
    app.ui.dialogs.is_empty()
        && app.camera_raw.is_none()
        && !app.develop_filter.prompting()
        && !app.develop_filter.in_session()
        && !crate::restoration_ui::is_open(app)
        && app.discard.is_none()
        && app.overwrite_prompt.is_none()
        && app.distort.liquify.is_none()
        && app.distort.gallery.is_none()
}

fn modal_owns_commands(app: &PhotocraftApp, ctx: &egui::Context) -> bool {
    !commands_available(app) || (app.draft_refine.open && ctx.embed_viewports()) || ctx.memory(|memory| memory.top_modal_layer().is_some())
}

fn disable_commands(bar: &mut MenuBar) {
    fn walk(nodes: &mut [Node]) {
        for node in nodes {
            match node {
                Node::Item(item) if item.id != HIDE && item.id != MINIMIZE => item.enabled = false,
                Node::Submenu { children, .. } => walk(children),
                _ => {}
            }
        }
    }
    for menu in &mut bar.menus {
        walk(&mut menu.children);
    }
}

fn photocraft_layout_impl(items: &[MenuItem], lang: Lang, language: &str, tops: &[&str], bindings: Option<Vec<(String, String)>>) -> Layout {
    let mut bar = from_items(tops, items);
    bar.tag_item("help.about", ItemRole::About);
    bar.tag_item("file.exit", ItemRole::Quit);
    bar.tag_submenu("Edit", "Preferences", ItemRole::Settings);
    bar.tag_submenu("Window", "Theme", ItemRole::Appearance);
    bar.tag_item("window.theme.toggle", ItemRole::Appearance);
    bar.tag_menu("Window", MenuRole::Window);
    bar.tag_menu("Help", MenuRole::Help);
    let mut layout = match bindings {
        Some(bindings) => li_commands::menu::mac_layout_with_bindings(&bar, |text| tr(lang, text).to_string(), bindings),
        None => mac_layout(&bar, lang),
    };
    // Language goes right after Settings (before Appearance), as in the other Craft apps.
    if let Some(app_menu) = layout.bar.menus.iter_mut().find(|m| m.role == MenuRole::App) {
        let settings = |n: &Node| match n {
            Node::Item(it) => it.role == Some(ItemRole::Settings),
            Node::Submenu { role, .. } => *role == Some(ItemRole::Settings),
            _ => false,
        };
        let at = app_menu.children.iter().position(settings).map_or(0, |i| i + 1);
        app_menu.children.insert(at, language_node(language, lang));
    }
    translate(&mut layout.bar, lang);
    layout
}

// The app's side

/// The native menu bar, while Local Image uses one (the in-window menus are hidden then).
pub struct NativeMenu {
    backend: Box<dyn Backend>,
    /// Hash of the state the menus were last built from.
    state: Option<u64>,
    external_state: Option<u64>,
    clicks: Vec<String>,
    /// AppKit toggled a check item itself: re-sync.
    dirty: bool,
    logged: bool,
}

impl NativeMenu {
    pub fn new(backend: Box<dyn Backend>) -> NativeMenu {
        NativeMenu { backend, state: None, external_state: None, clicks: Vec::new(), dirty: true, logged: false }
    }

    /// Native clicks awaiting dispatch by the active module's host.
    pub fn drain_clicks(&mut self) -> Vec<String> {
        std::mem::take(&mut self.clicks)
    }

    /// Display another module's shared menu model. Invalidate the editor cache so returning
    /// to Compositing always restores its own menus, even when its session did not change.
    pub fn sync_bar(&mut self, bar: &MenuBar) {
        let state = li_commands::menu::state_key(bar);
        if self.external_state != Some(state) || self.dirty {
            self.backend.sync(bar);
            self.external_state = Some(state);
        }
        self.state = None;
        self.dirty = false;
    }

    /// Before egui sees this frame's input: key equivalents become the key presses they were,
    /// and clicks wait for [`NativeMenu::run`].
    pub fn raw_input(&mut self, raw: &mut egui::RawInput) {
        for e in self.backend.drain() {
            match e {
                Event::Key(chord) => {
                    raw.events.extend(chord.key_event(true));
                    raw.events.extend(chord.key_event(false));
                }
                Event::Click(id) => self.clicks.push(id),
            }
            self.dirty = true;
        }
    }
}

/// Run the native menu clicks that arrived since the last frame, like in-window menu clicks.
pub fn run(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(menu) = app.services.native_menu.as_mut() else { return };
    let clicks = std::mem::take(&mut menu.clicks);
    for id in clicks {
        if id == MINIMIZE {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            continue;
        }
        if modal_owns_commands(app, ctx) {
            continue;
        }
        if let Err(e) = crate::menus::invoke(app, ctx, &id, serde_json::json!({})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
}

/// Bring the native menu up to date when anything it shows may have changed: a command ran, the
/// document or selection changed, or the user clicked or pressed a key. Building the rows walks
/// every command's enabled state, so it doesn't run on frames that only animate or scroll.
pub fn sync(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(menu) = app.services.native_menu.as_ref() else { return };
    let input = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. } | egui::Event::PointerButton { pressed: false, .. })));
    let modal = modal_owns_commands(app, ctx);
    let state = state_hash(app, input.then_some(app.frame), modal);
    if menu.state == Some(state) && !menu.dirty {
        return;
    }
    let mut layout = app_layout(app);
    if modal {
        disable_commands(&mut layout.bar);
    }
    let Some(menu) = app.services.native_menu.as_mut() else { return };
    if !menu.logged {
        for c in &layout.clashes {
            log::info!("menu: {} is {} on macOS and {} in Local Image; {}", c.shortcut, c.system, c.command, c.resolution);
        }
        menu.logged = true;
    }
    menu.external_state = None;
    menu.backend.sync(&layout.bar);
    menu.state = Some(state);
    menu.dirty = false;
}

/// What the menus' rows depend on, cheaply: commands run, the documents and their revisions,
/// selection, recent files, panels and view state, the language. `input` forces a change on
/// frames with a click or key press, which covers state the hash doesn't list.
fn state_hash(app: &PhotocraftApp, input: Option<u64>, modal: bool) -> u64 {
    let mut h = DefaultHasher::new();
    input.hash(&mut h);
    modal.hash(&mut h);
    commands_available(app).hash(&mut h);
    let s = &app.session;
    s.journal.len().hash(&mut h);
    s.active_index().hash(&mut h);
    s.clipboard.is_some().hash(&mut h);
    if let Some(d) = s.active() {
        (d.doc.id.0, d.revision, d.active_layer.map(|l| l.0), d.selected_layers.len(), d.isolated_layers.len()).hash(&mut h);
    }
    let ui = &app.ui;
    (&ui.recent_files, &ui.workspace, ui.palette_open, ui.transform.is_some(), ui.text_edit.is_some(), ui.dialogs.len()).hash(&mut h);
    serde_json::to_string(&(&ui.panels, &ui.extras, &ui.view, ui.theme, ui.tool)).unwrap_or_default().hash(&mut h);
    s.prefs().interface.language.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn queued_native_clicks_cannot_bypass_modal_ownership() {
        use std::sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        };
        struct Queue(Arc<Mutex<Vec<Event>>>);
        impl Backend for Queue {
            fn sync(&mut self, _: &MenuBar) {}
            fn drain(&mut self) -> Vec<Event> {
                std::mem::take(&mut *self.0.lock().unwrap())
            }
        }
        let mut app = app(true);
        let ctx = egui::Context::default();
        let opens = Arc::new(AtomicUsize::new(0));
        let count = opens.clone();
        app.services.file_dialog = Some(crate::file_dialog::answering(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            None
        }));
        let events = Arc::new(Mutex::new(vec![Event::Click("file.open".into()), Event::Click("edit.clear".into())]));
        app.services.native_menu = Some(NativeMenu::new(Box::new(Queue(events.clone()))));
        // Events can arrive before a dialog is opened, so dispatch must check fresh state.
        app.services.native_menu.as_mut().unwrap().raw_input(&mut Default::default());
        let before = app.session.active().unwrap().doc.clone();
        let id = crate::color_range_ui::open(&mut app);
        assert!(!app_layout(&app).bar.find("file.open").unwrap().enabled);
        assert!(!app_layout(&app).bar.find("edit.clear").unwrap().enabled);
        assert!(app_layout(&app).bar.find(HIDE).unwrap().enabled);
        assert!(app_layout(&app).bar.find(MINIMIZE).unwrap().enabled);
        run(&mut app, &ctx);
        assert_eq!(opens.load(Ordering::SeqCst), 0);
        assert_eq!(*app.session.active().unwrap().doc, *before);
        assert!(app.ui.dialog_mut(id).is_some());
        crate::dialogs::cancel(&mut app, id);
        // A first click in a batch can itself open a modal. Later clicks must not run under it.
        *events.lock().unwrap() = vec![Event::Click("file.new".into()), Event::Click("file.open".into()), Event::Click("edit.clear".into())];
        app.services.native_menu.as_mut().unwrap().raw_input(&mut Default::default());
        run(&mut app, &ctx);
        assert_eq!(opens.load(Ordering::SeqCst), 0);
        assert_eq!(*app.session.active().unwrap().doc, *before);
        assert_eq!(app.ui.dialogs.len(), 1);
        assert_eq!(app.ui.dialogs[0].kind, crate::state::DialogKind::NewDocument);
    }

    #[test]
    fn native_commands_are_disabled_for_custom_prompts_and_egui_modals() {
        let mut app = app(true);
        let ctx = egui::Context::default();
        crate::develop_filter_ui::open(&mut app, &ctx, None, None).unwrap();
        assert!(app.develop_filter.prompting());
        assert!(!commands_available(&app));
        assert!(!app_layout(&app).bar.find("file.open").unwrap().enabled);
        crate::develop_filter_ui::answer(&mut app, &ctx, "cancel").unwrap();
        assert!(commands_available(&app));
        // Modal layers also cover custom frontends whose state is not a generic Dialog.
        let mut output = ctx.run_ui(Default::default(), |ui| {
            egui::Modal::new(egui::Id::new("custom-test-modal")).show(ui.ctx(), |ui| {
                ui.label("Custom modal");
            });
        });
        output.textures_delta.clear();
        let mut menu = NativeMenu::new(Box::new(NoEvents));
        menu.clicks.push("file.new".into());
        app.services.native_menu = Some(menu);
        run(&mut app, &ctx);
        assert!(app.ui.dialogs.is_empty());
    }

    struct NoEvents;
    impl Backend for NoEvents {
        fn sync(&mut self, _: &MenuBar) {}
        fn drain(&mut self) -> Vec<Event> {
            Vec::new()
        }
    }

    fn app(doc: bool) -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        if doc {
            app.run("file.new", json!({"width": 64, "height": 48})).unwrap();
            app.sync_views();
        }
        app
    }

    fn layout(doc: bool) -> Layout {
        photocraft_layout(&crate::menus::menu_items(&app(doc)), Lang::En, "auto")
    }

    fn ids(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .map(|n| match n {
                Node::Item(it) => it.id.clone(),
                Node::Separator => "---".into(),
                Node::Submenu { label, .. } => format!("[{label}]"),
                Node::Standard(s) => format!("<{s:?}>"),
            })
            .collect()
    }

    #[test]
    fn j21_editor_native_adapter_retains_every_command_row() {
        let app = app(true);
        let source = crate::menus::menu_items(&app);
        let native = app_layout(&app).bar;
        let mut count = 0;
        for row in source.iter().filter(|row| row.label != "---" && row.path.first().is_some_and(|top| crate::menus::TOP_MENUS.contains(&top.as_str()))) {
            let item = native.find(&row.id).unwrap_or_else(|| panic!("lost {}", row.id));
            assert_eq!((item.enabled, item.checked), (row.enabled, row.checked), "{}", row.id);
            assert_eq!(item.shortcut, row.shortcut, "{}", row.id);
            if !matches!(row.id.as_str(), "help.about" | "file.exit") {
                assert_eq!(item.label, li_i18n::tr_id(li_i18n::current(), &row.id, &row.label), "{}", row.id);
            }
            count += 1;
        }
        assert!(count > 500, "{count} complete editor rows checked");
        eprintln!("J21 editor parity: {count} command rows retained");
    }

    #[test]
    fn j21_native_external_sync_skips_unchanged_state_and_refreshes_checks() {
        use std::cell::Cell;
        use std::rc::Rc;
        struct Counter(Rc<Cell<usize>>);
        impl Backend for Counter {
            fn sync(&mut self, _: &MenuBar) {
                self.0.set(self.0.get() + 1);
            }
            fn drain(&mut self) -> Vec<Event> {
                Vec::new()
            }
        }
        let count = Rc::new(Cell::new(0));
        let mut menu = NativeMenu::new(Box::new(Counter(count.clone())));
        let mut bar = MenuBar {
            menus: vec![Menu {
                title: "Photo".into(),
                role: MenuRole::Normal,
                children: vec![Node::Item(Item { checked: Some(false), ..Item::new("photo.rate", "Rate") })],
            }],
        };
        for _ in 0..10 {
            menu.sync_bar(&bar);
        }
        assert_eq!(count.get(), 1, "idle frames must not touch AppKit");
        if let Node::Item(item) = &mut bar.menus[0].children[0] {
            item.checked = Some(true);
        }
        menu.sync_bar(&bar);
        assert_eq!(count.get(), 2, "check changes update without rebuilding");
        menu.dirty = true;
        menu.sync_bar(&bar);
        assert_eq!(count.get(), 3, "native auto-toggled checks are reconciled");
    }

    #[test]
    fn photocraft_gets_a_mac_app_menu() {
        let l = layout(true);
        let app = &l.bar.menus[0];
        assert_eq!((app.title.as_str(), app.role), ("Local Image", MenuRole::App));
        assert_eq!(
            ids(&app.children),
            [
                "help.about",
                "---",
                "[Settings]",
                "[Language]",
                "[Appearance]",
                "---",
                "<Services>",
                "---",
                HIDE,
                "<HideOthers>",
                "<ShowAll>",
                "---",
                "file.exit"
            ]
        );
        let quit = l.bar.find("file.exit").unwrap();
        assert_eq!((quit.label.as_str(), quit.shortcut.as_deref()), ("Quit Local Image", Some("Cmd+Q")));
        assert_eq!(l.bar.find("help.about").unwrap().label, "About Local Image");
    }

    #[test]
    fn moved_items_leave_their_old_menus() {
        let l = layout(true);
        let by = |t: &str| l.bar.menus.iter().find(|m| m.title == t).unwrap();
        assert!(!ids(&by("File").children).contains(&"file.exit".to_string()));
        assert!(!ids(&by("Help").children).contains(&"help.about".to_string()));
        assert!(!ids(&by("Edit").children).contains(&"[Preferences]".to_string()));
        let titles: Vec<&str> = l.bar.menus.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles, ["Local Image", "File", "Edit", "Image", "Layer", "Type", "Select", "Filter", "View", "Window", "Help"]);
    }

    /// Local Image keeps Photoshop's ⌘H, ⌘M, ⌘W and ⌘, (Hide Layers): the system items give way.
    #[test]
    fn clashes_are_resolved_in_photocrafts_favour() {
        let l = layout(true);
        let clash = |sc: &str| l.clashes.iter().find(|c| c.shortcut == sc).map(|c| c.command.as_str());
        assert_eq!(clash("Cmd+H"), Some("view.extras"));
        assert_eq!(clash("Cmd+M"), Some("image.adjustments.curves"));
        assert_eq!(clash("Cmd+W"), Some("file.close"));
        assert_eq!(clash("Cmd+,"), Some("layer.hideLayers"));
        assert_eq!(l.bar.find(HIDE).unwrap().shortcut.as_deref(), Some("Ctrl+Cmd+H"), "as in Photoshop");
        assert_eq!(l.bar.find(MINIMIZE).unwrap().shortcut.as_deref(), Some("Ctrl+Cmd+M"));
        let window = l.bar.menu(MenuRole::Window).unwrap();
        assert_eq!(ids(&window.children)[..3], [MINIMIZE.to_string(), "<Zoom>".into(), "---".into()]);
        assert_eq!(ids(&window.children).last().unwrap(), "<BringAllToFront>");
        let file = l.bar.menus.iter().find(|m| m.title == "File").unwrap();
        assert!(!file.children.contains(&Node::Standard(Standard::CloseWindow)), "⌘W stays File › Close");
    }

    /// Themes and Next Theme move from Window to the app menu's Appearance, checked by the theme.
    #[test]
    fn themes_move_to_the_app_menus_appearance() {
        let mut a = app(false);
        a.ui.theme = crate::theme::ThemeKind::Classic;
        let l = photocraft_layout(&crate::menus::menu_items(&a), Lang::En, "auto");
        let app_menu = l.bar.menu(MenuRole::App).unwrap();
        let Some(Node::Submenu { children, .. }) = app_menu.children.iter().find(|n| matches!(n, Node::Submenu { label, .. } if label == "Appearance")) else {
            panic!("an Appearance submenu");
        };
        let themes = ids(children);
        assert_eq!(themes.last().map(String::as_str), Some("window.theme.toggle"));
        assert!(themes.contains(&"window.theme.pro".to_string()) && themes.contains(&"window.theme.classic".to_string()));
        assert_eq!(l.bar.find("window.theme.classic").unwrap().checked, Some(true));
        assert_eq!(l.bar.find("window.theme.pro").unwrap().checked, Some(false));
        assert_eq!(l.bar.find("window.theme.toggle").unwrap().label, "Next Theme");
        let window = ids(&l.bar.menu(MenuRole::Window).unwrap().children);
        assert!(!window.iter().any(|id| id.starts_with("window.theme.") || id == "[Theme]"), "{window:?}");
    }

    /// Language follows Settings in the app menu: Auto and every UI language in its own name,
    /// the preference checked; choosing one sets `interface.language`.
    #[test]
    fn the_app_menu_switches_the_ui_language() {
        let l = photocraft_layout(&crate::menus::menu_items(&app(false)), Lang::En, "ja");
        let app_menu = l.bar.menu(MenuRole::App).unwrap();
        let Some(Node::Submenu { children, .. }) = app_menu.children.iter().find(|n| matches!(n, Node::Submenu { label, .. } if label == "Language")) else {
            panic!("a Language submenu");
        };
        assert_eq!(children.iter().filter(|n| matches!(n, Node::Item(_))).count(), Lang::ALL.len() + 1);
        assert_eq!(l.bar.find("app.language.ja").map(|it| (it.label.as_str(), it.checked)), Some(("日本語", Some(true))));
        assert_eq!(l.bar.find("app.language.auto").and_then(|it| it.checked), Some(false));
        // Native names stay native in another UI language.
        let de = photocraft_layout(&crate::menus::menu_items(&app(false)), Lang::from_pref("de"), "de");
        assert_eq!(de.bar.find("app.language.ja").map(|it| it.label.as_str()), Some("日本語"));
        let mut a = app(false);
        let ctx = egui::Context::default();
        crate::menus::invoke(&mut a, &ctx, "app.language.fr", json!({})).unwrap();
        assert_eq!(a.session.prefs().interface.language, "fr");
        crate::menus::invoke(&mut a, &ctx, "app.language.auto", json!({})).unwrap();
        assert_eq!(a.session.prefs().interface.language, "auto");
        assert!(crate::menus::invoke(&mut a, &ctx, "app.language.xx-nope", json!({})).is_err(), "an unknown code is an error");
    }

    /// The menus show the shortcut that runs each command (Undo ⌘Z, Copy ⌘C were blank).
    #[test]
    fn menu_rows_show_the_shortcut_that_runs_them() {
        let l = layout(true);
        let sc = |id: &str| l.bar.find(id).and_then(|it| it.shortcut.clone());
        assert_eq!(sc("edit.undo").as_deref(), Some("Cmd+Z"));
        assert_eq!(sc("edit.copy").as_deref(), Some("Cmd+C"));
        assert_eq!(sc("edit.paste").as_deref(), Some("Cmd+V"));
        assert_eq!(sc("layer.hideLayers").as_deref(), Some("Cmd+,"));
        let app = app(true);
        let bindings = crate::shortcut_dispatch::bindings(&app);
        for it in crate::menus::menu_items(&app).iter().filter(|it| it.enabled) {
            let Some(shown) = it.shortcut.as_deref().and_then(li_keys::input::parse) else { continue };
            let runs = bindings.iter().find(|(_, b)| *b == shown).map(|(id, _)| id.as_str());
            assert!(runs.is_some(), "{} shows {:?}, which runs nothing", it.id, it.shortcut);
        }
    }

    /// A native menu can't turn a plain item into a check item in place: opening the first
    /// document must not change any item's kind, or the whole menu is rebuilt.
    #[test]
    fn opening_a_document_changes_state_not_structure() {
        let (a, b) = (crate::menus::menu_items(&app(false)), crate::menus::menu_items(&app(true)));
        let kind = |items: &[MenuItem]| items.iter().map(|i| (i.id.clone(), i.checked.is_some())).collect::<HashMap<_, _>>();
        let (ka, kb) = (kind(&a), kind(&b));
        let flips: Vec<&String> = ka.iter().filter(|(id, k)| kb.get(*id).is_some_and(|o| o != *k)).map(|(id, _)| id).collect();
        assert!(flips.is_empty(), "items that change kind: {flips:?}");
        assert_eq!(structure_key(&layout(false).bar), structure_key(&layout(true).bar));
    }

    #[test]
    fn native_key_equivalents_are_command_and_function_keys_only() {
        assert!(Chord::parse("Cmd+B").unwrap().native_ok());
        assert!(Chord::parse("F5").unwrap().native_ok());
        assert!(!Chord::parse("B").unwrap().native_ok());
        assert!(!Chord::parse("Alt+F").unwrap().native_ok());
        assert!(!Chord::parse("Shift+X").unwrap().native_ok());
        assert_eq!(Chord::parse("Ctrl+Cmd+M").unwrap().portable(), "Ctrl+Cmd+M");
        assert_eq!(Chord::parse("Cmd++").unwrap().key, "+");
    }

    /// Every menu shortcut is written so it parses and turns back into the key press egui gets.
    #[test]
    fn every_shortcut_becomes_a_key_press() {
        for it in layout(true).bar.items() {
            let Some(sc) = &it.shortcut else { continue };
            let c = Chord::parse(sc).unwrap_or_else(|| panic!("{sc} on {} doesn't parse", it.id));
            assert_eq!(&c.portable(), sc, "{} writes its modifiers in another order", it.id);
            let Some(egui::Event::Key { key, modifiers, .. }) = c.key_event(true) else { panic!("{sc} on {} has no egui key", it.id) };
            let parsed = li_keys::input::parse(sc).unwrap();
            assert!(li_keys::input::key_matches(&parsed, key, modifiers), "{sc} on {} doesn't match its own key press", it.id);
        }
    }

    fn frame(ctx: &egui::Context, raw: egui::RawInput, f: impl FnMut(&mut egui::Ui)) {
        let mut out = ctx.run_ui(raw, f);
        out.textures_delta.clear();
    }

    struct Fake {
        synced: std::rc::Rc<std::cell::RefCell<Vec<MenuBar>>>,
        events: Vec<Event>,
    }

    impl Backend for Fake {
        fn sync(&mut self, bar: &MenuBar) {
            self.synced.borrow_mut().push(bar.clone());
        }
        fn drain(&mut self) -> Vec<Event> {
            std::mem::take(&mut self.events)
        }
    }

    /// A key equivalent reaches Local Image's own key handling as the key press it was, so the
    /// shortcut runs once, through the same rules as without a native menu.
    #[test]
    fn key_equivalents_run_through_the_normal_shortcut_path() {
        let synced = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut app = app(true);
        let ctx = egui::Context::default();
        let rulers = Chord::parse("Cmd+R").unwrap();
        app.services.native_menu = Some(NativeMenu::new(Box::new(Fake { synced: synced.clone(), events: vec![Event::Key(rulers)] })));
        let before = app.ui.extras.rulers;
        let mut raw = egui::RawInput::default();
        if let Some(m) = app.services.native_menu.as_mut() {
            m.raw_input(&mut raw);
        }
        assert_eq!(raw.events.len(), 2, "a press and a release");
        frame(&ctx, raw, |ui| {
            run(&mut app, ui.ctx());
            crate::shortcuts::handle(&mut app, ui.ctx());
            sync(&mut app, ui.ctx());
        });
        assert_eq!(app.ui.extras.rulers, !before, "⌘R toggled View › Rulers once");
        assert_eq!(synced.borrow().len(), 1, "the menu was built");
        // Nothing changed: no rebuild of the rows on the next frame.
        frame(&ctx, egui::RawInput::default(), |ui| sync(&mut app, ui.ctx()));
        assert_eq!(synced.borrow().len(), 1);
    }

    #[test]
    fn clicks_run_the_command_and_resync() {
        let synced = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut app = app(true);
        let ctx = egui::Context::default();
        app.services.native_menu = Some(NativeMenu::new(Box::new(Fake { synced: synced.clone(), events: vec![Event::Click("window.panel.layers".into())] })));
        let before = app.ui.panels.layers;
        let mut raw = egui::RawInput::default();
        if let Some(m) = app.services.native_menu.as_mut() {
            m.raw_input(&mut raw);
        }
        frame(&ctx, raw, |ui| {
            run(&mut app, ui.ctx());
            sync(&mut app, ui.ctx());
        });
        assert_eq!(app.ui.panels.layers, !before);
        let bar = synced.borrow().last().cloned().unwrap();
        assert_eq!(bar.find("window.panel.layers").unwrap().checked, Some(!before));
    }

    /// With the Mac menu bar the title bar draws no menu titles: each title shows once fewer.
    #[test]
    fn the_title_bar_hides_its_menus_with_the_mac_menu_bar() {
        use egui_kittest::{Harness, kittest::Queryable};
        let counts = |native: bool| -> Vec<usize> {
            let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_max_steps(64).build_eframe(move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let mut app = app(true);
                if native {
                    app.services.native_menu = Some(NativeMenu::new(Box::new(Fake { synced: Default::default(), events: Vec::new() })));
                }
                app
            });
            h.run_steps(4);
            crate::menus::TOP_MENUS.iter().map(|t| h.query_all_by_label(t).count()).collect()
        };
        let (in_window, native) = (counts(false), counts(true));
        for (i, title) in crate::menus::TOP_MENUS.iter().enumerate() {
            assert_eq!(native[i] + 1, in_window[i], "{title}: {} in-window, {} with the Mac menu bar", in_window[i], native[i]);
        }
    }
}
