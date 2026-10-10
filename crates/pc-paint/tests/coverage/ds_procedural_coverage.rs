use photocraft_paint::procedural;
use photocraft_paint::{GrayTile, PatternStyle};

fn all_tips(n: u32, seed: u32) -> Vec<GrayTile> {
    vec![
        procedural::chalk_tip(n, seed),
        procedural::spatter_tip(n, seed, 12),
        procedural::bristle_tip(n, seed),
        procedural::charcoal_tip(n, seed),
        procedural::leaf_tip(n),
        procedural::grass_tip(n, seed, 9),
        procedural::sponge_tip(n, seed),
        procedural::star_tip(n),
        procedural::rake_tip(n, seed, 7),
    ]
}

#[test]
fn pattern_size_is_clamped_to_min_8() {
    for size in [0, 1, 7] {
        let p = procedural::pattern(PatternStyle::Noise, size, 0);
        assert_eq!(p.width, 8);
        assert_eq!(p.height, 8);
        assert_eq!(p.data.len(), 8 * 8);
    }
}

#[test]
fn pattern_size_is_clamped_to_max_1024() {
    let p = procedural::pattern(PatternStyle::Noise, 1025, 0);
    assert_eq!(p.width, 1024);
    assert_eq!(p.height, 1024);
    assert_eq!(p.data.len(), 1024 * 1024);
}

#[test]
fn pattern_values_are_in_unit_interval_for_all_styles() {
    for style in [PatternStyle::Noise, PatternStyle::Paper, PatternStyle::Canvas, PatternStyle::Dots] {
        let p = procedural::pattern(style, 64, 3);
        assert!(p.data.iter().all(|v| (0.0..=1.0).contains(v)), "Style {:?} out of range", style);
    }
}

#[test]
fn pattern_is_deterministic_same_input() {
    for style in [PatternStyle::Noise, PatternStyle::Paper, PatternStyle::Canvas, PatternStyle::Dots] {
        let a = procedural::pattern(style, 64, 42);
        let b = procedural::pattern(style, 64, 42);
        assert_eq!(a.data, b.data, "Style {:?} not deterministic", style);
    }
}

#[test]
fn pattern_different_seed_produces_different_data() {
    let a = procedural::pattern(PatternStyle::Noise, 64, 10);
    let b = procedural::pattern(PatternStyle::Noise, 64, 11);
    assert_ne!(a.data, b.data, "Different seeds should produce different noise");
}

#[test]
fn pattern_noise_seam_is_small() {
    let p = procedural::pattern(PatternStyle::Noise, 64, 1);
    let n = p.width;
    let seam: f32 = (0..n).map(|y| (p.data[y * n] - p.data[y * n + n - 1]).abs()).sum::<f32>() / n as f32;
    assert!(seam < 0.08, "Left/right seam too large: {}", seam);
}

#[test]
fn pattern_extreme_seed_does_not_panic() {
    for style in [PatternStyle::Noise, PatternStyle::Paper, PatternStyle::Canvas, PatternStyle::Dots] {
        let p = procedural::pattern(style, 32, u32::MAX);
        assert_eq!(p.width, 32);
        assert_eq!(p.height, 32);
    }
}

#[test]
fn tip_dimensions_match_for_odd_sizes() {
    for n in [1, 2, 3, 7, 13, 255] {
        for tile in all_tips(n, 1) {
            assert_eq!(tile.width, n, "width mismatch for n={}", n);
            assert_eq!(tile.height, n, "height mismatch for n={}", n);
            assert_eq!(tile.to_f32().len(), (n as usize).pow(2), "data length mismatch for n={}", n);
        }
    }
}

#[test]
fn tip_n_zero_returns_empty_tile() {
    for tile in all_tips(0, 1) {
        assert_eq!(tile.width, 0);
        assert_eq!(tile.height, 0);
        assert!(tile.to_f32().is_empty());
    }
}

#[test]
fn tip_values_are_finite_and_in_range() {
    for tile in all_tips(48, 1) {
        assert!(tile.is_valid());
        let data = tile.to_f32();
        for &v in &data {
            assert!(v.is_finite());
            assert!((0.0..=1.0).contains(&v));
        }
    }
}

#[test]
fn tip_has_paint() {
    for tile in all_tips(48, 1) {
        let sum: f32 = tile.to_f32().iter().sum();
        assert!(sum > 48.0, "tile appears empty, sum = {}", sum);
    }
}

#[test]
fn tip_corners_are_zero() {
    for tile in all_tips(48, 1) {
        assert_eq!(tile.get(0, 0), 0.0, "corner should be zero for a tip");
    }
}

#[test]
fn tip_deterministic_same_seed() {
    let first = all_tips(48, 5);
    let second = all_tips(48, 5);
    for (a, b) in first.iter().zip(second.iter()) {
        assert_eq!(a.to_f32(), b.to_f32(), "tip not deterministic");
    }
}

#[test]
fn tip_different_seed_produces_different_data() {
    let seed1 = all_tips(48, 100);
    let seed2 = all_tips(48, 101);
    // At least some seeded tips should differ
    let mut any_diff = false;
    for (a, b) in seed1.iter().zip(seed2.iter()) {
        if a.to_f32() != b.to_f32() {
            any_diff = true;
            break;
        }
    }
    assert!(any_diff, "No tip changed with different seed");
}

#[test]
fn tip_extreme_seed_does_not_panic() {
    for tile in all_tips(32, u32::MAX) {
        assert!(tile.is_valid());
    }
}

#[test]
fn tip_reasonable_counts_no_panic() {
    // Avoid huge allocations, but test moderately large counts
    let tiles = [procedural::spatter_tip(64, 1, 100), procedural::grass_tip(64, 1, 100), procedural::rake_tip(64, 1, 100)];
    for tile in tiles {
        assert!(tile.is_valid());
        assert_eq!(tile.width, 64);
        assert_eq!(tile.height, 64);
    }
}

#[test]
fn unseeded_tips_are_deterministic_across_calls() {
    let leaf1 = procedural::leaf_tip(32);
    let leaf2 = procedural::leaf_tip(32);
    let star1 = procedural::star_tip(32);
    let star2 = procedural::star_tip(32);
    assert_eq!(leaf1.to_f32(), leaf2.to_f32());
    assert_eq!(star1.to_f32(), star2.to_f32());
}
