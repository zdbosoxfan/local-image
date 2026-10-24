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
use crate::panels::detail::{fit_rect, native_px, pixel_rect};
use crate::render::Slot;
use crate::state::{BeforeAfter, Zoom};

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

type Shared<T> = std::rc::Rc<std::cell::RefCell<Vec<T>>>;

/// Collects the jobs the UI asks for without running them; hands back the results put in `.1`.
/// Takes the loupe's full-size renders, as the GPU does.
#[derive(Clone, Default)]
struct Capture(Shared<(Slot, RenderJob)>, Shared<(Slot, RenderResult, f64)>);
impl crate::render::RenderOffload for Capture {
    fn try_start(&mut self, slot: Slot, job: RenderJob) -> Option<RenderJob> {
        self.0.borrow_mut().push((slot, job));
        None
    }
    fn finished(&mut self) -> Vec<(Slot, RenderResult, f64)> {
        std::mem::take(&mut *self.1.borrow_mut())
    }
    fn full_size(&self) -> bool {
        true
    }
}

impl Capture {
    fn last(&self, slot: Slot) -> Option<RenderJob> {
        self.0.borrow().iter().rev().find(|(s, _)| *s == slot).map(|(_, j)| j.clone())
    }

    /// Finish the last job asked for `slot` (rendered small unless `full`: what is drawn depends
    /// on the job, not on the pixels).
    fn finish(&self, h: &mut Headless, slot: Slot, full: bool) {
        let job = self.last(slot).expect("a job to finish");
        let job = if full {
            job
        } else {
            RenderJob { request: lightcraft_engine::pipeline::RenderRequest { max_w: 64, max_h: 64, ..job.request.clone() }, ..job }
        };
        self.1.borrow_mut().push((slot, job.run(), 1.0));
        h.step();
        h.step();
    }
}

/// A Develop loupe (1600 × 1000 points at `ppp`) on an A1-sized photo (8640 × 5760, `settings`),
/// rendering through `Capture`.
fn loupe(settings: DevelopSettings, zoom: Zoom, ppp: f32) -> (Headless, Capture) {
    loupe_of([8640, 5760], [864, 576], settings, zoom, ppp)
}

/// A Develop loupe on a photo `size` pixels large whose original decodes to `source` pixels.
fn loupe_of(size: [u32; 2], source: [usize; 2], settings: DevelopSettings, zoom: Zoom, ppp: f32) -> (Headless, Capture) {
    let mut session = lightcraft_engine::Session::new();
    session.media.file_loader =
        Some(Arc::new(move |_: &str, _| Ok((li_raster::RgbFrame::filled(source[0], source[1], [0.2, 0.3, 0.4]), Default::default()))));
    let id = PhotoId(1);
    let mut p = Photo::new(id, Source::File { path: "a1.tif".into() }, "a1.tif", "TIFF", size[0], size[1], "2026-10-24");
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
    assert!(work.last(Slot::Detail).is_none(), "the 1:1 render waits for the loupe render of the photo");
    work.finish(&mut h, Slot::Main, false);
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
    work.finish(&mut h, Slot::Main, false);
    assert!(work.last(Slot::Detail).is_some_and(|j| j.settings.light.exposure == 0.5), "the release renders 1:1 again");
}

#[test]
fn the_1_to_1_render_is_drawn_once_it_is_current() {
    let (mut h, work) = loupe(DevelopSettings::default(), Zoom::Percent(100), 1.0);
    work.finish(&mut h, Slot::Main, false);
    work.finish(&mut h, Slot::Detail, false);
    assert_eq!(h.app.loupe_shown, Some((PhotoId(1), "detail")));
    h.app.session.execute("develop.set", &json!({"control": "light.exposure", "value": 0.3})).unwrap();
    h.step();
    assert_eq!(h.app.loupe_shown, Some((PhotoId(1), "render")), "an edit shows the loupe render until the new 1:1 render is in");
    assert!(!h.app.renderer.is_pending(Slot::Detail), "no 1:1 render of the old settings is waited for");
}

#[test]
fn a_cropped_frame_renders_1_to_1_at_its_own_size_and_fit_never_does() {
    let mut cropped = DevelopSettings::default();
    cropped.crop.geometry.rect = lightcraft_geom::Rect::new(0.25, 0.25, 0.75, 0.75);
    let (mut h, work) = loupe(cropped, Zoom::Percent(100), 1.0);
    work.finish(&mut h, Slot::Main, false);
    let detail = work.last(Slot::Detail).expect("a 1:1 render");
    assert_eq!((detail.request.max_w, detail.request.max_h), (4320, 2880), "the crop's own pixels");
    let (_h, work) = loupe(DevelopSettings::default(), Zoom::Fit, 2.0);
    assert!(work.last(Slot::Detail).is_none(), "Fit keeps the preview size the settings ask for");
}

/// A crop smaller than the loupe render: 1728 × 1152 of an 8640 × 5760 frame fits the loupe's
/// 2560 px render, but that render's binned preview source has only 512 pixels across it.
#[test]
fn a_small_crop_at_100_percent_is_rendered_from_the_original() {
    let mut cropped = DevelopSettings::default();
    cropped.crop.geometry.rect = lightcraft_geom::Rect::new(0.4, 0.4, 0.6, 0.6);
    let (mut h, work) = loupe(cropped, Zoom::Percent(100), 1.0);
    let main = work.last(Slot::Main).unwrap();
    assert_eq!((main.request.max_w, main.request.max_h, main.level), (1728, 1152, SourceLevel::Preview));
    work.finish(&mut h, Slot::Main, false);
    let detail = work.last(Slot::Detail).expect("a 1:1 render of the crop from the original");
    assert_eq!((detail.request.max_w, detail.request.max_h, detail.level), (1728, 1152, SourceLevel::Full));
    work.finish(&mut h, Slot::Detail, false);
    assert_eq!(h.app.loupe_shown, Some((PhotoId(1), "detail")));
}

#[test]
fn soft_proofing_and_before_views_skip_the_1_to_1_render() {
    let (mut h, work) = loupe(DevelopSettings::default(), Zoom::Percent(100), 1.0);
    work.finish(&mut h, Slot::Main, false);
    assert!(work.last(Slot::Detail).is_some());
    for before in [BeforeAfter::Original, BeforeAfter::SideBySide] {
        work.0.borrow_mut().clear();
        h.app.ui.before_after = before;
        h.step();
        assert!(work.last(Slot::Detail).is_none(), "{before:?}");
    }
    h.app.ui.before_after = BeforeAfter::Off;
    h.app.ui.soft_proof = true;
    work.0.borrow_mut().clear();
    h.step();
    work.finish(&mut h, Slot::Main, false);
    assert!(work.last(Slot::Detail).is_none(), "a soft proof renders on the CPU: no 1:1 render");
}

/// The 1:1 render is drawn texel for physical pixel: its rect starts on the pixel grid and spans
/// exactly its own pixels (at 200%, two screen pixels each), sampled without blending neighbours,
/// also for a crop whose aspect leaves the frame a fraction of a pixel off its rendered size.
#[test]
fn the_1_to_1_render_is_drawn_pixel_for_pixel() {
    let mut cropped = DevelopSettings::default();
    cropped.crop.geometry.rect = lightcraft_geom::Rect::new(0.0, 0.0, 0.7777, 0.9);
    for (ppp, zoom, scale, settings) in [
        (1.0, 100, 1.0, DevelopSettings::default()),
        (2.0, 100, 1.0, DevelopSettings::default()),
        (1.0, 200, 2.0, DevelopSettings::default()),
        (1.0, 100, 1.0, cropped),
    ] {
        let (mut h, work) = loupe_of([3000, 2000], [3000, 2000], settings, Zoom::Percent(zoom), ppp);
        h.app.ui.settings.preview_edge = 1600;
        // a click away from the centre: the frame's corner falls between pixels
        h.app.ui.pan = (0.3701, 0.6103);
        h.step();
        work.finish(&mut h, Slot::Main, false);
        work.finish(&mut h, Slot::Detail, true);
        let case = format!("{ppp}x {zoom}% {:?}", h.app.image_rect);
        assert_eq!(h.app.loupe_shown, Some((PhotoId(1), "detail")), "{case}");
        let tex = h.app.renderer.textures.get(&Slot::Detail).unwrap();
        let (id, size) = (tex.tex.id(), tex.size);
        let meta = h.view.ctx.tex_manager().read().meta(id).map(|m| m.options);
        assert_eq!(meta.map(|o| o.magnification), Some(egui::TextureFilter::Nearest), "{case}: texels are not blended");
        let drawn = h
            .view
            .shapes()
            .iter()
            .find_map(|c| match &c.shape {
                egui::epaint::Shape::Mesh(m) if m.texture_id == id => {
                    Some(m.vertices.iter().fold(Rect::NOTHING, |r, v| r.union(Rect::from_min_max(v.pos, v.pos))))
                }
                _ => None,
            })
            .expect("the 1:1 texture is drawn");
        let px = |v: f32| v * ppp;
        assert!([drawn.min.x, drawn.min.y].iter().all(|v| (px(*v) - px(*v).round()).abs() < 1e-3), "{case}: on the pixel grid: {drawn:?}");
        let want = [size[0] as f32 * scale, size[1] as f32 * scale];
        assert!((px(drawn.width()) - want[0]).abs() < 1e-2 && (px(drawn.height()) - want[1]).abs() < 1e-2, "{case}: {drawn:?} for {size:?}");
    }
}

#[test]
fn pixel_rects_need_a_whole_number_of_pixels_per_texel() {
    let img = Rect::from_min_size(pos2(10.3, 20.6), vec2(1500.2, 1000.1));
    let r = pixel_rect(img, [3000, 2000], 2.0).unwrap();
    assert_eq!((r.min, r.size()), (pos2(10.5, 20.5), vec2(1500.0, 1000.0)));
    assert!(pixel_rect(img, [2400, 1600], 2.0).is_none(), "1.25 pixels per texel is resampled");
    assert_eq!(pixel_rect(img, [1500, 1000], 2.0).unwrap().size(), vec2(1500.0, 1000.0), "two pixels per texel");
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

/// Sync gives the other photos the Upright mode at once; the frame loop stores each one's own
/// analysis when the background job is done, as one undo step with the Sync.
#[test]
fn the_frame_loop_stores_synced_upright_analyses() {
    let mut session = lightcraft_engine::Session::new();
    session.media.file_loader = Some(Arc::new(|_: &str, _| Ok((li_raster::RgbFrame::filled(96, 64, [0.2, 0.3, 0.4]), Default::default()))));
    for id in [PhotoId(1), PhotoId(2)] {
        let name = format!("p{}.tif", id.0);
        let p = Photo::new(id, Source::File { path: name.clone() }, &name, "TIFF", 96, 64, "2026-10-24");
        session.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    session.execute("library.select", &json!({"ids": [1, 2], "active": 1})).unwrap();
    let mut app = LightcraftApp::new(session, crate::Services { png: None, ..Default::default() });
    app.ui.view = crate::state::ViewMode::Detail;
    app.renderer.set_offload(Box::new(Capture::default()));
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    h.step();
    h.app.session.execute("geometry.upright", &json!({"mode": "vertical"})).unwrap();
    let steps = h.app.session.undo.len();
    h.app.session.execute("develop.sync", &json!({"groups": ["geometry"]})).unwrap();
    assert!(h.app.session.develop_of(PhotoId(2)).unwrap().upright_unsolved());
    let stored = h.step_until(T, |h| h.app.session.develop_of(PhotoId(2)).unwrap().geometry.upright_transform.is_some());
    assert!(stored, "the analysis is stored by the frame loop");
    assert_eq!(h.app.session.undo.len(), steps + 1, "with the Sync, as one step");
}
