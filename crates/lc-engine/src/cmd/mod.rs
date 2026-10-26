//! The command registry.
//!
//! Command ids group by area: `library.*` (view/selection), `photo.*` (rating, flags, rotate,
//! delete), `album.*`, `develop.*` (settings), `crop.*`, `mask.*`, `preset.*`, `version.*`,
//! `edit.*` (undo/redo), and queries (`catalog.query`, `photo.inspect`, `develop.get`…).

mod before;
pub(crate) mod browse;
mod color;
pub(crate) mod convert;
pub mod cull;
pub mod curves;
mod develop;
mod edit;
mod export;
pub mod filters;
pub mod keywords;
pub mod library;
mod library_safety;
pub mod lut_profiles;
pub mod manage;
mod masks;
mod merge;
pub mod metadata;
pub mod missing;
mod negative;
mod organize;
mod prefs;
mod preset_files;
pub mod previews;
mod query;
pub mod save_over;
mod smart_sort;
mod smart_sort_people;
pub mod upright;
mod xmp;

use serde_json::Value;

use crate::{EngineError, Result, Session};

// Command-model types specialised to this engine's Session.
pub type Run = li_commands::Run<Session, EngineError>;
pub type Enabled = li_commands::Enabled<Session>;
pub type CommandSpec = li_commands::CommandSpec<Session, EngineError>;

// enablement predicates

pub fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}
pub fn has_active(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no photo selected".into())
}
pub fn has_selection(s: &Session) -> std::result::Result<(), String> {
    if s.selection.ids.is_empty() && s.selection.active.is_none() { Err("no photos selected".into()) } else { Ok(()) }
}
pub fn can_undo(s: &Session) -> std::result::Result<(), String> {
    let changed_interaction = s.interaction.as_ref().is_some_and(|i| s.develop_of(i.photo).is_some_and(|current| current != i.original));
    if s.undo_entry().is_some() || changed_interaction { Ok(()) } else { Err("nothing to undo".into()) }
}
pub fn can_redo(s: &Session) -> std::result::Result<(), String> {
    if s.redo_entry().is_none() { Err("nothing to redo".into()) } else { Ok(()) }
}
pub fn has_clipboard(s: &Session) -> std::result::Result<(), String> {
    has_selection(s)?;
    if s.clipboard.is_none() { Err("no settings copied".into()) } else { Ok(()) }
}

macro_rules! cmd {
    ($id:literal, $label:literal, [$($m:literal),*], $sc:expr, $params:literal, $en:expr, $run:expr) => {
        $crate::cmd::CommandSpec { id: $id, label: $label, menu: &[$($m),*], shortcut: $sc, params: $params, enabled: $en, run: $run, journal: true }
    };
    (query $id:literal, $label:literal, [$($m:literal),*], $sc:expr, $params:literal, $en:expr, $run:expr) => {
        $crate::cmd::CommandSpec { id: $id, label: $label, menu: &[$($m),*], shortcut: $sc, params: $params, enabled: $en, run: $run, journal: false }
    };
}
pub(crate) use cmd;

pub fn command_specs() -> &'static [CommandSpec] {
    static SPECS: std::sync::OnceLock<Vec<CommandSpec>> = std::sync::OnceLock::new();
    SPECS.get_or_init(|| {
        let mut v = Vec::new();
        v.extend(edit::specs());
        v.extend(library::specs());
        v.extend(develop::specs());
        v.extend(negative::specs());
        v.extend(color::specs());
        v.extend(curves::specs());
        v.extend(masks::specs());
        v.extend(organize::specs());
        v.extend(keywords::specs());
        v.extend(manage::specs());
        v.extend(previews::specs());
        v.extend(library_safety::specs());
        v.extend(lut_profiles::specs());
        v.extend(cull::specs());
        v.extend(smart_sort::specs());
        v.extend(smart_sort_people::specs());
        v.extend(convert::specs());
        v.extend(convert::edit_specs());
        v.extend(merge::specs());
        v.extend(query::specs());
        v.extend(xmp::specs());
        v.extend(preset_files::specs());
        v.extend(prefs::specs());
        v.extend(export::specs());
        v.extend(save_over::specs());
        v.extend(before::specs());
        v.extend(browse::specs());
        v.extend(missing::specs());
        v.extend(metadata::specs());
        v.extend(filters::specs());
        v.extend(crate::enhance::cmds::specs());
        v.extend(upright::specs());
        v
    })
}

pub fn find_command(id: &str) -> Option<&'static CommandSpec> {
    command_specs().iter().find(|c| c.id == id)
}

// param helpers

pub(crate) fn photo_ids(p: &Value, cmd: &str) -> Result<Option<Vec<lightcraft_catalog::PhotoId>>> {
    match p.get("ids") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_u64().map(lightcraft_catalog::PhotoId).ok_or_else(|| bad(cmd, "`ids` must be photo ids (whole numbers)")))
            .collect::<Result<Vec<_>>>()
            .map(Some),
        Some(_) => Err(bad(cmd, "`ids` must be a list of photo ids")),
    }
}

/// The photo `id` names: `None` when it is missing or null (the command then uses the active
/// photo), else that id. An `id` that isn't a whole number is an error, never `None`: falling back
/// would act on the active photo instead of the one the call meant (`{"id": "5"}`).
pub(crate) fn photo_id(p: &Value, cmd: &str) -> Result<Option<lightcraft_catalog::PhotoId>> {
    match p.get("id") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_u64().map(|n| Some(lightcraft_catalog::PhotoId(n))).ok_or_else(|| bad(cmd, "`id` must be a photo id (a whole number)")),
    }
}

/// Missing and null indexes use the command's default; malformed indexes never do.
pub(crate) fn optional_index(p: &Value, key: &str, cmd: &str) -> Result<Option<usize>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| bad(cmd, format!("`{key}` must be a non-negative whole number"))),
    }
}

pub(crate) fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}
pub(crate) fn f64_req(p: &Value, key: &str, cmd: &str) -> Result<f64> {
    p.get(key).and_then(Value::as_f64).ok_or_else(|| bad(cmd, format!("missing number `{key}`")))
}
/// A non-negative whole number that fits `usize`. Keep integer JSON exact; accept integral
/// floating-point values for compatibility, without truncating or saturating their conversion.
pub(crate) fn index_req(p: &Value, key: &str, cmd: &str) -> Result<usize> {
    let invalid = || bad(cmd, format!("`{key}` must be a non-negative integer"));
    if let Some(i) = p.get(key).and_then(Value::as_u64) {
        return usize::try_from(i).map_err(|_| invalid());
    }
    let i = f64_req(p, key, cmd)?;
    // The exclusive bound is exactly representable, unlike usize::MAX on 64-bit targets.
    let upper_bound = 2.0_f64.powi(usize::BITS as i32);
    if !(i.is_finite() && i >= 0.0 && i.fract() == 0.0 && i < upper_bound) {
        return Err(invalid());
    }
    Ok(i as usize)
}
pub(crate) fn f64_or(p: &Value, key: &str, d: f64) -> f64 {
    p.get(key).and_then(Value::as_f64).unwrap_or(d)
}
pub(crate) fn str_param<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str)
}
pub(crate) fn bool_or(p: &Value, key: &str, d: bool) -> bool {
    p.get(key).and_then(Value::as_bool).unwrap_or(d)
}
pub(crate) fn ok() -> Result<Value> {
    Ok(Value::Null)
}
pub(crate) fn point(p: &Value, key: &str) -> Option<lightcraft_geom::Point> {
    let a = p.get(key)?.as_array()?;
    Some(lightcraft_geom::Point::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::index_req;

    #[test]
    fn command_indices_preserve_integer_precision_and_check_usize_bounds() {
        for i in [0, 1, usize::MAX] {
            assert_eq!(index_req(&json!({"index": i}), "index", "test").unwrap(), i);
        }
        #[cfg(target_pointer_width = "64")]
        {
            let i = 9_007_199_254_740_993usize;
            assert_eq!(index_req(&json!({"index": i}), "index", "test").unwrap(), i, "integer JSON must not round through f64");
        }
        let upper_bound = 2.0_f64.powi(usize::BITS as i32);
        assert!(index_req(&json!({"index": upper_bound}), "index", "test").is_err(), "2^usize::BITS must not saturate");
        #[cfg(target_pointer_width = "32")]
        {
            assert!(index_req(&json!({"index": u64::from(u32::MAX) + 1}), "index", "test").is_err());
            assert_eq!(index_req(&json!({"index": f64::from(u32::MAX)}), "index", "test").unwrap(), usize::MAX);
        }
    }
}

#[cfg(test)]
mod tests_ids_params;
