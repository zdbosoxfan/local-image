use photocraft_engine::web_cmds::{PRESETS, WebFormat, WebMetadata, WebSettings, assets_dir, optimize, preset_settings};
use photocraft_engine::{EngineError, Session};
use photocraft_geom::Rect;
use serde_json::json;

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
fn optimize_png24_transparent_preview_keeps_alpha() {
    let st = WebSettings { format: WebFormat::Png24, transparency: true, ..WebSettings::default() };
    let buf = vec![[1.0, 0.0, 0.0, 0.5]];
    let o = optimize(&buf, 1, Rect::new(0, 0, 1, 1), &st, None, None, 72.0, true).unwrap();
    assert_eq!(o.preview.len(), 4);
    assert_eq!(o.preview, vec![255, 0, 0, 128]);
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
