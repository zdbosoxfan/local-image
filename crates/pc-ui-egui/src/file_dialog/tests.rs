use super::*;
use crate::{Services, menus};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// The requests the fake dialog service was given, with their replies: dialogs the user is still in.
type Open = Rc<RefCell<Vec<(FileDialogRequest, FileDialogReply)>>>;

/// An app with an untitled document, whose file dialogs stay open until the test answers them, and
/// whose writer records the paths it wrote.
fn app() -> (PhotocraftApp, Open, Rc<RefCell<Vec<String>>>) {
    let (open, written): (Open, Rc<RefCell<Vec<String>>>) = Default::default();
    let (held, w) = (open.clone(), written.clone());
    let services = Services {
        export: Some(Arc::new(|_: &photocraft_doc::Document, _: &str, _: &crate::ExportSettings| Ok((b"out".to_vec(), Vec::new())))),
        write: Some(Box::new(move |path: &str, _: &[u8]| {
            w.borrow_mut().push(path.to_string());
            Ok(())
        })),
        file_dialog: Some(Box::new(move |request, reply| held.borrow_mut().push((request, reply)))),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    (app, open, written)
}

/// The user answers the open dialog, from the dialog's own thread.
fn answer(open: &Open, answer: Option<FileDialogAnswer>) {
    let (_, reply) = open.borrow_mut().pop().expect("a dialog is open");
    std::thread::spawn(move || reply.send(answer)).join().unwrap();
}

#[test]
fn the_app_keeps_running_while_a_dialog_is_open() {
    let (mut app, open, written) = app();
    let ctx = egui::Context::default();
    assert_eq!(menus::invoke(&mut app, &ctx, "file.saveAs", json!({})).unwrap(), json!({"fileDialog": "save"}));
    assert!(matches!(open.borrow().as_slice(), [(FileDialogRequest::Save { suggested }, _)] if suggested.ends_with(".psd")));
    // Frames go on while the user is in the dialog: nothing is written, commands still run, and
    // a second dialog is refused.
    for _ in 0..3 {
        app.poll_file_dialog(&ctx);
    }
    assert!(written.borrow().is_empty());
    assert!(app.file_dialog_open());
    app.run("layer.new.layer", json!({})).unwrap();
    assert_eq!(menus::invoke(&mut app, &ctx, "file.open", json!({})).unwrap_err(), "a file dialog is already open");
    assert_eq!(open.borrow().len(), 1);
    // The answer wakes the app, and the save goes on from where it asked.
    for _ in 0..4 {
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    }
    assert!(!ctx.has_requested_repaint(), "idle");
    answer(&open, Some(FileDialogAnswer::SaveTo("/pics/a.psd".into())));
    assert!(ctx.has_requested_repaint());
    app.poll_file_dialog(&ctx);
    assert_eq!(*written.borrow(), ["/pics/a.psd"]);
    let st = app.session.active().unwrap();
    assert_eq!(st.path.as_deref(), Some("/pics/a.psd"));
    assert!(!st.is_dirty());
    assert!(!app.file_dialog_open());
}

#[test]
fn the_answer_goes_to_the_document_it_was_asked_for() {
    let (mut app, open, written) = app();
    let ctx = egui::Context::default();
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    app.session.set_active(0);
    menus::invoke(&mut app, &ctx, "file.saveAs", json!({})).unwrap();
    app.session.set_active(1);
    answer(&open, Some(FileDialogAnswer::SaveTo("/pics/first.psd".into())));
    app.poll_file_dialog(&ctx);
    let paths: Vec<_> = app.session.documents().iter().map(|d| d.path.clone()).collect();
    assert_eq!(paths, [Some("/pics/first.psd".to_string()), None]);
    assert_eq!(app.session.active_index(), Some(0));
    // Once that document is closed, the answer has nothing to save.
    menus::invoke(&mut app, &ctx, "file.saveAs", json!({})).unwrap();
    app.run("file.close", json!({"document": 0})).unwrap();
    answer(&open, Some(FileDialogAnswer::SaveTo("/pics/again.psd".into())));
    app.poll_file_dialog(&ctx);
    assert_eq!(*written.borrow(), ["/pics/first.psd"]);
    assert!(app.ui.status_error && app.session.documents()[0].path.is_none());
}

#[test]
fn cancel_and_a_dialog_that_could_not_show_end_quietly() {
    let (mut app, open, written) = app();
    let ctx = egui::Context::default();
    menus::invoke(&mut app, &ctx, "file.saveAs", json!({})).unwrap();
    answer(&open, None);
    app.poll_file_dialog(&ctx);
    assert!(!app.file_dialog_open() && !app.ui.status_error);
    // A reply dropped unanswered (the dialog failed to show) is a Cancel, not a dialog left open forever.
    menus::invoke(&mut app, &ctx, "file.saveAs", json!({})).unwrap();
    open.borrow_mut().clear();
    app.poll_file_dialog(&ctx);
    assert!(!app.file_dialog_open() && !app.ui.status_error);
    assert!(written.borrow().is_empty());
    // Without a dialog service, asking is a Cancel straight away.
    app.services.file_dialog = None;
    assert_eq!(app.save_as(None).unwrap_err(), CANCELLED);
    assert_eq!(app.open_dialog_file(), Ok(Value::Null), "opening nothing isn't an error");
    assert!(!app.file_dialog_open());
}

#[test]
fn a_dialog_answered_at_once_continues_the_command_at_once() {
    let (mut app, _, written) = app();
    let ctx = egui::Context::default();
    app.services.file_dialog = Some(answering(|_| Some(FileDialogAnswer::SaveTo("/pics/now.psd".into()))));
    let r = menus::invoke(&mut app, &ctx, "file.saveAs", json!({})).unwrap();
    assert_eq!(r["path"], "/pics/now.psd");
    assert_eq!(*written.borrow(), ["/pics/now.psd"]);
    assert!(!app.file_dialog_open());
    app.services.file_dialog = Some(answering(|_| None));
    assert_eq!(menus::invoke(&mut app, &ctx, "file.saveAs", json!({})).unwrap_err(), CANCELLED);
}

#[test]
fn an_answer_of_the_wrong_kind_is_an_error_not_a_crash() {
    let (mut app, open, written) = app();
    let ctx = egui::Context::default();
    menus::invoke(&mut app, &ctx, "file.saveAs", json!({})).unwrap();
    answer(&open, Some(FileDialogAnswer::Paths(vec!["/pics/a.psd".into()])));
    app.poll_file_dialog(&ctx);
    assert!(app.ui.status_error && app.ui.status == UNEXPECTED);
    assert!(written.borrow().is_empty());
    app.open_dialog_file().unwrap();
    answer(&open, Some(FileDialogAnswer::SaveTo("/pics/a.psd".into())));
    app.poll_file_dialog(&ctx);
    assert_eq!(app.session.documents().len(), 1);
}

/// #6597: once Save As or Save a Copy writes over the file another open document is bound to,
/// that document no longer has the file and reads as unsaved (so closing it asks, and its Save
/// asks for a name), and a notice says so. Documents bound to other files are left alone.
#[test]
fn saving_over_the_file_of_another_open_document_detaches_it() {
    let dir = std::env::temp_dir().join(format!("photocraft-save-over-tab-{}", std::process::id()));
    let (a, d) = (dir.join("a.psd").to_string_lossy().into_owned(), dir.join("d.psd").to_string_lossy().into_owned());
    let (mut app, open, written) = app();
    let saved_as = |app: &mut PhotocraftApp, path: &str| {
        let st = app.session.active_mut().unwrap();
        st.path = Some(path.to_string());
        st.saved_revision = st.revision;
    };
    // A lives in a.psd and D in d.psd, both saved.
    saved_as(&mut app, &a);
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    saved_as(&mut app, &d);
    // B, in another tab, is saved as a.psd.
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    app.save_as(Some(a.clone())).unwrap();
    let docs = app.session.documents();
    assert_eq!((docs[0].path.as_deref(), docs[0].is_dirty()), (None, true), "A lost a.psd and is unsaved");
    assert_eq!((docs[1].path.as_deref(), docs[1].is_dirty()), (Some(d.as_str()), false), "D is untouched");
    assert_eq!(docs[2].path.as_deref(), Some(a.as_str()), "B lives in a.psd");
    let notice = app.ui.notices.iter().find(|n| n.title.contains("a.psd was saved over")).expect("a notice");
    assert!(notice.title.contains("a.psd was saved over") && notice.lines.len() == 1, "{} {:?}", notice.title, notice.lines);
    assert_eq!(notice.lines, [format!("\"{}\" no longer has a file and is marked unsaved: save it under a new name to keep it", docs[0].doc.name)]);
    // C's Save a Copy onto a.psd detaches B the same way, and C doesn't take the file.
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    let ctx = egui::Context::default();
    menus::invoke(&mut app, &ctx, "file.saveACopy", json!({})).unwrap();
    app.poll_file_dialog(&ctx);
    answer(&open, Some(FileDialogAnswer::SaveTo(a.clone())));
    app.poll_file_dialog(&ctx);
    let docs = app.session.documents();
    assert_eq!((docs[2].path.as_deref(), docs[2].is_dirty()), (None, true), "B lost a.psd and is unsaved");
    assert_eq!(docs[3].path, None, "a copy doesn't bind C to a.psd");
    assert_eq!(*written.borrow(), [a.clone(), a]);
}

#[test]
fn automation_saves_disable_link_reads_and_detach_only_after_a_successful_write() {
    let (mut app, _, _) = app();
    let path = "/pics/automation-overwrite.psd";
    app.session.active_mut().unwrap().path = Some(path.into());
    let saved = app.session.active().unwrap().revision;
    app.session.active_mut().unwrap().saved_revision = saved;
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    app.services.export = Some(Arc::new(|_, _, settings| {
        assert!(!settings.read_linked_files);
        Ok((b"out".to_vec(), Vec::new()))
    }));
    app.services.automation_write = Some(Box::new(|_, _| Err("publication failed".into())));
    assert!(app.save_automation(Some(path.into())).is_err());
    assert_eq!(app.session.documents()[0].path.as_deref(), Some(path));
    assert!(!app.session.documents()[0].is_dirty());
    app.services.automation_write = Some(Box::new(|_, _| Ok(())));
    app.save_automation(Some(path.into())).unwrap();
    assert_eq!(app.session.documents()[0].path, None);
    assert!(app.session.documents()[0].is_dirty());
}

#[test]
fn background_saves_detach_overwritten_tabs_only_after_commit() {
    for fail in [false, true] {
        let (mut app, _, _) = app();
        let path = "/pics/background-overwrite.psd";
        app.session.active_mut().unwrap().path = Some(path.into());
        let saved = app.session.active().unwrap().revision;
        app.session.active_mut().unwrap().saved_revision = saved;
        app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
        app.background_jobs = true;
        app.services.prepare_save = Some(Arc::new(move |snapshot, _, _, _| {
            Ok(crate::PreparedSave {
                snapshot,
                original: None,
                warnings: Vec::new(),
                write: crate::PreparedWrite { commit: Box::new(move || if fail { Err("publication failed".into()) } else { Ok(()) }) },
            })
        }));
        app.save_as(Some(path.into())).unwrap();
        assert_eq!(app.session.documents()[0].path.as_deref(), Some(path), "encoding hasn't committed");
        let ctx = egui::Context::default();
        let start = std::time::Instant::now();
        while app.pending_save.is_some() {
            crate::jobs_ui::tick_background(&mut app, &ctx);
            assert!(start.elapsed() < std::time::Duration::from_secs(30));
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(app.session.documents()[0].path.as_deref(), if fail { Some(path) } else { None });
        assert_eq!(app.session.documents()[0].is_dirty(), !fail);
    }
}
