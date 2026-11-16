//! File › Open as Smart Object… (#11678): a file opens as a new untitled document with the file's
//! size, resolution, colour mode, bit depth and profile, holding one embedded smart object layer.
//!
//! The layer is an embedded smart object as File › Place Embedded makes one, at the file's own
//! size and position `0, 0`, put into the document before it is admitted, so opening is one
//! history state.

use super::{
    Affine, Arc, Document, EngineError, Layer, LayerContent, Result, Session, SmartObject, SmartSource, Surface, Value, file_name, flattened_checked, json,
    read_file, stem, str_param, untitled_name,
};

const CMD: &str = "file.openAsSmartObject";

/// The history step's name.
const LABEL: &str = "Open As Smart Object";

/// Opens the file as a document holding one embedded smart object. The read, decode and flatten
/// run on the job's worker (inline under `execute`); the document is built and admitted when the
/// job is applied, with File › Open's Color Settings handling: with Ask When Opening a mismatched
/// or missing profile is left as it is until `color.profileMismatch` answers. The reply has the
/// shape of File › Open's job: `{document, documentId, name, layer, warnings, color}`.
pub(super) fn open_as_smart_object(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path", CMD)?.to_string();
    let name = file_name(&path);
    crate::jobs::run(
        s,
        &format!("Opening {name}"),
        false,
        move |ctx| {
            ctx.report(0.0, "Reading");
            let bytes = read_file(&path)?;
            ctx.check()?;
            let r = ctx.stage(0.05, 0.9, "Decoding", |ctl| photocraft_io::import_with(&name, &bytes, ctl)).map_err(|e| match e {
                photocraft_io::IoError::Cancelled => EngineError::Cancelled,
                e => EngineError::Other(e.to_string()),
            })?;
            ctx.check()?;
            ctx.report(0.9, "Flattening");
            // The file as one image: the smart object's rendered contents.
            let pixels = flattened_checked(&r.document, r.document.pixel_format(), CMD)?;
            Ok((path, name, Arc::new(bytes), r, pixels))
        },
        |s, (path, name, bytes, r, pixels)| apply(s, &path, &name, bytes, r, pixels),
    )
}

fn apply(s: &mut Session, path: &str, file: &str, bytes: Arc<Vec<u8>>, r: photocraft_io::ImportResult, pixels: Surface) -> Result<Value> {
    let src = &r.document;
    let layer_name = stem(path);
    // "<name>-1", the next free number while a document with that name is open.
    let name = untitled_name(&layer_name, |n| s.documents().iter().any(|d| d.doc.name == n));

    let mut doc = Document::new(name.clone(), src.size, src.mode, src.depth);
    doc.resolution_dpi = src.resolution_dpi;
    doc.icc_profile = src.icc_profile.clone();
    doc.color_table = src.color_table.clone();
    doc.duotone = src.duotone.clone();

    // Embedded (not linked): the original file name keeps its extension, so Edit Contents opens it.
    let source = SmartSource::Embedded { file_name: file.to_string(), bytes };
    let so = SmartObject::new(source, Affine { m: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] }, Some(pixels));
    let layer = doc.insert_above(None, Layer::new(layer_name, LayerContent::Smart(so)));
    let (index, color) = admit_as_smart_object(s, doc);
    let id = s.documents().get(index).map(|st| st.doc.id.0);
    Ok(json!({"document": index, "documentId": id, "name": name, "layer": layer.0, "warnings": r.warnings, "color": color}))
}

/// Admits a document built by Open as Smart Object (one smart object layer, which becomes the
/// active layer) as an untitled document whose history is the single state "Open As Smart
/// Object", as File › Open's is one state. Returns its index and the Color Settings report.
fn admit_as_smart_object(s: &mut Session, doc: Document) -> (usize, Value) {
    // Untitled: no path, so Save asks where to save.
    let (index, color) = s.open_document_deferred(doc, None);
    if let Some(st) = s.active_mut() {
        st.history.set_current_label(LABEL);
    }
    (index, color)
}
