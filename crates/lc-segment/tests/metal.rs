//! Small, deterministic Metal acceptance with no model download.
#![cfg(target_os = "macos")]

use candle_core::{Device, Tensor};

#[test]
fn apple_silicon_tensor_kernels_match_cpu() {
    let device = lightcraft_segment::best_device();
    assert!(device.is_metal(), "SAM must select Metal on the native Mac test host");
    let values: Vec<f32> = (0..64).map(|i| (i as f32 - 32.0) / 32.0).collect();
    let compute = |device: &Device| -> candle_core::Result<Vec<f32>> {
        let input = Tensor::from_vec(values.clone(), (8, 8), device)?;
        let product = input.matmul(&input.t()?.contiguous()?)?;
        let output = candle_nn::ops::softmax(&product.relu()?, 1)?;
        output.to_device(&Device::Cpu)?.flatten_all()?.to_vec1::<f32>()
    };
    let expected = compute(&Device::Cpu).unwrap();
    let actual = compute(&device).unwrap();
    for (expected, actual) in expected.iter().zip(actual) {
        assert!((expected - actual).abs() < 1e-5, "CPU {expected}, Metal {actual}");
    }
}
