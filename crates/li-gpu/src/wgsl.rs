//! Binding-free WGSL helpers shared by Develop and Compositing.
//!
//! Prepend [`COMMON`] once to a shader body. Image-processing kernels and tool-specific
//! noise stay with their CPU references; only the display transfer/dither are unified.

/// sRGB scalar/vector transfer, neutral TPDF quantisation, and analytic Gaussian box coverage.
pub const COMMON: &str = include_str!("../wgsl/common.wgsl");

/// Build a shader from the common helpers and a caller's bindings/entry points.
pub fn source(body: &str) -> String {
    let mut source = String::with_capacity(COMMON.len() + body.len() + 1);
    source.push_str(COMMON);
    source.push('\n');
    source.push_str(body);
    source
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(x: u32, y: u32) -> f32 {
        let mut h = x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x7feb_352d);
        h ^= h >> 16;
        h = h.wrapping_mul(0x7feb_352d);
        h ^= h >> 15;
        h = h.wrapping_mul(0x846c_a68b);
        h ^= h >> 16;
        (h & 65535) as f32 / 65536.0 - (h >> 16) as f32 / 65536.0
    }

    #[test]
    fn shared_wgsl_tpdf_golden_and_spatial_quality() {
        assert_eq!(lightcraft_pipeline::finish::DITHER_HASH, [0x7feb_352d, 0x846c_a68b, 0x9e37_79b9]);
        let mut mean = 0.0_f64;
        let mut variance = 0.0_f64;
        let mut correlation8 = 0.0_f64;
        for y in 0..128 {
            for x in 0..128 {
                let n = f64::from(noise(x, y));
                mean += n;
                variance += n * n;
                correlation8 += n * f64::from(noise(x + 8, y));
                for value in [0.0, 0.0001, 0.125, 0.5, 0.875, 0.9999, 1.0] {
                    let q = value * 255.0_f32;
                    let fade = q.min(255.0 - q).clamp(0.0, 1.0);
                    let golden = (q + noise(x, y) * fade + 0.5).clamp(0.0, 255.0) as u8;
                    assert_eq!(golden, lightcraft_pipeline::finish::dither8(value, x as usize, y as usize, 0));
                }
            }
        }
        mean /= 16384.0;
        variance /= 16384.0;
        correlation8 /= 16384.0;
        assert!(mean.abs() < 0.01, "TPDF mean {mean}");
        assert!((variance - 1.0 / 6.0).abs() < 0.01, "TPDF variance {variance}");
        assert!(correlation8.abs() < 0.01, "8-pixel repeat correlation {correlation8}");
        // The replaced canvas Bayer matrix repeats every eight pixels: lag-8 correlation
        // is its full variance, 0.08331298828125. The TPDF field has no such repeating tile.
        eprintln!("shared display dither 128x128: mean={mean:.8}, variance={variance:.8}, lag8={correlation8:.8}; previous Bayer lag8=0.08331299");
    }

    #[test]
    fn shared_wgsl_helpers_validate() -> Result<(), Box<dyn std::error::Error>> {
        let shader = source(PROBE);
        let module = wgpu::naga::front::wgsl::parse_str(&shader)?;
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(), wgpu::naga::valid::Capabilities::empty()).validate(&module)?;
        Ok(())
    }

    fn erf(x: f32) -> f32 {
        let a = x.abs();
        let mut t = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
        t *= t;
        x.signum() - x.signum() / (t * t)
    }

    fn coverage(p: [f32; 2], sigma: f32) -> f32 {
        let k = std::f32::consts::FRAC_1_SQRT_2 / sigma;
        let x = (erf((p[0] + 3.0) * k) - erf((p[0] - 5.0) * k)) * 0.5;
        let y = (erf((p[1] + 2.0) * k) - erf((p[1] - 7.0) * k)) * 0.5;
        x * y
    }

    // A small real-device sample, not a broad GPU acceptance sweep. There is no successful
    // no-adapter path: software rendering or unavailable hardware must fail this parity gate.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn shared_wgsl_real_gpu_parity() -> Result<(), Box<dyn std::error::Error>> {
        let _init = crate::TEST_INIT_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let shared = crate::device::create_headless(wgpu::Backends::PRIMARY, false, crate::DeviceHealth::new())?;
        let info = shared.adapter.get_info();
        assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "real GPU required: {info:?}");
        let device = &shared.device;
        let queue = &shared.queue;
        let shader = source(PROBE);
        let module =
            device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("li-gpu helper parity"), source: wgpu::ShaderSource::Wgsl(shader.into()) });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("li-gpu helper parity"),
            layout: None,
            module: &module,
            entry_point: Some("probe"),
            compilation_options: Default::default(),
            cache: None,
        });
        let bytes = 4096 * 3 * 16;
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("li-gpu helper results"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("li-gpu helper readback"),
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: output.as_entire_binding() }],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bindings, &[]);
            pass.dispatch_workgroups(64, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None })?;
        rx.recv()??;
        let data = readback.slice(..).get_mapped_range()?;
        let values: &[f32] = bytemuck::cast_slice(&data);
        let mut worst_transfer = 0.0_f32;
        let mut worst_shadow = 0.0_f32;
        let mut worst_display_code = 0.0_f32;
        for i in 0..4096_usize {
            let xy = [(i % 64) as u32, (i / 64) as u32];
            let v = sample(i);
            let n = noise(xy[0], xy[1]);
            let rows = &values[i * 12..i * 12 + 12];
            let reference = [photocraft_color::convert::linear_to_srgb(v), photocraft_color::convert::srgb_to_linear(v)];
            for (a, b) in rows[..2].iter().zip(reference) {
                worst_transfer = worst_transfer.max((a - b).abs());
            }
            assert_eq!(rows[2], n, "TPDF sample {i}");
            let golden = lightcraft_pipeline::finish::dither8(v, xy[0] as usize, xy[1] as usize, 0);
            assert_eq!(rows[3], f32::from(golden), "Develop byte {i}");
            for (channel, c) in [v, v * 0.5, v * 2.0].iter().enumerate() {
                worst_transfer = worst_transfer.max((rows[4 + channel] - photocraft_color::convert::linear_to_srgb(*c)).abs());
                worst_transfer = worst_transfer.max((rows[8 + channel] - photocraft_color::convert::srgb_to_linear(*c)).abs());
            }
            let sigma = [0.5, 1.25, 16.0][i % 3];
            let p = [xy[0] as f32 * 0.25 - 8.0, xy[1] as f32 * 0.25 - 8.0];
            worst_shadow = worst_shadow.max((rows[7] - coverage(p, sigma)).abs());
            let q = v.clamp(0.0, 1.0) * 1023.0;
            let code = (q + n * q.min(1023.0 - q).clamp(0.0, 1.0)).clamp(0.0, 1023.0);
            worst_display_code = worst_display_code.max((rows[11] - code).abs());
        }
        assert!(worst_transfer < 0.00002, "transfer error {worst_transfer}");
        assert!(worst_shadow < 0.000003, "Gaussian shadow error {worst_shadow}");
        assert!(worst_display_code < 0.0002, "10-bit display code error {worst_display_code}");
        eprintln!(
            "shared WGSL parity {:?}/{:?}, 4096 samples: Develop byte/TPDF exact; transfer max={worst_transfer:.9}, Gaussian max={worst_shadow:.9}, 10-bit code max={worst_display_code:.9}",
            info.name, info.backend
        );
        drop(data);
        readback.unmap();
        Ok(())
    }

    fn sample(i: usize) -> f32 {
        const SPECIAL: [f32; 12] = [-0.5, -0.001, 0.0, 0.0031308, 0.00313081, 0.04045, 0.04045001, 0.5, 0.9999, 1.0, 1.5, 2.0];
        if i < SPECIAL.len() { SPECIAL[i] } else { (i - SPECIAL.len()) as f32 / 4083.0 }
    }

    const PROBE: &str = r#"
@group(0) @binding(0) var<storage, read_write> results: array<vec4<f32>>;
fn sample(i: u32) -> f32 {
    let special = array<f32, 12>(-0.5, -0.001, 0.0, 0.0031308, 0.00313081, 0.04045, 0.04045001, 0.5, 0.9999, 1.0, 1.5, 2.0);
    if (i < 12u) { return special[i]; }
    return f32(i - 12u) / 4083.0;
}
@compute @workgroup_size(64)
fn probe(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= 4096u) { return; }
    let xy = vec2(i % 64u, i / 64u);
    let v = sample(i);
    results[3u*i] = vec4(linear_to_srgb(v), srgb_to_linear(v), li_dither_tpdf(xy), f32(li_dither8(v, xy)));
    let sigmas = array<f32, 3>(0.5, 1.25, 16.0);
    let p = vec2<f32>(xy) * 0.25 - 8.0;
    results[3u*i+1u] = vec4(linear_to_srgb3(vec3(v, v*0.5, v*2.0)), box_blur(p, vec2(-3.0, -2.0), vec2(5.0, 7.0), sigmas[i%3u]));
    results[3u*i+2u] = vec4(srgb_to_linear3(vec3(v, v*0.5, v*2.0)), li_dither_code(v, xy, 1023.0));
}
"#;
}
