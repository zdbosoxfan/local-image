//! Exercise the real egui widgets and background tasks, with deterministic model inference.
use crate::{LightcraftApp, Services, headless::Headless, panels::smart_sort::SmartSortDialog, state::Dialog};
use lightcraft_engine::{Session, smart_sort::mock::MockTagger};
use serde_json::json;
use std::{sync::Arc, time::Duration};
const T: Duration = Duration::from_secs(30);
fn click(h: &mut Headless, id: &str) {
    let result = h.request("ui.clickWidget", json!({"id":id}), T);
    assert_eq!(result["ok"], true, "{id}: {result}");
    h.step();
    h.step();
}
fn type_into(h: &mut Headless, id: &str, value: &str) {
    click(h, id);
    h.request("ui.key", json!({"key":"a","cmd":true}), T);
    h.request("ui.text", json!({"text":value}), T);
    h.step();
}
fn state(h: &Headless) -> &SmartSortDialog {
    let Some(Dialog::SmartSort { state }) = &h.app.ui.dialog else { panic!("no smart sort dialog") };
    state
}
fn state_mut(h: &mut Headless) -> &mut SmartSortDialog {
    let Some(Dialog::SmartSort { state }) = &mut h.app.ui.dialog else { panic!("no smart sort dialog") };
    state
}
fn harness(size: [f32; 2]) -> Headless {
    let mut session = Session::with_demo();
    session.smart.tagger = Some(Arc::new(MockTagger));
    let ids: Vec<_> = session.catalog.photos().filter(|p| p.flag != lightcraft_catalog::Flag::Reject).map(|p| p.id).take(3).collect();
    session.execute("library.select", &json!({"ids":ids})).unwrap();
    let app = LightcraftApp::new(session, Services { write_shared: Some(Arc::new(lightcraft_engine::export::write_file)), ..Default::default() });
    let mut h = Headless::new(app, size, 1.0);
    h.step();
    let result = h.request("ui.menu.invoke", json!({"id":"dialog.smartSort"}), T);
    assert_eq!(result["ok"], true, "{result}");
    h.step();
    h.step();
    h
}
fn setup(h: &mut Headless) {
    assert_eq!(state(h).preset.categories.len(), 6);
    assert!(widget(h, "dialog:window").contains_rect(widget(h, "smartSort:analyze")));
    click(h, "smartSort:preset");
    click(h, "smartSort:preset:3");
    type_into(h, "smartSort:folder:0", "A");
    type_into(h, "smartSort:folder:1", "B");
    type_into(h, "smartSort:tagInput:0", "red, blue,");
    assert_eq!(state(h).preset.categories[0].prompts, ["red", "blue"]);
    for expected in [["blue", "red"], ["red", "blue"]] {
        let to = widget(h, "smartSort:tag:0:1").center();
        let result = h.request("ui.dragWidget", json!({"id":"smartSort:tag:0:0","toX":to.x,"toY":to.y}), T);
        assert_eq!(result["ok"], true);
        h.step();
        h.step();
        assert_eq!(state(h).preset.categories[0].prompts, expected);
    }
    click(h, "smartSort:matchAll:0");
    assert!(state(h).preset.categories[0].match_all);
    click(h, "smartSort:matchAll:0");
    click(h, "smartSort:tagSetMenu:0");
    click(h, "smartSort:saveTagSet:0");
    type_into(h, "smartSort:tagSetName", "Colors");
    click(h, "smartSort:confirmTagSet");
    let sets = h.app.session.execute("smartSort.tagSets", &json!({})).unwrap();
    let index = sets.as_array().unwrap().iter().position(|s| s["name"] == "Colors").unwrap();
    click(h, "smartSort:tagSetMenu:1");
    click(h, &format!("smartSort:applyTagSet:1:{index}:replace"));
    assert_eq!(state(h).preset.categories[1].prompts, ["red", "blue"]);
    click(h, "smartSort:tagSetMenu:1");
    click(h, "smartSort:manageTagSets");
    type_into(h, "smartSort:renameTagSet:Colors", "Color set");
    click(h, "smartSort:renameTagSetSave:Colors");
    assert_eq!(h.app.session.smart.tag_sets[0].name, "Color set");
    click(h, "smartSort:deleteTagSet:Color set");
    assert!(h.app.session.smart.tag_sets.is_empty());
    click(h, "smartSort:closeTagSets");
    click(h, "smartSort:multi");
    click(h, "smartSort:analyze");
    assert!(h.step_until(T, |h| h.app.smart_sort.is_none() && state(h).step == 1));
    h.step();
    h.step();
    assert_eq!(state(h).rows.len(), 3);
    assert_eq!(state(h).counts.values().sum::<usize>(), state(h).rows.iter().map(|p| p.row.assigned.len().max(1)).sum::<usize>());
}
fn widget(h: &Headless, id: &str) -> egui::Rect {
    h.app.widgets.iter().rev().find(|(w, _)| w == id).map(|(_, r)| *r).unwrap_or_else(|| panic!("missing {id}"))
}
fn right_click(h: &mut Headless, id: &str) {
    let rect = widget(h, id);
    let p = rect.center();
    h.request("ui.click", json!({"x":p.x,"y":p.y,"button":"right"}), T);
    h.step();
    h.step();
}

#[test]
fn smart_sort_click_tags_review_drag_context_and_export_both_overlap_modes() {
    for first in [false, true] {
        let mut h = harness([1280.0, 800.0]);
        setup(&mut h);
        let id = state(&h).rows[0].id;
        let folder = if state(&h).rows[0].row.assigned.is_empty() { "Unsorted" } else { "A" };
        click(&mut h, &format!("smartSort:reviewFolder:{folder}"));
        let target = widget(&h, "smartSort:reviewFolder:B").center();
        let result = h.request("ui.dragWidget", json!({"id":format!("smartSort:thumb:{}",id.0),"toX":target.x,"toY":target.y}), T);
        assert_eq!(result["ok"], true, "{result}");
        h.step();
        h.step();
        let row = state(&h).rows.iter().find(|p| p.id == id).unwrap();
        assert!(row.row.manual);
        assert_eq!(row.row.assigned, ["B"]);
        click(&mut h, "smartSort:reviewFolder:B");
        right_click(&mut h, &format!("smartSort:thumb:{}", id.0));
        click(&mut h, "smartSort:context:alsoAdd");
        click(&mut h, "smartSort:alsoAdd:A");
        assert_eq!(state(&h).rows.iter().find(|p| p.id == id).unwrap().row.assigned, ["A", "B"]);
        // Correct every photo to both folders using real selection + Move / Also add menus.
        for id in state(&h).source.clone() {
            let assigned = state(&h).rows.iter().find(|p| p.id == id).unwrap().row.assigned.clone();
            let current = assigned.first().map(String::as_str).unwrap_or("Unsorted");
            click(&mut h, &format!("smartSort:reviewFolder:{current}"));
            right_click(&mut h, &format!("smartSort:thumb:{}", id.0));
            click(&mut h, "smartSort:context:moveTo");
            click(&mut h, "smartSort:moveTo:B");
            click(&mut h, "smartSort:reviewFolder:B");
            right_click(&mut h, &format!("smartSort:thumb:{}", id.0));
            click(&mut h, "smartSort:context:alsoAdd");
            click(&mut h, "smartSort:alsoAdd:A");
        }
        assert!(state(&h).rows.iter().all(|p| p.row.manual && p.row.assigned == ["A", "B"]));
        let inspect = h.request("ui.inspect", json!({}), T);
        assert_eq!(inspect["result"]["dialog"]["state"]["step"], 1);
        click(&mut h, "smartSort:next");
        assert_eq!(state(&h).step, 2);
        assert!(h.app.session.catalog.photo(id).unwrap().meta.keywords.contains(&"Smart Sort|B".into()));
        let undo = h.request("ui.menu.invoke", json!({"id":"edit.undo"}), T);
        assert_eq!(undo["ok"], true);
        assert!(!h.app.session.catalog.photo(id).unwrap().meta.keywords.iter().any(|k| k.starts_with("Smart Sort|")));
        let redo = h.request("ui.menu.invoke", json!({"id":"edit.redo"}), T);
        assert_eq!(redo["ok"], true);
        assert_eq!(state(&h).preset.folders.len(), 3);
        assert!(!state(&h).preset.folders[2].enabled);
        click(&mut h, "smartSort:folders:default:0");
        click(&mut h, "smartSort:folders:addCustom");
        click(&mut h, "smartSort:folders:tagPicker:3");
        click(&mut h, "smartSort:folders:tag:3:0");
        click(&mut h, "smartSort:folders:tagPicker:3");
        click(&mut h, "smartSort:folders:tag:3:1");
        // Close the picker by clicking outside its popup.
        h.request("ui.click", json!({"x":10,"y":10}), T);
        h.step();
        h.step();
        assert_eq!(state(&h).preset.folders[3].tags, ["A", "B"]);
        if first {
            click(&mut h, "smartSort:firstMatch");
        }
        click(&mut h, "smartSort:albums");
        let dir = std::env::temp_dir().join(format!("smart-sort-ui-{}-{first}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        type_into(&mut h, "smartSort:dest", &dir.to_string_lossy());
        state_mut(&mut h).export_params = json!({"format":"png","longEdge":64,"name":"{seq:3}"});
        click(&mut h, "smartSort:export");
        assert!(h.app.ui.dialog.is_none());
        assert!(h.step_until(T, |h| h.app.export.is_none() && h.app.last_export_result.is_some()));
        let files = &h.app.last_export_result.as_ref().unwrap()["files"];
        assert!(files.as_array().unwrap().iter().all(|f| f.get("path").is_some()), "{files}");
        assert_eq!(files.as_array().unwrap().len(), if first { 3 } else { 6 });
        assert!(!dir.join("A").exists());
        assert!(!dir.join("Unsorted").exists());
        assert_eq!(std::fs::read_dir(dir.join("B")).unwrap().count(), 3);
        if first {
            assert!(!dir.join("Custom 1").exists());
        } else {
            assert_eq!(std::fs::read_dir(dir.join("Custom 1")).unwrap().count(), 3);
        }
        let b = h.app.session.catalog.albums().find(|a| a.name == "B").unwrap();
        assert_eq!(h.app.session.catalog.album_count(b.id), 3);
        let custom = h.app.session.catalog.albums().find(|a| a.name == "Custom 1").unwrap();
        assert_eq!(h.app.session.catalog.album_count(custom.id), if first { 0 } else { 3 });
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn smart_sort_dialog_layout_at_both_sizes_and_no_service_download() {
    for size in [[1280.0, 800.0], [1920.0, 1080.0]] {
        let mut h = harness(size);
        setup(&mut h);
        for step in ["sort", "review", "export"] {
            click(&mut h, &format!("smartSort:step:{step}"));
            h.step();
            h.step();
            let rect = widget(&h, "dialog:window");
            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0], size[1])).contains_rect(rect), "{rect:?}");
            for (id, r) in &h.app.widgets {
                if id.starts_with("smartSort:") && !id.contains("tagSet") {
                    assert!(rect.expand(1.0).contains_rect(*r), "{id}: {r:?} outside {rect:?}");
                }
            }
            let screenshot = h.request("ui.screenshot", json!({}), T);
            assert_eq!(screenshot["ok"], true);
            assert_eq!(screenshot["result"]["width"], size[0] as u64);
            let image = h.snapshot(T);
            assert!(image.pixels.iter().any(|p| p.r() > 30 || p.g() > 30 || p.b() > 30));
            let rgba =
                lightcraft_raster::Rgba8 { width: image.size[0], height: image.size[1], data: image.pixels.iter().map(|p| p.to_array()).collect() };
            let png =
                lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&rgba), &lightcraft_codecs::EncodeMeta::default()).unwrap();
            let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/smart-sort-p2b/screenshots");
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join(format!("{step}-{}.png", size[0] as u32)), png).unwrap();
        }
    }
    let mut h = Headless::new(LightcraftApp::new(Session::new(), Services::default()), [1280.0, 800.0], 1.0);
    h.request("ui.menu.invoke", json!({"id":"dialog.smartSort"}), T);
    h.step();
    assert!(!h.app.widgets.iter().any(|(id, _)| id == "smartSort:download"));
}

#[test]
fn smart_sort_cancel_keeps_cache_and_resume_skips_completed_photos() {
    use lightcraft_engine::smart_sort::Tagger;
    struct SlowTagger(std::sync::atomic::AtomicUsize);
    impl Tagger for SlowTagger {
        fn model_id(&self) -> &str {
            "mock-tags"
        }
        fn dim(&self) -> usize {
            8
        }
        fn embed_text(&self, text: &str) -> Result<Vec<f32>, String> {
            MockTagger.embed_text(text)
        }
        fn embed_image(&self, img: &lightcraft_raster::Rgba8) -> Result<Vec<f32>, String> {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 0 {
                std::thread::sleep(Duration::from_secs(2));
            }
            MockTagger.embed_image(img)
        }
    }
    let mut h = harness([1280.0, 800.0]);
    let directory = std::env::temp_dir().join(format!("smart-sort-ui-cancel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let total = std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).max(1)) + 8;
    let image = lightcraft_raster::Rgba8::filled(16, 16, [255, 0, 0, 255]);
    let png = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&image), &lightcraft_codecs::EncodeMeta::default()).unwrap();
    h.app.session = std::mem::take(&mut h.app.session).with_fs();
    let mut ids = Vec::new();
    for i in 0..total {
        let path = directory.join(format!("{i}.png"));
        std::fs::write(&path, &png).unwrap();
        let id = h.app.session.catalog.alloc_photo_id();
        let photo = lightcraft_catalog::Photo::new(
            id,
            lightcraft_catalog::Source::File { path: path.to_string_lossy().into_owned() },
            &format!("{i}.png"),
            "PNG",
            16,
            16,
            "2026-10-09",
        );
        h.app.session.commit("test image", lightcraft_catalog::Op::AddPhoto { photo: Box::new(photo) }).unwrap();
        ids.push(id);
    }
    h.app.session.execute("library.select", &json!({"ids":ids})).unwrap();
    state_mut(&mut h).source = ids;
    h.app.session.smart.store = lightcraft_engine::smart_sort::Store::new(Some(&directory));
    h.app.session.smart.tagger = Some(Arc::new(SlowTagger(std::sync::atomic::AtomicUsize::new(0))));
    click(&mut h, "smartSort:preset");
    click(&mut h, "smartSort:preset:3");
    type_into(&mut h, "smartSort:tagInput:0", "red,");
    type_into(&mut h, "smartSort:tagInput:1", "blue,");
    click(&mut h, "smartSort:analyze");
    assert!(h.step_until(T, |h| h.app.session.smart.store.len("mock-tags") > 0));
    assert!(h.app.smart_sort.is_some());
    click(&mut h, "smartSort:cancel");
    assert!(h.step_until(T, |h| h.app.smart_sort.is_none()));
    h.step();
    h.step();
    assert_eq!(state(&h).step, 0);
    let kept = h.app.session.smart.store.len("mock-tags");
    assert!(kept > 0 && kept < total, "kept {kept} of {total}");
    let mut cached = lightcraft_engine::smart_sort::Store::new(Some(&directory));
    cached.ensure("mock-tags", 8).unwrap();
    assert_eq!(cached.len("mock-tags"), kept);
    h.app.session.smart.tagger = Some(Arc::new(MockTagger));
    click(&mut h, "smartSort:analyze");
    assert!(h.step_until(T, |h| h.app.smart_sort.is_none() && state(h).step == 1));
    assert_eq!(state(&h).skipped, kept);
    assert_eq!(h.app.session.smart.store.len("mock-tags"), total);
    h.settle(T);
    drop(h);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn smart_sort_model_download_uses_host_progress_and_cancel() {
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let out = calls.clone();
    let status = Arc::new(std::sync::Mutex::new(crate::ModelDownloadStatus::default()));
    let status_read = status.clone();
    let services = Services {
        download_models: Some(Box::new(move |id, cancel| {
            out.lock().unwrap().push((id.to_string(), cancel));
            Ok(())
        })),
        model_download_status: Some(Box::new(move |_| status_read.lock().unwrap().clone())),
        ..Default::default()
    };
    let mut h = Headless::new(LightcraftApp::new(Session::new(), services), [1280.0, 800.0], 1.0);
    h.request("ui.menu.invoke", json!({"id":"dialog.smartSort"}), T);
    h.step();
    h.step();
    click(&mut h, "smartSort:preset");
    click(&mut h, "smartSort:preset:3");
    click(&mut h, "smartSort:download");
    *status.lock().unwrap() = crate::ModelDownloadStatus { running: true, done: 120, total: 607, error: None };
    h.step();
    h.step();
    click(&mut h, "smartSort:downloadCancel");
    assert_eq!(*calls.lock().unwrap(), vec![("clip-b32-laion".into(), false), ("clip-b32-laion".into(), true)]);
}

fn p2b_harness(size: [f32; 2], times: &[Option<&str>], red_count: usize) -> Headless {
    use lightcraft_catalog::{Op, Photo, Source};
    use lightcraft_engine::smart_sort::Tagger;
    let mut h = harness(size);
    let mut ids = Vec::new();
    for (index, time) in times.iter().enumerate() {
        let id = h.app.session.catalog.alloc_photo_id();
        let mut photo = Photo::new(id, Source::Demo { scene: 1 }, &format!("p2b-{index}.jpg"), "JPEG", 64, 48, "2026-10-09");
        photo.captured = time.map(str::to_string);
        photo.content_hash = Some(format!("p2b-ui-{}", id.0));
        h.app.session.commit("test photo", Op::AddPhoto { photo: Box::new(photo) }).unwrap();
        let key = lightcraft_engine::media::content_key(h.app.session.catalog.photo(id).unwrap());
        let direction = if index < red_count {
            MockTagger.embed_image(&lightcraft_raster::Rgba8::filled(16, 16, [255, 0, 0, 255])).unwrap()
        } else {
            MockTagger.embed_text("blue").unwrap()
        };
        h.app.session.smart.store.ensure("mock-tags", 8).unwrap();
        h.app.session.smart.store.insert("mock-tags", key.clone(), direction).unwrap();
        h.app.session.smart.store.set_sharpness(key, index as f32);
        ids.push(id);
    }
    h.app.session.execute("library.select", &json!({"ids":&ids[..ids.len().min(3)]})).unwrap();
    h.app.ui.dialog = None;
    let result = h.request("ui.menu.invoke", json!({"id":"dialog.smartSort"}), T);
    assert_eq!(result["ok"], true);
    h.step();
    h.step();
    assert_eq!(state(&h).library_examples, ids[..ids.len().min(3)]);
    // Broaden the source to include matching candidates beyond the Library examples.
    state_mut(&mut h).source = ids.clone();
    click(&mut h, "smartSort:preset");
    click(&mut h, "smartSort:preset:3");
    type_into(&mut h, "smartSort:folder:0", "A");
    type_into(&mut h, "smartSort:folder:1", "B");
    type_into(&mut h, "smartSort:tagInput:0", "red,");
    type_into(&mut h, "smartSort:tagInput:1", "blue,");
    h
}
fn p2b_analyze(h: &mut Headless) {
    click(h, "smartSort:analyze");
    assert!(h.step_until(T, |h| h.app.smart_sort.is_none() && state(h).step == 1));
    h.step();
    h.step();
    assert!(state(h).error.is_empty(), "{}", state(h).error);
}
fn key(h: &mut Headless, key: &str, shift: bool, alt: bool, cmd: bool) {
    let result = h.request("ui.key", json!({"key":key,"shift":shift,"alt":alt,"cmd":cmd}), T);
    assert_eq!(result["ok"], true, "{result}");
    h.step();
    h.step();
}
fn assignment(h: &Headless, id: lightcraft_catalog::PhotoId) -> Vec<String> {
    state(h).rows.iter().find(|p| p.id == id).unwrap().row.assigned.clone()
}
#[test]
fn smart_sort_p2b_sessions_rename_nested_patterns_export_and_filter() {
    let mut h = p2b_harness([1280.0, 800.0], &[Some("2026-10-09T09:00:00"), Some("2026-10-09T09:10:00"), Some("2026-10-09T09:35:00"), None], 4);
    click(&mut h, "smartSort:sessions");
    assert!(state(&h).preset.sessions.enabled);
    assert!(h.app.widgets.iter().any(|(id, _)| id == "smartSort:session:1"));
    type_into(&mut h, "smartSort:session:0", "Morning keynote");
    assert_eq!(state(&h).preset.sessions.names["2026-10-09T09:00:00"], "Morning keynote");
    p2b_analyze(&mut h);
    click(&mut h, "smartSort:next");
    type_into(&mut h, "smartSort:eventName", "Shoot:test");
    type_into(&mut h, "smartSort:folderPattern", "{event}/{session}/{folder}");
    type_into(&mut h, "smartSort:filePattern", "{folder}_{seq:4}");
    // Narrow the default folder to the renamed session using the visible picker.
    click(&mut h, "smartSort:folderSession:0");
    click(&mut h, "smartSort:folderSession:0:0");
    let preset = state(&h).preset.clone();
    let ids = state(&h).source.clone();
    let plan = h.app.session.execute("smartSort.plan", &json!({"sortPreset":preset,"ids":ids})).unwrap();
    assert_eq!(plan["folders"][0]["ids"], json!(&ids[..2]));
    // Restore all sessions, so both timed groups and No time are exported.
    click(&mut h, "smartSort:folderSession:0");
    click(&mut h, "smartSort:folderSession:0:all");
    let dir = std::env::temp_dir().join(format!("smart-sort-p2b-sessions-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    type_into(&mut h, "smartSort:dest", &dir.to_string_lossy());
    state_mut(&mut h).export_params = json!({"format":"png","longEdge":32});
    click(&mut h, "smartSort:export");
    assert!(h.step_until(T, |h| h.app.export.is_none() && h.app.last_export_result.is_some()));
    let files = h.app.last_export_result.as_ref().unwrap()["files"].as_array().unwrap();
    assert_eq!(files.len(), 4, "{files:?}");
    assert!(dir.join("Shoot_test/Morning keynote/A/A_0001.png").exists(), "{files:?}");
    assert!(dir.join("Shoot_test/Session 2/A/A_0001.png").exists());
    assert!(dir.join("Shoot_test/No time/A/A_0001.png").exists());
    let saved = h.app.session.smart.prefs.last.as_ref().unwrap();
    assert_eq!(saved.sessions.names["2026-10-09T09:00:00"], "Morning keynote");
    assert_eq!(saved.folder_pattern, "{event}/{session}/{folder}");
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn smart_sort_p2b_example_folder_from_library_and_review_three_photos() {
    for library in [true, false] {
        let mut h = p2b_harness([1280.0, 800.0], &[None; 5], 4);
        p2b_analyze(&mut h);
        let ids = state(&h).source.clone();
        click(&mut h, "smartSort:reviewFolder:A");
        if !library {
            click(&mut h, &format!("smartSort:thumb:{}", ids[0].0));
            key(&mut h, "ArrowRight", true, false, false);
            key(&mut h, "ArrowRight", true, false, false);
            assert_eq!(state(&h).selected.len(), 3);
        }
        click(&mut h, "smartSort:folderFromExamples");
        let example = state(&h).preset.categories.last().unwrap();
        assert!(example.prompts.is_empty());
        assert_eq!(example.exemplars.len(), 3);
        let name = example.name.clone();
        for id in &ids[..4] {
            assert!(assignment(&h, *id).contains(&name), "{} {:?}", id.0, assignment(&h, *id));
        }
        assert!(!assignment(&h, ids[4]).contains(&name));
        click(&mut h, "smartSort:next");
        assert!(state(&h).preset.folders.iter().any(|f| f.name == name));
    }
}
#[test]
fn smart_sort_p2b_burst_badges_best_all_export_and_library_stack() {
    for all in [false, true] {
        let mut h = p2b_harness([1280.0, 800.0], &[Some("2026-10-09T09:00:00"), Some("2026-10-09T09:00:01"), Some("2026-10-09T09:30:00")], 3);
        click(&mut h, "smartSort:bursts");
        click(&mut h, "smartSort:burstStrictness");
        click(&mut h, "smartSort:burstStrictness:Normal");
        p2b_analyze(&mut h);
        let ids = state(&h).source.clone();
        assert_eq!(state(&h).bursts.len(), 2);
        assert_eq!(state(&h).bursts[0].best, ids[1]);
        assert!(h.app.widgets.iter().any(|(id, _)| id == &format!("smartSort:burstBadge:{}", ids[1].0)));
        assert!(!h.app.widgets.iter().any(|(id, _)| id == &format!("smartSort:thumb:{}", ids[0].0)));
        click(&mut h, "smartSort:next");
        if all {
            click(&mut h, "smartSort:allBursts");
        } else {
            click(&mut h, "smartSort:bestBursts");
        }
        click(&mut h, "smartSort:libraryStacks");
        let undo = h.app.session.undo.len();
        let dir = std::env::temp_dir().join(format!("smart-sort-p2b-bursts-{}-{all}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        type_into(&mut h, "smartSort:dest", &dir.to_string_lossy());
        state_mut(&mut h).export_params = json!({"format":"png","longEdge":32});
        click(&mut h, "smartSort:export");
        assert!(h.step_until(T, |h| h.app.export.is_none() && h.app.last_export_result.is_some()));
        let files = h.app.last_export_result.as_ref().unwrap()["files"].as_array().unwrap();
        assert_eq!(files.len(), if all { 3 } else { 2 }, "{files:?}");
        assert!(files.iter().any(|f| f["photo"] == json!(ids[1])));
        assert_eq!(h.app.session.catalog.stack_of(ids[0]).unwrap().photos.len(), 2);
        assert_eq!(h.app.session.undo.len(), undo + 1);
        h.app.session.undo_step().unwrap();
        assert!(h.app.session.catalog.stack_of(ids[0]).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn smart_sort_p2b_review_keyboard_assignments_selection_and_undo() {
    let mut h = p2b_harness([1280.0, 800.0], &[None; 4], 3);
    p2b_analyze(&mut h);
    let ids = state(&h).source.clone();
    click(&mut h, "smartSort:reviewFolder:A");
    click(&mut h, &format!("smartSort:thumb:{}", ids[0].0));
    key(&mut h, "1", false, false, false);
    assert_eq!(assignment(&h, ids[0]), ["A"]);
    key(&mut h, "0", false, false, false);
    assert!(assignment(&h, ids[0]).is_empty());
    key(&mut h, "1", false, true, false);
    assert_eq!(assignment(&h, ids[0]), ["A"]);
    key(&mut h, "2", false, true, false);
    assert_eq!(assignment(&h, ids[0]), ["A", "B"]);
    key(&mut h, "Backspace", false, false, false);
    assert_eq!(assignment(&h, ids[0]), ["B"]);
    key(&mut h, "z", false, false, true);
    assert_eq!(assignment(&h, ids[0]), ["A", "B"]);
    key(&mut h, "z", false, false, false);
    assert_eq!(assignment(&h, ids[0]), ["A"]);
    key(&mut h, "z", true, false, true);
    assert_eq!(assignment(&h, ids[0]), ["A", "B"]);
    let undo = h.request("ui.menu.invoke", json!({"id":"edit.undo"}), T);
    assert_eq!(undo["ok"], true);
    h.step();
    h.step();
    assert_eq!(assignment(&h, ids[0]), ["A"]);
    let redo = h.request("ui.menu.invoke", json!({"id":"edit.redo"}), T);
    assert_eq!(redo["ok"], true);
    h.step();
    h.step();
    assert_eq!(assignment(&h, ids[0]), ["A", "B"]);
    // Number keys in the dialog never change catalog ratings.
    assert_eq!(h.app.session.catalog.photo(ids[0]).unwrap().rating, 0);
    click(&mut h, "smartSort:reviewFolder:A");
    click(&mut h, &format!("smartSort:thumb:{}", ids[0].0));
    key(&mut h, "ArrowRight", false, false, false);
    assert_eq!(state(&h).selected.len(), 1);
    key(&mut h, "ArrowRight", true, false, false);
    assert_eq!(state(&h).selected.len(), 2);
    // Command-click toggles a photo independently of the range selection.
    let target = state(&h).selected[0];
    let before = state(&h).selected.len();
    let result = h.request("ui.clickWidget", json!({"id":format!("smartSort:thumb:{}",target.0),"cmd":true}), T);
    assert_eq!(result["ok"], true);
    h.step();
    h.step();
    assert_eq!(state(&h).selected.len(), before - 1);
    key(&mut h, "a", false, false, true);
    assert!(state(&h).selected.len() >= 3);
    key(&mut h, "Escape", false, false, false);
    assert!(state(&h).selected.is_empty());
    assert!(h.app.ui.dialog.is_some());
}

#[test]
fn smart_sort_p2b_sessions_bursts_layout_at_both_sizes() {
    for size in [[1280.0, 800.0], [1920.0, 1080.0]] {
        let mut h = p2b_harness(size, &[Some("2026-10-09T09:00:00"), Some("2026-10-09T09:00:01"), Some("2026-10-09T09:35:00")], 3);
        click(&mut h, "smartSort:sessions");
        click(&mut h, "smartSort:bursts");
        p2b_analyze(&mut h);
        for step in ["sort", "review", "export"] {
            click(&mut h, &format!("smartSort:step:{step}"));
            h.step();
            h.step();
            let rect = widget(&h, "dialog:window");
            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0], size[1])).contains_rect(rect), "{rect:?}");
            for (id, r) in &h.app.widgets {
                if id.starts_with("smartSort:") {
                    assert!(rect.expand(1.0).contains_rect(*r), "{id}: {r:?} outside {rect:?}");
                }
            }
            assert!(rect.contains_rect(widget(
                &h,
                if step == "export" {
                    "smartSort:export"
                } else if step == "review" {
                    "smartSort:next"
                } else {
                    "smartSort:analyze"
                }
            )));
            let image = h.snapshot(T);
            let rgba =
                lightcraft_raster::Rgba8 { width: image.size[0], height: image.size[1], data: image.pixels.iter().map(|p| p.to_array()).collect() };
            let png =
                lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&rgba), &lightcraft_codecs::EncodeMeta::default()).unwrap();
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/smart-sort-p2b/screenshots");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("{step}-features-{}.png", size[0] as u32)), png).unwrap();
        }
    }
}
