use super::*;
use crate::jobs::{JobId, Started};
use std::sync::atomic::{AtomicBool, Ordering};

struct Photo {
    path: std::path::PathBuf,
    bytes: Vec<u8>,
}
impl Photo {
    fn new(tag: &str) -> Self {
        use photocraft_raw::testgen::{DngSpec, mosaic, scene};
        let bytes = DngSpec::cfa(64, 40, mosaic(&scene(64, 40), 64, [0, 1, 1, 2], 0, 65535)).build();
        let path = std::env::temp_dir().join(format!("develop-job-{tag}-{}.dng", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        Self { path, bytes }
    }
    fn params(&self) -> Value {
        json!({"path": self.path.to_str().unwrap(), "photo": 71, "settings": {}, "name": "camera.dng"})
    }
    fn session(&self) -> Session {
        let mut s = Session::new();
        s.execute(OPEN_JOB, self.params()).unwrap();
        s
    }
}
impl Drop for Photo {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn job(s: &mut Session, cmd: &str, p: Value) -> JobId {
    let Started::Job(id) = s.start(cmd, p).unwrap() else { panic!("expected background job") };
    id
}
fn sync_params(ev: f64) -> Value {
    json!({"photo": 71, "settings": {"light": {"exposure": ev}}})
}

#[test]
fn develop_open_job_matches_the_synchronous_document_and_link() {
    let photo = Photo::new("open");
    let mut s = Session::new();
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let count = work_count_on_thread();
    let id = job(&mut s, OPEN_JOB, photo.params());
    assert_eq!(work_count_on_thread(), count, "no develop work on the caller");
    assert!(s.documents().is_empty());
    // Identical opening request focuses the existing job, never opens twice.
    let Started::Done(pending) = s.start(OPEN_JOB, photo.params()).unwrap() else { panic!("duplicate open") };
    assert_eq!(pending["job"], id.0);
    gate.store(true, Ordering::Relaxed);
    let result = s.wait_job(id).unwrap();
    assert_eq!(result["existing"], false);
    let expected = photo.session();
    let actual = s.active().unwrap().doc.clone();
    let expected = &expected.active().unwrap().doc;
    assert_eq!((actual.size, actual.depth, actual.mode, &actual.icc_profile), (expected.size, expected.depth, expected.mode, &expected.icc_profile));
    assert_eq!(actual.layers[0].surface().unwrap().read_region(actual.bounds()), expected.layers[0].surface().unwrap().read_region(expected.bounds()));
    assert_eq!(develop_layers(&actual)[0].1, Some(71));
    let LayerContent::Smart(sm) = &actual.layers[0].content else { panic!("Develop smart object") };
    assert_eq!(sm.source, SmartSource::Linked { path: photo.path.display().to_string() });
    assert_eq!(sm.develop, Some(DevelopLink { source: Some(photo.path.display().to_string()), library: None, settings: json!({}), photo: Some(71) }));
    assert_eq!(actual.metadata, expected.metadata);
    assert_eq!(actual.name, "camera");
    let count = work_count_on_thread();
    assert!(matches!(s.start(OPEN_JOB, photo.params()).unwrap(), Started::Done(_)));
    assert_eq!(work_count_on_thread(), count);
    assert_eq!(s.documents().len(), 1);
    // The worker result also agrees with the old direct IO import's pixels.
    let imported = photocraft_io::raw::develop_with_settings("camera.dng", &photo.bytes, &json!({})).unwrap();
    assert_eq!(
        actual.layers[0].surface().unwrap().read_region(actual.bounds()),
        imported.document.layers[0].surface().unwrap().read_region(imported.document.bounds())
    );
}

#[test]
fn develop_superseded_render_never_replaces_newer_settings() {
    let photo = Photo::new("latest");
    let mut s = photo.session();
    let old = s.active().unwrap().doc.clone();
    let steps = s.active().unwrap().history.past_len();
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let count = work_count_on_thread();
    let stale = job(&mut s, "develop.syncPhoto", sync_params(1.0));
    assert!(Arc::ptr_eq(&old, &s.active().unwrap().doc));
    let latest = job(&mut s, "develop.syncPhoto", sync_params(2.0));
    assert_eq!(work_count_on_thread(), count);
    assert!(s.job(stale).is_none());
    s.wait_job(latest).unwrap();
    gate.store(true, Ordering::Relaxed);
    s.join_cancelled_jobs();
    s.poll_jobs();
    assert_eq!(s.active().unwrap().history.past_len(), steps + 1);
    let LayerContent::Smart(sm) = &s.active().unwrap().doc.layers[0].content else { panic!() };
    assert_eq!(sm.develop.as_ref().unwrap().settings, sync_params(2.0)["settings"]);
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(&*s.active().unwrap().doc, &*old);
}

#[test]
fn develop_return_to_current_settings_cancels_pending_render() {
    let photo = Photo::new("revert");
    let mut s = photo.session();
    let old = s.active().unwrap().doc.clone();
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let stale = job(&mut s, "develop.syncPhoto", sync_params(1.0));
    assert!(matches!(s.start("develop.syncPhoto", json!({"photo": 71, "settings": {}})).unwrap(), Started::Done(_)));
    assert!(s.job(stale).is_none());
    gate.store(true, Ordering::Relaxed);
    s.join_cancelled_jobs();
    s.poll_jobs();
    assert!(Arc::ptr_eq(&old, &s.active().unwrap().doc));
}

#[test]
fn develop_set_supersedes_sync_and_preserves_unrelated_edits() {
    let photo = Photo::new("set");
    let mut s = photo.session();
    let target = s.active().unwrap().doc.layers[0].id;
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let stale = job(&mut s, "develop.syncPhoto", sync_params(1.0));
    let new_gate = Arc::new(AtomicBool::new(false));
    gate_next_work(new_gate.clone());
    let latest = job(&mut s, "layer.develop.set", json!({"layer": target.0, "settings": {"light": {"exposure": 2.0}}}));
    assert!(s.job(stale).is_none());
    s.execute("layer.new.layer", json!({})).unwrap();
    let retouch = s.active().unwrap().doc.layers[1].clone();
    let steps = s.active().unwrap().history.past_len();
    new_gate.store(true, Ordering::Relaxed);
    s.wait_job(latest).unwrap();
    assert_eq!(s.active().unwrap().doc.layers[1], retouch);
    assert_eq!(s.active().unwrap().history.past_len(), steps + 1);
    gate.store(true, Ordering::Relaxed);
    s.join_cancelled_jobs();
}

#[test]
fn develop_compositing_started_during_render_gets_a_new_layer() {
    let photo = Photo::new("paint");
    let mut s = photo.session();
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let id = job(&mut s, "develop.syncPhoto", sync_params(1.0));
    s.execute("layer.new.layer", json!({})).unwrap();
    let before = s.active().unwrap().doc.layers.clone();
    let steps = s.active().unwrap().history.past_len();
    gate.store(true, Ordering::Relaxed);
    let result = s.wait_job(id).unwrap();
    assert_eq!(result["added"][0]["name"], "Develop 2");
    assert_eq!(&s.active().unwrap().doc.layers[..2], &before);
    assert_eq!(s.active().unwrap().history.past_len(), steps + 1);
}

#[test]
fn develop_changed_or_closed_target_is_never_overwritten() {
    let photo = Photo::new("changed");
    let mut s = photo.session();
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let id = job(&mut s, "develop.syncPhoto", sync_params(1.0));
    s.execute("layer.setProps", json!({"name": "my layer"})).unwrap();
    let old = s.active().unwrap().doc.clone();
    gate.store(true, Ordering::Relaxed);
    assert_eq!(s.wait_job(id).unwrap()["updated"], 0);
    assert!(Arc::ptr_eq(&old, &s.active().unwrap().doc));
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let id = job(&mut s, "develop.syncPhoto", sync_params(2.0));
    s.execute("file.close", json!({})).unwrap();
    gate.store(true, Ordering::Relaxed);
    s.wait_job(id).unwrap();
    assert!(s.documents().is_empty());
}

#[test]
fn develop_multi_document_sync_follows_ids_and_preserves_focus() {
    let photo = Photo::new("many");
    let mut s = photo.session();
    let mut copy = (*s.active().unwrap().doc).clone();
    copy.id = DocId::fresh();
    let first = s.active().unwrap().doc.id;
    let second = copy.id;
    s.open_document(copy, None);
    s.execute("layer.new.layer", json!({})).unwrap();
    let second_layers = s.active().unwrap().doc.layers.clone();
    let steps: Vec<_> = s.documents().iter().map(|d| (d.doc.id, d.history.past_len())).collect();
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let id = job(&mut s, "develop.syncPhoto", sync_params(1.0));
    s.execute("document.move", json!({"document": 0, "to": 1})).unwrap();
    s.set_active(1);
    assert_eq!(s.active().unwrap().doc.id, first);
    gate.store(true, Ordering::Relaxed);
    let result = s.wait_job(id).unwrap();
    assert_eq!(result["updated"], 1);
    assert_eq!(result["added"].as_array().unwrap().len(), 1);
    assert_eq!(s.active().unwrap().doc.id, first);
    for d in s.documents() {
        assert_eq!(d.history.past_len(), steps.iter().find(|(id, _)| *id == d.doc.id).unwrap().1 + 1);
        if d.doc.id == second {
            assert_eq!(&d.doc.layers[..2], &second_layers);
        }
    }
}

#[test]
fn develop_missing_source_keeps_pixels_settings_and_history() {
    let photo = Photo::new("missing");
    let mut s = photo.session();
    let old = s.active().unwrap().doc.clone();
    let steps = s.active().unwrap().history.past_len();
    std::fs::remove_file(&photo.path).unwrap();
    let id = job(&mut s, "develop.syncPhoto", sync_params(1.0));
    assert!(s.wait_job(id).unwrap_err().to_string().contains("unavailable"));
    assert!(Arc::ptr_eq(&old, &s.active().unwrap().doc));
    assert_eq!(s.active().unwrap().history.past_len(), steps);
}

#[test]
fn library_identity_separates_colliding_photos_sources_and_pending_jobs() {
    let a = Photo::new("identity-a");
    let b = Photo::new("identity-b");
    let mut s = Session::new();
    let mut pa = a.params();
    pa["library"] = json!("library-a");
    let mut pb = b.params();
    pb["library"] = json!("library-b");
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let pending_a = job(&mut s, OPEN_JOB, pa.clone());
    let pending_b = job(&mut s, OPEN_JOB, pb.clone());
    s.wait_job(pending_b).unwrap();
    let b_snapshot = s.active().unwrap().doc.clone();
    gate.store(true, Ordering::Relaxed);
    s.wait_job(pending_a).unwrap();
    assert_eq!(s.documents().len(), 2);
    let ai = document_of_photo_in(&s, Some("library-a"), 71, a.path.to_str()).unwrap();
    let bi = document_of_photo_in(&s, Some("library-b"), 71, b.path.to_str()).unwrap();
    assert_ne!(ai, bi);
    assert!(Arc::ptr_eq(&s.documents()[bi].doc, &b_snapshot));
    assert!(document_of_photo_in(&s, Some("library-a"), 71, b.path.to_str()).is_none());
    assert!(document_of_photo(&s, 71).is_none(), "legacy unbound lookup matched a qualified catalog");
    let a_snapshot = s.documents()[ai].doc.clone();
    s.execute("develop.syncPhoto", json!({"library":"library-b","photo":71,"path":b.path,"settings":{"light":{"exposure":0.4}}})).unwrap();
    assert!(Arc::ptr_eq(&s.documents()[ai].doc, &a_snapshot));
    assert!(!Arc::ptr_eq(&s.documents()[bi].doc, &b_snapshot));
    let changed_b = s.documents()[bi].doc.clone();
    let result = s.execute("develop.syncPhoto", json!({"library":"library-b","photo":71,"path":a.path,"settings":{"light":{"exposure":0.8}}})).unwrap();
    assert_eq!(result["updated"], 0);
    assert!(Arc::ptr_eq(&s.documents()[bi].doc, &changed_b));
    // A virtual copy has the same source file but a distinct catalog photo identity.
    pb["photo"] = json!(72);
    s.execute(OPEN_JOB, pb).unwrap();
    assert_eq!(s.documents().len(), 3);
    // An unbound old-document request also remains independent.
    s.execute(OPEN_JOB, a.params()).unwrap();
    assert_eq!(s.documents().len(), 4);
    s.set_active(ai);
    s.execute("layer.smartObjects.relinkToFile", json!({"path":b.path})).unwrap();
    let LayerContent::Smart(sm) = &s.active().unwrap().doc.layers[0].content else { panic!("smart layer") };
    let link = sm.develop.as_ref().unwrap();
    assert!(link.library.is_none() && link.photo.is_none() && link.source.is_none(), "relinking another source retained a stale Library association");
}

#[test]
fn library_identity_virtual_copies_do_not_cancel_each_others_pending_sync() {
    let photo = Photo::new("identity-virtual-jobs");
    let mut s = Session::new();
    let mut p = photo.params();
    p["library"] = json!("library");
    s.execute(OPEN_JOB, p.clone()).unwrap();
    p["photo"] = json!(72);
    s.execute(OPEN_JOB, p).unwrap();
    let sibling = s.active().unwrap().doc.clone();
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let first = job(&mut s, "develop.syncPhoto", json!({"library":"library","photo":71,"path":photo.path,"settings":{"light":{"exposure":0.7}}}));
    // Host checks every linked photo, including already-current virtual copies.
    s.execute("develop.syncPhoto", json!({"library":"library","photo":72,"path":photo.path,"settings":{}})).unwrap();
    assert_eq!(s.job(first).unwrap().state, "running");
    gate.store(true, Ordering::Relaxed);
    s.wait_job(first).unwrap();
    assert!(Arc::ptr_eq(&s.documents()[1].doc, &sibling));
    let LayerContent::Smart(sm) = &s.documents()[0].doc.layers[0].content else { panic!("smart") };
    assert_eq!(sm.develop.as_ref().unwrap().settings["light"]["exposure"], 0.7);
}

#[test]
fn library_identity_existing_source_supersedes_a_pending_replacement_open() {
    let a = Photo::new("identity-existing-a");
    let b = Photo::new("identity-existing-b");
    let mut s = Session::new();
    let mut pa = a.params();
    pa["library"] = json!("library");
    let mut pb = b.params();
    pb["library"] = json!("library");
    s.execute(OPEN_JOB, pa.clone()).unwrap();
    let original = s.active().unwrap().doc.clone();
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let replacement = job(&mut s, OPEN_JOB, pb);
    assert_eq!(s.execute(OPEN_JOB, pa).unwrap()["existing"], true);
    gate.store(true, Ordering::Relaxed);
    assert!(s.wait_job(replacement).is_err());
    s.join_cancelled_jobs();
    assert_eq!(s.documents().len(), 1);
    assert!(Arc::ptr_eq(&s.active().unwrap().doc, &original));
}
