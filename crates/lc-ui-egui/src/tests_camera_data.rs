//! Real pointer-input coverage of camera presets through the existing headless harness.
use crate::{
    LightcraftApp, Services,
    headless::Headless,
    state::{RightPanel, ViewMode},
};
use lightcraft_catalog::Source;
use lightcraft_pipeline::{CameraWbPreset, SourceInfo};
use serde_json::json;
use std::{sync::Arc, time::Duration};
#[test]
fn camdata_camera_wb_dropdown_click_and_undo() {
    for dark in [true, false] {
        let mut session = lightcraft_engine::Session::with_demo();
        let id = session.active().unwrap();
        session
            .catalog
            .apply(lightcraft_catalog::Op::Relink {
                id,
                file_name: "camera-test.raw".into(),
                source: Source::File { path: "camera-test.raw".into() },
                format: Some("RAW".into()),
            })
            .unwrap();
        session.media.file_loader = Some(Arc::new(|_, _| {
            Ok((
                lightcraft_raster::Rgb32f::new(32, 32),
                SourceInfo {
                    raw: true,
                    camera_wb_presets: vec![CameraWbPreset { name: "Underwater".into(), temp: 4600.0, tint: 18.0 }],
                    ..Default::default()
                },
            ))
        }));
        session.source_now(id, lightcraft_engine::SourceLevel::Thumb).unwrap();
        let mut app = LightcraftApp::new(session, Services { png: None, ..Default::default() });
        app.ui.settings.gpu = false;
        app.ui.view = ViewMode::Detail;
        app.ui.right = RightPanel::Edit;
        app.ui.right_width = 400.0;
        app.ui.histogram = false;
        app.ui.left_panel = false;
        app.ui.filmstrip = false;
        app.ui.open_sections = vec!["light".into()]; // WB lives in Basic since the Lightroom Classic layout
        app.set_host_theme(if dark { Default::default() } else { super::tests_develop_c::light() });
        let mut h = Headless::new(app, [1200.0, 1800.0], 1.0);
        h.step();
        h.step();
        let t = Duration::from_secs(30);
        for widget in ["dropdown:wbMode", "wbPreset:Underwater"] {
            let result = h.request("ui.clickWidget", json!({"id":widget}), t);
            assert_eq!(result["ok"], true, "{widget}: {result}");
            h.step();
        }
        let edited = h.app.session.develop_of(id).unwrap();
        assert_eq!((edited.wb.temp, edited.wb.tint), (4600.0, 18.0));
        assert_eq!(edited.wb.mode, lightcraft_develop::WbMode::Custom);
        let result = h.request("engine.execute", json!({"command":"edit.undo","params":{}}), t);
        assert_eq!(result["ok"], true, "{result}");
        assert_ne!(h.app.session.develop_of(id).unwrap().wb.temp, 4600.0);
    }
}
