//! Conservative spatial rank evidence, not semantic recognition. Color/exposure changes
//! can alter a histogram without changing scene layout. Strong layout agreement can avoid
//! attenuating normalization for the same capture. Cropped/different scenes keep fallback.

const EDGE: usize = 8;
/// Average perceptual lightness in fixed spatial cells, then rank it. Uniform images and
/// simple gradients have too little identifying structure and deliberately yield no evidence.
pub(super) fn fingerprint(labs: &[[f64; 3]], width: usize, height: usize) -> Vec<f64> {
    if width < EDGE || height < EDGE || width.checked_mul(height) != Some(labs.len()) {
        return Vec::new();
    }
    let mut values = [0.0; EDGE * EDGE];
    let mut counts = [0usize; EDGE * EDGE];
    for (i, p) in labs.iter().enumerate() {
        if !p[0].is_finite() {
            return Vec::new();
        }
        let cell = (i / width * EDGE / height) * EDGE + (i % width * EDGE / width);
        values[cell] += p[0];
        counts[cell] += 1;
    }
    for (v, n) in values.iter_mut().zip(counts) {
        *v /= n.max(1) as f64;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>();
    if variance < 0.0064 {
        return Vec::new();
    }
    let row_variation: f64 = (0..EDGE)
        .map(|y| {
            let row = &values[y * EDGE..(y + 1) * EDGE];
            let mean = row.iter().sum::<f64>() / EDGE as f64;
            row.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
        })
        .sum();
    let col_variation: f64 = (0..EDGE)
        .map(|x| {
            let mean = (0..EDGE).map(|y| values[y * EDGE + x]).sum::<f64>() / EDGE as f64;
            (0..EDGE).map(|y| (values[y * EDGE + x] - mean).powi(2)).sum::<f64>()
        })
        .sum();
    if row_variation.min(col_variation) < variance * 0.1 {
        return Vec::new();
    }
    let mut order = (0..values.len()).collect::<Vec<_>>();
    order.sort_by(|a, b| values[*a].total_cmp(&values[*b]));
    let mut ranks = vec![0.0; values.len()];
    let mut first = 0;
    while first < order.len() {
        let mut end = first + 1;
        while end < order.len() && (values[order[end]] - values[order[first]]).abs() < 1e-9 {
            end += 1;
        }
        let rank = (first + end - 1) as f64 / (order.len() - 1) as f64 - 1.0;
        for index in &order[first..end] {
            ranks[*index] = rank;
        }
        first = end;
    }
    ranks
}
pub(super) fn correlation(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != EDGE * EDGE || b.len() != a.len() {
        return 0.0;
    }
    let mean_a = a.iter().sum::<f64>() / a.len() as f64;
    let mean_b = b.iter().sum::<f64>() / b.len() as f64;
    let (mut dot, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (a, b) in a.iter().zip(b) {
        let (a, b) = (a - mean_a, b - mean_b);
        dot += a * b;
        aa += a * a;
        bb += b * b;
    }
    if aa.min(bb) < 1e-12 { 0.0 } else { (dot / (aa * bb).sqrt()).clamp(-1.0, 1.0) }
}
/// Only strong structural agreement is positive evidence. Dissimilarity does not suppress
/// the existing color/scene fallback; that would mistake crop or viewpoint for no relation.
pub(super) fn confidence(a: &[f64], b: &[f64]) -> f64 {
    ((correlation(a, b) - 0.85) / 0.05).clamp(0.0, 1.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_survives_monotone_exposure_changes_but_rejects_rearrangement() {
        let image = (0..64 * 48)
            .map(|i| {
                let (x, y) = (i % 64, i / 64);
                let v = 0.1 + ((x * 13 + y * y * 5 + (x / 9) * (y / 7) * 19) % 71) as f64 / 100.0;
                [v, 0.02, -0.01]
            })
            .collect::<Vec<_>>();
        let shifted = image.iter().map(|p| [p[0] * 0.65 + 0.2, -0.03, 0.06]).collect::<Vec<_>>();
        let a = fingerprint(&image, 64, 48);
        let b = fingerprint(&shifted, 64, 48);
        assert_eq!(a.len(), 64);
        assert!(correlation(&a, &b) > 0.999);
        let flipped = shifted.chunks(64).flat_map(|row| row.iter().rev().copied()).collect::<Vec<_>>();
        assert!(confidence(&a, &fingerprint(&flipped, 64, 48)) < 0.1);
        let ramp = (0..64 * 48).map(|i| [0.1 + (i % 64) as f64 / 80.0, 0.0, 0.0]).collect::<Vec<_>>();
        assert!(fingerprint(&ramp, 64, 48).is_empty());
        assert_eq!(confidence(&[], &[]), 0.0);
    }
}
