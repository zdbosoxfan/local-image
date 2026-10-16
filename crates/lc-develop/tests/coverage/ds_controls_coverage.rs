use lightcraft_develop::{
    DevelopSettings,
    controls::{CONTROLS, ControlSpec, INDEXED, Section, Track, find, get, in_section, indexed_instances, set},
};

#[test]
fn every_regular_control_has_valid_spec_and_find_works() {
    let mut seen_ids = std::collections::HashSet::new();
    for spec in CONTROLS {
        assert!(!spec.id.is_empty(), "control id must not be empty");
        assert!(!spec.label.is_empty(), "control label must not be empty for {}", spec.id);
        assert!(spec.min <= spec.default && spec.default <= spec.max, "{} default out of range", spec.id);
        assert!(spec.step > 0.0, "{} step must be positive", spec.id);
        assert!(spec.decimals <= 4, "{} decimals unreasonable", spec.id);
        assert!(seen_ids.insert(spec.id), "duplicate control id {}", spec.id);
        let found = find(spec.id).expect("find() should return the spec");
        assert_eq!(found.id, spec.id);
        assert_eq!(found.label, spec.label);
        assert_eq!(found.section, spec.section);
        assert_eq!(found.min, spec.min);
        assert_eq!(found.max, spec.max);
        assert_eq!(found.default, spec.default);
        assert_eq!(found.step, spec.step);
        assert_eq!(found.decimals, spec.decimals);
        // Track is not PartialEq, so we cannot compare directly; but we can check id matching.
    }
}

#[test]
fn set_clamps_to_min_max_and_resets_non_finite_to_default() {
    let mut s = DevelopSettings::default();
    for spec in CONTROLS {
        // too high
        assert!(set(&mut s, spec.id, spec.max + 1000.0), "set() should work for {}", spec.id);
        assert_eq!(get(&s, spec.id), Some(spec.max), "{} should clamp to max", spec.id);
        // too low
        assert!(set(&mut s, spec.id, spec.min - 1000.0));
        assert_eq!(get(&s, spec.id), Some(spec.min), "{} should clamp to min", spec.id);
        // NaN -> default
        assert!(set(&mut s, spec.id, f64::NAN));
        assert_eq!(get(&s, spec.id), Some(spec.default), "{} should fall back to default on NaN", spec.id);
        // +inf -> default
        assert!(set(&mut s, spec.id, f64::INFINITY));
        assert_eq!(get(&s, spec.id), Some(spec.default), "{} should fall back to default on +inf", spec.id);
        // -inf -> default
        assert!(set(&mut s, spec.id, f64::NEG_INFINITY));
        assert_eq!(get(&s, spec.id), Some(spec.default), "{} should fall back to default on -inf", spec.id);
    }
}

#[test]
fn unknown_and_malformed_ids_are_rejected() {
    let mut s = DevelopSettings::default();
    // unknown id
    assert!(!set(&mut s, "does.not.exist", 1.0));
    assert_eq!(get(&s, "does.not.exist"), None);
    assert!(find("does.not.exist").is_none());
    // empty id
    assert!(!set(&mut s, "", 1.0));
    assert_eq!(get(&s, ""), None);
    // incomplete parts
    assert!(!set(&mut s, "light.", 1.0));
    assert_eq!(get(&s, "light."), None);
    assert!(!set(&mut s, ".exposure", 1.0));
    assert_eq!(get(&s, ".exposure"), None);
    // malformed indexed id (non-numeric index)
    assert!(!set(&mut s, "pointColor.x.hueShift", 1.0));
    assert_eq!(get(&s, "pointColor.x.hueShift"), None);
    // indexed id with no sample yet
    assert!(!set(&mut s, "pointColor.0.hueShift", 1.0));
    assert_eq!(get(&s, "pointColor.0.hueShift"), None);
    assert!(!set(&mut s, "redEye.0.pupilSize", 1.0));
    assert_eq!(get(&s, "redEye.0.pupilSize"), None);
    // indexed id with valid family but unknown field
    assert!(find("pointColor.0.nope").is_none());
}

#[test]
fn indexed_controls_require_existing_samples_and_clamp() {
    let mut s = DevelopSettings::default();
    s.point_colors.push(Default::default());
    s.point_colors.push(Default::default());
    s.red_eye.push(Default::default());

    // set works and clamps using template spec
    assert!(set(&mut s, "pointColor.0.hueShift", 250.0));
    assert_eq!(get(&s, "pointColor.0.hueShift"), Some(100.0));
    assert!(set(&mut s, "pointColor.0.hueShift", -150.0));
    assert_eq!(get(&s, "pointColor.0.hueShift"), Some(-100.0));
    assert!(set(&mut s, "pointColor.1.satShift", 5.0));
    assert_eq!(get(&s, "pointColor.1.satShift"), Some(5.0));
    // red eye control
    assert!(set(&mut s, "redEye.0.pupilSize", 200.0));
    assert_eq!(get(&s, "redEye.0.pupilSize"), Some(100.0));
    assert!(set(&mut s, "redEye.0.darken", -10.0));
    assert_eq!(get(&s, "redEye.0.darken"), Some(0.0));
    // out-of-bounds index returns false
    assert!(!set(&mut s, "pointColor.2.hueShift", 1.0));
    assert_eq!(get(&s, "pointColor.2.hueShift"), None);
    assert!(!set(&mut s, "redEye.1.pupilSize", 1.0));
    assert_eq!(get(&s, "redEye.1.pupilSize"), None);
}

#[test]
fn find_returns_template_for_indexed_ids() {
    assert_eq!(find("pointColor.0.hueShift").map(|c| c.id), Some("pointColor.hueShift"));
    assert_eq!(find("pointColor.5.lumRange").map(|c| c.id), Some("pointColor.lumRange"));
    assert_eq!(find("redEye.0.pupilSize").map(|c| c.id), Some("redEye.pupilSize"));
    assert_eq!(find("redEye.9.darken").map(|c| c.id), Some("redEye.darken"));
    assert!(find("pointColor.hueShift").is_none()); // missing index part
    assert!(find("pointColor.x.hueShift").is_none());
}

#[test]
fn indexed_instances_reflects_current_sample_counts() {
    let mut s = DevelopSettings::default();
    // no samples -> empty
    assert!(indexed_instances(&s).is_empty());

    s.point_colors.push(Default::default());
    s.point_colors.push(Default::default());
    let pc_template_count = INDEXED.iter().filter(|c| c.id.starts_with("pointColor.")).count();
    let re_template_count = INDEXED.iter().filter(|c| c.id.starts_with("redEye.")).count();

    let instances = indexed_instances(&s);
    assert_eq!(instances.len(), 2 * pc_template_count); // only point colors, no red eye
    for (id, spec) in instances {
        assert!(id.starts_with("pointColor."));
        assert!(spec.id.starts_with("pointColor."));
    }

    s.red_eye.push(Default::default());
    let instances = indexed_instances(&s);
    assert_eq!(instances.len(), 2 * pc_template_count + re_template_count);
    let mut red_eye_count = 0;
    for (id, spec) in instances {
        if id.starts_with("redEye.") {
            red_eye_count += 1;
            assert!(spec.id.starts_with("redEye."));
        }
    }
    assert_eq!(red_eye_count, re_template_count);
}

#[test]
fn in_section_returns_only_specified_section_controls() {
    // Sections that have controls defined in CONTROLS (non-indexed)
    let regular_sections = [
        Section::Light,
        Section::Curve,
        Section::Color,
        Section::Mixer,
        Section::BwMix,
        Section::Grading,
        Section::Effects,
        Section::Vignette,
        Section::Grain,
        Section::Detail,
        Section::Optics,
        Section::Geometry,
        Section::Profile,
        Section::Calibration,
        Section::Negative,
        Section::Raw,
        Section::LensDb,
        Section::ToneEq,
        Section::ColorCal,
        Section::SkinTone,
    ];

    for section in regular_sections {
        let controls: Vec<&ControlSpec> = in_section(section).collect();
        assert!(!controls.is_empty(), "section {:?} should have controls", section);
        for c in &controls {
            assert_eq!(c.section, section, "control {} in wrong section", c.id);
        }
        let expected_count = CONTROLS.iter().filter(|c| c.section == section).count();
        assert_eq!(controls.len(), expected_count, "section {:?} count mismatch", section);
    }

    // Indexed-only sections are not in CONTROLS and in_section should return empty
    assert_eq!(in_section(Section::PointColor).count(), 0);
    assert_eq!(in_section(Section::RedEye).count(), 0);
}

#[test]
fn section_labels_are_non_empty_and_consistent() {
    let expected = [
        (Section::Light, "Light"),
        (Section::Curve, "Tone Curve"),
        (Section::Color, "Color"),
        (Section::Mixer, "Color Mixer"),
        (Section::BwMix, "B&W Mixer"),
        (Section::Grading, "Color Grading"),
        (Section::Effects, "Effects"),
        (Section::Vignette, "Vignette"),
        (Section::Grain, "Grain"),
        (Section::Detail, "Detail"),
        (Section::Optics, "Optics"),
        (Section::Geometry, "Geometry"),
        (Section::Profile, "Profile"),
        (Section::Calibration, "Calibration"),
        (Section::PointColor, "Point Color"),
        (Section::RedEye, "Red Eye"),
        (Section::Negative, "Negative"),
        (Section::Raw, "Raw Processing"),
        (Section::LensDb, "Lens Profile"),
        (Section::ToneEq, "Tone Equalizer"),
        (Section::ColorCal, "Color Calibration"),
        (Section::SkinTone, "Skin Tone"),
    ];
    for (section, label) in expected {
        assert_eq!(section.label(), label, "section {:?} label mismatch", section);
    }
}

#[test]
fn control_spec_clamp_handles_non_finite() {
    let spec = find("light.exposure").unwrap();
    assert_eq!(spec.clamp(f64::NAN), spec.default);
    assert_eq!(spec.clamp(f64::INFINITY), spec.default);
    assert_eq!(spec.clamp(f64::NEG_INFINITY), spec.default);
    assert_eq!(spec.clamp(spec.min - 1.0), spec.min);
    assert_eq!(spec.clamp(spec.max + 1.0), spec.max);
    assert_eq!(spec.clamp((spec.min + spec.max) / 2.0), (spec.min + spec.max) / 2.0);
}

#[test]
fn format_adds_plus_for_centered_controls_only_when_positive() {
    // exposure: min -5, max 5, decimals 2 -> centered
    let spec = find("light.exposure").unwrap();
    assert_eq!(spec.format(0.5), "+0.50");
    assert_eq!(spec.format(-1.0), "-1.00");
    assert_eq!(spec.format(0.0), "0.00"); // zero does not get plus
    assert_eq!(spec.format(2.345), "+2.35"); // rounds

    // wb.temp: min 2000, max 50000, decimals 0 -> not centered, no plus
    let temp_spec = find("wb.temp").unwrap();
    assert_eq!(temp_spec.format(5500.0), "5500");
    assert_eq!(temp_spec.format(5500.6), "5501"); // rounds

    // grain.amount: min 0, max 100, decimals 0 -> not centered, no plus
    let grain_spec = find("grain.amount").unwrap();
    assert_eq!(grain_spec.format(10.0), "10");
    // 10.4 rounds down to 10
    assert_eq!(grain_spec.format(10.4), "10");
    // 10.6 rounds up to 11
    assert_eq!(grain_spec.format(10.6), "11");
}

#[test]
fn get_returns_current_value_after_set_and_default_before() {
    let mut s = DevelopSettings::default();
    // default read
    for c in CONTROLS.iter().take(10) {
        assert_eq!(get(&s, c.id), Some(c.default), "default value for {}", c.id);
    }
    // set a few and read back exactly
    assert!(set(&mut s, "light.exposure", 1.25));
    assert_eq!(get(&s, "light.exposure"), Some(1.25));
    assert!(set(&mut s, "wb.tint", -12.0));
    assert_eq!(get(&s, "wb.tint"), Some(-12.0));
    assert!(set(&mut s, "detail.sharpenRadius", 1.5));
    assert_eq!(get(&s, "detail.sharpenRadius"), Some(1.5));
    // fractional values are preserved exactly as f64
    assert!(set(&mut s, "toneEq.ev3", 0.123));
    assert_eq!(get(&s, "toneEq.ev3"), Some(0.123));
}

#[test]
fn setting_to_default_keeps_settings_equal_to_default() {
    let mut s1 = DevelopSettings::default();
    let s2 = DevelopSettings::default();
    for c in CONTROLS.iter() {
        assert!(set(&mut s1, c.id, c.default));
    }
    // Since all controls are set to their defaults, the entire settings struct should remain default.
    // But this is expensive; just compare a few fields or check that no changes occurred.
    // For safety, we can serialize both and compare.
    assert_eq!(
        serde_json::to_value(&s1).unwrap(),
        serde_json::to_value(&s2).unwrap(),
        "setting all controls to default should leave settings unchanged"
    );
}

#[test]
fn controls_json_schema_can_be_serialized() {
    let json = serde_json::to_value(CONTROLS).unwrap();
    assert!(json.is_array());
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), CONTROLS.len());
    // first control is profile.amount
    assert_eq!(arr[0]["id"], "profile.amount");
    assert!(arr[0]["min"].is_number());
    assert!(arr[0]["max"].is_number());
    assert!(arr[0]["default"].is_number());
    assert!(arr[0]["step"].is_number());
    assert!(arr[0]["decimals"].is_number());
    assert!(arr[0]["track"].is_object());
}

#[test]
fn track_variants_serialize_correctly() {
    let track = Track::Gradient { from: "#000000", to: "#ffffff" };
    let json = serde_json::to_value(track).unwrap();
    assert_eq!(json["kind"], "gradient");
    assert_eq!(json["from"], "#000000");
    assert_eq!(json["to"], "#ffffff");

    let hue_track = Track::Hue { band: 3 };
    let json = serde_json::to_value(hue_track).unwrap();
    assert_eq!(json["kind"], "hue");
    assert_eq!(json["band"], 3);
}

#[test]
fn control_spec_serialization_includes_all_fields() {
    let spec = find("light.exposure").unwrap();
    let json = serde_json::to_value(spec).unwrap();
    assert_eq!(json["id"], "light.exposure");
    assert_eq!(json["label"], "Exposure");
    assert_eq!(json["section"], "light"); // due to serde rename_all camelCase
    assert_eq!(json["min"], -5.0);
    assert_eq!(json["max"], 5.0);
    assert_eq!(json["default"], 0.0);
    assert_eq!(json["step"], 0.01);
    assert_eq!(json["decimals"], 2);
    // track is serialized as object
    assert!(json["track"].is_object());
}
