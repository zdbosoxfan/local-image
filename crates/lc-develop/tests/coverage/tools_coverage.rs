use lightcraft_develop::{Adaptation, CaptureSharpening, ColorCal, Demosaic, HighlightMode, Illuminant, LensDb, RawProcessing, ToneEq};
use serde_json::json;

#[test]
fn demosaic_is_dual_only_for_dual_variants() {
    assert!(Demosaic::DualRcd.is_dual());
    assert!(Demosaic::DualRcdVng.is_dual());
    assert!(Demosaic::DualAmazeVng.is_dual());
    assert!(Demosaic::DualMarkesteijnVng.is_dual());
    assert!(!Demosaic::Lmmse.is_dual());
    assert!(!Demosaic::Dcb.is_dual());
    assert!(!Demosaic::Markesteijn3.is_dual());
    assert!(!Demosaic::XtransDirectional.is_dual());
    assert!(!Demosaic::Auto.is_dual());
    assert!(!Demosaic::Ahd.is_dual());
    assert!(!Demosaic::Rcd.is_dual());
    assert!(!Demosaic::Vng4.is_dual());
    assert!(!Demosaic::Amaze.is_dual());
    assert!(!Demosaic::Ppg.is_dual());
    assert!(!Demosaic::Bilinear.is_dual());
}

#[test]
fn demosaic_serde_roundtrip_camel_case() {
    let mut all = Demosaic::ALL.to_vec();
    all.push(Demosaic::DualRcd);
    let expected_names = [
        "auto",
        "lmmse",
        "dcb",
        "markesteijn3",
        "dualMarkesteijnVng",
        "xtransDirectional",
        "ahd",
        "rcd",
        "dualRcdVng",
        "vng4",
        "amaze",
        "dualAmazeVng",
        "ppg",
        "bilinear",
        "dualRcd",
    ];
    for (i, v) in all.iter().enumerate() {
        let json = serde_json::to_string(v).unwrap();
        assert_eq!(json, format!("\"{}\"", expected_names[i]));
        let back: Demosaic = serde_json::from_str(&json).unwrap();
        assert_eq!(back, *v);
    }
}

#[test]
fn highlight_mode_all_and_labels() {
    assert_eq!(HighlightMode::ALL.len(), 4);
    assert_eq!(HighlightMode::Reconstruct.label(), "Reconstruct");
    assert_eq!(HighlightMode::Opposed.label(), "Inpaint Opposed");
    assert_eq!(HighlightMode::Segmentation.label(), "Segmentation");
    assert_eq!(HighlightMode::Clip.label(), "Clip");
    for v in HighlightMode::ALL.iter() {
        assert!(!v.label().is_empty());
    }
}

#[test]
fn highlight_mode_serde_roundtrip() {
    let expected_names = ["reconstruct", "opposed", "segmentation", "clip"];
    for (i, v) in HighlightMode::ALL.iter().enumerate() {
        let json = serde_json::to_string(v).unwrap();
        assert_eq!(json, format!("\"{}\"", expected_names[i]));
        let back: HighlightMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, *v);
    }
}

#[test]
fn capture_sharpening_default_json() {
    let d = CaptureSharpening::default();
    let v = serde_json::to_value(d).unwrap();
    assert_eq!(
        v,
        json!({
            "enabled": false,
            "radius": 0.0,
            "threshold": 0.0,
            "corner_boost": 0.0,
            "iterations": 8.0
        })
    );
}

#[test]
fn raw_processing_defaults_and_is_default() {
    let d = RawProcessing::default();
    assert!(d.is_default());
    let v = serde_json::to_value(d).unwrap();
    assert_eq!(
        v,
        json!({
            "demosaic": "auto",
            "dual_threshold": 20.0,
            "highlights": "reconstruct",
            "capture": {
                "enabled": false,
                "radius": 0.0,
                "threshold": 0.0,
                "corner_boost": 0.0,
                "iterations": 8.0
            }
        })
    );
}

#[test]
fn raw_processing_changes_decoding_only_for_demosaic_and_highlights() {
    let default = RawProcessing::default();
    assert!(!default.changes_decoding());

    let demosaic_changed: RawProcessing = serde_json::from_value(json!({
        "demosaic": "rcd",
        "dual_threshold": 20.0,
        "highlights": "reconstruct",
        "capture": {
            "enabled": false,
            "radius": 0.0,
            "threshold": 0.0,
            "corner_boost": 0.0,
            "iterations": 8.0
        }
    }))
    .unwrap();
    assert!(demosaic_changed.changes_decoding());

    let highlights_changed: RawProcessing = serde_json::from_value(json!({
        "demosaic": "auto",
        "dual_threshold": 20.0,
        "highlights": "clip",
        "capture": {
            "enabled": false,
            "radius": 0.0,
            "threshold": 0.0,
            "corner_boost": 0.0,
            "iterations": 8.0
        }
    }))
    .unwrap();
    assert!(highlights_changed.changes_decoding());

    let capture_changed: RawProcessing = serde_json::from_value(json!({
        "demosaic": "auto",
        "dual_threshold": 20.0,
        "highlights": "reconstruct",
        "capture": {
            "enabled": true,
            "radius": 1.0,
            "threshold": 50.0,
            "corner_boost": 10.0,
            "iterations": 12.0
        }
    }))
    .unwrap();
    assert!(!capture_changed.changes_decoding());
}

#[test]
fn lens_db_default_and_is_default() {
    let d = LensDb::default();
    assert!(d.is_default());
    let v = serde_json::to_value(&d).unwrap();
    // camera and lens are skipped because None
    assert_eq!(
        v,
        json!({
            "enabled": false,
            "distortion": 100.0,
            "tca": 100.0,
            "vignetting": 100.0
        })
    );
}

#[test]
fn tone_eq_default_and_zones() {
    let d = ToneEq::default();
    assert!(d.is_default());
    assert_eq!(d.zones(), [0.0; 9]);
    let v = serde_json::to_value(d).unwrap();
    assert_eq!(
        v,
        json!({
            "enabled": false,
            "ev8": 0.0, "ev7": 0.0, "ev6": 0.0, "ev5": 0.0, "ev4": 0.0,
            "ev3": 0.0, "ev2": 0.0, "ev1": 0.0, "ev0": 0.0,
            "smoothing": 0.0,
            "size": 5.0,
            "refine": 50.0,
            "mask_exposure": 0.0,
            "mask_contrast": 0.0
        })
    );
}

#[test]
fn tone_eq_zones_order_and_roundtrip() {
    let value = json!({
        "enabled": true,
        "ev8": -2.0, "ev7": -1.5, "ev6": -1.0, "ev5": -0.5, "ev4": 0.0,
        "ev3": 0.5, "ev2": 1.0, "ev1": 1.5, "ev0": 2.0,
        "smoothing": 1.0,
        "size": 25.0,
        "refine": 75.0,
        "mask_exposure": 0.5,
        "mask_contrast": -0.3
    });
    let parsed: ToneEq = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(parsed.zones(), [-2.0, -1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0]);
    let back = serde_json::to_value(parsed).unwrap();
    assert_eq!(back, value);
    assert!(!parsed.is_default());
}

#[test]
fn adaptation_all_and_labels() {
    assert_eq!(Adaptation::ALL.len(), 4);
    assert_eq!(Adaptation::Cat16.label(), "CAT16");
    assert_eq!(Adaptation::Bradford.label(), "Bradford (linear)");
    assert_eq!(Adaptation::FullBradford.label(), "Bradford (non-linear)");
    assert_eq!(Adaptation::Xyz.label(), "XYZ");
}

#[test]
fn adaptation_serde_roundtrip() {
    let expected_names = ["cat16", "bradford", "fullBradford", "xyz"];
    for (i, v) in Adaptation::ALL.iter().enumerate() {
        let json = serde_json::to_string(v).unwrap();
        assert_eq!(json, format!("\"{}\"", expected_names[i]));
        let back: Adaptation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, *v);
    }
}

#[test]
fn illuminant_all_and_labels() {
    assert_eq!(Illuminant::ALL.len(), 10);
    assert_eq!(Illuminant::WhiteBalance.label(), "As White Balance");
    assert_eq!(Illuminant::A.label(), "A (incandescent)");
    assert_eq!(Illuminant::D50.label(), "D50");
    assert_eq!(Illuminant::Custom.label(), "Custom");
    for v in Illuminant::ALL.iter() {
        assert!(!v.label().is_empty());
    }
}

#[test]
fn illuminant_serde_roundtrip() {
    let expected_names = ["whiteBalance", "a", "d50", "d55", "d65", "d75", "f2", "f7", "f11", "custom"];
    for (i, v) in Illuminant::ALL.iter().enumerate() {
        let json = serde_json::to_string(v).unwrap();
        assert_eq!(json, format!("\"{}\"", expected_names[i]));
        let back: Illuminant = serde_json::from_str(&json).unwrap();
        assert_eq!(back, *v);
    }
}

#[test]
fn color_cal_default_and_is_default() {
    let d = ColorCal::default();
    assert!(d.is_default());
    let v = serde_json::to_value(d).unwrap();
    assert_eq!(
        v,
        json!({
            "enabled": false,
            "adaptation": "cat16",
            "illuminant": "whiteBalance",
            "x": 0.3127,
            "y": 0.329,
            "gamut": 1.0,
            "clip": true
        })
    );
}

#[test]
fn malformed_json_does_not_panic() {
    assert!(serde_json::from_str::<Demosaic>("\"notAVariant\"").is_err());
    assert!(serde_json::from_str::<HighlightMode>("\"invalid\"").is_err());
    assert!(serde_json::from_str::<Adaptation>("\"notExists\"").is_err());
    assert!(serde_json::from_str::<Illuminant>("\"unknown\"").is_err());

    assert!(serde_json::from_str::<CaptureSharpening>("true").is_err());
    assert!(serde_json::from_str::<LensDb>("null").is_err());
    assert!(serde_json::from_str::<ToneEq>("{\"ev8\": \"not a number\"}").is_err());
    assert!(serde_json::from_str::<ColorCal>("{\"clip\": \"not bool\"}").is_err());
}

// Not a bug: serde's derived Deserialize also accepts a struct written as a sequence, and with `#[serde(default)]`
// an empty sequence yields the defaults. Settings are only ever written as objects, so this is harmless; a hand-written
// Deserialize to reject it would add risk to loading older edits for no benefit.
#[test]
fn raw_processing_from_empty_sequence_is_default() {
    assert_eq!(serde_json::from_str::<RawProcessing>("[]").unwrap(), RawProcessing::default());
}
