//! The Develop loupe at true pixel scale (100% of the frame it shows, a full-resolution render
//! when zoomed in) and slider resets to the photo's own defaults.

use std::sync::Arc;
use std::time::Duration;

use egui::{Rect, pos2, vec2};
use lightcraft_catalog::{Op, Photo, PhotoId, Source};
use lightcraft_develop::DevelopSettings;
use lightcraft_engine::media::{RenderJob, RenderResult, SourceLevel};
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

/// Collects the jobs the UI asks for without running them; hands back the results put in `.1`.
#[derive(Clone, Default)]
#[allow(clippy::type_complexity)]
struct Capture(std::rc::Rc<std::cell::RefCell<Vec<(Slot, RenderJob)>>>, std::rc::Rc<std::cell::RefCell<Vec<(Slot, RenderResult, f64)>>>);
impl crate::render::RenderOffload for Capture {
    fn try_start(&mut self, slot: Slot, job: RenderJob) -> Option<RenderJob> {
        self.0.borrow_mut().push((slot, job));
        None
    }
    fn finished(&mut self) -> Vec<(Slot, RenderResult, f64)> {
        std::mem::take(&mut *self.1.borrow_mut())
    }
}

impl Capture {
    fn last(&self, slot: Slot) -> Option<RenderJob> {
        self.0.borrow().iter().rev().find(|(s, _)| *s == slot).map(|(_, j)| j.clone())
    }
}

/// A Develop loupe (1600 × 1000 points at `ppp`) on an A1-sized photo (8640 × 5760, `settings`),
/// rendering through `Capture`.
fn loupe(settings: DevelopSettings, zoom: Zoom, ppp: f32) -> (Headless, Capture) {
    let mut session = lightcraft_engine::Session::new();
    session.media.file_loader = Some(Arc::new(|_: &str, _| Ok((li_raster::RgbFrame::filled(864, 576, [0.2, 0.3, 0.4]), Default::default()))));
    let id = PhotoId(1);
    let mut p = Photo::new(id, Source::File { path: "a1.tif".into() }, "a1.tif", "TIFF", 8640, 5760, "2026-10-24");
    p.develop = Arc::new(settings);
    session.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    session.selection = lightcraft_engine::Selection::single(id);
    let mut app = LightcraftApp::new(session, crate::Services { png: None, ..Default::default() });
    app.ui.view = crate::state::ViewMode::Detail;
    app.ui.right = crate::state::RightPanel::Edit;
    app.ui.left_panel = false;
    app.ui.filmstrip = false;
    app.ui.zoom = zoom;
    if !app.ui.section_open("light") {
        app.ui.toggle_section("light");
    }
    let work = Capture::default();
    app.renderer.set_offload(Box::new(work.clone()));
    let mut h = Headless::new(app, [1600.0, 1000.0], ppp);
    for _ in 0..3 {
        h.step();
    }
    (h, work)
}

#[test]
fn a_zoomed_in_loupe_renders_the_frame_at_its_native_pixels() {
    let (mut h, work) = loupe(DevelopSettings::default(), Zoom::Percent(100), 1.0);
    let main = work.last(Slot::Main).expect("the loupe render");
    assert!(main.request.max_w.max(main.request.max_h) <= 2560, "the loupe render keeps its preview size");
    let detail = work.last(Slot::Detail).expect("a 1:1 render");
    assert_eq!((detail.request.max_w, detail.request.max_h), (8640, 5760), "one output pixel per screen pixel at 100%");
    assert_eq!(detail.level, SourceLevel::Full, "demosaiced from the original, not binned");
    assert_eq!(detail.request.quality, lightcraft_engine::pipeline::Quality::Full);
    assert!(detail.view_cache.is_none() && detail.cache.is_none(), "a 1:1 render never replaces the cached previews");

    // a drag draws Main's drafts; the 1:1 render waits for the release
    work.0.borrow_mut().clear();
    h.app.session.execute("develop.beginInteraction", &json!({"label": "Exposure"})).unwrap();
    h.app.session.execute("develop.set", &json!({"control": "light.exposure", "value": 0.5})).unwrap();
    h.step();
    assert!(work.last(Slot::Main).is_some_and(|j| j.request.quality == lightcraft_engine::pipeline::Quality::Draft));
    assert!(work.last(Slot::Detail).is_none(), "no 1:1 render during a drag");
    h.app.session.execute("develop.endInteraction", &json!({})).unwrap();
    h.step();
    assert!(work.last(Slot::Detail).is_some_and(|j| j.settings.light.exposure == 0.5), "the release renders 1:1 again");
}

#[test]
fn the_1_to_1_render_is_drawn_once_it_is_current() {
    let (mut h, work) = loupe(DevelopSettings::default(), Zoom::Percent(100), 1.0);
    let small = |j: RenderJob| RenderJob { request: lightcraft_engine::pipeline::RenderRequest { max_w: 64, max_h: 64, ..j.request.clone() }, ..j };
    for slot in [Slot::Main, Slot::Detail] {
        // (rendered small: what is drawn depends on the job, not on the pixels)
        let result = small(work.last(slot).unwrap()).run();
        work.1.borrow_mut().push((slot, result, 1.0));
    }
    h.step();
    h.step();
    assert_eq!(h.app.loupe_shown, Some((PhotoId(1), "detail")));
    h.app.session.execute("develop.set", &json!({"control": "light.exposure", "value": 0.3})).unwrap();
    h.step();
    assert_eq!(h.app.loupe_shown, Some((PhotoId(1), "render")), "an edit shows the loupe render until the new 1:1 render is in");
}

#[test]
fn a_cropped_frame_renders_1_to_1_at_its_own_size_and_fit_never_does() {
    let mut cropped = DevelopSettings::default();
    cropped.crop.geometry.rect = lightcraft_geom::Rect::new(0.25, 0.25, 0.75, 0.75);
    let (_h, work) = loupe(cropped, Zoom::Percent(100), 1.0);
    let detail = work.last(Slot::Detail).expect("a 1:1 render");
    assert_eq!((detail.request.max_w, detail.request.max_h), (4320, 2880), "the crop's own pixels");
    let (_h, work) = loupe(DevelopSettings::default(), Zoom::Fit, 2.0);
    assert!(work.last(Slot::Detail).is_none(), "Fit keeps the preview size the settings ask for");
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
