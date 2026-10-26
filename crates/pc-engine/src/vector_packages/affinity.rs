//! Affinity Open/Place: guarded, detached previews and one atomic publication.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::{
    Affine, ColorMode, Document, Layer, LayerContent, LayerId, SampleType,
    vector_design::exchange::{ImportLimits, ImportOptions, ImportedScene, VectorWarning},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Place {
    #[serde(skip)]
    admission: Admission,
    pub document: u64,
    pub revision: u64,
    pub above: Option<LayerId>,
    pub bytes: Vec<u8>,
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default)]
    pub offset: [i32; 2],
}
/// Begin captures a document/revision/anchor once, before decoding or preview work.
pub fn capture(session: &Session, bytes: Vec<u8>) -> Result<Place> {
    let s = session.active().ok_or(EngineError::NoDocument)?;
    let request = Place {
        admission: Admission::default(),
        document: s.doc.id.0,
        revision: s.revision,
        above: s.active_layer,
        bytes,
        name: default_name(),
        offset: [0; 2],
    };
    check(session, &request)?;
    Ok(request)
}
#[derive(Clone, Copy, Debug)]
struct Admission(ImportLimits);
impl Admission {
    fn for_ram(available: u64) -> Self {
        let bytes = (available / 4).min(8 << 30);
        // Decoded/model pixels overlap conversion buffers and prepared RGBA32F caches.
        Self(ImportLimits { decompressed_bytes: bytes, pixels: bytes / 48, ..Default::default() })
    }
}
impl Default for Admission {
    fn default() -> Self {
        Self::for_ram(lightcraft_sysmem::profile::ram_available())
    }
}

mod effects;

fn error(s: impl ToString) -> EngineError {
    EngineError::Other(s.to_string())
}
pub fn check(session: &Session, request: &Place) -> Result<()> {
    let s = session.active().ok_or(EngineError::NoDocument)?;
    if s.doc.id.0 != request.document || s.revision != request.revision {
        return Err(error("Affinity placement is stale; start again"));
    }
    if request.offset.iter().any(|v| v.unsigned_abs() > 1_000_000) {
        return Err(error("Affinity placement exceeds coordinate bounds"));
    }
    if request.bytes.len() > ImportOptions::default().limits.bytes {
        return Err(error("Affinity input byte budget"));
    }
    if request.name.len() > 4096 {
        return Err(error("Affinity name budget"));
    }
    if let Some(id) = request.above {
        let mut path = s.doc.path_of(id).ok_or(EngineError::NoLayer(id))?;
        path.pop();
        if !path.is_empty()
            && s.doc.layer_at(&path).is_some_and(|p| {
                let locks = s.doc.effective_locks(p.id);
                locks.all || locks.pixels
            })
        {
            return Err(error("Affinity destination group is locked"));
        }
    }
    Ok(())
}
pub fn decode(session: &Session, request: &Place) -> Result<ImportedScene> {
    decode_with(session, request, &std::sync::atomic::AtomicBool::new(false))
}
/// A host can perform detached preparation on its cancellable file worker.
pub fn decode_with(session: &Session, request: &Place, cancel: &std::sync::atomic::AtomicBool) -> Result<ImportedScene> {
    check(session, request)?;
    let s = session.active().ok_or(EngineError::NoDocument)?;
    let options = ImportOptions { dpi: s.doc.resolution_dpi, limits: request.admission.0, ..Default::default() };
    photocraft_affinity::import_with(&request.bytes, &options, None, cancel).map_err(reader_error)
}
fn reader_error(e: photocraft_affinity::Error) -> EngineError {
    if e == photocraft_affinity::Error::Cancelled { EngineError::Cancelled } else { error(e) }
}
/// A preview is a clone of the document with the already prepared scene inserted. The
/// parser and native caches run once at Begin, never on every pointer update.
pub fn preview(session: &Session, request: &Place, scene: &ImportedScene) -> Result<(Document, LayerId)> {
    check(session, request)?;
    let s = session.active().ok_or(EngineError::NoDocument)?;
    validate_scene(Some(&s.doc), scene, request.admission.0)?;
    let mut doc = (*s.doc).clone();
    let mut group = Layer::group(&request.name, scene.layers.clone());
    // A placed file keeps its own viewport when the destination is larger and
    // when a native shape cache is rebuilt after editing.
    let (w, h) = (f64::from(scene.size.width), f64::from(scene.size.height));
    let frame = photocraft_doc::Path::new(vec![photocraft_doc::Subpath::polygon(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)])]);
    group.vector.clip = Some(frame.clone());
    group.vector_mask = Some(photocraft_doc::VectorMask::new(frame));
    translate(&mut group, request.offset)?;
    let id = doc.insert_above(request.above, group);
    if doc.max_group_depth() > photocraft_doc::MAX_GROUP_DEPTH {
        return Err(error("Affinity placement exceeds document group depth"));
    }
    Ok((doc, id))
}
fn validate_scene(destination: Option<&Document>, scene: &ImportedScene, limits: ImportLimits) -> Result<()> {
    if scene.size.width == 0
        || scene.size.height == 0
        || scene.size.area() > limits.pixels
        || !scene.resolution_dpi.is_finite()
        || !(1.0..=100_000.).contains(&scene.resolution_dpi)
        || !scene.symbols.is_empty()
    {
        return Err(error("invalid Affinity scene"));
    }
    if scene.layers.len() > limits.objects {
        return Err(error("Affinity layer count budget"));
    }
    let mut effected = Vec::new();
    let mut stack: Vec<_> = scene.layers.iter().map(|l| (l, 1usize)).collect();
    let mut ids = std::collections::HashSet::new();
    let mut bytes = 0u64;
    while let Some((l, depth)) = stack.pop() {
        if depth >= photocraft_doc::MAX_GROUP_DEPTH
            || ids.len() >= limits.objects
            || !ids.insert(l.id)
            || destination.is_some_and(|doc| doc.layer(l.id).is_some())
        {
            return Err(error("invalid Affinity layer targets/depth"));
        }
        if !l.opacity.is_finite() || !(0.0..=1.0).contains(&l.opacity) {
            return Err(error("invalid Affinity opacity"));
        }
        bytes = bytes.saturating_add(effects::validate(&l.effects)?);
        if photocraft_compose::effects::has_effects(l) {
            effected.push(l);
        }
        if let Some(m) = &l.vector_mask {
            check_path(&m.path)?;
        }
        if let Some(p) = &l.vector.clip {
            check_path(p)?;
        }
        let cache = match &l.content {
            LayerContent::Shape(sh) => {
                check_path(&sh.path)?;
                if let Some(a) = &l.vector.appearance {
                    photocraft_vector::appearance::validate(a).map_err(error)?;
                } else {
                    photocraft_vector::appearance::validate(&photocraft_vector::appearance::legacy_appearance(sh)).map_err(error)?;
                }
                Some(sh.cache.as_ref().ok_or_else(|| error("Affinity shape cache missing"))?)
            }
            LayerContent::Text(t) => {
                if t.transform.m.iter().any(|v| !v.is_finite() || v.abs() > 1_000_000.) {
                    return Err(error("invalid Affinity text transform"));
                }
                Some(t.cache.as_ref().ok_or_else(|| error("Affinity text cache missing"))?)
            }
            LayerContent::Raster(surface) => Some(surface),
            LayerContent::Group(g) => {
                if g.children.len().saturating_add(stack.len()).saturating_add(ids.len()) > limits.objects {
                    return Err(error("Affinity layer count budget"));
                }
                stack.extend(g.children.iter().map(|c| (c, depth + 1)));
                None
            }
            _ => return Err(error("invalid Affinity layer content")),
        };
        for surface in cache.into_iter().chain(l.mask.iter().map(|m| &m.surface)) {
            bytes = bytes.saturating_add(surface.tiles().fold(0u64, |total, (_, tile)| total.saturating_add(tile.bytes().len() as u64)));
        }
        if bytes > limits.decompressed_bytes {
            return Err(error("Affinity cache budget"));
        }
    }
    // Bounds may recurse through children, so evaluate them only after validating the entire tree.
    for layer in effected {
        let bounds = photocraft_compose::effect_source_bounds(layer, photocraft_doc::Rect::from_size(scene.size));
        if bounds.is_empty() {
            continue;
        }
        if [bounds.x0, bounds.y0, bounds.x1, bounds.y1].iter().any(|v| v.unsigned_abs() > 1_000_000) {
            return Err(error("Affinity effect source coordinates exceed bounds"));
        }
        let bounds = bounds.inflate(photocraft_compose::effects::margin(layer));
        // Shape/distance maps, RGBA working buffers and temporary maps, plus retained maps per effect.
        let per_pixel = 96 + 16 * layer.effects.items.len() as u64;
        bytes = bytes.saturating_add(bounds.size().area().saturating_mul(per_pixel));
        if bytes > limits.decompressed_bytes {
            return Err(error("Affinity effect working budget"));
        }
    }
    Ok(())
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
                return Err(error("Affinity text placement exceeds coordinate bounds"));
            }
            text.cache = text.cache.as_ref().map(|s| s.translated(offset[0], offset[1]));
        }
        LayerContent::Raster(surface) => {
            *surface = surface.translated(offset[0], offset[1]);
        }
        LayerContent::Group(group) => {
            if let Some(a) = &mut group.artboard {
                a.rect = a.rect.translate(offset[0], offset[1]);
            }
            for child in &mut group.children {
                translate(child, offset)?;
            }
        }
        _ => return Err(error("unexpected content in Affinity scene")),
    }
    Ok(())
}
fn check_path(path: &photocraft_doc::Path) -> Result<()> {
    crate::vector_api::validate_path(path)?;
    if path.control_bounds().is_some_and(|b| [b.0, b.1, b.2, b.3].iter().any(|v| v.abs() > 1_000_000.0)) {
        return Err(error("Affinity placement exceeds coordinate bounds"));
    }
    Ok(())
}
/// Publish an already decoded detached scene once; UI release does not decode again.
pub fn commit(session: &mut Session, request: &Place, scene: &ImportedScene) -> Result<LayerId> {
    let (prepared, id) = preview(session, request, scene)?;
    check(session, request)?;
    session.edit("Place Affinity", move |doc, active| {
        *doc = prepared;
        *active = Some(id);
        Ok(())
    })?;
    Ok(id)
}
fn place(session: &mut Session, params: &Value) -> Result<Value> {
    let request: Place = serde_json::from_value(params.clone()).map_err(error)?;
    let scene = decode(session, &request)?;
    let id = commit(session, &request, &scene)?;
    Ok(json!({"layer":id.0,"warnings":scene.warnings}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Open {
    bytes: Vec<u8>,
    #[serde(default = "default_dpi")]
    dpi: f32,
    #[serde(default = "default_name")]
    name: String,
}
fn default_dpi() -> f32 {
    72.0
}
fn default_name() -> String {
    "Affinity".into()
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
/// Byte Open boundary used by file jobs/UI without expanding binary data into JSON arrays.
pub fn open_bytes(session: &mut Session, bytes: &[u8], dpi: f32, name: &str) -> Result<Vec<VectorWarning>> {
    if name.len() > 4096 {
        return Err(error("Affinity document name exceeds bounds"));
    }
    let scene = read_open(bytes, dpi, &std::sync::atomic::AtomicBool::new(false))?;
    let (doc, warnings) = document(scene, name);
    session.add_document(doc, None);
    Ok(warnings)
}
/// Worker-safe detached Open decode. No Session mutation, file read or resource resolution.
pub fn read_open(bytes: &[u8], dpi: f32, cancel: &std::sync::atomic::AtomicBool) -> Result<ImportedScene> {
    let options = ImportOptions { dpi, limits: Admission::default().0, ..Default::default() };
    let scene = match photocraft_affinity::import_with(bytes, &options, None, cancel) {
        Ok(scene) => scene,
        Err(photocraft_affinity::Error::Unsupported(reason)) => photocraft_affinity::import_preview(bytes, &options, cancel, reason).map_err(reader_error)?,
        Err(e) => return Err(reader_error(e)),
    };
    validate_scene(None, &scene, options.limits)?;
    Ok(scene)
}
fn open(session: &mut Session, params: &Value) -> Result<Value> {
    let request: Open = serde_json::from_value(params.clone()).map_err(error)?;
    let warnings = open_bytes(session, &request.bytes, request.dpi, &request.name)?;
    let id = session.active().ok_or(EngineError::NoDocument)?.doc.id.0;
    Ok(json!({"document":id,"warnings":warnings}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "file.affinity.import",
            label: "Open Affinity",
            menu: &[],
            shortcut: None,
            params: "{bytes:[u8],dpi?:number,name?:string}",
            enabled: crate::commands::always,
            run: open,
            journal: true,
        },
        CommandSpec {
            id: "file.affinity.place",
            label: "Place Affinity",
            menu: &[],
            shortcut: None,
            params: "{document:u64,revision:u64,above:id|null,bytes:[u8],name?:string,offset?:[i32,i32]}",
            enabled: crate::vector_cmds::has_doc,
            run: place,
            journal: true,
        },
    ]
}

#[cfg(test)]
#[path = "affinity/tests.rs"]
mod tests;
