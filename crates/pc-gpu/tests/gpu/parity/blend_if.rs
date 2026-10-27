use super::*;

/// Execute the production WGSL bit arithmetic and compare exact CPU result bits. This covers
/// every U8/U16 grayscale code, colored byte samples, adjacent F32 slider values and subnormals.
#[test]
fn blend_if_integer_arithmetic_matches_cpu() {
    use wgpu::util::DeviceExt;
    let g = gpu();
    let mut colors = Vec::<[f32; 3]>::new();
    for levels in [255, 65535] {
        colors.extend((0..=levels).map(|n| [n as f32 / levels as f32; 3]));
    }
    for r in 0..=255 {
        for green in 0..=255 {
            colors.push([r as f32 / 255.0, green as f32 / 255.0, ((r * 37 + green * 79) % 256) as f32 / 255.0]);
        }
    }
    for n in 0..=255 {
        let bits = (n as f32 / 255.0).to_bits();
        for adjacent in bits.saturating_sub(4)..=bits + 4 {
            colors.push([f32::from_bits(adjacent); 3]);
        }
    }
    for bits in [1, 2, 3, 0x3fffff, 0x7fffff, 0x800000, 0x800001, 0x1000000, 0x3e400000, 0x3f000000, 0x3f800000] {
        colors.push([f32::from_bits(bits); 3]);
    }
    for i in 0..10000u32 {
        // Arbitrary normal/subnormal f32 exponents, not just uniform normalized sample codes.
        colors.push(std::array::from_fn(|c| f32::from_bits(i.wrapping_mul(0x9e3779b9).wrapping_add(c as u32 * 0x1234567) % 0x3f800001)));
    }
    let input: Vec<u8> = colors.iter().flat_map(|c| c.iter().copied().chain([0.0]).flat_map(|f| f.to_bits().to_le_bytes())).collect();
    let helpers =
        photocraft_gpu::SHADER.split("// BEGIN Blend If integer arithmetic").nth(1).unwrap().split("// END Blend If integer arithmetic").next().unwrap();
    let shader = format!(
        "{helpers}\n\
         @group(0) @binding(0) var<storage, read> input: array<vec4<u32>>;\n\
         @group(0) @binding(1) var<storage, read_write> output: array<vec4<u32>>;\n\
         @compute @workgroup_size(128) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{\n\
         if (id.x >= arrayLength(&input)) {{ return; }}\n\
         let v = bi_values(bitcast<vec3<f32>>(input[id.x].xyz));\n\
         output[id.x] = vec4(v.x, v.y, bi_add_sub(0x437f0000u, v.x, true), bi_add_sub(0x437f0000u, v.y, true));\n\
         }}"
    );
    let module = g
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("Blend If exact arithmetic"), source: wgpu::ShaderSource::Wgsl(shader.into()) });
    let pipeline = g.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let source = g.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: &input, usage: wgpu::BufferUsages::STORAGE });
    let output = g.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: input.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = g.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: input.len() as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let group = g.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: source.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: output.as_entire_binding() },
        ],
    });
    let mut encoder = g.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups((colors.len() as u32).div_ceil(128), 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, input.len() as u64);
    g.queue.submit([encoder.finish()]);
    readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    g.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    for (i, (rgb, row)) in colors.iter().zip(bytes.as_chunks::<16>().0).enumerate() {
        let v = rgb.map(|c| c.clamp(0.0, 1.0) * 255.0);
        let gray = 0.299 * v[0] + 0.587 * v[1] + 0.114 * v[2];
        let expected = [gray, v[0], 255.0 - gray, 255.0 - v[0]].map(f32::to_bits);
        let actual: [u32; 4] = std::array::from_fn(|c| u32::from_le_bytes(row[c * 4..c * 4 + 4].try_into().unwrap()));
        assert_eq!(actual, expected, "integer WGSL math case {i}, {rgb:?}");
    }
}

#[test]
fn blend_if_ranges_depth_modes_and_boundaries() {
    let mut g = gpu();
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        for mode in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen, BlendMode::Overlay] {
            let mut d = Document::new("Blend If", Size::new(72, 48), ColorMode::Rgb, depth);
            let fmt = d.pixel_format();
            d.layers.push(noise_layer("under", fmt, d.bounds(), 123, 0.0));
            let mut l = noise_layer("this", fmt, Rect::new(-12, -8, 65, 43), 456, 0.0);
            l.blend_if = blend_ranges();
            l.opacity = 0.63;
            l.fill_opacity = 0.79;
            l.blend = mode;
            l.mask = Some(mask(d.bounds(), 19, 1.0));
            l.excluded_channels = 2;
            d.layers.push(l);
            check(&mut g, &d, &format!("Blend If {depth:?} {mode:?}"));
        }
    }
    // Exact unsplit endpoints (including fully transparent this/under pixels).
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        for ch in 0..4 {
            let mut d = Document::new("endpoints", Size::new(8, 2), ColorMode::Rgb, depth);
            let mut l = Layer::raster("endpoints", d.pixel_format());
            for (x, v) in [0.0, 49.5, 50.0, 50.5, 199.5, 200.0, 200.5, 255.0].into_iter().enumerate() {
                let mut px = [v / 255.0; 4];
                px[3] = 0.8;
                l.surface_mut().unwrap().fill_rect(Rect::from_xywh(x as i32, 0, 1, 2), &px);
            }
            l.blend_if.set(ch, [photocraft_doc::BlendRange { black: [50, 50], white: [200, 200] }, photocraft_doc::BlendRange::FULL]);
            d.layers.push(l);
            check(&mut g, &d, &format!("inclusive Blend If endpoints {depth:?} channel {ch}"));
            d.layers[0].blend_if.set(ch, [photocraft_doc::BlendRange { black: [255, 255], white: [255, 255] }, photocraft_doc::BlendRange::FULL]);
            check(&mut g, &d, &format!("white Blend If endpoint {depth:?} channel {ch}"));
        }
    }
    let mut d = base_doc(32, 16);
    for mode in [ColorMode::Grayscale, ColorMode::Duotone] {
        d.mode = mode;
        d.layers[0].blend_if = blend_ranges();
        check(&mut g, &d, &format!("Blend If {mode:?}"));
    }
    // Neighbors must retain their CPU distinction: no epsilon snapping and no quantization
    // of fractional Gray/composited inputs, including the CPU's rounded white subtraction.
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        let mut values = Vec::new();
        for slider in [0u8, 1, 50, 65, 195, 200, 244, 254, 255] {
            let at = (f32::from(slider) / 255.0).to_bits();
            values.extend((at.saturating_sub(3)..=at + 3).map(f32::from_bits));
            if depth == SampleType::U16 {
                let code = u32::from(slider) * 257;
                values.extend((code.saturating_sub(1)..=(code + 1).min(65535)).map(|v| v as f32 / 65535.0));
            }
        }
        for ch in 0..4 {
            for side in 0..2 {
                let mut d = Document::new("adjacent endpoints", Size::new(values.len() as u32, 2), ColorMode::Rgb, depth);
                let mut samples = Layer::raster("boundary samples", d.pixel_format());
                for (x, &v) in values.iter().enumerate() {
                    // An opaque underlying source preserves these exact test bits through its
                    // first composite, isolating comparison from blend reciprocal accuracy.
                    let alpha = if side == 1 { 1.0 } else { 0.8 };
                    samples.surface_mut().unwrap().fill_rect(Rect::from_xywh(x as i32, 0, 1, 2), &[v, v, v, alpha]);
                }
                d.layers.push(samples);
                if side == 1 {
                    let mut top = Layer::raster("test underlying", d.pixel_format());
                    top.surface_mut().unwrap().fill_rect(d.bounds(), &[0.2, 0.5, 0.9, 0.8]);
                    d.layers.push(top);
                }
                for range in [
                    photocraft_doc::BlendRange { black: [50, 50], white: [200, 200] },
                    photocraft_doc::BlendRange { black: [50, 65], white: [195, 200] },
                    photocraft_doc::BlendRange { black: [50, 51], white: [199, 200] },
                    photocraft_doc::BlendRange { black: [1, 1], white: [1, 1] },
                ] {
                    let mut pair = [photocraft_doc::BlendRange::FULL; 2];
                    pair[side] = range;
                    d.layers[side].blend_if.set(ch, pair);
                    check(&mut g, &d, &format!("adjacent {depth:?} channel {ch} side {side} {range:?}"));
                }
            }
        }
    }
}

#[test]
fn blend_if_groups_clipping_adjustments_and_damage() {
    let mut g = gpu();
    for group_mode in [BlendMode::PassThrough, BlendMode::Multiply] {
        let mut d = base_doc(530, 90);
        let mut base = noise_layer("base", d.pixel_format(), Rect::new(220, -10, 520, 80), 31, 0.3);
        base.blend_if = blend_ranges();
        let mut clipped = noise_layer("clip", d.pixel_format(), d.bounds(), 41, 0.1);
        clipped.clipped = true;
        clipped.blend = BlendMode::Screen;
        clipped.blend_if = blend_ranges();
        clipped.mask = Some(mask(d.bounds(), 19, 1.0));
        let mut adj = Layer::new("adjusted this", LayerContent::Adjustment(Adjustment::Invert));
        adj.clipped = true;
        adj.opacity = 0.4;
        adj.blend_if = blend_ranges();
        let mut group = Layer::group("group", vec![base, clipped, adj]);
        group.blend = group_mode;
        group.opacity = 0.7;
        group.mask = Some(mask(d.bounds(), 29, 1.0));
        group.blend_if = blend_ranges();
        group.excluded_channels = 1;
        d.layers.push(group);
        check(&mut g, &d, &format!("Blend If group {group_mode:?}"));
        let r = Rect::new(250, 15, 275, 40);
        d.layers[1].children_mut().unwrap()[0].surface_mut().unwrap().fill_rect(r, &[0.9, 0.1, 0.2, 0.8]);
        diff_rect(&mut g, &d, r, "Blend If damaged page boundary").unwrap();
        check(&mut g, &d, "Blend If after damage");
        // Cached ranges must invalidate on a slider edit and undo.
        let undo = d.clone();
        d.layers[1].blend_if.set(2, [photocraft_doc::BlendRange::FULL; 2]);
        check(&mut g, &d, "Blend If slider changed");
        check(&mut g, &undo, "Blend If slider undo");
    }
}

#[test]
fn noisy_group_shape_tracks_blend_if_edits_and_document_mode() {
    let mut g = gpu();
    let mut d = base_doc(300, 80);
    let mut child = blob("child", d.pixel_format(), 250.0, 40.0, 25.0, [0.8, 0.3, 0.1]);
    child.blend_if = blend_ranges();
    let mut group = Layer::group("noisy group", vec![child]);
    group.mask = Some(mask(d.bounds(), 551, 1.0));
    let mut glow = glow(FxPaint::Color(Color::rgb(0.9, 0.6, 0.2)), GlowTechnique::Softer, 8.0, 0.25, GlowSource::Edge);
    glow.noise = 0.4;
    group.effects.items = vec![Effect::OuterGlow(glow)];
    d.layers.push(group);
    fx_check(&mut g, &d, "noise on Blend If group shape");
    d.layers[1].children_mut().unwrap()[0].blend_if.set(3, [photocraft_doc::BlendRange::FULL; 2]);
    // The CPU's pre-existing effect cache omits child Blend If and document mode. Compare
    // fresh reference maps for these synthetic edits, without changing CPU cache behavior.
    photocraft_compose::purge_effect_cache();
    fx_check(&mut g, &d, "group child's Blend If edited");
    d.mode = ColorMode::Lab;
    photocraft_compose::purge_effect_cache();
    fx_check(&mut g, &d, "group Blend If disabled by Lab mode");
}
