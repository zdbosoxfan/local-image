#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Affine, ColorMode, LayerContent, SampleType};
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
    let _gpu_gate = li_testkit::gpu::lock();
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
    let _gpu_gate = li_testkit::gpu::lock();
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
    let _gpu_gate = li_testkit::gpu::lock();
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
    let _gpu_gate = li_testkit::gpu::lock();
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
    let _gpu_gate = li_testkit::gpu::lock();
    use photocraft_gpu::{Compositor, render_to_vec};
    let (device, queue) = crate::gpu_test_support::compositor_device("SVG import");
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

#[test]
fn scene_adapter_characterization() {
    use photocraft_doc::{
        Artboard, DesignUnits, Fill, LayerMask, Path, PixelFormat, Rect, ShapeLayer, Size, Subpath, Surface, SymbolDef, SymbolId, TextLayer, VectorMask,
    };
    let path = Path::new(vec![Subpath::polygon(&[(1., 2.), (5., 2.), (5., 8.), (1., 8.)])]);
    let shifted = Path::new(vec![Subpath::polygon(&[(-6., -7.), (-2., -7.), (-2., -1.), (-6., -1.)])]);
    let mut pixels = Surface::new(PixelFormat::RGBA32F);
    pixels.write_pixel(2, 3, &[0.25, 0.5, 0.75, 1.]);
    let shape = Layer::new("shape", LayerContent::Shape(ShapeLayer { path: path.clone(), cache: Some(pixels.clone()), ..Default::default() }));
    let text = Layer::new(
        "type",
        LayerContent::Text(TextLayer {
            text: "editable".into(),
            transform: Affine { m: [2., 0., 0., 3., 11., 13.] },
            cache: Some(pixels.clone()),
            ..Default::default()
        }),
    );
    let raster = Layer::new("raster", LayerContent::Raster(pixels));
    let mut inner = Layer::group("inner", vec![shape, text, raster]);
    if let LayerContent::Group(group) = &mut inner.content {
        group.artboard = Some(Artboard::new(Rect::new(1, 2, 5, 8)));
    }
    inner.vector_mask = Some(VectorMask::new(path.clone()));
    inner.vector.clip = Some(path.clone());
    let mut mask = LayerMask::hide_all();
    mask.surface.write_pixel(2, 3, &[1.]);
    mask.density = 0.5;
    mask.feather = 2.;
    inner.mask = Some(mask);
    let mut layer = Layer::group("outer", vec![inner]);
    layer.vector_mask = Some(VectorMask::new(path.clone()));
    layer.vector.clip = Some(path.clone());
    translate(&mut layer, [-7, -9]).unwrap();
    assert_eq!(layer.vector_mask.as_ref().unwrap().path, shifted);
    assert_eq!(layer.vector.clip.as_ref().unwrap(), &shifted);
    let inner = &layer.children().unwrap()[0];
    assert_eq!(inner.vector_mask.as_ref().unwrap().path, shifted);
    assert_eq!(inner.vector.clip.as_ref().unwrap(), &shifted);
    let mask = inner.mask.as_ref().unwrap();
    assert_eq!(mask.surface.sample_channel(-5, -6, 0), 1.);
    assert_eq!(mask.surface.sample_channel(2, 3, 0), 0.);
    assert_eq!((mask.enabled, mask.linked, mask.density, mask.feather), (true, true, 0.5, 2.));
    let LayerContent::Group(group) = &inner.content else { panic!("nested group") };
    assert_eq!(group.artboard.as_ref().unwrap().rect, Rect::new(1, 2, 5, 8));
    let children = &group.children;
    let LayerContent::Shape(shape) = &children[0].content else { panic!("editable shape") };
    assert_eq!(shape.path, shifted);
    let LayerContent::Text(text) = &children[1].content else { panic!("editable text") };
    assert_eq!(text.text, "editable");
    assert_eq!(text.transform.m, [2., 0., 0., 3., 4., 4.]);
    let mut samples = Vec::new();
    for child in children {
        let surface = child.surface().unwrap();
        let pixel: Vec<_> = (0..4).map(|c| surface.sample_channel(-5, -6, c)).collect();
        assert_eq!(pixel, vec![0.25, 0.5, 0.75, 1.]);
        assert_eq!(surface.sample_channel(2, 3, 3), 0.);
        samples.push(pixel);
    }
    let geometry = json!({"shape":shape.path,"clip":inner.vector.clip,"vector_mask":inner.vector_mask.as_ref().unwrap().path,
        "transform":text.transform.m,"pixels":samples,"mask":[mask.surface.sample_channel(-5,-6,0),mask.surface.sample_channel(2,3,0)],
        "artboard":group.artboard.as_ref().unwrap().rect});
    let boundary = Path::new(vec![Subpath::polyline(&[(1_000_000., -1_000_000.), (0., 0.)])]);
    check_path(&boundary).unwrap();
    let mut outside = Layer::new("outside", LayerContent::Shape(ShapeLayer { path: boundary, ..Default::default() }));
    let path_error = translate(&mut outside, [1, 0]).unwrap_err().to_string();
    assert_eq!(path_error, "SVG placement exceeds coordinate bounds");
    let mut outside = Layer::new("outside type", LayerContent::Text(TextLayer { transform: Affine::translate(-1_000_000., 0.), ..Default::default() }));
    let text_error = translate(&mut outside, [-1, 0]).unwrap_err().to_string();
    assert_eq!(text_error, "SVG text placement exceeds coordinate bounds");
    let mut unsupported = Layer::new("fill", LayerContent::Fill(Fill::Solid(photocraft_doc::Color::BLACK)));
    let content_error = translate(&mut unsupported, [0, 0]).unwrap_err().to_string();
    assert_eq!(content_error, "unexpected content in SVG scene");
    let scene = ImportedScene {
        size: Size::new(37, 29),
        resolution_dpi: 144.,
        units: DesignUnits::Millimeters,
        layers: vec![layer.clone()],
        symbols: vec![SymbolDef { id: SymbolId(71), name: "master".into(), layers: vec![layer.clone()] }],
        warnings: vec![VectorWarning { feature: "synthetic".into(), object: Some("art".into()), message: "retained warning".into() }],
    };
    let (doc, warnings) = document(scene.clone(), "Imported art");
    assert_eq!(doc.name, "Imported art");
    assert_eq!(doc.size, scene.size);
    assert_eq!(doc.resolution_dpi, 144.);
    assert_eq!(doc.design_units, DesignUnits::Millimeters);
    assert_eq!(doc.mode, ColorMode::Rgb);
    assert_eq!(doc.depth, SampleType::F32);
    assert_eq!(doc.layers, scene.layers);
    assert_eq!(doc.symbols, scene.symbols);
    assert_eq!(warnings, scene.warnings);

    eprintln!(
        "CHARACTERIZATION {}",
        json!({"geometry":geometry,"errors":[path_error,text_error,content_error],
        "document":{"name":doc.name,"size":doc.size,"dpi":doc.resolution_dpi,"units":doc.design_units,
        "symbols":doc.symbols.iter().map(|s| (&s.name,s.id.0,s.layers.len())).collect::<Vec<_>>(),"warnings":warnings}})
    );
}
