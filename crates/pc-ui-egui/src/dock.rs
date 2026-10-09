//! Right-dock layout (#88): panel groups with fixed heights, like Photoshop's dock columns, and
//! panels torn off into floating windows that snap back in.
//!
//! A group's height never follows its content: content taller than the group scrolls inside
//! it. The last expanded group (Layers by default) fills what the others leave. Drag the gap
//! between two groups to resize them, double-click a tab (or use the panel menu) to collapse a
//! group to its tab strip, and drag a tab strip to move the group up or down the column
//! (unless Window › Workspace › Lock Workspace is on).
//!
//! Every tab ([`Tab`]) can move: drag a tab (or a group by its strip) out of the dock and it
//! becomes a floating panel under the pointer; drag a floating panel back over a group's tab
//! strip, between two groups or onto the dock's edge and a highlighted drop zone shows where it
//! will dock when let go (see `dock_float.rs`). Floating panels also merge into each other.
//!
//! The layout is [`DockLayout`] in `UiState::dock` (serialisable, drivable with `ui.set`), saved
//! with Window › Workspace › New Workspace…, reset by Reset Workspace, and remembered across
//! launches in the preferences (`panelLayout`) while Remember Workspace Changes is on. Layouts
//! saved before tabs could move have no tab lists and load into the default grouping.

use std::collections::BTreeMap;

use egui::{Rect, Sense, Stroke, Vec2, pos2, vec2};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::state::{DockTabs, Panels};
use crate::theme::Tokens;
use crate::widgets;

#[path = "dock_float.rs"]
mod float;
pub use float::{FloatRects, SNAP, TEAR, Target, last_floats, show_floating, target_at};

/// A dock panel group (one tab strip).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// Color | Swatches | Gradients | Patterns.
    Color,
    /// Properties | Adjustments.
    Properties,
    /// Character | Paragraph (not in the default workspace; Window › Character opens it, #150).
    Character,
    /// Navigator | Histogram | Info.
    Navigator,
    /// History | Actions | Layer Comps.
    History,
    /// Layers | Channels | Paths.
    Layers,
    /// local-image: Generate | Library (AI image generation and the generated library).
    Generate,
    /// A group made by docking panels between groups when their own groups are in use
    /// (saved as `custom1`, `custom2`, …). Shown while it holds tabs.
    Custom(u8),
}

impl Serialize for Group {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Group::Custom(n) => s.serialize_str(&format!("custom{n}")),
            g => s.serialize_str(g.key()),
        }
    }
}

impl<'de> Deserialize<'de> for Group {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Group::from_key(&s)
            .or_else(|| s.strip_prefix("custom").and_then(|n| n.parse::<u8>().ok()).map(Group::Custom))
            .ok_or_else(|| serde::de::Error::custom(format!("unknown dock group \"{s}\"")))
    }
}

impl Group {
    /// The built-in groups in Photoshop Essentials order, top to bottom.
    pub const ALL: [Group; 7] = [Group::Color, Group::Properties, Group::Generate, Group::Character, Group::Navigator, Group::History, Group::Layers];

    /// The `panels` / `dockTabs` key ("color", "properties", …); "custom" for user groups.
    pub fn key(self) -> &'static str {
        match self {
            Group::Color => "color",
            Group::Properties => "properties",
            Group::Character => "character",
            Group::Navigator => "navigator",
            Group::History => "history",
            Group::Layers => "layers",
            Group::Generate => "generate",
            Group::Custom(_) => "custom",
        }
    }

    pub fn is_custom(self) -> bool {
        matches!(self, Group::Custom(_))
    }

    /// Height (points, tab strip included) a group gets until the user resizes it, when the
    /// column has room (see [`DockLayout::heights_for`]).
    pub fn default_height(self) -> f32 {
        match self {
            Group::Color => 190.0,
            Group::Properties => 250.0,
            Group::Character => 270.0,
            Group::Navigator => 210.0,
            Group::History => 200.0,
            Group::Layers => 320.0,
            Group::Generate => 360.0,
            Group::Custom(_) => 240.0,
        }
    }

    /// What a group left at its default height gives way down to so the filler (Layers) keeps
    /// [`Group::preferred_fill`] in a short column (#147). Content taller than this scrolls.
    pub fn compact_height(self) -> f32 {
        match self {
            Group::Color => 130.0,
            Group::Properties => 160.0,
            Group::Character => 160.0,
            Group::Navigator => 140.0,
            Group::History => 130.0,
            Group::Layers => 200.0,
            Group::Generate => 200.0,
            Group::Custom(_) => 140.0,
        }
    }

    /// The height the filling group asks for before default-sized groups above it get their
    /// full defaults: Layers wants room for about ten rows at 900 pt (#147).
    pub fn preferred_fill(self) -> f32 {
        match self {
            Group::Layers => 500.0,
            g => g.min_height(),
        }
    }

    /// The smallest height an expanded group can be dragged or squeezed to.
    pub fn min_height(self) -> f32 {
        match self {
            Group::Layers => 140.0,
            _ => 80.0,
        }
    }

    /// The tabs the group starts with (its "home" tabs), in strip order.
    pub fn default_tabs(self, pro: bool) -> &'static [Tab] {
        match self {
            // Photoshop Essentials: Color | Swatches | Gradients | Patterns.
            Group::Color if pro => &[Tab::Color, Tab::Swatches, Tab::Gradients, Tab::Patterns],
            Group::Color => &[Tab::Swatches, Tab::Color, Tab::Gradients, Tab::Patterns],
            Group::Properties => &[Tab::Properties, Tab::Adjustments],
            Group::Character => &[Tab::Character, Tab::Paragraph],
            Group::Navigator => &[Tab::Navigator, Tab::Histogram, Tab::Info],
            Group::History => &[Tab::History, Tab::Actions, Tab::LayerComps],
            Group::Layers => &[Tab::Layers, Tab::Channels, Tab::Paths],
            Group::Generate => &[Tab::Generate, Tab::Library],
            Group::Custom(_) => &[],
        }
    }

    /// English labels of [`Group::default_tabs`].
    pub fn labels(self, pro: bool) -> Vec<&'static str> {
        self.default_tabs(pro).iter().map(|t| t.label()).collect()
    }

    /// The `dockTabs` entry: the selected home tab, as an index into [`Group::default_tabs`].
    pub fn tab_index(self, tabs: &DockTabs) -> Option<usize> {
        Some(match self {
            Group::Color => tabs.color,
            Group::Properties => tabs.properties,
            Group::Character => tabs.character,
            Group::Navigator => tabs.navigator,
            Group::History => tabs.history,
            Group::Layers => tabs.layers,
            Group::Generate => tabs.generate,
            Group::Custom(_) => return None,
        })
    }

    fn tab_slot(self, tabs: &mut DockTabs) -> Option<&mut usize> {
        Some(match self {
            Group::Color => &mut tabs.color,
            Group::Properties => &mut tabs.properties,
            Group::Character => &mut tabs.character,
            Group::Navigator => &mut tabs.navigator,
            Group::History => &mut tabs.history,
            Group::Layers => &mut tabs.layers,
            Group::Generate => &mut tabs.generate,
            Group::Custom(_) => return None,
        })
    }

    /// The group for a `panels` / `dockTabs` key ("color", "properties", …).
    pub fn from_key(key: &str) -> Option<Group> {
        Group::ALL.into_iter().find(|g| g.key() == key)
    }

    /// Is the group open (Window menu)? User groups are open while they hold tabs.
    pub fn shown(self, panels: &Panels) -> bool {
        match self {
            Group::Color => panels.color,
            Group::Properties => panels.properties,
            Group::Character => panels.character,
            Group::Navigator => panels.navigator,
            Group::History => panels.history,
            Group::Layers => panels.layers,
            Group::Generate => panels.generate,
            Group::Custom(_) => true,
        }
    }

    fn set_shown(self, panels: &mut Panels, on: bool) {
        let slot = match self {
            Group::Color => &mut panels.color,
            Group::Properties => &mut panels.properties,
            Group::Character => &mut panels.character,
            Group::Navigator => &mut panels.navigator,
            Group::History => &mut panels.history,
            Group::Layers => &mut panels.layers,
            Group::Generate => &mut panels.generate,
            Group::Custom(_) => return,
        };
        *slot = on;
    }
}

/// One dock panel (a tab), wherever it is: in a docked group or a floating panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tab {
    Color,
    Swatches,
    Gradients,
    Patterns,
    Properties,
    Adjustments,
    Character,
    Paragraph,
    Navigator,
    Histogram,
    Info,
    History,
    Actions,
    LayerComps,
    Layers,
    Channels,
    Paths,
    Generate,
    Library,
}

impl Tab {
    pub const ALL: [Tab; 19] = [
        Tab::Color,
        Tab::Swatches,
        Tab::Gradients,
        Tab::Patterns,
        Tab::Properties,
        Tab::Adjustments,
        Tab::Character,
        Tab::Paragraph,
        Tab::Navigator,
        Tab::Histogram,
        Tab::Info,
        Tab::History,
        Tab::Actions,
        Tab::LayerComps,
        Tab::Layers,
        Tab::Channels,
        Tab::Paths,
        Tab::Generate,
        Tab::Library,
    ];

    /// The English panel name (the tab label's translation key).
    pub fn label(self) -> &'static str {
        match self {
            Tab::Color => "Color",
            Tab::Swatches => "Swatches",
            Tab::Gradients => "Gradients",
            Tab::Patterns => "Patterns",
            Tab::Properties => "Properties",
            Tab::Adjustments => "Adjustments",
            Tab::Character => "Character",
            Tab::Paragraph => "Paragraph",
            Tab::Navigator => "Navigator",
            Tab::Histogram => "Histogram",
            Tab::Info => "Info",
            Tab::History => "History",
            Tab::Actions => "Actions",
            Tab::LayerComps => "Layer Comps",
            Tab::Layers => "Layers",
            Tab::Channels => "Channels",
            Tab::Paths => "Paths",
            Tab::Generate => "Generate",
            Tab::Library => "Library",
        }
    }

    /// The group the tab belongs to by default; a tab found nowhere goes back there.
    pub fn home(self) -> Group {
        match self {
            Tab::Color | Tab::Swatches | Tab::Gradients | Tab::Patterns => Group::Color,
            Tab::Properties | Tab::Adjustments => Group::Properties,
            Tab::Character | Tab::Paragraph => Group::Character,
            Tab::Navigator | Tab::Histogram | Tab::Info => Group::Navigator,
            Tab::History | Tab::Actions | Tab::LayerComps => Group::History,
            Tab::Layers | Tab::Channels | Tab::Paths => Group::Layers,
            Tab::Generate | Tab::Library => Group::Generate,
        }
    }

    /// Index in the home group's default tabs (what `dockTabs` stores).
    pub fn home_index(self, pro: bool) -> usize {
        self.home().default_tabs(pro).iter().position(|t| *t == self).unwrap_or(0)
    }

    /// Home tab `idx` of `g` (`dockTabs` / Window menu terms).
    pub fn of(g: Group, idx: usize, pro: bool) -> Option<Tab> {
        g.default_tabs(pro).get(idx).copied()
    }

    /// Tabs that lay out their own scrolling list and footer (they fill the group).
    pub fn scrolls_itself(self) -> bool {
        matches!(self, Tab::Layers | Tab::History | Tab::Generate | Tab::Library)
    }
}

/// Where a tab is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Docked(Group),
    /// A floating panel, by [`Floating::id`].
    Floating(u32),
}

/// A panel torn off the dock: a free-standing tab group over the canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Floating {
    /// Stable id (1, 2, …) for the window and drop targets.
    pub id: u32,
    pub tabs: Vec<Tab>,
    /// The tab it shows (`None`: the first).
    pub active: Option<Tab>,
    /// Top-left corner, screen points.
    pub pos: [f32; 2],
    /// Outer size, points.
    pub size: [f32; 2],
    /// Closed (its close button, or its Window menu item chosen while showing); the Window menu
    /// brings it back where it was.
    pub hidden: bool,
}

impl Default for Floating {
    fn default() -> Self {
        Self { id: 0, tabs: Vec::new(), active: None, pos: [120.0, 120.0], size: [280.0, 320.0], hidden: false }
    }
}

impl Floating {
    pub fn rect(&self) -> Rect {
        Rect::from_min_size(pos2(self.pos[0], self.pos[1]), vec2(self.size[0], self.size[1]))
    }

    /// The tab it shows.
    pub fn shown_tab(&self) -> Option<Tab> {
        self.active.filter(|t| self.tabs.contains(t)).or_else(|| self.tabs.first().copied())
    }
}

/// A drag of dock panels in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Drag {
    /// A docked tab, still in its group (it tears off once it leaves the dock).
    Tab { group: Group, tab: Tab },
    /// A docked group by its tab strip: reorders the column, or tears the group off.
    Group { group: Group, grab: Vec2 },
    /// A floating panel following the pointer (`grab`: pointer minus its top-left corner).
    Floating { id: u32, grab: Vec2 },
}

/// Per-session dock state that is never saved or compared.
#[derive(Clone, Debug, Default)]
pub(crate) struct Live {
    /// `dockTabs` as last seen, to notice the Window menu or a panel choosing a tab.
    seen: Option<DockTabs>,
    pub(crate) drag: Option<Drag>,
    /// The frame (pass) the drag was last advanced in.
    driven: Option<u64>,
    /// A floating panel to bring to the front.
    raise: Option<u32>,
}

impl PartialEq for Live {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// Order, heights, collapsed state and tabs of the dock groups, and the floating panels.
///
/// local-image: the default (Essentials) starts without the Generate panel (Layers keeps its
/// rows); Window › Generate, New from Prompt or the Home screen's prompt opens it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockLayout {
    /// Top-to-bottom order; a group missing here (say one added after the layout was saved)
    /// goes just below the nearest group that precedes it by default (or last).
    pub order: Vec<Group>,
    /// Heights the user dragged groups to (points, tab strip included). Unset = default.
    pub heights: BTreeMap<Group, f32>,
    /// Groups collapsed to their tab strip.
    pub collapsed: Vec<Group>,
    /// Each group's tabs once the user has moved tabs (empty: the default grouping). A tab found
    /// nowhere (a layout saved before tabs could move, a panel added since) joins its home group.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tabs: BTreeMap<Group, Vec<Tab>>,
    /// The tab a docked group shows when `dockTabs` doesn't decide it (a tab from another group).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub active: BTreeMap<Group, Tab>,
    /// Panels torn off the dock.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub floating: Vec<Floating>,
    #[serde(skip)]
    pub(crate) live: Live,
}

/// Gap between groups; it is also the splitter's grab area.
pub const GAP: f32 = 6.0;
/// Upper bound on a stored height (guards against absurd values from `ui.set`).
const MAX_HEIGHT: f32 = 4000.0;

impl DockLayout {
    /// Every group once, in display order (user groups included, empty groups too).
    pub fn order(&self) -> Vec<Group> {
        let mut out: Vec<Group> = Vec::with_capacity(Group::ALL.len() + 2);
        for g in &self.order {
            if !out.contains(g) {
                out.push(*g);
            }
        }
        // Missing groups slot in below their default predecessor, so a group new to an old
        // saved layout (Character) never lands below Layers and takes over as the filler.
        for (i, g) in Group::ALL.iter().enumerate() {
            if out.contains(g) {
                continue;
            }
            let prev = Group::ALL.iter().take(i).rev().find_map(|p| out.iter().position(|x| x == p));
            match prev {
                Some(at) => out.insert(at + 1, *g),
                None => out.push(*g),
            }
        }
        // User groups that hold tabs but lost their place go last.
        for g in self.tabs.keys() {
            if g.is_custom() && !out.contains(g) {
                out.push(*g);
            }
        }
        out
    }

    pub fn is_collapsed(&self, g: Group) -> bool {
        self.collapsed.contains(&g)
    }

    pub fn set_collapsed(&mut self, g: Group, on: bool) {
        self.collapsed.retain(|c| *c != g);
        if on {
            self.collapsed.push(g);
        }
    }

    /// The stored (or default) height, sanitised.
    pub fn height(&self, g: Group) -> f32 {
        match self.heights.get(&g) {
            Some(h) if h.is_finite() => h.clamp(g.min_height(), MAX_HEIGHT),
            _ => g.default_height(),
        }
    }

    /// Move `g` so it is drawn just before `before` (or last when `None`).
    pub fn move_group(&mut self, g: Group, before: Option<Group>) {
        if before == Some(g) {
            return;
        }
        let mut order = self.order();
        order.retain(|x| *x != g);
        let at = before.and_then(|b| order.iter().position(|x| *x == b)).unwrap_or(order.len());
        order.insert(at, g);
        self.order = order;
    }

    /// Every group's docked tabs (hidden and empty groups included), in display order. Tabs in
    /// floating panels are left out; a tab found nowhere joins its home group.
    pub fn assignment(&self, pro: bool) -> Vec<(Group, Vec<Tab>)> {
        let mut claimed: Vec<Tab> = Vec::with_capacity(Tab::ALL.len());
        for f in &self.floating {
            for t in &f.tabs {
                if !claimed.contains(t) {
                    claimed.push(*t);
                }
            }
        }
        let mut out: Vec<(Group, Vec<Tab>)> = self.order().into_iter().map(|g| (g, Vec::new())).collect();
        for (g, list) in out.iter_mut() {
            for t in self.tabs.get(g).into_iter().flatten() {
                if !claimed.contains(t) {
                    claimed.push(*t);
                    list.push(*t);
                }
            }
        }
        for (g, list) in out.iter_mut() {
            for t in g.default_tabs(pro) {
                if !claimed.contains(t) {
                    claimed.push(*t);
                    list.push(*t);
                }
            }
        }
        out
    }

    /// The tabs docked in `g`.
    pub fn group_tabs(&self, g: Group, pro: bool) -> Vec<Tab> {
        self.assignment(pro).into_iter().find(|(x, _)| *x == g).map(|(_, l)| l).unwrap_or_default()
    }

    /// Where `tab` is.
    pub fn locate(&self, tab: Tab, pro: bool) -> Place {
        if let Some(f) = self.floating.iter().find(|f| f.tabs.contains(&tab)) {
            return Place::Floating(f.id);
        }
        self.assignment(pro).into_iter().find(|(_, l)| l.contains(&tab)).map_or(Place::Docked(tab.home()), |(g, _)| Place::Docked(g))
    }

    pub fn floating(&self, id: u32) -> Option<&Floating> {
        self.floating.iter().find(|f| f.id == id)
    }

    pub fn floating_mut(&mut self, id: u32) -> Option<&mut Floating> {
        self.floating.iter_mut().find(|f| f.id == id)
    }

    /// Write the current grouping out as explicit tab lists (before moving tabs).
    fn materialize(&mut self, pro: bool) {
        self.tabs = self.assignment(pro).into_iter().filter(|(g, l)| !g.is_custom() || !l.is_empty()).collect();
    }

    /// Drop empty floating panels and empty user groups (and what is stored about them).
    fn prune(&mut self) {
        self.floating.retain(|f| !f.tabs.is_empty());
        let dead: Vec<Group> = self.order().into_iter().filter(|g| g.is_custom() && self.tabs.get(g).is_none_or(Vec::is_empty)).collect();
        for g in dead {
            self.tabs.remove(&g);
            self.order.retain(|x| *x != g);
            self.heights.remove(&g);
            self.collapsed.retain(|x| *x != g);
            self.active.remove(&g);
        }
    }

    /// Unique ids, no tab in two floating panels, no empty ones, finite geometry.
    fn tidy(&mut self) {
        let mut next = self.floating.iter().map(|f| f.id).max().unwrap_or(0);
        let mut ids: Vec<u32> = Vec::with_capacity(self.floating.len());
        let mut seen: Vec<Tab> = Vec::new();
        for f in &mut self.floating {
            f.tabs.retain(|t| {
                let fresh = !seen.contains(t);
                seen.push(*t);
                fresh
            });
            if f.id == 0 || ids.contains(&f.id) {
                next = next.saturating_add(1);
                f.id = next;
            }
            ids.push(f.id);
            for v in f.pos.iter_mut().chain(f.size.iter_mut()) {
                if !v.is_finite() {
                    *v = 200.0;
                }
            }
        }
        self.floating.retain(|f| !f.tabs.is_empty());
    }

    /// Take `tabs` out of wherever they are.
    pub fn take(&mut self, tabs: &[Tab], pro: bool) {
        self.materialize(pro);
        for list in self.tabs.values_mut() {
            list.retain(|t| !tabs.contains(t));
        }
        for f in &mut self.floating {
            f.tabs.retain(|t| !tabs.contains(t));
        }
        self.active.retain(|_, t| !tabs.contains(t));
        self.prune();
    }

    /// Dock `tabs` into group `g` at strip position `at` (clamped).
    pub fn insert(&mut self, tabs: &[Tab], g: Group, at: usize, pro: bool) {
        self.take(tabs, pro);
        let list = self.tabs.entry(g).or_default();
        let at = at.min(list.len());
        list.splice(at..at, tabs.iter().copied());
        if g.is_custom() && !self.order.contains(&g) {
            let mut order = self.order();
            order.push(g);
            self.order = order;
        }
        self.set_collapsed(g, false);
    }

    /// Dock `tabs` as a new group drawn before `before` (last when `None`). The group takes the
    /// identity of `lead`'s home group (or another of the tabs' homes) when that is empty, so a
    /// torn-off Layers group docked back is the Layers group again; otherwise a user group.
    pub fn new_group(&mut self, tabs: &[Tab], lead: Tab, before: Option<Group>, pro: bool) -> Group {
        self.take(tabs, pro);
        let a = self.assignment(pro);
        let empty = |g: Group| a.iter().any(|(x, l)| *x == g && l.is_empty());
        let homes = std::iter::once(lead).chain(tabs.iter().copied()).map(Tab::home);
        let g = homes.into_iter().find(|h| empty(*h)).unwrap_or_else(|| {
            let used = self.order();
            Group::Custom((1..=u8::MAX).find(|n| !used.contains(&Group::Custom(*n))).unwrap_or(u8::MAX))
        });
        self.tabs.insert(g, tabs.to_vec());
        self.move_group(g, before.filter(|b| *b != g));
        self.set_collapsed(g, false);
        g
    }

    /// Float `tabs` in a new panel at `pos` (top-left) of `size`. Returns its id.
    pub fn float(&mut self, tabs: &[Tab], active: Option<Tab>, pos: egui::Pos2, size: Vec2, pro: bool) -> u32 {
        self.take(tabs, pro);
        let id = self.floating.iter().map(|f| f.id).max().unwrap_or(0).saturating_add(1);
        self.floating.push(Floating { id, tabs: tabs.to_vec(), active, pos: [pos.x, pos.y], size: [size.x, size.y], hidden: false });
        id
    }

    /// Put `tabs` into floating panel `id` at strip position `at`.
    pub fn merge_into_floating(&mut self, tabs: &[Tab], id: u32, at: usize, pro: bool) {
        if self.floating(id).is_none() {
            return;
        }
        self.take(tabs, pro);
        // Taking the tabs can't empty the target unless it held only them: then re-create it.
        match self.floating_mut(id) {
            Some(f) => {
                let at = at.min(f.tabs.len());
                f.tabs.splice(at..at, tabs.iter().copied());
                f.hidden = false;
            }
            None => self.floating.push(Floating { id, tabs: tabs.to_vec(), ..Default::default() }),
        }
    }

    /// Lay out the `shown` groups (in display order) in a column `avail` points tall with
    /// `strip`-high tab strips. Returns each group's height. The last expanded group fills the
    /// rest. Groups the user never resized give way first, down to their compact heights, so
    /// the filler gets its preferred height (Layers: ~10 rows, #147); when the column is still
    /// too short every group gives way down to its minimum height.
    pub fn heights_for(&self, shown: &[Group], avail: f32, strip: f32) -> Vec<(Group, f32)> {
        let avail = if avail.is_finite() { avail.max(0.0) } else { 0.0 };
        let filler = shown.iter().rposition(|g| !self.is_collapsed(*g));
        let mut hs: Vec<f32> = shown
            .iter()
            .enumerate()
            .map(|(i, g)| {
                if self.is_collapsed(*g) {
                    strip
                } else if Some(i) == filler {
                    0.0
                } else {
                    self.height(*g)
                }
            })
            .collect();
        if let Some(f) = filler {
            let gaps = GAP * shown.len().saturating_sub(1) as f32;
            let min_fill = shown.get(f).map_or(0.0, |g| g.min_height());
            let pref_fill = shown.get(f).map_or(0.0, |g| g.preferred_fill());
            let used: f32 = hs.iter().sum::<f32>() + gaps;
            let mut deficit = (used + pref_fill - avail).max(0.0);
            for i in (0..f).rev() {
                if deficit <= 0.0 {
                    break;
                }
                let Some(g) = shown.get(i) else { continue };
                if self.is_collapsed(*g) || self.heights.contains_key(g) {
                    continue;
                }
                if let Some(h) = hs.get_mut(i) {
                    let give = (*h - g.compact_height()).max(0.0).min(deficit);
                    *h -= give;
                    deficit -= give;
                }
            }
            let used: f32 = hs.iter().sum::<f32>() + gaps;
            let mut deficit = (used + min_fill - avail).max(0.0);
            // Squeeze the expanded groups nearest the filler first.
            for i in (0..f).rev() {
                if deficit <= 0.0 {
                    break;
                }
                let Some(g) = shown.get(i) else { continue };
                if self.is_collapsed(*g) {
                    continue;
                }
                if let Some(h) = hs.get_mut(i) {
                    let give = (*h - g.min_height()).max(0.0).min(deficit);
                    *h -= give;
                    deficit -= give;
                }
            }
            let rest = avail - hs.iter().sum::<f32>() - gaps;
            if let Some(h) = hs.get_mut(f) {
                *h = rest.max(min_fill);
            }
        }
        shown.iter().copied().zip(hs).collect()
    }
}

// ----------------------------------------------------------------------- app-level state

/// Pro (Photoshop) themes order the Color group's tabs differently.
pub fn is_pro(app: &PhotocraftApp) -> bool {
    matches!(app.ui.theme, crate::theme::ThemeKind::Pro | crate::theme::ThemeKind::ProMedium)
}

/// The tab docked group `g` (holding `list`) shows given `dockTabs` = `dt`.
fn active_in(layout: &DockLayout, g: Group, list: &[Tab], dt: &DockTabs, pro: bool) -> Option<Tab> {
    if let Some(t) = layout.active.get(&g).filter(|t| list.contains(t)) {
        return Some(*t);
    }
    if let Some(t) = g.tab_index(dt).and_then(|i| Tab::of(g, i, pro)).filter(|t| list.contains(t)) {
        return Some(t);
    }
    list.first().copied()
}

/// The tab a docked group or floating panel shows.
pub fn active_tab(app: &PhotocraftApp, place: Place) -> Option<Tab> {
    let pro = is_pro(app);
    match place {
        Place::Docked(g) => active_in(&app.ui.dock, g, &app.ui.dock.group_tabs(g, pro), &app.ui.dock_tabs, pro),
        Place::Floating(id) => app.ui.dock.floating(id).and_then(Floating::shown_tab),
    }
}

/// Make `tab` the one its container shows. `prev` is `dockTabs` before the change that asked
/// for it: the home group keeps showing what it showed when `tab` lives elsewhere.
fn activate(app: &mut PhotocraftApp, tab: Tab, prev: &DockTabs) {
    let pro = is_pro(app);
    let home = tab.home();
    let place = app.ui.dock.locate(tab, pro);
    let layout = &mut app.ui.dock;
    if place != Place::Docked(home) && !layout.active.contains_key(&home) {
        let list = layout.group_tabs(home, pro);
        if let Some(t) = active_in(layout, home, &list, prev, pro) {
            layout.active.insert(home, t);
        }
    }
    match place {
        Place::Docked(g) if g == home => {
            layout.active.remove(&g);
        }
        Place::Docked(g) => {
            layout.active.insert(g, tab);
        }
        Place::Floating(id) => {
            if let Some(f) = layout.floating_mut(id) {
                f.active = Some(tab);
            }
        }
    }
}

/// The user chose `tab` (its strip, a drop): show it and record it in `dockTabs`.
pub fn select(app: &mut PhotocraftApp, tab: Tab) {
    let prev = app.ui.dock_tabs;
    activate(app, tab, &prev);
    let idx = tab.home_index(is_pro(app));
    if let Some(slot) = tab.home().tab_slot(&mut app.ui.dock_tabs) {
        *slot = idx;
    }
    if let Some(seen) = app.ui.dock.live.seen.as_mut()
        && let Some(slot) = tab.home().tab_slot(seen)
    {
        *slot = idx;
    }
}

/// Once per frame: tidy the layout and show the tabs the Window menu, panels or `ui.set` chose
/// by writing `dockTabs` wherever those tabs are now.
pub fn sync(app: &mut PhotocraftApp) {
    app.ui.dock.tidy();
    let now = app.ui.dock_tabs;
    let Some(seen) = app.ui.dock.live.seen else {
        app.ui.dock.live.seen = Some(now);
        return;
    };
    if seen == now {
        return;
    }
    let pro = is_pro(app);
    for g in Group::ALL {
        let (a, b) = (g.tab_index(&seen), g.tab_index(&now));
        if a != b
            && let Some(t) = b.and_then(|i| Tab::of(g, i, pro))
        {
            activate(app, t, &seen);
        }
    }
    app.ui.dock.live.seen = Some(now);
}

/// Is home tab `idx` of `g` open and selected where it is (Window menu check marks)?
pub fn tab_checked(app: &PhotocraftApp, g: Group, idx: usize) -> bool {
    let pro = is_pro(app);
    let Some(tab) = Tab::of(g, idx, pro) else { return false };
    match app.ui.dock.locate(tab, pro) {
        Place::Docked(c) => c.shown(&app.ui.panels) && active_tab(app, Place::Docked(c)) == Some(tab),
        Place::Floating(id) => app.ui.dock.floating(id).is_some_and(|f| !f.hidden && f.shown_tab() == Some(tab)),
    }
}

/// [`tab_checked`] and not in a collapsed group: the panel can be seen.
pub fn tab_showing(app: &PhotocraftApp, g: Group, idx: usize) -> bool {
    let pro = is_pro(app);
    let collapsed = Tab::of(g, idx, pro).is_some_and(|t| matches!(app.ui.dock.locate(t, pro), Place::Docked(c) if app.ui.dock.is_collapsed(c)));
    tab_checked(app, g, idx) && !collapsed
}

/// Window › <panel>: open home tab `idx` of `g` wherever it is, expanded and selected.
pub fn reveal_tab(app: &mut PhotocraftApp, g: Group, idx: usize) {
    let pro = is_pro(app);
    let Some(tab) = Tab::of(g, idx, pro) else {
        if let Some(slot) = g.tab_slot(&mut app.ui.dock_tabs) {
            *slot = idx;
        }
        reveal(app, g);
        return;
    };
    select(app, tab);
    match app.ui.dock.locate(tab, pro) {
        Place::Docked(c) => {
            c.set_shown(&mut app.ui.panels, true);
            app.ui.dock.set_collapsed(c, false);
        }
        Place::Floating(id) => {
            if let Some(f) = app.ui.dock.floating_mut(id) {
                f.hidden = false;
            }
            app.ui.dock.live.raise = Some(id);
        }
    }
}

/// Window › <panel> on a showing panel: close what holds it (its group, or its floating panel).
/// A user group's tabs go back to their home groups.
pub fn hide_tab(app: &mut PhotocraftApp, g: Group, idx: usize) {
    let pro = is_pro(app);
    let Some(tab) = Tab::of(g, idx, pro) else {
        g.set_shown(&mut app.ui.panels, false);
        return;
    };
    match app.ui.dock.locate(tab, pro) {
        Place::Docked(c) if c.is_custom() => {
            let list = app.ui.dock.group_tabs(c, pro);
            app.ui.dock.take(&list, pro);
        }
        Place::Docked(c) => c.set_shown(&mut app.ui.panels, false),
        Place::Floating(id) => {
            if let Some(f) = app.ui.dock.floating_mut(id) {
                f.hidden = true;
            }
        }
    }
}

/// Show `g` and expand it (Window › <panel>, the icon rail): a panel asked for is always
/// brought back, whatever state it was left in (#129). When all of `g`'s tabs float, their
/// floating panels open instead.
pub fn reveal(app: &mut PhotocraftApp, g: Group) {
    g.set_shown(&mut app.ui.panels, true);
    app.ui.dock.set_collapsed(g, false);
    let pro = is_pro(app);
    if app.ui.dock.group_tabs(g, pro).is_empty() {
        for f in app.ui.dock.floating.iter_mut().filter(|f| f.tabs.iter().any(|t| t.home() == g)) {
            f.hidden = false;
        }
    }
}

/// Icon rail click: a hidden group is shown, a collapsed one expanded and an expanded one
/// collapsed to its tab strip. A docked group is never hidden from the rail (it used to
/// toggle visibility, so one stray click made a panel vanish: #129); `docked` is false for
/// Studio's floating Properties card, which the rail shows and hides.
pub fn rail_click(app: &mut PhotocraftApp, g: Group, docked: bool) {
    if !g.shown(&app.ui.panels) {
        reveal(app, g);
    } else if !docked {
        g.set_shown(&mut app.ui.panels, false);
    } else {
        let collapse = !app.ui.dock.is_collapsed(g);
        app.ui.dock.set_collapsed(g, collapse);
    }
}

/// The groups the dock column draws, in display order, with their tabs: open and not empty
/// (Studio floats Properties outside the dock, so `pro` false leaves it out).
pub fn docked(app: &PhotocraftApp, pro: bool) -> Vec<(Group, Vec<Tab>)> {
    app.ui.dock
        .assignment(is_pro(app))
        .into_iter()
        .filter(|(g, l)| !l.is_empty() && g.shown(&app.ui.panels) && (pro || *g != Group::Properties))
        .collect()
}

// ----------------------------------------------------------------------------- drawing

/// Per-group interactions collected while drawing, applied afterwards.
enum Action {
    ToggleCollapse(Group),
    Close(Group),
    Move(Group, Option<Group>),
    Float(Group),
}

/// Rects of the groups drawn last frame (screen points), for tests and automation.
pub fn last_rects(ctx: &egui::Context) -> Vec<(Group, Rect)> {
    ctx.data(|d| d.get_temp::<Vec<(Group, Rect)>>(rects_id())).unwrap_or_default()
}

fn rects_id() -> egui::Id {
    egui::Id::new("dock-group-rects")
}

/// A group's tab strip as drawn last frame (screen points), for tests and automation.
#[derive(Clone, Debug, PartialEq)]
pub struct StripRects {
    pub group: Group,
    /// The group's tabs, in strip order.
    pub tab_ids: Vec<Tab>,
    /// The whole strip.
    pub strip: Rect,
    /// `(tab index, rect)` of the tabs on the strip (the others are in the chevron menu).
    pub tabs: Vec<(usize, Rect)>,
    /// The panel menu button.
    pub menu: Rect,
    /// The » overflow button, when some tabs didn't fit.
    pub chevron: Option<Rect>,
}

/// The tab strips drawn last frame.
pub fn last_strips(ctx: &egui::Context) -> Vec<StripRects> {
    ctx.data(|d| d.get_temp::<Vec<StripRects>>(strips_id())).unwrap_or_default()
}

fn strips_id() -> egui::Id {
    egui::Id::new("dock-strip-rects")
}

fn area_id() -> egui::Id {
    egui::Id::new("dock-area")
}

/// Record the dock column's area this frame (where panels dock). With no groups open the host
/// passes the strip along the dock's edge.
pub fn note_area(ctx: &egui::Context, area: Rect) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(area_id(), (pass, area)));
}

/// No group is docked: `edge` (along the dock's edge) still catches dragged panels.
pub fn note_empty(ctx: &egui::Context, edge: Rect) {
    note_area(ctx, edge);
    ctx.data_mut(|d| {
        d.insert_temp(rects_id(), Vec::<(Group, Rect)>::new());
        d.insert_temp(strips_id(), Vec::<StripRects>::new());
    });
}

/// The dock column's area, when the dock was drawn this frame or the last.
pub fn dock_area(ctx: &egui::Context) -> Option<Rect> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<(u64, Rect)>(area_id())).filter(|(p, _)| p.saturating_add(1) >= pass).map(|(_, r)| r)
}

/// Draw one tab's content filling `ui`: tabs with their own list fill it, the others scroll.
fn tab_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, tab: Tab, body: &mut impl FnMut(&mut PhotocraftApp, &mut egui::Ui, Tab)) {
    let inner = ui.available_height().max(0.0);
    if tab.scrolls_itself() {
        ui.set_min_height(inner);
        body(app, ui, tab);
    } else {
        egui::ScrollArea::vertical().id_salt(("dock-scroll", tab)).max_height(inner).auto_shrink([false, false]).show(ui, |ui| body(app, ui, tab));
    }
}

/// The strip height of a dock group in this theme.
pub(crate) fn strip_height(t: &Tokens) -> f32 {
    if t.pro { 28.0 } else { 40.0 }
}

/// Draw the dock `groups` (from [`docked`]) filling `ui`. `body` draws one tab's content.
pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui, groups: &[(Group, Vec<Tab>)], mut body: impl FnMut(&mut PhotocraftApp, &mut egui::Ui, Tab)) {
    sync(app);
    let t = Tokens::get(ui.ctx());
    let pro = is_pro(app);
    let strip = strip_height(&t);
    let order: Vec<Group> = groups.iter().map(|(g, _)| *g).collect();
    let area = ui.available_rect_before_wrap();
    note_area(ui.ctx(), area);
    let heights = app.ui.dock.heights_for(&order, area.height(), strip);
    let locked = app.session.prefs().workspace_locked;
    let rects = rects_after_layout(&heights, area);
    let mut actions: Vec<Action> = Vec::new();
    let mut strips: Vec<StripRects> = Vec::with_capacity(rects.len());
    // Where the press that started a drag was: the grab point keeps that offset.
    let pointer = ui.ctx().input(|i| i.pointer.press_origin()).or_else(|| ui.ctx().pointer_interact_pos());
    for (i, (g, rect)) in rects.iter().copied().enumerate() {
        let Some(tabs) = groups.iter().find(|(x, _)| *x == g).map(|(_, l)| l.clone()) else { continue };
        let collapsed = app.ui.dock.is_collapsed(g);
        let mut child = ui.new_child(egui::UiBuilder::new().id_salt(("dock-group", g)).max_rect(rect));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        child.spacing_mut().item_spacing.y = if t.pro { 0.0 } else { 6.0 };
        let active = active_in(&app.ui.dock, g, &tabs, &app.ui.dock_tabs, pro);
        let before = active.and_then(|a| tabs.iter().position(|x| *x == a)).unwrap_or(0);
        let mut sel = before;
        let labels: Vec<&str> = tabs.iter().map(|t| t.label()).collect();
        let resp = widgets::card_ex(&mut child, g.key(), &labels, &mut sel, collapsed, |ui, i| {
            if let Some(tab) = tabs.get(i).copied() {
                tab_body(app, ui, tab, &mut body);
            }
        });
        let strip_rect = resp.tabs.iter().fold(resp.strip.rect.union(resp.menu.rect), |r, (_, t)| r.union(*t));
        strips.push(StripRects { group: g, tab_ids: tabs.clone(), strip: strip_rect, tabs: resp.tabs.clone(), menu: resp.menu.rect, chevron: resp.chevron });
        if sel != before
            && let Some(tab) = tabs.get(sel)
        {
            select(app, *tab);
        }
        if resp.strip.double_clicked() || resp.tab_double_clicked {
            actions.push(Action::ToggleCollapse(g));
        }
        if !locked && app.ui.dock.live.drag.is_none() {
            if let Some(tab) = resp.tab_drag_started.and_then(|i| tabs.get(i).copied()) {
                app.ui.dock.live.drag = Some(Drag::Tab { group: g, tab });
            } else if resp.strip.drag_started() {
                let grab = pointer.map_or(vec2(40.0, 12.0), |p| p - rect.min);
                app.ui.dock.live.drag = Some(Drag::Group { group: g, grab });
            }
        }
        egui::Popup::menu(&resp.menu).show(|ui| {
            ui.set_min_width(170.0);
            if tabs.get(sel) == Some(&Tab::Layers) {
                crate::layer_row_ui::panel_menu(app, ui);
                ui.separator();
            }
            if ui.button(if collapsed { tl!("Expand Panel Group") } else { tl!("Collapse Panel Group") }).clicked() {
                actions.push(Action::ToggleCollapse(g));
                ui.close();
            }
            let pos = order.iter().position(|x| *x == g).unwrap_or(0);
            if ui.add_enabled(!locked && pos > 0, egui::Button::new(tl!("Move Group Up"))).clicked() {
                actions.push(Action::Move(g, order.get(pos.saturating_sub(1)).copied()));
                ui.close();
            }
            if ui.add_enabled(!locked && pos + 1 < order.len(), egui::Button::new(tl!("Move Group Down"))).clicked() {
                actions.push(Action::Move(g, order.get(pos + 2).copied()));
                ui.close();
            }
            if ui.add_enabled(!locked, egui::Button::new(tl!("Float Panel Group"))).clicked() {
                actions.push(Action::Float(g));
                ui.close();
            }
            ui.separator();
            if ui.button(tl!("Close Tab Group")).clicked() {
                actions.push(Action::Close(g));
                ui.close();
            }
        });
        // Splitter in the gap below this group: resizes it against the next expanded group.
        if i + 1 < rects.len() && !collapsed && rects.iter().skip(i + 1).any(|(n, _)| !app.ui.dock.is_collapsed(*n)) {
            let gap = Rect::from_min_size(pos2(rect.left(), rect.bottom()), vec2(rect.width(), GAP)).expand2(vec2(0.0, 2.0));
            let sresp = ui.interact(gap, ui.id().with(("dock-splitter", g)), Sense::drag());
            if sresp.hovered() || sresp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                ui.painter().line_segment([gap.left_center(), gap.right_center()], Stroke::new(2.0, t.accent.gamma_multiply(0.7)));
            }
            if sresp.dragged() {
                resize(&mut app.ui.dock, &heights, i, sresp.drag_delta().y);
            }
        }
    }
    ui.ctx().data_mut(|d| {
        d.insert_temp(rects_id(), rects.clone());
        d.insert_temp(strips_id(), strips);
    });
    ui.advance_cursor_after_rect(area);
    for a in actions {
        match a {
            Action::ToggleCollapse(g) => {
                let on = !app.ui.dock.is_collapsed(g);
                app.ui.dock.set_collapsed(g, on);
            }
            Action::Close(g) if g.is_custom() => {
                let list = app.ui.dock.group_tabs(g, pro);
                app.ui.dock.take(&list, pro);
            }
            Action::Close(g) => g.set_shown(&mut app.ui.panels, false),
            Action::Move(g, before) => {
                if before != Some(g) {
                    app.ui.dock.move_group(g, before);
                }
            }
            Action::Float(g) => {
                let r = rects.iter().find(|(x, _)| *x == g).map_or(area, |(_, r)| *r);
                let size = float::torn_size(area, r.height());
                let pos = pos2(area.left() - size.x - 24.0, r.top());
                float::float_group(app, g, pos, size);
            }
        }
    }
    // Drags of tabs and groups (and floating panels) move on after the column is drawn.
    float::drive(app, ui.ctx());
}

fn rects_after_layout(heights: &[(Group, f32)], area: Rect) -> Vec<(Group, Rect)> {
    let mut y = area.top();
    heights
        .iter()
        .map(|(g, h)| {
            let r = Rect::from_min_size(pos2(area.left(), y), vec2(area.width(), h.max(0.0)));
            y = r.bottom() + GAP;
            (*g, r)
        })
        .collect()
}

/// The group the dragged one lands before when released at `y` (`None` = last).
fn drop_before(order: &[Group], rects: &[(Group, Rect)], dragged: Group, y: f32) -> Option<Group> {
    let hit = rects.iter().find(|(_, r)| y < r.center().y).map(|(g, _)| *g);
    match hit {
        // Dropping onto itself, or just below itself, keeps the place.
        Some(g) if g == dragged => Some(dragged),
        Some(g) => {
            let after_self = order.iter().position(|x| *x == dragged).zip(order.iter().position(|x| *x == g)).is_some_and(|(a, b)| b == a + 1);
            if after_self { Some(dragged) } else { Some(g) }
        }
        None => None,
    }
}

/// Splitter `i` (below group `i`) dragged by `dy`: group `i` grows or shrinks against the next
/// expanded group (or the filler, which absorbs the difference).
fn resize(layout: &mut DockLayout, heights: &[(Group, f32)], i: usize, dy: f32) {
    if !dy.is_finite() || dy == 0.0 {
        return;
    }
    let Some(&(g, h)) = heights.get(i) else { return };
    let filler = heights.iter().rposition(|(g, _)| !layout.is_collapsed(*g));
    let Some(j) = heights.iter().enumerate().skip(i + 1).find(|(_, (n, _))| !layout.is_collapsed(*n)).map(|(j, _)| j) else { return };
    let Some(&(n, nh)) = heights.get(j) else { return };
    // The first drag pins the other groups at the heights they show, so groups still at their
    // defaults (which give way to the filler) don't shift while this one is resized.
    for (k, (o, oh)) in heights.iter().enumerate() {
        if Some(k) != filler && !layout.is_collapsed(*o) {
            layout.heights.entry(*o).or_insert(*oh);
        }
    }
    let new_h = (h + dy).clamp(g.min_height(), (h + nh - n.min_height()).max(g.min_height()));
    layout.heights.insert(g, new_h);
    if Some(j) != filler {
        layout.heights.insert(n, (nh - (new_h - h)).max(n.min_height()));
    }
}

// --------------------------------------------------------------------------- persistence

/// What `prefs.panelLayout` holds: the live layout, open panels and floating panels.
fn snapshot(app: &PhotocraftApp) -> Value {
    json!({"workspace": app.ui.workspace, "panels": app.ui.panels, "dockTabs": app.ui.dock_tabs, "dock": app.ui.dock})
}

/// The layout as last written to the preferences.
#[derive(Clone, PartialEq)]
struct Persisted {
    workspace: String,
    panels: Panels,
    tabs: DockTabs,
    dock: DockLayout,
}

/// Remember the layout in the preferences once the user lets go of the mouse (Workspace ›
/// Remember Workspace Changes). The JSON is only built when the layout changed since it was
/// last written (a typed compare per frame, no allocation).
pub fn persist(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !app.session.prefs().workspace.remember_workspace_changes || ctx.input(|i| i.pointer.any_down()) {
        return;
    }
    let id = egui::Id::new("dock-persisted");
    let unchanged = ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Option<Persisted>>(id)
            .as_ref()
            .is_some_and(|p| p.workspace == app.ui.workspace && p.panels == app.ui.panels && p.tabs == app.ui.dock_tabs && p.dock == app.ui.dock)
    });
    if unchanged {
        return;
    }
    let now = snapshot(app);
    if app.session.prefs().panel_layout != now {
        app.session.prefs.edit(|p| p.panel_layout = now);
    }
    let written = Persisted { workspace: app.ui.workspace.clone(), panels: app.ui.panels.clone(), tabs: app.ui.dock_tabs, dock: app.ui.dock.clone() };
    ctx.data_mut(|d| d.insert_temp(id, Some(written)));
}

/// Restore the remembered layout at launch. Unreadable parts keep their defaults.
pub fn restore(app: &mut PhotocraftApp) {
    if !app.session.prefs().workspace.remember_workspace_changes {
        return;
    }
    let saved = app.session.prefs().panel_layout.clone();
    apply(app, &saved);
    if let Some(ws) = saved.get("workspace").and_then(Value::as_str) {
        app.ui.workspace = ws.to_string();
    }
}

/// Apply the `panels`, `dockTabs` and `dock` parts of a saved layout (a workspace or
/// `panelLayout`). Missing or invalid parts are left alone.
pub fn apply(app: &mut PhotocraftApp, v: &Value) {
    if let Some(p) = v.get("panels").and_then(|p| serde_json::from_value(p.clone()).ok()) {
        app.ui.panels = p;
    }
    if let Some(t) = v.get("dockTabs").and_then(|t| serde_json::from_value(t.clone()).ok()) {
        app.ui.dock_tabs = t;
    }
    if let Some(d) = v.get("dock").and_then(|d| serde_json::from_value(d.clone()).ok()) {
        app.ui.dock = d;
    }
}

#[cfg(test)]
#[path = "dock_tests.rs"]
mod tests;
