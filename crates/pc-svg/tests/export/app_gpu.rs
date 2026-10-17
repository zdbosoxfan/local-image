//! Full PhotocraftApp offscreen canvas: pointer preview/release then public export/import.
//! Registered in pc-ui-egui/tests/gpu/main.rs; it is not an
//! isolated-widget proxy. No software adapters or successful skips are accepted.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::export_app_gpu::{dragged_shape_app, parity, present};
use photocraft_doc::vector_design::exchange::ImportOptions;
use photocraft_doc::{ColorMode, Document, SampleType};
use photocraft_engine::vector_packages::svg_export::{Request, Scope};
#[test]
fn svg_export_app_real_gpu_preview_release_and_roundtrip() {
    let _gpu_gate = crate::gpu_lock();

    let (mut h, gpu, original, _) = dragged_shape_app("SVG");
    let st = h.state().session.active().unwrap();
    let request = Request { document: st.doc.id.0, revision: st.revision, scope: Scope::Document, options: Default::default() };
    let output = h.state_mut().session.execute("file.svg.export", serde_json::to_value(&request).unwrap()).unwrap();
    let bytes: Vec<u8> = serde_json::from_value(output["bytes"].clone()).unwrap();
    let scene = photocraft_svg::import::import(&bytes, &ImportOptions { dpi: 96., ..Default::default() }).unwrap();
    let mut imported = Document::new("Imported", scene.size, ColorMode::Rgb, SampleType::U8);
    imported.layers = scene.layers;
    imported.resolution_dpi = scene.resolution_dpi;
    let a = photocraft_compose::flatten(&original);
    let b = photocraft_compose::flatten(&imported);
    let max =
        a.px.iter()
            .zip(&b.px)
            .flat_map(|(a, b)| (0..4).map(move |k| (if k == 3 { a[k] - b[k] } else { a[k] * a[3] - b[k] * b[3] }).abs()))
            .fold(0_f32, f32::max);
    assert!(max <= 1.0 / 255.0, "full app export/reimport max {max}");
    h.state_mut().session.open_document(imported, None);
    h.state_mut().sync_views();
    present(&mut h);
    parity(&gpu, &h.state().session.active().unwrap().doc, "SVG");
}
