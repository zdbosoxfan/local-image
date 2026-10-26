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

#[cfg(test)]
use photocraft_doc::SmartSource;
use photocraft_doc::{Layer, LayerContent, SmartObject};
use serde_json::{Value, json};

use crate::PhotocraftApp;

/// Layer › Smart Objects › Develop… (and double-clicking a Develop layer's thumbnail).
pub const DEVELOP_ID: &str = "li.developLayer";

/// File › Save and Return to Library.
pub const SAVE_RETURN_ID: &str = "file.saveAndReturnToLibrary";

/// Captured identity of a Library photo, independent of the currently active catalog/tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryOrigin {
    pub library: Option<String>,
    pub photo: u64,
    pub path: String,
}

pub fn layer_origin(layer: &Layer) -> Option<LibraryOrigin> {
    let LayerContent::Smart(sm) = &layer.content else { return None };
    let link = sm.develop.as_ref()?;
    Some(LibraryOrigin {
        library: link.library.as_deref().map(str::to_owned),
        photo: link.photo?,
        path: photocraft_engine::develop_layer_cmds::source_of(sm)?.to_string(),
    })
}

pub fn active_origin(app: &PhotocraftApp) -> Option<LibraryOrigin> {
    let st = app.session.active()?;
    if let Some(layer) = st.active_layer.and_then(|id| st.doc.layer(id))
        && layer_photo(layer).is_some()
    {
        return layer_origin(layer);
    }
    origin_of(&st.doc)
}

pub fn origin_of(doc: &photocraft_doc::Document) -> Option<LibraryOrigin> {
    doc.walk().iter().find_map(|(_, _, layer)| layer_origin(layer))
}

/// A qualified, captured source family. The host refreshes this mirror before editor jobs
/// publish. A changed catalog, source identity or Develop setting invalidates the save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibrarySaveSource {
    pub origin: LibraryOrigin,
    pub master: u64,
    pub canonical_path: std::path::PathBuf,
    pub source_paths: Vec<String>,
    pub settings: Vec<(u64, Value)>,
}
pub type LibrarySaveSources = std::sync::Arc<std::sync::Mutex<Vec<LibrarySaveSource>>>;

pub fn same_file_path(a: &str, b: &str) -> bool {
    a == b || li_fs::same_file(std::path::Path::new(a), std::path::Path::new(b))
}

/// A file Save / Save As wrote (local-image: the host takes these for the Library).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibrarySave {
    /// File result to update when Library publication completes.
    pub notice: Option<u64>,
    pub origin: Option<LibraryOrigin>,
    /// The file written.
    pub path: String,
    /// The Library photo the document came from (its Develop layer's), if any: a file the Library
    /// doesn't have yet is added stacked on top of it.
    pub photo: Option<u64>,
    /// File › Save and Return to Library: show the Library on the saved photo afterwards.
    pub show: bool,
    /// Flattened replacement of the source, with its previous version in Trash.
    pub overwritten: bool,
    /// Ordinary Save As replaced this Library source; no Trash backup is implied.
    pub source_reset: Option<LibrarySaveSource>,
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
    active_origin(app).map(|origin| (origin.photo, origin.path))
}

/// Source identity independent of the currently selected document tab.
pub fn original_of(doc: &photocraft_doc::Document) -> Option<(u64, String)> {
    origin_of(doc).map(|origin| (origin.photo, origin.path))
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
    // Save As asks where first: the return waits for its answer.
    let saved = app.save_as(path);
    app.after_file_dialog(saved, move |app, saved| {
        let saved = saved?;
        if let Some(pending) = app.pending_save.as_mut() {
            pending.return_to_library = true;
            return Ok(saved);
        }
        if let Some(prompt) = app.tiff_options.as_mut() {
            prompt.return_to_library = true;
        }
        match app.library_saves.get_mut(before..).and_then(<[LibrarySave]>::last_mut) {
            Some(s) => s.show = true,
            // A save that waits on a prompt (a layered TIFF's options) stays in Compositing.
            None if app.tiff_options.is_none() && app.session.active().is_some_and(|d| !d.is_dirty()) => app.switch_module = Some(crate::Module::Library),
            None => {}
        }
        Ok(saved)
    })
}

pub fn sync_in(app: &mut PhotocraftApp, library: &str, photo: u64, path: &str, settings: &Value) {
    match app.run("develop.syncPhoto", json!({"library": library, "photo": photo, "path": path, "settings": settings})) {
        Ok(r) => finish_sync(app, &r),
        Err(e) => {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
}

pub fn open_photo_in(app: &mut PhotocraftApp, library: &str, photo: u64, path: &str, settings: &Value, name: &str) -> Result<(), String> {
    let result = app.run("develop.openPhoto", json!({"library": library, "photo": photo, "path": path, "settings": settings, "name": name}))?;
    admit_photo_result(app, &result);
    app.sync_views();
    Ok(())
}

fn admit_photo_result(app: &mut PhotocraftApp, result: &Value) {
    if result["pending"].as_bool() == Some(true) {
        return;
    }
    if let Some(document) = result["documentId"].as_u64().map(photocraft_doc::DocId) {
        crate::toolsets_ui::persona::photo_admitted(app, document);
    }
}

/// Develop layers following `photo` take `settings` (one undo step per document that changed).
pub fn sync(app: &mut PhotocraftApp, photo: u64, settings: &Value) {
    match app.run("develop.syncPhoto", json!({ "photo": photo, "settings": settings })) {
        Ok(r) => finish_sync(app, &r),
        Err(e) => {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
}

pub(crate) fn finish_sync(app: &mut PhotocraftApp, r: &Value) {
    if let Some(added) = r["added"].as_array().filter(|a| !a.is_empty()) {
        for a in added {
            let name = a["name"].as_str().unwrap_or("Develop");
            let msg = li_i18n::fmt(tl!("Added your new Develop edits as layer {name}"), &[("name", name)]);
            crate::notices::post(app, msg.clone(), vec![tl!("The layers below are unchanged; hide or delete the new layer to go back.").into()], false, None);
            app.ui.status = msg;
        }
        app.ui.status_error = false;
        app.sync_views();
    } else if r["updated"].as_u64().unwrap_or(0) > 0 {
        app.ui.status = tl!("Updated from Develop").into();
        app.ui.status_error = false;
        app.sync_views();
    }
}

/// Shows Library photo `photo` (the file at `path`, developed with `settings`) in Compositing: the
/// document that already has it, or a new one whose Background is its Develop layer.
pub fn open_photo(app: &mut PhotocraftApp, photo: u64, path: &str, settings: &Value, name: &str) -> Result<(), String> {
    let r = app.run("develop.openPhoto", json!({ "path": path, "photo": photo, "settings": settings, "name": name }))?;
    admit_photo_result(app, &r);
    app.sync_views();
    if r["pending"].as_bool() != Some(true) && !r["existing"].as_bool().unwrap_or(false) {
        app.ui.status = li_i18n::fmt(tl!("{name} · Develop layer (double-click it to return to Develop)"), &[("name", name)]);
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
    app.develop_request = Some(active_origin(app).ok_or("This Develop layer has no source identity; reopen the photo from its Library")?);
    Ok(json!({ "photo": photo }))
}

#[cfg(test)]
mod tests {

    #[test]
    fn ds2_library_save_uses_active_develop_photo() {
        let (dir, first, mut app) = photo_app("ds2-active-source");
        let state = app.session.active_mut().unwrap();
        let mut second = state.doc.layers[0].clone();
        second.id = photocraft_doc::LayerId::fresh();
        if let LayerContent::Smart(smart) = &mut second.content {
            smart.develop.as_mut().unwrap().photo = Some(8);
            let path = dir.join("other.png").to_string_lossy().into_owned();
            smart.develop.as_mut().unwrap().source = Some(path.clone().into());
            smart.source = SmartSource::Linked { path };
        } else {
            panic!("expected Develop layer");
        }
        let id = second.id;
        std::sync::Arc::make_mut(&mut state.doc).layers.push(second);
        state.active_layer = Some(id);
        assert_eq!(library_original(&app).unwrap().0, 8);
        assert_eq!(library_save_path(&app).unwrap(), dir.join("other-Edit.psd").to_string_lossy());
        app.session.active_mut().unwrap().active_layer = None;
        assert_eq!(library_original(&app), Some((7, first)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn library_identity_save_handoff_is_captured_before_document_saved_scripts() {
        let (dir, path, mut app) = photo_app("identity-save-script");
        let a = active_origin(&app).unwrap();
        open_photo_in(&mut app, "other-library", 7, &path, &json!({}), "other.png").unwrap();
        let b = active_origin(&app).unwrap();
        assert_ne!(a, b);
        app.session.set_active(0);
        app.run("file.scripts.scriptEventsManager", json!({"add":{"event":"saveDocument","steps":[["document.activate",{"document":1}]]}})).unwrap();
        app.services.export = Some(std::sync::Arc::new(|_, _, _| Ok((vec![1], Vec::new()))));
        app.services.write = Some(Box::new(|_, _| Ok(())));
        app.save_as(Some(dir.join("saved.psd").to_string_lossy().to_string())).unwrap();
        assert_eq!(app.session.active_index(), Some(1), "Save Document callback did not run");
        let saved = app.library_saves.last().unwrap();
        assert_eq!(saved.origin.as_ref(), Some(&a));
        assert_ne!(saved.origin.as_ref(), Some(&b));
        std::fs::remove_dir_all(dir).unwrap();
    }

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
        assert_eq!(app.develop_request, active_origin(&app));
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

    #[test]
    fn develop_photo_success_admits_pixel_move_and_failure_preserves_vector() {
        use photocraft_engine::toolsets::Persona;
        let (dir, path, mut app) = photo_app("persona-admission");
        let ctx = egui::Context::default();
        crate::toolsets_ui::persona::frame(&mut app, &ctx);
        crate::toolsets_ui::persona::select(&mut app, &ctx, Persona::Vector);
        let document = app.session.active().unwrap().doc.id;
        assert!(open_photo_in(&mut app, "test-library", 99, &format!("{path}.missing"), &json!({}), "missing.png").is_err());
        crate::toolsets_ui::persona::frame(&mut app, &ctx);
        assert_eq!(app.session.active().unwrap().doc.id, document);
        assert_eq!(app.session.prefs().toolbar.persona, Persona::Vector);
        open_photo_in(&mut app, "test-library", 8, &path, &json!({}), "photo.png").unwrap();
        crate::toolsets_ui::persona::frame(&mut app, &ctx);
        assert_eq!(app.session.prefs().toolbar.persona, Persona::Pixel);
        assert_eq!(app.ui.tool, crate::state::Tool::Move);
        drop(app);
        std::fs::remove_dir_all(dir).unwrap();
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
        app.services.file_dialog = Some(crate::file_dialog::answering(move |request| {
            if let crate::FileDialogRequest::Save { suggested } = request {
                *s.borrow_mut() = suggested;
            }
            None
        }));
        assert_eq!(app.save_as(None).unwrap_err(), "cancelled");
        assert_eq!(*suggested.borrow(), edit2);
        // without the Library (no host), File › Save asks as usual
        app.host_modes = false;
        assert_eq!(library_save_path(&app), None);
        std::fs::remove_dir_all(dir).ok();
    }

    /// Real opening UI, driven by clicks, with the renderer gated on the worker thread.
    #[test]
    fn develop_opening_tab_keeps_frames_running_and_cancel_click_discards_it() {
        use egui_kittest::{Harness, kittest::Queryable};
        use photocraft_engine::develop_layer_cmds::{gate_next_work, work_count_on_thread};
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let (dir, path, mut app) = photo_app("background-ui");
        app.session = photocraft_engine::Session::new();
        app.background_jobs = true;
        let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_max_steps(8).build_eframe(|cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            app
        });
        let gate = Arc::new(AtomicBool::new(false));
        gate_next_work(gate.clone());
        let count = work_count_on_thread();
        open_photo(h.state_mut(), 7, &path, &json!({}), "photo.png").unwrap();
        assert_eq!(work_count_on_thread(), count);
        assert_eq!(h.state().jobs.opens.len(), 1);
        assert!(h.state().session.documents().is_empty());
        let job = h.state().jobs.opens[0].job;
        h.state_mut().jobs.last_started = None;
        open_photo(h.state_mut(), 7, &path, &json!({}), "photo.png").unwrap();
        assert_eq!(h.state().jobs.opens.len(), 1);
        assert_eq!(h.state().jobs.last_started, Some(job), "automation waits on the reused job");
        h.run_steps(3);
        assert!(h.query_all_by_label("Opening photo.png").next().is_some());
        h.get_all_by_label("Cancel").next().unwrap().click();
        h.run_steps(3);
        gate.store(true, Ordering::Relaxed);
        h.state_mut().session.join_cancelled_jobs();
        assert!(h.state().session.documents().is_empty());
        assert!(h.state().jobs.opens.is_empty());
        let gate = Arc::new(AtomicBool::new(false));
        gate_next_work(gate.clone());
        open_photo(h.state_mut(), 7, &path, &json!({}), "photo.png").unwrap();
        h.run_steps(2);
        assert!(h.query_all_by_label("Opening photo.png").next().is_some());
        gate.store(true, Ordering::Relaxed);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while h.state().session.has_jobs() && std::time::Instant::now() < until {
            h.step();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        h.run_steps(2);
        assert_eq!(h.state().session.documents().len(), 1);
        assert!(h.state().jobs.opens.is_empty());
        assert_eq!(active_photo(h.state()), Some(7));
        assert!(h.state().session.active().unwrap().path.is_none(), "Save must not overwrite the raw original");
        assert_eq!(work_count_on_thread(), count);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn internal_develop_profile_prompt_is_opt_in_for_background_handoffs() {
        for ask in [false, true] {
            let (dir, path, mut app) = photo_app(if ask { "profile-ask" } else { "profile-preserve" });
            app.session.execute("file.close", json!({})).unwrap();
            app.sync_views();
            app.session.color.settings.working_rgb = "adobe-rgb-compat".into();
            app.session.color.settings.policy_rgb = photocraft_engine::color_cmds::Policy::Convert;
            app.session.color.settings.ask_on_mismatch = true;
            app.background_jobs = true;
            app.run("develop.openPhoto", json!({"photo":8,"path":path,"settings":{},"askColorProfile":ask})).unwrap();
            let ctx = egui::Context::default();
            let start = std::time::Instant::now();
            while app.session.has_jobs() && start.elapsed() < std::time::Duration::from_secs(5) {
                crate::jobs_ui::tick(&mut app, &ctx);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert!(!app.session.has_jobs(), "handoff did not finish");
            let profile = photocraft_engine::color_cmds::document_profile(&app.session.active().unwrap().doc);
            assert!(!profile.same_colors(photocraft_cms::Builtin::AdobeRgbCompat.profile()));
            let dialogs = app.ui.dialogs.iter().filter(|dialog| dialog.fields.get("__prefsui").and_then(Value::as_str) == Some("mismatch")).count();
            assert_eq!(dialogs, usize::from(ask));
            if ask {
                let id = app.ui.dialogs.iter().find(|dialog| dialog.fields.get("__prefsui").and_then(Value::as_str) == Some("mismatch")).unwrap().id;
                crate::dialogs::cancel(&mut app, id);
                assert_eq!(photocraft_engine::color_cmds::document_profile(&app.session.active().unwrap().doc).to_bytes(), profile.to_bytes());
            }
            drop(app);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn internal_develop_profile_prompt_targets_admitted_doc_after_open_callback_switches_tabs() {
        let (dir, path, mut app) = photo_app("profile-callback");
        app.session.color.settings.working_rgb = "adobe-rgb-compat".into();
        app.run("file.scripts.scriptEventsManager", json!({"add":{"event":"openDocument","steps":[["document.activate",{"document":0}]]}})).unwrap();
        let other = app.session.active().unwrap().doc.clone();
        app.background_jobs = true;
        app.run("develop.openPhoto", json!({"photo":8,"path":path,"settings":{},"askColorProfile":true})).unwrap();
        let ctx = egui::Context::default();
        let start = std::time::Instant::now();
        while app.session.has_jobs() && start.elapsed() < std::time::Duration::from_secs(5) {
            crate::jobs_ui::tick(&mut app, &ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(!app.session.has_jobs());
        assert_eq!(app.session.active_index(), Some(0), "Open Document callback switched to the other tab");
        let admitted = app.session.documents()[1].doc.id;
        let dialog = app.ui.dialogs.iter().find(|dialog| dialog.fields.get("__prefsui").and_then(Value::as_str) == Some("mismatch")).unwrap().id;
        let fields = &mut app.ui.dialog_mut(dialog).unwrap().fields;
        assert_eq!(fields["target"]["doc"], admitted.0);
        fields.insert("action".into(), json!("convert"));
        crate::dialogs::confirm(&mut app, dialog).unwrap();
        assert_eq!(app.session.active_index(), Some(0));
        assert!(app.session.documents()[0].doc.as_ref() == other.as_ref(), "the unrelated tab changed");
        assert!(
            photocraft_engine::color_cmds::document_profile(&app.session.documents()[1].doc).same_colors(photocraft_cms::Builtin::AdobeRgbCompat.profile())
        );
        drop(app);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn internal_develop_profile_prompt_rejects_changed_closed_or_unauthorized_targets() {
        for change in ["edit", "close", "settings", "authorization"] {
            let (dir, _, mut app) = photo_app(&format!("profile-stale-{change}"));
            let target = crate::prefs_ui::mismatch_target(&app.session).unwrap();
            app.session.execute("file.new", json!({"width":2,"height":2})).unwrap();
            app.sync_views();
            let other = app.session.active().unwrap().doc.clone();
            let dialog =
                crate::prefs_ui::open_mismatch_for(&mut app, &json!({"action":"kept","mismatch":true,"embedded":"source","working":"destination"}), target);
            app.ui.dialog_mut(dialog).unwrap().fields.insert("action".into(), json!("discard"));
            match change {
                "edit" => {
                    app.session.set_active(0);
                    app.session.execute("layer.new.layer", json!({})).unwrap();
                    app.session.set_active(1);
                }
                "close" => {
                    app.session.set_active(0);
                    app.session.execute("file.close", json!({})).unwrap();
                }
                "settings" => app.session.color.settings.working_rgb = "display-p3".into(),
                _ => {
                    app.automation_input = true;
                    app.services.automation_command = Some(Box::new(|_, _| Err("denied".into())));
                }
            }
            assert!(crate::dialogs::confirm(&mut app, dialog).is_err(), "{change}");
            let actual = app.session.documents().iter().find(|state| state.doc.id == other.id).unwrap();
            assert!(actual.doc.as_ref() == other.as_ref(), "{change}: other tab changed");
            assert_eq!(app.session.active().unwrap().doc.id, other.id);
            drop(app);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
