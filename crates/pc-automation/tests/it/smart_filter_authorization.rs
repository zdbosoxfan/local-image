//! Stored smart-filter parameters must not give automation ambient filesystem access: a Color
//! Lookup `file` or a Displace `mapPath` stored in a document (a crafted `.pcraft`, or merged in by
//! `layer.smartFilter.setParams`) is checked against the session's gate whenever the filter is
//! re-rendered, on the requesting thread, inside `Session::edit` and on job workers.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use photocraft_automation::workspace::authorize_desktop_engine_step;
use photocraft_automation::{AuthorizedWorkspace, Headless};
use photocraft_doc::{BlendMode, Document, LayerContent, LayerId, SmartFilter};
use photocraft_engine::actions_cmds::Action;
use photocraft_engine::jobs::Started;
use photocraft_engine::{Session, smart_cmds};
use serde_json::{Value, json};

const LOOKUP: &str = "image.adjustments.colorLookup";
const DISPLACE: &str = "filter.distort.displace";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let base = std::env::temp_dir().join(format!("pc-filter-auth-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(base.join("workspace")).unwrap();
        std::fs::create_dir_all(base.join("outside")).unwrap();
        let f = Self(base);
        std::fs::write(f.outside("black.png"), png(0)).unwrap();
        std::fs::write(f.outside("white.png"), png(255)).unwrap();
        std::fs::write(f.outside("black.cube"), cube(0)).unwrap();
        std::fs::write(f.outside("white.cube"), cube(1)).unwrap();
        f
    }

    fn root(&self) -> PathBuf {
        self.0.join("workspace")
    }

    fn outside(&self, name: &str) -> PathBuf {
        self.0.join("outside").join(name)
    }

    fn headless(&self) -> Headless {
        Headless::with_workspace(AuthorizedWorkspace::new(Some(&self.root()), Some(&self.root())).unwrap())
    }

    /// A gated session whose active document holds a smart object with one harmless smart filter
    /// of the command under test (a Color Lookup with no table, or a Displace by document).
    fn setup(&self, lookup: bool) -> (Headless, LayerId) {
        let mut h = self.headless();
        h.handle("doc.new", json!({"width": 32, "height": 32, "background": "white"})).unwrap();
        let layer = LayerId(h.session.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap()["layer"].as_u64().unwrap());
        let (command, params) = if lookup { (LOOKUP, json!({"lut": "none"})) } else { (DISPLACE, json!({"horizontal": 50, "vertical": 0, "mapDocument": 0})) };
        h.session
            .edit("Add Smart Filter", |doc, _| {
                let LayerContent::Smart(sm) = &mut doc.layer_mut(layer).unwrap().content else { panic!("smart object") };
                sm.smart_filters.push(SmartFilter { command: command.into(), params, blend: BlendMode::Normal, opacity: 1.0, visible: true });
                Ok(())
            })
            .unwrap();
        (h, layer)
    }

    /// The [`Self::setup`] document with the filter's params pointing outside the workspace, as a
    /// crafted file would store them, saved as `stored.pcraft` in the workspace.
    fn saved_filter(&self, lookup: bool) -> (Document, LayerId) {
        let (h, layer) = self.setup(lookup);
        let mut doc = (*h.session.active().unwrap().doc).clone();
        let LayerContent::Smart(sm) = &mut doc.layer_mut(layer).unwrap().content else { panic!("smart object") };
        sm.smart_filters[0].params = path_params(lookup, &self.outside(if lookup { "white.cube" } else { "white.png" }));
        let bytes = photocraft_format::save_to_bytes(&doc, &Default::default()).unwrap();
        std::fs::write(self.root().join("stored.pcraft"), bytes).unwrap();
        (doc, layer)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn png(value: u8) -> Vec<u8> {
    let image = photocraft_codecs::Image::from_u8(32, 32, photocraft_codecs::ChannelLayout::Rgb, vec![value; 32 * 32 * 3]).unwrap();
    photocraft_codecs::encode(&image, photocraft_codecs::Format::Png, &Default::default()).unwrap()
}

fn cube(value: u8) -> String {
    format!("LUT_3D_SIZE 2\n{}", format!("{value} {value} {value}\n").repeat(8))
}

fn path_params(lookup: bool, path: &Path) -> Value {
    if lookup { json!({"file": path}) } else { json!({"mapPath": path, "mapDocument": null}) }
}

fn outside_names(lookup: bool) -> [&'static str; 2] {
    if lookup { ["black.cube", "white.cube"] } else { ["black.png", "white.png"] }
}

fn assert_denied<T: std::fmt::Debug>(result: Result<T, impl std::fmt::Display>) {
    match result {
        Err(error) => assert!(error.to_string().contains("ambient filesystem"), "{error}"),
        Ok(value) => panic!("ambient file access was accepted: {value:?}"),
    }
}

#[test]
fn parameter_edits_reject_outside_images_and_luts_without_changing_the_document() {
    for lookup in [false, true] {
        let f = Fixture::new();
        let (mut h, layer) = f.setup(lookup);
        let before = h.render_png(None, 64).unwrap();
        let doc = h.session.active().unwrap().doc.clone();
        let journal = h.session.journal.len();
        for name in outside_names(lookup) {
            let edit = json!({"layer": layer.0, "params": path_params(lookup, &f.outside(name))});
            assert_denied(h.session.execute("layer.smartFilter.setParams", edit.clone()));
            assert_denied(h.handle("engine.execute", json!({"command": "layer.smartFilter.setParams", "params": edit})));
            assert!(Arc::ptr_eq(&doc, &h.session.active().unwrap().doc));
            assert_eq!(h.session.journal.len(), journal);
            assert_eq!(h.render_png(None, 64).unwrap(), before);
        }
        // Numeric and embedded-data edits remain usable.
        let safe = if lookup { json!({"data": cube(0), "fileName": "embedded.cube"}) } else { json!({"horizontal": 10}) };
        h.session.execute("layer.smartFilter.setParams", json!({"layer": layer.0, "params": safe})).unwrap();
    }
}

#[test]
fn action_playback_cannot_hide_a_smart_filter_path_edit() {
    let f = Fixture::new();
    let (mut h, layer) = f.setup(true);
    let doc = h.session.active().unwrap().doc.clone();
    let steps = vec![("layer.smartFilter.setParams".to_string(), json!({"layer": layer.0, "params": path_params(true, &f.outside("white.cube"))}))];
    h.session.actions.list.push(Action { name: "edit filter".into(), steps });
    let result = h.session.execute("actions.play", json!({"action": "edit filter"})).unwrap();
    assert!(result["failed"]["error"].as_str().unwrap().contains("ambient filesystem"), "{result}");
    assert!(Arc::ptr_eq(&doc, &h.session.active().unwrap().doc));
}

#[test]
fn loaded_filters_are_checked_after_merging_even_when_the_edit_contains_no_path() {
    for lookup in [false, true] {
        let f = Fixture::new();
        let (_, layer) = f.saved_filter(lookup);
        let mut h = f.headless();
        h.open(Path::new("stored.pcraft")).unwrap();
        let doc = h.session.active().unwrap().doc.clone();
        assert_denied(h.session.execute("layer.smartFilter.setParams", json!({"layer": layer.0, "params": {"horizontal": 10, "dither": false}})));
        assert!(Arc::ptr_eq(&doc, &h.session.active().unwrap().doc));
        // Clearing the stored path makes the filter editable again.
        let safe = if lookup { json!({"file": "", "lut": "none"}) } else { json!({"mapPath": ""}) };
        h.session.execute("layer.smartFilter.setParams", json!({"layer": layer.0, "params": safe})).unwrap();
    }
}

#[test]
fn loaded_filters_cannot_read_outside_files_on_rerender() {
    for lookup in [false, true] {
        let f = Fixture::new();
        let (_, layer) = f.saved_filter(lookup);
        let mut h = f.headless();
        h.open(Path::new("stored.pcraft")).unwrap();
        let before = h.session.active().unwrap().doc.clone();
        assert_denied(h.session.execute("layer.smartFilter.setVisible", json!({"layer": layer.0, "visible": true})));
        assert!(Arc::ptr_eq(&before, &h.session.active().unwrap().doc));
    }
}

#[test]
fn a_rerender_inside_session_edit_surfaces_the_denial() {
    // Transforms, warps and linked-file refreshes re-render through Session::edit rather than a
    // command's own dispatch; the session's gate must reach them too.
    for lookup in [false, true] {
        let f = Fixture::new();
        let (doc, layer) = f.saved_filter(lookup);
        let mut h = f.headless();
        h.open(Path::new("stored.pcraft")).unwrap();
        let before = h.session.active().unwrap().doc.clone();
        assert_denied(h.session.edit("Re-render", |d, _| smart_cmds::refresh(d, layer).map(|_| ())));
        assert!(Arc::ptr_eq(&before, &h.session.active().unwrap().doc));
        // Without a gate (the local UI) the same document still renders.
        let mut trusted = doc;
        assert!(smart_cmds::refresh(&mut trusted, layer).is_ok());
    }
}

#[test]
fn background_rerender_keeps_the_desktop_request_authority() {
    for lookup in [false, true] {
        let f = Fixture::new();
        let (doc, layer) = f.saved_filter(lookup);
        let mut s = Session::new();
        s.add_document(doc, None);
        s.authorize = Some(authorize_desktop_engine_step);
        let before = s.active().unwrap().doc.clone();
        // A new smart filter re-renders the whole stack, the stored one included, on a worker.
        let started = s.start("filter.blur.gaussianBlur", json!({"layer": layer.0, "radius": 1})).unwrap();
        // The desktop restores UI authority as soon as the automation request returns.
        s.authorize = None;
        let Started::Job(job) = started else { panic!("expected a background render") };
        assert_denied(s.wait_job(job));
        assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc));
        // The worker's restriction does not leak into later trusted UI work.
        s.execute("layer.smartFilter.setVisible", json!({"layer": layer.0, "visible": true})).unwrap();
    }
}

#[test]
fn trusted_parameter_edits_still_read_luts_after_a_denied_request() {
    // Only the Color Lookup reads its stored path on re-render (a re-rendered Displace has no
    // map), so it is the one whose trusted output can show the file was read.
    let f = Fixture::new();
    let (mut h, layer) = f.setup(true);
    let edit = |path: &Path| json!({"layer": layer.0, "params": path_params(true, path)});
    assert_denied(h.session.execute("layer.smartFilter.setParams", edit(&f.outside("white.cube"))));
    let mut trusted = Headless::trusted_local();
    trusted.session.add_document((*h.session.active().unwrap().doc).clone(), None);
    let mut renders = Vec::new();
    for name in ["black.cube", "white.cube", "black.cube"] {
        trusted.session.execute("layer.smartFilter.setParams", edit(&f.outside(name))).unwrap();
        renders.push(trusted.render_png(None, 64).unwrap());
    }
    assert_ne!(renders[0], renders[1], "a trusted path-based LUT is read");
    assert_eq!(renders[0], renders[2]);
}
