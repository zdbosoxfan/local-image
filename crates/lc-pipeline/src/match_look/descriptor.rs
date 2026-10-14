//! Target identification uses only the existing signature and content fields.
//! Skip discarded appearance statistics; public reference analysis stays complete.
use super::{Look, Rgba8, appearance, measure_impl_with_membership};

pub(super) fn measure(image: &Rgba8) -> Look {
    measure_impl_with_membership(image, &[], true, Some(&appearance::disabled()))
}

#[cfg(test)]
mod tests {
    use super::super::{DevelopSettings, Mask, MaskComponent, MaskShape, Options, Rgb32f, Solved, SourceInfo, Techniques, analyze, solve_impl};
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn descriptor_bits(look: &Look) -> Vec<u64> {
        look.signature.iter().chain(&look.content).copied().map(f64::to_bits).collect()
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
    fn image(width: usize, height: usize, distribution: usize) -> Rgba8 {
        Rgba8::from_fn(width, height, |x, y| {
            let mut state = 0x7a51_1143_12a9_3bcdu64 ^ ((x + y * width) as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
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
    fn thin_descriptor_preserves_all_signature_and_content_bits_of_public_measure() {
        for (width, height) in [(0, 0), (1, 1), (1, 97), (7, 9), (13, 7), (64, 48), (128, 96)] {
            for distribution in 0..4 {
                let image = image(width, height, distribution);
                let old = super::super::measure(&image);
                let new = measure(&image);
                assert_eq!(new.content.len(), old.content.len());
                assert_eq!(descriptor_bits(&new), descriptor_bits(&old), "{width}x{height}, distribution={distribution}");
            }
        }
    }
    fn assert_solved_exact(new: &Solved, old: &Solved) {
        // Exhaustive destructuring forces new observable fields into this oracle.
        let Solved { monochrome, reach, fit, goal, settings, full_settings, before_error, after_error, weights, similarity, renders } = new;
        assert_eq!(*monochrome, old.monochrome);
        assert_eq!(*renders, old.renders, "logical render budget must remain identical");
        assert_eq!(
            [*reach, *fit, *before_error, *after_error, *similarity].map(f64::to_bits),
            [old.reach, old.fit, old.before_error, old.after_error, old.similarity].map(f64::to_bits)
        );
        assert_eq!(weights.iter().copied().map(f64::to_bits).collect::<Vec<_>>(), old.weights.iter().copied().map(f64::to_bits).collect::<Vec<_>>());
        assert_eq!(look_bits(goal), look_bits(&old.goal));
        assert_eq!(
            (goal.monochrome, goal.content.len(), goal.clusters.len()),
            (old.goal.monochrome, old.goal.content.len(), old.goal.clusters.len())
        );
        // JSON's shortest finite float encoding distinguishes every finite bit pattern,
        // including signed zero, while including every nested editable/layer field.
        assert_eq!(serde_json::to_vec(settings).unwrap(), serde_json::to_vec(&old.settings).unwrap());
        assert_eq!(serde_json::to_vec(full_settings).unwrap(), serde_json::to_vec(&old.full_settings).unwrap());
    }
    #[test]
    fn thin_descriptor_keeps_every_solved_field_render_budget_and_cancel_boundary_exact() {
        let src = Arc::new(Rgb32f::from_fn(80, 60, |x, y| {
            let v = 0.025 + x as f32 / 120.0;
            match y / 15 {
                0 => [v; 3],
                1 => [v * 1.2, v * 0.75, v * 0.6],
                2 => [v * 0.6, v * 0.8, v * 1.25],
                _ => [v * 0.6, v, v * 0.7],
            }
        }));
        let info = SourceInfo::default();
        let mut base = DevelopSettings::default();
        base.light.exposure = 0.1;
        base.color.saturation = 5.0;
        base.masks.push(Mask {
            id: 7,
            name: "Existing".into(),
            components: vec![MaskComponent { shape: MaskShape::Full, op: Default::default(), invert: false, name: None }],
            ..Default::default()
        });
        let mut desired = base.clone();
        desired.light.exposure = 0.5;
        desired.light.contrast = 9.0;
        desired.color.saturation = -20.0;
        desired.grading.shadows.hue = 210.0;
        desired.grading.shadows.sat = 7.0;
        let refs = [analyze(&src, &info, &desired)];
        let faces = [[0.1, 0.1, 0.6, 0.7]];
        for case in 0..8 {
            let opts = Options {
                new_layer: case & 1 != 0,
                appearance_aware: case & 2 != 0,
                region_aware: case & 4 != 0,
                consistent_batch: true,
                strength: [0.0, 57.0, 100.0, 125.0][case % 4],
                layer_placement: (case == 7).then_some(super::super::LayerPlacement { index: 0, id: 7 }),
                ..Default::default()
            };
            let faces = if case & 1 != 0 { &faces[..] } else { &[][..] };
            for shared in [None, Some(&desired)] {
                let techniques = Techniques { parallel_probes: false, ..Default::default() };
                let (new, old) = lightcraft_raster::sequential(|| {
                    (
                        solve_impl::<false>(&src, &info, &base, &refs, opts, faces, techniques, shared, &|| false).unwrap(),
                        solve_impl::<true>(&src, &info, &base, &refs, opts, faces, techniques, shared, &|| false).unwrap(),
                    )
                });
                assert_solved_exact(&new, &old);
            }
        }
        // A deterministic mid-solve cancel keeps exactly the original poll sequence.
        let cancel_at = |full: bool| {
            let polls = AtomicUsize::new(0);
            let cancel = || polls.fetch_add(1, Ordering::Relaxed) >= 5;
            let result = lightcraft_raster::sequential(|| {
                if full {
                    solve_impl::<true>(&src, &info, &base, &refs, Options::default(), &[], Techniques::default(), None, &cancel)
                } else {
                    solve_impl::<false>(&src, &info, &base, &refs, Options::default(), &[], Techniques::default(), None, &cancel)
                }
            });
            assert!(result.is_none());
            polls.load(Ordering::Relaxed)
        };
        assert_eq!(cancel_at(false), cancel_at(true));
    }
    #[test]
    #[ignore = "descriptor timing diagnostic; run explicitly; shared-host contention may affect timings"]
    fn thin_descriptor_timing() {
        use std::{hint::black_box, time::Instant};
        let image = image(128, 96, 0);
        assert_eq!(descriptor_bits(&measure(&image)), descriptor_bits(&super::super::measure(&image)));
        for thin in [false, true] {
            black_box(if thin { measure(&image) } else { super::super::measure(&image) });
        }
        let mut timings: [Vec<f64>; 2] = Default::default();
        for iteration in 0..20 {
            for thin in [iteration % 2 == 0, iteration % 2 != 0] {
                let start = Instant::now();
                black_box(if thin { measure(&image) } else { super::super::measure(&image) });
                timings[usize::from(thin)].push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        for values in &mut timings {
            values.sort_by(f64::total_cmp);
        }
        let old = timings[0][10];
        let new = timings[1][10];
        eprintln!(
            "target descriptor128x96 full={old:.3}ms thin={new:.3}ms ratio={:.3}x; optimized dev test; shared-host compilation/native-test contention is uncontrolled",
            old / new
        );
    }
}
