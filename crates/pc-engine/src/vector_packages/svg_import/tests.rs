#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
fn session() -> Session {
    let mut s = Session::new();
    s.add_document(Document::new("destination", photocraft_doc::Size::new(96, 96), ColorMode::Rgb, SampleType::F32), None);
    s
}
fn svg() -> String {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><g id="art"><path id="curve" fill="red" d="M4 24 C4 0 44 0 44 24 C44 48 4 48 4 24Z"/></g></svg>"#.into()
}
#[test]
fn svg_import_place_is_one_edit_with_detached_preview_undo_and_redo() {
    let mut s = session();
    let mut r = capture(&s, svg()).unwrap();
    r.offset = [10, 12];
    let scene = decode(&s, &r).unwrap();
    let before = s.active().unwrap().doc.clone();
    let journal = s.journal.len();
    let history = s.active().unwrap().history.entries().len();
    let (preview, _) = preview(&s, &r, &scene).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.journal.len(), journal);
    let result = s.execute("file.svg.place", serde_json::to_value(r).unwrap()).unwrap();
    assert_eq!(s.journal.len(), journal + 1);
    assert_eq!(s.active().unwrap().history.entries().len(), history + 1);
    assert!(s.active().unwrap().doc.layer(LayerId(result["layer"].as_u64().unwrap())).unwrap().is_group());
    assert_eq!(photocraft_compose::flatten(&preview), photocraft_compose::flatten(&s.active().unwrap().doc));
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
}
#[test]
fn svg_import_stale_invalid_and_locked_destination_publish_nothing() {
    let mut s = session();
    let r = capture(&s, svg()).unwrap();
    s.execute("layer.new.layer", json!({"name":"changed"})).unwrap();
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let journal = s.journal.len();
    assert!(s.execute("file.svg.place", serde_json::to_value(r).unwrap()).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().revision, revision);
    assert_eq!(s.journal.len(), journal);
    let mut r = capture(&s, "<svg".into()).unwrap();
    assert!(s.execute("file.svg.place", serde_json::to_value(&r).unwrap()).is_err());
    r.offset = [i32::MIN, 0];
    assert!(check(&s, &r).is_err());
    let child = Layer::raster("anchor", photocraft_doc::PixelFormat::RGBA32F);
    let id = child.id;
    let mut group = Layer::group("locked", vec![child]);
    group.locks.all = true;
    s.edit("test", move |doc, active| {
        doc.layers.push(group);
        *active = Some(id);
        Ok(())
    })
    .unwrap();
    assert!(capture(&s, svg()).is_err());
}
#[test]
fn svg_import_open_paste_and_shape_remain_editable() {
    let mut s = session();
    let r = capture(&s, svg()).unwrap();
    s.execute("file.svg.paste", serde_json::to_value(r).unwrap()).unwrap();
    let st = s.active().unwrap();
    let (id, path) =
        st.doc.walk().into_iter().find_map(|(_, _, l)| if let LayerContent::Shape(sh) = &l.content { Some((l.id, sh.path.clone())) } else { None }).unwrap();
    let shifted = path.transform(&Affine::translate(2.0, 3.0));
    s.execute("shape.applyPaths", json!({"edits":[{"layer":id.0,"path":shifted}]})).unwrap();
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().name, "curve");
    s.execute("file.svg.import", json!({"svg":svg(),"name":"opened","dpi":96})).unwrap();
    assert_eq!(s.active().unwrap().doc.name, "opened");
    assert_eq!(s.active().unwrap().doc.resolution_dpi, 96.0);
    assert_eq!(s.active().unwrap().doc.layer_count(), 2);
}
#[test]
fn svg_import_place_retains_viewport_clipping_after_native_edit() {
    let mut s = session();
    let mut request = capture(
        &s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect id="edge" x="12" y="4" width="10" height="8" fill="blue" stroke="red"/></svg>"#
            .into(),
    )
    .unwrap();
    request.offset = [10, 10];
    s.execute("file.svg.place", serde_json::to_value(request).unwrap()).unwrap();
    let before = photocraft_compose::flatten(&s.active().unwrap().doc);
    assert_eq!(before.get(25, 16)[3], 1.0);
    assert_eq!(before.get(26, 16)[3], 0.0);
    let (id, path) = s
        .active()
        .unwrap()
        .doc
        .walk()
        .into_iter()
        .find_map(|(_, _, l)| if let LayerContent::Shape(sh) = &l.content { Some((l.id, sh.path.transform(&Affine::translate(4.0, 0.0)))) } else { None })
        .unwrap();
    s.execute("shape.applyPaths", json!({"edits":[{"layer":id.0,"path":path}]})).unwrap();
    let after = photocraft_compose::flatten(&s.active().unwrap().doc);
    assert_eq!(after.get(26, 16)[3], 0.0);
    assert_eq!(after.get(30, 16)[3], 0.0);
    assert!(s.undo());
    assert_eq!(photocraft_compose::flatten(&s.active().unwrap().doc), before);
}
#[test]
fn svg_import_rtx5090_vulkan_detached_preview_commit_cpu_parity() {
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
    eprintln!("SVG import GPU: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut s = session();
    let mut r = capture(
        &s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><defs><linearGradient id="g"><stop stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs><g id="art"><path id="curve" fill="url(#g)" d="M4 24 C4 0 44 0 44 24 C44 48 4 48 4 24Z"/></g></svg>"##.into(),
    ).unwrap();
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
    eprintln!("SVG engine CPU/GPU premultiplied max difference {diff}");
    s.execute("file.svg.place", serde_json::to_value(r).unwrap()).unwrap();
    let committed = &s.active().unwrap().doc;
    let after = render_to_vec(&mut compositor, &device, &queue, committed, committed.bounds()).unwrap();
    assert_eq!(gpu, after);
    assert!(s.undo());
    assert!(s.active().unwrap().doc.layers.is_empty());
}
