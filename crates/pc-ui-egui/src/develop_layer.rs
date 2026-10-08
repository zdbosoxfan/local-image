//! local-image: Develop layers in Compositing (see `photocraft_engine::develop_layer_cmds`): a
//! photo from the Library's Develop module, shown as the original file plus its develop settings,
//! re-rendered through the same pipeline — no TIFF copies. The host (the app's Library | Develop |
//! Compositing switch) opens photos with [`open_photo`], keeps the layers following the Library
//! with [`sync`], and goes to Develop when a Develop layer is double-clicked ([`DEVELOP_ID`]).

use photocraft_doc::{Layer, LayerContent, SmartObject};
use serde_json::{Value, json};

use crate::PhotocraftApp;

/// Layer › Smart Objects › Develop… (and double-clicking a Develop layer's thumbnail).
pub const DEVELOP_ID: &str = "li.developLayer";

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

/// Develop layers following `photo` take `settings` (one undo step per document that changed).
pub fn sync(app: &mut PhotocraftApp, photo: u64, settings: &Value) {
    match app.run("develop.syncPhoto", json!({ "photo": photo, "settings": settings })) {
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
}
