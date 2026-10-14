//! The tests require the physical GPU; no software-adapter skips count as validation.
//! The three graph/device gates are required explicit final acceptance in docs/GPU-VALIDATION.md.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use li_seg::{
    denoise::Variant,
    restoration_graph::{Graph, Node, Op, Shape},
};
use lightcraft_gpu::denoise::Denoiser;

#[test]
#[ignore = "required physical GPU acceptance; see docs/GPU-VALIDATION.md"]
fn denoiser_retains_its_device_across_context_release() {
    let _gpu = support::lock();
    let shape = Shape { c: 3, h: 16, w: 16 };
    let graph = Graph { input: shape, nodes: vec![Node { inputs: vec![0], shape, op: Op::Activation { alpha: 0.0, sigmoid: false } }], output: 1 };
    let denoiser = Denoiser::from_graph(graph, Variant::Rgb).expect("physical GPU is required");
    let input = vec![0.3; shape.len()];
    assert_eq!(denoiser.run_tile(&input).unwrap(), input);
    assert!(lightcraft_gpu::shutdown(std::time::Duration::from_secs(120)));
    assert!(lightcraft_gpu::available(), "a replacement context must initialize");
    assert_eq!(denoiser.run_tile(&input).unwrap(), input, "weights/tensors must use their original device");
}

#[test]
#[ignore = "required RTX 5090 acceptance; see docs/GPU-VALIDATION.md"]
fn ai_denoise_gpu_transpose_and_shuffle() {
    let _gpu = support::lock();
    let s = Shape { c: 1, h: 16, w: 16 };
    let graph = Graph {
        input: s,
        nodes: vec![
            Node {
                inputs: vec![0],
                shape: Shape { c: 3, h: 32, w: 32 },
                op: Op::Conv {
                    weights: vec![1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12.],
                    bias: vec![0.; 3],
                    kernel: [2, 2],
                    stride: [2, 2],
                    pad: [0, 0],
                    transpose: true,
                },
            },
            Node { inputs: vec![1], shape: Shape { c: 3, h: 32, w: 32 }, op: Op::Activation { alpha: 0.01, sigmoid: false } },
        ],
        output: 2,
    };
    let d = Denoiser::from_graph(graph, Variant::Linear).expect("physical GPU is required");
    let input: Vec<f32> = (0..256).map(|i| if i % 3 == 0 { -0.2 } else { 0.3 }).collect();
    let out = d.run_tile(&input).unwrap();
    for c in 0..3 {
        for y in 0..32 {
            for x in 0..32 {
                let v = input[(y / 2) * 16 + x / 2] * (c * 4 + (y % 2) * 2 + x % 2 + 1) as f32;
                let expected = if v < 0. { v * 0.01 } else { v };
                assert!((out[c * 1024 + y * 32 + x] - expected).abs() < 2e-6);
            }
        }
    }
    let graph = Graph {
        input: Shape { c: 4, ..s },
        nodes: vec![Node { inputs: vec![0], shape: Shape { c: 1, h: 32, w: 32 }, op: Op::Shuffle { block: 2, crd: true } }],
        output: 1,
    };
    let d = Denoiser::from_graph(graph, Variant::Bayer).unwrap();
    let input: Vec<f32> = (0..4).flat_map(|c| vec![c as f32; 256]).collect();
    let out = d.run_tile(&input).unwrap();
    for y in 0..32 {
        for x in 0..32 {
            assert_eq!(out[y * 32 + x], ((y % 2) * 2 + x % 2) as f32);
        }
    }
    assert!(lightcraft_gpu::adapter_name().unwrap().contains("5090"));
}

#[test]
#[ignore = "approved pinned model copies required; run explicitly on the RTX 5090"]
fn ai_denoise_real_models_match_tract_on_5090() {
    let _gpu = support::lock();
    real_models_match_tract();
    assert!(lightcraft_gpu::adapter_name().unwrap().contains("5090"));
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
#[ignore = "installed pinned on-device models required; run explicitly on Apple Silicon Metal"]
fn ai_denoise_real_models_match_tract_on_apple_silicon() {
    let _gpu = support::lock();
    let name = lightcraft_gpu::adapter_name().expect("physical Apple GPU required");
    let shared = li_gpu::device::current().expect("production shared compute device");
    let info = shared.adapter.get_info();
    assert!(info.name.contains("Apple"), "{info:?}");
    assert_eq!(info.backend, wgpu::Backend::Metal);
    assert_ne!(info.device_type, wgpu::DeviceType::Cpu);
    eprintln!("Apple Silicon denoise parity: {name}, {:?}", info.device_type);
    real_models_match_tract();
}

fn real_models_match_tract() {
    for kind in [Variant::Linear, Variant::Bayer, Variant::Rgb] {
        let dir = std::env::var_os("LOCAL_IMAGE_DENOISE_MODELS").map(std::path::PathBuf::from).expect("set model-test directory");
        let path = li_seg::companion_path(&dir, kind.file());
        let t = kind.tile();
        let input: Vec<f32> = (0..kind.channels() * t * t)
            .map(|i| {
                let c = i / (t * t);
                let x = i % t;
                let y = (i / t) % t;
                0.02 + (c as f32 * 0.02) + ((x as f32 * 0.017).sin() * (y as f32 * 0.021).cos() + 1.) * 0.1 + (i % 7) as f32 * 0.001
            })
            .collect();
        let d = Denoiser::load(&path, kind).unwrap();
        let start = std::time::Instant::now();
        let gpu = d.run_tile(&input).unwrap();
        let gpu_ms = start.elapsed().as_secs_f64() * 1000.;
        let cpu = li_seg::denoise::Denoiser::load_variant(&path, kind).unwrap();
        let start = std::time::Instant::now();
        let cpu = cpu.run_tile(&input).unwrap();
        let cpu_ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(gpu.len(), cpu.len());
        let peak = cpu.iter().fold(0f32, |a, v| a.max(v.abs()));
        let max = gpu.iter().zip(&cpu).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
        let rms = (gpu.iter().zip(&cpu).map(|(a, b)| (a - b).powi(2) as f64).sum::<f64>() / gpu.len() as f64).sqrt();
        eprintln!(
            "{kind:?}: gpu {gpu_ms:.2} ms cpu {cpu_ms:.2} ms raw peak {peak:.6} max {max:.8} rms {rms:.8} resident {} bytes",
            d.resident_bytes()
        );
        assert!(max < peak.max(1.) * 2e-4 && rms < peak.max(1.) as f64 * 2e-5, "{kind:?}: mismatch {max} / {peak}");
    }
}

#[test]
#[ignore = "required physical GPU acceptance; see docs/GPU-VALIDATION.md"]
fn ai_denoise_gpu_conv_pool_skip_and_sigmoid() {
    let _gpu = support::lock();
    let shape = Shape { c: 3, h: 16, w: 16 };
    let conv = Shape { c: 2, ..shape };
    let pooled = Shape { c: 2, h: 8, w: 8 };
    let graph = Graph {
        input: shape,
        nodes: vec![
            Node {
                inputs: vec![0],
                shape: conv,
                op: Op::Conv {
                    weights: vec![1., 2., 3., -1., 0.5, 2.],
                    bias: vec![0.1, -0.2],
                    kernel: [1, 1],
                    stride: [1, 1],
                    pad: [0, 0],
                    transpose: false,
                },
            },
            Node { inputs: vec![1], shape: pooled, op: Op::Pool { kernel: [2, 2], stride: [2, 2] } },
            Node { inputs: vec![2, 2], shape: Shape { c: 4, ..pooled }, op: Op::Concat },
            Node { inputs: vec![3], shape: Shape { c: 4, ..pooled }, op: Op::Activation { alpha: 0., sigmoid: true } },
        ],
        output: 4,
    };
    let d = Denoiser::from_graph(graph, Variant::Linear).unwrap();
    let input: Vec<_> = (0..shape.len()).map(|i| i as f32 * 0.001 - 0.2).collect();
    let out = d.run_tile(&input).unwrap();
    for c in 0..4 {
        for y in 0..8 {
            for x in 0..8 {
                let mut maximum = f32::NEG_INFINITY;
                for dy in 0..2 {
                    for dx in 0..2 {
                        let i = (y * 2 + dy) * 16 + x * 2 + dx;
                        let [r, g, b] = [input[i], input[256 + i], input[512 + i]];
                        maximum = maximum.max(if c % 2 == 0 { r + 2. * g + 3. * b + 0.1 } else { -r + 0.5 * g + 2. * b - 0.2 });
                    }
                }
                let expected = 1. / (1. + (-maximum).exp());
                assert!((out[c * 64 + y * 8 + x] - expected).abs() < 2e-6);
            }
        }
    }
}
