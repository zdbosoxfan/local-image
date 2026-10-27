#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
fn session() -> Session {
    let mut s = Session::new();
    s.add_document(Document::new("destination", photocraft_doc::Size::new(96, 96), ColorMode::Rgb, SampleType::F32), None);
    s
}
fn source() -> Vec<u8> {
    include_bytes!("../../../../pc-affinity/tests/fixtures/shapes.afdesign").to_vec()
}
#[test]
fn affinity_place_preview_is_detached_atomic_undo_and_redo() {
    let _gpu_gate = li_testkit::gpu::lock();
    let mut s = session();
    let mut request = capture(&s, source()).unwrap();
    request.offset = [12, 14];
    let scene = decode(&s, &request).unwrap();
    let before = s.active().unwrap().doc.clone();
    let journal = s.journal.len();
    let rev = s.active().unwrap().revision;
    let (p, _) = preview(&s, &request, &scene).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.journal.len(), journal);
    s.execute("file.affinity.place", serde_json::to_value(&request).unwrap()).unwrap();
    assert_eq!(s.journal.len(), journal + 1);
    assert_eq!(s.active().unwrap().revision, rev + 1);
    assert_eq!(photocraft_compose::flatten(&p), photocraft_compose::flatten(&s.active().unwrap().doc));
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
}
#[test]
fn affinity_stale_locked_and_invalid_targets_publish_nothing() {
    let _gpu_gate = li_testkit::gpu::lock();
    let mut s = session();
    let r = capture(&s, source()).unwrap();
    s.execute("layer.new.layer", json!({"name":"changed"})).unwrap();
    let before = s.active().unwrap().doc.clone();
    let journal = s.journal.len();
    let revision = s.active().unwrap().revision;
    assert!(s.execute("file.affinity.place", serde_json::to_value(&r).unwrap()).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.journal.len(), journal);
    assert_eq!(s.active().unwrap().revision, revision);
    let child = Layer::raster("anchor", photocraft_doc::PixelFormat::RGBA32F);
    let id = child.id;
    let mut group = Layer::group("locked", vec![child]);
    group.locks.all = true;
    s.edit("fixture", move |doc, active| {
        doc.layers.push(group);
        *active = Some(id);
        Ok(())
    })
    .unwrap();
    assert!(capture(&s, source()).is_err());
    let mut r = r;
    r.document = s.active().unwrap().doc.id.0;
    r.revision = s.active().unwrap().revision;
    r.above = Some(LayerId(u64::MAX));
    assert!(check(&s, &r).is_err());
    r.above = None;
    r.offset = [i32::MIN, 0];
    assert!(check(&s, &r).is_err());
}
#[test]
fn affinity_open_preview_recovery_is_open_only_and_never_recovers_crc() {
    let _gpu_gate = li_testkit::gpu::lock();
    let mut s = session();
    let bytes = include_bytes!("../../../../pc-affinity/tests/fixtures/preview.afphoto").to_vec();
    assert!(matches!(read_open(&bytes, 72., &std::sync::atomic::AtomicBool::new(true)), Err(EngineError::Cancelled)));
    let request = capture(&s, source()).unwrap();
    assert!(matches!(decode_with(&s, &request, &std::sync::atomic::AtomicBool::new(true)), Err(EngineError::Cancelled)));
    assert!(decode(&s, &capture(&s, bytes.clone()).unwrap()).is_err());
    let r = s.execute("file.affinity.import", json!({"bytes":bytes,"name":"Recovery"})).unwrap();
    assert_eq!(r["warnings"][0]["feature"], "preview-fallback");
    assert_eq!(s.active().unwrap().doc.size, photocraft_doc::Size::new(8, 6));
    let mut bad = include_bytes!("../../../../pc-affinity/tests/fixtures/preview.afphoto").to_vec();
    // Original fixed #FT4 fixture: corrupt only the doc.dat CRC, retaining a valid
    // unsupported stream and a valid indexed thumbnail. Recovery would hide corruption.
    let fat = u64::from_le_bytes(bad[16..24].try_into().unwrap()) as usize;
    bad[fat + 88] ^= 1;
    assert!(photocraft_affinity::preview(&bad).is_ok());
    assert!(matches!(photocraft_affinity::import(&bad, &ImportOptions::default()), Err(photocraft_affinity::Error::Malformed(_))));
    let count = s.documents().len();
    assert!(s.execute("file.affinity.import", json!({"bytes":bad})).is_err());
    assert_eq!(s.documents().len(), count);
}
#[test]
fn affinity_imported_shape_remains_editable() {
    let _gpu_gate = li_testkit::gpu::lock();
    let mut s = session();
    s.execute("file.affinity.place", serde_json::to_value(capture(&s, source()).unwrap()).unwrap()).unwrap();
    let (id, path) = s
        .active()
        .unwrap()
        .doc
        .walk()
        .into_iter()
        .find_map(|(_, _, l)| if let LayerContent::Shape(sh) = &l.content { Some((l.id, sh.path.clone())) } else { None })
        .unwrap();
    s.execute("shape.applyPaths", json!({"edits":[{"layer":id.0,"path":path.transform(&Affine::translate(2.,3.))}]})).unwrap();
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().name, "Visible badge");
}
#[test]
fn affinity_rtx5090_vulkan_detached_preview_commit_cpu_parity() {
    let _gpu_gate = li_testkit::gpu::lock();
    use photocraft_gpu::{Compositor, render_to_vec};
    let (device, queue) = crate::gpu_test_support::compositor_device("Affinity import");
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut s = session();
    let mut r = capture(&s, source()).unwrap();
    r.offset = [12, 14];
    let scene = decode(&s, &r).unwrap();
    let (preview, _) = preview(&s, &r, &scene).unwrap();
    let cpu = photocraft_compose::flatten(&preview);
    let gpu = render_to_vec(&mut compositor, &device, &queue, &preview, preview.bounds()).unwrap();
    assert_eq!(cpu.px.len(), gpu.len());
    let diff = cpu
        .px
        .iter()
        .zip(&gpu)
        .flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()))
        .fold(0.0_f32, f32::max);
    assert!(diff <= 1.0 / 255.0, "premultiplied max diff {diff}");
    eprintln!("Affinity engine CPU/GPU premultiplied max difference {diff}");
    s.execute("file.affinity.place", serde_json::to_value(r).unwrap()).unwrap();
    let committed = &s.active().unwrap().doc;
    let after = render_to_vec(&mut compositor, &device, &queue, committed, committed.bounds()).unwrap();
    assert_eq!(gpu, after);
    assert!(s.undo());
    assert!(s.active().unwrap().doc.layers.is_empty());
    let fallback = photocraft_affinity::import_preview(
        include_bytes!("../../../../pc-affinity/tests/fixtures/preview.afphoto"),
        &ImportOptions::default(),
        &std::sync::atomic::AtomicBool::new(false),
        "unsupported layout",
    )
    .unwrap();
    let (fallback, _) = document(fallback, "recovery");
    let cpu = photocraft_compose::flatten(&fallback);
    let gpu = render_to_vec(&mut compositor, &device, &queue, &fallback, fallback.bounds()).unwrap();
    assert_eq!(cpu.px.len(), gpu.len());
    let diff = cpu
        .px
        .iter()
        .zip(&gpu)
        .flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()))
        .fold(0.0_f32, f32::max);
    assert!(diff <= 1.0 / 255.0, "preview fallback premultiplied max diff {diff}");
    eprintln!("Affinity raster fallback GPU premultiplied max difference {diff}");
}
#[test]
fn affinity_prepared_scene_validates_all_layers_before_publication() {
    let _gpu_gate = li_testkit::gpu::lock();
    let mut s = session();
    let request = capture(&s, source()).unwrap();
    let mut scene = decode(&s, &request).unwrap();
    let before = s.active().unwrap().doc.clone();
    if let LayerContent::Group(g) = &mut scene.layers[0].content
        && let LayerContent::Shape(sh) = &mut g.children[0].content
    {
        sh.cache = None;
    }
    assert!(commit(&mut s, &request, &scene).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    let mut scene = decode(&s, &request).unwrap();
    scene.layers.push(scene.layers[0].clone());
    assert!(commit(&mut s, &request, &scene).is_err());
    assert_eq!(s.active().unwrap().doc, before);
}

#[test]
fn affinity_invalid_native_effects_publish_no_document_or_history_change() {
    let _gpu_gate = li_testkit::gpu::lock();
    use photocraft_doc::effects::{Effect, FxCommon, Gradient};
    let mut session = session();
    let request = capture(&session, source()).unwrap();
    let scene = decode(&session, &request).unwrap();
    let before = session.active().unwrap().doc.clone();
    let revision = session.active().unwrap().revision;
    let journal = session.journal.len();
    let mut huge_shadow = Effect::default_drop_shadow();
    if let Effect::DropShadow(shadow) = &mut huge_shadow {
        shadow.size = 1_000_000.;
        shadow.use_global_light = false;
    }
    let nan = Effect::ColorOverlay { common: FxCommon::new(photocraft_doc::BlendMode::Normal, f32::NAN), color: photocraft_doc::Color::WHITE };
    let gradient = Effect::GradientOverlay {
        common: FxCommon::new(photocraft_doc::BlendMode::Normal, 1.),
        gradient: Gradient { stops: vec![(0., photocraft_doc::Color::WHITE); 1025], ..Default::default() },
        dither: false,
    };
    for effects in [
        vec![nan],
        vec![huge_shadow],
        vec![gradient],
        vec![Effect::ColorOverlay { common: FxCommon::new(photocraft_doc::BlendMode::Normal, 1.), color: photocraft_doc::Color::WHITE }; 65],
    ] {
        let mut malicious = scene.clone();
        malicious.layers[0].effects.enabled = true;
        malicious.layers[0].effects.items = effects;
        assert!(commit(&mut session, &request, &malicious).is_err());
        assert_eq!(session.active().unwrap().doc, before);
        assert_eq!(session.active().unwrap().revision, revision);
        assert_eq!(session.journal.len(), journal);
    }
}

#[test]
fn affinity_captured_admission_bounds_decode_and_cannot_be_supplied_by_json() {
    let _gpu_gate = li_testkit::gpu::lock();
    let session = session();
    let mut request = capture(&session, source()).unwrap();
    let mut forged = serde_json::to_value(&request).unwrap();
    forged["admission"] = json!({"decompressed_bytes":u64::MAX,"pixels":u64::MAX});
    assert!(serde_json::from_value::<Place>(forged).is_err());
    request.admission = Admission::for_ram(0);
    assert!(decode(&session, &request).is_err());
    for available in [1 << 20, 4 << 30, 32 << 30, u64::MAX] {
        let admission = Admission::for_ram(available).0;
        assert!(admission.decompressed_bytes <= available / 4);
        assert!(admission.decompressed_bytes <= 8 << 30);
        assert_eq!(admission.pixels * 48 + admission.decompressed_bytes % 48, admission.decompressed_bytes);
    }
    assert!(Admission::for_ram(32 << 30).0.pixels >= 100_000_000);
}

#[test]
fn affinity_valid_effects_use_the_captured_working_budget_before_preview() {
    let _gpu_gate = li_testkit::gpu::lock();
    let session = session();
    let mut request = capture(&session, source()).unwrap();
    let mut scene = decode(&session, &request).unwrap();
    let mut effect = photocraft_doc::Effect::default_drop_shadow();
    if let photocraft_doc::Effect::DropShadow(shadow) = &mut effect {
        shadow.use_global_light = false;
    }
    scene.layers[0].effects.enabled = true;
    scene.layers[0].effects.items = vec![effect];
    let admission = Admission::for_ram(32 << 30);
    request.admission = admission;
    assert!(preview(&session, &request, &scene).is_ok());
    // Admit the existing cache and fixed effect metadata, leaving no room for effect pixel buffers.
    let cache_bytes = scene
        .layers
        .iter()
        .flat_map(|layer| if let LayerContent::Group(group) = &layer.content { group.children.iter() } else { panic!() })
        .filter_map(|layer| layer.surface())
        .flat_map(|surface| surface.tiles())
        .map(|(_, tile)| tile.bytes().len() as u64)
        .sum::<u64>();
    request.admission.0.decompressed_bytes = cache_bytes + std::mem::size_of::<photocraft_doc::Effect>() as u64;
    let error = preview(&session, &request, &scene).unwrap_err().to_string();
    assert!(error.contains("effect working budget"), "{error}");
}

#[test]
fn affinity_open_and_place_use_the_document_group_depth_limit() {
    let _gpu_gate = li_testkit::gpu::lock();
    let session = session();
    let request = capture(&session, source()).unwrap();
    let limits = request.admission.0;
    let mut scene = ImportedScene {
        size: photocraft_doc::Size::new(1, 1),
        resolution_dpi: 72.,
        units: photocraft_doc::DesignUnits::Pixels,
        layers: vec![Layer::raster("pixel", photocraft_doc::PixelFormat::RGBA32F)],
        symbols: vec![],
        warnings: vec![],
    };
    for _ in 0..photocraft_doc::MAX_GROUP_DEPTH - 1 {
        scene.layers = vec![Layer::group("nested", std::mem::take(&mut scene.layers))];
    }
    assert!(validate_scene(None, &scene, limits).is_ok());
    assert!(preview(&session, &request, &scene).is_ok());
    scene.layers = vec![Layer::group("nested", std::mem::take(&mut scene.layers))];
    assert!(validate_scene(None, &scene, limits).is_ok());
    assert!(preview(&session, &request, &scene).is_err());
    scene.layers = vec![Layer::group("nested", std::mem::take(&mut scene.layers))];
    assert!(validate_scene(None, &scene, limits).is_err());
}

#[path = "../../../../pc-affinity/tests/support/affinity_docs.rs"]
mod affinity_docs;
#[test]
fn affinity_native_effect_pixels_survive_visibility_refresh_and_gpu_render() {
    use photocraft_gpu::{Compositor, render_to_vec};
    let _gpu_gate = li_testkit::gpu::lock();
    let scene = read_open(affinity_docs::EFFECTS, 72., &std::sync::atomic::AtomicBool::new(false)).unwrap();
    let (mut doc, warnings) = document(scene, "AF.5 native effects");
    assert!(warnings.iter().any(|w| w.feature == "bevel-approximation"));
    let first = photocraft_compose::flatten(&doc);
    let at = |x: usize, y: usize| first.px[y * 128 + x];
    assert_eq!(at(20, 20), [1., 0., 0., 1.]);
    assert_eq!(at(60, 28), [0., 0., 0., 1.]);
    assert_eq!(at(108, 16), [0., 0., 0., 1.]);
    assert!(at(8, 60)[2] > 0.5, "outline must surround the existing shape stroke");
    let exported = photocraft_io::export(&doc, "png", &Default::default()).unwrap();
    let png = image::load_from_memory(&exported.bytes).unwrap().into_rgba8();
    assert_eq!(png.dimensions(), (128, 128));
    let export_difference = first
        .px
        .iter()
        .zip(png.pixels())
        .flat_map(|(a, b)| {
            (0..4).map(move |i| {
                let decoded = f32::from(b[i]) / 255.;
                if i == 3 { (a[i] - decoded).abs() } else { (a[i] * a[3] - decoded * f32::from(b[3]) / 255.).abs() }
            })
        })
        .fold(0.0_f32, f32::max);
    assert!(export_difference <= 1. / 255. + 1e-5, "native effect PNG export max difference {export_difference}");
    let saved = photocraft_io::export(&doc, "pcraft", &Default::default()).unwrap();
    let restored = photocraft_format::load_from_bytes(&saved.bytes).unwrap();
    assert_eq!(restored.walk().iter().map(|(_, _, l)| &l.effects).collect::<Vec<_>>(), doc.walk().iter().map(|(_, _, l)| &l.effects).collect::<Vec<_>>());
    assert_eq!(photocraft_compose::flatten(&restored), first);
    let id = doc.walk().into_iter().find(|(_, _, l)| l.name == "Parent red").unwrap().2.id;
    doc.layer_mut(id).unwrap().visible = false;
    assert_eq!(photocraft_compose::flatten(&doc).px[20 * 128 + 20][3], 0.);
    doc.layer_mut(id).unwrap().visible = true;
    assert_eq!(photocraft_compose::flatten(&doc), first);
    let (device, queue) = crate::gpu_test_support::compositor_device("Affinity native effects");
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let gpu = render_to_vec(&mut compositor, &device, &queue, &doc, doc.bounds()).unwrap();
    let difference = first.px.iter().zip(gpu).flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs())).fold(0.0_f32, f32::max);
    assert!(difference <= 1. / 255. + 1e-5, "native effect CPU/GPU max difference {difference}");
    let scene = read_open(affinity_docs::TEXT_EFFECTS, 72., &std::sync::atomic::AtomicBool::new(false)).unwrap();
    let (mut text_doc, _) = document(scene, "Outlined masked type");
    let first = photocraft_compose::flatten(&text_doc);
    let parent = text_doc.walk().into_iter().find(|(_, _, l)| l.name == "Masked outlined text").unwrap().2.id;
    text_doc.layer_mut(parent).unwrap().visible = false;
    assert!(photocraft_compose::flatten(&text_doc).px.iter().all(|p| p[3] == 0.));
    text_doc.layer_mut(parent).unwrap().visible = true;
    assert_eq!(photocraft_compose::flatten(&text_doc), first);
    let gpu = render_to_vec(&mut compositor, &device, &queue, &text_doc, text_doc.bounds()).unwrap();
    let difference = first.px.iter().zip(gpu).flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs())).fold(0.0_f32, f32::max);
    assert!(difference <= 1. / 255. + 1e-5, "outlined masked text CPU/GPU max difference {difference}");
}

#[test]
fn affinity_worker_preparation_keeps_captured_admission_and_cancellation() {
    let session = session();
    let mut request = capture(&session, affinity_docs::EFFECTS.to_vec()).unwrap();
    request.admission = Admission::for_ram(0);
    let cancel = std::sync::atomic::AtomicBool::new(false);
    assert!(prepare(&request, 72., &cancel).is_err());
    request.admission = Admission::for_ram(32 << 30);
    let scene = prepare(&request, 72., &cancel).unwrap();
    assert!(scene.warnings.iter().any(|w| w.feature == "bevel-approximation"));
    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(matches!(prepare(&request, 72., &cancel), Err(EngineError::Cancelled)));
}
