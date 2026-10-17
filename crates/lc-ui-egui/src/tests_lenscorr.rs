//! Profile master and lens database fallback are exercised through real headless pointer input.
use crate::{
    LightcraftApp, Services,
    headless::Headless,
    state::{RightPanel, ViewMode},
    theme::Tokens,
};
use lightcraft_develop::{DevelopSettings, EmbeddedLens, EmbeddedVignette};
use serde_json::json;
use std::time::Duration;

fn click(h: &mut Headless, id: &str) {
    click_at(h, id, 0.5);
}
fn click_at(h: &mut Headless, id: &str, fraction: f32) {
    let reply = h.request("ui.clickWidget", json!({"id": id, "fx": fraction}), Duration::from_secs(20));
    assert_eq!(reply["ok"], true, "{id}: {reply}");
    h.step();
}
fn settings(h: &Headless) -> DevelopSettings {
    h.app.session.develop_of(h.app.session.active().unwrap()).unwrap().as_ref().clone()
}
fn rect(h: &Headless, id: &str) -> egui::Rect {
    h.app.widgets.iter().rev().find(|(name, _)| name == id).unwrap_or_else(|| panic!("missing {id}")).1
}
fn screenshot(h: &Headless, name: &str) {
    let image = h.paint();
    let panel = rect(h, "panel:right_panel");
    let scale = h.pixels_per_point;
    let x0 = (panel.left() * scale).max(0.0) as usize;
    let x1 = (panel.right() * scale).ceil().min(image.size[0] as f32) as usize;
    let out = li_raster::Rgba8::from_fn(x1 - x0, image.size[1], |x, y| image.pixels[y * image.size[0] + x + x0].to_array());
    let png = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&out), &Default::default()).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/design-system/lenscorr");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.png")), png).unwrap();
}

#[test]
fn lenscorr_master_and_database_switches_click_and_preserve_precedence() {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    app.ui.view = ViewMode::Detail;
    app.ui.right = RightPanel::Edit;
    app.ui.histogram = false;
    app.ui.left_panel = false;
    app.ui.filmstrip = false;
    app.ui.open_sections = vec!["optics".into()];
    app.session.execute("develop.merge", &json!({"settings": {"optics": {"lens_profile": false}, "lens_db": {"enabled": false}}})).unwrap();
    let mut h = Headless::new(app, [1000.0, 1600.0], 1.0);
    h.step();
    h.step();
    assert!(!settings(&h).optics.lens_profile);
    click(&mut h, "optics:lensProfile");
    assert!(settings(&h).optics.lens_profile && settings(&h).lens_db.enabled);
    click(&mut h, "optics:lensProfile");
    assert!(!settings(&h).optics.lens_profile && settings(&h).lens_db.enabled);
    click(&mut h, "lensDb:enabled");
    assert!(!settings(&h).lens_db.enabled);
    click(&mut h, "lensDb:enabled");
    assert!(settings(&h).lens_db.enabled && settings(&h).optics.lens_profile);
    click(&mut h, "optics:lensProfile");
    assert!(settings(&h).lens_db.enabled && !settings(&h).optics.lens_profile);
    assert!(super::tests_develop_c::texts(&h).contains("Enable Lens Corrections to apply"));
    click(&mut h, "lensDb:enabled");
    assert!(!settings(&h).lens_db.enabled && !settings(&h).optics.lens_profile);
    click(&mut h, "lensDb:enabled");
    assert!(settings(&h).lens_db.enabled && settings(&h).optics.lens_profile);

    // Vignette-only embedded data: lens database amounts and the absent distortion channel
    // must remain inactive, while the available embedded vignetting amount can be changed.
    let id = h.app.session.active().unwrap();
    let mut photo = h.app.session.catalog.photo(id).unwrap().as_ref().clone();
    photo.embedded_lens = Some(EmbeddedLens {
        vignette: Some(EmbeddedVignette { radius: 0.5, center: lightcraft_geom::Point::new(0.5, 0.5), k: [0.1, 0.0, 0.0, 0.0, 0.0] }),
        ..Default::default()
    });
    let header_lens = photo.embedded_lens;
    photo.source = lightcraft_catalog::Source::File { path: "/synthetic/header-vendor-lens.arw".into() };
    photo.kind = lightcraft_catalog::MediaKind::Raw;
    photo.format = "ARW".into();
    photo.width = 32;
    photo.height = 24;
    h.app.session.catalog.apply(lightcraft_catalog::Op::RemovePhoto { id }).unwrap();
    h.app.session.catalog.apply(lightcraft_catalog::Op::AddPhoto { photo: Box::new(photo) }).unwrap();
    h.app.session.media.file_loader = Some(std::sync::Arc::new(move |_, _| {
        Ok((
            li_raster::RgbFrame::from_fn(32, 24, |_, _| [0.2, 0.3, 0.4]),
            lightcraft_pipeline::SourceInfo { raw: true, lens: header_lens, ..Default::default() },
        ))
    }));
    h.app.session.media.clear_sources();
    assert_eq!(h.app.session.source_info(id).lens, header_lens, "uncached source uses its catalog header");
    h.app.session.source_now(id, lightcraft_engine::media::SourceLevel::Preview).unwrap();
    h.app.session.execute("develop.merge", &json!({"settings": {"lens_db": {"lens": {"maker": "Example", "model": "A very long lens name that must elide in the narrow Optics panel"}}}})).unwrap();
    h.step();
    h.step();
    assert!(super::tests_develop_c::texts(&h).contains("Embedded lens corrections take priority"));
    let before = settings(&h);
    click_at(&mut h, "slider:lensDb.distortion", 0.8);
    click_at(&mut h, "slider:optics.profileDistortion", 0.8);
    assert_eq!(settings(&h).lens_db.distortion, before.lens_db.distortion);
    assert_eq!(settings(&h).optics.profile_distortion, before.optics.profile_distortion);
    let reply =
        h.request("ui.dragWidget", json!({"id": "slider:optics.profileVignetting", "dx": 20.0, "dy": 0.0, "steps": 3}), Duration::from_secs(20));
    assert_eq!(reply["ok"], true, "{reply}");
    h.step();
    assert_ne!(settings(&h).optics.profile_vignetting, before.optics.profile_vignetting);

    // A photo without the embedded lens field (a host or probe that skips it), but decoding
    // discovers vendor data. The UI must use the same cached source facts as rendering, without
    // probing the file.
    let mut unprobed = h.app.session.catalog.photo(id).unwrap().as_ref().clone();
    let decoded_lens = unprobed.embedded_lens.unwrap();
    unprobed.embedded_lens = None;
    unprobed.source = lightcraft_catalog::Source::File { path: "/synthetic/unprobed-vendor-lens.arw".into() };
    unprobed.width = 32;
    unprobed.height = 24;
    h.app.session.catalog.apply(lightcraft_catalog::Op::RemovePhoto { id }).unwrap();
    h.app.session.catalog.apply(lightcraft_catalog::Op::AddPhoto { photo: Box::new(unprobed) }).unwrap();
    h.app.session.media.file_loader = Some(std::sync::Arc::new(move |_, _| {
        Ok((
            li_raster::RgbFrame::from_fn(32, 24, |_, _| [0.2, 0.3, 0.4]),
            lightcraft_pipeline::SourceInfo { raw: true, lens: Some(decoded_lens), ..Default::default() },
        ))
    }));
    h.app.session.media.clear_sources();
    h.app.session.source_now(id, lightcraft_engine::media::SourceLevel::Preview).unwrap();
    assert!(h.app.session.catalog.photo(id).unwrap().embedded_lens.is_none());
    assert_eq!(h.app.session.source_info(id).lens, Some(decoded_lens));
    h.step();
    h.step();
    let text = super::tests_develop_c::texts(&h);
    assert!(text.contains("Embedded lens corrections available"));
    assert!(text.contains("Embedded lens corrections take priority"));
    assert!(!text.contains("No embedded lens corrections"));
    let before = settings(&h);
    click_at(&mut h, "slider:lensDb.distortion", 0.8);
    click_at(&mut h, "slider:optics.profileDistortion", 0.8);
    assert_eq!(settings(&h).lens_db.distortion, before.lens_db.distortion);
    assert_eq!(settings(&h).optics.profile_distortion, before.optics.profile_distortion);
    click_at(&mut h, "slider:optics.profileVignetting", 0.7);
    assert_ne!(settings(&h).optics.profile_vignetting, before.optics.profile_vignetting);

    // The RAW Look controls share the panel width and retain their existing settings behavior.
    click(&mut h, "combo:developLook");
    click(&mut h, "look:sigmoid");
    assert_eq!(settings(&h).look, lightcraft_develop::Look::Sigmoid);
    click(&mut h, "dropdown:lookBase");
    click(&mut h, "lookBase:Linear");
    assert_eq!(settings(&h).look_options.base, lightcraft_develop::ToneBase::Linear);
    click_at(&mut h, "slider:look.huePreservation", 0.6);
    assert_eq!(settings(&h).look_options.hue_preservation, 60.0);

    for dark in [true, false] {
        for width in [250.0, 400.0] {
            for scale in [1.0, 1.5] {
                h.app.set_host_theme(if dark { Tokens::default() } else { super::tests_develop_c::light() });
                h.app.restyle(&h.view.ctx);
                assert_eq!(Tokens::get(&h.view.ctx).dark, dark);
                h.app.ui.right_width = width;
                h.pixels_per_point = scale;
                h.step();
                h.step();
                let panel = rect(&h, "panel:right_panel");
                let right = h.size.x - Tokens::get(&h.view.ctx).strip_w;
                let left = right - width;
                assert!((panel.width() - width).abs() <= 1.0, "panel grew beyond declared {width}: {panel:?}");
                assert!((panel.right() - right).abs() <= 1.0, "panel must end before the tool strip: {panel:?}");
                for id in [
                    "optics:lensProfile",
                    "lensDb:enabled",
                    "dropdown:lensDbLens",
                    "combo:developLook",
                    "dropdown:lookBase",
                    "slider:look.huePreservation",
                ] {
                    let r = rect(&h, id);
                    assert!(r.left() >= left && r.right() <= right, "{id}: {r:?} outside declared panel [{left}, {right}]");
                }
                screenshot(&h, &format!("{}-{width}-{scale}", if dark { "dark" } else { "light" }));
            }
        }
    }
}

#[test]
fn lenscorr_merge_profile_picker_and_master_share_the_optics_panel() {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    app.ui.view = ViewMode::Detail;
    app.ui.right = RightPanel::Edit;
    app.ui.histogram = false;
    app.ui.left_panel = false;
    app.ui.filmstrip = false;
    app.ui.open_sections = vec!["optics".into()];
    let mut h = Headless::new(app, [1000.0, 1600.0], 1.0);
    h.step();
    h.step();
    click(&mut h, "combo:lensProfile");
    click(&mut h, "choice:lensProfile:none");
    assert_eq!(settings(&h).lens_profile, "none");
    click(&mut h, "optics:lensProfile");
    assert!(settings(&h).optics.lens_profile);
    assert_eq!(settings(&h).lens_profile, "none", "master must preserve imported profile choice");
}
