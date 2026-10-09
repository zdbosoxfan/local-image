use lightcraft_engine::Session;
use lightcraft_engine::catalog::{Op, Photo, PhotoId, Source};
use lightcraft_engine::develop::DevelopSettings;
use lightcraft_engine::media;
use lightcraft_engine::pipeline::{Quality, RenderRequest, SourceInfo};
use lightcraft_geom::Rect;
use lightcraft_raster::Rgb32f;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_subfolder(tag: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("lc_media_test_{}_{}_{}", tag, std::process::id(), nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

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
fn source_level_serde_round_trip() {
    for level in [media::SourceLevel::Thumb, media::SourceLevel::Preview, media::SourceLevel::Full] {
        let s = serde_json::to_string(&level).unwrap();
        let back: media::SourceLevel = serde_json::from_str(&s).unwrap();
        assert_eq!(level, back);
    }
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
fn source_info_demo_and_file() {
    let p_demo = Photo::new(PhotoId(1), Source::Demo { scene: 1 }, "demo", "RAW", 3000, 2000, "2026-01-01");
    assert!(media::source_info(&p_demo).raw);

    let p_file = Photo::new(PhotoId(2), Source::File { path: "x.jpg".into() }, "x.jpg", "JPEG", 800, 600, "2026-01-01");
    assert!(!media::source_info(&p_file).raw);
}

#[test]
fn decoded_source_info_or_uses_header_fallback() {
    let img = Arc::new(Rgb32f::new(1, 1));
    let ds = media::DecodedSource::new(img, None);

    let header = SourceInfo { raw: true, ..Default::default() };
    let info = ds.info_or(header.clone());
    assert!(info.raw);

    let with_info = media::DecodedSource::new(Arc::new(Rgb32f::new(1, 1)), Some(SourceInfo { raw: false, ..Default::default() }));
    let info2 = with_info.info_or(header);
    assert!(!info2.raw);
}

#[test]
fn media_cache_usage_and_budget() {
    let mut m = media::MediaCache::default();
    let img = |w: usize| Arc::new(Rgb32f::new(w, w));
    m.insert(PhotoId(1), media::SourceLevel::Thumb, img(10));
    m.insert(PhotoId(2), media::SourceLevel::Preview, img(20));
    m.insert(PhotoId(3), media::SourceLevel::Full, img(30));

    let usage = m.usage();
    assert_eq!(usage.0.count, 1);
    assert_eq!(usage.1.count, 1);
    assert_eq!(usage.2.count, 1);
    assert!(m.held() > 0);

    // Set a tiny budget: thumb and full get evicted, but the newest preview (PhotoId 2) is kept.
    m.set_budget(10);
    assert!(m.get(PhotoId(1), media::SourceLevel::Thumb).is_none());
    assert!(m.get(PhotoId(3), media::SourceLevel::Full).is_none());
    assert!(m.get(PhotoId(2), media::SourceLevel::Preview).is_some());
    assert!(m.held() > 0);
}

#[test]
fn media_cache_forget_and_clear() {
    let mut m = media::MediaCache::default();
    let img = Arc::new(Rgb32f::new(5, 5));
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
    let img = Arc::new(Rgb32f::new(5, 5));
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
    let img = Arc::new(Rgb32f::new(10, 10));
    m.insert(PhotoId(1), media::SourceLevel::Thumb, img);
    assert!(m.get(PhotoId(1), media::SourceLevel::Thumb).is_none());
}

#[test]
fn media_cache_attach_disk_cache() {
    let dir = temp_subfolder("attach_disk");
    let mut m = media::MediaCache::default();
    m.attach_disk_cache(&dir, 1024);
    m.set_disk_cache_bytes(&dir, 2048);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn render_job_run_success_and_error_paths() {
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
fn quick_view_job_and_quick_thumb_job_run() {
    let mut s = Session::with_demo();
    let id = s.active().unwrap();
    let quick = s.quick_view_job(id, 128, true).unwrap();
    let r = quick.run();
    assert!(r.rendered.is_ok());

    let thumb_job = s.thumb_job(id, 128).unwrap();
    let _ = s.quick_thumb_job(&thumb_job);
}

#[test]
fn develop_outputs_finite_and_size() {
    let img = Arc::new(Rgb32f::new(8, 8));
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
fn session_variant_job_does_not_change_settings() {
    let mut s = Session::with_demo();
    let id = s.active().unwrap();
    let original = (*s.develop_of(id).unwrap()).clone();
    let mut variant_settings = original.clone();
    variant_settings.light.exposure = 1.0;

    let job = s.variant_job(id, &variant_settings, 128).unwrap();
    assert_ne!(job.key, s.thumb_job(id, 128).unwrap().key);

    let r = job.run();
    s.accept(&r);
    assert_eq!(*s.develop_of(id).unwrap(), original);
}

#[test]
fn session_accept_rejects_stale_source() {
    let mut s = Session::new();
    let p = Photo::new(PhotoId(1), Source::File { path: "f.jpg".into() }, "f.jpg", "JPEG", 800, 600, "2026-01-01");
    s.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    s.media.file_loader = Some(Arc::new(|_, _| Ok((Rgb32f::new(8, 8), SourceInfo::default()))));

    let job = s.thumb_job(PhotoId(1), 128).unwrap();
    let r = job.run();
    assert!(r.loaded.is_some());

    s.catalog
        .apply(Op::SetContent { id: PhotoId(1), width: 800, height: 600, file_size: 999, content_hash: Some("new".into()), preview_only: None })
        .unwrap();
    s.media.forget(PhotoId(1));

    s.accept(&r);
    assert_eq!(s.media.source_usage().0, 0);
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
