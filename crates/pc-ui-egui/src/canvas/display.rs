//! Canvas display: CPU textures and damage, the navigator thumbnail, GPU uploads and budget, filter
//! and adjustment proxies, and the display LUT.
use super::*;

pub(super) fn checker(app: &mut PhotocraftApp, ctx: &egui::Context) -> egui::TextureId {
    // Preferences › Transparency & Gamut colours (the texture is rebuilt when they change).
    let [ca, cb] = app.session.prefs().transparency_and_gamut.colors();
    if app.prefs_rt.checker_key != Some([ca, cb]) {
        app.prefs_rt.checker_key = Some([ca, cb]);
        app.checker = None;
    }
    app.checker
        .get_or_insert_with(|| {
            let (a, b) = (Color32::from_rgb(ca[0], ca[1], ca[2]), Color32::from_rgb(cb[0], cb[1], cb[2]));
            let img = egui::ColorImage::new([2, 2], vec![a, b, b, a]);
            let opts = TextureOptions {
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Nearest,
                wrap_mode: egui::TextureWrapMode::Repeat,
                mipmap_mode: None,
            };
            ctx.load_texture("checker", img, opts)
        })
        .id()
}

pub(super) fn buffer_to_image(buf: &photocraft_compose::Buffer) -> egui::ColorImage {
    let img = buf.to_rgba8();
    egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.pixels)
}

/// A composite as monitor pixels: document profile → monitor profile (nothing to do in the
/// common sRGB-on-sRGB case).
pub(super) fn display_image(
    display: Option<&photocraft_engine::display_color::CanvasDisplay>,
    buf: &photocraft_compose::Buffer,
    depth: photocraft_doc::SampleType,
) -> egui::ColorImage {
    match display {
        Some(d) if !d.is_identity() || depth != photocraft_doc::SampleType::U8 => {
            let img = d.to_rgba8(buf);
            egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.pixels)
        }
        _ => buffer_to_image(buf),
    }
}

/// The canvas display of `doc` on `display` (`None`: the main window's) and a key that changes
/// with it (folded into the canvas caches' preview keys, so a monitor or profile change
/// re-renders).
pub(super) fn canvas_display(
    app: &PhotocraftApp,
    doc: &Document,
    display: Option<u32>,
) -> (Option<std::sync::Arc<photocraft_engine::display_color::CanvasDisplay>>, u64) {
    match app.session.color.canvas_display_for(doc, display.or(app.session.color.main_display)) {
        Ok(d) => {
            let k = d.key;
            (Some(d), k)
        }
        Err(_) => (None, 0),
    }
}

/// [`canvas_display`]'s key for the GPU canvas texture: what the texture stores, the same on
/// every display (the display LUT holds the monitor profile).
pub(super) fn texture_key(display: Option<&photocraft_engine::display_color::CanvasDisplay>) -> u64 {
    display.map_or(0, |d| d.texture_key)
}

/// `app.canvases` key: CPU canvas textures are per document and display (they hold monitor
/// values); the GPU canvas state uses [`GPU_OUTPUT`].
pub(super) fn cache_key(doc: photocraft_doc::DocId, display: Option<u32>) -> (photocraft_doc::DocId, u32) {
    (doc, display.unwrap_or(0))
}

/// `app.canvases` output of the GPU canvas state (one texture per document for all displays).
pub(crate) const GPU_OUTPUT: u32 = u32::MAX;

/// The document to render: the committed one, or a clone with the live adjustment preview applied.
pub(super) fn legacy_display_doc(app: &mut PhotocraftApp, idx: usize) -> (std::sync::Arc<Document>, u64) {
    if let Some(shown) = crate::symbols::display_doc(&app.session, &app.ui.symbols, idx) {
        return shown;
    }
    // Puppet / Perspective Warp previews hide the layer they draw on a mesh.
    if let Some(shown) = crate::distort_ui::display_doc(app, idx) {
        return shown;
    }
    // Image › Adjustments dialog: a temporary adjustment layer clipped to the target.
    if let Some(shown) = crate::adjust_preview::display_doc(app, idx) {
        return shown;
    }
    // Gradient tool (live) drag, or a stop dragged in the Properties panel.
    if let Some(shown) = crate::gradient_ui::display_doc(app, idx) {
        return shown;
    }
    // Move tool drag: the moving layers at the pointer.
    if let Some(shown) = crate::move_ui::display_doc(app, idx) {
        return shown;
    }
    // Patch Tool drag: the selection healed from where the pointer is.
    if let Some(shown) = crate::patch_preview::display_doc(app, idx) {
        return shown;
    }
    let st = &app.session.documents()[idx];
    if let Some(l) = live_stroke(app, idx) {
        return (l.shown_doc.as_ref().unwrap_or(&l.stroke.doc).clone(), l.display_key());
    }
    if let (Some(t), Some(pv)) = (&app.ui.transform, &app.transform_preview)
        && app.session.active_index() == Some(idx)
        && st.doc.layer(photocraft_doc::LayerId(t.layer)).is_some()
    {
        return (pv.doc.clone(), (1 << 40) + pv.session);
    }
    // Layer Style dialog: show its effects live (Cancel just drops the preview).
    let style = app
        .ui
        .dialogs
        .iter()
        .find(|d| d.kind == crate::state::DialogKind::LayerStyle)
        .filter(|dialog| crate::dialogs::validate_document_target(app, &dialog.fields).is_ok());
    // The Preview checkbox off shows the document as it was when the dialog opened;
    // edits still land in the dialog state and OK applies them whatever it says.
    let preview_on = style.and_then(|d| d.fields.get("preview")).and_then(serde_json::Value::as_bool).unwrap_or(true);
    if style.is_none() || !preview_on {
        app.style_preview = None;
    }
    if let Some(d) = style.filter(|_| preview_on)
        && app.session.active_index() == Some(idx)
    {
        let key = (crate::layer_style::preview_hash(&d.fields) ^ st.revision.wrapping_mul(0x9e37_79b9_7f4a_7c15)) | 1 << 63;
        if app.style_preview.as_ref().map(|p| p.0) != Some(key) {
            let shown = crate::layer_style::preview_document(&st.doc, &app.session.patterns, &d.fields).map(std::sync::Arc::new);
            if let Err(error) = &shown {
                log::warn!("Layer Style preview: {error}");
            }
            app.style_preview = Some((key, shown));
        }
        if let Some((_, Ok(doc))) = &app.style_preview {
            return (doc.clone(), key);
        }
    }
    if let Some((layer, params)) = &app.live_adjust
        && let Some(l) = st.doc.layer(*layer)
        && let LayerContent::Adjustment(a) = &l.content
    {
        let kind = photocraft_engine::commands::adjustment_kind(a);
        let preview = photocraft_engine::adjust_params::from_params(kind, params, Some(a), st.doc.mode).unwrap_or_else(|_| a.clone());
        let mut doc = (*st.doc).clone();
        if let Some(lm) = doc.layer_mut(*layer) {
            lm.content = LayerContent::Adjustment(preview);
        }
        let key = 1 + params.to_string().bytes().fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
        return (std::sync::Arc::new(doc), key);
    }
    // Duotone documents display through their inks.
    if let Some(shown) = photocraft_engine::mode_cmds::display_document(&st.doc) {
        return (std::sync::Arc::new(shown), 1 << 41);
    }
    (st.doc.clone(), 0)
}

pub fn display_doc(app: &mut PhotocraftApp, ctx: &egui::Context, idx: usize) -> (std::sync::Arc<Document>, u64) {
    let (source, key) = legacy_display_doc(app, idx);
    crate::vector_integration::display(app, ctx, idx, source, key)
}

/// Longest side of the Navigator panel's image (about twice the panel's width, for HiDPI).
pub const NAVIGATOR_SIDE: u32 = 512;
/// How long adjustment-dialog settings must stay unchanged before the navigator shows them.
pub(super) const NAVIGATOR_SETTLE_MS: f64 = 200.0;

/// The Navigator panel's image of document `idx`. With the CPU canvas that is the canvas texture
/// itself; with the GPU canvas a thumbnail cached per revision, so an edit to a huge document
/// doesn't also pay a full-resolution CPU composite for the navigator.
pub(crate) type NavigatorTexture = (std::sync::Weak<Document>, u64, egui::TextureHandle);

pub fn navigator_texture(app: &mut PhotocraftApp, ctx: &egui::Context, idx: usize) -> Option<egui::TextureId> {
    if app.gpu.is_none() {
        return ensure_texture(app, ctx, idx, app.session.color.main_display).map(|(t, _)| t);
    }
    // Keyed by the document snapshot, not its revision: selecting a layer changes no pixels.
    let (snapshot, id) = app.session.documents().get(idx).map(|st| (std::sync::Arc::downgrade(&st.doc), st.doc.id))?;
    if app.drag.is_some()
        && let Some((_, p, t)) = app.navigator_textures.get(&id)
        && *p != u64::MAX
    {
        return Some(t.id());
    }
    let (doc, preview_key) = if app.live_stroke.is_some() { (app.session.documents().get(idx)?.doc.clone(), 0) } else { display_doc(app, ctx, idx) };
    let (display, display_key) = canvas_display(app, &doc, None);
    let preview_key = preview_key ^ display_key;
    let cached = app.navigator_textures.get(&id).cloned();
    if let Some((r, p, t)) = &cached
        && r.ptr_eq(&snapshot)
        && *p == preview_key
    {
        return Some(t.id());
    }
    // While a Move or Patch drag is under way the navigator keeps its image and catches up on release.
    if let Some((_, _, t)) = &cached
        && (crate::move_ui::showing(app) || crate::patch_preview::showing(app))
    {
        return Some(t.id());
    }
    // While an adjustment dialog's settings are changing, the navigator keeps its image and
    // catches up once they settle (a thumbnail composite per change would cost more than the canvas).
    if let Some((r, _, t)) = &cached
        && r.ptr_eq(&snapshot)
        && crate::adjust_preview::settling(app, NAVIGATOR_SETTLE_MS)
    {
        ctx.request_repaint_after(std::time::Duration::from_millis(NAVIGATOR_SETTLE_MS as u64));
        return Some(t.id());
    }
    #[cfg(not(target_arch = "wasm32"))]
    if cached.is_none() && doc.size.area() > NAVIGATOR_SYNC_PIXELS {
        let tex = ctx.load_texture(format!("navigator-{}", id.0), egui::ColorImage::new([1, 1], vec![egui::Color32::TRANSPARENT]), TextureOptions::LINEAR);
        app.navigator_textures.insert(id, (std::sync::Weak::<Document>::new(), u64::MAX, tex.clone()));
        return Some(navigator_in_background(app, ctx, id, (snapshot, preview_key), doc, display, tex));
    }
    // A large document's thumbnail is a full CPU composite (100+ ms at 24 MP with effects): it
    // renders on a worker thread while the panel keeps the previous image, so a brush stroke's
    // commit doesn't stall the UI. Small documents render here.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((_, _, t)) = &cached
        && doc.size.area() > NAVIGATOR_SYNC_PIXELS
    {
        let t = t.clone();
        return Some(navigator_in_background(app, ctx, id, (snapshot, preview_key), doc, display, t));
    }
    let t0 = crate::gpu_canvas::now_ms();
    let image = display_image(display.as_deref(), &photocraft_compose::thumbnail_buffer(&doc, NAVIGATOR_SIDE), doc.depth);
    let tex = match cached {
        Some((_, _, mut t)) => {
            t.set(image, TextureOptions::LINEAR);
            t
        }
        None => ctx.load_texture(format!("navigator-{}", id.0), image, TextureOptions::LINEAR),
    };
    app.perf.span("navigator", crate::gpu_canvas::now_ms() - t0);
    app.navigator_textures.insert(id, (snapshot, preview_key, tex.clone()));
    Some(tex.id())
}

/// Documents up to this many pixels render their Navigator image on the UI thread.
#[cfg(not(target_arch = "wasm32"))]
pub(super) const NAVIGATOR_SYNC_PIXELS: u64 = 4 << 20;

/// A Navigator image rendering on a worker thread: what it shows (document snapshot, preview
/// key), its result once done, and when it started (ms).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) type NavigatorJob = (std::sync::Weak<Document>, u64, std::sync::Arc<std::sync::Mutex<Option<egui::ColorImage>>>, f64);

/// [`navigator_texture`] for a large document: show `tex` (the last image) while the current
/// one renders on a worker thread (one at a time per document), then swap it in.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn navigator_in_background(
    app: &mut PhotocraftApp,
    ctx: &egui::Context,
    id: photocraft_doc::DocId,
    want: (std::sync::Weak<Document>, u64),
    doc: std::sync::Arc<Document>,
    display: Option<std::sync::Arc<photocraft_engine::display_color::CanvasDisplay>>,
    mut tex: egui::TextureHandle,
) -> egui::TextureId {
    let job = app.navigator_jobs.get(&id).cloned();
    if let Some((snapshot, preview_key, slot, t0)) = job {
        let done = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        let Some(image) = done else {
            // Still rendering (perhaps an older state: the newest starts when it's done).
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
            return tex.id();
        };
        app.perf.span("navigator", crate::gpu_canvas::now_ms() - t0);
        let current = snapshot.ptr_eq(&want.0) && preview_key == want.1;
        app.navigator_jobs.remove(&id);
        if current {
            tex.set(image, TextureOptions::LINEAR);
            app.navigator_textures.insert(id, (snapshot, preview_key, tex.clone()));
        }
        if current {
            return tex.id();
        }
    }
    let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
    let out = slot.clone();
    let spawned = std::thread::Builder::new().name("navigator".into()).spawn(move || {
        let image = display_image(display.as_deref(), &photocraft_compose::thumbnail_buffer(&doc, NAVIGATOR_SIDE), doc.depth);
        *out.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(image);
    });
    if spawned.is_ok() {
        app.navigator_jobs.insert(id, (want.0, want.1, slot, crate::gpu_canvas::now_ms()));
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
    tex.id()
}

/// Make sure the canvas texture for document `idx` on `display` is current; returns (texture
/// id, scale).
pub fn ensure_texture(app: &mut PhotocraftApp, ctx: &egui::Context, idx: usize, display: Option<u32>) -> Option<(egui::TextureId, f32)> {
    let output = display;
    let (revision, last_damage, id) = {
        let st = app.session.documents().get(idx)?;
        (st.revision, st.last_damage.map(|r| if r.is_empty() { r } else { r.inflate(effect_reach(&st.doc.layers)) }), st.doc.id)
    };
    let (mut doc, mut preview_key) = display_doc(app, ctx, idx);
    // An adjustment dialog's preview of a large document (drawn by the GPU path on a GPU machine)
    // would cost a full-size CPU composite per change here: show the document instead.
    if crate::adjust_preview::shown_key(app) == Some(preview_key)
        && let Some(st) = app.session.documents().get(idx)
        && crate::proxy::factor(&st.doc) > 1
    {
        (doc, preview_key) = (st.doc.clone(), 0);
    }
    let (display, display_key) = canvas_display(app, &doc, output);
    let seen = app.canvases.get(&cache_key(id, output)).map(|c| (c.tex_revision, c.tex_preview_key));
    let damage = seen.and_then(|seen| damage_since(app, idx, seen, (revision, preview_key), display_key, last_damage));
    let preview_key = preview_key ^ display_key;
    let sampling = if crate::vector_integration::views::pixel_preview(app, idx) { TextureOptions::NEAREST } else { TextureOptions::LINEAR };
    let cache = app.canvases.entry(cache_key(id, output)).or_insert(CanvasCache {
        revision: 0,
        texture: None,
        scale: 1.0,
        preview_key: 0,
        on_gpu: false,
        tex_revision: 0,
        tex_preview_key: 0,
    });
    if cache.tex_revision != revision || cache.texture.is_none() || cache.tex_preview_key != preview_key {
        let t0 = crate::gpu_canvas::now_ms();
        let longest = doc.size.width.max(doc.size.height);
        let factor = longest.div_ceil(texture_side(ctx)).max(1);
        let (w, h) = ((doc.size.width / factor).max(1), (doc.size.height / factor).max(1));
        let current = cache.texture.as_mut().filter(|t| t.size() == [w as usize, h as usize]);
        if let (Some(d), Some(t)) = (damage, current) {
            // Only what the edit or stroke touched: the reduced texture's pixels over it, with the
            // values the whole reduction gives them (factor 1 is the plain composite of `d`).
            let buf = photocraft_compose::render_reduced_damage(&doc, w, h, d);
            let r = buf.rect;
            if !r.is_empty() {
                let t1 = crate::gpu_canvas::now_ms();
                t.set_partial([r.x0 as usize, r.y0 as usize], display_image(display.as_deref(), &buf, doc.depth), sampling);
                let px = (r.width() as u64 * r.height() as u64).saturating_mul(u64::from(factor).pow(2));
                app.perf.record("rect", px, t1 - t0, crate::gpu_canvas::now_ms() - t1);
            }
        } else {
            // Reduced in bands straight from the compositor: no full-size composite in memory.
            let full = photocraft_compose::render_reduced(&doc, w, h);
            let t1 = crate::gpu_canvas::now_ms();
            let (img, scale) = (display_image(display.as_deref(), &full, doc.depth), 1.0 / factor as f32);
            match cache.texture.as_mut() {
                Some(t) if t.size() == img.size => t.set(img, sampling),
                _ => cache.texture = Some(ctx.load_texture(format!("canvas-{}-{}", id.0, cache_key(id, output).1), img, sampling)),
            }
            cache.scale = scale;
            app.perf.record("full", doc.size.width as u64 * doc.size.height as u64, t1 - t0, crate::gpu_canvas::now_ms() - t1);
        }
        cache.tex_revision = revision;
        cache.tex_preview_key = preview_key;
    }
    Some((cache.texture.as_ref()?.id(), cache.scale))
}

/// What changed since a canvas cache showed (`revision`, `preview key`) `seen`, when only a
/// rectangle did: the last edit's damage, or the live stroke's dabs since then. Cached keys have
/// the colour display's key folded in (`^ display_key`); `now`'s is still raw.
pub(super) fn damage_since(app: &PhotocraftApp, idx: usize, seen: (u64, u64), now: (u64, u64), display_key: u64, last_damage: Option<DRect>) -> Option<DRect> {
    if let Some(st) = app.session.documents().get(idx)
        && let Some(rect) = crate::vector_integration::damage(app, st.doc.id, (seen.0, seen.1 ^ display_key), now)
    {
        return Some(rect);
    }
    if seen.1 == now.1 ^ display_key && seen.0 + 1 == now.0 {
        return last_damage;
    }
    // Between an adjustment dialog's previews (and the document): the target's area.
    if seen.0 == now.0
        && let Some(st) = app.session.documents().get(idx)
        && let Some(r) = crate::adjust_preview::switch_region(app, st.doc.id, now.0, seen.1 ^ display_key, now.1)
    {
        return Some(r);
    }
    // Between a Move drag's offsets: where the moving layers were and are (Auto-Select's pick
    // at the press changes no pixels).
    if (seen.0 == now.0 || (seen.0 + 1 == now.0 && last_damage.is_some_and(|r| r.is_empty())))
        && let Some(st) = app.session.documents().get(idx)
        && let Some(r) = crate::move_ui::damage(app, st.doc.id, now.0, seen.1 ^ display_key, now.1)
    {
        return Some(if r.is_empty() { r } else { r.inflate(effect_reach(&st.doc.layers)) });
    }
    // Between a Patch drag's previews: the areas they healed.
    if seen.0 == now.0
        && let Some(st) = app.session.documents().get(idx)
        && let Some(r) = crate::patch_preview::damage(app, st.doc.id, now.0, seen.1 ^ display_key, now.1)
    {
        return Some(if r.is_empty() { r } else { r.inflate(effect_reach(&st.doc.layers)) });
    }
    let l = live_stroke(app, idx).filter(|l| seen.0 == now.0 && l.display_key() == now.1)?;
    let r = l.since(seen.1 ^ display_key)?;
    Some(if r.is_empty() { r } else { r.inflate(effect_reach(&l.shown_doc.as_ref().unwrap_or(&l.stroke.doc).layers)) })
}

/// Document `doc`'s canvas caches showed a preview that the edit just committed reproduces
/// (`was_preview` tells its keys): count it as the document itself, so the commit's damage rect
/// refreshes only that area instead of everything.
pub(crate) fn shown_as_document(app: &mut PhotocraftApp, doc: photocraft_doc::DocId, was_preview: impl Fn(u64) -> bool) {
    let Some(d) = app.session.documents().iter().find(|st| st.doc.id == doc).map(|st| st.doc.clone()) else { return };
    // Every display's cache of the document: the GPU state folds in the texture key, CPU
    // textures their display's key.
    let outputs: Vec<u32> = app.canvases.keys().filter(|k| k.0 == doc).map(|k| k.1).collect();
    for out in outputs {
        let (display, key) = canvas_display(app, &d, (out != 0 && out != GPU_OUTPUT).then_some(out));
        let gpu_key = texture_key(display.as_deref());
        let Some(c) = app.canvases.get_mut(&(doc, out)) else { continue };
        if was_preview(c.preview_key ^ gpu_key) {
            c.preview_key = gpu_key;
        }
        if was_preview(c.tex_preview_key ^ key) {
            c.tex_preview_key = key;
        }
    }
}

/// How far beyond an edit's damage rect the composite can change: layer effects (shadows, glows,
/// strokes, …) on the edited layer and on the groups around it reach that far.
pub(crate) fn effect_reach(layers: &[photocraft_doc::Layer]) -> i32 {
    layers
        .iter()
        .filter(|l| l.visible)
        .map(|l| {
            let own = if photocraft_compose::effects::has_effects(l) { photocraft_compose::effects::margin(l) } else { 0 };
            let inner = match &l.content {
                LayerContent::Group(g) => effect_reach(&g.children),
                _ => 0,
            };
            own + inner
        })
        .max()
        .unwrap_or(0)
}

/// GPU path: make sure document `idx` is current in the GPU canvas. Brush strokes re-composite
/// and upload only their damage rect; everything else re-composites the whole document.
/// Returns false if there is no GPU canvas.
pub(super) fn ensure_gpu(app: &mut PhotocraftApp, ctx: &egui::Context, idx: usize, visible: DRect) -> bool {
    let Some(gpu) = app.gpu.clone() else { return false };
    let Some((revision, last_damage, id)) = app
        .session
        .documents()
        .get(idx)
        .map(|st| (st.revision, st.last_damage.map(|r| if r.is_empty() { r } else { r.inflate(effect_reach(&st.doc.layers)) }), st.doc.id))
    else {
        return false;
    };
    let (doc, raw_key) = display_doc(app, ctx, idx);
    let (display, _) = canvas_display(app, &doc, None);
    // The texture is shared by every display: only what it stores counts, not the monitor.
    let display_key = texture_key(display.as_deref());
    let seen = app.canvases.get(&(id, GPU_OUTPUT)).map(|c| (c.revision, c.preview_key));
    let mut damage = seen.and_then(|seen| damage_since(app, idx, seen, (revision, raw_key), display_key, last_damage));
    // To an adjustment dialog's preview: only what the view shows now (the rest as it moves there).
    if damage.is_some()
        && let Some(r) = seen.filter(|s| s.0 == revision).and_then(|s| crate::adjust_preview::switch_region(app, id, revision, s.1 ^ display_key, raw_key))
    {
        damage = Some(crate::adjust_preview::switch_damage(app, raw_key, r, visible));
    }
    let preview_key = raw_key ^ display_key;
    let size = [doc.size.width, doc.size.height];
    // Display textures for a canvas this large would not fit the GPU: the reduced CPU canvas
    // draws it.
    if !gpu.fits_canvas(size, gpu.format_for(doc.depth, size)) {
        gpu.discard_display(id.0);
        return false;
    }
    let cache = app.canvases.entry((id, GPU_OUTPUT)).or_insert(CanvasCache {
        revision: 0,
        texture: None,
        scale: 1.0,
        preview_key: 0,
        on_gpu: false,
        tex_revision: 0,
        tex_preview_key: 0,
    });
    let present = cache.on_gpu && gpu.has(id.0, size);
    let current = present && cache.revision == revision && cache.preview_key == preview_key;
    if current {
        // GPU Memory, Memory Usage or the active document changed with nothing to redraw: size
        // the budget at once rather than at the next refresh.
        if app.perf.gpu_budget_inputs != budget_inputs(app, idx) {
            gpu_budget(app, &gpu, idx, false, visible);
        }
        // An adjustment preview composited where the view was: catch up where it moved to.
        if let Some(r) = crate::adjust_preview::uncovered(app, id, raw_key, visible) {
            let r = gpu.refresh_snapshot(id.0, &doc, Some(r), display.as_deref());
            app.perf.record(r.kind, r.px, r.composite_ms, r.upload_ms);
        }
        return true;
    }
    let partial = present && damage.is_some();
    gpu_budget(app, &gpu, idx, partial, visible);
    let r = gpu.refresh_snapshot(id.0, &doc, if partial { damage } else { None }, display.as_deref());
    if r.kind == "lost" || r.kind == crate::gpu_canvas::REFRESH_OVER_BUDGET {
        // The device is gone (`gpu_status` then drops the GPU canvas) or the canvas is over the
        // texture budget: draw this frame on the CPU path.
        return false;
    }
    if let Some(e) = &r.fallback
        && app.perf.gpu_fallback.as_deref() != Some(e.as_str())
    {
        log::info!("{e}; using the CPU compositor");
    }
    app.perf.record(r.kind, r.px, r.composite_ms, r.upload_ms);
    if r.kind.starts_with("gpu") {
        app.perf.gpu_uploads = r.uploads;
        app.perf.gpu_fallback = None;
    } else {
        app.perf.gpu_fallback = r.fallback;
    }
    let Some(cache) = app.canvases.get_mut(&(id, GPU_OUTPUT)) else { return true };
    cache.revision = revision;
    cache.preview_key = preview_key;
    cache.on_gpu = true;
    cache.texture = None;
    true
}

/// Point the wgpu compositor at what the view shows, and size its memory budget from GPU Memory,
/// or Memory Usage, the document's pixels and History (charged where the GPU shares the system's
/// memory), physical memory and the GPU's own: on full refreshes (pixels counted once per
/// structural change, not per brush dab) and whenever a preference or the document changes.
pub(super) fn gpu_budget(app: &mut PhotocraftApp, gpu: &crate::gpu_canvas::GpuCanvas, idx: usize, partial: bool, visible: DRect) {
    gpu.set_focus(Some(visible));
    let inputs = budget_inputs(app, idx);
    if partial
        && lightcraft_sysmem::gpu::sample().is_none()
        && !lightcraft_sysmem::gpu::pressured()
        && app.perf.gpu_budget_inputs == inputs
        && gpu.memory_budget().is_some()
    {
        return;
    }
    let Some(st) = app.session.documents().get(idx) else { return };
    let pixels = st.memory_bytes() as u64;
    let (allowance, fixed, _) = inputs;
    let budget = crate::gpu_canvas::memory_budget(allowance, pixels, crate::gpu_canvas::physical_memory(), gpu.video_memory(), fixed);
    let budget = lightcraft_sysmem::gpu::cache_budget(budget);
    gpu.set_memory_budget(budget);
    app.perf.gpu_budget_mb = budget >> 20;
    app.perf.gpu_budget_inputs = inputs;
}

/// Memory Usage and GPU Memory (Preferences › Performance, bytes) and the document the budget is
/// sized for.
fn budget_inputs(app: &PhotocraftApp, idx: usize) -> (u64, u64, Option<photocraft_doc::DocId>) {
    let performance = &app.session.prefs().performance;
    (
        u64::from(performance.memory_usage_mb).saturating_mul(1 << 20),
        u64::from(performance.gpu_memory_mb).saturating_mul(1 << 20),
        app.session.documents().get(idx).map(|st| st.doc.id),
    )
}

/// Live preview for an open filter dialog: run the filter on the proxy and upload it.
pub(super) fn ensure_filter_preview(app: &mut PhotocraftApp, idx: usize) -> Option<(u32, u64)> {
    if crate::adjust_preview::on_layer(app, idx) {
        return None;
    }
    let d = app.ui.dialogs.iter().find(|d| d.fields.contains_key("__filter"))?;
    crate::dialogs::validate_document_target(app, &d.fields).ok()?;
    if d.fields.get("__preview").and_then(serde_json::Value::as_bool) != Some(true) {
        return None;
    }
    let cmd = d.fields.get("__command")?.as_str()?.to_string();
    // Previews edit what the command will: a targeted layer mask included (#780).
    let params = app.with_mask_target(&cmd, crate::filter_dialog::params_of(&d.fields));
    let (doc_id, revision, doc, active) = {
        let st = app.session.documents().get(idx)?;
        (st.doc.id, st.revision, st.doc.clone(), st.active_layer)
    };
    let k = crate::proxy::factor(&doc);
    let hash = format!("{cmd}{params}").bytes().fold(k as u64 ^ revision.wrapping_mul(0x9e37), |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
    let key = doc_id.0 ^ (1u64 << 61);
    let fresh = matches!(&app.filter_preview, Some(p) if p.doc == doc_id && p.hash == hash);
    if !fresh {
        let t0 = crate::gpu_canvas::now_ms();
        let documents: Vec<_> = app.session.documents().iter().map(|state| state.doc.clone()).collect();
        let result = crate::filter_dialog::preview_document_with_sources(&doc, active, &cmd, &params, k, &documents).map(std::sync::Arc::new);
        if let Some(r) = &result {
            let buf = photocraft_compose::flatten(r);
            let t1 = crate::gpu_canvas::now_ms();
            let (display, _) = canvas_display(app, &doc, None);
            app.gpu.as_ref()?.upload_buffer_full(key, &texture_buffer(display.as_deref(), &buf), doc.depth);
            app.perf.record("filter-preview", r.size.area(), t1 - t0, crate::gpu_canvas::now_ms() - t1);
        }
        app.filter_preview = Some(crate::filter_dialog::FilterPreview { doc: doc_id, revision, hash, k, result });
    }
    app.filter_preview.as_ref().filter(|p| p.result.is_some()).map(|p| (p.k, key))
}

/// Conservative four-corner document coverage, with a margin for filtering.
pub(super) fn visible_doc_rect(xf: &ViewXform) -> DRect {
    let visible = xf.camera().document_bounds(xf.rect);
    let c = |v: f32| v.clamp(-1e9, 1e9) as i32;
    DRect::new(c(visible.left().floor()) - 2, c(visible.top().floor()) - 2, c(visible.right().ceil()) + 2, c(visible.bottom().ceil()) + 2)
}

/// Zoomed-out Image › Adjustments preview on a large document: the wgpu compositor renders the
/// reduced preview document (`adjust_preview::gpu_proxy`) into its own texture, over the target's
/// area after the first frame. Returns (factor, gpu key) to draw.
pub(super) fn ensure_adjust_proxy(app: &mut PhotocraftApp, idx: usize, zoom: f32) -> Option<(u32, u64)> {
    let frame = crate::adjust_preview::gpu_proxy(app, idx, zoom)?;
    let key = frame.doc.id.0;
    let size = [frame.doc.size.width, frame.doc.size.height];
    let gpu = app.gpu.clone()?;
    if frame.stale || !gpu.has(key, size) {
        let doc = app.session.documents().get(idx)?.doc.clone();
        let (display, _) = canvas_display(app, &doc, None);
        let r = gpu.refresh_snapshot(key, &frame.doc, frame.damage, display.as_deref());
        app.perf.record(if r.kind.starts_with("gpu") { "gpu-adjust-proxy" } else { "adjust-proxy" }, r.px, r.composite_ms, r.upload_ms);
        crate::adjust_preview::proxy_drawn(app, &frame);
    }
    Some((frame.k, key))
}

/// If a live adjustment preview is active on a large document, composite it on the proxy and upload
/// it under its own GPU key. Returns (factor, gpu key) when the proxy should be drawn.
pub(super) fn ensure_proxy_preview(app: &mut PhotocraftApp, idx: usize) -> Option<(u32, u64)> {
    let (layer, params) = app.live_adjust.clone()?;
    let (doc_id, revision, doc) = {
        let st = app.session.documents().get(idx)?;
        (st.doc.id, st.revision, st.doc.clone())
    };
    let k = crate::proxy::factor(&doc);
    if k <= 1 {
        return None;
    }
    let fresh = matches!(&app.proxy, Some((d, r, kk, _)) if *d == doc_id && *r == revision && *kk == k);
    if !fresh {
        app.proxy = Some((doc_id, revision, k, std::sync::Arc::new(crate::proxy::proxy_document(&doc, k))));
    }
    let proxy = app.proxy.as_ref()?.3.clone();
    let key = doc_id.0 ^ (1u64 << 62);
    let hash = params.to_string().bytes().fold(k as u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
    if app.proxy_uploaded != Some((doc_id, hash)) {
        let l = proxy.layer(layer)?;
        let LayerContent::Adjustment(a) = &l.content else { return None };
        let kind = photocraft_engine::commands::adjustment_kind(a);
        let preview = photocraft_engine::adjust_params::from_params(kind, &params, Some(a), proxy.mode).unwrap_or_else(|_| a.clone());
        let mut p = (*proxy).clone();
        if let Some(lm) = p.layer_mut(layer) {
            lm.content = LayerContent::Adjustment(preview);
        }
        let t0 = crate::gpu_canvas::now_ms();
        let buf = photocraft_compose::flatten(&p);
        let t1 = crate::gpu_canvas::now_ms();
        let (display, _) = canvas_display(app, &doc, None);
        app.gpu.as_ref()?.upload_buffer_full(key, &texture_buffer(display.as_deref(), &buf), doc.depth);
        app.perf.record("proxy", p.size.area(), t1 - t0, crate::gpu_canvas::now_ms() - t1);
        app.proxy_uploaded = Some((doc_id, hash));
    }
    Some((k, key))
}

/// A CPU composite as the GPU canvas texture stores it (sRGB-encoded for linear documents).
pub(super) fn texture_buffer<'a>(
    display: Option<&photocraft_engine::display_color::CanvasDisplay>,
    buf: &'a photocraft_compose::Buffer,
) -> std::borrow::Cow<'a, photocraft_compose::Buffer> {
    match display {
        Some(d) => d.texture_buffer(buf),
        None => std::borrow::Cow::Borrowed(buf),
    }
}

pub(crate) fn retain_gpu_documents(app: &mut PhotocraftApp) {
    let open: Vec<_> = app.session.documents().iter().map(|st| st.doc.id).collect();
    app.canvases.retain(|(id, _), _| open.contains(id));
    app.navigator_textures.retain(|id, _| open.contains(id));
    #[cfg(not(target_arch = "wasm32"))]
    app.navigator_jobs.retain(|id, _| open.contains(id));
    // Remove a closed adjustment owner's proxy before collecting its live GPU keys.
    crate::adjust_preview::retain_documents(app);
    crate::vector_integration::retain_documents(app);
    if let Some(gpu) = &app.gpu {
        // Keep each document's texture plus its preview textures (filter preview, adjustment proxy);
        // retaining only document ids freed the previews every frame (blank canvas while previewing).
        let mut live: Vec<u64> = app.session.documents().iter().flat_map(|st| [st.doc.id.0, st.doc.id.0 ^ (1u64 << 61), st.doc.id.0 ^ (1u64 << 62)]).collect();
        live.extend(crate::adjust_preview::gpu_keys(app));
        if let Some(f) = &app.move_fast {
            live.extend(f.keys());
        }
        gpu.retain(&live);
        // View IDs include the viewport and document slot. A closed secondary
        // window must release its scaled projection while the document stays open.
        let mut views = Vec::new();
        let mut keep = |viewport, document| {
            for kind in ["pc-canvas", "pc-canvas-proxy", "pc-canvas-move"] {
                views.push(egui::Id::new((kind, viewport, document)).value());
            }
        };
        for document in 0..app.session.documents().len() {
            keep(egui::ViewportId::ROOT, document);
        }
        for window in app.ui.windows.iter().filter(|w| w.open && w.document < app.session.documents().len()) {
            keep(egui::ViewportId::from_hash_of(("docwin", window.id)), window.document);
        }
        gpu.retain_views(&views);
    }
    // Upload markers must not outlive the resources they describe: native reopen can reuse
    // both the document ID and revision before the next frame while a preview dialog stays open.
    let documents = app.session.documents();
    if app.filter_preview.as_ref().is_some_and(|preview| !documents.iter().any(|st| st.doc.id == preview.doc)) {
        app.filter_preview = None;
    }
    if app.proxy_uploaded.is_some_and(|(doc, _)| !documents.iter().any(|st| st.doc.id == doc)) {
        app.proxy_uploaded = None;
        app.proxy = None;
    }
}

/// Lattice size of the canvas display LUT (colour management, Proof Colors, Gamut Warning).
pub(super) const DISPLAY_LUT: usize = 33;

/// Keep the GPU display LUT under `key` (the document's texture or a preview of it) in step with
/// `doc`'s colour management: document → monitor profile and View › Proof Colors / Gamut
/// Warning (the 32-bit preview is applied by the canvas shader, see [`hdr_preview`]). Returns the canvas `display` mode (0 none — the identity, e.g. sRGB on
/// an sRGB monitor —, 1 LUT, 2 LUT + gamut warning).
pub(super) fn sync_display_lut(app: &mut PhotocraftApp, doc: &photocraft_doc::Document, key: u64, display: Option<u32>) -> u8 {
    let Some(gpu) = app.gpu.clone() else { return 0 };
    let output = display.unwrap_or(0);
    // Rebuild only when anything feeding the LUT changes.
    let sig = app.session.color.display_signature_for(doc, display);
    if let Some((s, mode)) = gpu.display_lut_signature(key, output)
        && s == sig
    {
        return mode;
    }
    let gamut = app.session.color.proof(doc.id).gamut_warning;
    let high = gpu.output_depth() == 10;
    let lut =
        if high { app.session.color.gpu_canvas_lut16f_for(doc, DISPLAY_LUT, display) } else { app.session.color.gpu_canvas_lut_for(doc, DISPLAY_LUT, display) };
    let mode = match lut {
        Ok(Some(bytes)) => {
            gpu.set_display_lut_format(
                key,
                output,
                DISPLAY_LUT as u32,
                Some(&bytes),
                if high { eframe::wgpu::TextureFormat::Rgba16Float } else { eframe::wgpu::TextureFormat::Rgba8Unorm },
            );
            if gamut { 2 } else { 1 }
        }
        Ok(None) => {
            gpu.set_display_lut(key, output, DISPLAY_LUT as u32, None);
            0
        }
        Err(e) => {
            app.ui.status = format!("Color management: {e}");
            gpu.set_display_lut(key, output, DISPLAY_LUT as u32, None);
            0
        }
    };
    gpu.cache_display_lut_signature(key, output, sig, mode);
    mode
}

/// View › 32-bit Preview Options for the GPU canvas shader: (exposure, gamma) when active.
pub(super) fn hdr_preview(app: &PhotocraftApp, doc: &photocraft_doc::Document) -> Option<[f32; 2]> {
    app.session.color.hdr_preview(doc).map(|h| [h.exposure, h.gamma])
}
