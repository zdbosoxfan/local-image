use photocraft_doc::{LayerContent, LayerId};
use photocraft_engine::Session;
use serde_json::json;
type TestResult = Result<(), Box<dyn std::error::Error>>;
#[test]
fn apply_group_preview_and_undo() -> TestResult {
    let mut s = Session::new();
    s.execute("file.new", json!({"width":32,"height":32}))?;
    let id = s.active().ok_or("doc")?.active_layer.ok_or("layer")?;
    {
        let d = s.active_mut().ok_or("doc")?;
        let doc = std::sync::Arc::make_mut(&mut d.doc);
        let px = doc.layer_mut(id).ok_or("layer")?.surface_mut().ok_or("pixels")?;
        for y in 6..26 {
            for x in 6..26 {
                px.write_pixel(x, y, &[0., 0., 0., 1.]);
            }
        }
    }
    let count = s.active().ok_or("doc")?.doc.layers.len();
    let preview = s.execute("layer.vectorize.preview", json!({"scale":0.5,"params":{"preset":"black_white"}}))?;
    assert_eq!(preview["trace"]["width"], 16);
    assert_eq!(s.active().ok_or("doc")?.doc.layers.len(), count);
    let output = s.execute("layer.vectorize", json!({"params":{"preset":"black_white"}}))?;
    let gid = LayerId(output["group"].as_u64().ok_or("group id")?);
    let g = s.active().ok_or("doc")?.doc.layer(gid).ok_or("group")?;
    assert_eq!(g.name, "Vectorized – Background");
    let children = g.children().ok_or("children")?;
    assert_eq!(children.len(), 1);
    assert!(matches!(children[0].content, LayerContent::Shape(_)));
    assert_eq!(children[0].name, "#000000");
    assert!(s.is_enabled_with("layer.vectorize", &json!({"layer":id.0})));
    s.execute("edit.undo", json!({}))?;
    assert!(s.active().ok_or("doc")?.doc.layer(gid).is_none());
    assert!(s.active().ok_or("doc")?.doc.layer(id).is_some());
    s.execute("edit.redo", json!({}))?;
    assert!(s.active().ok_or("doc")?.doc.layer(gid).is_some());
    Ok(())
}
#[test]
fn invalid_source_and_parameters_leave_document_unchanged() -> TestResult {
    let mut s = Session::new();
    s.execute("file.new", json!({"width":8,"height":8}))?;
    let before = s.active().ok_or("doc")?.doc.layers.clone();
    assert!(s.execute("layer.vectorize", json!({"params":{"detail":-1.}})).is_err());
    assert!(s.execute("layer.vectorize.preview", json!({"scale":2.})).is_err());
    assert_eq!(s.active().ok_or("doc")?.doc.layers, before);
    s.execute("shape.create", json!({"kind":"rect","rect":[0,0,4,4]}))?;
    assert!(s.execute("layer.vectorize", json!({})).is_err());
    Ok(())
}
#[test]
fn selection_coordinates_and_cached_smart_object() -> TestResult {
    use photocraft_color::PixelFormat;
    use photocraft_doc::{Layer, SmartObject, SmartSource};
    use photocraft_geom::Affine;
    use photocraft_raster::Surface;
    let mut s = Session::new();
    s.execute("file.new", json!({"width":32,"height":32}))?;
    let smart_id = {
        let d = s.active_mut().ok_or("doc")?;
        let doc = std::sync::Arc::make_mut(&mut d.doc);
        let pixels = Surface::with_default(PixelFormat::RGBA8, &[0., 0., 0., 1.]);
        let smart = SmartObject::new(SmartSource::Linked { path: "/missing-vectorize-test-source.png".into() }, Affine::IDENTITY, Some(pixels));
        let layer = Layer::new("Logo", LayerContent::Smart(smart));
        let id = doc.insert_above(d.active_layer, layer);
        d.active_layer = Some(id);
        let mut selection = Surface::new(PixelFormat::GRAY8);
        for y in 12..22 {
            for x in 10..20 {
                selection.write_pixel(x, y, &[1.]);
            }
        }
        doc.selection = Some(selection);
        id
    };
    let preview = s.execute("layer.vectorize.preview", json!({"layer":smart_id.0,"scale":0.5,"params":{"preset":"black_white"}}))?;
    assert_eq!(preview["bounds"], json!([10, 12, 10, 10]));
    assert_eq!(preview["trace"]["width"], 5);
    let out = s.execute("layer.vectorize", json!({"layer":smart_id.0,"params":{"preset":"black_white"}}))?;
    let id = LayerId(out["group"].as_u64().ok_or("id")?);
    let d = s.active().ok_or("doc")?;
    let child = &d.doc.layer(id).ok_or("group")?.children().ok_or("children")?[0];
    let LayerContent::Shape(shape) = &child.content else {
        return Err("shape".into());
    };
    let (x0, y0, x1, y1) = shape.path.control_bounds().ok_or("bounds")?;
    assert!((x0 - 10.).abs() < 1e-6 && (y0 - 12.).abs() < 1e-6 && (x1 - 20.).abs() < 1e-6 && (y1 - 22.).abs() < 1e-6);
    let area: f64 = photocraft_vector::path_coverage(&shape.path, d.doc.bounds()).into_iter().map(f64::from).sum();
    assert!((area - 100.).abs() < 1e-6, "traced rectangle area {area}");
    Ok(())
}
#[test]
#[cfg(not(target_arch = "wasm32"))]
fn background_cancellation_does_not_apply_group() -> TestResult {
    let mut s = Session::new();
    s.execute("file.new", json!({"width":128,"height":128}))?;
    let count = s.active().ok_or("doc")?.doc.layers.len();
    let started = s.start("layer.vectorize", json!({}))?;
    let photocraft_engine::jobs::Started::Job(id) = started else {
        return Err("did not start worker".into());
    };
    assert!(s.cancel_job(id));
    assert!(s.wait_job(id).is_err());
    assert_eq!(s.active().ok_or("doc")?.doc.layers.len(), count);
    Ok(())
}
#[test]
#[cfg(not(target_arch = "wasm32"))]
fn background_preview_leaves_document_editable() -> TestResult {
    let mut s = Session::new();
    s.execute("file.new", json!({"width":32,"height":32}))?;
    let started = s.start("layer.vectorize.preview", json!({"params":{"preset":"black_white"}}))?;
    let photocraft_engine::jobs::Started::Job(id) = started else {
        return Err("did not start preview worker".into());
    };
    assert!(s.job(id).ok_or("job")?.document.is_none());
    assert!(s.is_enabled("layer.setProps"));
    s.execute("layer.setProps", json!({"name":"Edited during preview"}))?;
    let result = s.wait_job(id)?;
    assert_eq!(result["trace"]["width"], 16);
    assert_eq!(s.active().ok_or("doc")?.doc.layers.len(), 1);
    assert_eq!(s.active().ok_or("doc")?.doc.layers[0].name, "Edited during preview");
    Ok(())
}
#[test]
fn v1_paths_and_vector_masks_survive_vectorize() -> TestResult {
    use photocraft_doc::{Layer, NamedPath, ShapeLayer, VectorMask};
    let mut s = Session::new();
    s.execute("file.new", json!({"width":16,"height":16}))?;
    let id = s.active().ok_or("doc")?.active_layer.ok_or("layer")?;
    let path = photocraft_vector::shapes::rect(2., 2., 12., 12.);
    {
        let state = s.active_mut().ok_or("doc")?;
        let doc = std::sync::Arc::make_mut(&mut state.doc);
        doc.paths.push(NamedPath { name: "Saved".into(), path: path.clone(), psd_raw: None });
        doc.layers.push(Layer::new("Legacy shape", LayerContent::Shape(ShapeLayer { path: path.clone(), ..Default::default() })));
        let bounds = doc.bounds();
        let l = doc.layer_mut(id).ok_or("layer")?;
        l.vector_mask = Some(VectorMask::new(path.clone()));
        l.surface_mut().ok_or("surface")?.fill_rect(bounds, &[0., 0., 0., 1.]);
    }
    let old = photocraft_format::save_to_bytes(&s.active().ok_or("doc")?.doc, &Default::default())?;
    assert_eq!(photocraft_format::FORMAT_VERSION, 1);
    let loaded = photocraft_format::load_from_bytes(&old)?;
    assert!(loaded.layers.iter().any(|l| matches!(l.content, LayerContent::Shape(_))));
    assert_eq!(loaded.paths[0].path, path);
    assert_eq!(loaded.layer(id).ok_or("layer")?.vector_mask.as_ref().ok_or("mask")?.path, path);
    s.execute("layer.vectorize", json!({"params":{"preset":"black_white"}}))?;
    let new = photocraft_format::save_to_bytes(&s.active().ok_or("doc")?.doc, &Default::default())?;
    let roundtrip = photocraft_format::load_from_bytes(&new)?;
    assert_eq!(roundtrip.paths[0].path, path);
    assert_eq!(roundtrip.layers.len(), 3);
    Ok(())
}
