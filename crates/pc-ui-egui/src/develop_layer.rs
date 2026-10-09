//! local-image: Develop layers in Compositing (see `photocraft_engine::develop_layer_cmds`): a
//! photo from the Library's Develop module, shown as the original file plus its develop settings,
//! re-rendered through the same pipeline — no TIFF copies. The host (the app's Library | Develop |
//! Compositing switch) opens photos with [`open_photo`], keeps the layers following the Library
//! with [`sync`], and goes to Develop when a Develop layer is double-clicked ([`DEVELOP_ID`]).
//!
//! The round trip back, as Photoshop's Edit In from Lightroom: a document that came from a
//! Library photo saves beside the original as `<name>-Edit.psd` ([`library_save_path`]; ⌘S
//! writes there directly, Save As suggests it), and every save is reported to the host
//! ([`LibrarySave`] in `PhotocraftApp::library_saves`), which adds the file to the Library
//! stacked on top of the original — or reloads it when the Library already has it. File › Save
//! and Return to Library ([`SAVE_RETURN_ID`]) also shows the Library on the saved photo.

use photocraft_doc::{Layer, LayerContent, SmartObject, SmartSource};
use serde_json::{Value, json};

use crate::PhotocraftApp;

/// Layer › Smart Objects › Develop… (and double-clicking a Develop layer's thumbnail).
pub const DEVELOP_ID: &str = "li.developLayer";

/// File › Save and Return to Library.
pub const SAVE_RETURN_ID: &str = "file.saveAndReturnToLibrary";

/// A file Save / Save As wrote (local-image: the host takes these for the Library).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibrarySave {
    /// The file written.
    pub path: String,
    /// The Library photo the document came from (its Develop layer's), if any: a file the Library
    /// doesn't have yet is added stacked on top of it.
    pub photo: Option<u64>,
    /// File › Save and Return to Library: show the Library on the saved photo afterwards.
    pub show: bool,
}

/// The Library photo a layer follows, when it is a Develop layer.
pub fn layer_photo(l: &Layer) -> Option<u64> {
    match &l.content {
        LayerContent::Smart(SmartObject { develop: Some(d), .. }) => d.photo,
        _ => None,
    }
}

pub fn is_develop_layer(l: &Layer) -> bool {
    matches!(&l.content, LayerContent::Smart(SmartObject { develop: Some(_), .. }))
}

/// Every Library photo followed by a Develop layer in an open document.
pub fn linked_photos(app: &PhotocraftApp) -> Vec<u64> {
    let mut v: Vec<u64> =
        app.session.documents().iter().flat_map(|d| photocraft_engine::develop_layer_cmds::develop_layers(&d.doc).into_iter().filter_map(|(_, p)| p)).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// The photo the active document's Develop layer follows: the active layer's, else the document's
/// first Develop layer's.
pub fn active_photo(app: &PhotocraftApp) -> Option<u64> {
    let st = app.session.active()?;
    if let Some(p) = st.active_layer.and_then(|id| st.doc.layer(id)).and_then(layer_photo) {
        return Some(p);
    }
    photocraft_engine::develop_layer_cmds::develop_layers(&st.doc).into_iter().find_map(|(_, p)| p)
}

/// The Library photo the active document came from and the path of its original file: the photo
/// [`active_photo`] names, when one of the document's Develop layers links it to a file.
pub fn library_original(app: &PhotocraftApp) -> Option<(u64, String)> {
    let photo = active_photo(app)?;
    app.session.active()?.doc.walk().into_iter().find_map(|(_, _, l)| match &l.content {
        LayerContent::Smart(SmartObject { develop: Some(d), source: SmartSource::Linked { path }, .. }) if d.photo == Some(photo) && !path.is_empty() => {
            Some((photo, path.clone()))
        }
        _ => None,
    })
}

/// `<folder of original>/<name>-Edit.psd`, or `-Edit-2.psd`, `-Edit-3.psd`… when that file
/// exists: Lightroom's naming for an edit in Photoshop, never an existing file.
pub fn edit_path(original: &str) -> String {
    edit_path_with(original, |p| std::path::Path::new(p).exists())
}

/// [`edit_path`] with `taken` telling which paths exist.
pub fn edit_path_with(original: &str, taken: impl Fn(&str) -> bool) -> String {
    let p = std::path::Path::new(original);
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Untitled".into());
    let dir = p.parent().unwrap_or(std::path::Path::new(""));
    let at = |name: String| dir.join(name).to_string_lossy().to_string();
    let mut path = at(format!("{stem}-Edit.psd"));
    let mut n = 2;
    while taken(&path) {
        path = at(format!("{stem}-Edit-{n}.psd"));
        n += 1;
    }
    path
}

/// Where File › Save writes the active document without asking: beside the Library photo it came
/// from, as `<name>-Edit.psd` (Photoshop saving back to Lightroom). `None` unless the document is
/// unsaved, came from a Library photo, and the Library is there (`host_modes`).
pub fn library_save_path(app: &PhotocraftApp) -> Option<String> {
    let st = app.session.active()?;
    // Edit Contents documents save back into their smart object.
    if !app.host_modes || st.path.is_some() || app.session.is_enabled("layer.smartObjects.saveContents") {
        return None;
    }
    library_original(app).map(|(_, original)| edit_path(&original))
}

/// File › Save and Return to Library: saves as File › Save does (beside the original, for a photo
/// from the Library), then the host shows the Library on the saved photo, stacked with the
/// original. `{"path"}` saves there instead.
pub fn save_and_return(app: &mut PhotocraftApp, params: &Value) -> Result<Value, String> {
    if !app.host_modes {
        return Err(tl!("Open the Library to save into it").into());
    }
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| app.session.active().and_then(|d| d.path.clone()).filter(|p| photocraft_engine::file_cmds::saves_in_place(p)))
        .or_else(|| library_save_path(app));
    let before = app.library_saves.len();
    let (path, warnings) = app.save_as(path)?;
    match app.library_saves.get_mut(before..).and_then(<[LibrarySave]>::last_mut) {
        Some(s) => s.show = true,
        // A save that waits on a prompt (a layered TIFF's options) stays in Compositing.
        None if app.session.active().is_some_and(|d| d.saved_revision == d.revision) => app.switch_module = Some(crate::Module::Library),
        None => {}
    }
    Ok(json!({ "path": path, "warnings": warnings }))
}

/// Develop layers following `photo` take `settings` (one undo step per document that changed).
pub fn sync(app: &mut PhotocraftApp, photo: u64, settings: &Value) {
    match app.run("develop.syncPhoto", json!({ "photo": photo, "settings": settings })) {
        // A document with compositing work keeps it: the new develop is a new layer on top.
        Ok(r) if r["added"].as_array().is_some_and(|a| !a.is_empty()) => {
            for a in r["added"].as_array().into_iter().flatten() {
                let name = a["name"].as_str().unwrap_or("Develop");
                let msg = crate::i18n::fmt(tl!("Added your new Develop edits as layer {name}"), &[("name", name)]);
                crate::notices::post(
                    app,
                    msg.clone(),
                    vec![tl!("The layers below are unchanged; hide or delete the new layer to go back.").into()],
                    false,
                    None,
                );
                app.ui.status = msg;
            }
            app.ui.status_error = false;
            app.sync_views();
        }
        Ok(r) if r["updated"].as_u64().unwrap_or(0) > 0 => {
            app.ui.status = tl!("Updated from Develop").into();
            app.ui.status_error = false;
            app.sync_views();
        }
        Ok(_) => {}
        Err(e) => {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
}

/// Shows Library photo `photo` (the file at `path`, developed with `settings`) in Compositing: the
/// document that already has it, or a new one whose Background is its Develop layer.
pub fn open_photo(app: &mut PhotocraftApp, photo: u64, path: &str, settings: &Value, name: &str) -> Result<(), String> {
    let r = app.run("develop.openPhoto", json!({ "path": path, "photo": photo, "settings": settings, "name": name }))?;
    app.sync_views();
    if !r["existing"].as_bool().unwrap_or(false) {
        app.ui.status = crate::i18n::fmt(tl!("{name} · Develop layer (double-click it to return to Develop)"), &[("name", name)]);
        app.ui.status_error = false;
    }
    Ok(())
}

/// Double-click / Layer › Smart Objects › Develop…: the host switches to Develop on the layer's
/// photo. Without a Library (no host), says how to change its settings instead.
pub fn develop(app: &mut PhotocraftApp) -> Result<Value, String> {
    let Some(photo) = active_photo(app) else { return Err(tl!("The active layer isn't a Develop layer").into()) };
    if !app.host_modes {
        return Err(tl!("Open the Library to develop this layer").into());
    }
    app.develop_request = Some(photo);
    Ok(json!({ "photo": photo }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A photo opens as a Develop layer (no copy of its pixels on disk), follows new settings,
    /// opens once, and a double-click asks the host for Develop.
    #[test]
    fn open_follow_and_return_to_develop() {
        let dir = std::env::temp_dir().join(format!("li-develop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("photo.png");
        image::RgbImage::from_fn(40, 30, |x, _| image::Rgb([(x * 6) as u8, 120, 60])).save(&path).unwrap();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.host_modes = true;
        let p = path.display().to_string();
        open_photo(&mut app, 7, &p, &json!({}), "photo.png").unwrap();
        assert_eq!(app.session.documents().len(), 1);
        let st = app.session.active().unwrap();
        assert_eq!((st.doc.size.width, st.doc.size.height), (40, 30));
        let l = &st.doc.layers[0];
        assert!(is_develop_layer(l) && layer_photo(l) == Some(7));
        assert_eq!(linked_photos(&app), vec![7]);
        let before = st.doc.layers[0].content.clone();
        // Brighter in Develop: the layer re-renders.
        sync(&mut app, 7, &json!({ "light": { "exposure": 1.5 } }));
        let after = app.session.active().unwrap().doc.layers[0].content.clone();
        assert_ne!(before, after, "the Develop layer re-rendered");
        // Same settings again: nothing to do (no extra undo step).
        let undo = app.session.active().unwrap().history.undo_label().map(str::to_string);
        sync(&mut app, 7, &json!({ "light": { "exposure": 1.5 } }));
        assert_eq!(app.session.active().unwrap().history.undo_label().map(str::to_string), undo);
        // Opening the photo again shows the same document.
        open_photo(&mut app, 7, &p, &json!({ "light": { "exposure": 1.5 } }), "photo.png").unwrap();
        assert_eq!(app.session.documents().len(), 1);
        // Double-click: the host is asked for Develop on photo 7.
        assert!(develop(&mut app).is_ok());
        assert_eq!(app.develop_request, Some(7));
        std::fs::remove_dir_all(dir).ok();
    }

    fn photo_app(tag: &str) -> (std::path::PathBuf, String, PhotocraftApp) {
        let dir = std::env::temp_dir().join(format!("li-develop-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("photo.png");
        image::RgbImage::from_fn(40, 30, |x, _| image::Rgb([(x * 6) as u8, 120, 60])).save(&path).unwrap();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.host_modes = true;
        let p = path.display().to_string();
        open_photo(&mut app, 7, &p, &json!({}), "photo.png").unwrap();
        (dir, p, app)
    }

    /// Back from Develop with new edits to a photo that has compositing work on it: the new
    /// develop is a new layer on top ("Develop 2", then "Develop 3"); every existing layer,
    /// the first Develop layer included, stays as it was.
    #[test]
    fn new_develop_edits_become_a_new_layer_over_compositing_work() {
        let (dir, _, mut app) = photo_app("stack");
        app.run("layer.new.layer", json!({})).unwrap();
        let before: Vec<Layer> = app.session.active().unwrap().doc.layers.clone();
        assert_eq!(before.len(), 2);
        let undo = app.session.active().unwrap().history.undo_label().map(str::to_string);

        sync(&mut app, 7, &json!({ "light": { "exposure": 1.0 } }));
        let st = app.session.active().unwrap();
        assert_eq!(st.doc.layers.len(), 3);
        assert_eq!(&st.doc.layers[..2], &before[..], "the layers below are untouched");
        let top = &st.doc.layers[2];
        assert_eq!(top.name, "Develop 2");
        assert!(is_develop_layer(top) && layer_photo(top) == Some(7));
        assert_ne!(top.content, before[0].content, "developed with the new settings");
        assert_eq!(st.active_layer, Some(top.id));
        assert_eq!(app.ui.status, "Added your new Develop edits as layer Develop 2");
        assert!(app.ui.notices.iter().any(|n| n.title == app.ui.status));
        // one undo step takes it away again
        assert_ne!(st.history.undo_label().map(str::to_string), undo);

        // the same settings again (coming back without changes): nothing new
        sync(&mut app, 7, &json!({ "light": { "exposure": 1.0 } }));
        assert_eq!(app.session.active().unwrap().doc.layers.len(), 3);
        // another round trip: Develop 3, still over everything
        sync(&mut app, 7, &json!({ "light": { "exposure": -1.0 } }));
        let st = app.session.active().unwrap();
        let names: Vec<&str> = st.doc.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names[2..], ["Develop 2", "Develop 3"]);
        assert_eq!(&st.doc.layers[..2], &before[..]);
        std::fs::remove_dir_all(dir).ok();
    }

    /// An unsaved photo from the Library saves beside its original as `<name>-Edit.psd` (never
    /// over an existing file), and Save As suggests that path.
    #[test]
    fn a_photo_from_the_library_saves_beside_its_original() {
        let (dir, p, mut app) = photo_app("save");
        assert_eq!(library_original(&app), Some((7, p.clone())));
        let edit = dir.join("photo-Edit.psd").display().to_string();
        assert_eq!(library_save_path(&app).as_deref(), Some(edit.as_str()));
        std::fs::write(&edit, b"taken").unwrap();
        let edit2 = dir.join("photo-Edit-2.psd").display().to_string();
        assert_eq!(library_save_path(&app).as_deref(), Some(edit2.as_str()), "an existing edit is kept");
        assert_eq!(edit_path_with("/pics/IMG_1.CR3", |p| p.ends_with("IMG_1-Edit.psd") || p.ends_with("IMG_1-Edit-2.psd")), "/pics/IMG_1-Edit-3.psd");
        // Save As suggests the same place (the dialog opens there)
        let suggested = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        let s = suggested.clone();
        app.services.pick_save = Some(Box::new(move |sug: &str| {
            *s.borrow_mut() = sug.to_string();
            None
        }));
        assert_eq!(app.save_as(None).unwrap_err(), "cancelled");
        assert_eq!(*suggested.borrow(), edit2);
        // without the Library (no host), File › Save asks as usual
        app.host_modes = false;
        assert_eq!(library_save_path(&app), None);
        std::fs::remove_dir_all(dir).ok();
    }
}
