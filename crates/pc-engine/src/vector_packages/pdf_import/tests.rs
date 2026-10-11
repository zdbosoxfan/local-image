#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
#[allow(dead_code)]
mod fixture {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../pc-pdf/fixtures/import/source.rs"));
}
fn session() -> Session {
    let mut s = Session::new();
    s.add_document(Document::new("destination", photocraft_doc::Size::new(128, 128), ColorMode::Rgb, SampleType::F32), None);
    s
}
#[test]
fn pdf_import_place_is_one_edit_and_preview_undo_redo() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut s = session();
    let mut r = capture(&s, fixture::artwork()).unwrap();
    r.offset = [12, 15];
    let scene = decode(&s, &r).unwrap();
    let before = s.active().unwrap().doc.clone();
    let revision = s.active().unwrap().revision;
    let journal = s.journal.len();
    let history = s.active().unwrap().history.entries().len();
    let (prepared, _) = preview(&s, &r, &scene).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.journal.len(), journal);
    let result = s.execute("file.pdf.place", serde_json::to_value(r).unwrap()).unwrap();
    assert_eq!(s.active().unwrap().revision, revision + 1);
    assert_eq!(s.journal.len(), journal + 1);
    assert_eq!(s.active().unwrap().history.entries().len(), history + 1);
    assert!(s.active().unwrap().doc.layer(LayerId(result["layer"].as_u64().unwrap())).unwrap().is_group());
    assert_eq!(photocraft_compose::flatten(&prepared), photocraft_compose::flatten(&s.active().unwrap().doc));
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
}
#[test]
fn pdf_import_staleness_selection_invalid_and_locked_targets_are_atomic() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut s = session();
    let r = capture(&s, fixture::artwork()).unwrap();
    s.execute("layer.new.layer", json!({"name":"changed"})).unwrap();
    let before = s.active().unwrap().doc.clone();
    let journal = s.journal.len();
    assert!(s.execute("file.pdf.place", serde_json::to_value(r).unwrap()).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.journal.len(), journal);
    let r = capture(&s, fixture::artwork()).unwrap();
    s.active_mut().unwrap().selected_layers.clear();
    assert!(check(&s, &r).is_err());
    let mut r = capture(&s, fixture::artwork()).unwrap();
    r.offset = [i32::MIN, 0];
    assert!(check(&s, &r).is_err());
    r.offset = [0, 0];
    r.pdf = vec![1, 2, 3];
    assert!(decode(&s, &r).is_err());
    let child = Layer::raster("anchor", photocraft_doc::PixelFormat::RGBA32F);
    let id = child.id;
    let group = Layer::group("inner", vec![child]);
    let mut ancestor = Layer::group("locked ancestor", vec![group]);
    ancestor.locks.all = true;
    s.edit("fixture", move |d, a| {
        d.layers.push(ancestor);
        *a = Some(id);
        Ok(())
    })
    .unwrap();
    assert!(capture(&s, fixture::artwork()).is_err());
}
#[test]
fn pdf_import_open_selected_page_and_edit_glyph_undo() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let mut s = session();
    s.execute("file.pdf.import", json!({"pdf":fixture::pages(),"pages":[1],"name":"opened","dpi":144})).unwrap();
    let st = s.active().unwrap();
    assert_eq!(st.doc.size, photocraft_doc::Size::new(60, 120));
    assert_eq!(st.doc.name, "opened");
    let (id, path) = st
        .doc
        .walk()
        .into_iter()
        .find_map(|(_, _, l)| {
            if l.name == "PDF glyph"
                && let LayerContent::Shape(sh) = &l.content
            {
                Some((l.id, sh.path.clone()))
            } else {
                None
            }
        })
        .unwrap();
    let original_path = path.clone();
    let path = path.transform(&Affine::translate(3.0, 4.0));
    s.execute("shape.applyPaths", json!({"edits":[{"layer":id.0,"path":path}]})).unwrap();
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc.layer(id).unwrap().name, "PDF glyph");
    let LayerContent::Shape(shape) = &s.active().unwrap().doc.layer(id).unwrap().content else { panic!("glyph remains a shape") };
    assert_eq!(shape.path, original_path);
}
#[test]
fn pdf_import_rtx5090_vulkan_preview_edit_release_undo_cpu_parity() {
    let _gpu_gate = crate::gpu_test_support::lock();
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
    eprintln!("PDF import GPU {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut compositor = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut s = session();
    let original = s.active().unwrap().doc.clone();
    for data in [fixture::artwork(), fixture::masked(), fixture::shading(), fixture::image(), fixture::pages()] {
        let mut r = capture(&s, data).unwrap();
        r.offset = [10, 12];
        let scene = decode(&s, &r).unwrap();
        let (prepared, _) = preview(&s, &r, &scene).unwrap();
        let cpu = photocraft_compose::flatten(&prepared);
        let gpu = render_to_vec(&mut compositor, &device, &queue, &prepared, prepared.bounds()).unwrap();
        let diff = cpu
            .px
            .iter()
            .zip(&gpu)
            .flat_map(|(a, b)| (0..4).map(move |i| (if i == 3 { a[i] - b[i] } else { a[i] * a[3] - b[i] * b[3] }).abs()))
            .fold(0.0_f32, f32::max);
        assert!(diff <= 1.0 / 255.0, "premultiplied difference {diff}");
        eprintln!("PDF CPU/GPU premultiplied max diff {diff}");
        publish(&mut s, &r, &scene).unwrap();
        let committed = &s.active().unwrap().doc;
        assert_eq!(gpu, render_to_vec(&mut compositor, &device, &queue, committed, committed.bounds()).unwrap());
        let (id, path) = committed
            .walk()
            .into_iter()
            .find_map(|(_, _, l)| if let LayerContent::Shape(sh) = &l.content { Some((l.id, sh.path.clone())) } else { None })
            .unwrap_or((LayerId(0), photocraft_doc::Path::default()));
        if id.0 != 0 {
            s.execute("shape.applyPaths", json!({"edits":[{"layer":id.0,"path":path.transform(&Affine::translate(2.0,3.0))}]})).unwrap();
            assert!(s.undo());
        }
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, original);
    }
}
