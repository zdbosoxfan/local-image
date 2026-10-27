use super::*;

/// Upstream 980cf05b (#3274): a child's outside stroke reaching past the group's own effect
/// reach is part of the group's effect source, so the canvas matches the CPU.
#[test]
fn group_effects_follow_a_childs_stroke() {
    let mut g = gpu();
    let mut d = fx_doc(100, 80, SampleType::U8);
    let mut a = blob("a", d.pixel_format(), 45.0, 40.0, 16.0, [0.6, 0.6, 0.6]);
    a.effects.items = vec![Effect::Stroke(stroke(8.0, StrokePosition::Outside, FxPaint::Color(Color::rgb(0.0, 0.0, 0.0))))];
    let mut grp = Layer::group("g", vec![a]);
    grp.effects.items = vec![Effect::DropShadow(shadow(BlendMode::Multiply, 0.8, 120.0, 3.0, 2.0, 0.0))];
    d.layers.push(grp);
    fx_check(&mut g, &d, "group effects around a child's stroke");
}

#[test]
fn layer_effects_every_kind() {
    let mut g = gpu();
    let mut failures = Vec::new();
    for depth in [SampleType::U8, SampleType::U16] {
        for (name, fx) in effect_cases() {
            let mut d = fx_doc(96, 80, depth);
            d.patterns.push(checker_pattern());
            let mut l = blob("fx", d.pixel_format(), 46.0, 40.0, 22.0, [0.9, 0.4, 0.2]);
            l.effects.items = fx;
            d.layers.push(l);
            if let Err(e) = fx_diff(&mut g, &d, &format!("{name} {depth:?}")) {
                failures.push(e);
            }
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn layer_effects_opacity_fill_blend_and_off_canvas() {
    let mut g = gpu();
    let stack = effect_cases().into_iter().find(|(n, _)| *n == "full stack").unwrap().1;
    for (opacity, fill, blend) in [
        (0.7, 1.0, BlendMode::Normal),
        (1.0, 0.0, BlendMode::Normal),
        (0.6, 0.35, BlendMode::Multiply),
        (1.0, 0.5, BlendMode::Screen),
        (0.8, 1.0, BlendMode::Dissolve),
    ] {
        let mut d = fx_doc(96, 80, SampleType::U8);
        let mut l = blob("fx", d.pixel_format(), 46.0, 40.0, 22.0, [0.2, 0.6, 0.9]);
        l.effects.items = stack.clone();
        l.opacity = opacity;
        l.fill_opacity = fill;
        l.blend = blend;
        d.layers.push(l);
        fx_check(&mut g, &d, &format!("opacity {opacity} fill {fill} {blend:?}"));
    }
    // Fill below 100 % with an Outside stroke: the interior stays clear of the stroke (the
    // "outline only" look of Fill 0 %).
    for fill in [0.0, 0.5] {
        let mut d = fx_doc(80, 80, SampleType::U8);
        let mut l = blob("ring", d.pixel_format(), 40.0, 40.0, 20.0, [0.2, 0.6, 0.9]);
        l.effects.items = vec![Effect::Stroke(stroke(4.0, StrokePosition::Outside, FxPaint::Color(Color::rgb(0.95, 0.85, 0.1))))];
        l.fill_opacity = fill;
        d.layers.push(l);
        fx_check(&mut g, &d, &format!("outside stroke at fill {fill}"));
    }
    // An Outside stroke has no share beneath fully opaque pixels (#5861): non-Normal layers with
    // an Outside stroke on a soft blob and a hard square.
    for blend in [BlendMode::Multiply, BlendMode::Screen] {
        let mut d = fx_doc(80, 80, SampleType::U8);
        let mut l = blob("ring", d.pixel_format(), 40.0, 40.0, 20.0, [0.9, 0.1, 0.1]);
        l.effects.items = vec![Effect::Stroke(stroke(4.0, StrokePosition::Outside, FxPaint::Color(Color::rgb(0.1, 0.2, 0.9))))];
        l.blend = blend;
        d.layers.push(l);
        fx_check(&mut g, &d, &format!("outside stroke under a {blend:?} blob"));
        let mut d = fx_doc(40, 40, SampleType::U8);
        let mut l = Layer::raster("square", d.pixel_format());
        l.surface_mut().unwrap().fill_rect(Rect::new(15, 15, 25, 25), &[1.0, 0.0, 0.0, 1.0]);
        l.effects.items = vec![Effect::Stroke(stroke(3.0, StrokePosition::Outside, FxPaint::Color(Color::rgb(0.0, 0.0, 1.0))))];
        l.blend = blend;
        d.layers.push(l);
        fx_check(&mut g, &d, &format!("outside stroke under a {blend:?} square"));
    }
    // Shapes partly off the canvas (their effects reach back in) and a masked effect layer.
    let mut d = fx_doc(90, 70, SampleType::U8);
    let mut l = blob("edge", d.pixel_format(), 4.0, 66.0, 18.0, [0.9, 0.9, 0.2]);
    l.effects.items = stack.clone();
    l.mask = Some(mask(Rect::new(0, 30, 50, 70), 77, 1.0));
    d.layers.push(l);
    fx_check(&mut g, &d, "off canvas + mask");
    // Disabled master switch / disabled items are ignored.
    let mut d = fx_doc(64, 64, SampleType::U8);
    let mut l = blob("off", d.pixel_format(), 30.0, 30.0, 14.0, [0.9, 0.2, 0.2]);
    l.effects.items = stack;
    if let Effect::DropShadow(s) = &mut l.effects.items[0] {
        s.common.enabled = false;
    }
    d.layers.push(l.clone());
    fx_check(&mut g, &d, "disabled item");
    d.layers[1].effects.enabled = false;
    fx_check(&mut g, &d, "master switch off");
}

#[test]
fn layer_effects_on_groups_clipping_and_fills() {
    let mut g = gpu();
    let ds = Effect::DropShadow(shadow(BlendMode::Multiply, 0.8, 120.0, 5.0, 6.0, 0.0));
    let st = Effect::Stroke(stroke(3.0, StrokePosition::Outside, FxPaint::Color(Color::rgb(1.0, 1.0, 1.0))));
    let bv = Effect::BevelEmboss(bevel(BevelStyle::InnerBevel, true, 5.0, 1.0));
    // Effects on an isolated group and on a pass-through group (rendered isolated).
    for blend in [BlendMode::Normal, BlendMode::PassThrough] {
        let mut d = fx_doc(100, 80, SampleType::U8);
        let a = blob("a", d.pixel_format(), 35.0, 35.0, 16.0, [0.9, 0.3, 0.2]);
        let mut b = blob("b", d.pixel_format(), 62.0, 45.0, 14.0, [0.2, 0.3, 0.9]);
        b.blend = BlendMode::Screen;
        b.effects.items = vec![Effect::InnerGlow(glow(FxPaint::Color(Color::rgb(1.0, 1.0, 0.5)), GlowTechnique::Softer, 4.0, 0.0, GlowSource::Edge))];
        let mut grp = Layer::group("g", vec![a, b]);
        grp.blend = blend;
        grp.opacity = 0.85;
        grp.effects.items = vec![ds.clone(), st.clone()];
        d.layers.push(grp);
        fx_check(&mut g, &d, &format!("group {blend:?}"));
    }
    // An effect layer as a clipping base, and a clipped layer with effects.
    let mut d = fx_doc(100, 80, SampleType::U8);
    let mut base = blob("base", d.pixel_format(), 45.0, 40.0, 24.0, [0.3, 0.8, 0.4]);
    base.effects.items = vec![ds.clone(), bv.clone()];
    let mut c1 = noise_layer("c1", PixelFormat::RGBA8, Rect::new(10, 10, 60, 50), 91, 0.4);
    c1.clipped = true;
    c1.blend = BlendMode::Multiply;
    let mut c2 = blob("c2", d.pixel_format(), 60.0, 50.0, 12.0, [0.9, 0.9, 0.1]);
    c2.clipped = true;
    c2.effects.items = vec![st.clone(), ds.clone()];
    c2.opacity = 0.8;
    d.layers.extend([base, c1, c2]);
    fx_check(&mut g, &d, "clipping with effects");
    // A solid fill layer with a mask and effects; a gradient fill with a stroke.
    let mut d = fx_doc(90, 70, SampleType::U8);
    let mut solid = Layer::new("solid", LayerContent::Fill(Fill::Solid(Color::rgb(0.2, 0.5, 0.8))));
    let mut m = LayerMask::reveal_all();
    m.surface = photocraft_raster::Surface::with_default(PixelFormat::GRAY8, &[0.0]);
    m.surface.fill_rect(Rect::new(20, 15, 60, 50), &[1.0]);
    solid.mask = Some(m);
    solid.effects.items = vec![ds.clone(), st.clone(), bv];
    d.layers.push(solid);
    fx_check(&mut g, &d, "solid fill with effects");
}

#[test]
fn layer_effects_update_incrementally() {
    let mut g = gpu();
    let stack = effect_cases().into_iter().find(|(n, _)| *n == "full stack").unwrap().1;
    let mut d = fx_doc(700, 600, SampleType::U8);
    let mut l = blob("fx", d.pixel_format(), 300.0, 280.0, 200.0, [0.9, 0.4, 0.2]);
    l.effects.items = stack;
    d.layers.push(l);
    d.layers.push(noise_layer("plain", PixelFormat::RGBA8, Rect::new(500, 20, 650, 120), 5, 0.0));
    let first = fx_check(&mut g, &d, "initial");
    assert!(first.fx_programs > 0 && first.fx_shapes == 1);
    // An unrelated edit (another layer's pixels, the effect layer's opacity and an effect colour)
    // reuses every map.
    d.layers[2].surface_mut().unwrap().fill_rect(Rect::new(520, 30, 560, 60), &[0.0, 1.0, 0.0, 1.0]);
    d.layers[1].opacity = 0.8;
    if let Effect::ColorOverlay { color, .. } = &mut d.layers[1].effects.items[6] {
        *color = Color::rgb(0.1, 0.9, 0.9);
    }
    let s = fx_check(&mut g, &d, "unrelated edit");
    assert_eq!((s.fx_shapes, s.fx_programs), (0, 0), "{s:?}");
    // A dab on the effect layer recomputes a neighbourhood of it only.
    d.layers[1].surface_mut().unwrap().fill_rect(Rect::new(290, 270, 330, 300), &[0.1, 0.1, 0.9, 1.0]);
    let s = fx_check(&mut g, &d, "dab on the effect layer");
    assert_eq!(s.fx_shapes, 1);
    assert!(s.fx_pixels < first.fx_pixels / 2, "partial {} vs full {}", s.fx_pixels, first.fx_pixels);
    // Erasing to the layer's edge grows nothing; erasing all of it empties the effects.
    d.layers[1].surface_mut().unwrap().fill_rect(Rect::new(80, 60, 200, 200), &[0.0, 0.0, 0.0, 0.0]);
    fx_check(&mut g, &d, "erase part");
    // A setting change that keeps the effect reach rebuilds that effect only.
    if let Effect::DropShadow(s) = &mut d.layers[1].effects.items[0] {
        s.spread = 0.3;
    }
    let s = fx_check(&mut g, &d, "drop shadow spread");
    assert_eq!(s.fx_programs, 1, "{s:?}");
    // Moving the layer by whole pixels moves its maps with it: nothing is rebuilt.
    let moved = {
        let src = d.layers[1].surface().unwrap();
        let b = src.content_bounds();
        let mut dst = photocraft_raster::Surface::new(src.format());
        dst.write_region(Rect::new(b.x0 + 17, b.y0 - 9, b.x1 + 17, b.y1 - 9), &src.read_region(b));
        dst
    };
    *d.layers[1].surface_mut().unwrap() = moved;
    let s = fx_check(&mut g, &d, "moved");
    assert_eq!(s.fx_programs, 0, "{s:?}");
    // A change of shape with the move rebuilds.
    d.layers[1].surface_mut().unwrap().fill_rect(Rect::new(300, 300, 310, 310), &[1.0, 1.0, 1.0, 0.5]);
    fx_check(&mut g, &d, "moved and painted");
}

#[test]
fn layer_effects_on_shape_layers() {
    let mut g = gpu();
    use photocraft_doc::vector::{Path, ShapeLayer, Subpath};
    for alpha in [1.0, 0.55] {
        let mut d = fx_doc(90, 70, SampleType::U8);
        let path = Path::new(vec![Subpath::polygon(&[(14.3, 12.6), (70.2, 18.1), (60.7, 58.4), (24.9, 50.2)])]);
        let mut fill = Color::rgb(0.3, 0.6, 0.9);
        fill.alpha = alpha;
        let mut sh = ShapeLayer { path, fill: Some(Fill::Solid(fill)), stroke: None, live: None, cache: None, psd_raw: None };
        sh.cache = Some(photocraft_vector::render_shape(&sh, d.pixel_format(), d.bounds()));
        let mut l = Layer::new("shape", LayerContent::Shape(sh));
        l.effects.items = vec![
            Effect::Stroke(stroke(3.0, StrokePosition::Outside, FxPaint::Color(Color::rgb(1.0, 1.0, 1.0)))),
            Effect::Stroke(stroke(4.0, StrokePosition::Center, FxPaint::Gradient(gradient()))),
            Effect::DropShadow(shadow(BlendMode::Multiply, 0.7, 120.0, 4.0, 5.0, 0.0)),
            Effect::BevelEmboss(bevel(BevelStyle::InnerBevel, true, 4.0, 1.0)),
        ];
        d.layers.push(l);
        fx_check(&mut g, &d, &format!("shape layer alpha {alpha}"));
    }
}

#[test]
fn stroke_effects_on_filled_and_stroked_shapes() {
    // compose::effect_outline (a fading gradient fill keeps its strokes along the path), several
    // stroke instances with gradient frames, and a vector stroke above the interior effects with
    // clipped layers and a mask.
    let mut g = gpu();
    use photocraft_doc::vector::{Path, ShapeLayer, ShapeStroke, StrokeAlign, Subpath};
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        for (fill_kind, vector_stroke, masked) in [(0, false, false), (1, false, true), (0, true, false), (1, true, true)] {
            let mut d = fx_doc(90, 70, depth);
            let path = Path::new(vec![Subpath::polygon(&[(14.3, 12.6), (70.2, 18.1), (60.7, 58.4), (24.9, 50.2)])]);
            let mut clear = Color::rgb(0.9, 0.3, 0.1);
            clear.alpha = 0.0;
            let fill = if fill_kind == 0 {
                Fill::Solid(Color::rgb(0.3, 0.6, 0.9))
            } else {
                Fill::gradient(vec![(0.0, Color::rgb(0.9, 0.3, 0.1)), (1.0, clear)], 20.0, 1.0, GradientStyle::Linear, false)
            };
            let stroke_v = vector_stroke.then(|| ShapeStroke {
                width: 3.0,
                align: StrokeAlign::Inside,
                paint: Fill::Solid(Color::rgb(0.1, 0.8, 0.2)),
                ..ShapeStroke::default()
            });
            let mut sh = ShapeLayer { path, fill: Some(fill), stroke: stroke_v, live: None, cache: None, psd_raw: None };
            sh.cache = Some(photocraft_vector::render_shape(&sh, d.pixel_format(), d.bounds()));
            let mut l = Layer::new("shape", LayerContent::Shape(sh));
            if masked {
                l.mask = Some(mask(Rect::new(0, 0, 90, 70), 7, 0.6));
            }
            l.effects.items = vec![
                Effect::Stroke(stroke(2.0, StrokePosition::Outside, FxPaint::Color(Color::rgb(1.0, 1.0, 1.0)))),
                Effect::Stroke(stroke(5.0, StrokePosition::Outside, FxPaint::Gradient(gradient()))),
                Effect::Stroke(stroke(3.0, StrokePosition::Inside, FxPaint::Gradient(gradient()))),
                Effect::ColorOverlay { common: photocraft_doc::FxCommon::new(BlendMode::Multiply, 0.7), color: Color::rgb(0.2, 0.2, 0.9) },
                Effect::DropShadow(shadow(BlendMode::Multiply, 0.7, 120.0, 4.0, 5.0, 0.0)),
            ];
            d.layers.push(l);
            let mut c = noise_layer("clip", d.pixel_format(), Rect::new(30, 0, 60, 70), 9, 0.5);
            c.clipped = true;
            d.layers.push(c);
            fx_check(&mut g, &d, &format!("{depth:?} fill {fill_kind} vector stroke {vector_stroke} masked {masked}"));
        }
    }
}

#[test]
fn noisy_shadows_and_glows_depth_modes_and_pages() {
    let mut g = gpu();
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        for mode in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen] {
            for kind in 0..6 {
                let mut d = fx_doc(280, 70, depth);
                let mut l = blob("noisy", d.pixel_format(), 250.0, 35.0, 22.0, [0.8, 0.3, 0.1]);
                l.opacity = match mode {
                    BlendMode::Normal => 0.4,
                    BlendMode::Multiply => 0.72,
                    _ => 1.0,
                };
                l.fill_opacity = match depth {
                    SampleType::U8 => 0.61,
                    SampleType::U16 => 0.85,
                    SampleType::F32 => 1.0,
                };
                l.blend = mode;
                l.mask = Some(mask(d.bounds(), 75, 1.0));
                let mut s = shadow(mode, 0.8, 130.0, 5.0, 9.0, 0.2);
                s.noise = 0.37;
                if kind == 1 {
                    s.contour = contour();
                }
                let mut glow = glow(
                    FxPaint::Color(Color::rgb(0.9, 0.5, 0.2)),
                    if kind < 4 { GlowTechnique::Softer } else { GlowTechnique::Precise },
                    10.0,
                    0.3,
                    if kind == 3 { GlowSource::Center } else { GlowSource::Edge },
                );
                glow.noise = 0.43;
                if kind == 3 {
                    glow.contour = contour();
                }
                l.effects.items = vec![match kind {
                    0 => Effect::DropShadow(s),
                    1 => Effect::InnerShadow(s),
                    2 | 4 => Effect::OuterGlow(glow),
                    _ => Effect::InnerGlow(glow),
                }];
                d.layers.push(l);
                fx_check(&mut g, &d, &format!("noise {kind} {depth:?} {mode:?}"));
            }
        }
    }
}

#[test]
fn noisy_maps_hash_damage_move_and_setting_changes() {
    let mut g = gpu();
    let mut d = Document::new("noise hash", Size::new(280, 60), ColorMode::Rgb, SampleType::F32);
    let mut l = Layer::raster("hash", d.pixel_format());
    l.surface_mut().unwrap().fill_rect(Rect::new(-5, -5, 275, 55), &[0.2, 0.5, 0.7, 0.5]);
    l.fill_opacity = 0.0;
    let mut s = shadow(BlendMode::Normal, 1.0, 0.0, 0.0, 0.0, 0.0);
    s.contour = Contour::Linear;
    s.noise = 0.5;
    s.knocks_out = false;
    l.effects.items = vec![Effect::DropShadow(s)];
    d.layers.push(l);
    fx_check(&mut g, &d, "hash noise with negative map origin");
    let out = render_to_vec(&mut g.comp, &g.device, &g.queue, &d, d.bounds()).unwrap();
    for y in 0..55 {
        for x in 0..275 {
            let n = photocraft_compose::effects::hash_noise(x, y);
            assert!((out[(y * 280 + x) as usize][3] - n).abs() < 0.001, "noise hash at ({x}, {y})");
        }
    }
    let r = Rect::new(250, 20, 265, 40);
    d.layers[0].surface_mut().unwrap().fill_rect(r, &[0.7, 0.2, 0.1, 0.7]);
    diff_rect(&mut g, &d, r, "noisy map damage").unwrap();
    fx_check(&mut g, &d, "noisy map damage full");
    if let Effect::DropShadow(s) = &mut d.layers[0].effects.items[0] {
        s.noise = 0.8;
    }
    assert_eq!(fx_check(&mut g, &d, "noise amount edited").fx_programs, 1);
    let src = d.layers[0].surface().unwrap();
    let b = src.content_bounds();
    let mut moved = photocraft_raster::Surface::new(src.format());
    moved.write_region(Rect::new(b.x0 + 3, b.y0 + 2, b.x1 + 3, b.y1 + 2), &src.read_region(b));
    *d.layers[0].surface_mut().unwrap() = moved;
    assert!(fx_check(&mut g, &d, "noise after whole-pixel move").fx_programs > 0);
}

// Box widths are clamped to MAX_REACH, so no blur kernel spans more than one 4096-wide LUT row
// (`fx::tests::every_effect_map_lut_fits_one_row`). A size far past the clamp still matches
// the CPU, through the clamped kernel and the clipped apron.
#[test]
fn oversized_blur_is_clamped_like_the_cpu() {
    let mut g = gpu();
    let mut d = fx_doc(270, 16, SampleType::U16);
    let mut l = blob("wide blur", d.pixel_format(), 250.0, 8.0, 4.0, [0.5, 0.2, 0.1]);
    let mut s = shadow(BlendMode::Normal, 0.7, 0.0, 0.0, 2051.0, 0.0);
    s.noise = 0.25;
    l.effects.items = vec![Effect::DropShadow(s)];
    d.layers.push(l);
    let max = photocraft_compose::effects::MAX_REACH;
    assert_eq!(photocraft_compose::effects::tent_kernel(2051.0), photocraft_compose::effects::tent_kernel(max));
    assert!(photocraft_compose::effects::tent_kernel(2051.0).1.len() <= 4096);
    fx_check(&mut g, &d, "clamped blur LUT and clipped apron");
}

#[test]
fn oversized_effect_apron_splits_page_work() {
    let _lock = li_testkit::gpu::lock();
    let li_testkit::gpu::HardwareGpu { adapter, device, queue, .. } = li_testkit::gpu::hardware_device_with("photocraft-gpu apron split", |_| {
        wgpu::DeviceDescriptor { required_limits: wgpu::Limits { max_texture_dimension_2d: 4096, ..Default::default() }, ..Default::default() }
    });
    assert_eq!(Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float, "hardware adapter can't render Rgba32Float");
    let mut comp = Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    let mut d = base_doc(4200, 4);
    let mut l = noise_layer("long shifted shadow", d.pixel_format(), d.bounds(), 391, 0.2);
    l.fill_opacity = 0.5;
    l.effects.items = vec![Effect::DropShadow(shadow(BlendMode::Normal, 0.8, 0.0, 1100.0, 0.0, 0.0))];
    d.layers.push(l);
    // 2048 + 2×1102 exceeds 4096; the split windows still fit and preserve both sides.
    assert!(comp.supports(&d).is_ok());
    let cpu = photocraft_compose::render(&d, d.bounds());
    let (out, stats) = photocraft_gpu::render_to_vec_stats(&mut comp, &device, &queue, &d, d.bounds()).unwrap();
    assert!(stats.chunks > d.size.width.div_ceil(photocraft_gpu::CHUNK) as usize, "page work did not split: {stats:?}");
    assert!(worst_diff(&cpu.px, &out).0 <= TOL);
}

#[test]
fn noisy_blur_damage_keeps_reference_residue_speckles() {
    let mut g = gpu();
    let mut d = fx_doc(300, 90, SampleType::U8);
    let mut l = blob("soft noisy", d.pixel_format(), 245.0, 40.0, 26.0, [0.8, 0.3, 0.2]);
    let mut glow = glow(FxPaint::Color(Color::rgb(0.7, 0.6, 0.3)), GlowTechnique::Softer, 9.0, 0.2, GlowSource::Edge);
    glow.noise = 0.5;
    l.effects.items = vec![Effect::OuterGlow(glow)];
    d.layers.push(l);
    fx_check(&mut g, &d, "soft noise before damage");
    let r = Rect::new(240, 35, 260, 45);
    d.layers[1].surface_mut().unwrap().fill_rect(r, &[0.1, 0.7, 0.4, 0.4]);
    diff_rect(&mut g, &d, r.inflate(12), "soft noise partial damage").unwrap();
    fx_check(&mut g, &d, "soft noise full after damage");
}

#[test]
fn wide_glow_fields_remain_exact_inside_the_reference_region() {
    let mut g = gpu();
    let mut d = fx_doc(800, 540, SampleType::U8);
    let mut l = blob("wide precise", d.pixel_format(), 250.0, 10.0, 8.0, [0.4, 0.2, 0.7]);
    l.effects.items = vec![Effect::OuterGlow(glow(FxPaint::Color(Color::rgb(0.9, 0.7, 0.1)), GlowTechnique::Precise, 1000.0, 0.6, GlowSource::Edge))];
    d.layers.push(l);
    // Corner pixels inside the reference map can be farther from the shape than MAX_REACH.
    fx_check(&mut g, &d, "wide glow/spread fields beyond 512 pixels");
}

#[test]
fn wide_strokes_and_blended_shadow_knockout_match_cpu() {
    let mut g = gpu();
    for blend in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Exclusion] {
        let mut d = base_doc(128, 128);
        let mut layer = Layer::raster("shape", PixelFormat::RGBA8);
        layer.surface_mut().unwrap().fill_rect(Rect::new(50, 50, 78, 78), &[1.0, 0.25, 0.5, 1.0]);
        layer.blend = blend;
        layer.effects.items = vec![
            Effect::Stroke(stroke(40.0, StrokePosition::Outside, FxPaint::Color(Color::BLACK))),
            Effect::Stroke(stroke(4.0, StrokePosition::Outside, FxPaint::Color(Color::WHITE))),
            Effect::DropShadow(shadow(BlendMode::Multiply, 1.0, 90.0, 0.0, 0.0, 0.0)),
        ];
        d.layers.push(layer);
        fx_check(&mut g, &d, &format!("wide stroke and knockout {blend:?}"));
    }
}
