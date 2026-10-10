use lightcraft_pipeline::visualize::{MaskView, Overlay, spot_threshold};

#[test]
fn overlay_none_parts_roundtrip() {
    let o = Overlay::None;
    let (kind, value) = o.to_parts();
    assert_eq!((kind, value), (0, 0.0));
    assert_eq!(Overlay::from_parts(kind, value), o);
}

#[test]
fn overlay_point_color_range_parts_roundtrip() {
    for i in [0u8, 1, 127, 255] {
        let o = Overlay::PointColorRange(i);
        let (k, v) = o.to_parts();
        assert_eq!((k, v), (1, i as f64));
        assert_eq!(Overlay::from_parts(k, v), o);
    }
}

#[test]
fn overlay_point_color_range_from_parts_saturates_cast() {
    assert_eq!(Overlay::from_parts(1, 300.0), Overlay::PointColorRange(255));
    assert_eq!(Overlay::from_parts(1, -1.0), Overlay::PointColorRange(0));
    assert_eq!(Overlay::from_parts(1, f64::NAN), Overlay::PointColorRange(0));
}

#[test]
fn overlay_spots_parts_roundtrip() {
    for t in [0u8, 50, 100] {
        let o = Overlay::Spots(t);
        let (k, v) = o.to_parts();
        assert_eq!((k, v), (2, t as f64));
        assert_eq!(Overlay::from_parts(k, v), o);
    }
}

#[test]
fn overlay_spots_from_parts_clamps_value() {
    assert_eq!(Overlay::from_parts(2, 150.0), Overlay::Spots(100));
    assert_eq!(Overlay::from_parts(2, -10.0), Overlay::Spots(0));
    assert_eq!(Overlay::from_parts(2, f64::NAN), Overlay::Spots(0));
}

#[test]
fn overlay_mask_parts_roundtrip_basic() {
    let o = Overlay::Mask { id: 7, view: MaskView::Color, color: [10, 20, 30], opacity: 80 };
    let (k, v) = o.to_parts();
    assert_eq!(k, 3);
    assert_eq!(Overlay::from_parts(k, v), o);
}

#[test]
fn overlay_mask_parts_roundtrip_extreme_values() {
    let o = Overlay::Mask { id: u16::MAX, view: MaskView::WhiteOnBlack, color: [255, 255, 255], opacity: 100 };
    let (k, v) = o.to_parts();
    assert_eq!(Overlay::from_parts(k, v), o);

    let o2 = Overlay::Mask { id: 0, view: MaskView::Color, color: [0, 0, 0], opacity: 0 };
    let (k2, v2) = o2.to_parts();
    assert_eq!(Overlay::from_parts(k2, v2), o2);
}

#[test]
fn overlay_mask_parts_opacity_is_clamped_on_pack() {
    let o = Overlay::Mask { id: 1, view: MaskView::Color, color: [0, 0, 0], opacity: 200 };
    let (_, v) = o.to_parts();
    let decoded = Overlay::from_parts(3, v);
    assert!(matches!(decoded, Overlay::Mask { opacity: 100, .. }));
}

#[test]
fn overlay_mask_from_parts_invalid_view_falls_back_to_color() {
    // View index occupies bits 16..20; set it to 5 (out of range).
    let base = 0u64;
    let v = base | (5u64 << 16);
    let decoded = Overlay::from_parts(3, v as f64);
    assert!(matches!(decoded, Overlay::Mask { view: MaskView::Color, .. }));
}

#[test]
fn overlay_mask_parts_non_finite_or_negative_returns_none() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert_eq!(Overlay::from_parts(3, bad), Overlay::None);
    }
}

#[test]
fn overlay_from_parts_unknown_kind_returns_none() {
    assert_eq!(Overlay::from_parts(0, 0.0), Overlay::None);
    assert_eq!(Overlay::from_parts(6, 42.0), Overlay::None);
    assert_eq!(Overlay::from_parts(255, 1.0), Overlay::None);
}

#[test]
fn overlay_key_distinct_and_none_zero() {
    assert_eq!(Overlay::None.key(), 0);
    let mut keys = Vec::new();
    for o in [
        Overlay::PointColorRange(0),
        Overlay::PointColorRange(1),
        Overlay::Spots(0),
        Overlay::Spots(1),
        Overlay::Mask { id: 0, view: MaskView::Color, color: [0, 0, 0], opacity: 0 },
        Overlay::Mask { id: 0, view: MaskView::Color, color: [0, 0, 1], opacity: 0 },
        Overlay::ToneEqMask,
        Overlay::SharpenMask,
    ] {
        let k = o.key();
        assert_ne!(k, 0, "non-none overlay should not have key 0");
        assert!(!keys.contains(&k), "duplicate key for {o:?}");
        keys.push(k);
    }
}

#[test]
fn mask_view_all_variants_are_unique_and_complete() {
    assert_eq!(MaskView::ALL.len(), 5);
    for (i, v) in MaskView::ALL.iter().enumerate() {
        for (j, w) in MaskView::ALL.iter().enumerate() {
            if i != j {
                assert_ne!(v.name(), w.name());
            }
        }
    }
    assert_eq!(MaskView::Color.name(), "color");
    assert_eq!(MaskView::ColorOnBw.name(), "colorOnBw");
    assert_eq!(MaskView::ImageOnBlack.name(), "imageOnBlack");
    assert_eq!(MaskView::ImageOnWhite.name(), "imageOnWhite");
    assert_eq!(MaskView::WhiteOnBlack.name(), "whiteOnBlack");
}

#[test]
fn mask_view_name_parse_roundtrip() {
    for view in MaskView::ALL {
        assert_eq!(MaskView::parse(view.name()), Some(view));
    }
}

#[test]
fn mask_view_parse_invalid_returns_none() {
    assert_eq!(MaskView::parse(""), None);
    assert_eq!(MaskView::parse("Color"), None);
    assert_eq!(MaskView::parse("color "), None);
    assert_eq!(MaskView::parse("whiteonblack"), None);
}

#[test]
fn mask_view_next_cycles_through_all() {
    let order = [MaskView::Color, MaskView::ColorOnBw, MaskView::ImageOnBlack, MaskView::ImageOnWhite, MaskView::WhiteOnBlack];
    for i in 0..order.len() {
        let mut current = order[i];
        for step in 1..=order.len() {
            current = current.next();
            assert_eq!(current, order[(i + step) % order.len()]);
        }
    }
}

#[test]
fn mask_view_labels_are_nonempty_and_distinct() {
    let mut labels = Vec::new();
    for view in MaskView::ALL {
        let label = view.label();
        assert!(!label.is_empty());
        labels.push(label);
    }
    for i in 0..labels.len() {
        for j in (i + 1)..labels.len() {
            assert_ne!(labels[i], labels[j]);
        }
    }
}

#[test]
fn spot_threshold_monotonic_and_bounded() {
    let t0 = spot_threshold(0);
    let t50 = spot_threshold(50);
    let t100 = spot_threshold(100);
    assert!(t0 > t50);
    assert!(t50 > t100);

    // Exact values from the implementation:
    // t=0   -> 0.01 + 0.2 * 1.0^2 = 0.21
    // t=50  -> 0.01 + 0.2 * 0.5^2 = 0.06
    // t=100 -> 0.01 + 0.2 * 0.0^2 = 0.01
    // Use approximate equality to account for f32 rounding.
    let eps = 1e-6;
    assert!((t0 - 0.21_f32).abs() < eps, "t0 was {t0}, expected ~0.21");
    assert!((t50 - 0.06_f32).abs() < eps, "t50 was {t50}, expected ~0.06");
    assert!((t100 - 0.01_f32).abs() < eps, "t100 was {t100}, expected ~0.01");

    // Loose range sanity checks.
    assert!(t0 > 0.20 && t0 < 0.22);
    assert!(t50 > 0.05 && t50 < 0.07);
    assert!(t100 > 0.005 && t100 < 0.015);
}

#[test]
fn spot_threshold_clamps_above_100() {
    assert_eq!(spot_threshold(100), spot_threshold(150));
    assert_eq!(spot_threshold(100), spot_threshold(255));
    assert_eq!(spot_threshold(0), spot_threshold(0));
}
