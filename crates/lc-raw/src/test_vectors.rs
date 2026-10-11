//! Independent upstream vectors; see tests/fixtures/README.md.
pub(crate) fn read(name: &str) -> Vec<f32> {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap();
    assert!(bytes.len().is_multiple_of(4));
    bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect()
}
pub(crate) fn compare(name: &str, actual: &[f32], tolerance: f32) {
    let expected = read(name);
    assert_eq!(actual.len(), expected.len());
    let mut max = 0.0f32;
    let mut sum = 0.0f64;
    for (&a, &b) in actual.iter().zip(&expected) {
        let d = (a - b).abs();
        assert!(a.is_finite() && b.is_finite());
        max = max.max(d);
        sum += f64::from(d) * f64::from(d);
    }
    eprintln!("{name}: {} samples max {max:.9} rms {:.9}", actual.len(), (sum / actual.len() as f64).sqrt());
    assert!(max <= tolerance, "{name}: {max} > {tolerance}");
}
pub(crate) fn noise(i: usize) -> f32 {
    ((i as u32).wrapping_mul(1664525).wrapping_add(1013904223) >> 8 & 65535) as f32 / 65536.0
}
