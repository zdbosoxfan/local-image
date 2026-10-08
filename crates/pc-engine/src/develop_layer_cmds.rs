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
//! * `develop.syncPhoto {photo, settings}` — every Develop layer following that Library photo, in
//!   every open document, takes the new settings (one undo step per document).
//! * `develop.layers {}` — the Develop layers of the open documents (for the host).

use photocraft_doc::{DevelopLink, Document, Layer, LayerContent, LayerId, SmartObject, SmartSource};
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

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
    s.documents().iter().position(|d| develop_layers(&d.doc).iter().any(|(_, p)| *p == Some(photo)))
}

/// Re-renders layer `id` with `link` (keeping the old pixels when the source can't be read).
fn redevelop(doc: &mut Document, id: LayerId, link: DevelopLink) -> Result<()> {
    let mut l: Layer = doc.layer(id).cloned().ok_or(EngineError::Other("no such layer".into()))?;
    let LayerContent::Smart(sm) = &mut l.content else { return Err(EngineError::Other("not a smart object".into())) };
    sm.develop = Some(link);
    crate::smart_cmds::refresh_layer(doc, &mut l)?;
    if let Some(slot) = doc.layer_mut(id) {
        *slot = l;
    }
    Ok(())
}

fn open_photo(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "develop.openPhoto";
    let path = p.get("path").and_then(Value::as_str).filter(|v| !v.is_empty()).ok_or_else(|| bad(C, "missing `path`"))?.to_string();
    let settings = p.get("settings").cloned().unwrap_or_else(|| json!({}));
    let photo = p.get("photo").and_then(Value::as_u64);
    // Already open: show it (with the current settings).
    if let Some(i) = photo.and_then(|id| document_of_photo(s, id)) {
        s.set_active(i);
        if let Some(id) = photo {
            sync_photo(s, &json!({ "photo": id, "settings": settings }))?;
        }
        return Ok(json!({ "document": i, "existing": true }));
    }
    let bytes = std::fs::read(&path).map_err(|e| bad(C, format!("{path}: {e}")))?;
    let file_name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
    let r = photocraft_io::raw::develop_with_settings(&file_name, &bytes, &settings).map_err(|e| bad(C, e.to_string()))?;
    let mut doc = r.document;
    let stem = std::path::Path::new(&file_name).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or(file_name.clone());
    doc.name = p
        .get("name")
        .and_then(Value::as_str)
        .map(|n| std::path::Path::new(n).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(n.to_string()))
        .unwrap_or(stem);
    // The developed pixels become the Develop layer's cache; its source is the original file.
    let id = doc.layers.first().map(|l| l.id).ok_or(EngineError::Other("the developed photo has no layer".into()))?;
    let cache = doc.layer(id).and_then(|l| l.surface().cloned());
    let mut sm = SmartObject::new(SmartSource::Linked { path: path.clone() }, photocraft_geom::Affine::IDENTITY, cache);
    sm.develop = Some(DevelopLink { settings, photo });
    if let Some(l) = doc.layer_mut(id) {
        l.name = "Develop".into();
        l.content = LayerContent::Smart(sm);
    }
    let (i, _) = s.open_document(doc, None);
    Ok(json!({ "document": i, "existing": false, "layer": id.0, "warnings": r.warnings }))
}

fn set_settings(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "layer.develop.set";
    let settings = p.get("settings").cloned().ok_or_else(|| bad(C, "missing `settings`"))?;
    let d = s.active().ok_or(EngineError::NoDocument)?;
    let id = match p.get("layer").and_then(Value::as_u64) {
        Some(v) => LayerId(v),
        None => d.active_layer.ok_or_else(|| bad(C, "no active layer"))?,
    };
    let Some(LayerContent::Smart(sm)) = d.doc.layer(id).map(|l| &l.content) else { return Err(bad(C, "the layer isn't a Develop layer or smart object")) };
    let photo = p.get("photo").and_then(Value::as_u64).or(sm.develop.as_ref().and_then(|d| d.photo));
    s.edit("Develop", |doc, _| redevelop(doc, id, DevelopLink { settings, photo }))?;
    Ok(json!({ "layer": id.0 }))
}

fn sync_photo(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "develop.syncPhoto";
    let photo = p.get("photo").and_then(Value::as_u64).ok_or_else(|| bad(C, "missing `photo`"))?;
    let settings = p.get("settings").cloned().ok_or_else(|| bad(C, "missing `settings`"))?;
    let active = s.active_index();
    let mut updated = 0;
    for i in 0..s.documents().len() {
        let stale: Vec<LayerId> = s.documents()[i]
            .doc
            .walk()
            .into_iter()
            .filter_map(|(_, _, l)| match &l.content {
                LayerContent::Smart(SmartObject { develop: Some(d), .. }) if d.photo == Some(photo) && d.settings != settings => Some(l.id),
                _ => None,
            })
            .collect();
        if stale.is_empty() {
            continue;
        }
        s.set_active(i);
        s.edit("Update from Develop", |doc, _| {
            for id in &stale {
                redevelop(doc, *id, DevelopLink { settings: settings.clone(), photo: Some(photo) })?;
            }
            Ok(())
        })?;
        updated += stale.len();
    }
    if let Some(a) = active {
        s.set_active(a);
    }
    Ok(json!({ "updated": updated }))
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
            params: r##"{"path":str,"settings":DevelopSettings JSON,"photo":id?,"name":str?} → {document, existing, layer?}"##,
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
            params: r##"{"photo":id,"settings":DevelopSettings JSON} → {updated}"##,
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
