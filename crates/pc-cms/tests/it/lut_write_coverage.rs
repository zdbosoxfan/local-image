#![allow(clippy::unwrap_used, clippy::expect_used)]

use photocraft_cms::lutfile::*;

fn assert_data_close(a: &[f32], b: &[f32], eps: f32) {
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert!((x - y).abs() < eps, "mismatch: {x} vs {y}");
    }
}

fn floats_to_hex(data: &[f32]) -> String {
    let mut s = String::new();
    for v in data {
        let bytes = v.to_le_bytes();
        for b in bytes {
            s.push_str(&format!("{b:02X}"));
        }
    }
    s
}

#[test]
fn identity_matches_from_fn_identity_small_sizes() {
    for size in [2usize, 3, 5, 17] {
        let a = LutFile::identity(size);
        let b = LutFile::from_fn("", size, |c| c);
        assert_eq!(a.size, size);
        assert_eq!(a.data.len(), size * size * size * 3);
        assert_data_close(&a.data, &b.data, 1e-6);
    }
}

#[test]
fn identity_red_fastest_order_known_entries() {
    let l = LutFile::identity(2);
    assert_eq!(&l.data[0..3], &[0.0, 0.0, 0.0]);
    assert_eq!(&l.data[3..6], &[1.0, 0.0, 0.0]);
    assert_eq!(&l.data[6..9], &[0.0, 1.0, 0.0]);
    assert_eq!(&l.data[12..15], &[0.0, 0.0, 1.0]);
    assert_eq!(&l.data[21..24], &[1.0, 1.0, 1.0]);
}

#[test]
fn write_cube_roundtrip_identity() {
    let id = LutFile::identity(5);
    let text = write_cube(&LutFile { title: "Id".into(), ..id.clone() });
    let back = parse_cube(&text).unwrap();
    assert_eq!(back.size, 5);
    assert_eq!(back.title, "Id");
    assert_data_close(&back.data, &id.data, 1e-5);
}

#[test]
fn write_cube_escapes_quotes_in_title() {
    let l = LutFile { title: "a\"b".into(), size: 2, data: LutFile::identity(2).data };
    let text = write_cube(&l);
    assert!(text.contains("TITLE \"a'b\""));
    let back = parse_cube(&text).unwrap();
    assert_eq!(back.title, "a'b");
}

#[test]
fn parse_cube_rejects_empty_and_malformed() {
    assert!(parse_cube("").is_err());
    assert!(parse_cube("garbage").is_err());
    assert!(parse_cube("LUT_3D_SIZE 2\n0 0 0\n").is_err());
    assert!(parse_cube("LUT_3D_SIZE 99999\n").is_err());
    assert!(parse_cube("LUT_3D_SIZE 2\n0 0 0\n1 1 1\n2 2 2\n").is_err());
    assert!(parse_cube("LUT_1D_SIZE 2\n0 0 0\n").is_err());
}

#[test]
fn parse_cube_accepts_comments_and_unknown_keywords() {
    let text = "# comment\nLUT_IN_VIDEO_RANGE 0 1\nLUT_3D_SIZE 2\n# another\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n";
    let l = parse_cube(text).unwrap();
    assert_eq!(l.size, 2);
    assert_eq!(l.data.len(), 8 * 3);
}

#[test]
fn parse_cube_1d_expands_to_33() {
    let text = "LUT_1D_SIZE 2\n0 0 0\n0.5 1 1\n";
    let l = parse_cube(text).unwrap();
    assert_eq!(l.size, 33);
    assert_eq!(l.data.len(), 33 * 33 * 33 * 3);
    let last = &l.data[l.data.len() - 3..];
    assert!((last[0] - 0.5).abs() < 1e-6);
    assert!((last[1] - 1.0).abs() < 1e-6);
    assert!((last[2] - 1.0).abs() < 1e-6);
}

#[test]
fn parse_cube_1d_interpolates_linearly() {
    let text = "LUT_1D_SIZE 3\n0 0 0\n0.5 0.5 0.5\n1 1 1\n";
    let l = parse_cube(text).unwrap();
    // Node coordinate 0.5 corresponds to index 16 in a 33-grid.
    let idx = ((16 * 33 + 16) * 33 + 16) * 3;
    let expected = [0.5, 0.5, 0.5];
    assert_data_close(&l.data[idx..idx + 3], &expected, 1e-6);
}

#[test]
fn parse_cube_ignores_domain_min_max() {
    let base = "LUT_3D_SIZE 2\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n";
    let with_domain = format!("DOMAIN_MIN 0.1 0.2 0.3\nDOMAIN_MAX 0.8 0.9 1.0\n{base}");
    let a = parse_cube(base).unwrap();
    let b = parse_cube(&with_domain).unwrap();
    assert_eq!(a.data, b.data);
}

#[test]
fn parse_cube_rejects_1d_size_overflow() {
    let text = format!("LUT_1D_SIZE {}\n", usize::MAX);
    assert!(parse_cube(&text).is_err());
}

#[test]
fn parse_3dl_rejects_non_cube() {
    assert!(parse_3dl("1 2 3\n4 5 6\n").is_err());
}

#[test]
fn parse_3dl_blue_varying_fastest_size_3() {
    let mut text = String::new();
    for r in 0..3 {
        for g in 0..3 {
            for b in 0..3 {
                let fr = r as f64 / 2.0;
                let fg = g as f64 / 2.0;
                let fb = b as f64 / 2.0;
                text.push_str(&format!("{fr} {fg} {fb}\n"));
            }
        }
    }
    let l = parse_3dl(&text).unwrap();
    assert_eq!(l.size, 3);
    assert_data_close(&l.data, &LutFile::identity(3).data, 1e-6);
}

#[test]
fn parse_3dl_infers_bit_depth() {
    for scale in [1023.0, 4095.0, 65535.0] {
        let mut text = String::new();
        for r in 0..2 {
            for g in 0..2 {
                for b in 0..2 {
                    text.push_str(&format!("{} {} {}\n", r as f64 * scale, g as f64 * scale, b as f64 * scale));
                }
            }
        }
        let l = parse_3dl(&text).unwrap();
        assert_data_close(&l.data, &LutFile::identity(2).data, 1e-6);
    }
}

#[test]
fn parse_3dl_accepts_shaper_line() {
    let mut text = String::from("0 1 2 3\n");
    for r in 0..2 {
        for g in 0..2 {
            for b in 0..2 {
                text.push_str(&format!("{r} {g} {b}\n"));
            }
        }
    }
    let l = parse_3dl(&text).unwrap();
    assert_eq!(l.size, 2);
    assert_data_close(&l.data, &LutFile::identity(2).data, 1e-6);
}

#[test]
fn parse_look_roundtrip_identity() {
    let id = LutFile::identity(2);
    let hex = floats_to_hex(&id.data);
    let xml = format!("<?xml version=\"1.0\"?><look><LUT><size>\"2\"</size><data>\"{hex}\"</data></LUT></look>");
    let parsed = parse_look(&xml).unwrap();
    assert_eq!(parsed.size, 2);
    assert_data_close(&parsed.data, &id.data, 1e-6);
}

#[test]
fn parse_look_rejects_missing_size() {
    assert!(parse_look("<look></look>").is_err());
}

#[test]
fn parse_look_rejects_oversized_size() {
    let text = format!("<look><size>{}</size><data>00</data></look>", usize::MAX);
    assert!(parse_look(&text).is_err());
}

#[test]
fn parse_look_strips_rgba_alpha() {
    let id = LutFile::identity(2);
    let mut rgba = Vec::with_capacity(id.data.len() / 3 * 4);
    for triple in id.data.as_chunks::<3>().0 {
        rgba.extend_from_slice(triple);
        rgba.push(0.5);
    }
    let hex = floats_to_hex(&rgba);
    let xml = format!("<look><size>2</size><data>{hex}</data></look>");
    let parsed = parse_look(&xml).unwrap();
    assert_eq!(parsed.size, 2);
    assert_data_close(&parsed.data, &id.data, 1e-6);
}

#[test]
fn parse_dispatch_by_extension_uses_cube_for_unknown() {
    let cube = "LUT_3D_SIZE 2\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n";
    let by_cube = parse("a.cube", cube.as_bytes()).unwrap();
    let by_txt = parse("a.txt", cube.as_bytes()).unwrap();
    assert_eq!(by_cube, by_txt);
}

#[test]
fn parse_dispatch_sniffs_look_leading_less_than() {
    let id = LutFile::identity(2);
    let hex = floats_to_hex(&id.data);
    let xml = format!("<look><size>2</size><data>{hex}</data></look>");
    let parsed = parse("renamed.dat", xml.as_bytes()).unwrap();
    assert_eq!(parsed.size, 2);
    assert_data_close(&parsed.data, &id.data, 1e-6);
}

#[test]
fn builtin_luts_exist_and_in_range() {
    for (id, _) in BUILTIN {
        let l = builtin(id).unwrap();
        assert_eq!(l.data.len(), 33 * 33 * 33 * 3);
        assert!(l.data.iter().all(|v| (0.0..=1.0).contains(v)), "{id}");
    }
}

#[test]
fn builtin_unknown_returns_none() {
    assert!(builtin("definitely-not-a-builtin").is_none());
}

#[test]
fn from_fn_clamps_outputs() {
    let l = LutFile::from_fn("", 2, |_| [2.0, -1.0, 0.5]);
    assert_eq!(&l.data[0..3], &[1.0, 0.0, 0.5]);
}

#[test]
fn parse_cube_rejects_non_finite() {
    // A data line whose first token starts with a sign and parses to a non-finite
    // value must be rejected by `check()`.
    let base = "LUT_3D_SIZE 2\n";
    let mut rows = String::new();
    rows.push_str("+NaN 0 0\n");
    for _ in 0..7 {
        rows.push_str("0 0 0\n");
    }
    let text_nan = format!("{base}{rows}");
    assert!(parse_cube(&text_nan).is_err());

    let text_inf = text_nan.replace("+NaN", "-inf");
    assert!(parse_cube(&text_inf).is_err());
}

#[test]
fn write_cube_invalid_size_parse_rejects() {
    let l = LutFile { title: String::new(), size: 0, data: Vec::new() };
    let text = write_cube(&l);
    assert!(parse_cube(&text).is_err());
}

#[test]
fn write_cube_incomplete_data_parse_rejects() {
    let l = LutFile { title: String::new(), size: 2, data: vec![0.0; 6] };
    let text = write_cube(&l);
    assert!(parse_cube(&text).is_err());
}
