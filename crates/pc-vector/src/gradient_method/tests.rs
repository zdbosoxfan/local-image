use super::*;

#[test]
fn oklab_midpoints_and_straight_alpha_follow_the_reference() {
    let c = sample_stops(GradientMethod::Perceptual, &[(0.0, [0.0, 0.0, 0.0, 0.2]), (1.0, [1.0, 1.0, 1.0, 0.8])], &[0.25], &[(0.0, 1.0), (1.0, 0.0)], 0.25);
    // Halfway in OKLab between black and white is linear RGB 0.125, encoded as 0.38857286.
    for value in &c[..3] {
        assert!((*value - 0.38857286).abs() < 2e-6, "{c:?}");
    }
    assert!((c[3] - 0.375).abs() < 1e-6, "straight alpha times independent opacity: {c:?}");
    let red_blue = mix(GradientMethod::Perceptual, [1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0], 0.5);
    for (actual, expected) in red_blue.into_iter().zip([0.5504411, 0.3256207, 0.6365007, 1.0]) {
        assert!((actual - expected).abs() < 2e-6);
    }
}

#[test]
fn bisected_classic_ramps_preserve_scan_results_and_hard_stop_ties() {
    let stops = [(0.2, [0.1; 4]), (0.5, [0.8; 4]), (0.5, [0.3; 4]), (0.8, [0.6; 4])];
    let mids = [0.2, 0.5, 0.7];
    for t in [f32::NEG_INFINITY, -1.0, 0.2, 0.3, 0.5, 0.5_f32.next_up(), 0.7, 0.8, 1.0, f32::INFINITY, f32::NAN] {
        let mut expected = stops[stops.len() - 1].1;
        if t <= stops[0].0 {
            expected = stops[0].1;
        } else {
            for (i, w) in stops.windows(2).enumerate() {
                if t <= w[1].0 {
                    let u = if w[1].0 > w[0].0 { (t - w[0].0) / (w[1].0 - w[0].0) } else { 0.0 };
                    let k = midpoint_remap(u, mids[i]);
                    expected = std::array::from_fn(|ch| w[0].1[ch] + (w[1].1[ch] - w[0].1[ch]) * k);
                    break;
                }
            }
        }
        assert_eq!(sample_color(GradientMethod::Classic, &stops, &mids, t), expected, "t={t}");
        let opacity: Vec<_> = stops.iter().map(|(p, c)| (*p, c[0])).collect();
        assert_eq!(sample_opacity(&opacity, t), sample_color(GradientMethod::Classic, &stops, &[], t)[0]);
    }
    assert_eq!(sample_color(GradientMethod::Perceptual, &[], &[], 0.5), [0.0; 4]);
    assert_eq!(sample_color(GradientMethod::Perceptual, &stops[..1], &[], 0.9), stops[0].1);
    assert_eq!(sample_opacity(&[], 0.5), 1.0);
}

#[test]
fn adaptive_samples_preserve_breakpoints_and_bound_encoded_error() {
    let stops = [(0.0, [0.0, 0.0, 1.0, 0.2]), (1.0, [1.0, 1.0, 0.0, 0.9])];
    let at = |t, _| sample_stops(GradientMethod::Perceptual, &stops, &[0.3], &[(0.0, 0.9), (0.5, 0.2), (1.0, 0.7)], t);
    let samples = refine_samples(vec![(0.0, 0), (0.3, 0), (0.5, 0), (1.0, 0)], at).unwrap();
    assert!(samples.len() > 4 && samples.len() <= MAX_EXPANDED_STOPS);
    assert!(samples.contains(&(0.3, 0)) && samples.contains(&(0.5, 0)));
    for pair in samples.windows(2) {
        let (a, b) = (at(pair[0].0, 0), at(pair[1].0, 0));
        for i in 0..=16 {
            let u = i as f32 / 16.0;
            let actual = at(pair[0].0 + (pair[1].0 - pair[0].0) * u, 0);
            for ch in 0..4 {
                assert!((actual[ch] - (a[ch] + (b[ch] - a[ch]) * u)).abs() <= TOLERANCE, "{}..{} ch={ch}", pair[0].0, pair[1].0);
            }
        }
    }
    let hard = vec![(0.0, 0), (0.5, -1), (0.5, 1), (1.0, 0)];
    assert_eq!(refine_samples(hard.clone(), |t, side| [if t > 0.5 || t == 0.5 && side > 0 { 1.0 } else { 0.0 }; 4]).unwrap(), hard);
}

#[test]
fn expansion_refuses_invalid_or_unrepresentable_ramps() {
    assert!(refine_samples(vec![(0.0, 0); MAX_EXPANDED_STOPS + 1], |_, _| [0.0; 4]).is_err());
    assert!(refine_samples(vec![(0.0, 0), (f32::NAN, 0)], |_, _| [0.0; 4]).is_err());
    assert!(refine_samples(vec![(0.0, 0), (1.0, 0)], |_, _| [f32::NAN; 4]).is_err());
    assert!(refine_samples(vec![(0.0, 0), (1.0, 0)], |t, _| [if t < 0.3333333 { 0.0 } else { 1.0 }; 4]).is_err());
}
