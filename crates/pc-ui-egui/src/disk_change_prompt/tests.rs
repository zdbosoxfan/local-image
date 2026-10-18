#![allow(clippy::unwrap_used)]
use super::*;
use crate::Services;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// How many times each path was written, standing in for its size and modification time.
type Disk = Rc<RefCell<HashMap<String, u64>>>;
type Written = Rc<RefCell<Vec<String>>>;

/// An app whose writes and file stamps go to a fake disk.
fn app() -> (PhotocraftApp, Disk, Written) {
    let disk = Disk::default();
    let written = Written::default();
    let (read, wrote, log) = (disk.clone(), disk.clone(), written.clone());
    let app = PhotocraftApp::new(
        photocraft_engine::Session::new(),
        Services {
            export: Some(Box::new(|doc, path, _| {
                let r = photocraft_io::export(doc, path.rsplit('.').next().unwrap(), &Default::default()).map_err(|e| e.to_string())?;
                Ok((r.bytes, r.warnings))
            })),
            write: Some(Box::new(move |path, _| {
                *wrote.borrow_mut().entry(path.to_string()).or_default() += 1;
                log.borrow_mut().push(path.to_string());
                Ok(())
            })),
            file_stamp: Some(Box::new(move |path| read.borrow().get(path).map(|&n| FileStamp { len: n, modified: None }))),
            ..Default::default()
        },
    );
    (app, disk, written)
}

fn changed_by_another_app(disk: &Disk, path: &str) {
    *disk.borrow_mut().entry(path.to_string()).or_default() += 1;
}

fn prompt(app: &PhotocraftApp) -> Option<&crate::state::Dialog> {
    app.ui.dialogs.iter().find(|d| owns(&d.fields))
}

#[test]
fn save_asks_before_replacing_a_file_another_app_changed() {
    let ctx = egui::Context::default();
    let (mut app, disk, written) = app();
    app.run("file.new", serde_json::json!({"width": 8, "height": 8})).unwrap();
    changed_by_another_app(&disk, "/pics/a.psd");
    app.opened_from("/pics/a.psd");
    app.run("layer.new.layer", serde_json::json!({})).unwrap();
    // Unchanged on disk: Save writes at once.
    crate::menus::invoke(&mut app, &ctx, "file.save", serde_json::json!({})).unwrap();
    assert_eq!(*written.borrow(), ["/pics/a.psd"]);
    assert!(prompt(&app).is_none());

    // Our own save isn't a change; another app's write after it is.
    changed_by_another_app(&disk, "/pics/a.psd");
    app.run("layer.new.layer", serde_json::json!({})).unwrap();
    let r = crate::menus::invoke(&mut app, &ctx, "file.save", serde_json::json!({})).unwrap();
    assert_eq!(r["confirmationPending"], true);
    assert_eq!(written.borrow().len(), 1, "nothing is written before the answer");
    let p = prompt(&app).expect("Save asks first");
    assert!(p.fields["message"].as_str().unwrap().contains("a.psd"));

    // Cancel leaves the file and the unsaved document alone.
    let id = p.id;
    app.ui.close_dialog(id);
    assert_eq!(written.borrow().len(), 1);
    assert!(app.session.active().unwrap().is_dirty());

    // Replace writes it, and the next Save doesn't ask again.
    let id = crate::menus::invoke(&mut app, &ctx, "file.save", serde_json::json!({})).unwrap()["dialog"].as_u64().unwrap();
    crate::dialogs::confirm(&mut app, id).unwrap();
    assert_eq!(written.borrow().len(), 2);
    assert!(!app.session.active().unwrap().is_dirty());
    app.run("layer.new.layer", serde_json::json!({})).unwrap();
    crate::menus::invoke(&mut app, &ctx, "file.save", serde_json::json!({})).unwrap();
    assert_eq!(written.borrow().len(), 3);
    assert!(prompt(&app).is_none());
}

/// A file that was never stamped (no stamp service, or a new document) never asks.
#[test]
fn an_unstamped_file_saves_without_asking() {
    let ctx = egui::Context::default();
    let (mut app, _disk, written) = app();
    app.services.file_stamp = None;
    app.run("file.new", serde_json::json!({"width": 8, "height": 8})).unwrap();
    app.opened_from("/pics/b.psd");
    app.run("layer.new.layer", serde_json::json!({})).unwrap();
    crate::menus::invoke(&mut app, &ctx, "file.save", serde_json::json!({})).unwrap();
    assert_eq!(*written.borrow(), ["/pics/b.psd"]);
    assert!(prompt(&app).is_none());
}
