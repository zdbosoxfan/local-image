use li_raster::RgbFrame;
use lightcraft_engine::Session;
use lightcraft_engine::catalog::{Op, Photo, PhotoId, Source};
use lightcraft_engine::develop::DevelopSettings;
use lightcraft_engine::media;
use lightcraft_engine::pipeline::{Quality, RenderRequest, SourceInfo};
use lightcraft_geom::Rect;
use std::sync::Arc;

#[test]
fn source_level_for_size_boundaries() {
    assert_eq!(media::SourceLevel::for_size(0), media::SourceLevel::Thumb);
    assert_eq!(media::SourceLevel::for_size(1), media::SourceLevel::Thumb);
    assert_eq!(media::SourceLevel::for_size(520), media::SourceLevel::Thumb);
    assert_eq!(media::SourceLevel::for_size(521), media::SourceLevel::Preview);
    assert_eq!(media::SourceLevel::for_size(2560), media::SourceLevel::Preview);
    assert_eq!(media::SourceLevel::for_size(2561), media::SourceLevel::Full);
    assert_eq!(media::SourceLevel::for_size(usize::MAX), media::SourceLevel::Full);
}

#[test]
fn source_level_max_edge_values() {
    assert_eq!(media::SourceLevel::Thumb.max_edge(), 512);
    assert_eq!(media::SourceLevel::Preview.max_edge(), 2560);
    assert_eq!(media::SourceLevel::Full.max_edge(), usize::MAX);
}

#[test]
fn thumb_bucket_rounds_up_to_defined_sizes() {
    for (input, expected) in
        [(0, 128), (1, 128), (127, 128), (128, 128), (129, 256), (256, 256), (383, 384), (384, 384), (385, 512), (512, 512), (513, 512), (10000, 512)]
    {
        assert_eq!(media::thumb_bucket(input), expected, "input {input}");
    }
}

#[test]
fn content_key_uses_hash_demo_and_file() {
    let mut s = Session::new();
    let p = Photo::new(PhotoId(1), Source::File { path: "f.jpg".into() }, "f.jpg", "JPEG", 100, 80, "2026-01-01");
    s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();

    s.catalog.apply(Op::SetContent { id: PhotoId(1), width: 100, height: 80, file_size: 123, content_hash: None, preview_only: None }).unwrap();
    let p = s.catalog.photo(PhotoId(1)).unwrap();
    assert_eq!(media::content_key(p), "file:f.jpg:123");

    s.catalog
        .apply(Op::SetContent { id: PhotoId(1), width: 100, height: 80, file_size: 123, content_hash: Some("hash123".into()), preview_only: None })
        .unwrap();
    let p = s.catalog.photo(PhotoId(1)).unwrap();
    assert_eq!(media::content_key(p), "hash123");

    let p_demo = Photo::new(PhotoId(2), Source::Demo { scene: 42 }, "demo", "RAW", 3000, 2000, "2026-01-01");
    assert_eq!(media::content_key(&p_demo), "demo:42");
}

#[test]
fn upright_discards_results_for_replaced_content() {
    use serde_json::json;
    use std::time::{Duration, Instant};

    let mut s = Session::new();
    s.media.file_loader = Some(Arc::new(|_, _| Ok((RgbFrame::filled(64, 48, [0.5; 3]), SourceInfo::default()))));
    for id in [PhotoId(1), PhotoId(2), PhotoId(3)] {
        let path = format!("{}.tif", id.0);
        let mut photo = Photo::new(id, Source::File { path: path.clone() }, &path, "TIFF", 64, 48, "2026-01-01");
        photo.content_hash = Some(format!("original-{}", id.0));
        if id == PhotoId(1) {
            let mut settings = DevelopSettings::default();
            settings.geometry.upright = lightcraft_develop::Upright::Level;
            settings.geometry.upright_transform = Some(lightcraft_geom::Homography::IDENTITY);
            photo.develop = Arc::new(settings);
        }
        s.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    }
    s.execute("library.select", &json!({"ids": [1, 2, 3], "active": 1})).unwrap();
    s.execute("develop.sync", &json!({"groups": ["geometry"]})).unwrap();
    assert_eq!(s.upright.jobs().len(), 1);
    let start = Instant::now();
    while s.upright.busy() {
        assert!(start.elapsed() < Duration::from_secs(60), "Upright analysis finishes before polling");
        std::thread::sleep(Duration::from_millis(2));
    }
    s.catalog
        .apply(Op::SetContent { id: PhotoId(2), width: 64, height: 48, file_size: 0, content_hash: Some("replacement".into()), preview_only: None })
        .unwrap();
    s.media.forget(PhotoId(2));

    assert_eq!(s.upright_poll(), 1, "only the unchanged source receives its analysis");
    assert!(s.develop_of(PhotoId(2)).unwrap().upright_unsolved(), "replacement pixels keep no transform from the old source");
    assert!(s.develop_of(PhotoId(3)).unwrap().geometry.upright_transform.is_some(), "the unchanged source keeps its completed analysis");
}

#[test]
fn source_info_demo_and_file() {
    let p_demo = Photo::new(PhotoId(1), Source::Demo { scene: 1 }, "demo", "RAW", 3000, 2000, "2026-01-01");
    assert!(media::source_info(&p_demo).raw);

    let p_file = Photo::new(PhotoId(2), Source::File { path: "x.jpg".into() }, "x.jpg", "JPEG", 800, 600, "2026-01-01");
    assert!(!media::source_info(&p_file).raw);
}

#[test]
fn decoded_source_info_or_uses_header_fallback() {
    let img = Arc::new(RgbFrame::new(1, 1));
    let ds = media::DecodedSource::new(img, None);

    let header = SourceInfo { raw: true, ..Default::default() };
    let info = ds.info_or(header.clone());
    assert!(info.raw);

    let with_info = media::DecodedSource::new(Arc::new(RgbFrame::new(1, 1)), Some(SourceInfo { raw: false, ..Default::default() }));
    let info2 = with_info.info_or(header);
    assert!(!info2.raw);
}

#[test]
fn media_cache_forget_and_clear() {
    let mut m = media::MediaCache::default();
    let img = Arc::new(RgbFrame::new(5, 5));
    m.insert(PhotoId(1), media::SourceLevel::Thumb, img.clone());
    m.insert(PhotoId(1), media::SourceLevel::Preview, img.clone());
    m.insert(PhotoId(1), media::SourceLevel::Full, img.clone());

    assert!(m.get(PhotoId(1), media::SourceLevel::Thumb).is_some());
    m.forget(PhotoId(1));
    assert!(m.get(PhotoId(1), media::SourceLevel::Thumb).is_none());
    assert!(m.get(PhotoId(1), media::SourceLevel::Preview).is_none());
    assert!(m.get(PhotoId(1), media::SourceLevel::Full).is_none());

    m.insert(PhotoId(2), media::SourceLevel::Thumb, img.clone());
    m.clear_sources();
    assert!(m.get(PhotoId(2), media::SourceLevel::Thumb).is_none());
}

#[test]
fn media_cache_preview_capacity_lru() {
    let mut m = media::MediaCache::default();
    m.preview_capacity = 2;
    let img = Arc::new(RgbFrame::new(5, 5));
    m.insert(PhotoId(1), media::SourceLevel::Preview, img.clone());
    m.insert(PhotoId(2), media::SourceLevel::Preview, img.clone());
    m.insert(PhotoId(3), media::SourceLevel::Preview, img.clone());

    assert!(m.get(PhotoId(1), media::SourceLevel::Preview).is_none());
    assert!(m.get(PhotoId(2), media::SourceLevel::Preview).is_some());
    assert!(m.get(PhotoId(3), media::SourceLevel::Preview).is_some());
}

#[test]
fn media_cache_budget_enforced_evicts() {
    let mut m = media::MediaCache::default();
    m.set_budget(1);
    let img = Arc::new(RgbFrame::new(10, 10));
    m.insert(PhotoId(1), media::SourceLevel::Thumb, img);
    assert!(m.get(PhotoId(1), media::SourceLevel::Thumb).is_none());
}

#[test]
fn render_job_run_success_and_error_paths() {
    let _gpu = crate::gpu_support::lock();
    // success with demo
    let mut s = Session::with_demo();
    let id = s.active().unwrap();
    let job = s.render_job(id, 64, 64, false, true).unwrap();
    let r = job.run();
    assert!(r.rendered.is_ok());
    let ok_img = r.rendered.unwrap().image;
    assert!(ok_img.width > 0 && ok_img.height > 0);

    // error path
    let mut s2 = Session::new();
    let p = Photo::new(PhotoId(1), Source::File { path: "missing.jpg".into() }, "missing.jpg", "JPEG", 800, 600, "2026-01-01");
    s2.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    s2.media.file_loader = Some(Arc::new(|_, _| Err("no file".into())));
    let job = s2.thumb_job(PhotoId(1), 128).unwrap();
    let r = job.run();
    assert!(r.rendered.is_err());
}

#[test]
fn render_job_draft_changes_quality_and_key() {
    let mut s = Session::with_demo();
    let id = s.active().unwrap();
    let job = s.render_job(id, 64, 64, false, true).unwrap();
    let old_key = job.key;
    let draft = job.draft();
    assert_eq!(draft.request.quality, Quality::Draft);
    assert_ne!(draft.key, old_key);
}

#[test]
fn develop_outputs_finite_and_size() {
    let img = Arc::new(RgbFrame::new(8, 8));
    let info = SourceInfo::default();
    let settings = DevelopSettings::default();
    let req = RenderRequest::fit(8, 8);
    let rendered = media::develop(&img, &info, &settings, &req, None, false);
    assert_eq!(rendered.image.width, 8);
    assert_eq!(rendered.image.height, 8);
    assert_eq!(rendered.image.as_bytes().len(), 8 * 8 * 4);
}

#[test]
fn session_render_job_levels() {
    let mut s = Session::with_demo();
    let p = s.catalog.photos().next().unwrap().clone();
    let long = p.width.max(p.height) as usize;
    assert!(long > 2560, "demo photos are camera-sized");

    let job = s.render_job(p.id, 100, 100, false, true).unwrap();
    assert_eq!(job.level, media::SourceLevel::Thumb);

    let job = s.render_job(p.id, 800, 800, false, true).unwrap();
    assert_eq!(job.level, media::SourceLevel::Preview);

    let job = s.render_job(p.id, long, long, false, true).unwrap();
    assert_eq!(job.level, media::SourceLevel::Full);
}

#[test]
fn session_thumb_job_buckets_and_cache() {
    let mut s = Session::with_demo();
    let id = s.active().unwrap();

    let job100 = s.thumb_job(id, 100).unwrap();
    assert_eq!(job100.request.max_w, 128);
    assert_eq!(job100.request.max_h, 128);
    assert!(job100.cache.is_some());

    let job300 = s.thumb_job(id, 300).unwrap();
    assert_eq!(job300.request.max_w, 384);
    assert_eq!(job300.request.max_h, 384);
    assert!(job300.cache.is_some());

    let job600 = s.thumb_job(id, 600).unwrap();
    assert_eq!(job600.request.max_w, 512);
    assert_eq!(job600.request.max_h, 512);
}

#[test]
fn session_source_now_roundtrip() {
    let mut s = Session::with_demo();
    let id = s.active().unwrap();

    let img = s.source_now(id, media::SourceLevel::Thumb).unwrap();
    assert!(img.width > 0 && img.height > 0);
    assert!(s.media.get(id, media::SourceLevel::Thumb).is_some());

    s.media.forget(id);
    assert!(s.media.get(id, media::SourceLevel::Thumb).is_none());
}

#[test]
fn face_job_handles_invalid_face() {
    let _gpu = crate::gpu_support::lock();
    let mut s = Session::with_demo();
    let id = s.active().unwrap();

    let rect = Rect { x0: f64::NAN, y0: f64::NAN, x1: f64::INFINITY, y1: f64::INFINITY };

    let job = s.face_job(id, rect, 128);
    assert!(job.is_some());
    let job = job.unwrap();
    assert!(job.request.max_w <= 512 && job.request.max_h <= 512);

    let r = job.run();
    assert!(r.rendered.is_ok());
}
