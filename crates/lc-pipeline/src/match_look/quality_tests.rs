use super::*;

#[test]
fn forked_probe_caches_are_bit_identical_and_invalidate_changed_stage_keys() {
    let source = Arc::new(Rgb32f::from_fn(80, 64, |x, y| {
        let value = 0.01 + x as f32 / 110.0 + y as f32 / 900.0;
        [value, value * 0.86, value * 0.74]
    }));
    let info = SourceInfo { raw: true, ..Default::default() };
    let mut request = RenderRequest::fit(64, 64);
    request.depth = crate::OutputDepth::F32Linear;
    for layered in [false, true] {
        let mut base = DevelopSettings::default();
        base.wb.mode = WbMode::Custom;
        base.light.highlights = -18.0;
        base.light.shadows = 9.0;
        base.raw.capture.enabled = true;
        base.raw.capture.radius = 0.7;
        base.raw.capture.iterations = 1.0;
        if layered {
            let mut first = Mask {
                id: 3,
                components: vec![MaskComponent { shape: MaskShape::Full, op: Default::default(), invert: false, name: None }],
                ..Default::default()
            };
            first.tools.light = Some(lightcraft_develop::Light { exposure: 0.15, ..Default::default() });
            let mut second = first.clone();
            second.id = 4;
            second.tools.light = None;
            let mut curve = ToneCurve::default();
            set_channel(&mut curve, 2, 0.5, 0.53);
            second.tools.curve = Some(curve);
            base.masks = vec![first, second];
        }
        let cache = StageCache::default();
        let warm = crate::render_cached(&source, &info, &base, &request, &cache);
        let counters = cache.stack_renders();
        let extension = cache.extension::<std::sync::Mutex<u32>>();
        *extension.lock().unwrap() = 17;
        let mut probes = Vec::new();
        let mut wb = base.clone();
        wb.wb.temp = 5100.0;
        probes.push(wb);
        let mut exposure = base.clone();
        exposure.light.exposure = 0.4;
        probes.push(exposure);
        let mut master = base.clone();
        set_curve_point(&mut master.curve.master, 0.25, 0.38);
        probes.push(master);
        let mut channel = base.clone();
        set_channel(&mut channel.curve, 0, 0.5, 0.54);
        probes.push(channel);
        if layered {
            let mut prefix = base.clone();
            prefix.masks[0].tools.color = Some(lightcraft_develop::ColorAdj { saturation: 23.0, vibrance: 0.0 });
            probes.push(prefix);
        }
        for probe in probes {
            let fork = cache.fork_cpu();
            assert_eq!(fork.stack_renders(), (0, 0));
            assert!(fork.peek_extension::<std::sync::Mutex<u32>>().is_none());
            assert_eq!(fork.len(), cache.len());
            assert!(Arc::ptr_eq(&fork.lock()[0].sampled, &cache.lock()[0].sampled));
            assert!(Arc::ptr_eq(&fork.pre.lock().unwrap().as_ref().unwrap().2, &cache.pre.lock().unwrap().as_ref().unwrap().2));
            let reused = crate::render_cached(&source, &info, &probe, &request, &fork);
            let fresh = crate::render_cached(&source, &info, &probe, &request, &StageCache::default());
            assert_eq!(reused.image, fresh.image);
            assert_eq!(reused.deep, fresh.deep);
            assert_eq!(cache.stack_renders(), counters);
            fork.clear();
            assert_eq!(fork.len(), 0);
            assert!(!cache.is_empty());
            assert!(cache.pre.lock().unwrap().is_some());
        }
        let fork = cache.fork_cpu();
        let changed = Arc::new(source.map(|p| p.map(|v| v * 0.75)));
        let reused = crate::render_cached(&changed, &info, &base, &request, &fork);
        let fresh = crate::render_cached(&changed, &info, &base, &request, &StageCache::default());
        assert_eq!(reused.deep, fresh.deep, "changed source identity must invalidate reused pre/geometry");
        assert_ne!(reused.deep, warm.deep);
        let smaller = RenderRequest { max_w: 40, max_h: 40, ..request.clone() };
        let resized = crate::render_cached(&source, &info, &base, &smaller, &fork);
        let fresh = crate::render_cached(&source, &info, &base, &smaller, &StageCache::default());
        assert_eq!(resized.deep, fresh.deep, "output dimensions must invalidate geometry");
        assert_eq!(*extension.lock().unwrap(), 17);
    }
}

#[test]
fn editable_luma_master_represents_hdr_lift_without_a_twofold_slope_cap() {
    let mut before = DevelopSettings::default();
    before.curve.master = vec![Point::new(0.0, 0.0), Point::new(0.4, 0.42), Point::new(1.0, 1.0)];
    let mut full = before.clone();
    set_curve_point(&mut full.curve.master, 0.1, 0.3);
    set_curve_point(&mut full.curve.master, 0.25, 0.4);
    assert!(full.curve.master.contains(&Point::new(0.4, 0.42)), "existing user vertex must survive insertion");
    assert!((curve_value(&full.curve.master, 0.1) - 0.3).abs() < 1e-12);
    let opts = Options { normalize: false, color_adjust: false, color_tone: false, ..Default::default() };
    for strength in [0.0, 40.0, 65.0, 100.0, 125.0] {
        let faded = amount_settings(&before, &full, Options { strength, ..opts });
        let values = (0..=1024).map(|i| curve_value(&faded.curve.master, i as f64 / 1024.0)).collect::<Vec<_>>();
        assert!(values.windows(2).all(|v| v[1] > v[0]));
        assert!(values.iter().all(|v| (0.0..=1.0).contains(v)));
        if strength == 0.0 {
            assert_eq!(faded.curve, before.curve);
        }
        if strength == 100.0 {
            assert_eq!(faded.curve, full.curve);
        }
    }
    assert_eq!(reset_group(&before, &full, opts, "light").unwrap().curve, before.curve);
    let excluded = amount_settings(&before, &full, Options { adjustments: Adjustments { luma_curve: false, ..Default::default() }, ..opts });
    assert_eq!(excluded.curve, before.curve);
}

#[test]
fn same_capture_known_hdr_master_is_recovered_as_editable_luma_points() {
    let source = Arc::new(Rgb32f::from_fn(160, 96, |x, y| {
        let v = 0.001 + (x as f32 / 159.0).powi(2) * 0.8;
        if y < 48 { [v; 3] } else { [v * 0.96, v, v * 0.92] }
    }));
    let info = SourceInfo::default();
    let base = DevelopSettings::default();
    let mut desired = base.clone();
    desired.curve.master =
        [(0.0, 0.0), (0.1, 0.3), (0.25, 0.45), (0.5, 0.65), (0.8, 0.78), (0.95, 0.82), (1.0, 1.0)].map(|(x, y)| Point::new(x, y)).to_vec();
    let refs = [analyze(&source, &info, &desired)];
    for new_layer in [false, true] {
        let opts = Options {
            normalize: false,
            color_adjust: false,
            color_tone: false,
            new_layer,
            adjustments: Adjustments { contrast: false, highlights: false, shadows: false, whites: false, blacks: false, ..Default::default() },
            ..Default::default()
        };
        let solved = solve(&source, &info, &base, &refs, opts, &|| false).unwrap();
        let curve = if new_layer { solved.settings.masks.last().unwrap().tools.curve.as_ref().unwrap() } else { &solved.settings.curve };
        assert!(!curve.master.is_empty());
        assert!(
            solved.after_error < solved.before_error * 0.35,
            "master recovery {} -> {}, layer={new_layer}",
            solved.before_error,
            solved.after_error
        );
        let zero = amount_settings(&base, &solved.full_settings, Options { strength: 0.0, ..opts });
        if new_layer {
            assert!(ToneCurve::point_curve_is_identity(&zero.masks.last().unwrap().tools.curve.as_ref().unwrap().master));
        } else {
            assert_eq!(zero.curve, base.curve);
        }
        assert_eq!(amount_settings(&base, &solved.full_settings, opts), solved.settings);
        let disabled =
            solve(&source, &info, &base, &refs, Options { adjustments: Adjustments { luma_curve: false, ..opts.adjustments }, ..opts }, &|| false)
                .unwrap();
        assert_eq!(disabled.settings, base);
    }
}

#[test]
fn palette_prior_covers_all_fitted_axes_and_is_relative_to_existing_controls() {
    let mut base = DevelopSettings::default();
    base.mixer.yellow.sat = 17.0;
    assert_eq!(palette_prior(&base, &base), 0.0);
    for axis in ["sat", "hue", "lum"] {
        let mut changed = base.clone();
        let id = format!("mixer.yellow.{axis}");
        controls::set(&mut changed, &id, controls::get(&base, &id).unwrap() + palette_scale(axis));
        assert!((palette_prior(&base, &changed) - PALETTE_PRIOR).abs() < 1e-12);
    }
}

#[test]
fn palette_bound_probe_scales_its_actual_displacement() {
    let mut current = DevelopSettings::default();
    current.mixer.blue.sat = 50.0;
    let mut proposed = [0.0; 8];
    proposed[5] = 40.0;
    let probe = palette_candidate(&current, &proposed, "sat", 1.0);
    assert_eq!(probe.mixer.blue.sat, 55.0);
    let mut actual = [0.0; 8];
    actual[5] = probe.mixer.blue.sat - current.mixer.blue.sat;
    let half = palette_candidate(&current, &actual, "sat", 0.5);
    assert_eq!(half.mixer.blue.sat, 52.5);
}

#[test]
fn managed_template_absolute_wb_converts_to_relative_and_resets_without_promoting_local_bw() {
    let mut base = DevelopSettings::default();
    let wb = lightcraft_develop::WhiteBalance { mode: WbMode::Custom, temp: 7200.0, tint: 14.0 };
    let mut mask = Mask {
        id: 8,
        components: vec![MaskComponent { shape: MaskShape::Full, op: Default::default(), invert: false, name: None }],
        ..Default::default()
    };
    mask.tools.wb = Some(wb);
    mask.tools.treatment = Some(Treatment::Bw);
    mask.adjust.temp = 9.0;
    mask.adjust.tint = -3.0;
    base.masks.push(mask.clone());
    let opts = Options { new_layer: true, layer_placement: Some(LayerPlacement { id: 8, index: 0 }), ..Default::default() };
    let effective = relative_wb(&base, opts, (6500.0, 0.0));
    assert_eq!(effective, (7200.0, 14.0));
    let mut edits = relative_start(&base, opts);
    edits.wb = wb;
    edits.treatment = base.treatment;
    let converted = layer_settings(&base, &edits, opts, (6500.0, 0.0));
    assert_eq!(converted.treatment, Treatment::Color);
    assert_eq!(converted.masks[0].tools.treatment, Some(Treatment::Bw));
    assert!(converted.masks[0].tools.wb.is_none());
    assert!((6500.0 * (converted.masks[0].adjust.temp * 0.6 / 100.0).exp() - 7200.0).abs() < 1e-8);
    assert!((converted.masks[0].adjust.tint * 0.4 - 14.0).abs() < 1e-10);
    let reset = reset_group(&base, &converted, opts, "normalize").unwrap();
    assert_eq!(reset.masks[0].tools.wb, Some(wb));
    assert_eq!((reset.masks[0].adjust.temp, reset.masks[0].adjust.tint), (mask.adjust.temp, mask.adjust.tint));
    let excluded = layer_settings(&base, &DevelopSettings::default(), Options { keep_white_balance: true, ..opts }, (6500.0, 0.0));
    assert_eq!(excluded.masks[0].tools.wb, Some(wb));
    assert_eq!((excluded.masks[0].adjust.temp, excluded.masks[0].adjust.tint), (mask.adjust.temp, mask.adjust.tint));
    let source = Arc::new(Rgb32f::from_fn(48, 32, |x, y| {
        let value = 0.015 + x as f32 / 80.0;
        if y < 16 { [value, value * 0.7, value * 0.9] } else { [value * 0.8, value, value * 0.6] }
    }));
    let info = SourceInfo::default();
    let reference = analyze(&source, &info, &DevelopSettings::default());
    let solved = solve(&source, &info, &base, &[reference], opts, &|| false).unwrap();
    assert_eq!(solved.full_settings.treatment, Treatment::Color, "legacy local B&W must not become a global edit");
    assert_eq!(solved.full_settings.masks[0].tools.treatment, Some(Treatment::Bw));
}

#[test]
fn managed_template_preserves_mask_and_unselected_tools_through_fit_amount_reset() {
    let mut before = DevelopSettings::default();
    let mut template = Mask {
        id: 12,
        name: "Owner's masked look".into(),
        opacity: 37.0,
        invert: true,
        refine: 18.0,
        components: vec![MaskComponent { shape: MaskShape::Full, op: Default::default(), invert: true, name: Some("Manual coverage".into()) }],
        ..Default::default()
    };
    template.adjust.amount = 63.0;
    template.adjust.temp = 9.0;
    template.adjust.tint = -7.0;
    template.adjust.clarity = 13.0;
    template.tools.effects = Some(lightcraft_develop::Effects { clarity: 27.0, dehaze: 9.0, ..Default::default() });
    template.tools.detail = Some(lightcraft_develop::Detail::default());
    template.tools.wb = Some(lightcraft_develop::WhiteBalance { temp: 7200.0, tint: 6.0, ..Default::default() });
    template.tools.light = Some(lightcraft_develop::Light { contrast: 22.0, ..Default::default() });
    let mut curve = ToneCurve::default();
    curve.red = vec![Point::new(0.0, 0.0), Point::new(0.4, 0.35), Point::new(1.0, 1.0)];
    template.tools.curve = Some(curve.clone());
    let mut grading = lightcraft_develop::ColorGrading::default();
    grading.highlights.hue = 31.0;
    grading.highlights.sat = 23.0;
    template.tools.grading = Some(grading);
    before.masks = vec![template.clone(), Mask { id: 19, name: "Later".into(), ..Default::default() }];
    let opts = Options {
        new_layer: true,
        keep_white_balance: true,
        layer_placement: Some(LayerPlacement { id: 12, index: 0 }),
        adjustments: Adjustments { contrast: false, red_curve: false, highlights_balance: false, ..Default::default() },
        ..Default::default()
    };
    let mut edit = DevelopSettings::default();
    edit.light.exposure = 1.0;
    edit.light.contrast = -44.0;
    edit.light.shadows = 32.0;
    edit.curve.darks = 8.0;
    set_channel(&mut edit.curve, 2, 0.5, 0.54);
    edit.color.saturation = 18.0;
    edit.grading.shadows.sat = 12.0;
    let full = layer_settings(&before, &edit, opts, (6500.0, 0.0));
    let check = |settings: &DevelopSettings| {
        let mask = &settings.masks[0];
        assert_eq!(settings.masks.len(), 2);
        assert_eq!(settings.masks[1], before.masks[1]);
        assert_eq!(
            (&mask.name, &mask.components, mask.opacity, mask.invert, mask.refine),
            (&template.name, &template.components, template.opacity, template.invert, template.refine)
        );
        assert_eq!(mask.adjust, template.adjust);
        assert_eq!(mask.tools.effects, template.tools.effects);
        assert_eq!(mask.tools.detail, template.tools.detail);
        assert_eq!(mask.tools.wb, template.tools.wb);
        assert_eq!(mask.tools.light.unwrap().contrast, 22.0);
        assert_eq!(mask.tools.curve.as_ref().unwrap().red, curve.red);
        assert_eq!(mask.tools.grading.unwrap().highlights, grading.highlights);
    };
    check(&full);
    assert_eq!(full.masks[0].tools.light.unwrap().exposure, 1.0);
    for strength in [0.0, 40.0, 100.0, 125.0] {
        check(&amount_settings(&before, &full, Options { strength, ..opts }));
    }
    for group in ["normalize", "light", "color", "tone"] {
        check(&reset_group(&before, &full, opts, group).unwrap());
    }
}
