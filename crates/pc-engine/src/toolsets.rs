//! Tool sets: named layouts of the Compositing toolbar (which tools show). Five are built in and
//! read-only (All Tools, Photographer, Essentials, Retouching, AI); the user's own live in
//! `toolbar.sets` of the preferences. Commands `toolset.list / select / save / rename / delete /
//! duplicate / reset` drive them, so the toolbar, the menus, the dialog and automation agree.
//!
//! Tools are named as in `ui.set {tool}` (`Move`, `RectMarquee`, …). A set belongs to a
//! [`Persona`] so that a later Vector persona can carry sets of its own; every set today is Pixel.
//! The toolbar keeps the shell's group structure (the flyouts) and shows, per group, the tools
//! the set lists; a tool a set leaves out is still reachable by its shortcut (the toolbar then
//! shows the active tool in its place while it is active).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::prefs::ToolbarCustomization;
use crate::{EngineError, Result, Session};

/// Command bodies report plain messages; [`wrap`] turns them into bad-parameter errors.
type R<T> = std::result::Result<T, String>;

/// Which editing persona a set belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Persona {
    /// The raster editor (the only persona so far).
    #[default]
    Pixel,
}

/// A named list of tools.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolSet {
    pub id: String,
    pub name: String,
    pub persona: Persona,
    /// Tool names; the toolbar lays them out in its own group order.
    pub tools: Vec<String>,
}

/// The id of All Tools, the default set.
pub const DEFAULT_ID: &str = "allTools";

/// Every tool, in the toolbar's canonical order.
pub const ALL_TOOLS: &[&str] = &[
    "Move",
    "RectMarquee",
    "EllipseMarquee",
    "Lasso",
    "PolygonLasso",
    "MagneticLasso",
    "ObjectSelection",
    "QuickSelection",
    "MagicWand",
    "Crop",
    "PerspectiveCrop",
    "Slice",
    "SliceSelect",
    "Eyedropper",
    "Ruler",
    "Note",
    "Count",
    "SpotHealing",
    "Healing",
    "Patch",
    "ContentAwareMove",
    "RedEye",
    "Brush",
    "Pencil",
    "MixerBrush",
    "CloneStamp",
    "HistoryBrush",
    "Eraser",
    "BackgroundEraser",
    "MagicEraser",
    "Gradient",
    "PaintBucket",
    "Blur",
    "Sharpen",
    "Smudge",
    "Dodge",
    "Burn",
    "Sponge",
    "Pen",
    "Type",
    "VerticalType",
    "PathSelection",
    "DirectSelection",
    "Rectangle",
    "EllipseShape",
    "Triangle",
    "Polygon",
    "Line",
    "CustomShape",
    "AiRemove",
    "AiCutout",
    "Hand",
    "Zoom",
];

const PHOTOGRAPHER: &[&str] = &[
    "Move",
    "RectMarquee",
    "EllipseMarquee",
    "Lasso",
    "ObjectSelection",
    "QuickSelection",
    "Crop",
    "PerspectiveCrop",
    "Eyedropper",
    "SpotHealing",
    "Healing",
    "Patch",
    "ContentAwareMove",
    "RedEye",
    "CloneStamp",
    "MagicEraser",
    "AiRemove",
    "Brush",
    "Eraser",
    "Gradient",
    "Dodge",
    "Burn",
    "Sponge",
    "Blur",
    "Sharpen",
    "Type",
    "Hand",
    "Zoom",
];

const ESSENTIALS: &[&str] = &[
    "Move",
    "RectMarquee",
    "Lasso",
    "ObjectSelection",
    "QuickSelection",
    "Crop",
    "Eyedropper",
    "SpotHealing",
    "Brush",
    "CloneStamp",
    "Eraser",
    "Gradient",
    "Pen",
    "Type",
    "Rectangle",
    "Hand",
    "Zoom",
];

const RETOUCHING: &[&str] = &[
    "Move",
    "Lasso",
    "QuickSelection",
    "Eyedropper",
    "SpotHealing",
    "Healing",
    "Patch",
    "ContentAwareMove",
    "RedEye",
    "Brush",
    "CloneStamp",
    "HistoryBrush",
    "Eraser",
    "Blur",
    "Sharpen",
    "Smudge",
    "Dodge",
    "Burn",
    "Sponge",
    "Hand",
    "Zoom",
];

const AI: &[&str] = &["Move", "ObjectSelection", "QuickSelection", "Crop", "SpotHealing", "ContentAwareMove", "AiRemove", "AiCutout", "Hand", "Zoom"];

fn built(id: &str, name: &str, tools: &[&str]) -> ToolSet {
    ToolSet { id: id.into(), name: name.into(), persona: Persona::Pixel, tools: tools.iter().map(|t| t.to_string()).collect() }
}

/// The built-in sets, All Tools first.
pub fn builtins() -> Vec<ToolSet> {
    vec![
        built(DEFAULT_ID, "All Tools", ALL_TOOLS),
        built("photographer", "Photographer", PHOTOGRAPHER),
        built("essentials", "Essentials", ESSENTIALS),
        built("retouching", "Retouching", RETOUCHING),
        built("ai", "AI", AI),
    ]
}

/// Longest set name.
pub const MAX_NAME: usize = 40;

impl ToolbarCustomization {
    pub fn is_builtin(id: &str) -> bool {
        builtins().iter().any(|b| b.id == id)
    }

    /// Every set: the built-ins, then the user's.
    pub fn all_sets(&self) -> Vec<ToolSet> {
        let mut v = builtins();
        v.extend(self.sets.iter().cloned());
        v
    }

    /// The set in use (All Tools when the saved id no longer exists).
    pub fn active(&self) -> ToolSet {
        let sets = self.all_sets();
        let want = if self.active_set.is_empty() { DEFAULT_ID } else { self.active_set.as_str() };
        sets.iter().find(|s| s.id == want).or_else(|| sets.first()).cloned().unwrap_or_default()
    }

    /// Preferences saved before tool sets had a hidden list and an order: they become the custom
    /// set "My Tools" (made active), and the old fields are cleared. Nothing changes for a
    /// default toolbar.
    pub fn migrate(&mut self) {
        if self.hidden.is_empty() && self.order.is_empty() {
            return;
        }
        if self.sets.is_empty() && self.active_set.is_empty() {
            let known = |t: &str| ALL_TOOLS.iter().any(|a| a.eq_ignore_ascii_case(t));
            let mut tools: Vec<String> = Vec::new();
            for t in self.order.iter().filter(|t| known(t)) {
                let canon = ALL_TOOLS.iter().find(|a| a.eq_ignore_ascii_case(t)).map(|a| a.to_string()).unwrap_or_default();
                if !tools.contains(&canon) {
                    tools.push(canon);
                }
            }
            for t in ALL_TOOLS {
                if !tools.iter().any(|x| x == t) {
                    tools.push(t.to_string());
                }
            }
            let hidden: Vec<String> = self.hidden.iter().map(|h| h.to_ascii_lowercase()).collect();
            tools.retain(|t| !hidden.contains(&t.to_ascii_lowercase()));
            self.sets.push(ToolSet { id: "custom-1".into(), name: "My Tools".into(), persona: Persona::Pixel, tools });
            self.active_set = "custom-1".into();
        }
        self.hidden.clear();
        self.order.clear();
    }

    fn next_id(&self) -> String {
        (1u32..).map(|n| format!("custom-{n}")).find(|id| !self.sets.iter().any(|s| &s.id == id)).unwrap_or_default()
    }

    /// A free name: `base`, else `base 2`, `base 3` …
    pub fn unique_name(&self, base: &str) -> String {
        let taken = |n: &str| self.all_sets().iter().any(|s| s.name.eq_ignore_ascii_case(n));
        let base: String = base.trim().chars().take(MAX_NAME).collect();
        if !base.is_empty() && !taken(&base) {
            return base;
        }
        let base = if base.is_empty() { "My Tools".to_string() } else { base };
        (2u32..).map(|n| format!("{base} {n}")).find(|n| !taken(n)).unwrap_or(base)
    }
}

fn str_param<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
    p.get(k).and_then(Value::as_str)
}

fn describe(c: &ToolbarCustomization) -> Value {
    let sets: Vec<Value> = c
        .all_sets()
        .iter()
        .map(|s| json!({"id": s.id, "name": s.name, "persona": s.persona, "builtin": ToolbarCustomization::is_builtin(&s.id), "tools": s.tools}))
        .collect();
    json!({"active": c.active().id, "sets": sets})
}

/// A set by id, or by name (case-insensitive).
fn find(c: &ToolbarCustomization, p: &Value) -> R<ToolSet> {
    let sets = c.all_sets();
    let hit = match (str_param(p, "id"), str_param(p, "name")) {
        (Some(id), _) => sets.iter().find(|s| s.id == id || s.name.eq_ignore_ascii_case(id)),
        (None, Some(n)) => sets.iter().find(|s| s.name.eq_ignore_ascii_case(n)),
        _ => return Err("toolset needs an `id` (or a `name`)".into()),
    };
    hit.cloned().ok_or_else(|| "no such tool set".into())
}

fn clean_tools(p: &Value) -> R<Option<Vec<String>>> {
    let Some(arr) = p.get("tools").and_then(Value::as_array) else { return Ok(None) };
    let mut out: Vec<String> = Vec::new();
    for t in arr.iter().filter_map(Value::as_str) {
        let canon = ALL_TOOLS.iter().find(|a| a.eq_ignore_ascii_case(t)).ok_or_else(|| format!("unknown tool `{t}`"))?;
        if !out.iter().any(|o| o == canon) {
            out.push(canon.to_string());
        }
    }
    if out.is_empty() {
        return Err("a tool set needs at least one tool".into());
    }
    Ok(Some(out))
}

fn check_name(c: &ToolbarCustomization, name: &str, except: Option<&str>) -> R<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("a tool set needs a name".into());
    }
    if n.chars().count() > MAX_NAME {
        return Err(format!("a tool set name is at most {MAX_NAME} characters"));
    }
    if c.all_sets().iter().any(|s| s.name.eq_ignore_ascii_case(n) && Some(s.id.as_str()) != except) {
        return Err(format!("a tool set named \"{n}\" already exists"));
    }
    Ok(n.to_string())
}

fn list_impl(s: &mut Session, _: &Value) -> R<Value> {
    Ok(describe(&s.prefs().toolbar))
}

fn select_impl(s: &mut Session, p: &Value) -> R<Value> {
    let set = find(&s.prefs().toolbar, p)?;
    s.edit_prefs(|pr| pr.toolbar.active_set = set.id.clone());
    Ok(describe(&s.prefs().toolbar))
}

/// `toolset.save`: create a set (`name`, `tools`, or `from` an existing set), or update a custom
/// one (`id`, with `tools` and/or `name`). Built-ins are read-only: saving changes to one is a
/// new set (omit `id`, give `from`).
fn save_impl(s: &mut Session, p: &Value) -> R<Value> {
    let c = s.prefs().toolbar.clone();
    let tools = clean_tools(p)?;
    if let Some(id) = str_param(p, "id") {
        if ToolbarCustomization::is_builtin(id) {
            return Err("built-in tool sets are read-only: save a copy (omit `id`, give `from`)".into());
        }
        let name = match str_param(p, "name") {
            Some(n) => Some(check_name(&c, n, Some(id))?),
            None => None,
        };
        if !c.sets.iter().any(|x| x.id == id) {
            return Err("no such tool set".into());
        }
        s.edit_prefs(|pr| {
            if let Some(set) = pr.toolbar.sets.iter_mut().find(|x| x.id == id) {
                if let Some(t) = tools {
                    set.tools = t;
                }
                if let Some(n) = name {
                    set.name = n;
                }
            }
        });
        return Ok(describe(&s.prefs().toolbar));
    }
    let from = match str_param(p, "from") {
        Some(f) => Some(find(&c, &json!({"id": f}))?),
        None => None,
    };
    let tools = tools.or_else(|| from.as_ref().map(|f| f.tools.clone())).ok_or_else(|| "toolset.save needs `tools` or `from`".to_string())?;
    let name = match str_param(p, "name") {
        Some(n) => check_name(&c, n, None)?,
        None => c.unique_name(&from.as_ref().map_or("My Tools".to_string(), |f| format!("{} copy", f.name))),
    };
    let id = c.next_id();
    s.edit_prefs(|pr| {
        pr.toolbar.sets.push(ToolSet { id: id.clone(), name, persona: from.map(|f| f.persona).unwrap_or_default(), tools });
        pr.toolbar.active_set = id.clone();
    });
    let mut out = describe(&s.prefs().toolbar);
    out["created"] = json!(id);
    Ok(out)
}

fn duplicate_impl(s: &mut Session, p: &Value) -> R<Value> {
    let set = find(&s.prefs().toolbar, p)?;
    let mut q = json!({"from": set.id});
    if let Some(n) = str_param(p, "newName") {
        q["name"] = json!(n);
    }
    save_impl(s, &q)
}

fn rename_impl(s: &mut Session, p: &Value) -> R<Value> {
    let set = find(&s.prefs().toolbar, p)?;
    if ToolbarCustomization::is_builtin(&set.id) {
        return Err("built-in tool sets cannot be renamed".into());
    }
    let new = str_param(p, "newName").or_else(|| str_param(p, "to")).ok_or_else(|| "toolset.rename needs `newName`".to_string())?;
    save_impl(s, &json!({"id": set.id, "name": new}))
}

fn delete_impl(s: &mut Session, p: &Value) -> R<Value> {
    let set = find(&s.prefs().toolbar, p)?;
    if ToolbarCustomization::is_builtin(&set.id) {
        return Err("built-in tool sets cannot be deleted".into());
    }
    s.edit_prefs(|pr| {
        pr.toolbar.sets.retain(|x| x.id != set.id);
        if pr.toolbar.active_set == set.id {
            pr.toolbar.active_set = DEFAULT_ID.into();
        }
    });
    Ok(describe(&s.prefs().toolbar))
}

/// `toolset.reset`: back to All Tools (the user's own sets stay).
fn reset_impl(s: &mut Session, _: &Value) -> R<Value> {
    s.edit_prefs(|pr| pr.toolbar.active_set = DEFAULT_ID.into());
    Ok(describe(&s.prefs().toolbar))
}

macro_rules! wrap {
    ($name:ident, $imp:ident, $id:literal) => {
        fn $name(s: &mut Session, p: &Value) -> Result<Value> {
            $imp(s, p).map_err(|msg| EngineError::BadParams { cmd: $id.into(), msg })
        }
    };
}
wrap!(list, list_impl, "toolset.list");
wrap!(select, select_impl, "toolset.select");
wrap!(save, save_impl, "toolset.save");
wrap!(duplicate, duplicate_impl, "toolset.duplicate");
wrap!(rename, rename_impl, "toolset.rename");
wrap!(delete, delete_impl, "toolset.delete");
wrap!(reset, reset_impl, "toolset.reset");

macro_rules! spec {
    ($id:literal, $label:literal, $params:literal, $run:expr, $journal:expr) => {
        CommandSpec { id: $id, label: $label, menu: &[], shortcut: None, params: $params, enabled: always, run: $run, journal: $journal }
    };
}

fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}

/// The `toolset.*` command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!("toolset.list", "List Tool Sets", r##"{} → {"active":id,"sets":[{id,name,persona,builtin,tools}]}"##, list, false),
        spec!("toolset.select", "Select Tool Set", r##"{"id":"allTools|photographer|essentials|retouching|ai|custom-N"} (or "name")"##, select, true),
        spec!(
            "toolset.save",
            "Save Tool Set",
            r##"{"id":"custom-N"?,"name":str?,"tools":["<tool>",…]?,"from":"<set id>"?} creates a set (selected) when `id` is omitted; built-ins are read-only"##,
            save,
            true
        ),
        spec!("toolset.duplicate", "Duplicate Tool Set", r##"{"id":"<set>","newName":str?}"##, duplicate, true),
        spec!("toolset.rename", "Rename Tool Set", r##"{"id":"custom-N","newName":str}"##, rename, true),
        spec!("toolset.delete", "Delete Tool Set", r##"{"id":"custom-N"} (custom sets only)"##, delete, true),
        spec!("toolset.reset", "Reset Toolbar to All Tools", r##"{}"##, reset, true),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session() -> Session {
        Session::new()
    }

    #[test]
    fn built_in_sets_use_known_tools_and_all_tools_is_the_default() {
        for b in builtins() {
            assert!(!b.tools.is_empty());
            for t in &b.tools {
                assert!(ALL_TOOLS.contains(&t.as_str()), "{} lists unknown {t}", b.id);
            }
        }
        let s = session();
        assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID);
        assert_eq!(builtins().iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), ["allTools", "photographer", "essentials", "retouching", "ai"]);
        assert_eq!(builtins()[0].tools.len(), ALL_TOOLS.len());
    }

    #[test]
    fn create_rename_duplicate_select_and_delete_custom_sets() {
        let mut s = session();
        let r = s.execute("toolset.save", json!({"name": "Mine", "tools": ["move", "Brush"]})).unwrap();
        let id = r["created"].as_str().unwrap().to_string();
        assert_eq!(r["active"], id);
        assert_eq!(s.prefs().toolbar.active().tools, ["Move", "Brush"]);
        assert!(s.execute("toolset.save", json!({"name": "mine", "tools": ["Move"]})).is_err(), "names are unique");
        assert!(s.execute("toolset.save", json!({"name": "X", "tools": ["Nope"]})).is_err());
        s.execute("toolset.rename", json!({"id": id, "newName": "Faces"})).unwrap();
        assert_eq!(s.prefs().toolbar.active().name, "Faces");
        let d = s.execute("toolset.duplicate", json!({"id": "retouching"})).unwrap();
        assert_eq!(s.prefs().toolbar.active().name, "Retouching copy");
        assert_eq!(s.prefs().toolbar.active().tools.len(), RETOUCHING.len());
        s.execute("toolset.select", json!({"id": "ai"})).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, "ai");
        let copy = d["created"].as_str().unwrap().to_string();
        s.execute("toolset.delete", json!({"id": copy})).unwrap();
        assert_eq!(s.prefs().toolbar.sets.len(), 1);
        s.execute("toolset.select", json!({"id": id})).unwrap();
        s.execute("toolset.delete", json!({"id": id})).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID, "deleting the active set falls back to All Tools");
    }

    #[test]
    fn built_ins_are_read_only() {
        let mut s = session();
        for id in ["allTools", "photographer", "essentials", "retouching", "ai"] {
            assert!(s.execute("toolset.delete", json!({"id": id})).is_err(), "{id}");
            assert!(s.execute("toolset.rename", json!({"id": id, "newName": "Z"})).is_err(), "{id}");
            assert!(s.execute("toolset.save", json!({"id": id, "tools": ["Move"]})).is_err(), "{id}");
        }
        assert_eq!(s.prefs().toolbar.sets.len(), 0);
    }

    #[test]
    fn old_hidden_and_order_become_my_tools() {
        let mut s = session();
        s.load_prefs_json(r#"{"toolbar": {"hidden": ["Sponge", "burn"], "order": ["Brush"]}}"#).unwrap();
        let t = &s.prefs().toolbar;
        assert!(t.hidden.is_empty() && t.order.is_empty());
        let a = t.active();
        assert_eq!(a.name, "My Tools");
        assert_eq!(a.tools.first().map(String::as_str), Some("Brush"));
        assert!(!a.tools.iter().any(|x| x == "Sponge" || x == "Burn"));
        assert_eq!(a.tools.len(), ALL_TOOLS.len() - 2);
        // a default toolbar stays on All Tools
        let mut s = session();
        s.load_prefs_json(r#"{"toolbar": {}}"#).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID);
        assert!(s.prefs().toolbar.sets.is_empty());
        // and preferences with no toolbar key at all
        s.load_prefs_json(r#"{}"#).unwrap();
        assert_eq!(s.prefs().toolbar.active().id, DEFAULT_ID);
    }

    #[test]
    fn sets_survive_a_prefs_round_trip() {
        let mut s = session();
        s.execute("toolset.save", json!({"name": "Mine", "tools": ["Move", "Zoom"]})).unwrap();
        let json = s.prefs_to_json();
        let mut t = session();
        t.load_prefs_json(&json).unwrap();
        assert_eq!(t.prefs().toolbar.sets, s.prefs().toolbar.sets);
        assert_eq!(t.prefs().toolbar.active().name, "Mine");
        assert_eq!(t.prefs().toolbar.active().persona, Persona::Pixel);
    }
}
