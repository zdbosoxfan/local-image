use super::*;

/// `retain_docs` drops the plans, resident pages and effect maps of documents that aren't open,
/// and keeps the open ones.
#[test]
fn retain_docs_frees_closed_documents() {
    let mut g = gpu();
    let a = base_doc(64, 48);
    let mut b = base_doc(32, 32);
    b.id = photocraft_doc::DocId(a.id.0 + 1);
    check(&mut g, &a, "document a");
    check(&mut g, &b, "document b");
    assert_eq!(g.comp.held_docs(), vec![a.id, b.id]);
    g.comp.retain_docs(|d| d == b.id);
    assert_eq!(g.comp.held_docs(), vec![b.id], "only the open document is left");
    g.comp.retain_docs(|_| false);
    assert!(g.comp.held_docs().is_empty());
    check(&mut g, &a, "renders again after being forgotten");
}

#[test]
fn sparse_mask_defaults_invalidate_warm_resident_pages() {
    let mut g = gpu();
    let mut doc = Document::new("Sparse mask defaults", Size::new(768, 32), ColorMode::Rgb, SampleType::U8);
    let mut pixels = photocraft_raster::Surface::new(doc.pixel_format());
    pixels.fill_rect(doc.bounds(), &[1.0, 0.0, 0.0, 1.0]);
    let mut layer = Layer::new("red", LayerContent::Raster(pixels));
    let sparse = |default: f32| {
        let mut surface = photocraft_raster::Surface::with_default(PixelFormat::GRAY8, &[default]);
        for x in [10, 700] {
            surface.fill_rect(Rect::new(x, 10, x + 1, 11), &[1.0 - default]);
        }
        surface.prune();
        assert_eq!(surface.tile_count(), 2, "the middle tile must remain absent");
        surface
    };
    let original = sparse(1.0);
    let partial = sparse(0.25);
    layer.mask = Some(LayerMask { surface: original.clone(), ..LayerMask::reveal_all() });
    doc.layers.push(layer);
    // Keep both compositors warm across mask inversion, undo, and changes between
    // two nonzero defaults. A nonzero/zero flag alone cannot invalidate correctly.
    for (name, surface) in [
        ("original", original.clone()),
        ("inverted", sparse(0.0)),
        ("undo inversion", original),
        ("partial", partial.clone()),
        ("inverted partial", sparse(0.75)),
        ("undo partial inversion", partial),
    ] {
        doc.layers[0].mask.as_mut().unwrap().surface = surface;
        check(&mut g, &doc, name);
        let cpu = photocraft_compose::render(&doc, doc.bounds());
        for comp in [&mut g.comp, &mut g.paged] {
            let rendered = render_to_vec(comp, &g.device, &g.queue, &doc, doc.bounds()).unwrap();
            let gap = 10 * 768 + 300;
            let (actual, expected) = (rendered[gap][3], cpu.px[gap][3]);
            if expected == 0.0 || expected == 1.0 {
                assert_eq!(actual, expected, "{name}: missing middle tile alpha endpoint");
            } else {
                // The CPU skips density arithmetic at 100%; WGSL evaluates
                // 1 - (1 - v), which can differ by one f32 ULP. The GRAY8 mask
                // must retain exactly the same coverage byte, including the gap.
                assert_eq!((actual * 255.0).round() as u8, (expected * 255.0).round() as u8, "{name}: missing middle tile coverage");
            }
        }
    }
}

// Documents larger than the texture limit

/// A document several pages wide under [`PAGED_LIMIT`]: pixel layers crossing page borders (one
/// masked, one off-canvas), a big effect layer whose region exceeds the limit (maps built per
/// page cell), a type layer with a shadow, an adjustment, a clipped group and a gradient fill.
fn big_doc() -> Document {
    let (w, h) = (1100, 900);
    let mut d = fx_doc(w, h, SampleType::U8);
    // Right over the background: adjustment results round to 8 bits, which would turn sub-LSB
    // float differences of blends beneath into whole steps.
    let mut adj = Layer::new("invert", LayerContent::Adjustment(Adjustment::Invert));
    adj.opacity = 0.6;
    adj.mask = Some(mask(Rect::new(240, 0, 800, 520), 83, 0.0));
    d.layers.push(adj);
    let mut top = noise_layer("top", PixelFormat::RGBA8, Rect::new(-30, 200, 700, 820), 81, 0.0);
    top.blend = BlendMode::Multiply;
    top.mask = Some(mask(Rect::new(100, 240, 600, 700), 82, 1.0));
    d.layers.push(top);
    let stack = effect_cases().into_iter().find(|(n, _)| *n == "full stack").unwrap().1;
    let mut big = blob("big", d.pixel_format(), 520.0, 450.0, 330.0, [0.9, 0.4, 0.2]);
    big.effects.items = stack;
    big.effects.items.push(Effect::OuterGlow(glow(FxPaint::Color(Color::rgb(0.2, 1.0, 0.6)), GlowTechnique::Softer, 40.0, 0.2, GlowSource::Edge)));
    d.layers.push(big);
    let mut text = as_text(blob("type", d.pixel_format(), 900.0, 150.0, 90.0, [0.1, 0.2, 0.9]));
    text.effects.items = vec![Effect::DropShadow(shadow(BlendMode::Multiply, 0.8, 135.0, 12.0, 20.0, 0.0))];
    d.layers.push(text);
    let a = noise_layer("ga", PixelFormat::RGBA8, Rect::new(200, 500, 1000, 880), 84, 0.6);
    let mut c = noise_layer("gc", PixelFormat::RGBA8, Rect::new(0, 600, 1100, 700), 85, 0.0);
    c.clipped = true;
    c.blend = BlendMode::Screen;
    let mut grp = Layer::group("g", vec![a, c]);
    grp.opacity = 0.9;
    d.layers.push(grp);
    let stops = vec![(0.0, Color::rgb(1.0, 0.0, 0.0)), (0.6, Color::rgb(0.0, 1.0, 0.2)), (1.0, Color::rgb(0.1, 0.1, 0.9))];
    let mut grad = Layer::new("grad", LayerContent::Fill(Fill::gradient(stops, 30.0, 0.8, GradientStyle::Radial, false)));
    grad.opacity = 0.3;
    grad.blend = BlendMode::Overlay;
    d.layers.push(grad);
    d
}

#[test]
fn documents_larger_than_the_texture_limit() {
    let _gamma = TEXT_GAMMA.lock().unwrap_or_else(|e| e.into_inner());
    let mut g = gpu();
    let mut d = big_doc();
    assert!(g.paged.supports(&d).is_ok());
    // Every pass sees page borders, and effect maps and blurs crossing them.
    check(&mut g, &d, "big document");
    let s = photocraft_gpu::render_to_vec_stats(&mut g.paged, &g.device, &g.queue, &d, d.bounds()).unwrap().1;
    assert!(s.cells >= 20 && s.chunks == s.cells, "{s:?}");
    // A viewport off the page grid covers only its cells.
    let view = Rect::new(300, 330, 790, 610);
    diff_rect(&mut g, &d, view, "viewport").unwrap_or_else(|e| panic!("{e}"));
    let s = photocraft_gpu::render_to_vec_stats(&mut g.paged, &g.device, &g.queue, &d, view).unwrap().1;
    assert_eq!(s.cells, 6, "{s:?}");
    // A dab across a page corner on the effect layer, and an edit elsewhere.
    d.layers[3].surface_mut().unwrap().fill_rect(Rect::new(500, 490, 530, 530), &[0.1, 0.1, 0.9, 1.0]);
    d.layers[2].surface_mut().unwrap().fill_rect(Rect::new(250, 250, 270, 270), &[0.0, 0.0, 0.0, 0.0]);
    check(&mut g, &d, "after dabs");
    let s = photocraft_gpu::render_to_vec_stats(&mut g.paged, &g.device, &g.queue, &d, d.bounds()).unwrap().1;
    assert_eq!((s.tiles_uploaded, s.fx_shapes), (0, 0), "nothing changed since the last render: {s:?}");
}

#[test]
fn pages_are_evicted_under_the_budget() {
    let _gamma = TEXT_GAMMA.lock().unwrap_or_else(|e| e.into_inner());
    let mut g = gpu();
    let mut d = big_doc();
    check(&mut g, &d, "warm");
    let all = g.paged.resident_bytes();
    // A budget of one page: everything but the cell being drawn is evicted as the render goes.
    let page = u64::from(g.paged.page_size()).pow(2) * 4;
    g.paged.set_resident_budget(page);
    for round in 0..3 {
        let cpu = photocraft_compose::flatten(&d);
        let (out, s) = photocraft_gpu::render_to_vec_stats(&mut g.paged, &g.device, &g.queue, &d, d.bounds()).unwrap();
        let worst = worst_diff(&cpu.px, &out);
        assert!(worst.0 <= TOL, "round {round}: max diff {:.2}/255 at {}", worst.0 * 255.0, worst.1);
        assert!(s.evicted > 0, "round {round}: {s:?}");
        assert!(g.paged.resident_bytes() < all / 4, "{} of {all}", g.paged.resident_bytes());
        // Evicted pages come back with the edits made meanwhile.
        d.layers[2].surface_mut().unwrap().fill_rect(Rect::new(10 + round * 300, 300, 60 + round * 300, 340), &[0.9, 0.9, 0.1, 1.0]);
        d.layers[3].surface_mut().unwrap().fill_rect(Rect::new(200 + round * 200, 420, 240 + round * 200, 470), &[0.0, 0.0, 0.0, 0.0]);
    }
    check(&mut g, &d, "after eviction");
}

#[test]
fn the_focused_view_stays_resident_after_a_full_refresh() {
    let _gamma = TEXT_GAMMA.lock().unwrap_or_else(|e| e.into_inner());
    let g = gpu();
    let d = big_doc();
    let mut comp = Compositor::try_new_with_format(&g.device, wgpu::TextureFormat::Rgba32Float).unwrap();
    comp.set_texture_limit(PAGED_LIMIT);
    let view = Rect::new(520, 300, 760, 500);
    photocraft_gpu::render_to_vec(&mut comp, &g.device, &g.queue, &d, view).unwrap();
    // A budget holding just the view's pages: a full refresh evicts as it goes, and draws the
    // view's cells last, so the next edit in the view uploads nothing.
    // The working set of a region is what rendering it makes resident.
    assert_eq!(comp.working_set(&d, view), comp.resident_bytes());
    assert!(comp.working_set(&d, d.bounds()) > 4 * comp.resident_bytes());
    comp.set_resident_budget(comp.resident_bytes());
    assert!(comp.fits_budget(&d, view) && !comp.fits_budget(&d, d.bounds()));
    comp.set_focus(Some(view));
    for _ in 0..2 {
        let s = photocraft_gpu::render_to_vec_stats(&mut comp, &g.device, &g.queue, &d, d.bounds()).unwrap().1;
        assert!(s.evicted > 0, "{s:?}");
        let (out, s) = photocraft_gpu::render_to_vec_stats(&mut comp, &g.device, &g.queue, &d, view).unwrap();
        assert_eq!(s.tiles_uploaded, 0, "{s:?}");
        let cpu = photocraft_compose::render(&d, view);
        assert!(worst_diff(&cpu.px, &out).0 <= TOL);
    }
}

#[test]
fn cached_plans_rebind_pixel_edits_and_survive_document_switches() {
    let mut g = gpu();
    let mut a = base_doc(64, 64);
    let b = base_doc(64, 64);
    check(&mut g, &a, "cached A");
    check(&mut g, &b, "cached B");
    let n = g.comp.plan_builds();
    for i in 0..3 {
        a.layers[0].surface_mut().unwrap().fill_rect(Rect::new(12, 12, 20, 20), &[0.1 * i as f32, 0.2, 0.8, 1.0]);
        check(&mut g, &a, "pixel edit with cached plan");
        check(&mut g, &b, "other cached document");
        assert_eq!(g.comp.plan_builds(), n);
    }
    a.layers[0].opacity = 0.4;
    check(&mut g, &a, "opacity invalidates plan");
    assert_eq!(g.comp.plan_builds(), n + 1);
    g.comp.forget_doc(a.id);
    check(&mut g, &a, "reopened document");
}

#[test]
fn cached_feathered_mask_updates_match_cpu() {
    let mut g = gpu();
    let mut d = base_doc(64, 64);
    let mut mask = LayerMask::reveal_all();
    mask.feather = 3.0;
    mask.surface.fill_rect(Rect::new(1, 1, 40, 40), &[0.0]);
    d.layers[0].mask = Some(mask);
    check(&mut g, &d, "cached feathered mask");
    let n = g.comp.plan_builds();
    d.layers[0].mask.as_mut().unwrap().surface.fill_rect(Rect::new(5, 5, 10, 10), &[0.5]);
    check(&mut g, &d, "feathered mask pixel edit inside existing bounds");
    assert_eq!(g.comp.plan_builds(), n + 1);
}

#[test]
fn deferred_encode_uniforms_survive_an_intervening_render() {
    let mut g = gpu();
    let a = Document::with_background("deferred", Size::new(64, 64), ColorMode::Rgb, SampleType::U8, Color::rgb(0.2, 0.5, 0.8));
    let b = Document::with_background("submitted first", Size::new(64, 64), ColorMode::Rgb, SampleType::U8, Color::rgb(0.9, 0.1, 0.3));
    let row = 64 * 16;
    let buffer = g.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("deferred readback"),
        size: u64::from(row * 64),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = g.device.create_command_encoder(&Default::default());
    let mut format = wgpu::TextureFormat::Rgba32Float;
    g.comp
        .encode(&g.device, &g.queue, &mut encoder, &a, a.bounds(), &mut |enc, out| {
            assert_eq!(out.rect, a.bounds());
            format = out.texture.format();
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: out.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(64) },
                },
                wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
            );
        })
        .unwrap();
    // render() owns its submit and resets its uniform pool. The earlier encoder has not
    // been submitted yet, so it needs independent uniforms until the caller submits it.
    g.comp.render(&g.device, &g.queue, &b, b.bounds(), |_, _| {}).unwrap();
    g.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    g.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap();
    let actual: [f32; 4] = std::array::from_fn(|i| {
        if format == wgpu::TextureFormat::Rgba32Float {
            f32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap())
        } else {
            half::f16::from_le_bytes(data[i * 2..i * 2 + 2].try_into().unwrap()).to_f32()
        }
    });
    let expected = photocraft_compose::render(&a, a.bounds()).px[0];
    assert!(actual.iter().zip(expected).all(|(a, b)| (a - b).abs() <= TOL), "{actual:?} != {expected:?}");
}

#[test]
fn replaced_resident_views_do_not_accumulate_binding_owners() {
    let mut g = gpu();
    let mut d = Document::new("resident replacement", Size::new(512, 512), ColorMode::Rgb, SampleType::U8);
    d.layers.push(Layer::raster("changing page", PixelFormat::RGBA8));
    let mut baseline = None;
    for step in 0..16 {
        // Same layer/page identity; alternate both upload region and storage kind well below
        // the budget. Merely checking resident_bytes cannot detect pinned obsolete views.
        let fmt = if step % 2 == 0 { PixelFormat::RGBA8 } else { PixelFormat::RGBA32F };
        let edge = if step % 4 < 2 { 64 } else { 320 };
        let mut surface = photocraft_raster::Surface::new(fmt);
        surface.fill_rect(Rect::new(0, 0, edge, edge), &[0.2, 0.5, 0.7, 0.8]);
        *d.layers[0].surface_mut().unwrap() = surface;
        let cpu = photocraft_compose::flatten(&d);
        let (out, stats) = photocraft_gpu::render_to_vec_stats(&mut g.comp, &g.device, &g.queue, &d, d.bounds()).unwrap();
        assert_eq!(stats.evicted, 0, "replacement must be tested without budget eviction");
        assert!(worst_diff(&cpu.px, &out).0 <= TOL, "replacement parity at {step}");
        let entries = g.comp.texture_binding_cache_entries();
        let expected = *baseline.get_or_insert(entries);
        assert!(expected > 0);
        assert_eq!(entries, expected, "old texture bindings retained at replacement {step}");
        let (_, unchanged) = photocraft_gpu::render_to_vec_stats(&mut g.comp, &g.device, &g.queue, &d, d.bounds()).unwrap();
        assert_eq!(unchanged.tiles_uploaded, 0, "unchanged resident should be reused");
        assert_eq!(g.comp.texture_binding_cache_entries(), entries);
    }
}

#[test]
fn removed_layers_release_cached_texture_bindings_under_budget() {
    let mut g = gpu();
    let mut d = base_doc(64, 64);
    for step in 0..8 {
        check(&mut g, &d, "layer before removal");
        assert!(g.comp.texture_binding_cache_entries() > 0);
        d.layers.clear();
        check(&mut g, &d, "empty after removal");
        assert_eq!(g.comp.resident_bytes(), 0);
        assert_eq!(g.comp.texture_binding_cache_entries(), 0, "bindings survived removal {step}");
        assert_eq!(g.paged.texture_binding_cache_entries(), 0);
        d.layers.push(noise_layer("new layer", PixelFormat::RGBA8, d.bounds(), step + 1, 0.5));
    }
}

#[test]
fn effect_and_pattern_replacement_do_not_accumulate_binding_owners() {
    let mut g = gpu();
    let mut d = fx_doc(96, 96, SampleType::U8);
    d.layers.push(blob("effect", d.pixel_format(), 48.0, 48.0, 18.0, [0.8, 0.3, 0.1]));
    let mut ceiling = 0;
    for step in 0..12 {
        d.layers[1].effects.items = vec![
            Effect::DropShadow(shadow(BlendMode::Multiply, 0.75, 120.0, 3.0, 4.0 + (step % 2) as f32, 0.1 * (step % 3) as f32)),
            Effect::DropShadow(shadow(BlendMode::Normal, 0.4, 240.0, 2.0, 3.0, 0.0)),
        ];
        if step % 3 == 2 {
            d.layers[1].effects.items.pop();
        }
        fx_check(&mut g, &d, "replaced effect maps");
        let entries = g.comp.texture_binding_cache_entries();
        if step < 2 {
            ceiling = ceiling.max(entries);
        }
        assert!(entries <= ceiling, "stale effect-map bindings at {step}: {entries} > {ceiling}");
    }
    g.comp.forget_doc(d.id);
    g.paged.forget_doc(d.id);
    let mut d = base_doc(64, 64);
    let pat = checker_pattern();
    d.layers.push(Layer::new(
        "pattern",
        LayerContent::Fill(Fill::Pattern { name: pat.name.clone(), id: pat.id.clone(), scale: 1.0, angle: 0.0, link: true, phase: (0.0, 0.0) }),
    ));
    d.patterns.push(pat);
    let mut baseline = None;
    for step in 0..12 {
        if step < 6 {
            d.patterns[0].surface.fill_rect(Rect::new(0, 0, 2, 2), &[step as f32 / 12.0, 0.2, 0.7, 1.0]);
        } else {
            // Untiled defaults have no tile pointer to fingerprint, either.
            d.patterns[0].surface = photocraft_raster::Surface::with_default(PixelFormat::RGBA8, &[step as f32 / 12.0, 0.7, 0.2, 1.0]);
        }
        check(&mut g, &d, "replaced pattern texture");
        let entries = g.comp.texture_binding_cache_entries();
        assert_eq!(entries, *baseline.get_or_insert(entries), "stale pattern bindings at {step}");
    }
}

#[test]
fn closing_pattern_documents_releases_primary_cache() {
    let mut g = gpu();
    for step in 0..8 {
        let mut d = base_doc(64, 64);
        let mut pat = checker_pattern();
        pat.id = format!("document pattern {step}");
        d.layers.push(Layer::new(
            "pattern",
            LayerContent::Fill(Fill::Pattern { name: pat.name.clone(), id: pat.id.clone(), scale: 1.0, angle: 0.0, link: true, phase: (0.0, 0.0) }),
        ));
        d.patterns.push(pat);
        check(&mut g, &d, "pattern before close");
        assert!(g.comp.pattern_cache_bytes() > 0);
        g.comp.forget_doc(d.id);
        g.paged.forget_doc(d.id);
        assert_eq!(g.comp.pattern_cache_bytes(), 0, "pattern leaked after document {step}");
        assert_eq!(g.paged.pattern_cache_bytes(), 0);
    }
}

fn pattern_budget_doc(label: &str, count: u32) -> Document {
    let mut d = base_doc(64, 64);
    for i in 0..count {
        let mut pat = checker_pattern();
        pat.id = format!("{label} {i}");
        pat.surface.fill_rect(Rect::new(0, 0, 2, 2), &[i as f32 / 5.0, 0.3, 0.6, 1.0]);
        let mut layer = Layer::new(
            "pattern",
            LayerContent::Fill(Fill::Pattern { name: pat.name.clone(), id: pat.id.clone(), scale: 1.0, angle: 0.0, link: true, phase: (i as f32, 0.0) }),
        );
        layer.opacity = 0.5;
        d.layers.push(layer);
        d.patterns.push(pat);
    }
    d
}

#[test]
fn pattern_cache_budget_evicts_old_documents_and_preserves_warm_patterns() {
    let mut g = gpu();
    let one = 6 * 5 * 16;
    let budget = 3 * one;
    g.comp.set_memory_budget(budget * 8);
    g.paged.set_memory_budget(budget * 8);
    let a = pattern_budget_doc("A", 2);
    let b = pattern_budget_doc("B", 2);
    for step in 0..8 {
        let doc = if step % 2 == 0 { &a } else { &b };
        check(&mut g, doc, "pattern LRU under budget");
        assert!(g.comp.pattern_cache_bytes() <= budget);
        assert!(g.paged.pattern_cache_bytes() <= budget);
        let uploads = g.comp.pattern_uploads();
        check(&mut g, doc, "unchanged current patterns");
        assert_eq!(g.comp.pattern_uploads(), uploads, "current working set should remain warm");
    }
    assert!(g.comp.pattern_uploads() > 4, "fixture must force actual eviction/reupload");
    g.comp.forget_doc(a.id);
    g.comp.forget_doc(b.id);
    assert_eq!(g.comp.pattern_cache_bytes(), 0);
    assert_eq!(g.comp.texture_binding_cache_entries(), 0);
}

#[test]
fn pattern_working_set_refusal_precedes_allocation_and_budget_changes_retry() {
    let mut g = gpu();
    let d = pattern_budget_doc("over budget", 4);
    let one = 6 * 5 * 16;
    g.comp.set_memory_budget(3 * one * 8);
    let err = g.comp.supports(&d).unwrap_err();
    assert!(err.to_string().contains("pattern budget"), "{err}");
    assert!(photocraft_gpu::render_to_vec(&mut g.comp, &g.device, &g.queue, &d, d.bounds()).is_err());
    assert_eq!((g.comp.pattern_cache_bytes(), g.comp.pattern_uploads()), (0, 0));
    assert!(!photocraft_compose::flatten(&d).px.is_empty(), "CPU fallback remains available");
    g.comp.set_memory_budget(4 * one * 8);
    assert!(g.comp.supports(&d).is_ok(), "budget-dependent refusals must be invalidated");
    check(&mut g, &d, "admitted after budget increase");
    assert_eq!(g.comp.pattern_cache_bytes(), 4 * one);
    g.comp.set_memory_budget(one * 8);
    assert!(g.comp.pattern_cache_bytes() <= one, "shrinking a budget evicts immediately");
    assert!(g.comp.supports(&d).is_err());
    g.comp.set_texture_limit(256);
    assert_eq!(g.comp.pattern_cache_bytes(), 0);
}

#[test]
fn effect_only_pattern_inputs_obey_working_set_budget() {
    let mut g = gpu();
    let mut d = fx_doc(64, 64, SampleType::U8);
    let mut layer = blob("pattern bevel", d.pixel_format(), 32.0, 32.0, 12.0, [0.7, 0.3, 0.1]);
    layer.effects.items = effect_cases().into_iter().find(|(name, _)| *name == "bevel texture").unwrap().1;
    d.layers.push(layer);
    d.patterns.push(checker_pattern());
    g.comp.set_memory_budget(8);
    let err = g.comp.supports(&d).unwrap_err();
    assert!(err.to_string().contains("pattern budget"), "{err}");
    assert_eq!(g.comp.pattern_uploads(), 0);
    g.comp.set_memory_budget(8 << 20);
    check(&mut g, &d, "admitted effect pattern");
    assert!(g.comp.pattern_cache_bytes() > 0);
}

#[test]
fn pattern_context_tracks_document_cmyk_profile() {
    let mut g = gpu();
    let mut d = Document::new("CMYK pattern profile", Size::new(64, 64), ColorMode::Cmyk, SampleType::U8);
    let surface = photocraft_raster::Surface::with_default(PixelFormat::new(ColorMode::Cmyk, SampleType::U8, true), &[0.0, 0.5, 0.5, 0.0, 1.0]);
    let pat = Pattern::new("CMYK", surface, 6, 5);
    d.layers.push(Layer::new(
        "pattern",
        LayerContent::Fill(Fill::Pattern { name: pat.name.clone(), id: pat.id.clone(), scale: 1.0, angle: 0.0, link: true, phase: (0.0, 0.0) }),
    ));
    d.patterns.push(pat);
    let profile = photocraft_cms::synth::cmyk_profile(&photocraft_cms::synth::CmykParams {
        description: "Pattern uncoated fixture".into(),
        tvi: [0.26, 0.26, 0.26, 0.3],
        grid_a2b: 5,
        grid_b2a: 9,
        ..Default::default()
    })
    .to_bytes();
    let initial = photocraft_compose::flatten(&d).px;
    check(&mut g, &d, "default CMYK pattern");
    d.icc_profile = Some(profile);
    let changed = photocraft_compose::flatten(&d).px;
    assert!(worst_diff(&initial, &changed).0 > 0.01, "profile fixture must materially change appearance");
    check(&mut g, &d, "same pattern in assigned CMYK profile");
    d.icc_profile = None;
    check(&mut g, &d, "same pattern restored to default CMYK");
}

#[test]
fn pattern_context_pixel_changes_rebuild_bevel_program() {
    let mut g = gpu();
    let mut d = fx_doc(96, 96, SampleType::U8);
    let mut layer = blob("textured bevel", d.pixel_format(), 48.0, 48.0, 24.0, [0.7, 0.3, 0.1]);
    layer.effects.items = effect_cases().into_iter().find(|(name, _)| *name == "bevel texture").unwrap().1;
    d.layers.push(layer);
    // Identical name/id/dimensions and the same tile allocation after an in-place edit.
    d.patterns = vec![checker_pattern()];
    fx_check(&mut g, &d, "first bevel texture");
    let before = photocraft_compose::flatten(&d).px;
    d.patterns[0].surface.fill_rect(Rect::new(0, 0, 3, 3), &[0.95, 0.95, 0.95, 1.0]);
    let after = photocraft_compose::flatten(&d).px;
    assert!(worst_diff(&before, &after).0 > TOL, "fixture must visibly change bevel lighting");
    let stats = fx_check(&mut g, &d, "changed bevel texture");
    assert!(stats.fx_programs > 0, "pattern-only change must rebuild its effect maps");
}
