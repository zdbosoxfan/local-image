//! The GPU canvas on a realistic photo-editing document: a full-canvas photo, layers in several
//! blend modes (one masked), adjustment layers (one masked, one clipped), a type layer with a drop
//! shadow, a shape and a pass-through group. The wgpu compositor (not the CPU fallback) must draw
//! it, matching the CPU compositor. Skips when there is no GPU adapter.
//!
//! `bench_gpu_canvas_24mp` (ignored) times the canvas paths that cause stutter on a 6000×4000
//! version, on a device created the way the app creates it:
//!
//! ```sh
//! cargo test --release -p photocraft-ui-egui --test gpu_canvas_perf -- --ignored --nocapture
//! ```

use std::time::Instant;

use eframe::egui_wgpu::RenderState;
use photocraft_color::{BlendMode, Color, ColorMode, SampleType};
use photocraft_doc::{Document, Effect, Layer, LayerId, LayerMask, Size};
use photocraft_engine::Session;
use photocraft_geom::Rect;
use photocraft_ui_egui::gpu_canvas::{self, GpuCanvas};
use serde_json::{Value, json};

/// Concurrent wgpu devices in one process crash on some drivers (see `canvas_16f.rs`).
use crate::gpu_lock;

fn render_state() -> Option<RenderState> {
    let rs =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| egui_kittest::wgpu::create_render_state(gpu_canvas::wgpu_setup(), Default::default()))).ok();
    if let Some(rs) = &rs {
        eprintln!("test adapter: {:?}", rs.adapter.get_info());
    } else {
        eprintln!("skipping: no GPU adapter");
    }
    rs
}

/// Deterministic pseudo-random in [0, 1).
fn rnd(k: u64) -> f32 {
    let mut x = k.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// A photo-ish layer: gradients and noise in `block`-pixel squares over `r`.
fn photo(name: &str, doc: &Document, r: Rect, seed: u64, block: i32) -> Layer {
    let mut l = Layer::raster(name, doc.pixel_format());
    let s = l.surface_mut().expect("raster");
    let base = [rnd(seed), rnd(seed + 1), rnd(seed + 2)];
    let mut y = r.y0;
    while y < r.y1 {
        let mut x = r.x0;
        while x < r.x1 {
            let fx = (x - r.x0) as f32 / r.width().max(1) as f32;
            let fy = (y - r.y0) as f32 / r.height().max(1) as f32;
            let n = rnd(seed ^ ((x as u64) << 20) ^ y as u64) * 0.12;
            let px = [(base[0] * 0.6 + 0.4 * fx + n).min(1.0), (base[1] * 0.5 + 0.5 * fy + n).min(1.0), (base[2] * 0.7 + 0.3 * (1.0 - fx) + n).min(1.0), 1.0];
            s.fill_rect(Rect::new(x, y, (x + block).min(r.x1), (y + block).min(r.y1)), &px);
            x += block;
        }
        y += block;
    }
    l
}

fn exec(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn layer_of(v: &Value) -> LayerId {
    LayerId(v.get("layer").or_else(|| v.get("id")).and_then(Value::as_u64).expect("layer id in result"))
}

fn insert(s: &mut Session, l: Layer) -> LayerId {
    let st = s.active_mut().expect("doc");
    let mut doc = (*st.doc).clone();
    let id = doc.insert_above(None, l);
    st.doc = std::sync::Arc::new(doc);
    exec(s, "layer.select", json!({"layer": id.0}));
    id
}

/// The realistic document, `w`×`h` 8-bit RGB, in a session (for brush strokes), and the id of the
/// layer to paint on and of the Curves layer to tweak.
fn realistic(w: u32, h: u32) -> (Session, LayerId, LayerId) {
    let (wi, hi) = (w as i32, h as i32);
    let block = (w as i32 / 750).max(2);
    let doc = Document::with_background("Realistic", Size::new(w, h), ColorMode::Rgb, SampleType::U8, Color::rgb(0.5, 0.5, 0.5));
    let mut s = Session::new();
    s.add_document(doc.clone(), None);
    // 1. A full-canvas photo.
    insert(&mut s, photo("Photo", &doc, doc.bounds(), 1, block));
    // 2. A multiply texture, masked with a vertical ramp.
    let mut tex = photo("Texture", &doc, doc.bounds(), 2, block * 3);
    tex.blend = BlendMode::Multiply;
    tex.opacity = 0.7;
    let mut m = LayerMask::reveal_all();
    for i in 0..16 {
        let y0 = hi * i / 16;
        m.surface.fill_rect(Rect::new(0, y0, wi, hi * (i + 1) / 16), &[i as f32 / 15.0]);
    }
    tex.mask = Some(m);
    insert(&mut s, tex);
    // 3. A screen light leak and 4. an overlay vignette patch.
    let mut leak = photo("Light leak", &doc, Rect::new(wi / 2, 0, wi, hi / 2), 3, block * 4);
    leak.blend = BlendMode::Screen;
    leak.opacity = 0.5;
    insert(&mut s, leak);
    let mut patch = photo("Overlay patch", &doc, Rect::new(wi / 8, hi / 3, wi * 5 / 8, hi * 5 / 6), 4, block * 2);
    patch.blend = BlendMode::Overlay;
    let paint = insert(&mut s, patch);
    // 5. Curves (global) and 6. Hue/Saturation with a mask.
    let curves = layer_of(&exec(&mut s, "layer.newAdjustmentLayer.curves", json!({"points": [[0, 0], [90, 70], [190, 205], [255, 255]]})));
    let hue = layer_of(&exec(&mut s, "layer.newAdjustmentLayer.hueSaturation", json!({"saturation": -25, "hue": 8})));
    {
        let st = s.active_mut().expect("doc");
        let mut d = (*st.doc).clone();
        let mut m = LayerMask::reveal_all();
        m.surface.fill_rect(Rect::new(0, 0, wi / 3, hi), &[0.0]);
        d.layer_mut(hue).expect("hue layer").mask = Some(m);
        st.doc = std::sync::Arc::new(d);
    }
    // 7. A type layer with a drop shadow; 8. a shape.
    let text = layer_of(&exec(
        &mut s,
        "type.create",
        json!({"x": wi / 10, "y": hi / 5, "text": "Golden hour", "size": (h as f32 / 12.0).max(8.0), "color": "#fff4e0"}),
    ));
    {
        let st = s.active_mut().expect("doc");
        let mut d = (*st.doc).clone();
        let l = d.layer_mut(text).expect("text layer");
        let Effect::DropShadow(mut ds) = Effect::default_drop_shadow() else { unreachable!("drop shadow") };
        ds.distance = h as f32 / 400.0;
        ds.size = h as f32 / 200.0;
        l.effects.items = vec![Effect::DropShadow(ds)];
        st.doc = std::sync::Arc::new(d);
    }
    exec(&mut s, "shape.create", json!({"kind": "ellipse", "rect": [wi * 3 / 4, hi * 3 / 5, wi / 6, wi / 6], "fill": "#3060c0", "name": "Badge"}));
    // 9. A pass-through group with a soft-light layer and a clipped Levels.
    {
        let st = s.active_mut().expect("doc");
        let mut d = (*st.doc).clone();
        let mut soft = photo("Soft light", &d, Rect::new(0, hi / 2, wi, hi), 5, block * 5);
        soft.blend = BlendMode::SoftLight;
        let group = Layer::group("Grade", vec![soft]);
        d.insert_above(None, group);
        st.doc = std::sync::Arc::new(d);
    }
    // 10. Levels on top.
    exec(&mut s, "layer.newAdjustmentLayer.levels", json!({"inBlack": 6, "inWhite": 248, "gamma": 1.08}));
    exec(&mut s, "layer.select", json!({"layer": paint.0}));
    (s, paint, curves)
}

/// Largest per-channel difference between the canvas texels and the CPU composite (premultiplied).
fn max_error(doc: &Document, texels: &[[f32; 4]]) -> f32 {
    let buf = photocraft_compose::render(doc, doc.bounds());
    buf.px.iter().zip(texels).map(|(c, t)| (0..4).map(|i| ((if i < 3 { c[i] * c[3] } else { c[3] }) - t[i]).abs()).fold(0.0, f32::max)).fold(0.0, f32::max)
}

fn layer_count(doc: &Document) -> usize {
    doc.walk().len()
}

#[test]
fn realistic_document_composites_on_the_gpu() {
    let _gpu = gpu_lock();
    let Some(rs) = render_state() else { return };
    let g = GpuCanvas::new(&rs);
    let (s, _, _) = realistic(900, 600);
    let doc = s.active().expect("doc").doc.clone();
    assert!(layer_count(&doc) >= 10, "{} layers", layer_count(&doc));
    assert!(g.supports(&doc), "the wgpu compositor refuses the document");
    let r = g.refresh(doc.id.0, &doc, None, None);
    assert_eq!(r.kind, "gpu-full", "fallback: {:?}", r.fallback);
    assert!(r.fallback.is_none());
    let (_, size, texels) = g.read_texels(doc.id.0).expect("read back");
    assert_eq!(size, [900, 600]);
    let e = max_error(&doc, &texels);
    assert!(e <= 3.0 / 255.0, "GPU canvas differs from the CPU composite by {e}");
    // A damage rect stays on the GPU too.
    let d = g.refresh(doc.id.0, &doc, Some(Rect::new(100, 100, 228, 228)), None);
    assert_eq!(d.kind, "gpu-rect", "fallback: {:?}", d.fallback);
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Times (ms) of the canvas paths on a 6000×4000 realistic document. Run in release.
#[test]
#[ignore = "benchmark: run with --release -- --ignored --nocapture"]
fn bench_gpu_canvas_24mp() {
    let _gpu = gpu_lock();
    let Some(rs) = render_state() else { return };
    let info = rs.adapter.get_info();
    eprintln!("adapter: {} ({:?}, {:?}, driver {} {})", info.name, info.backend, info.device_type, info.driver, info.driver_info);
    let g = GpuCanvas::new(&rs);
    let t = Instant::now();
    let (mut s, paint, curves) = realistic(6000, 4000);
    eprintln!("build document: {:.0} ms", ms(t));
    let doc = s.active().expect("doc").doc.clone();
    let key = doc.id.0;
    let wait = |g: &GpuCanvas| {
        g.health().wait(&rs.device, None);
    };
    let row = |name: &str, v: &[f64]| {
        let mut s = v.to_vec();
        s.sort_by(f64::total_cmp);
        eprintln!("{name:<44} median {:>8.2} ms   min {:>8.2}   max {:>8.2}   (n={})", median(v.to_vec()), s[0], s[s.len() - 1], v.len());
    };
    eprintln!("layers: {}, supported by the wgpu compositor: {}", layer_count(&doc), g.supports(&doc));

    // Cold full refresh (every layer page uploaded).
    let t = Instant::now();
    let r = g.refresh(key, &doc, None, None);
    let cpu = ms(t);
    wait(&g);
    eprintln!("cold full refresh: {:.1} ms CPU, {:.1} ms with GPU; kind {} fallback {:?}", cpu, ms(t), r.kind, r.fallback);
    assert_eq!(r.kind, "gpu-full", "fallback: {:?}", r.fallback);

    // Warm full refresh (an adjustment tweak): each a different Curves point.
    let (mut cpu_v, mut gpu_v) = (Vec::new(), Vec::new());
    for i in 0..9 {
        exec(&mut s, "layer.setAdjustment", json!({"layer": curves.0, "points": [[0, 0], [90, 70 + i], [190, 205], [255, 255]]}));
        let d = s.active().expect("doc").doc.clone();
        let t = Instant::now();
        let r = g.refresh(key, &d, None, None);
        cpu_v.push(ms(t));
        wait(&g);
        gpu_v.push(ms(t));
        assert_eq!(r.kind, "gpu-full", "fallback: {:?}", r.fallback);
    }
    row("adjustment tweak: full refresh (CPU side)", &cpu_v);
    row("adjustment tweak: full refresh (+GPU)", &gpu_v);

    // Brush dab: a 96×96 change on a layer, refreshed over its damage rect.
    let (mut cpu_v, mut gpu_v, mut plan_v) = (Vec::new(), Vec::new(), Vec::new());
    let mut d = (*s.active().expect("doc").doc).clone();
    for i in 0..40 {
        let (x, y) = (1000 + i * 90, 1500 + (i % 7) * 60);
        let r = Rect::new(x, y, x + 96, y + 96);
        d.layer_mut(paint).and_then(|l| l.surface_mut()).expect("paint layer").fill_rect(r, &[0.9, 0.2, 0.1, 0.6]);
        let t = Instant::now();
        let p = photocraft_gpu::plan(&d).map(|p| p.passes.len());
        plan_v.push(ms(t));
        assert!(p.is_ok());
        let t = Instant::now();
        let out = g.refresh(key, &d, Some(r), None);
        cpu_v.push(ms(t));
        wait(&g);
        gpu_v.push(ms(t));
        assert_eq!(out.kind, "gpu-rect", "fallback: {:?}", out.fallback);
    }
    row("plan(doc) alone", &plan_v);
    row("brush dab 96²: rect refresh (CPU side)", &cpu_v);
    row("brush dab 96²: rect refresh (+GPU)", &gpu_v);

    // The CPU fallback for comparison: the same dab composited and uploaded by the CPU.
    let mut v = Vec::new();
    for i in 0..5 {
        let r = Rect::new(1000 + i * 90, 1500, 1096 + i * 90, 1596);
        let t = Instant::now();
        let buf = photocraft_compose::render(&d, r);
        std::hint::black_box(&buf);
        v.push(ms(t));
    }
    row("CPU compositor: same dab rect", &v);

    // Same 24 MP canvas with all three formerly unsupported features together.
    let featured = former_fallback_document(d.clone());
    assert!(g.supports(&featured));
    let mut cpu = Vec::new();
    let mut gpu = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        std::hint::black_box(photocraft_compose::render(&featured, featured.bounds()));
        cpu.push(ms(t));
        let t = Instant::now();
        let refreshed = g.refresh(key, &featured, None, None);
        wait(&g);
        gpu.push(ms(t));
        assert_eq!(refreshed.kind, "gpu-full", "{:?}", refreshed.fallback);
        assert!(refreshed.fallback.is_none());
    }
    row("Blend If + noisy glow + Lab: CPU full", &cpu);
    row("Blend If + noisy glow + Lab: GPU full", &gpu);

    // Live brush stroke (engine, UI thread): begin, pointer moves, commit.
    let brush = json!({"size": 60.0, "hardness": 0.6, "spacing": 0.15});
    exec(&mut s, "tools.setBrush", brush);
    let pts: Vec<Vec<f64>> = (0..400).map(|i| vec![500.0 + i as f64 * 12.0, 2000.0 + (i as f64 * 0.05).sin() * 600.0, 0.8]).collect();
    let params = |n: usize, seed: Option<u64>| {
        let mut p = json!({"points": pts[..n], "zoom": 0.25, "freehand": true});
        if let Some(seed) = seed {
            p["seed"] = json!(seed);
        }
        p
    };
    let t = Instant::now();
    let mut live = photocraft_engine::brush_cmds::LiveStroke::begin(&s, &params(1, None)).expect("live stroke");
    let begin = ms(t);
    let mut push_v = Vec::new();
    for chunk in pts[1..].chunks(4) {
        let sp: Vec<_> = chunk.iter().map(|p| photocraft_engine::paint::StrokePoint::new(p[0], p[1], p[2] as f32)).collect();
        let t = Instant::now();
        live.push(&sp).expect("push");
        push_v.push(ms(t));
    }
    eprintln!("live stroke begin: {begin:.2} ms");
    row("live stroke push (4 points)", &push_v);
    let seed = live.seed;
    let t = Instant::now();
    s.prepare_live_commit(live);
    exec(&mut s, "paint.stroke", params(pts.len(), Some(seed)));
    let reuse = ms(t);
    assert_eq!(s.live_commits_reused(), 1, "the commit rendered the stroke again");
    exec(&mut s, "edit.undo", json!({}));
    let t = Instant::now();
    exec(&mut s, "paint.stroke", params(pts.len(), Some(seed)));
    eprintln!("stroke commit (mouse-up), 400 points: {reuse:.1} ms reusing the live stroke, {:.1} ms rendering it again", ms(t));

    // Layers panel: thumbnail fingerprints of every layer (each frame the panel shows).
    let d = s.active().expect("doc").doc.clone();
    let mut v = Vec::new();
    for _ in 0..20 {
        let t = Instant::now();
        let h = d.walk().iter().filter_map(|(_, _, l)| l.surface()).fold(0u64, |a, s| a ^ photocraft_ui_egui::surface_fingerprint(s));
        std::hint::black_box(h);
        v.push(ms(t));
    }
    row("layer thumbnail fingerprints (all layers)", &v);

    // Navigator thumbnail (CPU, after every commit while the Navigator panel shows).
    let d = s.active().expect("doc").doc.clone();
    let t = Instant::now();
    let thumb = photocraft_compose::thumbnail_buffer(&d, 512);
    eprintln!("navigator thumbnail_buffer: {:.1} ms ({}×{})", ms(t), thumb.rect.width(), thumb.rect.height());
}

/// CPU-only before/after comparisons. The before paths are the original implementations:
/// plan each refresh, hash all tile pointers each frame, replay the whole stroke at mouse-up.
#[test]
#[ignore = "CPU stutter benchmark; run with --ignored --nocapture"]
fn bench_cpu_stutter() {
    let row = |name: &str, v: &[f64]| {
        let mut sorted = v.to_vec();
        sorted.sort_by(f64::total_cmp);
        eprintln!("{name}: median {:.6} ms; min {:.6}; max {:.6}; n={}", median(v.to_vec()), sorted[0], sorted[sorted.len() - 1], v.len());
    };
    let (mut s, paint, _) = realistic(1800, 1200);
    let d = s.active().unwrap().doc.clone();
    let mut cache = photocraft_gpu::plan_cache::PlanCache::default();
    cache.get(&d, |_| Ok(())).unwrap();
    let mut before = Vec::new();
    let mut after = Vec::new();
    for _ in 0..100 {
        let t = Instant::now();
        std::hint::black_box(photocraft_gpu::plan(&d).unwrap());
        before.push(ms(t));
        let t = Instant::now();
        std::hint::black_box(cache.get(&d, |_| Ok(())).unwrap());
        after.push(ms(t));
    }
    assert_eq!(cache.builds, 1);
    row("plan before (uncached)", &before);
    row("plan after (structure cached)", &after);
    let surfaces: Vec<_> = d.walk().into_iter().filter_map(|(_, _, l)| l.surface()).collect();
    before.clear();
    after.clear();
    for _ in 0..1000 {
        let t = Instant::now();
        let mut h = 0u64;
        for surf in &surfaces {
            let mut v = 0xcbf2_9ce4_8422_2325 ^ surf.tile_count() as u64;
            for (c, tile) in surf.tiles() {
                let ptr = std::sync::Arc::as_ptr(tile) as usize as u64;
                v = (v ^ ptr ^ ((c.tx as u64) << 32 | c.ty as u32 as u64)).wrapping_mul(0x100_0000_01b3);
            }
            h ^= v;
        }
        std::hint::black_box(h);
        before.push(ms(t));
        let t = Instant::now();
        std::hint::black_box(surfaces.iter().fold(0u64, |h, s| h ^ photocraft_ui_egui::surface_fingerprint(s)));
        after.push(ms(t));
    }
    row("thumbnail fingerprint before (all tile pointers)", &before);
    row("thumbnail fingerprint after (surface revisions)", &after);

    let pts: Vec<[f64; 3]> = (0..400).map(|i| [100.0 + i as f64 * 4.0, 600.0 + (i as f64 * 0.06).sin() * 240.0, 0.8]).collect();
    let mut p = json!({"points": [pts[0]], "size": 24, "hardness": 1.0, "freehand": true, "seed": 123});
    let mut live = photocraft_engine::brush_cmds::LiveStroke::begin(&s, &p).unwrap();
    let ptr = std::sync::Arc::as_ptr(&live.doc);
    let mut pushes = Vec::new();
    for chunk in pts[1..].chunks(4) {
        let t = Instant::now();
        live.push(&chunk.iter().map(|p| photocraft_engine::paint::StrokePoint::new(p[0], p[1], p[2] as f32)).collect::<Vec<_>>()).unwrap();
        pushes.push(ms(t));
        assert_eq!(ptr, std::sync::Arc::as_ptr(&live.doc), "document cloned during a push");
    }
    row("live stroke push (4 points, no document clone)", &pushes);
    p["points"] = json!(pts);
    let t = Instant::now();
    s.prepare_live_commit(live);
    s.execute("paint.stroke", p.clone()).unwrap();
    let reuse = ms(t);
    assert_eq!(s.live_commits_reused(), 1);
    let result = s.active().unwrap().doc.clone();
    s.execute("edit.undo", json!({})).unwrap();
    s.execute("layer.select", json!({"layer": paint.0})).unwrap();
    let t = Instant::now();
    s.execute("paint.stroke", p).unwrap();
    let replay = ms(t);
    assert_eq!(result.layers, s.active().unwrap().doc.layers);
    eprintln!("mouse-up before (replay): {replay:.3} ms; after (reuse): {reuse:.3} ms");
}

#[test]
#[ignore = "CPU symmetry before/after benchmark; run with --ignored --nocapture"]
fn bench_cpu_symmetry() {
    use photocraft_color::PixelFormat;
    use photocraft_engine::paint::{BrushSettings, StrokePoint, StrokeRenderer};
    let brush = BrushSettings { size: 16.0, hardness: 1.0, spacing: 0.15, seed: 123, pressure_size: false, ..Default::default() };
    let pre = photocraft_doc::Surface::new(PixelFormat::RGBA8);
    let points: Vec<_> = (0..240).map(|i| StrokePoint::new(10.0 + i as f64 * 3.0, 100.0 + (i as f64 * 0.08).sin() * 50.0, 1.0)).collect();
    let run = |incremental: bool| {
        let (mut a, mut b) = (StrokeRenderer::new(&brush, Some(pre.format()), 1.0), StrokeRenderer::new(&brush, Some(pre.format()), 1.0));
        let mut target = pre.clone();
        let mut tail = Rect::EMPTY;
        let mut times = Vec::new();
        for p in &points {
            let t = Instant::now();
            a.push(&[*p]);
            let mut mirror = *p;
            mirror.x = 800.0 - p.x;
            b.push(&[mirror]);
            if incremental {
                let (_, next, _) = a.composite_union_live(&mut b, &pre, &mut target, None, false, tail);
                tail = next;
            } else {
                // Original live symmetry path: clone both renderers to finish and merge their
                // whole coverage maps on every pointer move.
                let (mut fa, mut fb) = (a.clone(), b.clone());
                fa.finish();
                fb.finish();
                let bounds = fa.bounds().union(&fb.bounds()).union(&tail);
                if !bounds.is_empty() {
                    target.write_region(bounds, &pre.read_region(bounds));
                }
                tail = bounds;
                fa.composite_union(&fb, &pre, &mut target, None, false);
            }
            times.push(ms(t));
        }
        (target, times)
    };
    let (old, before) = run(false);
    let (new, after) = run(true);
    assert_eq!(old, new);
    eprintln!(
        "symmetry push before: median {:.3} ms, max {:.3}; after: median {:.3} ms, max {:.3}; n=240",
        median(before.clone()),
        before.iter().copied().fold(0.0, f64::max),
        median(after.clone()),
        after.iter().copied().fold(0.0, f64::max)
    );
}

#[test]
fn full_refresh_over_residency_budget_keeps_gpu_compositing() {
    let _gpu = gpu_lock();
    let Some(rs) = render_state() else { return };
    let g = GpuCanvas::with_tile(&rs, Some(256));
    g.set_memory_budget(1 << 20);
    let (s, _, _) = realistic(900, 600);
    let d = &s.active().unwrap().doc;
    let r = g.refresh(d.id.0, d, None, None);
    assert_eq!(r.kind, "gpu-full", "{:?}", r.fallback);
    let (_, _, pixels) = g.read_texels(d.id.0).unwrap();
    assert!(max_error(d, &pixels) <= 3.0 / 255.0);
}

/// The three formerly unsupported features on the realistic editing document.
fn former_fallback_document(mut doc: Document) -> Document {
    use photocraft_doc::adjust::{CurvePoint, ToneSpace};
    use photocraft_doc::{Adjustment, BlendRange, FxCommon, FxPaint, Glow, GlowSource, GlowTechnique, LayerContent};
    let textured = doc.layers.iter_mut().find(|l| l.name == "Texture").expect("texture");
    textured.blend_if.set(0, [BlendRange { black: [0, 55], white: [210, 255] }, BlendRange { black: [15, 65], white: [200, 245] }]);
    let text = doc.layers.iter_mut().find(|l| l.name == "Golden hour" || matches!(l.content, LayerContent::Text(_))).expect("type layer");
    text.effects.items.push(Effect::OuterGlow(Glow {
        common: FxCommon::new(BlendMode::Screen, 0.65),
        paint: FxPaint::Color(Color::rgb(1.0, 0.6, 0.15)),
        technique: GlowTechnique::Softer,
        source: GlowSource::Edge,
        spread: 0.15,
        size: doc.size.height as f32 / 200.0,
        contour: Default::default(),
        range: 0.7,
        jitter: 0.0,
        noise: 0.35,
        anti_alias: false,
    }));
    let curve = vec![CurvePoint { input: 0.0, output: 0.0 }, CurvePoint { input: 0.45, output: 0.55 }, CurvePoint { input: 1.0, output: 1.0 }];
    doc.layers.push(Layer::new(
        "Lab lightness curve",
        LayerContent::Adjustment(Adjustment::Curves { master: vec![], per_channel: [curve, vec![], vec![]], space: ToneSpace::Lab, black: vec![] }),
    ));
    doc
}

#[test]
fn former_fallbacks_use_tiled_gpu_canvas_and_damage() {
    let _gpu = gpu_lock();
    let Some(rs) = render_state() else { return };
    let g = GpuCanvas::new(&rs);
    let (s, paint, _) = realistic(2300, 240);
    let mut doc = former_fallback_document((*s.active().unwrap().doc).clone());
    assert!(g.supports(&doc));
    for damage in [None, Some(Rect::new(2010, 100, 2070, 160))] {
        if let Some(r) = damage {
            doc.layer_mut(paint).unwrap().surface_mut().unwrap().fill_rect(r, &[0.8, 0.1, 0.3, 0.7]);
        }
        let refresh = g.refresh(doc.id.0, &doc, damage, None);
        assert!(refresh.kind.starts_with("gpu-"), "{:?}", refresh.fallback);
        assert!(refresh.fallback.is_none());
        let (_, _, out) = g.read_texels(doc.id.0).expect("readback");
        assert!(max_error(&doc, &out) <= 3.0 / 255.0);
    }
}

#[test]
fn native_cmyk_adjustment_uses_embedded_profile_on_gpu_canvas() {
    use photocraft_doc::adjust::{CurvePoint, ToneSpace};
    use photocraft_doc::{Adjustment, LayerContent};
    let _gpu = gpu_lock();
    let Some(rs) = render_state() else { return };
    let g = GpuCanvas::new(&rs);
    let mut doc = Document::new("embedded CMYK", Size::new(40, 30), ColorMode::Cmyk, SampleType::U16);
    let profile = photocraft_cms::synth::cmyk_profile(&photocraft_cms::synth::CmykParams {
        description: "Uncoated parity".into(),
        tvi: [0.26, 0.26, 0.26, 0.3],
        grid_a2b: 5,
        grid_b2a: 9,
        ..Default::default()
    });
    doc.icc_profile = Some(profile.to_bytes());
    let mut raster = Layer::raster("ink", doc.pixel_format());
    for x in 0..40 {
        raster.surface_mut().unwrap().fill_rect(Rect::new(x, 0, x + 1, 30), &[x as f32 / 40.0, 0.4, 0.2, 0.1, 1.0]);
    }
    let curve = vec![CurvePoint { input: 0.0, output: 0.0 }, CurvePoint { input: 0.4, output: 0.55 }, CurvePoint { input: 1.0, output: 1.0 }];
    doc.layers = vec![
        raster,
        Layer::new(
            "ink curve",
            LayerContent::Adjustment(Adjustment::Curves {
                master: curve.clone(),
                per_channel: [curve.clone(), vec![], vec![]],
                space: ToneSpace::Cmyk,
                black: curve,
            }),
        ),
    ];
    for profile in [doc.icc_profile.clone(), None] {
        doc.icc_profile = profile;
        let refresh = g.refresh(doc.id.0, &doc, None, None);
        assert_eq!(refresh.kind, "gpu-full", "{:?}", refresh.fallback);
        let (_, _, out) = g.read_texels(doc.id.0).expect("readback");
        assert!(max_error(&doc, &out) <= 3.0 / 255.0);
    }
}
