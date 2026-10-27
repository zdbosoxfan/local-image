//! SVG Open/Place/Paste: guarded, detached previews and one atomic publication.
#[cfg(test)]
use super::import_scene::check_path;
use super::import_scene::translate;
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::{
    Document, Layer, LayerId,
    vector_design::exchange::{ImportOptions, ImportedScene},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Place {
    pub document: u64,
    pub revision: u64,
    pub above: Option<LayerId>,
    pub svg: String,
    #[serde(default)]
    pub offset: [i32; 2],
}
/// Begin captures a document/revision/anchor once, before decoding or preview work.
pub fn capture(session: &Session, svg: String) -> Result<Place> {
    let s = session.active().ok_or(EngineError::NoDocument)?;
    let request = Place { document: s.doc.id.0, revision: s.revision, above: s.active_layer, svg, offset: [0; 2] };
    check(session, &request)?;
    Ok(request)
}
fn error(s: impl ToString) -> EngineError {
    EngineError::Other(s.to_string())
}
pub fn check(session: &Session, request: &Place) -> Result<()> {
    let s = session.active().ok_or(EngineError::NoDocument)?;
    if s.doc.id.0 != request.document || s.revision != request.revision {
        return Err(error("SVG placement is stale; start again"));
    }
    if request.offset.iter().any(|v| v.unsigned_abs() > 1_000_000) {
        return Err(error("SVG placement exceeds coordinate bounds"));
    }
    if let Some(id) = request.above {
        let mut path = s.doc.path_of(id).ok_or(EngineError::NoLayer(id))?;
        path.pop();
        if !path.is_empty() && s.doc.layer_at(&path).is_some_and(|p| s.doc.effective_locks(p.id).all) {
            return Err(error("SVG destination group is locked"));
        }
    }
    Ok(())
}
pub fn decode(session: &Session, request: &Place) -> Result<ImportedScene> {
    check(session, request)?;
    let s = session.active().ok_or(EngineError::NoDocument)?;
    let options = ImportOptions { dpi: s.doc.resolution_dpi, ..Default::default() };
    photocraft_svg::import(request.svg.as_bytes(), &options).map_err(error)
}
/// A preview is a clone of the document with the already prepared scene inserted. The
/// parser and native caches run once at Begin, never on every pointer update.
pub fn preview(session: &Session, request: &Place, scene: &ImportedScene) -> Result<(Document, LayerId)> {
    check(session, request)?;
    let s = session.active().ok_or(EngineError::NoDocument)?;
    let mut doc = (*s.doc).clone();
    let mut group = Layer::group("SVG", scene.layers.clone());
    // A placed file keeps its own viewport when the destination is larger and
    // when a native shape cache is rebuilt after editing.
    let (w, h) = (f64::from(scene.size.width), f64::from(scene.size.height));
    let frame = photocraft_doc::Path::new(vec![photocraft_doc::Subpath::polygon(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)])]);
    group.vector.clip = Some(frame.clone());
    group.vector_mask = Some(photocraft_doc::VectorMask::new(frame));
    translate(&mut group, request.offset)?;
    let id = doc.insert_above(request.above, group);
    if doc.max_group_depth() > photocraft_doc::MAX_GROUP_DEPTH {
        return Err(error("SVG placement exceeds document group depth"));
    }
    Ok((doc, id))
}
fn place(session: &mut Session, params: &Value) -> Result<Value> {
    let request: Place = serde_json::from_value(params.clone()).map_err(error)?;
    let scene = decode(session, &request)?;
    commit_prepared(session, &request, &scene)
}
pub fn commit_prepared(session: &mut Session, request: &Place, scene: &ImportedScene) -> Result<Value> {
    let (prepared, id) = preview(session, request, scene)?;
    check(session, request)?;
    session.edit("Place SVG", move |doc, active| {
        *doc = prepared;
        *active = Some(id);
        Ok(())
    })?;
    Ok(json!({"layer":id.0,"warnings":scene.warnings}))
}
/// Publish an already decoded dialog scene with the normal command bookkeeping.
/// The command dispatcher uses `commit_prepared` and records its own bookkeeping.
pub fn publish_prepared(session: &mut Session, request: &Place, scene: &ImportedScene, paste: bool) -> Result<Value> {
    crate::vector_integration::initialize();
    let command = if paste { "file.svg.paste" } else { "file.svg.place" };
    let params = serde_json::to_value(request).map_err(error)?;
    if let Some(why) = session.disabled_reason_with(command, &params) {
        return Err(EngineError::Disabled(command.to_string(), why));
    }
    let result = commit_prepared(session, request, scene)?;
    session.after_command(command, params, true);
    Ok(result)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Open {
    svg: String,
    #[serde(default = "default_dpi")]
    dpi: f32,
    #[serde(default = "default_name")]
    name: String,
}
fn default_dpi() -> f32 {
    72.0
}
fn default_name() -> String {
    "SVG".into()
}
pub use super::import_scene::document;
fn open(session: &mut Session, params: &Value) -> Result<Value> {
    let request: Open = serde_json::from_value(params.clone()).map_err(error)?;
    let scene = photocraft_svg::import(request.svg.as_bytes(), &ImportOptions { dpi: request.dpi, ..Default::default() }).map_err(error)?;
    let (doc, warnings) = document(scene, &request.name);
    let id = doc.id.0;
    session.add_document(doc, None);
    Ok(json!({"document":id,"warnings":warnings}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "file.svg.import",
            label: "Open SVG",
            menu: &[],
            shortcut: None,
            params: "{svg:string,dpi?:number,name?:string}",
            enabled: crate::commands::always,
            run: open,
            journal: true,
        },
        CommandSpec {
            id: "file.svg.place",
            label: "Place SVG",
            menu: &[],
            shortcut: None,
            params: "{document:u64,revision:u64,above:id|null,svg:string,offset?:[i32,i32]}",
            enabled: crate::vector_cmds::has_doc,
            run: place,
            journal: true,
        },
        CommandSpec {
            id: "file.svg.paste",
            label: "Paste SVG",
            menu: &[],
            shortcut: None,
            params: "same captured parameters as file.svg.place",
            enabled: crate::vector_cmds::has_doc,
            run: place,
            journal: true,
        },
    ]
}

#[cfg(test)]
#[path = "svg_import/tests.rs"]
mod tests;

#[cfg(test)]
mod prepared_publication_regressions {
    use super::*;
    #[test]
    fn prepared_svg_publication_records_one_command_without_redecoding() {
        let mut session = Session::new();
        session.execute("file.new", json!({"width":16,"height":16})).unwrap();
        let mut request =
            capture(&session, r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8"/></svg>"#.into()).unwrap();
        let scene = decode(&session, &request).unwrap();
        let before = session.active().unwrap().doc.layer_count();
        let journal = session.journal.len();
        // Publication must consume the prepared scene, not parse the source again.
        request.svg = "already decoded".into();
        publish_prepared(&mut session, &request, &scene, true).unwrap();
        assert_eq!(session.journal.len(), journal + 1);
        assert_eq!(session.journal.last().unwrap().0, "file.svg.paste");
        assert!(session.undo());
        assert_eq!(session.active().unwrap().doc.layer_count(), before);
    }
}
