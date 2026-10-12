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
    assert_eq!(
        sm.develop,
        Some(DevelopLink {
            library_baseline: None,
            source: Some(photo.path.display().to_string().into()),
            library: None,
            settings: json!({}),
            photo: Some(71)
        })
    );
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

#[test]
fn internal_develop_profile_preserves_managed_pixels_despite_external_import_policy() {
    use crate::color_cmds::{Policy, document_profile};
    let photo = Photo::new("internal-profile");
    let expected = photocraft_io::raw::develop_with_settings("camera.dng", &photo.bytes, &json!({})).unwrap().document;
    for policy in [Policy::Preserve, Policy::Convert, Policy::Off] {
        let mut session = Session::new();
        session.color.settings.policy_rgb = policy;
        session.color.settings.working_rgb = "srgb".into();
        session.color.settings.ask_on_mismatch = true;
        let result = session.execute(OPEN_JOB, photo.params()).unwrap();
        let actual = &session.active().unwrap().doc;
        assert_eq!(actual.icc_profile, expected.icc_profile);
        assert_eq!(actual.layers[0].surface().unwrap().read_region(actual.bounds()), expected.layers[0].surface().unwrap().read_region(expected.bounds()));
        assert_eq!(result["color"]["ask"], false);
        assert_eq!(result["color"]["action"], "kept");
        let mut external = Session::new();
        external.color.settings = session.color.settings.clone();
        let (_, report) = external.open_document(expected.clone(), None);
        assert_eq!(
            report["action"],
            match policy {
                Policy::Preserve => "kept",
                Policy::Convert => "converted",
                Policy::Off => "discarded",
            }
        );
        if policy == Policy::Convert {
            assert!(document_profile(&external.active().unwrap().doc).same_colors(photocraft_cms::Builtin::Srgb.profile()));
        }
    }
    let mut session = Session::new();
    let mut params = photo.params();
    params["askColorProfile"] = json!(true);
    let pending = job(&mut session, OPEN_JOB, params);
    let result = session.wait_job(pending).unwrap();
    assert_eq!(result["color"]["ask"], true);
    assert_eq!(session.active().unwrap().doc.icc_profile, expected.icc_profile);
}

#[test]
fn internal_develop_profile_prompt_request_supersedes_non_prompt_open() {
    let photo = Photo::new("profile-latest");
    let mut session = Session::new();
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let first = job(&mut session, OPEN_JOB, photo.params());
    let mut params = photo.params();
    params["askColorProfile"] = json!(true);
    let next = job(&mut session, OPEN_JOB, params);
    assert_ne!(first, next);
    gate.store(true, Ordering::Relaxed);
    let result = session.wait_job(next).unwrap();
    assert_eq!(result["color"]["ask"], true);
    let _ = session.wait_job(first);
    assert_eq!(session.documents().len(), 1);
}

#[test]
fn overwrite_preservation_keeps_sources_history_snapshots_and_library_baseline() {
    let path = std::env::temp_dir().join(format!("overwrite-preserved-{}.png", std::process::id()));
    image::RgbImage::from_fn(19, 13, |x, y| image::Rgb([(x * 7 + 30) as u8, (y * 11 + 20) as u8, 90])).save(&path).unwrap();
    let original = Arc::new(std::fs::read(&path).unwrap());
    let source = path.to_string_lossy().into_owned();
    let mut session = Session::new();
    let settings = json!({"light":{"exposure":0.5}});
    session.execute(OPEN_JOB, json!({"library":"catalog-a","photo":71,"path":source,"settings":settings})).unwrap();
    let state = session.active_mut().unwrap();
    let LayerContent::Smart(smart) = &mut Arc::make_mut(&mut state.doc).layers[0].content else { panic!("Develop source") };
    smart.develop.as_mut().unwrap().source = None; // Supported legacy path-backed link.
    session.execute("layer.new.layer", json!({"name":"Retouch"})).unwrap();
    session.execute("edit.fill", json!({"color":"#d04020"})).unwrap();
    session.execute("history.newSnapshot", json!({"name":"Keep retouch"})).unwrap();
    session.execute("edit.fill", json!({"color":"#3050a0"})).unwrap();
    assert!(session.undo());
    let old = session.active().unwrap().clone();
    let old_base = old.doc.layers[0].surface().unwrap().clone();
    let dropped = session.prepare_source_overwrite(&source, original.clone()).unwrap();
    drop(dropped); // A failed/cancelled native commit does not publish this plan.
    assert!(Arc::ptr_eq(&session.active().unwrap().doc, &old.doc));
    let prepared = session.prepare_source_overwrite(&source, original.clone()).unwrap();
    image::RgbImage::from_pixel(19, 13, image::Rgb([20u8, 200, 60])).save(&path).unwrap();
    let replacement = Arc::new(std::fs::read(&path).unwrap());
    prepared.publish(&mut session);
    session.acknowledge_library_overwrite("catalog-a", 71, &source, &json!({}));
    let state = session.active().unwrap();
    assert_eq!((state.revision, state.saved_revision, state.is_dirty()), (old.revision, old.saved_revision, old.is_dirty()));
    assert_eq!(state.history.entries(), old.history.entries());
    assert_eq!(state.history.redo_labels().collect::<Vec<_>>(), old.history.redo_labels().collect::<Vec<_>>());
    assert_eq!(state.doc.layers.iter().map(|layer| layer.id).collect::<Vec<_>>(), old.doc.layers.iter().map(|layer| layer.id).collect::<Vec<_>>());
    assert_eq!(state.doc.layers[1], old.doc.layers[1]);
    let verify = |doc: &Document| {
        let LayerContent::Smart(smart) = &doc.layers[0].content else { panic!("Develop source") };
        assert!(matches!(&smart.source,SmartSource::Embedded{bytes,..} if bytes.as_ref()==original.as_ref()));
        let link = smart.develop.as_ref().unwrap();
        assert_eq!(link.settings, settings);
        assert_eq!(link.library_baseline.as_deref(), Some(&json!({})));
        assert_eq!(source_of(smart), Some(source.as_str()));
        let rendered = crate::smart_cmds::render(doc, smart).unwrap().unwrap();
        assert_eq!(rendered.read_region(doc.bounds()), old_base.read_region(doc.bounds()));
    };
    verify(&state.doc);
    let mut history = state.history.clone();
    history.map_documents(|doc| {
        verify(doc);
        doc.clone()
    });
    for snapshot in &state.snapshots {
        verify(&snapshot.doc);
    }
    // Qualified acknowledgement cannot follow an equal photo ID in another catalog.
    session.acknowledge_library_overwrite("catalog-b", 71, &source, &json!({"wrong":true}));
    verify(&session.active().unwrap().doc);
    for extension in ["pcraft", "psd", "tif"] {
        let name = format!("preserved.{extension}");
        let options = photocraft_io::ExportOptions { tiff_layers: true, ..Default::default() };
        let saved = crate::vector_integration::export_document(&session.active().unwrap().doc, &name, &options).unwrap().bytes;
        let reopened = photocraft_io::import(&name, &saved).unwrap().document;
        let LayerContent::Smart(smart) = &reopened.layers[0].content else { panic!("persisted Develop source") };
        let link = smart.develop.as_ref().unwrap();
        assert_eq!(link.library_baseline.as_deref(), Some(&json!({})), "{extension}");
        assert_eq!(link.settings, settings);
        let (_, bytes) = crate::smart_cmds::source_bytes(&reopened.metadata, &smart.source).unwrap();
        assert_eq!(bytes.as_ref(), original.as_ref(), "{extension}");
    }
    let again = session.prepare_source_overwrite(&source, replacement.clone()).unwrap();
    again.publish(&mut session);
    verify(&session.active().unwrap().doc); // Later overwrites don't rewrite already frozen bytes.
    let count = session.active().unwrap().doc.layers.len();
    let layer = session.active().unwrap().doc.layers[0].id;
    session.execute("layer.develop.set", json!({"layer":layer.0,"settings":{"light":{"exposure":0.75}}})).unwrap();
    let LayerContent::Smart(adjusted) = &session.active().unwrap().doc.layers[0].content else { panic!("adjusted source") };
    assert_eq!(adjusted.develop.as_ref().unwrap().library_baseline.as_deref(), Some(&json!({})));
    assert!(matches!(&adjusted.source,SmartSource::Embedded{bytes,..} if bytes.as_ref()==original.as_ref()));
    let ack = session.execute("develop.syncPhoto", json!({"library":"catalog-a","photo":71,"path":source,"settings":{}})).unwrap();
    assert_eq!(ack["updated"], 0);
    assert_eq!(session.active().unwrap().doc.layers.len(), count);
    session.execute("develop.syncPhoto", json!({"library":"catalog-a","photo":71,"path":source,"settings":{"light":{"exposure":0.25}}})).unwrap();
    let doc = &session.active().unwrap().doc;
    assert_eq!(doc.layers.len(), count + 1);
    let LayerContent::Smart(new) = &doc.layers.last().unwrap().content else { panic!("new Develop layer") };
    assert!(matches!(&new.source,SmartSource::Linked{path} if path==&source));
    assert!(new.develop.as_ref().unwrap().library_baseline.is_none());
    let (_, bytes) = crate::smart_cmds::source_bytes(&doc.metadata, &new.source).unwrap();
    assert_eq!(bytes.as_ref(), replacement.as_ref());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn overwrite_preservation_rejects_source_work_before_preparing_state() {
    let photo = Photo::new("overwrite-busy");
    let mut session = photo.session();
    let before = session.active().unwrap().doc.clone();
    let gate = Arc::new(AtomicBool::new(false));
    gate_next_work(gate.clone());
    let pending = job(&mut session, "develop.syncPhoto", sync_params(0.75));
    assert!(session.prepare_source_overwrite(&photo.path.to_string_lossy(), Arc::new(photo.bytes.clone())).is_err());
    assert!(Arc::ptr_eq(&session.active().unwrap().doc, &before));
    session.cancel_job(pending);
    gate.store(true, Ordering::Relaxed);
    session.join_cancelled_jobs();
}

#[test]
fn overwrite_preservation_clean_saved_project_requires_resave_of_frozen_source() {
    let photo = Photo::new("overwrite-clean-saved");
    let mut session = photo.session();
    let saved = photocraft_format::save_to_bytes(&session.active().unwrap().doc, &Default::default()).unwrap();
    let old_native = photocraft_format::load_from_bytes(&saved).unwrap();
    assert!(matches!(&old_native.layers[0].content,LayerContent::Smart(smart) if matches!(&smart.source,SmartSource::Linked{..})));
    let state = session.active_mut().unwrap();
    state.path = Some("saved-original.pcraft".into());
    state.saved_revision = state.revision;
    let revision = state.revision;
    let history = state.history.entries();
    assert!(!state.is_dirty());
    let prepared = session.prepare_source_overwrite(&photo.path.to_string_lossy(), Arc::new(photo.bytes.clone())).unwrap();
    std::fs::write(&photo.path, b"replacement file").unwrap();
    prepared.publish(&mut session);
    let state = session.active().unwrap();
    assert!(state.is_dirty(), "closing needs a save prompt for the newly embedded source");
    assert_eq!(state.revision, revision);
    assert_eq!(state.history.entries(), history);
    assert_eq!(state.path.as_deref(), Some("saved-original.pcraft"));
    let saved = photocraft_format::save_to_bytes(&state.doc, &Default::default()).unwrap();
    let reopened = photocraft_format::load_from_bytes(&saved).unwrap();
    let LayerContent::Smart(smart) = &reopened.layers[0].content else { panic!("saved source") };
    let (_, bytes) = crate::smart_cmds::source_bytes(&reopened.metadata, &smart.source).unwrap();
    assert_eq!(bytes.as_ref(), &photo.bytes);
    assert!(crate::smart_cmds::render(&reopened, smart).unwrap().is_some(), "cold refresh uses original embedded bytes");
}
