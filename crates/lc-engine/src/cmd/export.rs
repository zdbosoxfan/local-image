//! Export presets: built-in ones ([`crate::export::builtin_presets`]) and the user's, saved with the
//! library. `app.export {preset}` (desktop app, MCP, `lightcraft-cli run`) starts from a preset's
//! params; the call's own params override them (see [`Session::export_params`]).

use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd};
use crate::export::{ExportPreset, builtin_presets};
use crate::{Result, Session};

impl Session {
    /// Built-in presets followed by the user's.
    pub fn all_export_presets(&self) -> Vec<(ExportPreset, bool)> {
        builtin_presets().into_iter().map(|p| (p, true)).chain(self.export_presets.iter().cloned().map(|p| (p, false))).collect()
    }

    /// `app.export` params with a named `preset` expanded: the preset's params, overridden by the
    /// call's own (`preset` itself removed). Unknown preset names are an error.
    pub fn export_params(&self, p: &Value) -> std::result::Result<Value, String> {
        let Some(name) = p.get("preset").and_then(Value::as_str) else { return Ok(p.clone()) };
        let (preset, _) = self
            .all_export_presets()
            .into_iter()
            .find(|(x, _)| x.name.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| format!("unknown export preset `{name}` (see export.presets)"))?;
        let mut out = preset.params;
        if let (Some(o), Some(own)) = (out.as_object_mut(), p.as_object()) {
            for (k, v) in own.iter().filter(|(k, _)| *k != "preset") {
                o.insert(k.clone(), v.clone());
            }
        }
        Ok(out)
    }
}

fn list(s: &Session) -> Value {
    Value::Array(s.all_export_presets().into_iter().map(|(p, builtin)| json!({"name": p.name, "builtin": builtin, "params": p.params})).collect())
}

/// Params that describe a destination or a selection, not the export settings.
const NOT_SETTINGS: &[&str] = &["ids", "path", "dir", "preset"];

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "export.savePreset";
    let name = p.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(ID, "missing `name`"))?;
    if builtin_presets().iter().any(|b| b.name.eq_ignore_ascii_case(name)) {
        return Err(bad(ID, format!("`{name}` is a built-in preset; choose another name")));
    }
    let mut params = match p.get("params") {
        Some(v @ Value::Object(_)) => v.clone(),
        Some(_) => return Err(bad(ID, "`params` must be an object of app.export params")),
        None => s.last_export.clone().ok_or_else(|| bad(ID, "no `params` and no previous export to save"))?,
    };
    if let Some(o) = params.as_object_mut() {
        o.retain(|k, _| !NOT_SETTINGS.contains(&k.as_str()));
    }
    let preset = ExportPreset { name: name.to_string(), params };
    match s.export_presets.iter_mut().find(|x| x.name.eq_ignore_ascii_case(name)) {
        Some(x) => *x = preset,
        None => s.export_presets.push(preset),
    }
    s.save_prefs()?;
    Ok(list(s))
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "export.deletePreset";
    let name = p.get("name").and_then(Value::as_str).ok_or_else(|| bad(ID, "missing `name`"))?;
    let before = s.export_presets.len();
    s.export_presets.retain(|x| !x.name.eq_ignore_ascii_case(name.trim()));
    if s.export_presets.len() == before {
        return Err(bad(ID, format!("no user export preset `{name}`")));
    }
    s.save_prefs()?;
    Ok(list(s))
}

/// Export ▸ Add to This Catalog (and Add to Stack): import exported files and stack each on
/// top of the photo it was exported from. `files` are `app.export` results (`{path, photo}`).
fn add_to_library(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "export.addToLibrary";
    let files: Vec<(String, Option<u64>)> = p
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| bad(ID, "missing `files` ([{path, photo}])"))?
        .iter()
        .filter_map(|f| Some((f.get("path")?.as_str()?.to_string(), f.get("photo").and_then(Value::as_u64))))
        .collect();
    let stack = p.get("stack").and_then(Value::as_bool).unwrap_or(false);
    if files.is_empty() {
        return Ok(json!({"imported": []}));
    }
    s.execute("library.import", &json!({"paths": files.iter().map(|f| f.0.clone()).collect::<Vec<_>>()}))?;
    let mut added = Vec::new();
    for (path, from) in files {
        let found = s.catalog.photos().find(|ph| match &ph.source {
            lightcraft_catalog::Source::File { path: q } => {
                *q == path || lightcraft_catalog::safe_file::same_file(std::path::Path::new(q), std::path::Path::new(&path))
            }
            _ => false,
        });
        let Some(new) = found.map(|ph| ph.id.0) else { continue };
        if stack && let Some(from) = from.filter(|f| *f != new && s.catalog.photo(lightcraft_catalog::PhotoId(*f)).is_some()) {
            let _ = s.execute("stack.group", &json!({"ids": [new, from], "top": new, "collapsed": false}));
        }
        added.push(new);
    }
    Ok(json!({"imported": added}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "export.presets",
            "Export Presets",
            [],
            None,
            "{} → [{name, builtin, params}] (use with app.export {preset: name, …overrides})",
            always,
            |s, _| Ok(list(s))
        ),
        cmd!(
            "export.savePreset",
            "Save Export Preset",
            [],
            None,
            "{name, params?: app.export params (default: the last export's)} — adds or replaces a user preset, saved with the library → presets",
            always,
            save
        ),
        cmd!("export.deletePreset", "Delete Export Preset", [], None, "{name} — removes a user preset → presets", always, delete),
        cmd!(
            "export.addToLibrary",
            "Add Exported Files to Library",
            [],
            None,
            "{files: [{path, photo?}] (app.export results), stack?: false} — import exported files into the library; with `stack`, each goes on top of a stack with the photo it was exported from → {imported: [ids]}",
            always,
            add_to_library
        ),
        cmd!(
            query "export.checkTarget",
            "Check Output Path",
            [],
            None,
            "{path} — fails when writing `path` would replace a photo's original (or its XMP sidecar) in the library; front ends call it before saving a render or screenshot to a user-given path → {path}",
            always,
            |s, p| {
                let path = p.get("path").and_then(Value::as_str).ok_or_else(|| bad("export.checkTarget", "missing `path`"))?;
                s.check_write_target(path).map_err(|e| bad("export.checkTarget", e))?;
                Ok(json!({"path": path}))
            }
        ),
    ]
}
