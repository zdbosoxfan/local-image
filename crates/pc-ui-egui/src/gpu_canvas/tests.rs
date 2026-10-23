use super::*;

#[test]
fn gpu_budget_follows_memory_usage_and_ram() {
    let _gpu_gate = crate::gpu_test_support::lock();
    const G: u64 = 1 << 30;
    // What Memory Usage leaves after the document, within a quarter of RAM.
    assert_eq!(memory_budget(8 * G, 3 * G, Some(48 * G), None, 0), 5 * G);
    assert_eq!(memory_budget(8 * G, 3 * G, Some(16 * G), None, 0), 4 * G);
    assert_eq!(memory_budget(64 * G, 3 * G, Some(16 * G), None, 0), 4 * G);
    // Never below the floor, even with nothing left; 16 GB assumed when RAM is unknown.
    assert_eq!(memory_budget(8 * G, 10 * G, Some(48 * G), None, 0), MIN_GPU_BUDGET);
    assert_eq!(memory_budget(64 * G, 0, None, None, 0), 4 * G);
    assert!(physical_memory().is_none_or(|b| b >= 256 << 20));
}

#[test]
fn small_mip_levels_are_redrawn_whole() {
    use resources::mip_damage;
    // A 3 × 3 dab at (800, 500) of a 1791 × 1083 tile, at mip 1 (895 × 541): halved and padded.
    assert_eq!(mip_damage([800, 500, 803, 503], 895, 541), [399, 249, 403, 253]);
    // At most 128 px on either side: the whole level, wherever the damage is.
    assert_eq!(mip_damage([100, 60, 102, 63], 111, 67), [0, 0, 111, 67]);
    assert_eq!(mip_damage([0, 0, 1, 1], 128, 2), [0, 0, 128, 2]);
    assert_eq!(mip_damage([0, 0, 1, 1], 129, 2), [0, 0, 2, 2]);
}

#[test]
fn filter_modes() {
    let _gpu_gate = crate::gpu_test_support::lock();
    assert_eq!(filter_mode(0.25), (0.0, 2.0));
    assert_eq!(filter_mode(1.0).0, 2.0);
    assert_eq!(filter_mode(1.5).0, 1.0);
    assert_eq!(filter_mode(3.3).0, 2.0);
}

#[test]
fn mips_and_premultiply() {
    let _gpu_gate = crate::gpu_test_support::lock();
    assert_eq!(mip_count(1, 1), 1);
    assert_eq!(mip_count(4096, 100), 13);
    assert_eq!(mip_count(5, 3), 3);
    assert_eq!(premultiply_rgba8(&[[1.0, 0.5, 0.0, 0.5]]), vec![128, 64, 0, 128]);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn half_floats() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // Every finite half survives the round trip; ties round to even; overflow saturates.
    for h in 0..=u16::MAX {
        if h & 0x7c00 != 0x7c00 && h != 0x8000 {
            assert_eq!(f32_to_f16(half::f16::from_bits(h).to_f32()), h, "{h:#06x}");
        }
    }
    assert_eq!(f32_to_f16(1.0), 0x3c00);
    assert_eq!(f32_to_f16(1.0 + 2f32.powi(-11)), 0x3c00);
    assert_eq!(f32_to_f16(1.0 + 3.0 * 2f32.powi(-11)), 0x3c02);
    assert_eq!(f32_to_f16(2f32.powi(-24)), 1);
    assert_eq!(f32_to_f16(2f32.powi(-26)), 0);
    assert_eq!(f32_to_f16(65504.0), 0x7bff);
    assert_eq!(f32_to_f16(1e9), 0x7bff);
    assert_eq!(f32_to_f16(f32::INFINITY), 0x7bff);
    assert_eq!(f32_to_f16(f32::NAN), 0);
    assert_eq!(f32_to_f16(-2.0), 0xc000);
    assert_eq!(premultiply_rgba16f(&[[2.0, -1.0, f32::NAN, 0.5]]), [0x00, 0x3c, 0, 0, 0, 0, 0x00, 0x38]);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn shaders_validate() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use wgpu::naga;
    for (name, src) in [("canvas", CANVAS_WGSL), ("mip", MIP_WGSL), ("encode", ENCODE_WGSL)] {
        let src = if name == "mip" { src.to_string() } else { li_gpu::wgsl::source(src) };
        let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&src)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod health_and_output_tests {
    use super::*;

    #[test]
    fn display_budget_rejects_large_canvases_and_downgrades_float_textures() {
        let _gate = crate::gpu_test_support::lock();
        let rs = egui_kittest::wgpu::create_render_state(wgpu_setup(), Default::default());
        assert_ne!(rs.adapter.get_info().device_type, wgpu::DeviceType::Cpu);
        let mut gpu = GpuCanvas::new(&rs);
        let size = [128, 64];
        let floats = canvas_texture_bytes(size, gpu.tile, 8);
        gpu.high = HighPolicy::Budget;
        gpu.canvas_budget = floats - 1;
        assert_eq!(gpu.format_for(photocraft_doc::SampleType::U16, size), FORMAT);
        gpu.canvas_budget = floats;
        assert_eq!(gpu.format_for(photocraft_doc::SampleType::U16, size), FORMAT_HIGH);
        let doc =
            photocraft_doc::Document::new("Huge", photocraft_doc::Size::new(300_000, 300_000), photocraft_doc::ColorMode::Rgb, photocraft_doc::SampleType::U8);
        gpu.upload_full(doc.id.0, size, &vec![0; size[0] as usize * size[1] as usize * 4]);
        assert!(gpu.texture_info(doc.id.0).is_some());
        let result = gpu.refresh(doc.id.0, &doc, None, None);
        assert_eq!(result.kind, REFRESH_OVER_BUDGET);
        assert!(gpu.composite(&doc, doc.bounds(), false).is_err());
        gpu.upload_composite(doc.id.0, &doc, None);
        assert!(!gpu.has(doc.id.0, [300_000, 300_000]));
        assert!(gpu.texture_info(doc.id.0).is_none(), "growing past the budget releases the stale display");
        assert_eq!(gpu.health.errors(), 0);
        li_gpu::device::release_window();
    }

    #[test]
    fn soft_circuit_breaker_displays_cpu_and_recovers_same_device() {
        let _gate = crate::gpu_test_support::lock();
        let mut rs = egui_kittest::wgpu::create_render_state(wgpu_setup(), Default::default());
        rs.target_format = wgpu::TextureFormat::Rgba8Unorm;
        let info = rs.adapter.get_info();
        assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "physical GPU required: {info:?}");
        let mut app = crate::PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.set_wgpu(rs.clone());
        let gpu = app.gpu.clone().expect("production canvas must be installed");
        let shared = li_gpu::device::current().unwrap();
        let device = shared.device.clone();
        let ctx = egui::Context::default();
        let doc = photocraft_doc::Document::with_background(
            "CPU recovery white",
            photocraft_doc::Size::new(256, 16),
            photocraft_doc::ColorMode::Rgb,
            photocraft_doc::SampleType::U16,
            photocraft_doc::Color::WHITE,
        );
        app.session.add_document(doc.clone(), None);
        gpu.upload_composite(doc.id.0, &doc, None);
        assert!(gpu.has(doc.id.0, [256, 16]));
        for frame in 1..=3 {
            app.frame = frame;
            gpu.begin_frame(frame);
            gpu.health.operation_failed("repeated recoverable allocation failure", 71);
            crate::gpu_status::check(&mut app, &ctx);
            assert!(app.gpu_active(), "soft error at frame {frame} must retain the production canvas");
            assert_eq!(li_gpu::device::current().unwrap().device, device);
        }
        assert!(matches!(gpu.health.fault(), Some(Fault::Error(_))));
        assert!(gpu.fault().is_none(), "only a lost device removes the shell canvas");
        assert!(gpu.health.can_present());
        assert!(!gpu.health.can_render());
        assert!(!gpu.has(doc.id.0, [256, 16]), "failed-operation textures were invalidated");
        let refresh = gpu.refresh(doc.id.0, &doc, None, None);
        assert_eq!(refresh.kind, "full", "paused acceleration uses the CPU composite/upload path");
        assert!(refresh.fallback.as_deref().is_some_and(|reason| reason.contains("repeated recoverable allocation failure")));
        let (_, size, pixels) = app.read_canvas_texels(doc.id.0).expect("CPU fallback remains readable while the circuit is paused");
        assert_eq!(size, [256, 16]);
        assert!(pixels.iter().all(|pixel| *pixel == [1.0; 4]), "production CPU upload preserves white exactly");
        let cb = CanvasCallback {
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256.0, 16.0)),
            params: ViewParams {
                doc: doc.id.0,
                doc_size: size,
                zoom: 1.0,
                center: [128.0, 8.0],
                shadow: false,
                pixel_grid: false,
                view_key: 71,
                display: 0,
                output: 0,
                hdr: None,
                flip: false,
                rotation: 0.0,
            },
        };
        let shown = float_capture(&gpu, &cb, CANVAS_WGSL, 0.0);
        assert_eq!(shown, pixels, "actual production callback must paint the CPU fallback during soft backoff");
        assert!(gpu.health.can_present() && !gpu.health.can_render());
        gpu.health.retry_now();
        assert!(gpu.health.can_render());
        assert!(app.gpu_active());
        assert_eq!(li_gpu::device::current().unwrap().device, device, "retry reuses the same window device");
        assert!(shared.health.can_render(), "Develop/compositor observe the same successful retry");
        eprintln!(
            "soft circuit breaker {:?}/{:?}: 3 failed frames, live CPU white upload + 4096 exact callback pixels, retry on the same device",
            info.name, info.backend
        );
        li_gpu::device::release_window();
    }

    // Independent CPU oracle: the established Develop TPDF hash, generalized to output depth.
    fn expected_code(v: f32, x: u32, y: u32, levels: u32) -> u32 {
        let mut h = x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x7feb_352d);
        h ^= h >> 16;
        h = h.wrapping_mul(0x7feb_352d);
        h ^= h >> 15;
        h = h.wrapping_mul(0x846c_a68b);
        h ^= h >> 16;
        let noise = (h & 65535) as f32 / 65536.0 - (h >> 16) as f32 / 65536.0;
        let q = v.clamp(0.0, 1.0) * levels as f32;
        let fade = q.min(levels as f32 - q).clamp(0.0, 1.0);
        (q + noise * fade + 0.5).clamp(0.0, levels as f32) as u32
    }

    // Execute the production compositor-to-canvas encode pipeline, then let the production
    // callback consume those same tiles. No replacement shader or alternate final renderer.
    fn encode_ramp(gpu: &GpuCanvas, size: [u32; 2], ramp: &[[f32; 4]], previous: bool) {
        let (device, queue) = (&gpu.rs.device, &gpu.rs.queue);
        let source = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("linear composite sample"),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bytes: Vec<_> = ramp
            .iter()
            .flat_map(|p| {
                let v = li_color::convert::srgb_to_linear(p[0]);
                [v, v, v, 1.0].into_iter().flat_map(f32::to_le_bytes)
            })
            .collect();
        queue.write_texture(
            source.as_image_copy(),
            &bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size[0] * 16), rows_per_image: Some(size[1]) },
            wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        );
        let source_view = source.create_view(&Default::default());
        let offset =
            init_buffer(device, "encode offset", &[0_i32, 0, 1, 0].into_iter().flat_map(i32::to_le_bytes).collect::<Vec<_>>(), wgpu::BufferUsages::UNIFORM);
        let renderer = gpu.rs.renderer.read();
        let res = renderer.callback_resources.get::<Resources>().unwrap();
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("production encode"),
            layout: &res.encode_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&source_view) },
                wgpu::BindGroupEntry { binding: 1, resource: offset.as_entire_binding() },
            ],
        });
        // Original vector transfer helper from HEAD, used only as an independent golden.
        let previous_body = ENCODE_WGSL.replace("linear_to_srgb3(rgb)", "srgb_encode(rgb)");
        let previous_source = format!(
            r#"
fn srgb_encode(c: vec3<f32>) -> vec3<f32> {{
    return select(1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3(0.0031308));
}}
{previous_body}
"#
        );
        let previous_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("original vector encode"),
            source: wgpu::ShaderSource::Wgsl(previous_source.into()),
        });
        let previous_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("original encode bindings"),
            bind_group_layouts: &[Some(&res.encode_bgl)],
            immediate_size: 0,
        });
        let previous_pipeline = pipeline(device, "original vector encode", &previous_layout, &previous_module, ("vs", "fs"), FORMAT_HIGH, None);
        let doc = res.docs.get(&1).unwrap();
        assert_eq!(doc.tiles.len(), 1);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("production encode"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &doc.tiles[0].levels[0],
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(if previous { &previous_pipeline } else { res.encode_pipeline(FORMAT_HIGH).unwrap() });
            pass.set_bind_group(0, &bindings, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        assert!(gpu.health.wait(device, None));
    }

    // Capture the actual callback's fragment value in float storage. This separates texture
    // sampling/dither arithmetic from the fixed-function UNORM target conversion.
    fn float_capture(gpu: &GpuCanvas, cb: &CanvasCallback, body: &str, levels: f32) -> Vec<[f32; 4]> {
        let rs = &gpu.rs;
        let size = cb.params.doc_size;
        let module = rs.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("final float diagnostic"),
            source: wgpu::ShaderSource::Wgsl(li_gpu::wgsl::source(body).into()),
        });
        let mut renderer = rs.renderer.write();
        let res = renderer.callback_resources.get_mut::<Resources>().unwrap();
        let layout = rs.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&res.view_bgl), Some(&res.tile_bgl), Some(&res.lut_bgl)],
            immediate_size: 0,
        });
        let pipe = pipeline(&rs.device, "float diagnostic", &layout, &module, ("vs_tile", "fs_tile"), wgpu::TextureFormat::Rgba32Float, None);
        let original = std::mem::replace(&mut res.tile_pipeline, pipe);
        let original_levels = std::mem::replace(&mut res.depth_levels, levels);
        let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("final float diagnostic"),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = rs.device.create_command_encoder(&Default::default());
        let extra = cb.prepare(
            &rs.device,
            &rs.queue,
            &ScreenDescriptor { size_in_pixels: size, pixels_per_point: 1.0 },
            &mut encoder,
            &mut renderer.callback_resources,
        );
        rs.queue.submit(extra);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            cb.paint(
                egui::PaintCallbackInfo { viewport: cb.rect, clip_rect: cb.rect, pixels_per_point: 1.0, screen_size_px: size },
                &mut pass,
                &renderer.callback_resources,
            );
        }
        let res = renderer.callback_resources.get_mut::<Resources>().unwrap();
        res.tile_pipeline = original;
        res.depth_levels = original_levels;
        drop(renderer);
        let read = rs.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(size[0] * size[1] * 16),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size[0] * 16), rows_per_image: Some(size[1]) },
            },
            wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        );
        rs.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        read.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        assert!(gpu.health.wait_present(&rs.device, None));
        rx.recv().unwrap().unwrap();
        let data = read.slice(..).get_mapped_range().unwrap();
        let result =
            data.as_chunks::<16>().0.iter().map(|pixel| std::array::from_fn(|c| f32::from_le_bytes(pixel[c * 4..c * 4 + 4].try_into().unwrap()))).collect();
        drop(data);
        read.unmap();
        result
    }

    #[test]
    fn final_canvas_tpdf_endpoints_neutral_ramp_and_shared_health() {
        let _gate = crate::gpu_test_support::lock();
        for body in [CANVAS_WGSL, ENCODE_WGSL] {
            let source = li_gpu::wgsl::source(body);
            let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
            wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(), wgpu::naga::valid::Capabilities::empty()).validate(&module).unwrap();
        }
        let base = egui_kittest::wgpu::create_render_state(wgpu_setup(), Default::default());
        let info = base.adapter.get_info();
        assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "physical GPU required: {info:?}");
        assert!(supports_f16_canvas(&base.adapter), "hardware float canvas required: {info:?}");
        let first = GpuCanvas::new(&base);
        let second = GpuCanvas::new(&base);
        let shared = li_gpu::device::current().unwrap();
        assert_eq!(shared.device, base.device);
        let errors = first.health.errors();
        first.health.operation_failed("injected recoverable allocation failure", 17);
        for health in [&first.health, &second.health, &shared.health] {
            assert_eq!(health.errors(), errors + 1);
            assert_eq!(health.strikes(), 1);
            assert!(!health.can_render());
            assert!(health.needs_retry());
            assert!(health.is_ok());
        }
        second.health.retry_now();
        assert!(first.health.can_render() && shared.health.can_render());
        second.health.operation_succeeded();

        let size = [256_u32, 16_u32];
        let mut buf = photocraft_compose::Buffer::transparent(photocraft_geom::Rect::new(0, 0, 256, 16));
        for (i, p) in buf.px.iter_mut().enumerate() {
            let x = i % 256;
            // Exact half-float samples, including pure black and white in every row.
            let v = if x == 255 { 1.0 } else { x as f32 / 256.0 };
            *p = [v, v, v, 1.0];
        }
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256.0, 16.0));
        let cb = CanvasCallback {
            rect,
            params: ViewParams {
                doc: 1,
                doc_size: size,
                zoom: 1.0,
                center: [128.0, 8.0],
                shadow: false,
                pixel_grid: false,
                view_key: 7,
                display: 0,
                output: 0,
                hdr: None,
                flip: false,
                rotation: 0.0,
            },
        };
        for format in [wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureFormat::Rgb10a2Unorm] {
            let mut rs = base.clone();
            rs.target_format = format;
            let gpu = GpuCanvas::new(&rs);
            gpu.upload_buffer_full(1, &buf, photocraft_doc::SampleType::U16);
            gpu.set_pixel_preview(1, true);
            encode_ramp(&gpu, size, &buf.px, true);
            let (_, _, previous_encoded) = gpu.read_texels(1).unwrap();
            encode_ramp(&gpu, size, &buf.px, false);
            let (source_format, source_size, encoded) = gpu.read_texels(1).unwrap();
            assert_eq!((source_format, source_size), (FORMAT_HIGH, size));
            for (i, (actual, previous)) in encoded.iter().zip(&previous_encoded).enumerate() {
                if i % 256 == 255 {
                    assert_eq!(*actual, [1.0; 4], "mathematical white is exact after shared encoding");
                    assert!(previous[0] <= actual[0], "white endpoint accuracy cannot regress");
                } else {
                    assert_eq!(actual, previous, "non-white half texel {i} must remain bit-exact to original vector helper");
                }
            }
            eprintln!(
                "encode white: before={}, after={} (exact mathematical endpoint); all 4080 nonwhite texels bit-exact",
                previous_encoded[255][0], encoded[255][0]
            );
            let mut worst_encode = 0.0_f32;
            let mut worst_storage_ulps = 0_u16;
            for (actual, expected) in encoded.iter().zip(&buf.px) {
                worst_encode = worst_encode.max((actual[0] - expected[0]).abs());
                // An exact representable fixture eliminates input/storage uncertainty. One
                // adjacent half code permits only the measured floating-point transfer plus
                // render-target conversion; exact old/new texel equality above is the gate
                // against regression. This bound scales with exponent, unlike a float epsilon.
                let actual_half = f32_to_f16(actual[0]);
                let expected_half = f32_to_f16(expected[0]);
                let ulps = actual_half.abs_diff(expected_half);
                worst_storage_ulps = worst_storage_ulps.max(ulps);
                assert!(ulps <= 1, "encode exceeds one adjacent half storage code: actual={actual:?}, expected={expected:?}");
            }
            eprintln!(
                "production encode nonwhite bit-exact and white exact, CPU maximum error={worst_encode:.9}, max half storage code difference={worst_storage_ulps}"
            );
            let levels = if format == wgpu::TextureFormat::Rgb10a2Unorm { 1023 } else { 255 };
            let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("final canvas"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let target = texture.create_view(&Default::default());
            let read = rs.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("final canvas readback"),
                size: u64::from(size[0] * size[1] * 4),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            // The before sample changes only the final dither block back to the previous
            // Bayer shader. Every production binding, callback, texture and encode path stays
            // the same, so these timings compare this slice rather than an alternate renderer.
            let mut before_body = CANVAS_WGSL.to_owned();
            let from = before_body.find("        let xy = vec2<u32>(p);").unwrap();
            let to = from + before_body[from..].find("        );").unwrap() + "        );".len();
            before_body.replace_range(
                from..to,
                r#"
        let xy = vec2<u32>(p);
        var rank = 0u;
        for (var bit = 0u; bit < 3u; bit++) {
            let xb = (xy.x >> bit) & 1u;
            let yb = (xy.y >> bit) & 1u;
            rank = rank * 4u + ((xb ^ yb) * 2u + yb);
        }
        rgb = clamp(rgb + vec3((f32(rank) + 0.5) / 64.0 - 0.5) / view.i.x, vec3(0.0), vec3(1.0));
"#,
            );
            let module = rs.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("previous Bayer final pass"),
                source: wgpu::ShaderSource::Wgsl(li_gpu::wgsl::source(&before_body).into()),
            });
            let mut renderer = rs.renderer.write();
            let res = renderer.callback_resources.get_mut::<Resources>().unwrap();
            let layout = rs.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("previous final-pass bindings"),
                bind_group_layouts: &[Some(&res.view_bgl), Some(&res.tile_bgl), Some(&res.lut_bgl)],
                immediate_size: 0,
            });
            let current = res.tile_pipeline.clone();
            let previous = pipeline(
                &rs.device,
                "previous Bayer",
                &layout,
                &module,
                ("vs_tile", "fs_tile"),
                format,
                Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
            );
            drop(renderer);
            let sampled = float_capture(&gpu, &cb, CANVAS_WGSL, 0.0);
            assert_eq!(sampled, encoded, "actual production textureLoad samples must equal stored encoded texels before dither");
            for before in [true, false] {
                let final_float = float_capture(&gpu, &cb, if before { &before_body } else { CANVAS_WGSL }, levels as f32);
                eprintln!(
                    "float diagnostic {format:?} before={before}, x4,y0: stored={}, sampled={}, final_float={} (code={}); x255,y0: stored={}, final_float={}",
                    encoded[4][0],
                    sampled[4][0],
                    final_float[4][0],
                    final_float[4][0] * levels as f32,
                    encoded[255][0],
                    final_float[255][0]
                );
                rs.renderer.write().callback_resources.get_mut::<Resources>().unwrap().tile_pipeline = if before { previous.clone() } else { current.clone() };
                let mut render_ms = Vec::new();
                let mut readback_ms = Vec::new();
                let mut baseline_error_sum = 0_u64;
                let mut baseline_error_max = 0_u32;
                let mut baseline_count = 0_u64;
                // One warm-up and three measured final passes: a small sample, no timing gate.
                for frame in 0..4 {
                    let start = std::time::Instant::now();
                    let mut encoder = rs.device.create_command_encoder(&Default::default());
                    let mut renderer = rs.renderer.write();
                    let extra = cb.prepare(
                        &rs.device,
                        &rs.queue,
                        &ScreenDescriptor { size_in_pixels: size, pixels_per_point: 1.0 },
                        &mut encoder,
                        &mut renderer.callback_resources,
                    );
                    rs.queue.submit(extra);
                    {
                        let mut pass = encoder
                            .begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("production final pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &target,
                                    resolve_target: None,
                                    depth_slice: None,
                                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            })
                            .forget_lifetime();
                        cb.paint(
                            egui::PaintCallbackInfo { viewport: rect, clip_rect: rect, pixels_per_point: 1.0, screen_size_px: size },
                            &mut pass,
                            &renderer.callback_resources,
                        );
                    }
                    drop(renderer);
                    rs.queue.submit([encoder.finish()]);
                    assert!(gpu.health.wait(&rs.device, None));
                    let render = start.elapsed().as_secs_f64() * 1000.0;
                    let start = std::time::Instant::now();
                    let mut encoder = rs.device.create_command_encoder(&Default::default());
                    encoder.copy_texture_to_buffer(
                        texture.as_image_copy(),
                        wgpu::TexelCopyBufferInfo {
                            buffer: &read,
                            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size[0] * 4), rows_per_image: Some(size[1]) },
                        },
                        wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                    );
                    rs.queue.submit([encoder.finish()]);
                    let (tx, rx) = std::sync::mpsc::sync_channel(1);
                    read.slice(..).map_async(wgpu::MapMode::Read, move |result| {
                        let _ = tx.send(result);
                    });
                    assert!(gpu.health.wait(&rs.device, None));
                    rx.recv().unwrap().unwrap();
                    let data = read.slice(..).get_mapped_range().unwrap();
                    let readback = start.elapsed().as_secs_f64() * 1000.0;
                    for (i, pixel) in data.as_chunks::<4>().0.iter().enumerate() {
                        let actual = if levels == 1023 {
                            let packed = u32::from_le_bytes(*pixel);
                            [packed & 1023, (packed >> 10) & 1023, (packed >> 20) & 1023]
                        } else {
                            [u32::from(pixel[0]), u32::from(pixel[1]), u32::from(pixel[2])]
                        };
                        let x = (i % 256) as u32;
                        let y = (i / 256) as u32;
                        let expected = if before {
                            let mut rank = 0_u32;
                            for bit in 0..3 {
                                let xb = (x >> bit) & 1;
                                let yb = (y >> bit) & 1;
                                rank = rank * 4 + ((xb ^ yb) * 2 + yb);
                            }
                            let noise = (rank as f32 + 0.5) / 64.0 - 0.5;
                            ((encoded[i][0] + noise / levels as f32).clamp(0.0, 1.0) * levels as f32).round() as u32
                        } else {
                            expected_code(encoded[i][0], x, y, levels)
                        };
                        assert_eq!(actual[0], actual[1], "neutral green {format:?} before={before} ({x},{y})");
                        assert_eq!(actual[0], actual[2], "neutral blue {format:?} before={before} ({x},{y})");
                        if before {
                            // Vulkan permits either adjacent integer for float->UNORM,
                            // and blending precision may be only destination precision:
                            // https://docs.vulkan.org/spec/latest/chapters/fundamentals.html#fundamentals-fixedfpconv
                            // The legacy shader leaves rounding to that conversion; measure
                            // it against nearest without treating its hardware choice as a
                            // regression. The new shader must still match the CPU exactly.
                            let code = (final_float[i][0] * levels as f32).clamp(0.0, levels as f32);
                            assert!(
                                actual[0] >= code.floor() as u32 && actual[0] <= code.ceil() as u32,
                                "legacy conversion outside adjacent integers {format:?} ({x},{y}): actual={} fragment_code={code}",
                                actual[0]
                            );
                            let error = actual[0].abs_diff(expected);
                            assert!(error <= 1, "legacy conversion differs by more than one code from nearest");
                            baseline_error_max = baseline_error_max.max(error);
                            baseline_error_sum += u64::from(error);
                            baseline_count += 1;
                        } else {
                            assert_eq!(
                                actual,
                                [expected; 3],
                                "final {format:?} pixel ({x},{y}), frame {frame}, stored={} sampled={} final_float={} final_code={}",
                                encoded[i][0],
                                sampled[i][0],
                                final_float[i][0],
                                final_float[i][0] * levels as f32
                            );
                            if x == 0 {
                                assert_eq!(actual, [0; 3]);
                            }
                            if x == 255 {
                                assert_eq!(actual, [levels; 3]);
                            }
                        }
                    }
                    drop(data);
                    read.unmap();
                    if frame != 0 {
                        render_ms.push(render);
                        readback_ms.push(readback);
                    }
                }
                assert_eq!(gpu.health.errors(), errors + 1, "production shaders raised an error");
                if before {
                    eprintln!(
                        "previous Bayer native conversion {format:?}: nearest-code mean absolute error={:.9}, max={baseline_error_max}; new TPDF requires exact nearest-code equality",
                        baseline_error_sum as f64 / baseline_count as f64
                    );
                }
                eprintln!(
                    "production final canvas {:?}/{:?} {format:?}, {} 256x16: 16384 RGB pixels, neutral exact (old adjacent native conversion / new exact CPU codes and endpoints); encode max={worst_encode:.9}; three warm frame+GPU-wait ms={render_ms:?}; separate copy+readback ms={readback_ms:?}",
                    info.name,
                    info.backend,
                    if before { "before Bayer" } else { "after TPDF" }
                );
            }
        }
        li_gpu::device::release_window();
    }
}
