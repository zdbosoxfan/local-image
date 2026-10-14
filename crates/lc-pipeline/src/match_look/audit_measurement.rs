//! Existing measurements with only zero-weight appearance axes omitted for audits.
//! Public reference/saved-Look analysis must continue to use the full measurement.
use super::{Look, Options, Rgba8};

pub(super) fn measure(image: &Rgba8, opts: Options) -> Look {
    if opts.appearance_aware { super::measure(image) } else { super::descriptor::measure(image) }
}

#[cfg(test)]
mod tests {
    use super::super::{Adjustments, LEGACY_FEATURES, blend, distance, reachable_goal};
    use super::*;
    fn bits(look: &Look, appearance: bool) -> Vec<u64> {
        let end = if appearance { look.features.len() } else { LEGACY_FEATURES };
        look.features[..end]
            .iter()
            .chain(&look.support[..end])
            .chain(&look.signature)
            .chain(&look.hue_distribution)
            .chain(&look.band_mass)
            .chain(&look.content)
            .chain(std::iter::once(&look.importance))
            .copied()
            .chain(look.clusters.iter().flat_map(|c| [c.ab[0], c.ab[1], c.weight]))
            .map(f64::to_bits)
            .collect()
    }
    fn image(width: usize, height: usize, distribution: usize) -> Rgba8 {
        Rgba8::from_fn(width, height, |x, y| {
            let mut state = 0x7b3d_63ac_11e2_b403u64 ^ ((x + y * width) as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            match distribution {
                0 => [state as u8, (state >> 16) as u8, (state >> 32) as u8, (state >> 48) as u8],
                1 => [128; 4],
                2 => {
                    let v = (x * 7 + y * 13) as u8;
                    [v, v, v, 255]
                }
                _ => [[255, 0, 0, 255], [0, 255, 0, 0], [0, 0, 255, 128], [0, 0, 0, 255], [255; 4]][(x + y) % 5],
            }
        })
    }
    #[test]
    fn audit_measurement_preserves_consumed_bits_goals_errors_and_ties_for_all_modes() {
        let reference = [super::super::measure(&image(64, 48, 0)), super::super::measure(&image(64, 48, 2))];
        for (width, height) in [(0, 0), (1, 1), (1, 97), (7, 9), (64, 48), (128, 96)] {
            for distribution in 0..4 {
                let before = image(width, height, distribution);
                let after = before.map(|p| [p[0].saturating_add(12), p[1].saturating_sub(7), p[2], p[3]]);
                let old = [super::super::measure(&before), super::super::measure(&after)];
                for appearance_aware in [false, true] {
                    let measured = [
                        super::super::measure_for_audit(&before, Options { appearance_aware, ..Default::default() }),
                        super::super::measure_for_audit(&after, Options { appearance_aware, ..Default::default() }),
                    ];
                    for (new, old) in measured.iter().zip(&old) {
                        assert_eq!(bits(new, appearance_aware), bits(old, appearance_aware));
                        assert_eq!((new.monochrome, new.content.len(), new.clusters.len()), (old.monochrome, old.content.len(), old.clusters.len()));
                    }
                    for count in [1, 20] {
                        let refs = (0..count).map(|i| reference[i % 2].clone()).collect::<Vec<_>>();
                        let weights = vec![1.0 / count as f64; count];
                        let reference_goal = blend(&refs, &weights);
                        for groups in 0..16 {
                            let opts = Options {
                                appearance_aware,
                                normalize: groups & 1 != 0,
                                light_contrast: groups & 2 != 0,
                                color_adjust: groups & 4 != 0,
                                color_tone: groups & 8 != 0,
                                consistent_batch: groups & 1 != 0,
                                region_aware: groups & 2 != 0,
                                protect_skin: groups & 4 != 0,
                                new_layer: groups & 8 != 0,
                                keep_exposure: groups == 7,
                                keep_white_balance: groups == 11,
                                adjustments: if groups == 0 {
                                    Adjustments { exposure: false, white_balance: false, ..Default::default() }
                                } else {
                                    Default::default()
                                },
                                strength: [0.0, 57.0, 100.0, 125.0][groups % 4],
                                ..Default::default()
                            };
                            for similarity in [0.0, 0.91, 1.0] {
                                let (old_goal, old_reach) = reachable_goal(&old[0], &reference_goal, opts, similarity);
                                let (new_goal, new_reach) = reachable_goal(&measured[0], &reference_goal, opts, similarity);
                                assert_eq!(bits(&new_goal, true), bits(&old_goal, true));
                                assert_eq!(new_reach.to_bits(), old_reach.to_bits());
                                let old_errors = [distance(&old[0], &old_goal, opts), distance(&old[1], &old_goal, opts)];
                                let new_errors = [distance(&measured[0], &new_goal, opts), distance(&measured[1], &new_goal, opts)];
                                assert_eq!(new_errors.map(f64::to_bits), old_errors.map(f64::to_bits));
                                assert_eq!(new_errors[1] >= new_errors[0], old_errors[1] >= old_errors[0]);
                                let tied = distance(&measured[0], &new_goal, opts);
                                assert_eq!(tied.to_bits(), old_errors[0].to_bits());
                                assert!(tied >= tied, "equal errors must retain the existing audit decision");
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    #[ignore = "audit measurement timing diagnostic; shared-host ML/native contention is uncontrolled"]
    fn options_aware_audit_measurement_timing() {
        use std::{hint::black_box, time::Instant};
        for (width, height) in [(512, 341), (1024, 683)] {
            let image = image(width, height, 0);
            let opts = Options::default();
            assert_eq!(bits(&super::super::measure(&image), false), bits(&measure(&image, opts), false));
            for aware in [false, true] {
                black_box(if aware { measure(&image, opts) } else { super::super::measure(&image) });
            }
            let mut timings: [Vec<f64>; 2] = Default::default();
            for iteration in 0..6 {
                for aware in [iteration % 2 == 0, iteration % 2 != 0] {
                    let start = Instant::now();
                    black_box(if aware { measure(&image, opts) } else { super::super::measure(&image) });
                    timings[usize::from(aware)].push(start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            for values in &mut timings {
                values.sort_by(f64::total_cmp);
            }
            let old = timings[0][3];
            let new = timings[1][3];
            eprintln!(
                "audit measurement {width}x{height}: publicfull={old:.3}ms options-aware-disabled={new:.3}ms ratio={:.3}x; optimized dev test; shared-host ML/native contention uncontrolled",
                old / new
            );
        }
    }
}
