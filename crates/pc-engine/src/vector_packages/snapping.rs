//! Package-owned, non-journaled snapping query commands. Queries never publish previews.
//! The query module is reachable here before package 00 adds the root vector_snap re-export.

#[path = "../vector_snap.rs"]
pub mod query;

use crate::{EngineError, Result, Session, commands::CommandSpec};
use query::{Angle, Cache, Exclusions, Settings, View};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Params {
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    view: View,
    #[serde(default)]
    exclude: Exclusions,
    #[serde(default)]
    point: Option<[f64; 2]>,
    #[serde(default)]
    rect: Option<[f64; 4]>,
    #[serde(default)]
    angle: Option<Angle>,
    #[serde(default)]
    bypass: bool,
    #[serde(default)]
    document: Option<u64>,
    #[serde(default)]
    revision: Option<u64>,
}
fn run(s: &mut Session, value: &Value) -> Result<Value> {
    let p: Params = serde_json::from_value(value.clone()).map_err(|e| EngineError::Other(format!("vector.snap.query: {e}")))?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if p.document.is_some_and(|id| id != st.doc.id.0) || p.revision.is_some_and(|r| r != st.revision) {
        return Err(EngineError::Other("stale vector snap query".into()));
    }
    p.settings.validate()?;
    p.view.tolerance(p.settings.tolerance_px)?;
    let cache = Cache::build(&st.doc, st.revision, p.exclude, query::installed_geometry())?;
    let result = match (p.point, p.rect) {
        (Some(point), None) => cache.point(point, &p.settings, p.view, p.angle, p.bypass)?,
        (None, Some(rect)) if p.angle.is_none() => cache.rect(rect, &p.settings, p.view, p.bypass)?,
        _ => return Err(EngineError::Other("vector.snap.query requires exactly one point or rect; angle requires point".into())),
    };
    Ok(json!({"document":st.doc.id.0,"revision":st.revision,"result":result}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "vector.snap.query",
        label: "Query Vector Snapping",
        menu: &[],
        shortcut: None,
        params: "{point:[x,y]|rect:[x0,y0,x1,y1],settings?:Settings,view?:{zoom,pixelsPerPoint},exclude?:{layers,paths,workPath},angle?:{origin,step,offset},bypass?:bool,document?:id,revision?:u64}",
        enabled: crate::vector_cmds::has_doc,
        run,
        journal: false,
    }]
}
