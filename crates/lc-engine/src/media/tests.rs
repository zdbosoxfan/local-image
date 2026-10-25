use super::*;

#[test]
fn background_mask_results_use_prospective_raw_decoding() {
    let mut session = crate::Session::new();
    let id = PhotoId(1);
    let mut photo = Photo::new(id, Source::File { path: "pending.arw".into() }, "pending.arw", "ARW", 6000, 4000, "2026-01-01");
    photo.kind = MediaKind::Raw;
    session.catalog.apply(lightcraft_catalog::Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    session.media.file_loader_with = Some(Arc::new(|_, _, _| panic!("preparing a job must not decode on the caller")));
    let mut incoming = DevelopSettings::default();
    incoming.raw.demosaic = lightcraft_develop::Demosaic::Rcd;
    incoming.raw.highlights = lightcraft_develop::HighlightMode::Clip;
    let expected = crate::files::RawOptions::of(&incoming);
    let job = session.preview_job(id, 1024, 1024, false, &incoming).unwrap();
    let SourceRef::RawFile { opts, .. } = job.source else { panic!("the prospective raw settings must control decoding") };
    assert_eq!(opts, expected);
    assert_eq!(*session.develop_of(id).unwrap(), DevelopSettings::default(), "preparing a mask must not commit its incoming settings");
}

#[test]
fn ds_finding_smart_preview_keeps_distinct_look_curve() {
    let camera = lightcraft_pipeline::tone::CameraTone::new(std::array::from_fn(|i| {
        let x = (i + 1) as f32 / 32.0;
        [x, x * 0.5]
    }))
    .unwrap();
    let look = lightcraft_pipeline::tone::CameraTone::new(std::array::from_fn(|i| {
        let x = (i + 1) as f32 / 32.0;
        [x, x * 0.8]
    }))
    .unwrap();
    let source = DecodedSource { camera_tone: Some(camera), ..DecodedSource::new(Arc::new(RgbFrame::new(1, 1)), None) };
    let info = source.info_or(SourceInfo { look_curve: Some(look), ..Default::default() });
    assert_eq!(info.camera_tone, Some(camera));
    assert_eq!(info.look_curve, Some(look));
}

/// Upright needs the lens's 35 mm-equivalent focal length: the recorded one, else the focal
/// length times the camera's crop factor; decoded sources keep the catalog's.
#[test]
fn source_info_carries_the_35mm_focal_length() {
    let mut p = Photo::new(PhotoId(1), Source::File { path: "a.arw".into() }, "a.arw", "ARW", 8640, 5760, "2026-01-01");
    p.kind = MediaKind::Raw;
    p.meta.camera = "Sony ILCE-1".into();
    p.meta.focal_mm = Some(16.0);
    assert_eq!(source_info(&p).focal_35mm, Some(16.0), "full frame by the lens database");
    p.meta.focal_35mm = Some(24.0);
    assert_eq!(source_info(&p).focal_35mm, Some(24.0), "the recorded value wins (an APS-C crop mode)");
    p.meta.focal_35mm = None;
    p.meta.camera = "Nobody Camera 9000".into();
    assert_eq!(source_info(&p).focal_35mm, None, "no crop factor: no guess");
    let decoded = DecodedSource::new(Arc::new(RgbFrame::new(1, 1)), Some(SourceInfo { raw: true, ..Default::default() }));
    assert_eq!(decoded.info_or(SourceInfo { focal_35mm: Some(28.0), ..Default::default() }).focal_35mm, Some(28.0));
}

#[test]
fn decoder_info_survives_render_jobs_cache_and_eviction() {
    let tone = lightcraft_pipeline::tone::CameraTone::new(std::array::from_fn(|i| {
        let x = 0.01 * (i + 1) as f32;
        [x, (x * 2.0).min(0.9)]
    }))
    .unwrap();
    let info = SourceInfo { raw: true, relative_wb: true, camera_tone: Some(tone), ..Default::default() };
    let mut s = crate::Session::with_demo();
    let id = s.active().unwrap();
    let mut job = s.render_job(id, 64, 64, false, true).unwrap();
    let loaded_info = info.clone();
    job.source = SourceRef::File {
        path: "synthetic.arw".into(),
        max_edge: 64,
        loader: Some(Arc::new(move |_, _| {
            let mut image = RgbFrame::new(64, 64);
            image.data.fill([0.1; 3]);
            Ok((image, loaded_info.clone()))
        })),
        fallback: None,
    };
    job.info = SourceInfo::default(); // Header facts cannot override decoder facts.
    job.settings = Arc::new(DevelopSettings::default());
    let r = job.clone().run();
    assert_eq!(r.loaded.as_ref().unwrap().info, Some(info.clone()));
    let expected = lightcraft_pipeline::render(&r.loaded.as_ref().unwrap().image, &info, &job.settings, &job.request);
    assert_eq!(r.rendered.as_ref().unwrap().image.data, expected.image.data);
    s.accept(&r);
    job.source = s.media.source_ref(s.catalog.photo(id).unwrap(), job.level);
    let again = job.run();
    assert!(again.loaded.is_none());
    assert_eq!(r.rendered.unwrap().image.data, again.rendered.unwrap().image.data);
    s.media.forget(id);
    assert!(s.media.get_source(id, r.level).is_none());
}

#[test]
fn budget_evicts_least_recently_used_across_caches() {
    let img = |w: usize| Arc::new(RgbFrame::new(w, w));
    let mut m = MediaCache::default();
    let mb = 1 << 20;
    m.set_budget(30 * mb);
    // 3 previews of 12 MB (1000² × 12 B) don't fit 30 MB: the least recently used goes
    m.insert(PhotoId(1), SourceLevel::Preview, img(1000));
    m.insert(PhotoId(2), SourceLevel::Preview, img(1000));
    assert!(m.get(PhotoId(1), SourceLevel::Preview).is_some()); // 1 is now more recent than 2
    m.insert(PhotoId(3), SourceLevel::Preview, img(1000));
    assert!(m.get(PhotoId(2), SourceLevel::Preview).is_none());
    assert!(m.get(PhotoId(1), SourceLevel::Preview).is_some() && m.get(PhotoId(3), SourceLevel::Preview).is_some());
    // a thumbnail source and a rendered preview compete in the same budget
    m.insert(PhotoId(4), SourceLevel::Thumb, img(500));
    m.rendered.put(Hash128(7), Arc::new(li_raster::Rgba8::new(1000, 1000)));
    assert!(m.held() <= 30 * mb, "{}", m.held());
    // the newest preview (the one on screen) is never evicted, even alone over budget
    m.set_budget(mb);
    assert!(m.get(PhotoId(3), SourceLevel::Preview).is_some());
    assert_eq!(m.usage().1.count, 1);
    assert_eq!(m.usage().0.count, 0);
    assert_eq!(m.rendered.mem_usage().0, 0);
}

#[test]
fn work_gate_waits_for_room() {
    use crate::memory::WorkGate;
    let g = Arc::new(WorkGate::new(100));
    let a = g.acquire(80);
    // more than fits waits until `a` is released; urgent work never waits
    let urgent = g.acquire_urgent(500);
    assert_eq!(g.usage().0, 580);
    drop(urgent);
    let g2 = g.clone();
    let t = std::thread::spawn(move || {
        let _b = g2.acquire(50);
        g2.usage().0
    });
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(g.usage().0, 80, "the second holder is still waiting");
    drop(a);
    assert_eq!(t.join().unwrap(), 50);
    assert_eq!(g.usage().0, 0);
    // a single holder may exceed the limit
    drop(g.acquire(1000));
}

#[test]
fn levels_cover_sizes_and_large_exports_use_the_original() {
    assert_eq!(SourceLevel::for_size(256), SourceLevel::Thumb);
    assert_eq!(SourceLevel::for_size(2560), SourceLevel::Preview);
    assert_eq!(SourceLevel::for_size(2561), SourceLevel::Full);
    let mut s = crate::Session::with_demo();
    let p = s.catalog.photos().next().unwrap().clone();
    let long = p.width.max(p.height) as usize;
    assert!(long > 2560, "demo photos are camera-sized");
    // a full-size export reads the source at its native size, not an upscaled preview
    let job = s.render_job(p.id, long, long, false, true).unwrap();
    assert_eq!(job.level, SourceLevel::Full);
    match &job.source {
        SourceRef::Demo { max_edge, .. } => assert_eq!(*max_edge, long),
        _ => panic!("demo source expected"),
    }
    let loupe = s.render_job(p.id, 1600, 1600, false, true).unwrap();
    assert_eq!(loupe.level, SourceLevel::Preview);
    assert_ne!(loupe.key, s.render_job(p.id, 1600, 1600, false, true).unwrap().draft().key);
    // two photos with identical settings and size never share a result key
    let plain: Vec<_> = s.catalog.photos().filter(|p| !p.is_edited()).map(|p| p.id).take(2).collect();
    let (a, b) = (s.render_job(plain[0], 1600, 1600, false, true).unwrap(), s.render_job(plain[1], 1600, 1600, false, true).unwrap());
    assert_ne!(a.key, b.key);
}

#[test]
fn library_workflow_small_exports_keep_original_geometry_and_detail() {
    use lightcraft_catalog::Op;
    use lightcraft_geom::{Orientation, Rect};
    use lightcraft_pipeline::{OutputDepth, OutputSpace};
    let mut session = crate::Session::with_demo();
    let id = session.active().unwrap();
    // Model the independently rounded source levels without decoding a large file.
    session.media.insert(id, SourceLevel::Preview, Arc::new(RgbFrame::new(2560, 3)));
    session.media.insert(id, SourceLevel::Full, Arc::new(RgbFrame::new(4096, 4)));
    for rotated_crop in [false, true] {
        let mut settings = DevelopSettings::default();
        let target = if rotated_crop {
            settings.orientation = Orientation::Rotate90;
            settings.crop.geometry.rect = Rect::new(0., 0., 1., 0.5);
            (2, 1024)
        } else {
            (2048, 2)
        };
        session.catalog.apply(Op::SetDevelop { id, settings: Arc::new(settings.clone()), label: "geometry fixture".into(), edited: None }).unwrap();
        let preview = session.render_job(id, 2048, 2, false, true).unwrap();
        assert_eq!(preview.level, SourceLevel::Preview, "display previews stay bounded");
        let export = session.export_job(id, target.0, target.1, OutputSpace::Rec2020, OutputDepth::F32Linear).unwrap();
        assert_eq!(export.level, SourceLevel::Full);
        assert_ne!(preview.key, export.key);
        assert_eq!(export.settings.hash64(), settings.hash64(), "crop and orientation survive");
        assert_eq!(export.request.quality, Quality::Full);
        assert_eq!(export.request.space, OutputSpace::Rec2020);
        assert_eq!(export.request.depth, OutputDepth::F32Linear);
        assert!(export.cache.is_none() && export.view_cache.is_none());
        let original = export.source.load_source().unwrap();
        assert_eq!((original.image.width, original.image.height), (4096, 4));
        assert_eq!(lightcraft_pipeline::output_size(4096, 4, &export.settings, &export.request), target);
        if !rotated_crop {
            assert_eq!(
                lightcraft_pipeline::output_size(2560, 3, &export.settings, &export.request),
                (1707, 2),
                "control reproduces proxy rounding defect"
            );
        }
    }
}

#[test]
fn variant_jobs_have_their_own_keys_and_hit_the_cache() {
    let mut s = crate::Session::with_demo();
    let id = s.active().unwrap();
    let mine = (*s.develop_of(id).unwrap()).clone();
    let with = |pid: &str| {
        let mut d = mine.clone();
        d.profile.id = pid.into();
        d
    };
    let (a, b) = (with("lc.vivid"), with("lc.bw.sepia"));
    let ja = s.variant_job(id, &a, 128).unwrap();
    let jb = s.variant_job(id, &b, 128).unwrap();
    assert_ne!(ja.key, jb.key, "keys differ per settings");
    assert_eq!(ja.key, s.variant_job(id, &a, 128).unwrap().key, "stable");
    assert_ne!(ja.key, s.variant_job(id, &a, 256).unwrap().key, "and per size");
    assert_ne!(ja.key, s.thumb_job(id, 128).unwrap().key, "not the photo's own thumbnail");
    assert_eq!(ja.level, SourceLevel::Thumb);
    let (cache, ck) = ja.cache.clone().unwrap();
    assert!(cache.get(ck).is_none());
    let r = ja.run();
    s.accept(&r);
    let first = r.rendered.unwrap().image;
    assert!(first.width.max(first.height) <= 128);
    assert!(cache.get(ck).is_some(), "rendered variants are cached");
    assert!(cache.get(jb.cache.as_ref().unwrap().1).is_none());
    // a second job for the same variant is served from the cache, pixel for pixel
    let again = s.variant_job(id, &a, 128).unwrap().run();
    assert!(again.loaded.is_none());
    assert_eq!(again.rendered.unwrap().image.as_bytes(), first.as_bytes());
    let other = jb.run().rendered.unwrap().image;
    assert_ne!(other.as_bytes(), first.as_bytes());
    // the photo's own settings never changed
    assert_eq!(*s.develop_of(id).unwrap(), mine);
    // hover previews: a loupe-sized job with other settings, its own key
    let pv = s.preview_job(id, 800, 600, true, &a).unwrap();
    assert_ne!(pv.key, s.render_job(id, 800, 600, false, true).unwrap().key);
    assert_eq!(*pv.settings, a);
}
