//! Original coupled prior on incremental channel-curve chroma, in normalized knot space.
//! Shared luminance moves are free; rapidly alternating channel casts are discouraged.
pub(super) const WEIGHT: f64 = 0.00003;

pub(super) fn matrix(i: usize, j: usize) -> f64 {
    let (ci, ki) = (i / 3, i % 3);
    let (cj, kj) = (j / 3, j % 3);
    let laplacian = [[-2.0, 1.0, 0.0], [1.0, -2.0, 1.0], [0.0, 1.0, -2.0]];
    let spatial = (0..3).map(|k| laplacian[k][ki] * laplacian[k][kj]).sum::<f64>();
    WEIGHT * (f64::from(ci == cj) - 1.0 / 3.0) * spatial
}
pub(super) fn energy(delta: &[f64; 9]) -> f64 {
    (0..9).map(|i| delta[i] * (0..9).map(|j| matrix(i, j) * delta[j]).sum::<f64>()).sum::<f64>().max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coupled_prior_is_positive_symmetric_and_preserves_luminance_direction() {
        let same = [0.1, 0.3, -0.1, 0.1, 0.3, -0.1, 0.1, 0.3, -0.1];
        assert!(energy(&same) < 1e-18);
        for i in 0..9 {
            for j in 0..9 {
                assert!((matrix(i, j) - matrix(j, i)).abs() < 1e-18);
            }
        }
        let smooth = [0.3, 0.5, 0.3, -0.3, -0.5, -0.3, 0.0, 0.0, 0.0];
        let alternating = [0.3, -0.5, 0.3, -0.3, 0.5, -0.3, 0.0, 0.0, 0.0];
        assert!(energy(&alternating) > energy(&smooth) * 10.0);
        for i in 0..9 {
            let mut plus = smooth;
            plus[i] += 1e-5;
            let mut minus = smooth;
            minus[i] -= 1e-5;
            let numeric = (energy(&plus) - energy(&minus)) / 2e-5;
            let analytic = 2.0 * (0..9).map(|j| matrix(i, j) * smooth[j]).sum::<f64>();
            assert!((numeric - analytic).abs() < 1e-12);
        }
    }
}
