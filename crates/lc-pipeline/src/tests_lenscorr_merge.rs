//! Merge contracts: one provider, one CA correction, one shading gain.
#![allow(clippy::unwrap_used)]
use crate::{
    RenderRequest, SourceInfo, frame_for,
    geometry::Frame,
    lcp::{LcpCorrection, LcpModel},
    lensdb::{Distortion, LensCorrection, Tca},
};
use li_raster::RgbFrame;
use lightcraft_develop::{DevelopSettings, EmbeddedLens, EmbeddedVignette, SonyLensData};
use lightcraft_geom::{Orientation, Point};

fn profile() -> LcpCorrection {
    let base = LcpModel { focal: [1.0; 2], center: [0.5; 2], radial: [0.0; 3], tangential: [0.0; 2], scale: 1.0 };
    LcpCorrection {
        distortion: Some(base),
        chromatic: Some([LcpModel { scale: 1.002, ..base }, base, LcpModel { scale: 0.998, ..base }]),
        vignette: Some(LcpModel { radial: [-0.2, 0.0, 0.0], ..base }),
        exif_orientation: Orientation::Rotate90,
    }
}
#[test]
fn lenscorr_merge_selects_embedded_then_lensfun_then_lcp_without_stacking() {
    let src = RgbFrame::filled(120, 80, [0.2; 3]);
    let db = LensCorrection {
        diag_norm: 2.0,
        distortion: Distortion::Poly3 { k1: -0.1 },
        tca: Tca::Linear { kr: 1.001, kb: 0.999 },
        vignetting: Some([-0.2, 0.0, 0.0]),
        ..Default::default()
    };
    let embedded = EmbeddedLens {
        vignette: Some(EmbeddedVignette { k: [0.2, 0.0, 0.0, 0.0, 0.0], center: Point::new(0.5, 0.5), radius: 0.6 }),
        ..Default::default()
    };
    let mut s = DevelopSettings::default();
    s.optics.lens_profile = true;
    s.optics.remove_ca = true;
    s.lens_db.enabled = true;
    let mut reverse = crate::optics::Warp::from_settings(120.0, 80.0, &s, None);
    reverse.set_lcp(&profile(), Orientation::Normal, 120.0, 80.0, &s);
    reverse.set_lensdb(&db, Orientation::Normal, 120.0, 80.0, &s);
    assert!(reverse.lcp.is_none() && reverse.lensdb.is_some(), "setter order must not stack providers");
    let mut info = SourceInfo {
        lens: Some(embedded),
        lens_db: Some(db),
        lcp: Some(profile()),
        lens_camera_to_working: Some(li_color::Mat3::IDENTITY),
        ..Default::default()
    };
    let w = frame_for(&src, &info, &s, false).warp.unwrap();
    assert!(w.lens.is_some() && w.lensdb.is_none() && w.lcp.is_none());
    assert!(
        (w.gain(Point::new(0.5, 0.5)) - crate::optics::embedded_vignette_gain(&embedded.vignette.unwrap(), Point::new(0.5, 0.5), 120.0, 80.0) as f32)
            .abs()
            < 1e-6
    );
    info.lens = None;
    let w = frame_for(&src, &info, &s, false).warp.unwrap();
    assert!(w.lens.is_none() && w.lensdb.is_some() && w.lcp.is_none());
    assert_eq!(w.ca, [0.0; 3]);
    assert!(w.camera_basis.is_some());
    info.lens_db = None;
    let frame = frame_for(&src, &info, &s, false);
    let w = frame.warp.as_ref().unwrap();
    assert!(w.lens.is_none() && w.lensdb.is_none() && w.lcp.is_some());
    assert_eq!(w.ca, [0.0; 3]);
    assert!(w.camera_basis.is_some());
    assert!(!frame.gpu_samplable(), "preserve the bounded LCP CPU geometry route");
    let metadata = frame.for_metadata();
    let mw = metadata.warp.as_ref().unwrap();
    assert!(!mw.per_channel() && !mw.has_gain() && mw.camera_basis.is_none());
    assert_eq!(mw.to_source(Point::new(20.0, 30.0), 1), w.to_source(Point::new(20.0, 30.0), 1));
    s.optics.lens_profile = false;
    assert!(frame_for(&src, &info, &s, false).warp.is_none());
}
#[test]
fn lenscorr_merge_sony_handoff_and_baked_dng_do_not_receive_fallbacks() {
    let src = RgbFrame::filled(120, 80, [0.2; 3]);
    let sony =
        SonyLensData { distortion: vec![0, -100, -200], ca_red: vec![0, 500, 1000], ca_blue: vec![0, -500, -1000], vignetting: vec![0, 1000, 2000] };
    let mut s = DevelopSettings::default();
    s.optics.lens_profile = true;
    s.optics.remove_ca = true;
    s.lens_db.enabled = true;
    let mut info = SourceInfo {
        sony_lens: Some(sony),
        lcp: Some(profile()),
        lens_db: Some(LensCorrection { diag_norm: 2.0, distortion: Distortion::Poly3 { k1: -0.1 }, ..Default::default() }),
        ..Default::default()
    };
    let w = frame_for(&src, &info, &s, false).warp.unwrap();
    assert!(w.lens.unwrap().radial.is_some() && w.lcp.is_none() && w.lensdb.is_none());
    assert_eq!(w.ca, [0.0; 3]);
    info.embedded_lens_baked = true;
    info.embedded_ca_baked = true;
    assert!(frame_for(&src, &info, &s, false).warp.is_none());
    s.optics.distortion = 25.0;
    let w = frame_for(&src, &info, &s, false).warp.unwrap();
    assert!(w.lens.is_none() && w.lensdb.is_none() && w.lcp.is_none());
    assert_ne!(w.k1, 0.0);
    assert_eq!(w.ca, [0.0; 3]);
    let old: SourceInfo = serde_json::from_value(serde_json::json!({"raw":true})).unwrap();
    assert!(!old.embedded_lens_baked && !old.embedded_ca_baked);
    // Preserve noise calibration keys alongside the deferred fitted look key.
    let request = RenderRequest::fit(120, 80);
    let mut noise = SourceInfo::default();
    let a = crate::plan(&src, &noise, &s, &request).lin_key;
    noise.camera_noise =
        Some(crate::detail::nr::NoiseCalibration { a: [0.001; 3], b: [0.0001; 3], camera_to_working: li_color::Mat3::IDENTITY.to_f32() });
    assert_ne!(a, crate::plan(&src, &noise, &s, &request).lin_key);
    let direct = Frame::with_lcp(120, 80, &s, false, None, None, Some(&profile()));
    assert!(direct.warp.is_some());
}
