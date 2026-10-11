use lightcraft_merge::ransac::{Model, Rng, dlt, fit_model, ransac};

#[test]
fn model_variants_are_distinct() {
    // Ensure the three model variants are distinct and correctly matched.
    assert_ne!(Model::Translation, Model::Similarity);
    assert_ne!(Model::Translation, Model::Homography);
    assert_ne!(Model::Similarity, Model::Homography);
    assert_eq!(Model::Translation, Model::Translation);
    assert_eq!(Model::Similarity, Model::Similarity);
    assert_eq!(Model::Homography, Model::Homography);
}

#[test]
fn rng_new_zero_seed_equals_seed_one() {
    // Rng::new clamps seed 0 to 1.
    let mut a = Rng::new(0);
    let mut b = Rng::new(1);
    for _ in 0..10 {
        assert_eq!(a.next_u64(), b.next_u64());
    }
}

#[test]
fn rng_same_seed_sequence_is_deterministic() {
    let mut a = Rng::new(42);
    let mut b = Rng::new(42);
    for _ in 0..20 {
        assert_eq!(a.next_u64(), b.next_u64());
        assert_eq!(a.below(100), b.below(100));
        assert_eq!(a.unit(), b.unit());
    }
}

#[test]
fn rng_different_seeds_produce_different_sequences() {
    let mut a = Rng::new(1);
    let mut b = Rng::new(2);
    let mut differ = false;
    for _ in 0..10 {
        if a.next_u64() != b.next_u64() {
            differ = true;
            break;
        }
    }
    assert!(differ, "different seeds should eventually produce different values");
}

#[test]
fn rng_below_zero_returns_zero() {
    // below(0) uses n.max(1) internally and returns 0.
    let mut rng = Rng::new(7);
    for _ in 0..10 {
        assert_eq!(rng.below(0), 0);
    }
}

#[test]
fn rng_below_one_returns_zero() {
    let mut rng = Rng::new(8);
    for _ in 0..10 {
        assert_eq!(rng.below(1), 0);
    }
}

#[test]
fn rng_below_many_values_within_range() {
    let mut rng = Rng::new(123);
    for n in [2usize, 3, 10, 100, 1000, 1_000_000] {
        for _ in 0..100 {
            let v = rng.below(n);
            assert!(v < n, "below({}) returned {}, out of range", n, v);
        }
    }
}

#[test]
fn rng_below_handles_large_n() {
    let mut rng = Rng::new(999);
    // Should not panic and should be < usize::MAX.
    let v = rng.below(usize::MAX);
    assert!(v < usize::MAX);
}

#[test]
fn rng_unit_values_are_in_unit_interval() {
    let mut rng = Rng::new(456);
    for _ in 0..10_000 {
        let x = rng.unit();
        assert!((0.0..1.0).contains(&x), "unit() returned {}", x);
    }
}

#[test]
fn rng_unit_values_are_finite_and_bounded() {
    let mut rng = Rng::new(789);
    for _ in 0..5_000 {
        let x = rng.unit();
        assert!(x.is_finite(), "unit() returned non-finite {}", x);
        assert!((0.0..1.0).contains(&x), "unit() returned out of range {}", x);
    }
}

#[test]
fn fit_model_translation_empty_returns_none() {
    let result = fit_model(Model::Translation, &[], &[]);
    assert!(result.is_none());
}

#[test]
fn fit_model_similarity_empty_returns_none() {
    let result = fit_model(Model::Similarity, &[], &[]);
    assert!(result.is_none());
}

#[test]
fn fit_model_homography_empty_returns_none() {
    let result = fit_model(Model::Homography, &[], &[]);
    assert!(result.is_none());
}

#[test]
fn dlt_empty_returns_none() {
    let result = dlt(&[], &[]);
    assert!(result.is_none());
}

#[test]
fn ransac_empty_returns_none() {
    let result = ransac(Model::Translation, &[], &[], 1.0, 0, 0);
    assert!(result.is_none());
}

#[test]
fn ransac_insufficient_min_inliers_returns_none() {
    // For Homography, k=4. With empty input, any min_inliers > 0 makes n < k.max(min_inliers).
    let result = ransac(Model::Homography, &[], &[], 2.0, 10, 123);
    assert!(result.is_none());
}
