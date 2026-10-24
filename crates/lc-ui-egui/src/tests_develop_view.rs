//! The Develop view: 100% zoom counts the pixels of the frame shown, and slider resets go to
//! the photo's own defaults.

use std::sync::Arc;
use std::time::Duration;

use egui::{Rect, pos2, vec2};
use lightcraft_catalog::{Op, Photo, PhotoId, Source};
use lightcraft_develop::DevelopSettings;
use lightcraft_engine::media::{RenderJob, RenderResult};
use lightcraft_pipeline::geometry::Frame;
use serde_json::json;

use crate::LightcraftApp;
use crate::headless::Headless;
use crate::panels::detail::{fit_rect, native_px};
use crate::render::Slot;
use crate::state::Zoom;

const T: Duration = Duration::from_secs(40);

#[test]
fn percent_zoom_counts_the_pixels_of_the_cropped_and_rotated_frame() {
    let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(4000.0, 3000.0));
    let at_100 = |s: &DevelopSettings| {
        let frame = Frame::new(6000, 4000, s, true);
        fit_rect(area, frame.aspect() as f32, Zoom::Percent(100), native_px(&frame), 1.0, (0.5, 0.5)).size()
    };
    assert_eq!(at_100(&DevelopSettings::default()), vec2(6000.0, 4000.0));
    let mut cropped = DevelopSettings::default();
    cropped.crop.geometry.rect = lightcraft_geom::Rect::new(0.0, 0.0, 0.5, 1.0);
    assert_eq!(at_100(&cropped), vec2(3000.0, 4000.0), "the left half at its own pixels, not twice its size");
    let rotated = DevelopSettings { orientation: lightcraft_geom::Orientation::Rotate90, ..Default::default() };
    assert_eq!(at_100(&rotated), vec2(4000.0, 6000.0), "rotated: 4000 across, not 6000");
    let frame = Frame::new(6000, 4000, &DevelopSettings::default(), true);
    assert_eq!(fit_rect(area, 1.5, Zoom::Percent(200), native_px(&frame), 2.0, (0.5, 0.5)).size(), vec2(6000.0, 4000.0), "HiDPI: points, not pixels");
}

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
