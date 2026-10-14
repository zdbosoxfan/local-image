//! Reuse the main measurement's exact chroma values. Both reductions retain their
//! original iteration order and arithmetic; no feature or solver math is changed.

pub(super) fn moments(_labs: &[[f64; 3]], chromas: &[f64]) -> (f64, f64) {
    let mean = chromas.iter().copied().sum::<f64>() / chromas.len().max(1) as f64;
    let variation = (chromas.iter().map(|c| (*c - mean).powi(2)).sum::<f64>() / chromas.len().max(1) as f64).sqrt() / mean.max(1e-9);
    (mean, variation)
}

#[cfg(test)]
mod tests {
    use super::super::{Look, appearance, measure_samples_with_reductions, order_statistics};
    use super::*;

    // The previous formula is deliberately independent of the saved chromas.
    fn old_moments(labs: &[[f64; 3]], _: &[f64]) -> (f64, f64) {
        let mean = labs.iter().map(|p| p[1].hypot(p[2])).sum::<f64>() / labs.len().max(1) as f64;
        let variation = (labs.iter().map(|p| (p[1].hypot(p[2]) - mean).powi(2)).sum::<f64>() / labs.len().max(1) as f64).sqrt() / mean.max(1e-9);
        (mean, variation)
    }
    fn look_bits(look: &Look) -> Vec<u64> {
        look.features
            .iter()
            .chain(&look.support)
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
    fn labs(width: usize, height: usize) -> Vec<[f64; 3]> {
        let mut state = 0x435f_7a92_dd01_67beu64;
        (0..width * height)
            .map(|i| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                [
                    0.03 + (i % width.max(1)) as f64 / width.max(1) as f64 * 0.9,
                    (state % 1001) as f64 / 5000.0 - 0.1,
                    ((state >> 20) % 1001) as f64 / 5000.0 - 0.1,
                ]
            })
            .collect()
    }
    #[test]
    fn cached_chroma_preserves_old_moment_bits_for_edges_and_nonfinite_values() {
        let special = [
            0.0,
            -0.0,
            f64::from_bits(1),
            f64::from_bits(0x8000_0000_0000_0001),
            f64::MIN_POSITIVE,
            -f64::MIN_POSITIVE,
            0.1,
            -0.3,
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::from_bits(0x7ff8_0000_0000_0123),
            f64::from_bits(0xfff0_0000_0000_0001),
        ];
        for a in special {
            for b in special {
                let labs = [[0.2, a, b]; 7];
                let chromas = labs.iter().map(|p| p[1].hypot(p[2])).collect::<Vec<_>>();
                let old = old_moments(&labs, &[]);
                let new = moments(&labs, &chromas);
                assert_eq!([old.0.to_bits(), old.1.to_bits()], [new.0.to_bits(), new.1.to_bits()]);
            }
        }
        assert_eq!(old_moments(&[], &[]), moments(&[], &[]));
    }
    #[test]
    fn every_look_bit_matches_old_chroma_formula_for_faces_descriptors_and_appearance() {
        for (width, height) in [(0, 0), (1, 1), (1, 97), (13, 7), (64, 48), (256, 192)] {
            for distribution in 0..4 {
                let mut labs = labs(width, height);
                for (i, lab) in labs.iter_mut().enumerate() {
                    match distribution {
                        1 => *lab = [0.18, 0.0, -0.0],
                        2 => {
                            lab[1] = [0.0, -0.0, f64::from_bits(1), -f64::from_bits(1)][i % 4];
                            lab[2] = -lab[1];
                        }
                        3 => {
                            lab[1] = [0.05, f64::INFINITY, f64::NEG_INFINITY, f64::from_bits(0x7ff8_0000_0000_0123)][i % 4];
                        }
                        _ => {}
                    }
                }
                let membership = appearance::memberships_with_dimensions(&labs, width, height);
                let disabled = appearance::disabled();
                let face = [[0.1, 0.15, 0.6, 0.65]];
                for faces in [&[][..], &face[..]] {
                    for descriptor in [false, true] {
                        for membership in [None, Some(&membership), Some(&disabled)] {
                            let new = measure_samples_with_reductions::<true>(
                                width,
                                height,
                                labs.iter().copied(),
                                faces,
                                descriptor,
                                membership,
                                order_statistics::luminance,
                                moments,
                            );
                            let old = measure_samples_with_reductions::<false>(
                                width,
                                height,
                                labs.iter().copied(),
                                faces,
                                descriptor,
                                membership,
                                order_statistics::luminance,
                                old_moments,
                            );
                            assert_eq!(new.monochrome, old.monochrome);
                            assert_eq!((new.content.len(), new.clusters.len()), (old.content.len(), old.clusters.len()));
                            assert_eq!(
                                look_bits(&new),
                                look_bits(&old),
                                "{width}x{height}, distribution={distribution}, descriptor={descriptor}, faces={}",
                                faces.len()
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    #[ignore = "measurement timing diagnostic; run explicitly with --ignored --nocapture"]
    fn cached_chroma_measurement_timing() {
        use std::{hint::black_box, time::Instant};
        for (width, height) in [(60, 45), (256, 192), (512, 384)] {
            let labs = labs(width, height);
            let disabled = appearance::disabled();
            let measure = |cached: bool| {
                if cached {
                    measure_samples_with_reductions::<true>(
                        width,
                        height,
                        labs.iter().copied(),
                        &[],
                        false,
                        Some(&disabled),
                        order_statistics::luminance,
                        moments,
                    )
                } else {
                    measure_samples_with_reductions::<false>(
                        width,
                        height,
                        labs.iter().copied(),
                        &[],
                        false,
                        Some(&disabled),
                        order_statistics::luminance,
                        old_moments,
                    )
                }
            };
            assert_eq!(look_bits(&measure(true)), look_bits(&measure(false)));
            for cached in [false, true] {
                black_box(measure(cached));
            }
            let mut timings: [Vec<f64>; 2] = Default::default();
            for iteration in 0..12 {
                for cached in [iteration % 2 == 0, iteration % 2 != 0] {
                    let started = Instant::now();
                    black_box(measure(cached));
                    timings[usize::from(cached)].push(started.elapsed().as_secs_f64() * 1000.0);
                }
            }
            for samples in &mut timings {
                samples.sort_by(f64::total_cmp);
            }
            let old = timings[0][timings[0].len() / 2];
            let new = timings[1][timings[1].len() / 2];
            eprintln!(
                "chroma measurement {width}x{height}: old={old:.3}ms cached={new:.3}ms speedup={:.3}x extra_scratch={} bytes",
                old / new,
                width * height * std::mem::size_of::<f64>()
            );
        }
    }
}
