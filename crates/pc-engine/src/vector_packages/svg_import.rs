//! SVG Open/Place/Paste: guarded, detached previews and one atomic publication.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::{
    Affine, ColorMode, Document, Layer, LayerContent, LayerId, SampleType,
    vector_design::exchange::{ImportOptions, ImportedScene, VectorWarning},
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
fn translate(layer: &mut Layer, offset: [i32; 2]) -> Result<()> {
    let affine = Affine::translate(f64::from(offset[0]), f64::from(offset[1]));
    if let Some(m) = &mut layer.vector_mask {
        m.path = m.path.transform(&affine);
        check_path(&m.path)?;
    }
    if let Some(p) = &mut layer.vector.clip {
        *p = p.transform(&affine);
        check_path(p)?;
    }
    if let Some(m) = &mut layer.mask {
        m.surface = m.surface.translated(offset[0], offset[1]);
    }
    match &mut layer.content {
        LayerContent::Shape(shape) => {
            shape.path = shape.path.transform(&affine);
            check_path(&shape.path)?;
            shape.cache = shape.cache.as_ref().map(|s| s.translated(offset[0], offset[1]));
        }
        LayerContent::Text(text) => {
            text.transform = affine.mul(&text.transform);
            if text.transform.m.iter().any(|v| !v.is_finite() || v.abs() > 1_000_000.0) {
                return Err(error("SVG text placement exceeds coordinate bounds"));
            }
            text.cache = text.cache.as_ref().map(|s| s.translated(offset[0], offset[1]));
        }
        LayerContent::Raster(surface) => {
            *surface = surface.translated(offset[0], offset[1]);
        }
        LayerContent::Group(group) => {
            for child in &mut group.children {
                translate(child, offset)?;
            }
        }
        _ => return Err(error("unexpected content in SVG scene")),
    }
    Ok(())
}
fn check_path(path: &photocraft_doc::Path) -> Result<()> {
    crate::vector_api::validate_path(path)?;
    if path.control_bounds().is_some_and(|b| [b.0, b.1, b.2, b.3].iter().any(|v| v.abs() > 1_000_000.0)) {
        return Err(error("SVG placement exceeds coordinate bounds"));
    }
    Ok(())
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
/// File adapters all return the shared scene; Open wraps it as a new native document.
pub fn document(scene: ImportedScene, name: &str) -> (Document, Vec<VectorWarning>) {
    let mut doc = Document::new(name, scene.size, ColorMode::Rgb, SampleType::F32);
    doc.resolution_dpi = scene.resolution_dpi;
    doc.design_units = scene.units;
    doc.layers = scene.layers;
    doc.symbols = scene.symbols;
    (doc, scene.warnings)
}
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
