#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::vector_design::exchange::VectorWarning;
use photocraft_doc::{Affine, ColorMode, LayerContent, SampleType};
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
    let _gpu_gate = li_testkit::gpu::lock();
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
    let _gpu_gate = li_testkit::gpu::lock();
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
    let _gpu_gate = li_testkit::gpu::lock();
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
    let _gpu_gate = li_testkit::gpu::lock();
    use photocraft_gpu::{Compositor, render_to_vec};
    let (device, queue) = crate::gpu_test_support::compositor_device("PDF import");
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
    translate(&mut layer, [-7, -9], "PDF").unwrap();
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
    check_path(&boundary, "PDF").unwrap();
    let mut outside = Layer::new("outside", LayerContent::Shape(ShapeLayer { path: boundary, ..Default::default() }));
    let path_error = translate(&mut outside, [1, 0], "PDF").unwrap_err().to_string();
    assert_eq!(path_error, "PDF placement exceeds coordinate bounds");
    let mut outside = Layer::new("outside type", LayerContent::Text(TextLayer { transform: Affine::translate(-1_000_000., 0.), ..Default::default() }));
    let text_error = translate(&mut outside, [-1, 0], "PDF").unwrap_err().to_string();
    assert_eq!(text_error, "PDF text placement exceeds coordinate bounds");
    let mut unsupported = Layer::new("fill", LayerContent::Fill(Fill::Solid(photocraft_doc::Color::BLACK)));
    let content_error = translate(&mut unsupported, [0, 0], "PDF").unwrap_err().to_string();
    assert_eq!(content_error, "unexpected content in PDF scene");
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
