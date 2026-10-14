//! Experimental appearance groups, not semantic materials. Membership is frozen before
//! fitting: changing a probe's hue must not change which source observations it explains.
//! Original math; no external implementation or learned model is used.

pub(super) const FEATURES: usize = 63;
const GROUPS: usize = 9;
const STRIDE: usize = 7;
const QUANTILES: [f64; 5] = [0.1, 0.25, 0.5, 0.75, 0.9];

#[derive(Clone, Debug)]
pub(super) struct Membership {
    weights: Vec<[f64; GROUPS]>,
    reliability: [f64; GROUPS],
}

/// Skip experimental statistics in ordinary solver probes without changing public measurements.
pub(super) fn disabled() -> Membership {
    Membership { weights: Vec::new(), reliability: [0.0; GROUPS] }
}

/// Without dimensions, effective sample count is available but spatial dispersion is not.
#[cfg(test)]
pub(super) fn memberships(labs: &[[f64; 3]]) -> Membership {
    memberships_with_dimensions(labs, 0, 0)
}

/// An optional 8×8 tile coverage check avoids trusting a small isolated patch. Dimensions
/// must describe exactly this raster; invalid dimensions fall back to sample support.
pub(super) fn memberships_with_dimensions(labs: &[[f64; 3]], width: usize, height: usize) -> Membership {
    let spatial = width > 0 && height > 0 && width.checked_mul(height) == Some(labs.len());
    let mut mass = [0.0; GROUPS];
    let mut squared = [0.0; GROUPS];
    let mut tiles = [[0.0; 64]; GROUPS];
    let weights = labs
        .iter()
        .enumerate()
        .map(|(i, &[l, a, b])| {
            let mut w = [0.0; GROUPS];
            if [l, a, b].iter().all(|v| v.is_finite()) && (0.04..0.98).contains(&l) {
                let c = a.hypot(b);
                // Soft neutral core includes modest beige casts without calling them known white.
                w[0] = (1.0 - c / 0.055).clamp(0.0, 1.0).powi(2);
                let chromatic = ((c - 0.015) / 0.04).clamp(0.0, 1.0);
                let hues = crate::colorops::band_weights(b.atan2(a) as f32);
                for g in 0..8 {
                    w[g + 1] = chromatic * f64::from(hues[g]);
                }
            }
            let tile = if spatial { ((i / width) * 8 / height) * 8 + (i % width) * 8 / width } else { 0 };
            for g in 0..GROUPS {
                mass[g] += w[g];
                squared[g] += w[g] * w[g];
                if spatial {
                    tiles[g][tile] += w[g];
                }
            }
            w
        })
        .collect();
    let mut reliability = std::array::from_fn(|g| {
        let effective = mass[g] * mass[g] / squared[g].max(1e-12);
        if mass[g] < 3.0 || effective < 12.0 {
            return 0.0;
        }
        // Once sufficiently sampled, a large sky has no more influence than a small facade.
        let sample = ((effective - 8.0) / 24.0).clamp(0.0, 1.0);
        let dispersion = if spatial {
            let occupied = tiles[g].iter().filter(|&&v| v >= 0.5).count();
            if occupied < 2 { 0.0 } else { (occupied as f64 / 4.0).min(1.0) }
        } else {
            1.0
        };
        sample * dispersion
    });
    let total = reliability.iter().sum::<f64>();
    if total > 0.0 {
        for value in &mut reliability {
            *value /= total;
        }
    }
    Membership { weights, reliability }
}

/// Returns five lightness quantiles and two trimmed chroma means per group. Support is
/// identical across its seven statistics; normalize again after intersecting two images'
/// supported groups. A missing class contributes no target, including the zero placeholder.
pub(super) fn measure(labs: &[[f64; 3]], membership: &Membership) -> ([f64; FEATURES], [f64; FEATURES]) {
    let mut features = [0.0; FEATURES];
    let mut support = [0.0; FEATURES];
    if labs.len() != membership.weights.len() || labs.is_empty() {
        return (features, support);
    }
    for g in 0..GROUPS {
        if membership.reliability[g] <= 0.0 {
            continue;
        }
        let mut axes: [Vec<(f64, f64)>; 3] = std::array::from_fn(|_| Vec::new());
        let mut original_mass = 0.0;
        for (lab, weights) in labs.iter().zip(&membership.weights) {
            let w = weights[g];
            original_mass += w;
            if w <= 0.0 || !lab.iter().all(|v| v.is_finite()) {
                continue;
            }
            for axis in 0..3 {
                axes[axis].push((lab[axis], w));
            }
        }
        let mass = axes[0].iter().map(|p| p.1).sum::<f64>();
        if mass < 3.0 || mass < 0.95 * original_mass {
            continue;
        }
        for values in &mut axes {
            values.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        let start = g * STRIDE;
        for (q, quantile) in QUANTILES.iter().enumerate() {
            features[start + q] = weighted_quantile(&axes[0], *quantile, mass);
        }
        features[start + 5] = trimmed_mean(&axes[1], mass);
        features[start + 6] = trimmed_mean(&axes[2], mass);
        for value in &mut support[start..start + STRIDE] {
            *value = membership.reliability[g];
        }
    }
    (features, support)
}

fn weighted_quantile(sorted: &[(f64, f64)], q: f64, mass: f64) -> f64 {
    let mut cumulative = 0.0;
    for &(v, w) in sorted {
        cumulative += w;
        // Identical weighted populations must keep their quantiles when a class is tiled;
        // accumulation roundoff at an exact CDF boundary must not select the next value.
        if cumulative + 1e-12 * mass >= q * mass {
            return v;
        }
    }
    sorted.last().map_or(0.0, |p| p.0)
}

fn trimmed_mean(sorted: &[(f64, f64)], mass: f64) -> f64 {
    let (lo, hi) = (0.1 * mass, 0.9 * mass);
    let (mut cumulative, mut sum, mut kept) = (0.0, 0.0, 0.0);
    for &(v, w) in sorted {
        let end = cumulative + w;
        let overlap = (end.min(hi) - cumulative.max(lo)).max(0.0);
        sum += v * overlap;
        kept += overlap;
        cumulative = end;
    }
    sum / kept.max(1e-12)
}

pub(super) fn valid_value(i: usize, v: f64) -> bool {
    i < FEATURES && v.is_finite() && if i % STRIDE < 5 { (-0.001..=1.01).contains(&v) } else { (-0.5..=0.5).contains(&v) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(counts: [usize; 3]) -> Vec<[f64; 3]> {
        let hues = crate::colorops::band_hues();
        let colors = [
            [0.004, 0.002],
            [f64::from(hues[1]).cos() * 0.14, f64::from(hues[1]).sin() * 0.14],
            [f64::from(hues[5]).cos() * 0.14, f64::from(hues[5]).sin() * 0.14],
        ];
        let mut labs = Vec::new();
        for (group, count) in counts.iter().enumerate() {
            for _ in 0..*count {
                for l in [0.32, 0.43, 0.54, 0.65] {
                    labs.push([l, colors[group][0], colors[group][1]]);
                }
            }
        }
        labs
    }

    fn matched_error(a: &([f64; FEATURES], [f64; FEATURES]), b: &([f64; FEATURES], [f64; FEATURES])) -> f64 {
        let total = (0..FEATURES).map(|i| a.1[i].min(b.1[i])).sum::<f64>();
        (0..FEATURES).map(|i| (a.0[i] - b.0[i]).powi(2) * a.1[i].min(b.1[i])).sum::<f64>() / total.max(1e-12)
    }

    #[test]
    fn appearance_occupancy_does_not_change_shared_color_or_tone() {
        let small_sky = scene([40, 40, 12]);
        let large_sky = scene([12, 12, 400]);
        let a = measure(&small_sky, &memberships(&small_sky));
        let b = measure(&large_sky, &memberships(&large_sky));
        assert!(matched_error(&a, &b) < 1e-20);
        for i in 0..FEATURES {
            assert!((a.1[i] - b.1[i]).abs() < 1e-10, "area changed class influence at {i}");
        }
        // The global mean does change, demonstrating the composition confound.
        let global = |v: &[[f64; 3]]| v.iter().map(|p| p[2]).sum::<f64>() / v.len() as f64;
        assert!((global(&small_sky) - global(&large_sky)).abs() > 0.04);
    }

    #[test]
    fn appearance_absent_blue_has_no_target_for_warm_and_neutral_source() {
        let source = scene([40, 40, 0]);
        let reference = scene([40, 40, 100]);
        let a = measure(&source, &memberships(&source));
        let b = measure(&reference, &memberships(&reference));
        assert!(matched_error(&a, &b) < 1e-20);
        assert_eq!(a.1[6 * STRIDE..7 * STRIDE], [0.0; STRIDE]);
        assert!(b.1[6 * STRIDE] > 0.0);
    }

    #[test]
    fn appearance_probe_hue_change_cannot_evade_frozen_observations() {
        let source = scene([0, 0, 40]);
        let frozen = memberships(&source);
        let changed = source.iter().map(|p| [p[0], 0.14, 0.06]).collect::<Vec<_>>();
        let a = measure(&source, &frozen);
        let b = measure(&changed, &frozen);
        assert_eq!(a.1, b.1);
        assert!(matched_error(&a, &b) > 0.001);
    }

    #[test]
    fn appearance_rejects_isolated_spatial_support_and_invalid_lengths() {
        let mut image = vec![[0.0; 3]; 64 * 64];
        for y in 0..6 {
            for x in 0..6 {
                image[y * 64 + x] = [0.5, 0.0, 0.0];
            }
        }
        let frozen = memberships_with_dimensions(&image, 64, 64);
        assert!(measure(&image, &frozen).1.iter().all(|w| *w == 0.0));
        assert!(measure(&image[..3], &frozen).1.iter().all(|w| *w == 0.0));
        assert!(measure(&[], &memberships(&[])).1.iter().all(|w| *w == 0.0));
    }
}
