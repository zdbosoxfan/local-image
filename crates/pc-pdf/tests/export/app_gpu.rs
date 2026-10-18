//! Full PhotocraftApp offscreen canvas: pointer preview/release then public export; the public import gate needs the `pdf-import-integrated` feature.
//! Registered in pc-ui-egui/tests/gpu/main.rs; it is not an
//! isolated-widget proxy. No software adapters or successful skips are accepted.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::export_app_gpu::{dragged_shape_app, parity, present};
use egui_kittest::{Harness, kittest::Queryable};
#[cfg(feature = "pdf-import-integrated")]
use photocraft_doc::vector_design::exchange::ImportOptions;
use photocraft_doc::{ColorMode, Document, Layer, LayerContent, SampleType};
use photocraft_engine::vector_packages::pdf_export::{Request, Scope};
use photocraft_ui_egui::{PhotocraftApp, gpu_canvas::GpuCanvas};
use serde_json::json;
use std::sync::{Arc, Mutex};
#[test]
fn pdf_export_app_real_gpu_preview_release_and_menu_dialog() {
    let _gpu_gate = crate::gpu_lock();

    let (mut h, gpu, original, revision) = dragged_shape_app("PDF");
    let st = h.state().session.active().unwrap();
    let request = Request { document: st.doc.id.0, revision: st.revision, scope: Scope::Document, options: Default::default() };
    let output = h.state_mut().session.execute("file.pdf.export", serde_json::to_value(&request).unwrap()).unwrap();
    let bytes: Vec<u8> = serde_json::from_value(output["bytes"].clone()).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.7"));
    let written = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let record = written.clone();
    h.state_mut().services.pick_save = Some(Box::new(|_| Some("fixture.pdf".into())));
    h.state_mut().services.write = Some(Box::new(move |_, bytes| {
        record.lock().unwrap().push(bytes.to_vec());
        Ok(())
    }));
    // Export As is enabled where the app has its raster exporter; PDF itself goes through pick_save/write.
    h.state_mut().services.export = Some(std::sync::Arc::new(|_, _, _| Err("raster export is not used by this PDF test".into())));
    h.get_by_label("File").click();
    h.run_steps(2);
    h.get_by_label("Export ⏵").hover();
    h.run_steps(3);
    // The visible PDF… item became a format of the unified Export As dialog (3b4604a1).
    h.get_by_label_contains("Export As…").click();
    h.run_steps(3);
    h.get(egui_kittest::kittest::By::new().role(egui::accesskit::Role::ComboBox).value("PNG")).click();
    h.run_steps(2);
    h.get_by_label("PDF").click();
    h.run_steps(3);
    h.get_by_label("Text as curves").click();
    h.run_steps(2);
    h.get_by_label("Compress PDF").click();
    h.run_steps(2);
    h.get_by_label("Export").click();
    h.run_steps(3);
    assert!(h.state().ui.pdf_export.error.is_none(), "{:?}", h.state().ui.pdf_export.error);
    assert_eq!(written.lock().unwrap().len(), 1);
    assert!(h.state().ui.pdf_export.request.is_none());
    assert_eq!(h.state().session.active().unwrap().revision, revision + 1);
    // A real raster layer produces the normal, visible app warning notice.
    let format = h.state().session.active().unwrap().doc.pixel_format();
    h.state_mut()
        .session
        .edit("Fixture pixel layer", |doc, _| {
            let mut surface = photocraft_doc::Surface::new(format);
            surface.write_region(photocraft_geom::Rect::new(8, 8, 16, 16), &[0.2, 0.1, 0.5, 1.0].repeat(64));
            doc.layers.push(Layer::new("Pixels", LayerContent::Raster(surface)));
            Ok(())
        })
        .unwrap();
    let ctx = h.ctx.clone();
    photocraft_ui_egui::menus::invoke(h.state_mut(), &ctx, "file.export.exportAs", json!({ "format": "pdf" })).unwrap();
    h.run_steps(3);
    h.get_by_label("Export").click();
    h.run_steps(3);
    assert_eq!(written.lock().unwrap().len(), 2);
    assert!(!h.state().ui.notices.is_empty());
    assert!(h.state().ui.notices.iter().any(|n| n.lines.iter().any(|line| line.contains("embedded"))));
    let warning = h.state().ui.notices.iter().flat_map(|n| &n.lines).find(|line| line.contains("embedded")).unwrap().clone();
    // Notice lines are shown under Details / History since the compact notice row (f0558183).
    h.get_by_label("Details / History").click();
    h.run_steps(2);
    let _ = h.get_by_label(&warning);

    #[cfg(feature = "pdf-import-integrated")]
    roundtrip(&mut h, &gpu, &original, &bytes);
}
#[cfg(feature = "pdf-import-integrated")]
fn roundtrip(h: &mut Harness<'static, PhotocraftApp>, gpu: &GpuCanvas, original: &Document, bytes: &[u8]) {
    let scene = photocraft_pdf::import::import(bytes, &ImportOptions { dpi: original.resolution_dpi, ..Default::default() }).unwrap();
    let mut imported = Document::new("Imported", scene.size, ColorMode::Rgb, SampleType::U8);
    imported.layers = scene.layers;
    imported.resolution_dpi = scene.resolution_dpi;
    let a = photocraft_compose::flatten(original);
    let b = photocraft_compose::flatten(&imported);
    assert_eq!(a.rect, b.rect);
    let max =
        a.px.iter()
            .zip(&b.px)
            .flat_map(|(a, b)| (0..4).map(move |k| (if k == 3 { a[k] - b[k] } else { a[k] * a[3] - b[k] * b[3] }).abs()))
            .fold(0_f32, f32::max);
    assert!(max <= 1. / 255., "PDF import/export premultiplied diff {max}");
    h.state_mut().session.open_document(imported, None);
    h.state_mut().sync_views();
    present(h);
    parity(gpu, &h.state().session.active().unwrap().doc, "PDF");
}
