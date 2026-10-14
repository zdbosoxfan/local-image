//! Drive the Library's real widgets, queues and worker channels with deterministic MockFaces.
use crate::{
    LightcraftApp, Services,
    headless::Headless,
    state::{Dialog, ViewMode},
};
use lightcraft_catalog::{Op, Photo, PhotoId, Source};
use lightcraft_engine::{
    Session,
    smart_sort::{FaceTagger, mock::MockTagger, people::MockFaces},
};
use lightcraft_raster::Rgba8;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
const T: Duration = Duration::from_secs(30);
fn image(colors: &[[u8; 3]]) -> Rgba8 {
    let mut image = Rgba8::filled(16 * colors.len().max(1), 16, [0, 0, 0, 255]);
    for (i, c) in colors.iter().enumerate() {
        for y in 0..8 {
            for x in 0..8 {
                image.set(i * 16 + x, y, [255; 4]);
                image.set(i * 16 + x + 8, y, [c[0], c[1], c[2], 255]);
            }
        }
    }
    image
}
fn add(s: &mut Session, colors: &[[u8; 3]], store: bool) -> PhotoId {
    let id = s.catalog.alloc_photo_id();
    let mut p = Photo::new(id, Source::Demo { scene: 1 }, &format!("{id:?}.jpg"), "JPEG", 64, 40, "2026-10-09");
    p.content_hash = Some(format!("people-ui-{}", id.0));
    let key = lightcraft_engine::media::content_key(&p);
    s.commit("fixture", Op::AddPhoto { photo: Box::new(p) }).unwrap();
    if store {
        s.smart.people.store.insert("mock-faces", key, MockFaces.faces(&image(colors)).unwrap()).unwrap();
    }
    id
}
fn session() -> Session {
    let mut s = Session::new();
    s.smart.people.tagger = Some(Arc::new(MockFaces));
    s.smart.tagger = Some(Arc::new(MockTagger));
    s.smart.prefs.faces_enabled = true;
    s.smart.people.ready().unwrap();
    s
}
fn harness(s: Session) -> Headless {
    headless(LightcraftApp::new(s, Services::default()))
}
fn headless(app: LightcraftApp) -> Headless {
    let mut h = Headless::new(app, [1440., 1100.], 1.);
    h.app.ui.left_panel = false;
    h.app.ui.right = crate::state::RightPanel::None;
    h.app.ui.filmstrip = false;
    h.app.ui.filter_bar = true;
    h.app.ui.view = ViewMode::People;
    h.step();
    h.step();
    h
}
fn widget(h: &Headless, id: &str) -> egui::Rect {
    h.app
        .widgets
        .iter()
        .rev()
        .find(|(name, _)| name == id)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("missing {id}; widgets {:?}", h.app.widgets.iter().map(|(n, _)| n).collect::<Vec<_>>()))
}
fn reveal_widget(h: &mut Headless, viewport: &str, id: &str) {
    for _ in 0..12 {
        let body = widget(h, viewport).shrink(12.);
        let target = widget(h, id);
        if body.contains(target.center()) {
            return;
        }
        h.request("ui.hoverWidget", json!({"id":viewport}), T);
        h.request("ui.scroll", json!({"dy":body.center().y-target.center().y}), T);
        for _ in 0..8 {
            h.step();
        }
    }
    panic!("{id} remains outside the scroll viewport");
}
fn click(h: &mut Headless, id: &str) {
    let r = h.request("ui.clickWidget", json!({"id":id}), T);
    assert_eq!(r["ok"], true, "{id}: {r}");
    h.step();
    h.step();
}
fn key(h: &mut Headless, k: &str, cmd: bool) {
    let r = h.request("ui.key", json!({"key":k,"cmd":cmd}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.step();
    h.step();
}
fn text(h: &mut Headless, id: &str, value: &str) {
    click(h, id);
    key(h, "a", true);
    h.request("ui.text", json!({"text":value}), T);
    h.step();
}
fn named(s: &mut Session, id: PhotoId, name: &str) -> u64 {
    s.execute("smartSort.findPerson", &json!({"name":name,"faces":[{"photo":id,"face":0}]})).unwrap()["person"].as_u64().unwrap()
}
fn list(s: &mut Session) -> Value {
    s.execute("smartSort.people", &json!({"showEveryone":true})).unwrap()
}
#[test]
fn ds_finding_large_people_query_keeps_ui_cancellable() {
    let mut s = session();
    for _ in 0..5000 {
        add(&mut s, &[[255, 0, 0]], true);
    }
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let mut app = LightcraftApp::new(s, Services::default());
    app.ui.people.query_test_gate = Some(Arc::new(crate::panels::people_workspace::QueryTestGate {
        entered: entered_tx,
        resume: std::sync::Mutex::new(resume_rx),
        completed: completed_tx,
    }));
    let mut h = headless(app);
    // These are deadlock watchdogs for channel delivery under saturated CPUs,
    // not frame budgets. The worker stays blocked until after the actual click.
    let watchdog = Duration::from_secs(120);
    assert_ne!(entered_rx.recv_timeout(watchdog).unwrap(), std::thread::current().id(), "clustering ran on the UI thread");
    assert!(h.app.session.smart.people.data.people.is_empty());
    assert!(h.app.widgets.iter().any(|(id, _)| id == "people:groupCancel"));
    click(&mut h, "people:groupCancel");
    assert!(h.app.session.smart.people.data.people.is_empty());
    assert!(!h.app.widgets.iter().any(|(id, _)| id == "people:groupCancel"));
    resume_tx.send(()).unwrap();
    assert!(completed_rx.recv_timeout(watchdog).unwrap(), "worker must observe the UI cancellation");
    h.step();
    assert!(h.app.session.smart.people.data.people.is_empty(), "cancelled query must not publish people");
}

#[test]
fn people_ui_threshold_names_ignore_restore_pin_and_grid_filter() {
    let mut s = session();
    let a = add(&mut s, &[[255, 0, 0]], true);
    let b = add(&mut s, &[[255, 0, 0]], true);
    let c = add(&mut s, &[[0, 255, 0]], true);
    add(&mut s, &[[0, 255, 0]], true);
    add(&mut s, &[[0, 255, 0]], true);
    let people = list(&mut s);
    let red = people["people"].as_array().unwrap().iter().find(|p| p["count"] == 2).unwrap()["id"].as_u64().unwrap();
    let green = people["people"].as_array().unwrap().iter().find(|p| p["count"] == 3).unwrap()["id"].as_u64().unwrap();
    let mut h = harness(s);
    assert!(!h.app.widgets.iter().any(|(id, _)| id == &format!("smartSort:bubble:{red}")));
    click(&mut h, "smartSort:showEveryone");
    widget(&h, &format!("smartSort:bubble:{red}"));
    click(&mut h, "smartSort:pasteNames");
    text(&mut h, "smartSort:namesText", "Jane Doe,Speaker\nJohn Roe,Guest");
    click(&mut h, "smartSort:namesApply");
    text(&mut h, &format!("smartSort:bubbleName:{red}"), "Jan");
    click(&mut h, &format!("smartSort:nameSuggestion:{red}:Jane Doe"));
    assert_eq!(h.app.session.smart.people.data.person(red).unwrap().name, "Jane Doe");
    assert!(!h.app.session.smart.people.data.person(red).unwrap().folder_enabled);
    click(&mut h, &format!("smartSort:pin:{red}"));
    assert!(h.app.session.smart.people.data.person(red).unwrap().pinned);
    assert!(widget(&h, &format!("smartSort:bubble:{red}")).left() < widget(&h, &format!("smartSort:bubble:{green}")).left());
    click(&mut h, &format!("smartSort:ignore:{red}"));
    assert!(!h.app.widgets.iter().any(|(id, _)| id == &format!("smartSort:bubble:{red}")));
    // Ignored identities disappear from the Library's folder/filter picker.
    h.app.ui.view = ViewMode::PhotoGrid;
    h.step();
    click(&mut h, "filter:person");
    assert!(!h.app.widgets.iter().any(|(id, _)| id == &format!("filter:person:{red}")));
    assert!(!h.app.widgets.iter().any(|(id, _)| id == "filter:personName:Jane Doe"));
    h.request("ui.click", json!({"x":10,"y":10}), T);
    h.app.ui.view = ViewMode::People;
    h.step();
    // Expand the Ignored list using the real egui disclosure widget.
    let r = h.app.widgets.iter().find(|(id, _)| id == "smartSort:ignored").map(|(_, r)| *r);
    if let Some(r) = r {
        h.request("ui.click", json!({"x":r.center().x,"y":r.center().y}), T);
        h.step();
    }
    click(&mut h, &format!("smartSort:restore:{red}"));
    click(&mut h, &format!("smartSort:bubble:{red}"));
    assert_eq!(h.app.ui.view, ViewMode::PhotoGrid);
    let mut visible = h.app.session.visible_cloned();
    visible.sort();
    assert_eq!(visible, vec![a, b]);
    assert!(!h.app.session.visible_cloned().contains(&c));
    click(&mut h, "button:filterClear");
    assert_eq!(h.app.session.visible_cloned().len(), 5);
}
#[test]
fn people_ui_confirm_queue_keys_rejection_skip_undo_live_ranking() {
    let mut s = session();
    let a = add(&mut s, &[[255, 0, 0]], true);
    let b = add(&mut s, &[[140, 210, 0]], true);
    let c = add(&mut s, &[[150, 200, 0]], true);
    let person = named(&mut s, a, "Jane");
    let mut h = harness(s);
    click(&mut h, &format!("smartSort:review:{person}"));
    assert!(h.app.ui.people.queue.is_some());
    assert!(h.app.session.catalog.photo(b).unwrap().meta.person_ids.is_empty());
    key(&mut h, "n", false);
    assert_eq!(h.app.session.smart.people.data.person(person).unwrap().rejected_faces.len(), 1);
    key(&mut h, "s", false);
    assert_eq!(h.app.ui.people.queue.as_ref().unwrap().skipped.len(), 1);
    key(&mut h, "z", true);
    assert!(h.app.ui.people.queue.as_ref().unwrap().skipped.is_empty());
    key(&mut h, "y", false);
    assert_eq!(h.app.session.smart.people.data.person(person).unwrap().confirmed_faces.len(), 2);
    let matches = h.app.session.execute("smartSort.findPerson", &json!({"person":person,"queue":true})).unwrap();
    assert!(matches["matches"].as_array().unwrap().is_empty());
    assert!(h.app.session.catalog.photo(c).unwrap().meta.person_ids.contains(&person));
    key(&mut h, "z", true);
    assert_eq!(h.app.session.smart.people.data.person(person).unwrap().confirmed_faces.len(), 1);
    click(&mut h, "smartSort:createPersonFolder");
    assert!(h.app.session.smart.people.data.person(person).unwrap().folder_enabled);
}
#[test]
fn people_ui_merge_drag_prompt_and_split_are_undoable() {
    let mut s = session();
    let a = add(&mut s, &[[255, 0, 0]], true);
    let b = add(&mut s, &[[0, 255, 0]], true);
    let first = named(&mut s, a, "Jane");
    let second = named(&mut s, b, "John");
    s.execute("library.filter", &json!({"personIds":[second]})).unwrap();
    let mut h = harness(s);
    click(&mut h, "smartSort:showEveryone");
    let target = widget(&h, &format!("smartSort:bubble:{first}")).center();
    let r = h.request("ui.dragWidget", json!({"id":format!("smartSort:bubble:{second}"),"toX":target.x,"toY":target.y}), T);
    assert_eq!(r["ok"], true);
    h.step();
    h.step();
    click(&mut h, "smartSort:mergeSame");
    assert_eq!(h.app.session.smart.people.data.people.len(), 1);
    assert_eq!(h.app.session.visible_cloned().len(), 2);
    // A stored folder's former identity opens review of the merged identity.
    crate::panels::people_workspace::review_person(&mut h.app, second);
    h.step();
    h.step();
    assert_eq!(h.app.ui.people.queue.as_ref().unwrap().person, first);
    assert!(h.app.ui.people.error.is_empty());
    click(&mut h, "smartSort:peopleClose");
    click(&mut h, "smartSort:peopleUndo");
    assert_eq!(h.app.session.smart.people.data.people.len(), 2);
    assert_eq!(h.app.session.visible_cloned(), vec![b]);
    click(&mut h, &format!("smartSort:split:{first}"));
    click(&mut h, "smartSort:splitFace");
    assert_eq!(h.app.session.smart.people.data.people.len(), 3);
    click(&mut h, "smartSort:reviewUndo");
    assert_eq!(h.app.session.smart.people.data.people.len(), 2);
}
#[test]
fn people_ui_finder_face_click_name_find_create_folder_and_everyone_filter() {
    let mut s = session();
    let a = add(&mut s, &[[255, 0, 0]], true);
    let b = add(&mut s, &[[0, 255, 0]], true);
    let both = add(&mut s, &[[255, 0, 0], [0, 255, 0]], true);
    let jane = named(&mut s, a, "Jane");
    let john = named(&mut s, b, "John");
    let mut h = harness(s);
    h.app.ui.view = ViewMode::Detail;
    h.app.session.execute("library.select", &json!({"ids":[both]})).unwrap();
    h.step();
    let image = widget(&h, "canvas:image");
    let canvas = image.min + image.size() * 0.8;
    h.request("ui.click", json!({"x":canvas.x,"y":canvas.y,"button":"right"}), T);
    h.step();
    h.step();
    click(&mut h, "photo:findPerson");
    click(&mut h, &format!("smartSort:face:{}:0", both.0));
    text(&mut h, "smartSort:findName", "Jane");
    click(&mut h, "smartSort:findPhotos");
    click(&mut h, "smartSort:createPersonFolder");
    click(&mut h, "smartSort:peopleClose");
    assert_eq!(h.app.session.filter.person_ids, vec![jane]);
    assert!(h.app.session.smart.people.data.person(jane).unwrap().folder_enabled);
    click(&mut h, "filter:person");
    click(&mut h, &format!("filter:person:{john}"));
    assert_eq!(h.app.session.visible_cloned().len(), 3);
    click(&mut h, "filter:peopleEveryone");
    assert_eq!(h.app.session.visible_cloned(), vec![both]);
    h.request("ui.click", json!({"x":10,"y":10}), T);
    h.step();
    click(&mut h, "button:filterSave");
    assert!(h.app.ui.dialog.is_some());
}
#[test]
fn people_ui_faces_opt_in_analysis_and_person_folder_export_picker() {
    let mut s = session();
    let a = add(&mut s, &[[255, 0, 0]], true);
    add(&mut s, &[[255, 0, 0]], true);
    add(&mut s, &[[255, 0, 0]], true);
    let b = add(&mut s, &[[0, 255, 0]], true);
    let both = add(&mut s, &[[255, 0, 0], [0, 255, 0]], true);
    let jane = named(&mut s, a, "Jane");
    let john = named(&mut s, b, "John");
    s.smart.prefs.faces_enabled = false;
    let ids: Vec<_> = s.catalog.photos().map(|p| p.id).collect();
    s.execute("library.select", &json!({"ids":ids})).unwrap();
    let mut h = harness(s);
    crate::panels::smart_sort::open(&mut h.app, true);
    h.step();
    h.step();
    assert!(!h.app.session.smart.prefs.faces_enabled);
    click(&mut h, "smartSort:faces");
    assert!(h.app.session.smart.prefs.faces_enabled);
    click(&mut h, "smartSort:analyze");
    assert!(h.step_until(T, |h| h.app.smart_sort.is_none() && h.app.people_task.is_none()));
    h.step();
    click(&mut h, "smartSort:peopleSection");
    text(&mut h, &format!("smartSort:bubbleName:{jane}"), "Jane Doe");
    key(&mut h, "Enter", false);
    click(&mut h, &format!("smartSort:personFolder:{jane}"));
    click(&mut h, "smartSort:next");
    h.step();
    let Some(Dialog::SmartSort { state }) = &h.app.ui.dialog else { panic!() };
    assert!(state.preset.folders.iter().any(|f| f.person_ids == [jane]));
    assert!(!state.include_names);
    // Compact fixture layout leaves the actual custom-folder picker visible.
    let Some(Dialog::SmartSort { state }) = &mut h.app.ui.dialog else { panic!() };
    state.preset.folders.retain(|f| f.person_ids == [jane]);
    state.people_folder = Some(0);
    h.step();
    h.step();
    h.app.ui.people.show_everyone = true;
    h.app.ui.people.refresh = true;
    h.step();
    h.step();
    // A second folder with the same name has its own membership and bubble counts.
    let Some(Dialog::SmartSort { state }) = &mut h.app.ui.dialog else { panic!() };
    let mut other = state.preset.folders[0].clone();
    other.person_ids = vec![john];
    state.preset.folders.push(other);
    state.people_folder = Some(1);
    h.step();
    h.step();
    let jane_row = h.app.ui.people.list["people"].as_array().unwrap().iter().find(|r| r["id"] == jane).unwrap();
    assert_eq!(jane_row["folderCount"], 1);
    let Some(Dialog::SmartSort { state }) = &mut h.app.ui.dialog else { panic!() };
    state.preset.folders.pop();
    state.people_folder = Some(0);
    h.step();
    h.step();
    let john_row = h.app.ui.people.list["people"].as_array().unwrap().iter().find(|r| r["id"] == john).unwrap();
    assert_eq!(john_row["folderCount"], 1);
    assert_eq!(john_row["count"], 2);
    reveal_widget(&mut h, "smartSort:body", &format!("smartSort:folderBubble:{john}"));
    click(&mut h, &format!("smartSort:folderBubble:{john}"));
    reveal_widget(&mut h, "smartSort:body", "smartSort:folderEveryone:0");
    click(&mut h, "smartSort:folderEveryone:0");
    let Some(Dialog::SmartSort { state }) = &h.app.ui.dialog else { panic!() };
    assert_eq!(state.preset.folders[0].person_ids.len(), 2, "{:?}", state.preset.folders[0]);
    assert!(state.preset.folders[0].everyone, "{:?}; rect {:?}", state.preset.folders[0], widget(&h, "smartSort:folderEveryone:0"));
    let result = h.app.session.execute("smartSort.plan", &json!({"sortPreset":state.preset,"ids":ids})).unwrap();
    assert_eq!(result["folders"][0]["ids"], json!([both]));
    click(&mut h, "smartSort:includeNames");
    let Some(Dialog::SmartSort { state }) = &h.app.ui.dialog else { panic!() };
    assert!(state.include_names);
}
#[test]
fn people_ui_headshots_seed_paste_and_clear_without_models() {
    let mut s = session();
    add(&mut s, &[[255, 0, 0]], true);
    add(&mut s, &[[0, 255, 0]], true);
    add(&mut s, &[[0, 0, 255]], true);
    let dir = std::env::temp_dir().join(format!("people-ui-headshots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, c) in [("Jane", [255, 0, 0]), ("John", [0, 255, 0]), ("Ana", [0, 0, 255])] {
        let img = image(&[c]);
        let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &Default::default()).unwrap();
        std::fs::write(dir.join(format!("{name}.png")), bytes).unwrap();
    }
    let mut h = harness(s);
    h.app.services.pick_folder = Some(Box::new({
        let dir = dir.clone();
        move || Some(dir.to_string_lossy().into_owned())
    }));
    click(&mut h, "smartSort:seedHeadshots");
    assert!(h.step_until(T, |h| h.app.people_task.is_none()));
    h.step();
    assert_eq!(h.app.session.smart.people.data.people.iter().filter(|p| !p.seed_faces.is_empty()).count(), 3);
    for person in h.app.session.smart.people.data.people.iter().filter(|p| !p.seed_faces.is_empty()) {
        widget(&h, &format!("smartSort:bubble:{}", person.id));
        let rows = lightcraft_engine::smart_sort::people::matches(
            &h.app.session.catalog,
            &h.app.session.smart.people,
            &h.app.session.smart.people.data,
            person.id,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].similarity, 1.);
    }
    h.app.session.smart.people.tagger = None;
    h.app.session.smart.prefs.faces_enabled = false;
    h.app.ui.dialog = Some(Dialog::Settings { tab: "general".into() });
    h.step();
    h.step();
    click(&mut h, "button:settingsClearFaces");
    click(&mut h, "check:removeFaceNames");
    click(&mut h, "smartSort:clearFaceData");
    assert!(h.app.session.smart.people.data.people.is_empty());
    assert!(h.app.session.smart.people.tagger.is_none());
    assert!(h.app.session.catalog.photos().all(|p| p.meta.regions.iter().all(|r| !r.auto)));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn people_ui_analysis_worker_cache_cancel_and_opt_out() {
    let mut s = session();
    let mut ids = Vec::new();
    for n in 0..3 {
        let id = s.catalog.alloc_photo_id();
        let mut p = Photo::new(id, Source::File { path: format!("/fixture/{n}.arw") }, "fixture.arw", "RAW", 16, 16, "2026-10-09");
        p.kind = lightcraft_catalog::MediaKind::Raw;
        p.develop = Arc::new(p.camera_defaults());
        p.content_hash = Some(format!("worker-{n}"));
        s.commit("fixture", Op::AddPhoto { photo: Box::new(p) }).unwrap();
        ids.push(id);
    }
    s.media.preview_loader = Some(Arc::new(|_, _| Some(image(&[[255, 0, 0]]))));
    let mut h = harness(s);
    crate::people_task::start(&mut h.app, &ids, None).unwrap();
    assert!(h.step_until(T, |h| h.app.people_task.is_none()));
    assert_eq!(h.app.session.smart.people.store.len("mock-faces"), 3);
    crate::people_task::start(&mut h.app, &ids, None).unwrap();
    assert_eq!(h.app.people_task.as_ref().unwrap().progress.lock().unwrap().1, 0);
    assert!(h.step_until(T, |h| h.app.people_task.is_none()));
    crate::people_task::start(&mut h.app, &ids, None).unwrap();
    h.app.people_task.as_ref().unwrap().cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(h.step_until(T, |h| h.app.people_task.is_none()));
    h.app.session.execute("smartSort.facesEnable", &json!({"enabled":false})).unwrap();
    assert!(crate::people_task::start(&mut h.app, &ids, None).is_err());
    assert!(h.app.session.smart.people.tagger.is_none());
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
#[ignore = "requires the owner's real Vulkan GPU; exercises the complete app without opening windows"]
fn people_ui_real_gpu_interactions() {
    use std::time::Instant;
    lightcraft_engine::gpu::backend::set_init_marker(None);
    assert!(lightcraft_engine::gpu::available(), "{}", lightcraft_engine::gpu::unavailable_reason().unwrap_or_default());
    let adapter = lightcraft_engine::gpu::adapter_name().unwrap();
    assert!(!adapter.to_lowercase().contains("llvmpipe"));
    let mut s = session();
    let first = add(&mut s, &[[255, 0, 0]], true);
    for _ in 0..1999 {
        add(&mut s, &[[255, 0, 0]], true);
    }
    let person = named(&mut s, first, "Jane");
    let mut h = harness(s);
    assert!(h.settle(T));
    let start = Instant::now();
    for _ in 0..30 {
        h.step();
    }
    let frame_ms = start.elapsed().as_secs_f64() * 1000. / 30.;
    let start = Instant::now();
    click(&mut h, &format!("smartSort:review:{person}"));
    let queue_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    key(&mut h, "y", false);
    let confirm_ms = start.elapsed().as_secs_f64() * 1000.;
    println!("{adapter}: 2,000-photo People view {frame_ms:.2} ms/frame; open queue {queue_ms:.2} ms; Yes and live re-ranking {confirm_ms:.2} ms");
    assert_eq!(h.app.session.smart.people.data.person(person).unwrap().confirmed_faces.len(), 2);
}

#[test]
fn people_ui_missing_model_download_progress_cancel_without_network() {
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };
    let mut s = Session::new();
    s.smart.tagger = Some(Arc::new(MockTagger));
    let mut h = harness(s);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(false));
    h.app.services.download_models = Some(Box::new({
        let calls = calls.clone();
        let running = running.clone();
        move |id, cancel| {
            calls.lock().unwrap().push((id.to_string(), cancel));
            running.store(!cancel, Ordering::Relaxed);
            Ok(())
        }
    }));
    h.app.services.model_download_status = Some(Box::new({
        let running = running.clone();
        move |_| crate::ModelDownloadStatus { running: running.load(Ordering::Relaxed), done: 10, total: 100, error: None }
    }));
    h.step();
    click(&mut h, "smartSort:facesDownload");
    assert!(!h.app.session.smart.prefs.faces_enabled);
    click(&mut h, "smartSort:facesDownloadCancel");
    assert_eq!(*calls.lock().unwrap(), vec![("yunet".to_string(), false), ("yunet".to_string(), true)]);
}

#[test]
fn people_ui_probable_merge_prompt_different_and_queue_merge_undo() {
    let mut s = session();
    let a = add(&mut s, &[[255, 0, 0]], true);
    let b = add(&mut s, &[[255, 0, 0]], true);
    let first = named(&mut s, a, "Jane");
    let mut data = s.smart.people.data.clone();
    let second = data.add("Jane again".into()).unwrap();
    let face_key = lightcraft_engine::smart_sort::people::FaceKey(lightcraft_engine::media::content_key(s.catalog.photo(b).unwrap()), 0);
    data.person_mut(second).unwrap().confirmed_faces.push(face_key);
    lightcraft_engine::smart_sort::people::update_centroids(&mut data, &s.smart.people);
    s.commit_people("fixture", data).unwrap();
    let mut h = harness(s);
    click(&mut h, "smartSort:showEveryone");
    click(&mut h, &format!("smartSort:review:{first}"));
    assert_eq!(h.app.ui.people.merge, Some((first, second)));
    click(&mut h, "smartSort:mergeDifferent");
    assert!(h.app.ui.people.merge.is_none());
    click(&mut h, "smartSort:peopleClose");
    click(&mut h, &format!("smartSort:review:{first}"));
    assert!(h.app.ui.people.merge.is_none());
    click(&mut h, "smartSort:peopleClose");
    click(&mut h, "smartSort:peopleUndo");
    click(&mut h, &format!("smartSort:review:{first}"));
    click(&mut h, "smartSort:mergeSame");
    assert_eq!(h.app.session.smart.people.data.people.len(), 1);
    key(&mut h, "z", true);
    assert_eq!(h.app.session.smart.people.data.people.len(), 2);
}

#[test]
fn people_ui_export_person_folder_names_opt_in_and_live_smart_album() {
    use lightcraft_engine::smart_sort::{FolderDef, SortPreset};
    use std::sync::Mutex;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/people-ui-export");
    std::fs::create_dir_all(&root).unwrap();
    let img = image(&[[255, 0, 0]]);
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &Default::default()).unwrap();
    let source = root.join("headshot.png");
    std::fs::write(&source, bytes).unwrap();
    for on in [false, true] {
        let mut s = session();
        let id = s.catalog.alloc_photo_id();
        let mut p = Photo::new(id, Source::File { path: source.to_string_lossy().into_owned() }, "headshot.png", "PNG", 16, 16, "2026-10-09");
        p.content_hash = Some("export-people-fixture".into());
        let k = lightcraft_engine::media::content_key(&p);
        s.commit("fixture", Op::AddPhoto { photo: Box::new(p) }).unwrap();
        s.smart.people.store.insert("mock-faces", k, MockFaces.faces(&img).unwrap()).unwrap();
        let person = named(&mut s, id, "Jane");
        let mut h = harness(s);
        let writes = Arc::new(Mutex::new(Vec::new()));
        h.app.services.write = Some(Box::new({
            let writes = writes.clone();
            move |path, bytes| {
                writes.lock().unwrap().push((path.to_string(), bytes.to_vec()));
                Ok(())
            }
        }));
        h.app.ui.dialog = Some(Dialog::SmartSort {
            state: Box::new(crate::panels::smart_sort::SmartSortDialog {
                step: 2,
                source: vec![id],
                people_folders: true,
                albums: true,
                dest: root.join(format!("out-{on}")).to_string_lossy().into_owned(),
                export_params: json!({"format":"original"}),
                preset: SortPreset {
                    categories: vec![lightcraft_engine::smart_sort::Category {
                        name: "Red".into(),
                        prompts: vec!["red".into()],
                        ..Default::default()
                    }],
                    folders: vec![FolderDef {
                        name: "Jane".into(),
                        custom: true,
                        person_ids: vec![person],
                        people_enabled: true,
                        use_rules: false,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }),
        });
        h.step();
        h.step();
        if on {
            click(&mut h, "smartSort:includeNames");
        }
        click(&mut h, "smartSort:export");
        assert!(h.app.ui.dialog.is_none(), "{:?}", h.app.ui.dialog);
        let outputs = writes.lock().unwrap();
        assert_eq!(outputs.len(), 2);
        let (_, sidecar) = outputs.iter().find(|(p, _)| p.ends_with(".xmp")).unwrap();
        let sidecar = std::str::from_utf8(sidecar).unwrap();
        assert_eq!(sidecar.contains("People|Jane"), on);
        let album_id = h.app.session.catalog.albums().find(|a| a.name == "Jane").unwrap().id;
        assert_eq!(h.app.session.catalog.album_count(album_id), 1);
        // The saved person rule continues to track catalog membership rather than all photos.
        let mut meta = h.app.session.catalog.photo(id).unwrap().meta.clone();
        meta.person_ids.clear();
        h.app.session.commit("remove membership", Op::SetMeta { id, meta: Box::new(meta) }).unwrap();
        assert_eq!(h.app.session.catalog.album_count(album_id), 0);
    }
    std::fs::remove_file(source).unwrap();
}

#[test]
fn people_ui_smart_rule_picker_preserves_identity_sets_and_ignores_hidden_people() {
    use lightcraft_catalog::{Rule, RuleSet};
    let mut s = session();
    let a = add(&mut s, &[[255, 0, 0]], true);
    let b = add(&mut s, &[[0, 255, 0]], true);
    let both = add(&mut s, &[[255, 0, 0], [0, 255, 0]], true);
    let jane = named(&mut s, a, "Jane");
    let john = named(&mut s, b, "John");
    let hidden = s.smart.people.data.add("Ignored stranger".into()).unwrap();
    s.smart.people.data.person_mut(hidden).unwrap().ignored = true;
    let mut h = harness(s);
    h.app.ui.view = ViewMode::PhotoGrid;
    h.app.ui.dialog = Some(Dialog::SmartRules {
        id: None,
        name: "Together".into(),
        rules: RuleSet { rules: vec![Rule::Field { field: "person".into(), op: "allOf".into(), value: json!([jane, john]) }], ..Default::default() },
    });
    h.step();
    h.step();
    click(&mut h, "rulePerson:rules-0");
    assert!(!h.app.widgets.iter().any(|(id, _)| id == &format!("rulePerson:rules-0:{hidden}")));
    click(&mut h, &format!("rulePerson:rules-0:{jane}"));
    let Some(Dialog::SmartRules { rules, .. }) = &h.app.ui.dialog else { panic!() };
    assert!(rules.matches(h.app.session.catalog.photo(b).unwrap(), &h.app.session.catalog));
    assert!(!rules.matches(h.app.session.catalog.photo(a).unwrap(), &h.app.session.catalog));
    assert!(rules.matches(h.app.session.catalog.photo(both).unwrap(), &h.app.session.catalog));
    click(&mut h, &format!("rulePerson:rules-0:{jane}"));
    h.request("ui.click", json!({"x":10,"y":10}), T);
    h.step();
    click(&mut h, "button:dialogOk");
    let album = h.app.session.catalog.albums().find(|a| a.name == "Together").unwrap();
    assert_eq!(h.app.session.catalog.album_count(album.id), 1);
}

#[test]
fn people_ui_minimum_range_and_top_sixty_show_all() {
    use lightcraft_engine::smart_sort::people::FaceKey;
    let mut s = session();
    let faces = MockFaces.faces(&image(&[[255, 0, 0]])).unwrap();
    for n in 0..62 {
        let key = format!("headshot-seed-{n}");
        s.smart.people.store.insert("mock-faces", key.clone(), faces.clone()).unwrap();
        let id = s.smart.people.data.add(format!("Speaker {n}")).unwrap();
        let person = s.smart.people.data.person_mut(id).unwrap();
        person.seed_faces = vec![FaceKey(key, 0)];
        person.centroid = faces[0].embedding.clone();
    }
    let mut h = harness(s);
    assert_eq!(h.app.ui.people.list["people"].as_array().unwrap().len(), 60);
    assert_eq!(h.app.ui.people.list["remaining"], 2);
    text(&mut h, "smartSort:minFaces", "10");
    key(&mut h, "Enter", false);
    assert_eq!(h.app.session.smart.people.data.min_photos, 10);
    reveal_widget(&mut h, "people:body", "smartSort:showAll");
    click(&mut h, "smartSort:showAll");
    assert_eq!(h.app.ui.people.list["people"].as_array().unwrap().len(), 62);
}

fn ds2_painted_text(h: &Headless) -> String {
    fn text(shape: &egui::Shape, out: &mut String) {
        match shape {
            egui::Shape::Text(t) => {
                out.push_str(t.galley.text());
                out.push('\n');
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    text(s, out)
                }
            }
            _ => {}
        }
    }
    let mut result = String::new();
    for shape in h.view.shapes() {
        text(&shape.shape, &mut result)
    }
    result
}

#[test]
fn ds2_finder_retains_download_error_and_settings_owns_input() {
    let mut s = Session::new();
    let id = add(&mut s, &[], false);
    let mut h = harness(s);
    h.app.services.download_models = Some(Box::new(|_, _| Err("DS download failed".into())));
    h.app.ui.people.finder = Some(id);
    h.step();
    h.step();
    click(&mut h, "smartSort:facesDownload");
    assert_eq!(h.app.ui.people.error, "DS download failed");
    assert!(ds2_painted_text(&h).contains("DS download failed"));
    h.app.ui.dialog = Some(Dialog::Settings { tab: "general".into() });
    h.step();
    h.step();
    assert!(!h.app.widgets.iter().any(|(id, _)| id == "people:overlay"));
    click(&mut h, "button:settingsTab-import");
    assert!(matches!(&h.app.ui.dialog,Some(Dialog::Settings {tab}) if tab=="import"));
    assert_eq!(h.app.ui.people.finder, Some(id));
}

#[test]
fn ds2_clear_face_data_failure_is_displayed_in_settings() {
    let dir = std::env::temp_dir().join(format!("ds2-face-clear-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("AI/faces-blocked.bin")).unwrap();
    let mut s = Session::new();
    s.smart.people = lightcraft_engine::smart_sort::PeopleEngine::new(Some(&dir));
    let mut h = harness(s);
    h.app.ui.dialog = Some(Dialog::Settings { tab: "general".into() });
    h.step();
    h.step();
    // Resolve the new scroll-window's hit testing before the down/up pair.
    h.request("ui.hoverWidget", json!({"id":"button:settingsClearFaces"}), T);
    h.step();
    click(&mut h, "button:settingsClearFaces");
    click(&mut h, "smartSort:clearFaceData");
    assert!(h.app.ui.people.error.contains("could not delete face cache"));
    assert!(ds2_painted_text(&h).contains(&h.app.ui.people.error));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ds2_imported_people_cache_shares_snapshots_and_empty_filter_remains_actionable() {
    let mut s = session();
    add(&mut s, &[], false);
    let id = s.smart.people.data.add("Jane".into()).unwrap();
    s.smart.people.data.person_mut(id).unwrap().ignored = true;
    let mut h = harness(s);
    let (_, a) = h.app.session.visible_shared();
    let (_, b) = h.app.session.visible_shared();
    assert!(Arc::ptr_eq(&a, &b));
    let a = h.app.caches.imported_people(&h.app.session.catalog, &h.app.session.filter, &h.app.session.smart.people.data);
    let b = h.app.caches.imported_people(&h.app.session.catalog, &h.app.session.filter, &h.app.session.smart.people.data);
    assert!(Arc::ptr_eq(&a, &b));
    h.app.session.execute("library.filter", &json!({"personIds":[id]})).unwrap();
    h.step();
    h.step();
    assert!(ds2_painted_text(&h).contains("No named people in these photos"));
    assert!(h.app.widgets.iter().any(|(name, _)| name.starts_with("chip:")), "active filter must still offer a removable chip");
}

#[test]
fn shared_shell_people_name_blur_waits_for_library_input_ownership() {
    let mut s = session();
    let photo = add(&mut s, &[[255, 0, 0]], true);
    let person = named(&mut s, photo, "Original person");
    let mut h = harness(s);
    h.app.ui.people.show_everyone = true;
    h.app.ui.people.refresh = true;
    h.step();
    h.step();
    let field = format!("smartSort:bubbleName:{person}");
    text(&mut h, &field, "Retained person");
    let focus = h.view.ctx.memory(|m| m.focused()).unwrap();
    let before = h.app.session.smart.people.data.clone();
    let undo = h.app.session.undo.len();
    // Exactly one disabled frame, then immediate dismissal. The next frame still
    // carries egui's lost_focus signal, but it is not a genuine Library blur.
    h.app.host_modal = true;
    h.step();
    h.app.host_modal = false;
    h.step();
    assert_eq!(h.app.session.smart.people.data, before, "quick dismissal must not publish delayed focus loss");
    assert_eq!(h.app.session.undo.len(), undo);
    assert_eq!(h.app.ui.people.names.get(&person).map(String::as_str), Some("Retained person"));
    click(&mut h, &field);
    h.app.host_modal = true;
    for input in [egui::Key::Escape, egui::Key::Enter] {
        h.view.ctx.memory_mut(|m| m.request_focus(focus));
        // Bypass the modal-suspended control channel, but use the driver's raw
        // input hook and clock so all frames advance through one time source.
        for pressed in [true, false] {
            assert!(h.app.synthetic.is_empty());
            h.app.synthetic.push(egui::Event::Key { key: input, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE });
            h.step();
            assert!(h.app.synthetic.is_empty(), "the key reached the frame input");
        }
        assert_eq!(h.app.session.smart.people.data, before, "disabled name blur must not publish");
        assert_eq!(h.app.session.undo.len(), undo);
        assert_eq!(h.app.ui.people.names.get(&person).map(String::as_str), Some("Retained person"));
        assert!(!h.app.widgets.iter().any(|(id, _)| id.starts_with("smartSort:nameSuggestion:")));
    }
    h.app.host_modal = false;
    h.step();
    h.step();
    assert_eq!(h.app.ui.people.names.get(&person).map(String::as_str), Some("Retained person"));
    assert_eq!(h.app.session.smart.people.data, before);
    assert_eq!(h.app.session.undo.len(), undo);
    click(&mut h, &field);
    // Clicking empty chrome is an ordinary blur, with no second person action.
    h.request("ui.click", json!({"x":10,"y":1000}), T);
    h.step();
    assert_eq!(h.app.session.smart.people.data.person(person).unwrap().name, "Retained person");
    assert_eq!(h.app.session.undo.len(), undo + 1);
    h.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(h.app.session.smart.people.data, before);
}
