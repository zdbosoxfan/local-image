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
    use photocraft_gpu::{Compositor, render_to_vec};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .expect("RTX 5090 Vulkan required");
    let info = adapter.get_info();
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert!(info.name.contains("5090"), "{info:?}");
    assert_eq!(Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("Affinity import GPU: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut s = session();
    let mut r = capture(&s, source()).unwrap();
    r.offset = [12, 14];
    let scene = decode(&s, &r).unwrap();
    let (preview, _) = preview(&s, &r, &scene).unwrap();
    let cpu = photocraft_compose::flatten(&preview);
    let gpu = render_to_vec(&mut compositor, &device, &queue, &preview, preview.bounds()).unwrap();
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
