use photocraft_engine::web_cmds::{PRESETS, WebFormat, WebMetadata, WebSettings, assets_dir, optimize, parse_asset_name, parse_defaults, preset_settings};
use photocraft_engine::{EngineError, Session};
use photocraft_geom::Rect;
use serde_json::json;

fn px(r: u8, g: u8, b: u8, a: u8) -> [f32; 4] {
    [f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, f32::from(a) / 255.0]
}

#[test]
fn web_format_from_id_aliases_and_ext() {
    assert_eq!(WebFormat::from_id("gif"), Some(WebFormat::Gif));
    assert_eq!(WebFormat::from_id("GIF"), Some(WebFormat::Gif));
    assert_eq!(WebFormat::from_id("png8"), Some(WebFormat::Png8));
    assert_eq!(WebFormat::from_id("png-8"), Some(WebFormat::Png8));
    assert_eq!(WebFormat::from_id("png24"), Some(WebFormat::Png24));
    assert_eq!(WebFormat::from_id("png-24"), Some(WebFormat::Png24));
    assert_eq!(WebFormat::from_id("png"), Some(WebFormat::Png24));
    assert_eq!(WebFormat::from_id("png32"), Some(WebFormat::Png24));
    assert_eq!(WebFormat::from_id("jpeg"), Some(WebFormat::Jpeg));
    assert_eq!(WebFormat::from_id("jpg"), Some(WebFormat::Jpeg));
    assert_eq!(WebFormat::from_id("wbmp"), Some(WebFormat::Wbmp));
    assert_eq!(WebFormat::from_id("bmp"), None);

    assert_eq!(WebFormat::Gif.ext(), "gif");
    assert_eq!(WebFormat::Png8.ext(), "png");
    assert_eq!(WebFormat::Png24.ext(), "png");
    assert_eq!(WebFormat::Jpeg.ext(), "jpg");
    assert_eq!(WebFormat::Wbmp.ext(), "wbmp");
}

#[test]
fn web_format_indexed_property() {
    assert!(WebFormat::Gif.indexed());
    assert!(WebFormat::Png8.indexed());
    assert!(!WebFormat::Png24.indexed());
    assert!(!WebFormat::Jpeg.indexed());
    assert!(!WebFormat::Wbmp.indexed());
}

#[test]
fn web_settings_from_params_clamps_and_errors() {
    let p = json!({
        "format": "jpeg",
        "colors": 999,
        "quality": 150,
        "ditherAmount": 200,
        "webSnap": -10,
        "matte": "#336699",
        "metadata": "all",
        "transparency": false
    });
    let st = WebSettings::from_params(&p, "test").unwrap();
    assert_eq!(st.format, WebFormat::Jpeg);
    assert_eq!(st.colors, 256);
    assert_eq!(st.quality, 100);
    assert_eq!(st.dither_amount, 1.0);
    assert_eq!(st.web_snap, 0.0);
    assert_eq!(st.matte, Some([51.0 / 255.0, 102.0 / 255.0, 153.0 / 255.0]));
    assert_eq!(st.metadata, WebMetadata::All);
    assert!(!st.transparency);

    let err = WebSettings::from_params(&json!({"format": "tiff"}), "c").unwrap_err();
    assert!(matches!(err, EngineError::BadParams { cmd, .. } if cmd == "c"));
}

#[test]
fn web_settings_to_params_round_trip() {
    let original = WebSettings {
        format: WebFormat::Gif,
        colors: 32,
        dither_amount: 0.5,
        matte: None,
        quality: 42,
        metadata: WebMetadata::CopyrightAndContact,
        ..WebSettings::default()
    };
    let p = original.to_params();
    let decoded = WebSettings::from_params(&p, "roundtrip").unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn preset_settings_match_names() {
    assert_eq!(PRESETS.len(), 12);
    assert!(PRESETS.iter().all(|p| preset_settings(p).is_some()));

    let gif = preset_settings("GIF 32 No Dither").unwrap();
    assert_eq!(gif.format, WebFormat::Gif);
    assert_eq!(gif.colors, 32);

    let jpeg_low = preset_settings("JPEG Low").unwrap();
    assert_eq!(jpeg_low.format, WebFormat::Jpeg);
    assert_eq!(jpeg_low.quality, 10);

    assert!(preset_settings("Not a real preset").is_none());
}

#[test]
fn optimize_empty_rect_returns_err() {
    let st = WebSettings::default();
    let buf = vec![[0.0; 4]];
    let err = optimize(&buf, 1, Rect::new(0, 0, 0, 1), &st, None, None, 72.0, false).unwrap_err();
    assert!(matches!(err, EngineError::Other(_)));
}

#[test]
fn optimize_png24_opaque_small() {
    let st = WebSettings { format: WebFormat::Png24, transparency: true, ..WebSettings::default() };
    let buf = vec![px(255, 0, 0, 255), px(0, 255, 0, 255), px(0, 0, 255, 255), px(255, 255, 255, 255)];
    let o = optimize(&buf, 2, Rect::new(0, 0, 2, 2), &st, None, None, 72.0, true).unwrap();
    assert_eq!(o.width, 2);
    assert_eq!(o.height, 2);
    assert_eq!(o.ext, "png");
    assert_eq!(o.colors, None);
    assert!(!o.bytes.is_empty());
    assert_eq!(o.preview.len(), 16);
}

#[test]
fn optimize_png24_transparent_preview_keeps_alpha() {
    let st = WebSettings { format: WebFormat::Png24, transparency: true, ..WebSettings::default() };
    let buf = vec![[1.0, 0.0, 0.0, 0.5]];
    let o = optimize(&buf, 1, Rect::new(0, 0, 1, 1), &st, None, None, 72.0, true).unwrap();
    assert_eq!(o.preview.len(), 4);
    assert_eq!(o.preview, vec![255, 0, 0, 128]);
}

#[test]
fn optimize_jpeg_output_and_preview() {
    let st = WebSettings { format: WebFormat::Jpeg, quality: 50, ..WebSettings::default() };
    let buf = vec![px(10, 20, 30, 255), px(200, 100, 50, 255)];
    let o = optimize(&buf, 2, Rect::new(0, 0, 2, 1), &st, None, None, 72.0, true).unwrap();
    assert_eq!(o.width, 2);
    assert_eq!(o.height, 1);
    assert_eq!(o.ext, "jpg");
    assert_eq!(o.colors, None);
    assert!(!o.bytes.is_empty());
    assert_eq!(o.preview.len(), 8);
}

#[test]
fn optimize_gif_indexed_palette_and_transparency() {
    let st = WebSettings { format: WebFormat::Gif, colors: 8, transparency: true, matte: None, ..WebSettings::default() };
    let buf = vec![
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 0.0], // fully transparent
        [0.0, 0.0, 1.0, 1.0],
        [1.0, 1.0, 0.0, 1.0],
    ];
    let o = optimize(&buf, 2, Rect::new(0, 0, 2, 2), &st, None, None, 72.0, true).unwrap();
    assert_eq!(o.width, 2);
    assert_eq!(o.height, 2);
    assert_eq!(o.ext, "gif");
    let colors = o.colors.expect("palette size");
    assert!(colors <= 8);
    assert!(!o.bytes.is_empty());
    assert_eq!(o.preview.len(), 16);
    // The second pixel (index 4..7) is the transparent one; its alpha must be zero.
    assert_eq!(o.preview[7], 0);
}

#[test]
fn optimize_wbmp_two_colors_and_preview() {
    let st = WebSettings { format: WebFormat::Wbmp, ..WebSettings::default() };
    let buf = vec![[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]];
    let o = optimize(&buf, 2, Rect::new(0, 0, 2, 1), &st, None, None, 72.0, true).unwrap();
    assert_eq!(o.width, 2);
    assert_eq!(o.height, 1);
    assert_eq!(o.ext, "wbmp");
    assert_eq!(o.colors, Some(2));
    assert_eq!(o.preview.len(), 8);
    assert_eq!(&o.preview[..4], &[0, 0, 0, 255]);
    assert_eq!(&o.preview[4..], &[255, 255, 255, 255]);
}

#[test]
fn optimize_png24_deterministic() {
    let st = WebSettings { format: WebFormat::Png24, ..WebSettings::default() };
    let buf: Vec<[f32; 4]> = (0..64)
        .map(|i| {
            let v = i as f32 / 63.0;
            [v, 1.0 - v, 0.5, 1.0]
        })
        .collect();
    let rect = Rect::new(0, 0, 8, 8);
    let a = optimize(&buf, 8, rect, &st, None, None, 72.0, false).unwrap();
    let b = optimize(&buf, 8, rect, &st, None, None, 72.0, false).unwrap();
    assert_eq!(a.bytes, b.bytes);
}

#[test]
fn optimize_with_nan_inf_does_not_panic() {
    let st = WebSettings::default();
    let buf = vec![[f32::NAN, f32::NEG_INFINITY, f32::INFINITY, f32::NAN], [0.5, 0.5, 0.5, 0.5]];
    let o = optimize(&buf, 2, Rect::new(0, 0, 2, 1), &st, None, None, 72.0, false);
    assert!(o.is_ok());
}

#[test]
fn optimize_odd_sizes_all_formats_ok() {
    let buf: Vec<[f32; 4]> = (0..3 * 5)
        .map(|i| {
            let x = i as f32 / 14.0;
            [x, 1.0 - x, (x * 255.0).round() as u8 as f32 / 255.0, 0.3]
        })
        .collect();
    for fmt in [WebFormat::Gif, WebFormat::Png8, WebFormat::Png24, WebFormat::Jpeg, WebFormat::Wbmp] {
        let st = WebSettings { format: fmt, colors: 8, ..WebSettings::default() };
        let o = optimize(&buf, 3, Rect::new(0, 0, 3, 5), &st, None, None, 72.0, false).unwrap();
        assert_eq!(o.width, 3);
        assert_eq!(o.height, 5);
    }
}

#[test]
fn parse_asset_name_basic_png_and_jpeg_quality() {
    let v = parse_asset_name("foo.png", 72.0);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].file, "foo.png");
    assert_eq!(v[0].format, "png32");
    assert_eq!(v[0].quality, None);

    let v = parse_asset_name("photo.jpg80%", 72.0);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].file, "photo.jpg");
    assert_eq!(v[0].format, "jpg");
    assert_eq!(v[0].quality, Some(80));
}

#[test]
fn parse_asset_name_multiple_and_size() {
    let v = parse_asset_name("48x48 icons/a.png8, ?x100 thumb.gif", 72.0);
    assert_eq!(v.len(), 2);

    assert_eq!(v[0].width, Some(48.0));
    assert_eq!(v[0].height, Some(48.0));
    assert_eq!(v[0].file, "icons/a.png");
    assert_eq!(v[0].format, "png8");

    assert_eq!(v[1].width, None);
    assert_eq!(v[1].height, Some(100.0));
    assert_eq!(v[1].file, "thumb.gif");
    assert_eq!(v[1].format, "gif");
}

#[test]
fn parse_asset_name_invalid_names() {
    assert_eq!(parse_asset_name("foo.txt", 72.0).len(), 0);
    assert_eq!(parse_asset_name("foo", 72.0).len(), 0);
    assert_eq!(parse_asset_name("", 72.0).len(), 0);
    assert_eq!(parse_asset_name("   ", 72.0).len(), 0);
}

#[test]
fn parse_defaults_variants() {
    let d = parse_defaults("default 50% low/ + 200% @2x", 72.0).unwrap();
    assert_eq!(d.len(), 2);

    assert_eq!(d[0].scale, Some(0.5));
    assert_eq!(d[0].folder, "low/");
    assert_eq!(d[0].suffix, "");

    assert_eq!(d[1].scale, Some(2.0));
    assert_eq!(d[1].folder, "");
    assert_eq!(d[1].suffix, "@2x");
}

#[test]
fn assets_dir_appends_assets() {
    let dir = assets_dir("/tmp/foo/bar.psd");
    assert!(dir.ends_with("bar-assets"));
}

#[test]
fn save_for_web_without_doc_returns_error() {
    let mut s = Session::new();
    let err = s.execute("file.export.saveForWebLegacy", json!({})).unwrap_err();
    assert!(matches!(err, EngineError::Disabled(_, _) | EngineError::NoDocument), "expected disabled/no document error, got {err:?}");
}
