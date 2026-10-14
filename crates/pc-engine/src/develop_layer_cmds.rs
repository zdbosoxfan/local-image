//! local-image: **Develop layers** — the bridge from the Library's Develop module to Compositing
//! without TIFF copies. A Develop layer is a smart object whose source is the original file
//! (linked) and whose [`DevelopLink`] holds the photo's develop settings: it renders through
//! LightCraft's pipeline (`photocraft_io::raw::develop_with_settings`), so a raw stays raw, and it
//! re-renders whenever the settings change (coming back from Develop, or `layer.develop.set`).
//! Retouching and compositing happen in layers above; the Develop layer itself is never
//! rasterised by them.
//!
//! * `develop.openPhoto {path, settings, photo?, name?}` — a new document whose Background is a
//!   Develop layer of the file (or the open document that already shows that photo).
//! * `layer.develop.set {settings, layer?}` — re-develops a Develop layer (or turns a smart object
//!   into one).
//! * `develop.syncPhoto {photo, settings}` — the new settings of that Library photo reach every
//!   open document following it (one undo step per document): a document that is still just the
//!   photo re-develops its layer; one with compositing work gets a new Develop layer on top
//!   ("Develop 2"…) and keeps every existing layer as it was.
//! * `develop.layers {}` — the Develop layers of the open documents (for the host).

use photocraft_doc::{DevelopLink, DocId, Document, Layer, LayerContent, LayerId, SmartObject, SmartSource};
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::jobs;
use crate::{EngineError, Result, Session};
use lightcraft_sysmem::JobCtl;
use std::sync::Arc;
#[path = "develop_layer_overwrite.rs"]
mod overwrite;
pub use overwrite::{PreparedSourceOverwrite, acknowledge_library_snapshot, freeze_source_snapshot};

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

/// The Develop layers of `doc`: (layer id, photo it follows).
pub fn develop_layers(doc: &Document) -> Vec<(LayerId, Option<u64>)> {
    doc.walk()
        .into_iter()
        .filter_map(|(_, _, l)| match &l.content {
            LayerContent::Smart(SmartObject { develop: Some(d), .. }) => Some((l.id, d.photo)),
            _ => None,
        })
        .collect()
}

/// The open document showing Library photo `photo` in a Develop layer.
pub fn document_of_photo(s: &Session, photo: u64) -> Option<usize> {
    document_of_photo_in(s, None, photo, None)
}

/// Qualified Library lookup; unbound legacy links never match a named catalog.
pub fn document_of_photo_in(s: &Session, library: Option<&str>, photo: u64, path: Option<&str>) -> Option<usize> {
    s.documents().iter().position(|d| {
        d.doc.walk().iter().any(|(_, _, layer)| match &layer.content {
            LayerContent::Smart(sm) => {
                sm.develop.as_ref().is_some_and(|link| link.photo == Some(photo) && link.library.as_deref() == library)
                    && path.is_none_or(|path| source_of(sm) == Some(path))
            }
            _ => false,
        })
    })
}

pub fn source_of(sm: &SmartObject) -> Option<&str> {
    // A PSD may represent its embedded source as a Linked UUID, so retain the
    // explicit Library source path rather than treating that UUID as a disk path.
    sm.develop.as_ref().and_then(|d| d.source.as_deref()).or(match &sm.source {
        SmartSource::Linked { path } => Some(path.as_str()),
        _ => None,
    })
}

/// Expensive source reads, rendering, transforms and smart filters all run in the worker.
fn render_layer(doc: &Document, mut layer: Layer, link: DevelopLink, ctx: &JobCtl) -> Result<Layer> {
    ctx.check()?;
    let LayerContent::Smart(sm) = &mut layer.content else { return Err(EngineError::Other("not a smart object".into())) };
    sm.develop = Some(link);
    ctx.report(0.05, "Developing");
    work_probe(ctx)?;
    if !crate::smart_cmds::refresh_layer(doc, &mut layer)? {
        return Err(EngineError::Other("the Develop source is unavailable; the old pixels were kept".into()));
    }
    ctx.check()?;
    Ok(layer)
}

pub const OPEN_JOB: &str = "develop.openPhoto";

fn photo_key(library: Option<&str>, photo: u64) -> String {
    format!("develop/photo/{}", json!([library, photo]))
}
fn layer_key(doc: DocId, layer: LayerId) -> String {
    format!("develop/layer/{}/{}", doc.0, layer.0)
}

fn open_photo(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = OPEN_JOB;
    let path = p.get("path").and_then(Value::as_str).filter(|v| !v.is_empty()).ok_or_else(|| bad(C, "missing `path`"))?.to_string();
    let settings = p.get("settings").cloned().unwrap_or_else(|| json!({}));
    let ask_color_profile = match p.get("askColorProfile") {
        None => false,
        Some(Value::Bool(ask)) => *ask,
        _ => return Err(bad(C, "askColorProfile must be a boolean")),
    };
    let photo = p.get("photo").and_then(Value::as_u64);
    let library = p.get("library").and_then(Value::as_str).map(str::to_owned);
    if library.is_some() && photo.is_none() {
        return Err(bad(C, "a qualified Library open needs a photo identity"));
    }
    let keys = vec![
        photo.map(|photo| format!("develop/open/{}", photo_key(library.as_deref(), photo))).unwrap_or_else(|| format!("develop/open/{path}")),
        format!("develop/open/source/{}", json!([library, photo, path, ask_color_profile])),
    ];
    // Returning to an existing source supersedes any pending open of the same
    // logical photo at another source, even though no new render is needed.
    if let Some(i) = photo.and_then(|id| document_of_photo_in(s, library.as_deref(), id, Some(&path))) {
        jobs::cancel_latest(s, &keys[..1]);
        s.set_active(i);
        return Ok(json!({ "document": i, "documentId": s.documents()[i].doc.id.0, "existing": true }));
    }
    if let Some(job) = jobs::latest(s, &keys, &settings) {
        return Ok(json!({"job": job.0, "pending": true}));
    }
    let name = p.get("name").and_then(Value::as_str).unwrap_or(&path).to_string();
    let gate = take_work_gate();
    jobs::run_latest(
        s,
        &format!("Opening {name}"),
        keys,
        move |ctx| {
            wait_work_gate(ctx, gate)?;
            let _work = enter_work(ctx)?;
            ctx.report(0.0, "Reading");
            let bytes = jobs::read_path(&path)?;
            ctx.check()?;
            let file_name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
            ctx.report(0.05, "Developing");
            work_probe(ctx)?;
            let r = photocraft_io::raw::develop_with_settings(&file_name, &bytes, &settings).map_err(|e| bad(C, e.to_string()))?;
            ctx.check()?;
            ctx.report(1.0, "Opening");
            Ok((r, path, settings, photo, library, name, ask_color_profile))
        },
        |s, (r, path, settings, photo, library, name, ask_color_profile)| {
            let mut doc = r.document;
            doc.name = std::path::Path::new(&name).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or(name.clone());
            let id = doc.layers.first().map(|l| l.id).ok_or(EngineError::Other("the developed photo has no layer".into()))?;
            let cache = doc.layer(id).and_then(|l| l.surface().cloned());
            crate::smart_cmds::approve_link(&path)?;
            let source = Some(path.clone().into());
            let mut sm = SmartObject::new(SmartSource::Linked { path }, photocraft_geom::Affine::IDENTITY, cache);
            sm.develop = Some(DevelopLink { library_baseline: None, source, library: library.map(Into::into), settings, photo });
            if let Some(l) = doc.layer_mut(id) {
                l.name = "Develop".into();
                l.content = LayerContent::Smart(sm);
            }
            // This is already a colour-managed internal render, not a foreign
            // file import. Retain its profile regardless of external-import policy.
            // An explicit caller can still ask the normal mismatch dialog to offer
            // Convert/Discard; those commands retain their visible failure path.
            let embedded = crate::color_cmds::document_profile(&doc);
            let working = s.color.working(doc.mode);
            let mismatch = !embedded.same_colors(&working);
            let color = json!({"action":"kept","policy":"preserve","embedded":embedded.description,"working":working.description,
                "mismatch":mismatch,"ask":ask_color_profile && mismatch,"internalDevelop":true});
            let i = s.add_document(doc, None);
            Ok(
                json!({ "document": i, "documentId": s.documents()[i].doc.id.0, "name": name, "existing": false, "layer": id.0, "warnings": r.warnings, "color": color }),
            )
        },
    )
}

fn set_settings(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "layer.develop.set";
    let settings = p.get("settings").cloned().ok_or_else(|| bad(C, "missing `settings`"))?;
    let d = s.active().ok_or(EngineError::NoDocument)?;
    let id = p.get("layer").and_then(Value::as_u64).map(LayerId).or(d.active_layer).ok_or_else(|| bad(C, "no active layer"))?;
    let layer = d.doc.layer(id).cloned().ok_or(EngineError::NoLayer(id))?;
    let LayerContent::Smart(sm) = &layer.content else { return Err(bad(C, "the layer isn't a Develop layer or smart object")) };
    let photo = p.get("photo").and_then(Value::as_u64).or(sm.develop.as_ref().and_then(|d| d.photo));
    let library =
        p.get("library").and_then(Value::as_str).map(str::to_owned).or_else(|| sm.develop.as_ref().and_then(|d| d.library.as_deref().map(str::to_owned)));
    let source = sm.develop.as_ref().and_then(|d| d.source.clone()).or_else(|| match &sm.source {
        SmartSource::Linked { path } => Some(path.clone().into()),
        _ => None,
    });
    let mut keys = vec![layer_key(d.doc.id, id)];
    if let Some(photo) = photo {
        keys.push(photo_key(library.as_deref(), photo));
    }
    if let Some(job) = jobs::latest(s, &keys, &settings) {
        return Ok(json!({"job": job.0, "pending": true}));
    }
    let base = d.doc.clone();
    let doc_id = base.id;
    jobs::cancel_latest(s, &keys);
    if sm.develop.as_ref().is_some_and(|link| link.settings == settings && link.photo == photo && link.library.as_deref() == library.as_deref()) {
        return Ok(json!({"layer": id.0}));
    }
    // A canvas adjustment edits the frozen source, not the Library's last acknowledged
    // state. Retain that baseline unless this command explicitly changes the binding.
    let library_baseline =
        sm.develop.as_ref().filter(|link| link.photo == photo && link.library.as_deref() == library.as_deref()).and_then(|link| link.library_baseline.clone());
    let gate = take_work_gate();
    jobs::run_latest(
        s,
        "Develop",
        keys,
        move |ctx| {
            wait_work_gate(ctx, gate)?;
            let _work = enter_work(ctx)?;
            let new = render_layer(&base, layer.clone(), DevelopLink { library_baseline, source, library: library.map(Into::into), settings, photo }, ctx)?;
            Ok((base, layer, new))
        },
        move |s, (base, old, new)| {
            let Some(i) = s.documents().iter().position(|d| d.doc.id == doc_id) else { return Err(EngineError::Cancelled) };
            if s.job_on(doc_id).is_some() {
                return Err(EngineError::Cancelled);
            }
            validate_layer(&s.documents()[i].doc, &base, &old)?;
            let active = s.active_index();
            s.set_active(i);
            let r = s.edit("Develop", |doc, _| {
                *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = new;
                Ok(())
            });
            restore_active(s, active);
            r?;
            Ok(json!({"layer": id.0}))
        },
    )
}

// A layer changed or was removed while the render ran: never overwrite that work. Unrelated
// layers may still change. Geometry/format changes invalidate the worker's placement.
fn validate_layer(doc: &Document, base: &Document, old: &Layer) -> Result<()> {
    if doc.size != base.size || doc.pixel_format() != base.pixel_format() || doc.layer(old.id) != Some(old) {
        return Err(EngineError::Cancelled);
    }
    Ok(())
}

fn restore_active(s: &mut Session, active: Option<usize>) {
    if let Some(i) = active {
        s.set_active(i);
    }
}

/// Is `doc` still just the photo: one layer, its Develop layer (no compositing work yet)?
pub fn is_untouched(doc: &Document) -> bool {
    doc.walk().len() == 1 && develop_layers(doc).len() == 1
}

/// The name of the next Develop layer: "Develop 2", "Develop 3"… (the first one is "Develop").
fn next_develop_name(doc: &Document) -> String {
    let names: Vec<&str> = doc.walk().into_iter().map(|(_, _, l)| l.name.as_str()).collect();
    (2..).map(|n| format!("Develop {n}")).find(|n| !names.contains(&n.as_str())).unwrap_or_else(|| "Develop".into())
}

/// Prepare a clean Develop layer from the template, without its filters or layer settings.
fn new_develop_layer(template: &Layer, link: DevelopLink) -> Layer {
    let LayerContent::Smart(sm) = &template.content else { return template.clone() };
    let mut sm = sm.clone();
    sm.smart_filters.clear();
    sm.filter_mask = None;
    sm.filters_enabled = true;
    sm.stack_mode = None;
    sm.psd_raw = None;
    sm.develop = Some(link);
    Layer::new("Develop", LayerContent::Smart(sm))
}

/// Render all documents following a Library photo off-thread, then apply one undo step in each.
fn sync_photo(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "develop.syncPhoto";
    let photo = p.get("photo").and_then(Value::as_u64).ok_or_else(|| bad(C, "missing `photo`"))?;
    let settings = p.get("settings").cloned().ok_or_else(|| bad(C, "missing `settings`"))?;
    let library = p.get("library").and_then(Value::as_str).map(str::to_owned);
    let path = p.get("path").and_then(Value::as_str).map(str::to_owned);
    if library.is_some() && path.is_none() {
        return Err(bad(C, "a qualified Library sync needs its source path"));
    }
    let keys = vec![photo_key(library.as_deref(), photo), format!("develop/sync/source/{}", json!([library, photo, path]))];
    if let Some(job) = jobs::latest(s, &keys, &settings) {
        return Ok(json!({"job": job.0, "pending": true}));
    }
    jobs::cancel_latest(s, &keys);
    let targets: Vec<_> = s
        .documents()
        .iter()
        .filter_map(|d| {
            let layer = d
                .doc
                .walk()
                .into_iter()
                .filter_map(|(_, _, l)| match &l.content {
                    LayerContent::Smart(sm)
                        if sm.develop.as_ref().is_some_and(|link| link.photo == Some(photo) && link.library.as_deref() == library.as_deref())
                            && path.as_deref().is_none_or(|path| source_of(sm) == Some(path)) =>
                    {
                        sm.develop.as_ref().map(|link| (l, link))
                    }
                    _ => None,
                })
                .next_back()?;
            (layer.1.library_baseline.as_deref().unwrap_or(&layer.1.settings) != &settings).then(|| (d.doc.clone(), layer.0.clone()))
        })
        .collect();
    if targets.is_empty() {
        return Ok(json!({"updated": 0, "added": []}));
    }
    let gate = take_work_gate();
    jobs::run_latest(
        s,
        "Update from Develop",
        keys,
        move |ctx| {
            wait_work_gate(ctx, gate)?;
            let _work = enter_work(ctx)?;
            let mut rendered = Vec::new();
            for (base, old) in targets {
                let link = DevelopLink {
                    library_baseline: None,
                    source: path.clone().map(Into::into).or_else(|| match &old.content {
                        LayerContent::Smart(sm) => sm.develop.as_ref().and_then(|link| link.source.clone()),
                        _ => None,
                    }),
                    library: library.clone().map(Into::into),
                    settings: settings.clone(),
                    photo: Some(photo),
                };
                // After an overwrite the old layer renders frozen bytes. A real
                // new Library edit must instead read today's linked source.
                let mut template = old.clone();
                if let LayerContent::Smart(smart) = &mut template.content
                    && smart.develop.as_ref().is_some_and(|link| link.library_baseline.is_some())
                    && let Some(source) = link.source.as_ref()
                {
                    crate::smart_cmds::approve_link(source)?;
                    smart.source = SmartSource::Linked { path: source.to_string() };
                    smart.psd_raw = None;
                }
                // Render both paths once: in-place keeps the template's smart filters; new layers
                // start clean. Decide again at apply time, since compositing may start during work.
                let clean = render_layer(&base, new_develop_layer(&template, link.clone()), link.clone(), ctx)?;
                let in_place = if is_untouched(&base) { Some(render_layer(&base, template, link, ctx)?) } else { None };
                rendered.push((base, old, clean, in_place));
            }
            ctx.report(1.0, "Applying Develop edits");
            Ok(rendered)
        },
        |s, rendered| {
            let active = s.active_index();
            let mut updated = 0;
            let mut added = Vec::new();
            for (base, old, mut clean, in_place) in rendered {
                let Some(i) = s.documents().iter().position(|d| d.doc.id == base.id) else { continue };
                if s.job_on(base.id).is_some() || validate_layer(&s.documents()[i].doc, &base, &old).is_err() {
                    continue;
                }
                let untouched = is_untouched(&s.documents()[i].doc);
                s.set_active(i);
                let result = if let Some(new) = in_place.filter(|_| untouched) {
                    s.edit("Update from Develop", |doc, _| {
                        *doc.layer_mut(old.id).ok_or(EngineError::NoLayer(old.id))? = new;
                        Ok(())
                    })
                    .map(|_| updated += 1)
                } else {
                    s.edit("Add Develop Layer", |doc, active| {
                        clean.name = next_develop_name(doc);
                        *active = Some(clean.id);
                        added.push(json!({"document": i, "layer": clean.id.0, "name": clean.name}));
                        doc.layers.push(clean);
                        Ok(())
                    })
                };
                restore_active(s, active);
                result?;
            }
            Ok(json!({"updated": updated, "added": added}))
        },
    )
}

fn list(s: &mut Session, _: &Value) -> Result<Value> {
    let docs: Vec<Value> = s
        .documents()
        .iter()
        .enumerate()
        .map(|(i, d)| json!({ "document": i, "layers": develop_layers(&d.doc).into_iter().map(|(id, photo)| json!({"layer": id.0, "photo": photo})).collect::<Vec<_>>() }))
        .collect();
    Ok(json!({ "documents": docs }))
}

fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "develop.openPhoto",
            label: "Open Photo as Develop Layer",
            menu: &[],
            shortcut: None,
            params: r##"{"path":str,"settings":DevelopSettings JSON,"photo":id?,"name":str?,"askColorProfile":bool=false} → {document, existing, layer?}"##,
            enabled: always,
            run: open_photo,
            journal: true,
        },
        CommandSpec {
            id: "layer.develop.set",
            label: "Develop Layer Settings",
            menu: &[],
            shortcut: None,
            params: r##"{"settings":DevelopSettings JSON,"layer":id?,"photo":id?}"##,
            enabled: has_doc,
            run: set_settings,
            journal: true,
        },
        CommandSpec {
            id: "develop.syncPhoto",
            label: "Update Develop Layers",
            menu: &[],
            shortcut: None,
            params: r##"{"photo":id,"settings":DevelopSettings JSON} → {updated, added: [{document, layer, name}]}"##,
            enabled: always,
            run: sync_photo,
            journal: true,
        },
        CommandSpec { id: "develop.layers", label: "Develop Layers", menu: &[], shortcut: None, params: "{}", enabled: always, run: list, journal: false },
    ]
}

/// The decoded-and-developed source of a Develop layer, cached by file and settings.
pub(crate) fn developed_doc(name: &str, bytes: &[u8], settings: &Value) -> Result<Document> {
    photocraft_io::raw::develop_with_settings(name, bytes, settings).map(|r| r.document).map_err(|e| EngineError::Other(e.to_string()))
}

thread_local! {
    static WORK_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static WORK_GATE: std::cell::RefCell<Option<Arc<std::sync::atomic::AtomicBool>>> = const { std::cell::RefCell::new(None) };
}

/// Headless-test probe: expensive Develop work performed on the calling thread.
#[doc(hidden)]
pub fn work_count_on_thread() -> usize {
    WORK_COUNT.with(std::cell::Cell::get)
}

/// Headless-test hook: the next new Develop request waits on this gate in its worker.
/// Install only on the caller thread; requests in other tests are unaffected.
#[doc(hidden)]
pub fn gate_next_work(gate: Arc<std::sync::atomic::AtomicBool>) {
    WORK_GATE.with(|g| *g.borrow_mut() = Some(gate));
}

fn take_work_gate() -> Option<Arc<std::sync::atomic::AtomicBool>> {
    WORK_GATE.with(|g| g.borrow_mut().take())
}
fn wait_work_gate(ctx: &JobCtl, gate: Option<Arc<std::sync::atomic::AtomicBool>>) -> Result<()> {
    if let Some(gate) = gate {
        while !gate.load(std::sync::atomic::Ordering::Relaxed) {
            ctx.check()?;
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    Ok(ctx.check()?)
}
// Full-size sources and spatial stages are large. Superseded requests waiting for this slot
// stop before they decode/render, rather than accumulating simultaneous full-size workers.
static DEVELOP_WORK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn enter_work(ctx: &JobCtl) -> Result<std::sync::MutexGuard<'static, ()>> {
    loop {
        ctx.check()?;
        match DEVELOP_WORK.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(e)) => return Ok(e.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(std::time::Duration::from_millis(1)),
        }
    }
}

fn work_probe(ctx: &JobCtl) -> Result<()> {
    WORK_COUNT.with(|c| c.set(c.get() + 1));
    Ok(ctx.check()?)
}

#[cfg(test)]
#[path = "develop_layer_jobs_tests.rs"]
mod tests;
