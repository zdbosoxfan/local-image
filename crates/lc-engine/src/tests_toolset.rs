//! Toolset upgrades in the engine: the raw loader's default path (demosaic, highlight
//! reconstruction, no capture sharpening, no lens database) stays bit-identical — golden hashes
//! recorded before the raw options existed — and the new options change what they should.

use lightcraft_raster::Rgb32f;

/// A 640 × 480 Bayer DNG with texture, colour and a clipped highlight.
pub(crate) fn textured_dng() -> Vec<u8> {
    use lightcraft_raw::*;
    let (w, h) = (640usize, 480usize);
    let cfa = Cfa::bayer("RGGB").unwrap();
    let data: Vec<u16> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let c = cfa.color_at(x, y) as usize;
            let base = 400.0 + 9000.0 * (x as f32 / w as f32) * [0.8, 1.0, 0.6][c];
            let checker = if (x / 24 + y / 24) % 2 == 0 { 1.0 } else { 0.55 };
            let fine = 1.0 + 0.08 * (((x * 7 + y * 13) % 11) as f32 / 11.0 - 0.5);
            let v = 256.0 + base * checker * fine;
            let v = if (x as i32 - 520).pow(2) + (y as i32 - 120).pow(2) < 60 * 60 { 20000.0 } else { v };
            v.min(16000.0) as u16
        })
        .collect();
    let raw = RawImage {
        format: RawFormat::Dng,
        width: w,
        height: h,
        cpp: 1,
        data: RawData::U16(data),
        cfa: Some(cfa),
        bits: 14,
        black: BlackLevel::uniform(256.0),
        white: vec![16000.0],
        active_area: Rect::new(0, 0, w, h),
        crop: Rect::new(0, 0, w, h),
        orientation: Orientation::Normal,
        color: ColorData {
            illuminant: [17, 21],
            color_matrix: [
                Some(lightcraft_color::Mat3([[0.9, 0.2, -0.15], [-0.3, 1.25, 0.08], [0.02, -0.12, 0.85]])),
                Some(lightcraft_color::Mat3([[0.7, 0.3, -0.1], [-0.35, 1.3, 0.1], [0.05, -0.2, 1.0]])),
            ],
            as_shot_neutral: Some([0.5, 1.0, 0.7]),
            ..Default::default()
        },
        wb_multipliers: None,
        linearized: false,
        opcodes: Default::default(),
        metadata: Default::default(),
    };
    write_dng(&raw, &Default::default()).unwrap()
}

pub(crate) fn hash_img(img: &Rgb32f) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in &img.data {
        for v in p {
            for b in v.to_bits().to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        }
    }
    h ^ ((img.width as u64) << 32 | img.height as u64)
}

/// Loader output hashes at a binned preview size, a bilinear thumbnail size and full size
/// (AHD), recorded on x86_64 Linux before the raw options existed.
const GOLDEN: [(usize, u64); 3] = [(200, 0xa0de_2a87_4e35_4a66), (500, 0x1cc8_83f3_9dae_2c30), (usize::MAX, 0x96a3_63c6_ffd0_f874)];

#[test]
fn default_raw_loading_is_bit_identical() {
    let dng = textured_dng();
    let mut got = Vec::new();
    for (edge, _) in GOLDEN {
        let (img, _) = crate::files::load_bytes(&dng, edge).unwrap();
        got.push((edge, hash_img(&img)));
    }
    if std::env::var_os("LC_GOLDEN_PRINT").is_some() {
        println!("{got:x?}");
    }
    if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        for ((edge, h), (_, g)) in got.iter().zip(GOLDEN) {
            assert_eq!(*h, g, "load at {edge}: source changed");
        }
    }
}

#[test]
fn raw_options_change_the_decoding() {
    use crate::files::{RawOptions, load_bytes, load_bytes_with};
    use lightcraft_develop::{Demosaic, HighlightMode};
    let dng = textured_dng();
    let (base, info) = load_bytes(&dng, usize::MAX).unwrap();
    assert_eq!((info.sensor_scale, info.capture_radius), (1.0, None));
    assert!((0.2..=0.5).contains(&info.capture_threshold), "{}", info.capture_threshold);
    for opts in [
        RawOptions { demosaic: Demosaic::Rcd, ..Default::default() },
        RawOptions { demosaic: Demosaic::DualRcd, dual_threshold: 0.2, ..Default::default() },
        RawOptions { highlights: HighlightMode::Opposed, ..Default::default() },
        RawOptions { highlights: HighlightMode::Clip, ..Default::default() },
    ] {
        let (img, _) = load_bytes_with(&dng, usize::MAX, &opts).unwrap();
        assert_ne!(hash_img(&img), hash_img(&base), "{opts:?}");
        assert_ne!(opts.key(), 0);
    }
    assert_eq!(RawOptions::default().key(), 0);
    // the radius is measured when asked for; a binned preview knows its scale
    let (_, i) = load_bytes_with(&dng, usize::MAX, &RawOptions { capture_radius: true, ..Default::default() }).unwrap();
    assert!(i.capture_radius.is_some_and(|r| (0.0..=1.5).contains(&r)));
    let (_, i) = load_bytes(&dng, 200).unwrap();
    assert!((i.sensor_scale - 3.2).abs() < 0.01, "{}", i.sensor_scale);
}

#[test]
fn sources_are_cached_per_raw_options() {
    use serde_json::json;
    let dir = std::env::temp_dir().join(format!("lc-toolset-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.dng");
    std::fs::write(&path, textured_dng()).unwrap();
    let mut s = crate::Session::new().with_fs();
    let r = s.execute("library.import", &json!({"paths": [path.to_string_lossy()]})).unwrap();
    let id = lightcraft_catalog::PhotoId(r["imported"][0].as_u64().unwrap());
    s.execute("library.select", &json!({"ids": [id.0]})).unwrap();
    let a = s.render_now(id, 640, 480).unwrap().image;
    // a default-decoded source is cached
    let p = s.catalog.photo(id).unwrap().clone();
    assert!(matches!(s.media.source_ref(&p, crate::SourceLevel::Preview), crate::media::SourceRef::Loaded(_)));
    s.execute("develop.merge", &json!({"settings": {"raw": {"demosaic": "rcd", "highlights": "opposed"}}})).unwrap();
    let p = s.catalog.photo(id).unwrap().clone();
    assert!(matches!(s.media.source_ref(&p, crate::SourceLevel::Preview), crate::media::SourceRef::RawFile { .. }), "decoded again");
    let b = s.render_now(id, 640, 480).unwrap().image;
    assert_ne!(a.data, b.data);
    let p = s.catalog.photo(id).unwrap().clone();
    assert!(matches!(s.media.source_ref(&p, crate::SourceLevel::Preview), crate::media::SourceRef::Loaded(_)), "and cached");
    // back to the defaults: the original render
    s.execute("develop.merge", &json!({"settings": {"raw": {"demosaic": "auto", "highlights": "reconstruct"}}})).unwrap();
    assert_eq!(s.render_now(id, 640, 480).unwrap().image.data, a.data);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lens_database_corrections_follow_the_settings() {
    let db = crate::lens_db::database().unwrap();
    // a fully calibrated lens and a camera for it
    let lens = db
        .lenses
        .iter()
        .find(|l| l.calib_distortion.len() > 1 && l.calib_vignetting.len() > 2 && !l.calib_tca.is_empty())
        .unwrap();
    let cam = db.cameras.iter().find(|c| lens.mounts.contains(&c.mount)).unwrap();
    let mut p = lightcraft_catalog::Photo::new(
        lightcraft_catalog::PhotoId(1),
        lightcraft_catalog::Source::File { path: "x.nef".into() },
        "x.nef",
        "NEF",
        6000,
        4000,
        "2026-10-09",
    );
    p.meta.camera = format!("{} {}", cam.maker, cam.model);
    p.meta.lens = lens.model.clone();
    p.meta.focal_mm = Some(lens.calib_distortion[0].focal);
    p.meta.aperture = Some(lens.calib_vignetting[0].aperture);
    let mut s = lightcraft_develop::DevelopSettings::default();
    assert_eq!(crate::lens_db::for_photo(&p, &s), None, "off by default");
    s.lens_db.enabled = true;
    s.lens_db.lens = Some(lightcraft_develop::LensName { maker: lens.maker.clone(), model: lens.model.clone() });
    assert!(crate::lens_db::for_photo(&p, &s).is_some());
    assert!(crate::media::job_info(&p, &s).lens_db.is_some());
    s.set_section_enabled("optics", false);
    assert_eq!(crate::lens_db::for_photo(&p, &s), None);
}
