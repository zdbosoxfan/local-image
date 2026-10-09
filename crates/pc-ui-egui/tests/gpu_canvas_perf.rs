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
use photocraft_doc::{Document, Effect, Layer, LayerContent, LayerId, LayerMask, Size};
use photocraft_engine::Session;
use photocraft_geom::Rect;
use photocraft_ui_egui::gpu_canvas::{self, GpuCanvas};
use serde_json::{Value, json};

/// Concurrent wgpu devices in one process crash on some drivers (see `canvas_16f.rs`).
static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu_lock() -> std::sync::MutexGuard<'static, ()> {
    GPU_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn render_state() -> Option<RenderState> {
    let rs =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| egui_kittest::wgpu::create_render_state(gpu_canvas::wgpu_setup(), Default::default()))).ok();
    if rs.is_none() {
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
    let text = layer_of(&exec(&mut s, "type.create", json!({"x": wi / 10, "y": hi / 5, "text": "Golden hour", "size": (h as f32 / 12.0).max(8.0), "color": "#fff4e0"})));
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
