//! Toolset upgrades (demosaic / capture sharpening / highlights / lens database / tone equalizer
//! / colour calibration / film looks / depth masks): golden hashes pinning renders and settings
//! hashes of settings written before those tools existed. Every new tool is off by default, so
//! these must stay bit-identical. Recorded on x86_64 Linux right after develop layers landed.

use lightcraft_develop::{DevelopSettings, EmbeddedLens, EmbeddedVignette, EmbeddedWarp};
use lightcraft_geom::Point;
use lightcraft_raster::{Rgb32f, Rgba8};
use serde_json::{Value, json};

use crate::{RenderRequest, SourceInfo, render};

const W: usize = 200;
const H: usize = 136;

fn noise(x: usize, y: usize, k: u32) -> f32 {
    let mut v = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ k.wrapping_mul(0xcb1a_b31f);
    v ^= v >> 13;
    v = v.wrapping_mul(0x5bd1_e995);
    v ^= v >> 15;
    (v & 0xffff) as f32 / 32768.0 - 1.0
}

/// Scene-linear test photo: luminance ramp, hue bands, a checker edge, noise, clipped corner.
fn scene() -> Rgb32f {
    Rgb32f::from_fn(W, H, |x, y| {
        let l = 0.003 * 1.035f32.powi(x as i32);
        let tint = [[1.0, 0.45, 0.3], [0.9, 0.8, 0.25], [0.35, 0.9, 0.35], [0.3, 0.7, 1.0], [0.75, 0.4, 0.95], [0.8, 0.8, 0.8]][y * 6 / H];
        let edge = if (x / 17 + y / 17) % 2 == 0 { 1.0 } else { 0.65 };
        let n = 1.0 + 0.04 * noise(x, y, 11);
        let c = [l * tint[0] * edge * n, l * tint[1] * edge * n, l * tint[2] * edge * n];
        if x > W - 12 && y < 12 { [4.0, 3.5, 3.0] } else { c }
    })
}

fn fnv(img: &Rgba8) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in &img.data {
        for b in p {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

fn embedded() -> EmbeddedLens {
    EmbeddedLens {
        warp: Some(EmbeddedWarp {
            planes: [[1.0, -0.02, 0.004, 0.0, 0.0, 0.0], [1.0, -0.021, 0.004, 0.0, 0.0, 0.0], [1.0, -0.022, 0.004, 0.0, 0.0, 0.0]],
            center: Point::new(0.5, 0.5),
            radius: 0.6,
        }),
        vignette: Some(EmbeddedVignette { k: [0.3, 0.05, 0.0, 0.0, 0.0], center: Point::new(0.5, 0.5), radius: 0.6 }),
    }
}

/// Settings as written before the toolset upgrades: every existing tool family.
fn cases() -> Vec<(&'static str, Value)> {
    vec![
        ("default", json!({})),
        (
            "raw defaults + edits",
            json!({
                "wb": {"mode": "custom", "temp": 4800.0, "tint": 6.0},
                "light": {"exposure": 0.6, "contrast": 20.0, "highlights": -50.0, "shadows": 40.0, "whites": 8.0, "blacks": -10.0},
                "curve": {"lights": 15.0, "master": [{"x": 0.0, "y": 0.02}, {"x": 0.5, "y": 0.55}, {"x": 1.0, "y": 1.0}], "red": [{"x": 0.0, "y": 0.0}, {"x": 0.5, "y": 0.48}, {"x": 1.0, "y": 1.0}]},
                "color": {"vibrance": 25.0, "saturation": 5.0},
                "mixer": {"orange": {"hue": -5.0, "sat": 10.0, "lum": 8.0}},
                "point_colors": [{"lum": 0.6, "chroma": 0.12, "hue": 40.0, "hue_shift": 10.0, "sat_shift": -20.0}],
                "grading": {"midtones": {"hue": 30.0, "sat": 12.0, "lum": 0.0}, "global": {"hue": 200.0, "sat": 5.0, "lum": 0.0}},
                "effects": {"texture": 20.0, "clarity": 15.0, "dehaze": 12.0},
                "vignette": {"amount": -20.0, "style": "colorPriority"},
                "grain": {"amount": 15.0, "seed": 9},
                "detail": {"sharpen_amount": 40.0, "nr_luminance": 15.0, "nr_color": 25.0},
                "calibration": {"shadows_tint": 5.0, "red_hue": 10.0, "blue_sat": 15.0}
            }),
        ),
        (
            "optics + geometry + crop",
            json!({
                "optics": {"remove_ca": true, "lens_profile": true, "distortion": 12.0, "vignetting": 20.0, "ca_red": 10.0, "defringe_purple_amount": 5.0},
                "geometry": {"vertical": 8.0, "rotate": 1.5, "scale": 95.0},
                "crop": {"geometry": {"rect": {"x0": 0.05, "y0": 0.04, "x1": 0.93, "y1": 0.97}, "angle": 2.0}}
            }),
        ),
        ("profile look", json!({"profile": {"id": "lc.film.warm-print", "amount": 120.0}, "light": {"exposure": 0.3}})),
        ("b&w profile", json!({"profile": {"id": "lc.bw.red-filter", "amount": 100.0}})),
        ("negative", json!({"negative": {"enabled": true, "film": "color"}})),
        (
            "layers",
            json!({
                "masks": [{"id": 1, "name": "L", "visible": true, "invert": false, "opacity": 70.0,
                    "components": [{"op": "add", "invert": false, "shape": {"kind": "radial", "center": {"x": 0.4, "y": 0.5}, "rx": 0.3, "ry": 0.25, "angle": 0.0, "feather": 50.0, "invert": false}}],
                    "adjust": {"exposure": 0.3},
                    "tools": {"light": {"contrast": 30.0, "shadows": 20.0}, "grading": {"shadows": {"hue": 210.0, "sat": 25.0, "lum": 0.0}}}}]
            }),
        ),
    ]
}

/// (case/source kind, render hash, settings hash), x86_64 Linux.
const GOLDEN: [(&str, u64, u64); 14] = [
    ("default/rendered", 0x11aba23997cf23ea, 0x12a719f1bc9181a3),
    ("default/raw", 0xc2082c862dbb6ee9, 0x12a719f1bc9181a3),
    ("raw defaults + edits/rendered", 0x1ff221ea991f326a, 0x4de6ce922ddbca2c),
    ("raw defaults + edits/raw", 0xbc9c429bb9ac900a, 0x4de6ce922ddbca2c),
    ("optics + geometry + crop/rendered", 0x5626a087deaf3da3, 0xc35ab3adf97809b0),
    ("optics + geometry + crop/raw", 0x40a3e4fc12596d81, 0xc35ab3adf97809b0),
    ("profile look/rendered", 0xdef219e6f08ead62, 0x2e1a0b037741d7c4),
    ("profile look/raw", 0x5718b047fd492719, 0x2e1a0b037741d7c4),
    ("b&w profile/rendered", 0xb0647da755d9d959, 0xe03778c72eb700fb),
    ("b&w profile/raw", 0xbfe5b0881505b6b4, 0xe03778c72eb700fb),
    ("negative/rendered", 0xb71acfaa8b5e6ed7, 0x6c1203e7c3fdbcb3),
    ("negative/raw", 0xb71acfaa8b5e6ed7, 0x6c1203e7c3fdbcb3),
    ("layers/rendered", 0xceba5c62420067ad, 0x47486480d432991f),
    ("layers/raw", 0x686f18d0cb9cae25, 0x47486480d432991f),
];

#[test]
fn settings_from_before_the_toolset_render_bit_identically() {
    let src = scene();
    let mut got = Vec::new();
    for (name, v) in cases() {
        let s = DevelopSettings::from_json(&v).expect("settings parse");
        assert_eq!(DevelopSettings::from_json(&s.to_json()).expect("roundtrip"), s, "{name}");
        let raw = SourceInfo { raw: true, as_shot_temp: 5200.0, as_shot_tint: 4.0, lens: Some(embedded()), ..Default::default() };
        for (kind, info) in [("rendered", SourceInfo::default()), ("raw", raw)] {
            let img = render(&src, &info, &s, &RenderRequest::fit(W, H)).image;
            got.push((format!("{name}/{kind}"), fnv(&img), s.hash64()));
        }
    }
    if std::env::var_os("LC_GOLDEN_PRINT").is_some() {
        for (n, h, sh) in &got {
            println!("    (\"{n}\", 0x{h:016x}, 0x{sh:016x}),");
        }
    }
    if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        for ((name, h, sh), (gname, g, gs)) in got.iter().zip(GOLDEN) {
            assert_eq!(name, gname);
            assert_eq!((*h, *sh), (g, gs), "{name}: render or settings hash changed");
        }
    }
}
