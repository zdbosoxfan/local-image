//! The Develop view: slider resets to the photo's own defaults.

use std::sync::Arc;
use std::time::Duration;

use lightcraft_catalog::{Op, Photo, PhotoId, Source};
use lightcraft_engine::media::{RenderJob, RenderResult};
use serde_json::json;

use crate::LightcraftApp;
use crate::headless::Headless;
use crate::render::Slot;

const T: Duration = Duration::from_secs(40);

/// Collects the jobs the UI asks for without running them.
#[derive(Clone, Default)]
struct Capture(std::rc::Rc<std::cell::RefCell<Vec<(Slot, RenderJob)>>>);
impl crate::render::RenderOffload for Capture {
    fn try_start(&mut self, slot: Slot, job: RenderJob) -> Option<RenderJob> {
        self.0.borrow_mut().push((slot, job));
        None
    }
    fn finished(&mut self) -> Vec<(Slot, RenderResult, f64)> {
        vec![]
    }
}

/// A raw as import leaves it (measured as-shot white balance, raw sharpening and colour noise
/// reduction), shown in Develop with the Basic and Detail panels open.
fn raw_in_develop() -> Headless {
    let mut session = lightcraft_engine::Session::new();
    let id = PhotoId(1);
    let mut p = Photo::new(id, Source::File { path: "a1.arw".into() }, "a1.arw", "ARW", 600, 400, "2026-10-24");
    p.kind = lightcraft_catalog::MediaKind::Raw;
    p.measured_wb = true;
    p.as_shot_wb = Some((5200.0, 4.0));
    p.develop = Arc::new(p.camera_defaults());
    session.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    session.selection = lightcraft_engine::Selection::single(id);
    let mut app = LightcraftApp::new(session, crate::Services { png: None, ..Default::default() });
    app.ui.view = crate::state::ViewMode::Detail;
    app.ui.right = crate::state::RightPanel::Edit;
    app.ui.left_panel = false;
    app.renderer.set_offload(Box::new(Capture::default()));
    for section in ["light", "detail"] {
        if !app.ui.section_open(section) {
            app.ui.toggle_section(section);
        }
    }
    let mut h = Headless::new(app, [1400.0, 2400.0], 1.0);
    for _ in 0..3 {
        h.step();
    }
    h
}

fn double_click(h: &mut Headless, id: &str) {
    let r = h.request("ui.clickWidget", json!({"id": id, "count": 2}), T);
    assert_eq!(r["ok"], true, "{id}: {r}");
    // a second double-click right after would count as a triple click
    for _ in 0..60 {
        h.step();
    }
}

#[test]
fn double_clicking_a_slider_returns_it_to_the_photos_own_default() {
    let mut h = raw_in_develop();
    let develop = |h: &Headless| (*h.app.session.develop_of(PhotoId(1)).unwrap()).clone();
    h.app.session.execute("develop.set", &json!({"values": {"detail.sharpenAmount": 90, "wb.temp": 3000}})).unwrap();
    h.step();
    double_click(&mut h, "slider:detail.sharpenAmount");
    assert_eq!(develop(&h).detail.sharpen_amount, 40.0, "the raw default, not 0");
    double_click(&mut h, "slider:wb.temp");
    let wb = develop(&h).wb;
    assert_eq!((wb.mode, wb.temp, wb.tint), (lightcraft_develop::WbMode::AsShot, 5200.0, 4.0), "As Shot, not 6500 K Custom");
}
