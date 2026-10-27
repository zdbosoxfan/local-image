//! PDF Open/Place: guarded, detached previews and one atomic publication.
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
    pub pdf: Vec<u8>,
    #[serde(default)]
    pub pages: Vec<usize>,
    #[serde(default)]
    pub page_box: photocraft_pdf::PageBox,
    #[serde(default)]
    pub ignore_rotation: bool,
    pub targets: Vec<LayerId>,
    #[serde(default)]
    pub offset: [i32; 2],
}
/// Begin captures a document/revision/anchor once, before decoding or preview work.
pub fn capture(session: &Session, pdf: Vec<u8>) -> Result<Place> {
    let s = session.active().ok_or(EngineError::NoDocument)?;
    let request = Place {
        document: s.doc.id.0,
        revision: s.revision,
        above: s.active_layer,
        pdf,
        pages: Vec::new(),
        page_box: photocraft_pdf::PageBox::Crop,
        ignore_rotation: false,
        targets: s.selected_layers.clone(),
        offset: [0; 2],
    };
    check(session, &request)?;
    Ok(request)
}
fn error(s: impl ToString) -> EngineError {
    EngineError::Other(s.to_string())
}
pub fn check(session: &Session, request: &Place) -> Result<()> {
    let s = session.active().ok_or(EngineError::NoDocument)?;
    if s.doc.id.0 != request.document || s.revision != request.revision {
        return Err(error("PDF placement is stale; start again"));
    }
    if s.selected_layers != request.targets || s.active_layer != request.above {
        return Err(error("PDF placement selection changed; start again"));
    }
    for &id in &request.targets {
        if s.doc.layer(id).is_none() {
            return Err(EngineError::NoLayer(id));
        }
    }
    if request.offset.iter().any(|v| v.unsigned_abs() > 1_000_000) {
        return Err(error("PDF placement exceeds coordinate bounds"));
    }
    if let Some(id) = request.above {
        let mut path = s.doc.path_of(id).ok_or(EngineError::NoLayer(id))?;
        path.pop();
        if path.iter().enumerate().any(|(i, _)| s.doc.layer_at(&path[..=i]).is_some_and(|p| s.doc.effective_locks(p.id).all)) {
            return Err(error("PDF destination group is locked"));
        }
    }
    Ok(())
}
pub fn decode(session: &Session, request: &Place) -> Result<ImportedScene> {
    check(session, request)?;
    let s = session.active().ok_or(EngineError::NoDocument)?;
    let options = ImportOptions { dpi: s.doc.resolution_dpi, pages: request.pages.clone(), ..Default::default() };
    photocraft_pdf::import_with(
        &request.pdf,
        &options,
        &photocraft_pdf::PdfOptions { page_box: request.page_box, ignore_rotation: request.ignore_rotation },
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map_err(error)
}
/// A preview is a clone of the document with the already prepared scene inserted. The
/// parser and native caches run once at Begin, never on every pointer update.
pub fn preview(session: &Session, request: &Place, scene: &ImportedScene) -> Result<(Document, LayerId)> {
    check(session, request)?;
    let s = session.active().ok_or(EngineError::NoDocument)?;
    let mut doc = (*s.doc).clone();
    let mut group = Layer::group("PDF", scene.layers.clone());
    // A placed file keeps its own viewport when the destination is larger and
    // when a native shape cache is rebuilt after editing.
    let (w, h) = (f64::from(scene.size.width), f64::from(scene.size.height));
    let frame = photocraft_doc::Path::new(vec![photocraft_doc::Subpath::polygon(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)])]);
    group.vector.clip = Some(frame.clone());
    group.vector_mask = Some(photocraft_doc::VectorMask::new(frame));
    translate(&mut group, request.offset, "PDF")?;
    let id = doc.insert_above(request.above, group);
    if doc.max_group_depth() > photocraft_doc::MAX_GROUP_DEPTH {
        return Err(error("PDF placement exceeds document group depth"));
    }
    Ok((doc, id))
}
pub fn publish(session: &mut Session, request: &Place, scene: &ImportedScene) -> Result<Value> {
    let (prepared, id) = preview(session, request, scene)?;
    check(session, request)?;
    session.edit("Place PDF", move |doc, active| {
        *doc = prepared;
        *active = Some(id);
        Ok(())
    })?;
    Ok(json!({"layer":id.0,"warnings":scene.warnings}))
}
fn place(session: &mut Session, params: &Value) -> Result<Value> {
    let request: Place = serde_json::from_value(params.clone()).map_err(error)?;
    let scene = decode(session, &request)?;
    publish(session, &request, &scene)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Open {
    pdf: Vec<u8>,
    #[serde(default)]
    pages: Vec<usize>,
    #[serde(default)]
    page_box: photocraft_pdf::PageBox,
    #[serde(default)]
    ignore_rotation: bool,
    #[serde(default = "default_dpi")]
    dpi: f32,
    #[serde(default = "default_name")]
    name: String,
}
fn default_dpi() -> f32 {
    72.0
}
fn default_name() -> String {
    "PDF".into()
}
pub use super::import_scene::document;
fn open(session: &mut Session, params: &Value) -> Result<Value> {
    let request: Open = serde_json::from_value(params.clone()).map_err(error)?;
    let scene = photocraft_pdf::import_with(
        &request.pdf,
        &ImportOptions { dpi: request.dpi, pages: request.pages, ..Default::default() },
        &photocraft_pdf::PdfOptions { page_box: request.page_box, ignore_rotation: request.ignore_rotation },
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map_err(error)?;
    let (doc, warnings) = document(scene, &request.name);
    let id = doc.id.0;
    session.add_document(doc, None);
    Ok(json!({"document":id,"warnings":warnings}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "file.pdf.import",
            label: "Open PDF",
            menu: &[],
            shortcut: None,
            params: "{pdf:bytes,pages?:[usize],page_box?:Crop|Media,ignore_rotation?:bool,dpi?:number,name?:string}",
            enabled: crate::commands::always,
            run: open,
            journal: true,
        },
        CommandSpec {
            id: "file.pdf.place",
            label: "Place PDF",
            menu: &[],
            shortcut: None,
            params: "{document:u64,revision:u64,above:id|null,targets:[id],pdf:bytes,pages?:[usize],page_box?:Crop|Media,ignore_rotation?:bool,offset?:[i32,i32]}",
            enabled: crate::vector_cmds::has_doc,
            run: place,
            journal: true,
        },
    ]
}

#[cfg(test)]
#[path = "pdf_import/tests.rs"]
mod tests;
