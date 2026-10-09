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

/// Is `doc` still just the photo: one layer, its Develop layer (no compositing work yet)?
pub fn is_untouched(doc: &Document) -> bool {
    doc.walk().len() == 1 && develop_layers(doc).len() == 1
}

/// The name of the next Develop layer: "Develop 2", "Develop 3"… (the first one is "Develop").
fn next_develop_name(doc: &Document) -> String {
    let names: Vec<&str> = doc.walk().into_iter().map(|(_, _, l)| l.name.as_str()).collect();
    (2..).map(|n| format!("Develop {n}")).find(|n| !names.contains(&n.as_str())).unwrap_or_else(|| "Develop".into())
}

/// A new Develop layer on top of `doc` following `link`: the source, placement and geometry of
/// Develop layer `template`, without its smart filters, mask or layer settings.
fn add_develop_layer(doc: &mut Document, template: LayerId, link: DevelopLink) -> Result<Layer> {
    let Some(LayerContent::Smart(sm)) = doc.layer(template).map(|l| &l.content) else {
        return Err(EngineError::Other("not a Develop layer".into()));
    };
    let mut sm = sm.clone();
    sm.smart_filters.clear();
    sm.filter_mask = None;
    sm.filters_enabled = true;
    sm.stack_mode = None;
    sm.psd_raw = None;
    sm.develop = Some(link);
    let mut l = Layer::new(next_develop_name(doc), LayerContent::Smart(sm));
    crate::smart_cmds::refresh_layer(doc, &mut l)?;
    doc.layers.push(l.clone());
    Ok(l)
}

/// New develop settings for Library photo `photo`, in every open document that follows it:
/// a document that is still just the photo re-develops its layer in place; one with compositing
/// work on it keeps every layer as it is and gets the new settings as a new Develop layer on top
/// ("Develop 2", "Develop 3"…), so the work above and the earlier develop stay untouched. A
/// document whose newest Develop layer of the photo already has the settings is left alone.
fn sync_photo(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "develop.syncPhoto";
    let photo = p.get("photo").and_then(Value::as_u64).ok_or_else(|| bad(C, "missing `photo`"))?;
    let settings = p.get("settings").cloned().ok_or_else(|| bad(C, "missing `settings`"))?;
    let active = s.active_index();
    let mut updated = 0;
    let mut added = Vec::new();
    for i in 0..s.documents().len() {
        let doc = &s.documents()[i].doc;
        // bottom to top: the last is the newest
        let mine: Vec<(LayerId, bool)> = doc
            .walk()
            .into_iter()
            .filter_map(|(_, _, l)| match &l.content {
                LayerContent::Smart(SmartObject { develop: Some(d), .. }) if d.photo == Some(photo) => Some((l.id, d.settings == settings)),
                _ => None,
            })
            .collect();
        let Some(&(newest, current)) = mine.last() else { continue };
        if current {
            continue;
        }
        let in_place = is_untouched(doc);
        s.set_active(i);
        let link = DevelopLink { settings: settings.clone(), photo: Some(photo) };
        if in_place {
            s.edit("Update from Develop", |doc, _| redevelop(doc, newest, link))?;
            updated += 1;
        } else {
            let l = s.edit("Add Develop Layer", |doc, active| {
                let l = add_develop_layer(doc, newest, link)?;
                *active = Some(l.id);
                Ok(l)
            })?;
            added.push(json!({ "document": i, "layer": l.id.0, "name": l.name }));
        }
    }
    if let Some(a) = active {
        s.set_active(a);
    }
    Ok(json!({ "updated": updated, "added": added }))
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
