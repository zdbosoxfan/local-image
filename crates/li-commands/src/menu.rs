//! Shared command menu models and platform layout policy.
//!
//! Product shells provide command availability and translated labels. The native tree, macOS
//! role placement, accelerator conflict handling and rebuild identity stay identical across them.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use serde::Serialize;
use serde_json::Value;

/// Menu tree entry for rendering and for `ui.inspect`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    pub path: Vec<String>,
    pub shortcut: Option<String>,
    pub enabled: bool,
    pub checked: Option<bool>,
    /// Edit › Menus colour (red, orange, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommandMenuNode {
    Item {
        id: String,
        #[serde(skip_serializing_if = "Value::is_null")]
        params: Value,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        shortcut: Option<String>,
        enabled: bool,
        /// `Some` for toggles and radio-style choices.
        #[serde(skip_serializing_if = "Option::is_none")]
        checked: Option<bool>,
    },
    Separator,
    Submenu {
        label: String,
        children: Vec<CommandMenuNode>,
    },
}

impl CommandMenuNode {
    /// `id|params` — a stable key for one item (native menu ids).
    pub fn key(id: &str, params: &Value) -> String {
        if params.is_null() { id.to_string() } else { format!("{id}|{params}") }
    }
}

pub const APP_NAME: &str = "Local Image";

#[derive(Clone, Debug, Default, PartialEq, Hash)]
pub struct MenuBar {
    pub menus: Vec<Menu>,
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Menu {
    pub title: String,
    pub role: MenuRole,
    pub children: Vec<Node>,
}

/// What a top-level menu is for. macOS treats the app, Window and Help menus specially.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MenuRole {
    #[default]
    Normal,
    App,
    Window,
    Help,
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub enum Node {
    Item(Item),
    Separator,
    Submenu {
        label: String,
        children: Vec<Node>,
        role: Option<ItemRole>,
    },
    /// An item AppKit provides and runs itself (Services, Zoom, …).
    Standard(Standard),
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Item {
    /// The command id (`file.open`), or one of [`HIDE`] / [`MINIMIZE`].
    pub id: String,
    pub label: String,
    /// `Cmd+Shift+Z` form; `Ctrl` is the real Control key.
    pub shortcut: Option<String>,
    pub enabled: bool,
    /// `Some` for check items.
    pub checked: Option<bool>,
    pub role: Option<ItemRole>,
}

/// Items macOS expects in fixed places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemRole {
    About,
    Settings,
    Quit,
    Hide,
    Minimize,
    /// Window › Theme and Next Theme, which become the app menu's Appearance submenu.
    Appearance,
    /// The app menu's Language submenu (its labels are final, never translated by id).
    Language,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Standard {
    Services,
    HideOthers,
    ShowAll,
    Minimize,
    Zoom,
    BringAllToFront,
    CloseWindow,
}

/// Hide Local Image: a custom item (the predefined one always takes ⌘H, which is View › Extras).
pub const HIDE: &str = "app.hide";
/// Minimize: a custom item (the predefined one always takes ⌘M, which is Curves).
pub const MINIMIZE: &str = "window.minimize";

impl Item {
    pub fn new(id: &str, label: &str) -> Item {
        Item { id: id.into(), label: label.into(), shortcut: None, enabled: true, checked: None, role: None }
    }
}

impl MenuBar {
    /// Every item, depth first.
    pub fn items(&self) -> Vec<&Item> {
        fn walk<'a>(nodes: &'a [Node], out: &mut Vec<&'a Item>) {
            for n in nodes {
                match n {
                    Node::Item(it) => out.push(it),
                    Node::Submenu { children, .. } => walk(children, out),
                    Node::Separator | Node::Standard(_) => {}
                }
            }
        }
        let mut out = Vec::new();
        for m in &self.menus {
            walk(&m.children, &mut out);
        }
        out
    }

    pub fn find(&self, id: &str) -> Option<&Item> {
        self.items().into_iter().find(|it| it.id == id)
    }

    pub fn menu(&self, role: MenuRole) -> Option<&Menu> {
        self.menus.iter().find(|m| m.role == role)
    }

    pub fn tag_item(&mut self, id: &str, role: ItemRole) {
        fn walk(nodes: &mut [Node], id: &str, role: ItemRole) {
            for n in nodes {
                match n {
                    Node::Item(it) if it.id == id => it.role = Some(role),
                    Node::Submenu { children, .. } => walk(children, id, role),
                    _ => {}
                }
            }
        }
        for m in &mut self.menus {
            walk(&mut m.children, id, role);
        }
    }

    pub fn tag_submenu(&mut self, top: &str, name: &str, role: ItemRole) {
        let Some(menu) = self.menus.iter_mut().find(|m| m.title == top) else { return };
        for n in &mut menu.children {
            if let Node::Submenu { label, role: r, .. } = n
                && label == name
            {
                *r = Some(role);
            }
        }
    }

    pub fn tag_menu(&mut self, title: &str, role: MenuRole) {
        if let Some(m) = self.menus.iter_mut().find(|m| m.title == title) {
            m.role = role;
        }
    }
}

/// The menus as a tree, from the flat rows [`MenuItem`] returns, in the order the
/// in-window bar draws them: at each depth leaves and separators in order, and a submenu where its
/// first child is.
pub fn from_items(tops: &[&str], items: &[MenuItem]) -> MenuBar {
    fn level(rows: &[&MenuItem], depth: usize) -> Vec<Node> {
        let mut out = Vec::new();
        let mut shown: Vec<&str> = Vec::new();
        for it in rows {
            if it.path.len() == depth {
                if it.label == "---" {
                    out.push(Node::Separator);
                } else {
                    out.push(Node::Item(Item {
                        id: it.id.clone(),
                        label: it.label.clone(),
                        shortcut: it.shortcut.clone(),
                        enabled: it.enabled,
                        checked: it.checked,
                        role: None,
                    }));
                }
            } else if let Some(name) = it.path.get(depth) {
                let name = name.as_str();
                if shown.contains(&name) {
                    continue;
                }
                shown.push(name);
                let child: Vec<&MenuItem> = rows.iter().copied().filter(|c| c.path.get(depth).map(String::as_str) == Some(name)).collect();
                out.push(Node::Submenu { label: name.to_string(), children: level(&child, depth + 1), role: None });
            }
        }
        tidy(out)
    }
    let menus = tops
        .iter()
        .map(|top| {
            let rows: Vec<&MenuItem> = items.iter().filter(|i| i.path.first().map(String::as_str) == Some(*top)).collect();
            Menu { title: top.to_string(), role: MenuRole::Normal, children: level(&rows, 1) }
        })
        .collect();
    MenuBar { menus }
}

/// Drop separators at the start or end of a level and collapse runs of them, at every depth.
fn tidy(nodes: Vec<Node>) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::with_capacity(nodes.len());
    for n in nodes {
        let n = match n {
            Node::Submenu { label, children, role } => Node::Submenu { label, children: tidy(children), role },
            other => other,
        };
        if matches!(n, Node::Separator) && matches!(out.last(), None | Some(Node::Separator)) {
            continue;
        }
        out.push(n);
    }
    while matches!(out.last(), Some(Node::Separator)) {
        out.pop();
    }
    out
}

/// A hash of everything a native menu can only change by rebuilding: menus, roles, item ids,
/// kinds (plain or check), shortcuts and submenu labels. Labels, enabled and checked are updated
/// in place.
pub fn structure_key(bar: &MenuBar) -> u64 {
    fn walk(nodes: &[Node], h: &mut DefaultHasher) {
        for n in nodes {
            match n {
                Node::Item(it) => ('i', &it.id, it.checked.is_some(), &it.shortcut, it.role).hash(h),
                Node::Separator => '-'.hash(h),
                Node::Submenu { label, children, role } => {
                    ('[', label, role).hash(h);
                    walk(children, h);
                    ']'.hash(h);
                }
                Node::Standard(s) => ('s', s).hash(h),
            }
        }
    }
    let mut h = DefaultHasher::new();
    for m in &bar.menus {
        (&m.title, m.role).hash(&mut h);
        walk(&m.children, &mut h);
    }
    h.finish()
}

/// A hash of the complete menu model, including labels, enabled state and check marks.
/// Hosts may use this to avoid sending an identical live model to the native backend.
pub fn state_key(bar: &MenuBar) -> u64 {
    let mut hash = DefaultHasher::new();
    bar.hash(&mut hash);
    hash.finish()
}

// shortcuts

use li_keys::Chord;

// macOS layout

/// A system key equivalent Local Image already uses for something else.
#[derive(Clone, Debug, PartialEq)]
pub struct Clash {
    pub shortcut: &'static str,
    /// What macOS uses it for.
    pub system: &'static str,
    /// The Local Image command bound to it.
    pub command: String,
    /// What the layout did about it.
    pub resolution: &'static str,
}

pub struct Layout {
    pub bar: MenuBar,
    pub clashes: Vec<Clash>,
}

/// Turn Local Image's menus (tagged with roles) into a Mac menu bar. See the module docs.
pub fn mac_layout(bar: &MenuBar, translate: impl Fn(&str) -> String) -> Layout {
    let bound = bar.items().into_iter().filter_map(|it| Some((Chord::parse(it.shortcut.as_deref()?)?.portable(), it.id.clone()))).collect();
    mac_layout_impl(bar, translate, bound, false)
}

/// Lay out menus using the complete effective command shortcuts, including commands that have
/// no visible menu row. Each binding is `(command_id, shortcut)`. This prevents system items
/// from stealing hidden commands' keys and preserves explicit Settings/Quit remaps or unbinding.
/// The provided bindings are authoritative; their command ids match the role-tagged source rows.
pub fn mac_layout_with_bindings(bar: &MenuBar, translate: impl Fn(&str) -> String, bindings: impl IntoIterator<Item = (String, String)>) -> Layout {
    let bound = bindings.into_iter().filter_map(|(id, shortcut)| Some((Chord::parse(&shortcut)?.portable(), id))).collect();
    mac_layout_impl(bar, translate, bound, true)
}

fn mac_layout_impl(bar: &MenuBar, translate: impl Fn(&str) -> String, bound: HashMap<String, String>, preserve_command_shortcuts: bool) -> Layout {
    let mut bar = bar.clone();
    let mut clashes = Vec::new();
    let taken = |sc: &str, own: Option<&str>| bound.get(sc).filter(|cmd| Some(cmd.as_str()) != own).cloned();
    let named = |s: &str| translate(s).replace("{app}", APP_NAME);

    let about = take(&mut bar, |n| matches!(n, Node::Item(it) if it.role == Some(ItemRole::About)));
    let settings = take(&mut bar, |n| match n {
        Node::Item(it) => it.role == Some(ItemRole::Settings),
        Node::Submenu { role, .. } => *role == Some(ItemRole::Settings),
        _ => false,
    });
    let quit = take(&mut bar, |n| matches!(n, Node::Item(it) if it.role == Some(ItemRole::Quit)));
    let language = take(&mut bar, |n| matches!(n, Node::Submenu { role, .. } if *role == Some(ItemRole::Language)));
    let appearance = appearance_node(&mut bar, &translate);

    let mut menu = Vec::new();
    if let Some(Node::Item(mut it)) = about {
        it.label = named("About {app}");
        menu.extend([Node::Item(it), Node::Separator]);
    }
    if let Some(s) = settings {
        let own = node_ids(&s);
        let comma = bound.get("Cmd+,").filter(|cmd| !own.contains(*cmd)).cloned();
        menu.push(settings_node(s, comma, &translate, preserve_command_shortcuts, &mut clashes));
    }
    menu.extend(language);
    menu.extend(appearance);
    if !menu.is_empty() && menu.last() != Some(&Node::Separator) {
        menu.push(Node::Separator);
    }
    menu.extend([Node::Standard(Standard::Services), Node::Separator]);
    let mut hide = Item::new(HIDE, &named("Hide {app}"));
    hide.role = Some(ItemRole::Hide);
    match taken("Cmd+H", None) {
        Some(command) => {
            let free = taken("Ctrl+Cmd+H", None).is_none();
            if free {
                hide.shortcut = Some("Ctrl+Cmd+H".into());
            }
            let resolution = if free { "Hide moves to ⌃⌘H, as in Photoshop" } else { "Hide has no shortcut" };
            clashes.push(Clash { shortcut: "Cmd+H", system: "Hide", command, resolution });
        }
        None => hide.shortcut = Some("Cmd+H".into()),
    }
    menu.push(Node::Item(hide));
    let hide_others = taken("Cmd+Alt+H", None);
    if let Some(command) = &hide_others {
        let resolution = if preserve_command_shortcuts { "no system Hide Others item" } else { "AppKit's Hide Others keeps ⌥⌘H" };
        clashes.push(Clash { shortcut: "Cmd+Alt+H", system: "Hide Others", command: command.clone(), resolution });
    }
    if hide_others.is_none() || !preserve_command_shortcuts {
        menu.push(Node::Standard(Standard::HideOthers));
    }
    menu.extend([Node::Standard(Standard::ShowAll), Node::Separator]);
    if let Some(Node::Item(mut quit)) = quit {
        quit.label = named("Quit {app}");
        if !preserve_command_shortcuts {
            match taken("Cmd+Q", Some(&quit.id)) {
                Some(command) => {
                    clashes.push(Clash { shortcut: "Cmd+Q", system: "Quit", command, resolution: "Quit has no shortcut" });
                    quit.shortcut = None;
                }
                None => quit.shortcut = Some("Cmd+Q".into()),
            }
        }
        menu.push(Node::Item(quit));
    }
    bar.menus.insert(0, Menu { title: APP_NAME.into(), role: MenuRole::App, children: menu });

    match taken("Cmd+W", None) {
        Some(command) => clashes.push(Clash { shortcut: "Cmd+W", system: "Close Window", command, resolution: "no system Close Window item" }),
        None => {
            if let Some(file) = bar.menus.iter_mut().find(|m| m.title == "File") {
                file.children.extend([Node::Separator, Node::Standard(Standard::CloseWindow)]);
            }
        }
    }

    if bar.menu(MenuRole::Window).is_none() {
        let at = bar.menus.iter().position(|m| m.role == MenuRole::Help).unwrap_or(bar.menus.len());
        bar.menus.insert(at, Menu { title: "Window".into(), role: MenuRole::Window, children: Vec::new() });
    }
    let minimize = match taken("Cmd+M", None) {
        Some(command) => {
            let mut it = Item::new(MINIMIZE, &translate("Minimize"));
            it.role = Some(ItemRole::Minimize);
            let free = taken("Ctrl+Cmd+M", None).is_none();
            if free {
                it.shortcut = Some("Ctrl+Cmd+M".into());
            }
            let resolution = if free { "Minimize moves to ⌃⌘M, as in Photoshop" } else { "Minimize has no shortcut" };
            clashes.push(Clash { shortcut: "Cmd+M", system: "Minimize", command, resolution });
            Node::Item(it)
        }
        None => Node::Standard(Standard::Minimize),
    };
    if let Some(window) = bar.menus.iter_mut().find(|m| m.role == MenuRole::Window) {
        let mut children = vec![minimize, Node::Standard(Standard::Zoom), Node::Separator];
        children.append(&mut window.children);
        children.extend([Node::Separator, Node::Standard(Standard::BringAllToFront)]);
        window.children = children;
    }

    for m in &mut bar.menus {
        m.children = tidy(std::mem::take(&mut m.children));
    }
    // A menu emptied by the moves goes away (the Help menu stays: it has the search field).
    bar.menus.retain(|m| !m.children.is_empty() || m.role == MenuRole::Help);
    Layout { bar, clashes }
}

/// Settings in the app menu: Edit › Preferences, a submenu of settings pages, becomes "Settings"
/// with ⌘, on its first page unless Local Image binds ⌘, elsewhere.
fn settings_node(
    node: Node,
    comma_taken: Option<String>,
    translate: &impl Fn(&str) -> String,
    preserve_command_shortcuts: bool,
    clashes: &mut Vec<Clash>,
) -> Node {
    if let Some(command) = &comma_taken {
        clashes.push(Clash { shortcut: "Cmd+,", system: "Settings", command: command.clone(), resolution: "Settings has no shortcut" });
    }
    let give = |it: &mut Item| {
        if !preserve_command_shortcuts && comma_taken.is_none() && it.shortcut.is_none() {
            it.shortcut = Some("Cmd+,".into());
        }
    };
    match node {
        Node::Item(mut it) => {
            it.label = translate("Settings…");
            give(&mut it);
            Node::Item(it)
        }
        Node::Submenu { mut children, role, .. } => {
            if let Some(Node::Item(first)) = children.iter_mut().find(|n| matches!(n, Node::Item(i) if i.enabled)) {
                give(first);
            }
            Node::Submenu { label: translate("Settings"), children, role }
        }
        other => other,
    }
}

/// Appearance in the app menu: the themes (Window › Theme, tagged [`ItemRole::Appearance`]), then
/// Next Theme, both leaving the Window menu. None when there are no themes.
fn appearance_node(bar: &mut MenuBar, translate: &impl Fn(&str) -> String) -> Option<Node> {
    let Some(Node::Submenu { mut children, .. }) = take(bar, |n| matches!(n, Node::Submenu { role, .. } if *role == Some(ItemRole::Appearance))) else {
        return None;
    };
    if let Some(Node::Item(mut next)) = take(bar, |n| matches!(n, Node::Item(it) if it.role == Some(ItemRole::Appearance))) {
        // Placed now: translated by id like the themes above it.
        next.role = None;
        children.extend([Node::Separator, Node::Item(next)]);
    }
    Some(Node::Submenu { label: translate("Appearance"), children, role: Some(ItemRole::Appearance) })
}

fn node_ids(node: &Node) -> Vec<String> {
    match node {
        Node::Item(it) => vec![it.id.clone()],
        Node::Submenu { children, .. } => children.iter().flat_map(node_ids).collect(),
        _ => Vec::new(),
    }
}

/// Remove and return the first node matching `pred`, at any depth.
fn take(bar: &mut MenuBar, pred: impl Fn(&Node) -> bool + Copy) -> Option<Node> {
    fn walk(nodes: &mut Vec<Node>, pred: impl Fn(&Node) -> bool + Copy) -> Option<Node> {
        if let Some(i) = nodes.iter().position(pred) {
            return Some(nodes.remove(i));
        }
        nodes.iter_mut().find_map(|n| match n {
            Node::Submenu { children, .. } => walk(children, pred),
            _ => None,
        })
    }
    bar.menus.iter_mut().find_map(|m| walk(&mut m.children, pred))
}

/// What the native menu reports.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// An item was chosen with the mouse (or the keyboard inside an open menu).
    Click(String),
    /// A key equivalent was pressed: the shortcut, to be handled as the key press it was.
    Key(Chord),
}

/// A platform's native menu bar (the desktop app implements it for macOS with muda).
pub trait Backend {
    /// Bring the menu in step with `bar`: update labels, enabled and checked in place, or
    /// rebuild when its structure changed.
    fn sync(&mut self, bar: &MenuBar);
    /// Events since the last call. Platform items (Hide, Services, Zoom, …) are handled by the
    /// backend and not reported.
    fn drain(&mut self) -> Vec<Event>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(id: &str, path: &[&str], shortcut: Option<&str>) -> MenuItem {
        MenuItem {
            id: id.into(),
            label: id.into(),
            path: path.iter().map(|s| (*s).into()).collect(),
            shortcut: shortcut.map(str::to_string),
            enabled: true,
            checked: None,
            color: None,
            tooltip: None,
        }
    }

    fn tagged_bar() -> MenuBar {
        let mut bar = from_items(
            &["File", "Edit", "View", "Window", "Help"],
            &[
                row("open", &["File"], Some("Cmd+O")),
                row("quit", &["File"], None),
                row("settings", &["Edit", "Preferences"], None),
                row("extras", &["View"], Some("Cmd+H")),
                row("curves", &["View"], Some("Cmd+M")),
                row("close", &["File"], Some("Cmd+W")),
                row("theme", &["Window", "Theme"], None),
                row("next-theme", &["Window"], None),
                row("about", &["Help"], None),
            ],
        );
        bar.tag_item("about", ItemRole::About);
        bar.tag_item("quit", ItemRole::Quit);
        bar.tag_submenu("Edit", "Preferences", ItemRole::Settings);
        bar.tag_submenu("Window", "Theme", ItemRole::Appearance);
        bar.tag_item("next-theme", ItemRole::Appearance);
        bar.tag_menu("Window", MenuRole::Window);
        bar.tag_menu("Help", MenuRole::Help);
        bar
    }

    #[test]
    fn shared_menu_duplicate_command_presentations_keep_stable_positions() {
        let first = row("dialog.export", &["File"], Some("Cmd+E"));
        let second = MenuItem { label: "Custom…".into(), path: vec!["File".into(), "Presets".into()], shortcut: None, ..first.clone() };
        let mut bar = from_items(&["File"], &[first, second]);
        let before = bar.items();
        assert_eq!((before[0].id.as_str(), before[0].shortcut.as_deref()), ("dialog.export", Some("Cmd+E")));
        assert_eq!((before[1].id.as_str(), before[1].label.as_str(), before[1].shortcut.as_deref()), ("dialog.export", "Custom…", None));
        let structure = structure_key(&bar);
        if let Node::Submenu { children, .. } = &mut bar.menus[0].children[1]
            && let Node::Item(item) = &mut children[0]
        {
            item.label = "Custom translated…".into();
            item.enabled = false;
        }
        assert_eq!(structure_key(&bar), structure);
        let live = bar.items();
        assert_eq!((live[0].label.as_str(), live[0].enabled), ("dialog.export", true));
        assert_eq!((live[1].label.as_str(), live[1].enabled), ("Custom translated…", false));
    }

    #[test]
    fn shared_menu_flat_tree_preserves_order_and_cleans_separators() {
        let mut leading = row("separator", &["File"], None);
        leading.label = "---".into();
        let mut nested = leading.clone();
        nested.path.push("Recent".into());
        let bar = from_items(
            &["File"],
            &[
                leading.clone(),
                row("open", &["File"], None),
                leading.clone(),
                leading.clone(),
                nested.clone(),
                row("recent-first", &["File", "Recent"], None),
                nested,
                row("close", &["File"], None),
                row("recent-second", &["File", "Recent"], None),
                leading,
            ],
        );
        let children = &bar.menus[0].children;
        assert_eq!(children.len(), 4);
        assert!(matches!(&children[0], Node::Item(it) if it.id == "open"));
        assert_eq!(children[1], Node::Separator);
        assert!(matches!(&children[2], Node::Submenu { label, children, .. }
            if label == "Recent" && children.len() == 3
            && matches!(&children[0], Node::Item(it) if it.id == "recent-first")
            && children[1] == Node::Separator
            && matches!(&children[2], Node::Item(it) if it.id == "recent-second")));
        assert!(matches!(&children[3], Node::Item(it) if it.id == "close"));
        assert_eq!(bar.items().iter().map(|it| it.id.as_str()).collect::<Vec<_>>(), ["open", "recent-first", "recent-second", "close"]);
    }

    #[test]
    fn shared_menu_mac_role_and_shortcut_golden() {
        let source = tagged_bar();
        let layout = mac_layout(&source, str::to_string);
        assert_eq!(source.menus[0].children.len(), 3, "layout leaves its input unchanged");
        let app = layout.bar.menu(MenuRole::App).unwrap();
        let signature: Vec<_> = app
            .children
            .iter()
            .map(|node| match node {
                Node::Item(it) => it.id.clone(),
                Node::Separator => "---".into(),
                Node::Submenu { label, .. } => format!("[{label}]"),
                Node::Standard(role) => format!("<{role:?}>"),
            })
            .collect();
        assert_eq!(signature, ["about", "---", "[Settings]", "[Appearance]", "---", "<Services>", "---", HIDE, "<HideOthers>", "<ShowAll>", "---", "quit"]);
        assert_eq!(layout.bar.find("about").unwrap().label, "About Local Image");
        assert_eq!(layout.bar.find("quit").unwrap().shortcut.as_deref(), Some("Cmd+Q"));
        assert_eq!(layout.bar.find("settings").unwrap().shortcut.as_deref(), Some("Cmd+,"));
        assert_eq!(layout.bar.find(HIDE).unwrap().shortcut.as_deref(), Some("Ctrl+Cmd+H"));
        assert_eq!(layout.bar.find(MINIMIZE).unwrap().shortcut.as_deref(), Some("Ctrl+Cmd+M"));
        assert_eq!(
            layout.clashes.iter().map(|c| (c.shortcut, c.command.as_str())).collect::<Vec<_>>(),
            [("Cmd+H", "extras"), ("Cmd+W", "close"), ("Cmd+M", "curves")]
        );
        assert!(layout.bar.menu(MenuRole::Help).unwrap().children.is_empty(), "Help retains system search");
        assert!(layout.bar.menus.iter().all(|menu| !menu.children.contains(&Node::Separator)
            || (!matches!(menu.children.first(), Some(Node::Separator)) && !matches!(menu.children.last(), Some(Node::Separator)))));
    }

    #[test]
    fn shared_menu_language_moves_after_settings_preserving_native_names() {
        let mut source = tagged_bar();
        let language = Node::Submenu {
            label: "Language".into(),
            role: Some(ItemRole::Language),
            children: vec![Node::Item(Item { checked: Some(true), role: Some(ItemRole::Language), ..Item::new("app.language.ja", "日本語") })],
        };
        source.menus[1].children.push(language.clone());
        let layout = mac_layout(&source, str::to_string);
        let children = &layout.bar.menu(MenuRole::App).unwrap().children;
        assert!(matches!(&children[2], Node::Submenu { label, .. } if label == "Settings"));
        assert_eq!(children[3], language);
        assert!(matches!(&children[4], Node::Submenu { label, .. } if label == "Appearance"));
        assert_eq!(layout.bar.find("app.language.ja").unwrap().label, "日本語");
        assert_eq!(layout.bar.find("app.language.ja").unwrap().checked, Some(true));
        assert!(
            !layout
                .bar
                .menus
                .iter()
                .any(|m| m.role != MenuRole::App && m.children.iter().any(|node| matches!(node, Node::Submenu { role: Some(ItemRole::Language), .. })))
        );
    }

    #[test]
    fn shared_menu_occupied_fallbacks_never_steal_accelerators() {
        let mut source = tagged_bar();
        source.menus[0].children.extend([
            Node::Item(Item { shortcut: Some("Ctrl+Cmd+H".into()), ..Item::new("other-hide", "Other hide") }),
            Node::Item(Item { shortcut: Some("Ctrl+Cmd+M".into()), ..Item::new("other-minimize", "Other minimize") }),
            Node::Item(Item { shortcut: Some("Cmd+,".into()), ..Item::new("other-settings", "Other settings") }),
            Node::Item(Item { shortcut: Some("Cmd+Q".into()), ..Item::new("other-quit", "Other quit") }),
        ]);
        let layout = mac_layout(&source, |label| format!("translated {label}"));
        for id in [HIDE, MINIMIZE, "settings", "quit"] {
            assert_eq!(layout.bar.find(id).unwrap().shortcut, None, "{id}");
        }
        assert_eq!(layout.bar.find("about").unwrap().label, "translated About Local Image");
        assert!(layout.bar.menu(MenuRole::App).unwrap().children.iter().any(|n| matches!(n, Node::Submenu { label, .. } if label == "translated Settings")));
        assert!(layout.clashes.iter().any(|c| c.shortcut == "Cmd+," && c.command == "other-settings"));
    }

    #[test]
    fn shared_menu_effective_role_shortcuts_preserve_unbound_and_custom() {
        let source = tagged_bar();
        let unbound = mac_layout_with_bindings(&source, str::to_string, []);
        assert_eq!(unbound.bar.find("settings").unwrap().shortcut, None);
        assert_eq!(unbound.bar.find("quit").unwrap().shortcut, None);
        let mut custom = source;
        if let Node::Submenu { children, .. } = &mut custom.menus[1].children[0]
            && let Node::Item(settings) = &mut children[0]
        {
            settings.shortcut = Some("Cmd+J".into());
        }
        if let Node::Item(quit) = &mut custom.menus[0].children[1] {
            quit.shortcut = Some("Ctrl+Alt+Q".into());
        }
        let layout = mac_layout_with_bindings(
            &custom,
            str::to_string,
            [
                ("settings".into(), "Cmd+J".into()),
                ("quit".into(), "Ctrl+Alt+Q".into()),
                ("hidden-comma".into(), "Cmd+,".into()),
                ("hidden-q".into(), "Cmd+Q".into()),
            ],
        );
        assert_eq!(layout.bar.find("settings").unwrap().shortcut.as_deref(), Some("Cmd+J"));
        assert_eq!(layout.bar.find("quit").unwrap().shortcut.as_deref(), Some("Ctrl+Alt+Q"));
    }

    #[test]
    fn shared_menu_hidden_effective_bindings_protect_system_and_fallback_keys() {
        for occupy_fallbacks in [false, true] {
            let mut bindings = vec![
                ("hidden-hide".into(), "Cmd+H".into()),
                ("hidden-minimize".into(), "Cmd+M".into()),
                ("hidden-close".into(), "Cmd+W".into()),
                ("hidden-settings".into(), "Cmd+,".into()),
            ];
            if occupy_fallbacks {
                bindings.extend([("hidden-fallback-h".into(), "Ctrl+Cmd+H".into()), ("hidden-fallback-m".into(), "Ctrl+Cmd+M".into())]);
            }
            let layout = mac_layout_with_bindings(&tagged_bar(), str::to_string, bindings);
            assert_eq!(layout.bar.find(HIDE).unwrap().shortcut.as_deref(), (!occupy_fallbacks).then_some("Ctrl+Cmd+H"));
            assert_eq!(layout.bar.find(MINIMIZE).unwrap().shortcut.as_deref(), (!occupy_fallbacks).then_some("Ctrl+Cmd+M"));
            assert_eq!(layout.bar.find("settings").unwrap().shortcut, None);
            assert!(!layout.bar.menus.iter().any(|m| m.children.contains(&Node::Standard(Standard::CloseWindow))));
            for (shortcut, command) in [("Cmd+H", "hidden-hide"), ("Cmd+M", "hidden-minimize"), ("Cmd+W", "hidden-close"), ("Cmd+,", "hidden-settings")] {
                assert!(layout.clashes.iter().any(|clash| clash.shortcut == shortcut && clash.command == command), "{shortcut}");
            }
        }
    }

    #[test]
    fn shared_menu_full_bindings_remove_hide_others_collision() {
        let mut source = tagged_bar();
        source.menus[0].children.push(Node::Item(Item { shortcut: Some("Cmd+Alt+H".into()), ..Item::new("custom-hide-others", "Custom action") }));
        let legacy = mac_layout(&source, str::to_string);
        assert!(legacy.bar.menu(MenuRole::App).unwrap().children.contains(&Node::Standard(Standard::HideOthers)));
        let layout = mac_layout_with_bindings(&source, str::to_string, [("custom-hide-others".into(), "Cmd+Alt+H".into())]);
        assert!(!layout.bar.menu(MenuRole::App).unwrap().children.contains(&Node::Standard(Standard::HideOthers)));
        assert_eq!(layout.bar.find("custom-hide-others").unwrap().shortcut.as_deref(), Some("Cmd+Alt+H"));
        let clash = layout.clashes.iter().find(|c| c.shortcut == "Cmd+Alt+H").unwrap();
        assert_eq!((clash.command.as_str(), clash.resolution), ("custom-hide-others", "no system Hide Others item"));
    }

    #[test]
    fn shared_menu_structure_identity_excludes_live_state() {
        let source = tagged_bar();
        let mut live = source.clone();
        if let Node::Item(item) = &mut live.menus[0].children[0] {
            item.label = "Translated Open".into();
            item.enabled = false;
        }
        assert_eq!(structure_key(&source), structure_key(&live));
        assert_eq!(state_key(&source), state_key(&source.clone()));
        assert_ne!(state_key(&source), state_key(&live));
        if let Node::Item(item) = &mut live.menus[0].children[0] {
            item.checked = Some(false);
        }
        assert_ne!(structure_key(&source), structure_key(&live));
        let check_key = structure_key(&live);
        let unchecked_key = state_key(&live);
        if let Node::Item(item) = &mut live.menus[0].children[0] {
            item.checked = Some(true);
        }
        assert_eq!(check_key, structure_key(&live));
        assert_ne!(unchecked_key, state_key(&live));
    }

    #[test]
    fn shared_menu_library_serialization_and_parameter_identity_golden() {
        let node = CommandMenuNode::Submenu {
            label: "Rating".into(),
            children: vec![
                CommandMenuNode::Item {
                    id: "photo.rate".into(),
                    params: json!({"rating": 4}),
                    label: "4 Stars".into(),
                    shortcut: Some("4".into()),
                    enabled: true,
                    checked: Some(false),
                },
                CommandMenuNode::Separator,
                CommandMenuNode::Item { id: "view.detail".into(), params: Value::Null, label: "Loupe".into(), shortcut: None, enabled: false, checked: None },
            ],
        };
        assert_eq!(
            serde_json::to_value(node).unwrap(),
            json!({"kind":"submenu","label":"Rating","children":[
                {"kind":"item","id":"photo.rate","params":{"rating":4},"label":"4 Stars","shortcut":"4","enabled":true,"checked":false},
                {"kind":"separator"}, {"kind":"item","id":"view.detail","label":"Loupe","enabled":false}
            ]})
        );
        assert_eq!(CommandMenuNode::key("photo.rate", &json!({"rating": 4})), "photo.rate|{\"rating\":4}");
        assert_ne!(CommandMenuNode::key("photo.rate", &json!({"rating": 4})), CommandMenuNode::key("photo.rate", &json!({"rating": 5})));
        assert_eq!(CommandMenuNode::key("view.detail", &Value::Null), "view.detail");
        assert_eq!(
            serde_json::to_value(row("open", &["File"], None)).unwrap(),
            json!({"id":"open","label":"open","path":["File"],"shortcut":null,"enabled":true,"checked":null})
        );
    }
}
