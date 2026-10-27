use super::*;
use lightcraft_catalog::{Catalog, Op, Photo, Source};

struct Blocked;
impl RenderOffload for Blocked {
    fn try_start(&mut self, _: Slot, job: RenderJob) -> Option<RenderJob> {
        Some(job)
    }
    fn finished(&mut self) -> Vec<(Slot, RenderResult, f64)> {
        vec![]
    }
}
fn photo() -> Arc<Photo> {
    Arc::new(Photo::new(PhotoId(1), Source::File { path: "audit.jpg".into() }, "audit.jpg", "JPEG", 6000, 4000, "2026-01-01"))
}
#[test]
fn snapshot_tracks_cow_edits_undo_reload_and_replacement() {
    let mut catalog = Catalog::default();
    catalog.apply(Op::AddPhoto { photo: Box::new((*photo()).clone()) }).unwrap();
    let old = catalog.photo(PhotoId(1)).unwrap().clone();
    let mut r = Renderer::default();
    r.remember_thumb(&old, 384, 77, false);
    r.pending.insert(Slot::Thumb(PhotoId(1)), (77, 10));
    assert!(r.thumb_current(&old, 384, 10));
    assert!(!r.thumb_current(&old, 512, 10));
    let mut settings = lightcraft_develop::DevelopSettings::default();
    settings.light.exposure = 1.0;
    let undo = catalog.apply(Op::SetDevelop { id: PhotoId(1), settings: Arc::new(settings), label: "edit".into(), edited: None }).unwrap();
    assert!(!r.thumb_current(catalog.photo(PhotoId(1)).unwrap(), 384, 10));
    catalog.apply(undo).unwrap();
    assert!(!r.thumb_current(catalog.photo(PhotoId(1)).unwrap(), 384, 10));
    catalog
        .apply(Op::SetContent { id: PhotoId(1), width: 6000, height: 4000, file_size: 99, content_hash: Some("changed".into()), preview_only: None })
        .unwrap();
    assert!(!r.thumb_current(catalog.photo(PhotoId(1)).unwrap(), 384, 10));
    assert!(!r.thumb_current(&photo(), 384, 10));
    r.forget_all();
    assert!(r.thumb_inputs.is_empty());
}
#[test]
fn queue_promotions_running_work_and_cancellation() {
    let p = photo();
    let slot = Slot::Thumb(p.id);
    let mut session = Session::new();
    session.catalog.apply(Op::AddPhoto { photo: Box::new((*p).clone()) }).unwrap();
    let job = session.thumb_job(p.id, 384).unwrap();
    let key = job.key;
    let mut r = Renderer::default();
    r.set_offload(Box::new(Blocked));
    r.remember_thumb(&p, 384, key, false);
    r.request(slot, job, 5);
    assert!(r.thumb_current(&p, 384, 5));
    assert!(!r.thumb_current(&p, 384, 10));
    r.request(slot, session.thumb_job(p.id, 384).unwrap(), 10);
    assert_eq!(r.pending.get(&slot), Some(&(key, 10)));
    r.queue.clear(); // running work: cannot reprioritize, just as Renderer::request
    assert!(r.thumb_current(&p, 384, 5));
    r.pending.clear();
    assert!(!r.thumb_current(&p, 384, 10));
    r.evict_thumbs(&Default::default(), 600);
    assert!(r.thumb_inputs.is_empty());
}
#[test]
fn failures_retry_after_revision_and_quick_only_is_not_final() {
    let p = photo();
    let slot = Slot::Thumb(p.id);
    let mut r = Renderer::default();
    r.remember_thumb(&p, 384, 77, false);
    r.pending.insert(Slot::ThumbQuick(p.id), (77, 11));
    assert!(!r.thumb_current(&p, 384, 10));
    r.failed.insert(slot, (77, "no decoder".into(), 0));
    assert!(r.thumb_current(&p, 384, 10));
    r.catalog_rev = 1;
    assert!(!r.thumb_current(&p, 384, 10));
}

#[derive(Default)]
struct Work {
    jobs: Vec<(Slot, RenderJob)>,
    finished: Vec<(Slot, RenderResult, f64)>,
}
struct Manual(std::rc::Rc<std::cell::RefCell<Work>>);
impl RenderOffload for Manual {
    fn try_start(&mut self, slot: Slot, job: RenderJob) -> Option<RenderJob> {
        self.0.borrow_mut().jobs.push((slot, job));
        None
    }
    fn finished(&mut self) -> Vec<(Slot, RenderResult, f64)> {
        std::mem::take(&mut self.0.borrow_mut().finished)
    }
}
fn fixture() -> (crate::LightcraftApp, std::rc::Rc<std::cell::RefCell<Work>>, egui::Context) {
    fixture_with(Session::new(), (*photo()).clone())
}
fn fixture_with(s: Session, photo: Photo) -> (crate::LightcraftApp, std::rc::Rc<std::cell::RefCell<Work>>, egui::Context) {
    fixture_filled(s, photo, [0.3, 0.2, 0.1])
}
fn fixture_filled(mut s: Session, photo: Photo, fill: [f32; 3]) -> (crate::LightcraftApp, std::rc::Rc<std::cell::RefCell<Work>>, egui::Context) {
    s.media.file_loader = Some(Arc::new(move |_, _| Ok((li_raster::RgbFrame::filled(8, 8, fill), Default::default()))));
    s.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    let mut app = crate::LightcraftApp::new(s, crate::Services::default());
    app.renderer.keep_pixels = true;
    let work = std::rc::Rc::new(std::cell::RefCell::new(Work::default()));
    app.renderer.set_offload(Box::new(Manual(work.clone())));
    let ctx = egui::Context::default();
    app.renderer.poll(&ctx, &mut app.session);
    (app, work, ctx)
}
fn take_job(work: &std::rc::Rc<std::cell::RefCell<Work>>) -> (Slot, RenderJob) {
    work.borrow_mut().jobs.pop().unwrap()
}
fn finish(app: &mut crate::LightcraftApp, work: &std::rc::Rc<std::cell::RefCell<Work>>, ctx: &egui::Context, slot: Slot, result: RenderResult) {
    work.borrow_mut().finished.push((slot, result, 0.0));
    app.renderer.poll(ctx, &mut app.session);
}
fn request(app: &mut crate::LightcraftApp) {
    crate::panels::grid::request_thumb(app, PhotoId(1), 256, 10);
}

#[test]
fn ds2_missing_thumbnail_renders_committed_settings_once_during_interaction() {
    let (mut app, work, ctx) = fixture();
    app.session.execute("library.select", &serde_json::json!({"ids":[1],"active":1})).unwrap();
    app.session.begin_interaction("Exposure").unwrap();
    let original = app.session.interaction.as_ref().unwrap().original.clone();
    let mut edited = (*original).clone();
    edited.light.exposure = 2.0;
    app.session.set_develop(PhotoId(1), edited, "Exposure").unwrap();
    request(&mut app);
    let (slot, job) = take_job(&work);
    assert_eq!(*job.settings, *original);
    assert!(job.cache.is_none());
    finish(&mut app, &work, &ctx, slot, job.run());
    assert!(app.renderer.thumb(PhotoId(1)).is_some());
    for _ in 0..30 {
        request(&mut app);
    }
    assert!(work.borrow().jobs.is_empty());
}

#[test]
fn unchanged_ready_and_running_frames_build_no_jobs() {
    let (mut app, work, ctx) = fixture();
    request(&mut app);
    assert_eq!(app.renderer.thumb_jobs_built, 1);
    for _ in 0..180 {
        request(&mut app);
    }
    assert_eq!(app.renderer.thumb_jobs_built, 1);
    let (slot, job) = take_job(&work);
    finish(&mut app, &work, &ctx, slot, job.run());
    for _ in 0..180 {
        request(&mut app);
    }
    assert_eq!(app.renderer.thumb_jobs_built, 1);
    assert!(work.borrow().jobs.is_empty());
}

#[test]
fn unchanged_queued_frames_build_no_jobs() {
    let (mut app, _, _) = fixture();
    app.renderer.set_offload(Box::new(Blocked));
    request(&mut app);
    for _ in 0..180 {
        request(&mut app);
    }
    assert_eq!(app.renderer.thumb_jobs_built, 1);
    assert_eq!(app.renderer.queue.len(), 1);
}

/// A full-size render waits for a heavy-job permit while exports or preview builds hold them
/// all, and a cancel (the loupe left 100%, the photo was edited) stops it before it decodes.
#[test]
fn full_size_renders_wait_for_their_permit_and_stop_when_cancelled() {
    let mut s = Session::new();
    s.media.file_loader = Some(Arc::new(|_, _| Ok((li_raster::RgbFrame::filled(8, 8, [0.3, 0.2, 0.1]), Default::default()))));
    s.catalog.apply(Op::AddPhoto { photo: Box::new((*photo()).clone()) }).unwrap();
    let mut app = crate::LightcraftApp::new(s, crate::Services::default());
    let ctx = egui::Context::default();
    let permits = lightcraft_sysmem::heavy_jobs();
    let held: Vec<_> = (0..permits.max_jobs()).filter_map(|_| permits.admit(1, &|| false)).collect();
    let job = app.session.detail_job(PhotoId(1), 64, 64, true).unwrap();
    app.renderer.request_admitted(Slot::Detail, job, 95, 1 << 20);
    std::thread::sleep(std::time::Duration::from_millis(300));
    app.renderer.poll(&ctx, &mut app.session);
    assert!(app.renderer.is_pending(Slot::Detail) && !app.renderer.textures.contains_key(&Slot::Detail), "waits for a permit");
    app.renderer.cancel(Slot::Detail);
    assert!(!app.renderer.is_pending(Slot::Detail));
    drop(held);
    let job = app.session.detail_job(PhotoId(1), 64, 64, true).unwrap();
    let key = job.key;
    app.renderer.request_admitted(Slot::Detail, job, 95, 1 << 20);
    let t0 = std::time::Instant::now();
    while !app.renderer.textures.contains_key(&Slot::Detail) && t0.elapsed() < std::time::Duration::from_secs(60) {
        std::thread::sleep(std::time::Duration::from_millis(10));
        app.renderer.poll(&ctx, &mut app.session);
    }
    assert_eq!(app.renderer.textures.get(&Slot::Detail).map(|t| t.key), Some(key), "runs once a permit is free");
    assert!(app.renderer.failure(Slot::Detail).is_none(), "the cancelled request left no failure behind");
}

#[test]
fn forgetting_views_cancels_admitted_renders() {
    let mut session = Session::new();
    session.catalog.apply(Op::AddPhoto { photo: Box::new((*photo()).clone()) }).unwrap();
    let job = session.detail_job(PhotoId(1), 64, 64, true).unwrap();
    let mut renderer = Renderer::default();
    renderer.request_admitted(Slot::Detail, job, 95, 1 << 20);
    let stop = renderer.admitted[&Slot::Detail].clone();
    renderer.forget_all();
    assert!(stop.load(Ordering::Relaxed), "obsolete views must stop waiting for a heavy-job permit");
    assert!(renderer.admitted.is_empty(), "obsolete cancellation handles must be released");
    assert!(!renderer.is_pending(Slot::Detail));
}

/// A full-size render takes the GPU in turn with exports and preview builds (while one holds
/// it, the render waits), and hands back the original it decoded, kept for the next one.
#[test]
fn full_size_renders_take_their_turn_on_the_gpu() {
    let mut s = Session::new();
    s.media.file_loader = Some(Arc::new(|_, _| Ok((li_raster::RgbFrame::filled(8, 8, [0.3, 0.2, 0.1]), Default::default()))));
    s.catalog.apply(Op::AddPhoto { photo: Box::new((*photo()).clone()) }).unwrap();
    let mut app = crate::LightcraftApp::new(s, crate::Services::default());
    assert!(lightcraft_engine::gpu::available(), "real GPU required: {:?}", lightcraft_engine::gpu::unavailable_reason());
    let ctx = egui::Context::default();
    let export = lightcraft_engine::export::pipeline::GPU_GATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let job = app.session.detail_job(PhotoId(1), 64, 64, true).unwrap();
    let key = job.key;
    app.renderer.request_admitted(Slot::Detail, job, 95, 1 << 20);
    std::thread::sleep(std::time::Duration::from_millis(300));
    app.renderer.poll(&ctx, &mut app.session);
    assert!(app.renderer.is_pending(Slot::Detail) && !app.renderer.textures.contains_key(&Slot::Detail), "waits while an export renders");
    drop(export);
    let t0 = std::time::Instant::now();
    while !app.renderer.textures.contains_key(&Slot::Detail) && t0.elapsed() < std::time::Duration::from_secs(60) {
        std::thread::sleep(std::time::Duration::from_millis(10));
        app.renderer.poll(&ctx, &mut app.session);
    }
    assert_eq!(app.renderer.textures.get(&Slot::Detail).map(|t| t.key), Some(key), "renders once the GPU is free");
    assert_eq!(app.session.media.usage().2.count, 1, "the decoded original is kept");
}

#[test]
fn oversized_images_fit_the_largest_texture() {
    let result = |image: li_raster::Rgba8| RenderResult {
        request_id: 0,
        source_key: None,
        photo: PhotoId(1),
        level: lightcraft_engine::media::SourceLevel::Thumb,
        key: 0,
        rendered: Ok(lightcraft_engine::pipeline::Rendered { histogram: Histogram::of_srgb8(&image), image, deep: None }),
        loaded: None,
        quick: None,
        proof_monitor: None,
    };
    let fitted = fit_texture(result(li_raster::Rgba8::filled(9000, 300, [200, 100, 50, 255])), 4096).rendered.unwrap().image;
    assert_eq!((fitted.width, fitted.height), (4096, 137));
    assert_eq!(fitted.get(2000, 60), [200, 100, 50, 255]);
    let small = fit_texture(result(li_raster::Rgba8::filled(300, 200, [1, 2, 3, 255])), 4096).rendered.unwrap().image;
    assert_eq!((small.width, small.height), (300, 200));
}

#[test]
fn proof_monitor_pixels_skip_second_display_conversion() {
    let (mut app, _, _) = fixture();
    let mut color = photocraft_engine::color_cmds::ColorState::default();
    color.settings.monitor_profile = "display-p3".into();
    let display = color.srgb_preview_display().unwrap();
    app.renderer.set_display(display.clone());
    let proof = lightcraft_pipeline::Proof { monitor: app.renderer.proof_monitor(), ..Default::default() };
    let job = app.session.render_job(PhotoId(1), 64, 64, false, true).unwrap().with_proof(Some(proof));
    let result = job.run();
    assert_eq!(result.proof_monitor, Some(display.monitor.content_hash()));
    let bytes = result.rendered.as_ref().unwrap().image.as_bytes();
    let twice = display.rgb8_to_display(result.rendered.as_ref().unwrap().image.width, &bytes);
    assert_ne!(bytes, twice, "sample detects a second monitor transform");
    let converted = display_result(result, Some(&display), display.key);
    assert_eq!(converted.color.unwrap().pixels.iter().flat_map(|p| p.to_array()).collect::<Vec<_>>(), bytes);
}

#[test]
fn display_textures_match_shared_editor_transform_and_rebuild_on_monitor_change() {
    let (mut app, work, ctx) = fixture();
    let mut color = photocraft_engine::color_cmds::ColorState::default();
    color.settings.monitor_profile = "display-p3".into();
    let display = color.srgb_preview_display().unwrap();
    app.renderer.set_display(display.clone());
    request(&mut app);
    let (slot, job) = take_job(&work);
    let result = job.run();
    let img = &result.rendered.as_ref().unwrap().image;
    let expected = display.rgb8_to_display(img.width, &img.as_bytes());
    finish(&mut app, &work, &ctx, slot, result);
    let pixels = app.renderer.thumb(PhotoId(1)).unwrap().pixels.as_ref().unwrap();
    assert_eq!(pixels.pixels.iter().flat_map(|p| p.to_array()).collect::<Vec<_>>(), expected);
    color.settings.monitor_profile = "srgb".into();
    app.renderer.set_display(color.srgb_preview_display().unwrap());
    assert!(app.renderer.thumb(PhotoId(1)).is_none());
    request(&mut app);
    let (slot, job) = take_job(&work);
    let result = job.run();
    let original = result.rendered.as_ref().unwrap().image.as_bytes();
    finish(&mut app, &work, &ctx, slot, result);
    assert_eq!(
        app.renderer.thumb(PhotoId(1)).unwrap().pixels.as_ref().unwrap().pixels.iter().flat_map(|p| p.to_array()).collect::<Vec<_>>(),
        original
    );
}

#[test]
fn wide_gamut_monitor_views_show_exactly_the_export_in_its_space() {
    use lightcraft_pipeline::{OutputDepth, OutputSpace};
    // scene-linear Rec.2020 green inside Display P3 and outside sRGB
    let (mut app, work, ctx) = fixture_filled(Session::new(), (*photo()).clone(), [0.10, 0.45, 0.05]);
    let mut color = photocraft_engine::color_cmds::ColorState::default();
    color.settings.monitor_profile = "display-p3".into();
    let display = color.srgb_preview_display().unwrap();
    app.renderer.set_display(display.clone());
    let job = app.session.loupe_job(PhotoId(1), 8, 8, true).unwrap();
    let (srgb_key, srgb_view) = (job.key, job.view_cache.as_ref().unwrap().1);
    app.renderer.request(Slot::Main, job, 100);
    let (slot, job) = take_job(&work);
    assert_eq!(job.request.space, OutputSpace::DisplayP3);
    assert_ne!(job.key, srgb_key, "a P3 view never passes for the sRGB render");
    assert_ne!(job.view_cache.as_ref().unwrap().1, srgb_view, "P3 pixels never pass for the sRGB view preview");
    finish(&mut app, &work, &ctx, slot, job.run());
    let shown: Vec<u8> = app.renderer.textures[&Slot::Main].pixels.as_ref().unwrap().pixels.iter().flat_map(|p| p.to_array()).collect();
    let export = app.session.render_export(PhotoId(1), 8, 8, OutputSpace::DisplayP3, OutputDepth::U8).unwrap().image.as_bytes();
    assert_eq!(shown.len(), export.len());
    let worst = shown.iter().zip(&export).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
    assert!(worst <= 1, "the loupe shows the P3 export (worst difference {worst} codes)");
    // the sRGB render clips this green; converted to the monitor it carries > 10 codes more red
    let srgb = app.session.render_job(PhotoId(1), 8, 8, false, true).unwrap().run().rendered.unwrap().image;
    let clipped = display.rgb8_to_display(srgb.width, &srgb.as_bytes());
    let red = |b: &[u8]| b.as_chunks::<4>().0.iter().map(|p| f32::from(p[0])).sum::<f32>() / (b.len() / 4) as f32;
    assert!(red(&clipped) - red(&export) > 10.0, "sRGB clipping added red: {} vs {}", red(&clipped), red(&export));
    // the other views follow; thumbnails, stand-ins and soft proofs do not
    let compare = app.session.render_job(PhotoId(1), 8, 8, false, true).unwrap();
    app.renderer.request(Slot::Compare(0), compare, 60);
    assert_eq!(take_job(&work).1.request.space, OutputSpace::DisplayP3);
    app.renderer.request(Slot::Thumb(PhotoId(1)), app.session.thumb_job(PhotoId(1), 256).unwrap(), 10);
    assert_eq!(take_job(&work).1.request.space, OutputSpace::Srgb);
    let proof = lightcraft_pipeline::Proof { monitor: app.renderer.proof_monitor(), ..Default::default() };
    let proofed = app.session.loupe_job(PhotoId(1), 8, 8, true).unwrap().with_proof(Some(proof));
    app.renderer.request(Slot::Main, proofed, 100);
    assert_eq!(take_job(&work).1.request.space, OutputSpace::Srgb);
    // an sRGB monitor keeps today's sRGB views and their cache
    color.settings.monitor_profile = "srgb".into();
    app.renderer.set_display(color.srgb_preview_display().unwrap());
    app.renderer.request(Slot::Main, app.session.loupe_job(PhotoId(1), 8, 8, true).unwrap(), 100);
    let (_, job) = take_job(&work);
    assert_eq!((job.request.space, job.key, job.view_cache.unwrap().1), (OutputSpace::Srgb, srgb_key, srgb_view));
}

/// Poll until `done` holds (stand-ins and prefetches run on the renderer's own pool).
fn poll_until(app: &mut crate::LightcraftApp, ctx: &egui::Context, done: impl Fn(&Renderer) -> bool) {
    let start = std::time::Instant::now();
    while !done(&app.renderer) {
        assert!(start.elapsed() < std::time::Duration::from_secs(60), "render pool timed out");
        std::thread::sleep(std::time::Duration::from_millis(2));
        app.renderer.poll(ctx, &mut app.session);
    }
}

#[test]
fn wide_gamut_loupe_stand_ins_show_the_view_they_stand_in_for() {
    // stepping onto a prefetched neighbour on a P3 monitor: its stand-in is its cached view
    // render, shown exactly as the view that replaces it (no colour shift when it lands)
    let (mut app, work, ctx) = fixture_filled(Session::new(), (*photo()).clone(), [0.10, 0.45, 0.05]);
    let mut color = photocraft_engine::color_cmds::ColorState::default();
    color.settings.monitor_profile = "display-p3".into();
    app.renderer.set_display(color.srgb_preview_display().unwrap());
    app.renderer.prefetch(Slot::Prefetch(0), app.session.loupe_job(PhotoId(1), 8, 8, true).unwrap(), 1);
    poll_until(&mut app, &ctx, |r| !r.is_pending(Slot::Prefetch(0)));
    app.renderer.request_quick(Slot::Preview, app.session.quick_view_job(PhotoId(1), 8, true).unwrap(), 110);
    poll_until(&mut app, &ctx, |r| r.textures.contains_key(&Slot::Preview));
    assert_eq!(app.renderer.textures[&Slot::Preview].quick, Some(QuickSource::Cached));
    app.renderer.request(Slot::Main, app.session.loupe_job(PhotoId(1), 8, 8, true).unwrap(), 100);
    let (slot, job) = take_job(&work);
    finish(&mut app, &work, &ctx, slot, job.run());
    let bytes = |s: Slot| -> Vec<u8> { app.renderer.textures[&s].pixels.as_ref().unwrap().pixels.iter().flat_map(|p| p.to_array()).collect() };
    let (stand_in, view) = (bytes(Slot::Preview), bytes(Slot::Main));
    assert_eq!(stand_in.len(), view.len());
    let worst = stand_in.iter().zip(&view).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
    assert!(worst <= 1, "the stand-in shows the view (worst difference {worst} codes)");
}

#[test]
fn resizing_the_disk_cache_keeps_textures_and_skips_rebuilds() {
    let lib = std::env::temp_dir().join(format!("lc-ui-thumb-resize-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&lib);
    let mut s = Session::new().with_fs();
    s.open_library(&lib, false).unwrap();
    let (mut app, work, ctx) = fixture_with(s, (*photo()).clone());
    request(&mut app);
    let (slot, job) = take_job(&work);
    finish(&mut app, &work, &ctx, slot, job.run());
    assert!(app.renderer.thumb(PhotoId(1)).is_some());
    let cache = app.session.media.rendered.clone();
    app.session.execute("library.preferences", &serde_json::json!({"cacheMb": 300})).unwrap();
    app.renderer.poll(&ctx, &mut app.session);
    // thumbnails are keyed by content: a new budget leaves them (and the cache) valid
    assert!(app.renderer.thumb(PhotoId(1)).is_some());
    assert!(Arc::ptr_eq(&cache, &app.session.media.rendered));
    for _ in 0..30 {
        request(&mut app);
    }
    assert_eq!(app.renderer.thumb_jobs_built, 1);
    assert!(work.borrow().jobs.is_empty());
    // an explicit clear still drops them
    app.session.execute("library.clearPreviews", &serde_json::json!({})).unwrap();
    app.renderer.poll(&ctx, &mut app.session);
    assert!(app.renderer.thumb(PhotoId(1)).is_none());
    drop(app);
    let _ = std::fs::remove_dir_all(&lib);
}

#[test]
fn raw_quick_path_frames_build_no_jobs() {
    // an unedited raw: the grid shows its embedded preview first (held here until released),
    // then queues the real thumbnail behind everything on screen
    let gate = Arc::new(std::sync::Mutex::new(()));
    let held = gate.lock().unwrap();
    let mut s = Session::new();
    let g = gate.clone();
    s.media.preview_loader = Some(Arc::new(move |_: &str, _| {
        drop(g.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        Some(li_raster::Rgba8 { width: 4, height: 4, data: vec![[200, 100, 50, 255]; 16] })
    }));
    let mut raw = (*photo()).clone();
    raw.kind = lightcraft_catalog::MediaKind::Raw;
    raw.develop = Arc::new(raw.camera_defaults());
    let (mut app, _, ctx) = fixture_with(s, raw);
    app.renderer.set_offload(Box::new(Blocked));
    let quick = Slot::ThumbQuick(PhotoId(1));
    // the stand-in is running
    for _ in 0..180 {
        request(&mut app);
    }
    assert!(app.renderer.is_pending(quick));
    assert_eq!(app.renderer.thumb_jobs_built, 1);
    drop(held);
    let t0 = std::time::Instant::now();
    while !app.renderer.textures.contains_key(&quick) {
        assert!(t0.elapsed() < std::time::Duration::from_secs(20), "the embedded preview never arrived");
        std::thread::sleep(std::time::Duration::from_millis(5));
        app.renderer.poll(&ctx, &mut app.session);
    }
    // the stand-in is shown: one more job queues the real thumbnail at background priority,
    // and the grid asking at its own (higher) priority every frame does not rebuild it
    for _ in 0..180 {
        request(&mut app);
    }
    assert_eq!(app.renderer.thumb_jobs_built, 2);
    assert_eq!(app.renderer.queue.len(), 1);
    assert_eq!(app.renderer.pending.get(&Slot::Thumb(PhotoId(1))).map(|p| p.1), Some(crate::panels::grid::BACKGROUND_THUMB_PRIORITY));
}

#[test]
fn closing_import_review_cancels_queued_and_running_identities() {
    let (mut app, work, ctx) = fixture();
    let job = app.session.thumb_job(PhotoId(1), 256).unwrap();
    app.renderer.request(Slot::Import(0), job.clone(), 10);
    let (slot, running) = take_job(&work);
    app.renderer.set_offload(Box::new(Blocked));
    app.renderer.request(Slot::Import(1), job, 10);
    app.renderer.forget_imports();
    assert!(!app.renderer.is_pending(Slot::Import(0)));
    assert!(!app.renderer.is_pending(Slot::Import(1)));
    assert!(app.renderer.queue.is_empty());
    work.borrow_mut().finished.push((slot, running.run(), 0.0));
    app.renderer.set_offload(Box::new(Manual(work.clone())));
    app.renderer.poll(&ctx, &mut app.session);
    assert!(!app.renderer.textures.contains_key(&slot));
    assert_eq!(app.session.media.source_usage().0, 0);
}

#[test]
fn loupe_shows_superseded_drafts_newer_than_its_pixels_during_a_drag() {
    let (mut app, work, ctx) = fixture();
    let mut jobs = Vec::new();
    for exposure in [0.0, 1.0, 2.0] {
        let mut edit = lightcraft_develop::DevelopSettings::default();
        edit.light.exposure = exposure;
        app.session.catalog.apply(Op::SetDevelop { id: PhotoId(1), settings: Arc::new(edit), label: "drag".into(), edited: None }).unwrap();
        let job = app.session.loupe_job(PhotoId(1), 8, 8, true).unwrap().draft();
        app.renderer.request(Slot::Main, job, 100);
        jobs.push(take_job(&work).1);
    }
    let keys: Vec<u64> = jobs.iter().map(|j| j.key).collect();
    let mut jobs = jobs.into_iter();
    let (first, second, last) = (jobs.next().unwrap(), jobs.next().unwrap(), jobs.next().unwrap());
    // the first draft lands while the newest is still rendering: the loupe moves on
    finish(&mut app, &work, &ctx, Slot::Main, first.run());
    assert_eq!(app.renderer.textures.get(&Slot::Main).map(|t| t.key), Some(keys[0]));
    assert!(app.session.media.source_usage().0 > 0, "an advancing draft also caches its decoded source");
    assert!(app.renderer.is_pending(Slot::Main));
    finish(&mut app, &work, &ctx, Slot::Main, last.run());
    assert_eq!(app.renderer.textures.get(&Slot::Main).map(|t| t.key), Some(keys[2]));
    // a draft older than the pixels shown never replaces them
    finish(&mut app, &work, &ctx, Slot::Main, second.run());
    assert_eq!(app.renderer.textures.get(&Slot::Main).map(|t| t.key), Some(keys[2]));
    assert!(!app.renderer.is_pending(Slot::Main));
}

#[test]
fn superseded_loupe_result_cannot_restore_another_photo_or_pre_reload_source() {
    for reload in [false, true] {
        let (mut app, work, ctx) = fixture();
        let old = app.session.loupe_job(PhotoId(1), 8, 8, true).unwrap().draft();
        app.renderer.request(Slot::Main, old, 100);
        let (_, old) = take_job(&work);
        let id = if reload {
            app.session
                .catalog
                .apply(Op::SetContent {
                    id: PhotoId(1),
                    width: 6000,
                    height: 4000,
                    file_size: 100,
                    content_hash: Some("reloaded".into()),
                    preview_only: None,
                })
                .unwrap();
            PhotoId(1)
        } else {
            let mut p = (*photo()).clone();
            p.id = PhotoId(2);
            app.session.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
            PhotoId(2)
        };
        let new = app.session.loupe_job(id, 8, 8, true).unwrap();
        app.renderer.request(Slot::Main, new, 100);
        let (_, new) = take_job(&work);
        finish(&mut app, &work, &ctx, Slot::Main, old.run());
        assert!(!app.renderer.textures.contains_key(&Slot::Main));
        assert_eq!(app.session.media.source_usage().0, 0);
        assert!(app.renderer.is_pending(Slot::Main));
        finish(&mut app, &work, &ctx, Slot::Main, new.run());
        assert_eq!(app.renderer.textures[&Slot::Main].photo, id);
    }
}

#[test]
fn superseded_preview_with_repeated_key_does_not_clear_latest_pending_request() {
    let (mut app, work, ctx) = fixture();
    let job = app.session.loupe_job(PhotoId(1), 8, 8, true).unwrap().draft();
    app.renderer.request(Slot::Main, job.clone(), 100);
    let (_, old) = take_job(&work);
    let mut other = job.clone();
    other.key ^= 1;
    app.renderer.request(Slot::Main, other, 100);
    take_job(&work);
    app.renderer.request(Slot::Main, job, 100);
    let (_, latest) = take_job(&work);
    finish(&mut app, &work, &ctx, Slot::Main, old.run());
    assert!(app.renderer.is_pending(Slot::Main), "the repeated key is still running under a newer request id");
    finish(&mut app, &work, &ctx, Slot::Main, latest.run());
    assert!(!app.renderer.is_pending(Slot::Main));
}

#[test]
fn library_reset_rejects_old_job_even_when_photo_id_and_key_are_reused() {
    let (mut app, work, ctx) = fixture();
    request(&mut app);
    let (slot, old) = take_job(&work);
    let old_result = old.run();
    app.renderer.forget_all();
    let photo = app.session.catalog.photo(PhotoId(1)).unwrap().as_ref().clone();
    app.session = lightcraft_engine::Session::new();
    app.session.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    app.session.media.file_loader = Some(Arc::new(|_, _| {
        let mut image = li_raster::RgbFrame::new(8, 8);
        image.data.fill([0.1, 0.2, 0.9]);
        Ok((image, Default::default()))
    }));
    app.renderer.poll(&ctx, &mut app.session);
    request(&mut app);
    let (_, new) = take_job(&work);
    assert_eq!(old_result.key, new.key);
    assert_ne!(old_result.request_id, new.request_id);
    let expected = new.clone().run().rendered.unwrap().image.as_bytes();
    finish(&mut app, &work, &ctx, slot, new.run());
    finish(&mut app, &work, &ctx, slot, old_result);
    assert_eq!(
        app.renderer.thumb(PhotoId(1)).unwrap().pixels.as_ref().unwrap().pixels.iter().flat_map(|c| c.to_array()).collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn late_results_and_errors_cannot_replace_newest_pixels_or_sources() {
    for order in [0, 1, 2] {
        let (mut app, work, ctx) = fixture();
        request(&mut app);
        let (slot, old) = take_job(&work);
        let mut edit = lightcraft_develop::DevelopSettings::default();
        edit.light.exposure = 1.0;
        app.session.catalog.apply(Op::SetDevelop { id: PhotoId(1), settings: Arc::new(edit), label: "edit".into(), edited: None }).unwrap();
        request(&mut app);
        let (_, new) = take_job(&work);
        let expected = new.clone().run().rendered.unwrap().image.as_bytes();
        if order != 0 {
            finish(&mut app, &work, &ctx, slot, new.run());
            // An obsolete source and error must not poison the current successful request.
            let mut stale = old.run();
            if order == 2 {
                stale.rendered = Err("old failure".into());
            }
            finish(&mut app, &work, &ctx, slot, stale);
        } else {
            finish(&mut app, &work, &ctx, slot, old.run());
            assert!(app.renderer.thumb(PhotoId(1)).is_none());
            assert_eq!(app.session.media.source_usage().0, 0);
            finish(&mut app, &work, &ctx, slot, new.run());
        }
        assert_eq!(
            app.renderer.thumb(PhotoId(1)).unwrap().pixels.as_ref().unwrap().pixels.iter().flat_map(|c| c.to_array()).collect::<Vec<_>>(),
            expected
        );
        assert!(app.renderer.failure(slot).is_none());
    }
}

#[test]
fn undo_to_existing_texture_supersedes_pending_edit() {
    let (mut app, work, ctx) = fixture();
    request(&mut app);
    let (slot, first) = take_job(&work);
    let key = first.key;
    finish(&mut app, &work, &ctx, slot, first.run());
    let original = app.renderer.thumb(PhotoId(1)).unwrap().pixels.clone().unwrap();
    let mut edit = lightcraft_develop::DevelopSettings::default();
    edit.light.exposure = 1.0;
    let undo = app.session.catalog.apply(Op::SetDevelop { id: PhotoId(1), settings: Arc::new(edit), label: "edit".into(), edited: None }).unwrap();
    request(&mut app);
    let (_, edited) = take_job(&work);
    app.session.catalog.apply(undo).unwrap();
    request(&mut app);
    let (_, restored) = take_job(&work);
    assert_eq!(restored.key, key);
    finish(&mut app, &work, &ctx, slot, restored.run());
    finish(&mut app, &work, &ctx, slot, edited.run());
    assert_eq!(app.renderer.thumb(PhotoId(1)).unwrap().pixels.as_ref().unwrap().pixels, original.pixels);
}

#[test]
fn explicit_clear_repairs_same_key_bad_pixels_and_rejects_old_same_key_job() {
    let (mut app, work, ctx) = fixture();
    let job = app.session.thumb_job(PhotoId(1), 256).unwrap();
    let expected = job.clone().run().rendered.unwrap().image;
    let (cache, key) = job.cache.clone().unwrap();
    let mut bad = expected.clone();
    bad.data.fill([255, 0, 255, 255]);
    cache.put(key, Arc::new(bad.clone()));
    request(&mut app);
    let (slot, corrupt) = take_job(&work);
    finish(&mut app, &work, &ctx, slot, corrupt.run());
    assert_eq!(app.renderer.thumb(PhotoId(1)).unwrap().pixels.as_ref().unwrap().pixels[0].to_array(), [255, 0, 255, 255]);
    // An old job can have the SAME render key and stale decoded pixels.
    let mut old = job.clone();
    old.source = lightcraft_engine::media::SourceRef::Loaded(Box::new(lightcraft_engine::media::DecodedSource::new(
        Arc::new(li_raster::RgbFrame::new(8, 8)),
        None,
    )));
    app.renderer.textures.clear();
    app.renderer.request(slot, old, 10);
    let (_, old) = take_job(&work);
    app.session.execute("library.clearPreviews", &serde_json::json!({})).unwrap();
    app.renderer.poll(&ctx, &mut app.session);
    assert!(app.renderer.thumb(PhotoId(1)).is_none());
    request(&mut app);
    let (_, repaired) = take_job(&work);
    assert_eq!(repaired.key, job.key);
    finish(&mut app, &work, &ctx, slot, repaired.run());
    finish(&mut app, &work, &ctx, slot, old.run());
    assert_eq!(cache.get(key).unwrap().as_bytes(), expected.as_bytes());
    assert_eq!(
        app.renderer.thumb(PhotoId(1)).unwrap().pixels.as_ref().unwrap().pixels.iter().flat_map(|c| c.to_array()).collect::<Vec<_>>(),
        expected.as_bytes()
    );
}
