//! Per-frame paint callback that draws one document view (and its scaled projection).
use super::*;

// Per-frame callback

pub(super) struct CanvasCallback {
    pub(super) rect: egui::Rect,
    pub(super) params: ViewParams,
}

/// Sampling mode: 0 trilinear, 1 bilinear, 2 anti-aliased nearest.
pub(super) fn filter_mode(scale: f32) -> (f32, f32) {
    if scale < 0.999 {
        (0.0, (-scale.log2()).max(0.0))
    } else if (scale - scale.round()).abs() < 1e-3 || scale >= 2.0 {
        (2.0, 0.0)
    } else {
        (1.0, 0.0)
    }
}

impl CanvasCallback {
    pub(super) fn prepare_scaled(&self, device: &wgpu::Device, queue: &wgpu::Queue, screen: &ScreenDescriptor, res: &mut Resources) -> f64 {
        // Scaled projection/compositing is acceleration work; the existing CPU-composited
        // document texture still presents at any zoom while the shared circuit is paused.
        if !res.health.can_render() {
            return 1.0;
        }
        use photocraft_geom::Rect;
        use scaled_view::cpu::{ProjectionCache, ViewRequest, scaled_rect};
        if res.pixel_previews.contains(&self.params.doc) {
            return 1.0;
        }
        let physical = f64::from(self.params.zoom) * f64::from(screen.pixels_per_point);
        if !physical.is_finite() || physical <= 1.0 || !self.params.center.iter().all(|v| v.is_finite()) {
            return 1.0;
        }
        let Some(source) = res.scaled_sources.get(&self.params.doc) else { return 1.0 };
        if !source
            .doc
            .walk()
            .iter()
            .any(|(_, _, l)| l.visible && matches!(l.content, photocraft_doc::LayerContent::Shape(_) | photocraft_doc::LayerContent::Text(_)))
        {
            return 1.0;
        }
        let visible = self.camera(screen.pixels_per_point).document_bounds(self.rect * screen.pixels_per_point);
        let visible = Rect::new(visible.left().floor() as i32, visible.top().floor() as i32, visible.right().ceil() as i32, visible.bottom().ceil() as i32)
            .intersect(&source.doc.bounds());
        let request = ViewRequest {
            document: source.doc.id,
            revision: source.generation,
            view_key: self.params.view_key,
            logical_zoom: f64::from(self.params.zoom),
            pixels_per_point: f64::from(screen.pixels_per_point),
            visible,
            display_generation: 0,
            dependencies: HashMap::new(),
        };
        let Ok(scale) = request.scale() else { return 1.0 };
        let (Ok(mut rect), Ok(canvas)) = (scaled_rect(visible, scale), scaled_rect(source.doc.bounds(), scale)) else { return 1.0 };
        if rect.is_empty() {
            return 1.0;
        }
        let required = rect.inflate(1).intersect(&canvas);
        // Global tile cells plus one cell of sampling apron. Panning within this region is warm.
        const SIDE: i32 = 256;
        rect = Rect::new(
            rect.x0.div_euclid(SIDE) * SIDE,
            rect.y0.div_euclid(SIDE) * SIDE,
            (-(-rect.x1).div_euclid(SIDE)) * SIDE,
            (-(-rect.y1).div_euclid(SIDE)) * SIDE,
        )
        .inflate(SIDE)
        .intersect(&canvas);
        if u64::from(rect.width()) * u64::from(rect.height()) > scaled_view::cpu::MAX_VIEW_PIXELS {
            return 1.0;
        }
        let key = self.params.view_key;
        let cache_count = res.scaled_frames.len() + usize::from(!res.scaled_frames.contains_key(&key)) + 1;
        if let Some(b) = res.compositor_budget {
            let share = b / cache_count as u64;
            if let Some(comp) = &mut res.compositor {
                comp.set_memory_budget(share);
            }
            for frame in res.scaled_frames.values_mut() {
                if let Some(comp) = &mut frame.compositor {
                    comp.set_memory_budget(share);
                }
            }
        }
        let mut frame = res.scaled_frames.remove(&key);
        if let Some(f) = &frame
            && f.source == self.params.doc
            && f.generation == source.generation
            && f.scale == scale
            && f.rect.contains_rect(&required)
            && (!f.projection_failed || f.rect == rect)
        {
            let ready_scale = if f.projection_failed { 1.0 } else { scale };
            if let Some(frame) = frame.take() {
                res.scaled_frames.insert(key, frame);
            }
            return ready_scale;
        }
        let source_doc = source.doc.clone();
        let dependencies = source.dependencies.clone();
        let generation = source.generation;
        let encode_srgb = source.encode_srgb;
        // Keep composite coverage until final screen quantization, including 8-bit docs.
        let format = if res.encode_pipeline_high.is_some() { FORMAT_HIGH } else { FORMAT };
        let fresh = frame.as_ref().is_none_or(|f| {
            f.source != self.params.doc
                || f.scale != scale
                || !f.rect.contains_rect(&required)
                || f.projection_failed && f.rect != rect
                || f.textures.format != format
                || f.textures.size != [canvas.width(), canvas.height()]
        });
        if fresh {
            // Scale/viewport targets change much more often than the device or source.
            // Keep per-view pipelines and let resident/projection keys rebind new pixels.
            let retained = frame.take().filter(|old| old.source == self.params.doc);
            let (projection, compositor, uploads) =
                retained.map_or_else(|| (ProjectionCache::default(), None, 0), |old| (old.projection, old.compositor, old.uploads));
            let textures = DocTextures::region(device, res, [canvas.width(), canvas.height()], SIDE as u32, format, rect, 1);
            frame = Some(ScaledFrame {
                source: self.params.doc,
                previous: source_doc.clone(),
                dependencies: HashMap::new(),
                generation: 0,
                scale,
                rect,
                textures,
                rebuilds: projection.rebuilds,
                projection,
                projection_failed: false,
                compositor,
                uploads,
            });
        }
        let Some(mut frame) = frame else { return 1.0 };
        let mut changed = if fresh || frame.projection_failed {
            frame.rect
        } else {
            scaled_view::damage(&frame.previous, &source_doc).and_then(|r| scaled_rect(r, scale).ok()).unwrap_or(frame.rect).intersect(&frame.rect)
        };
        let dependencies_changed = !fresh && frame.dependencies != dependencies;
        if dependencies_changed {
            for (_, _, layer) in source_doc.walk() {
                if frame.dependencies.get(&layer.id) != dependencies.get(&layer.id) {
                    let r = scaled_view::source_change_bounds(layer, source_doc.bounds()).and_then(|r| scaled_rect(r, scale).ok()).unwrap_or(frame.rect);
                    changed = changed.union(&r.intersect(&frame.rect));
                }
            }
        }
        if changed.is_empty() && !dependencies_changed {
            frame.dependencies = dependencies;
            frame.generation = generation;
            frame.previous = source_doc;
            res.scaled_frames.insert(key, frame);
            return scale;
        }
        // Every displayed tile owns a one-pixel sampling gutter. Update both copies at
        // shared edges; only the core rectangle is drawn, so translucent tiles do not overlap.
        changed = changed.inflate(1).intersect(&canvas);
        let inverse = Rect::new(
            (f64::from(frame.rect.x0) / scale).floor() as i32,
            (f64::from(frame.rect.y0) / scale).floor() as i32,
            (f64::from(frame.rect.x1) / scale).ceil() as i32,
            (f64::from(frame.rect.y1) / scale).ceil() as i32,
        );
        #[cfg(test)]
        {
            res.scaled_projection_attempts += 1;
        }
        let projected = match frame.projection.project_at(&source_doc, inverse, scale, &dependencies, scaled_view::project) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("scaled canvas: {e}");
                frame.projection_failed = true;
                frame.dependencies = dependencies;
                frame.generation = generation;
                frame.previous = source_doc;
                res.scaled_frames.insert(key, frame);
                return 1.0;
            }
        };
        if dependencies_changed {
            // A font/master/appearance generation can expand ink while the persisted 1x
            // cache stays unchanged (including previously missing/empty glyphs). Include
            // the newly projected coverage, not just the old source bounds.
            let padding = projected
                .walk()
                .iter()
                .filter(|(_, _, l)| photocraft_compose::effects::has_effects(l))
                .map(|(_, _, l)| photocraft_compose::effects::margin(l))
                .fold(1i32, i32::saturating_add);
            for (_, _, layer) in projected.walk() {
                if frame.dependencies.get(&layer.id) != dependencies.get(&layer.id) {
                    let r = photocraft_compose::change_bounds(layer, projected.bounds()).unwrap_or(frame.rect);
                    changed = changed.union(&r.inflate(padding).intersect(&frame.rect.inflate(1).intersect(&canvas)));
                }
            }
        }
        let pipe = res.encode_pipeline(format).cloned();
        let mut compositor = frame.compositor.take().or_else(|| {
            if res.scaled_compositor_failed.is_some() {
                return None;
            }
            #[cfg(test)]
            {
                res.scaled_compositor_creations += 1;
            }
            match photocraft_gpu::Compositor::try_new_with_format(device, wgpu::TextureFormat::Rgba32Float) {
                Ok(compositor) => Some(compositor),
                Err(error) => {
                    log::warn!("scaled canvas pipelines: {error}; using CPU rendering");
                    res.scaled_compositor_failed = Some(error);
                    None
                }
            }
        });
        let mut ok = false;
        if let (Some(comp), Some(pipe)) = (&mut compositor, pipe) {
            comp.set_health(res.health.clone());
            if let Some(b) = res.compositor_budget {
                comp.set_memory_budget(b / cache_count as u64);
            }
            let bgl = &res.encode_bgl;
            let textures = &frame.textures;
            let result = scaled_view::gpu::render_scaled(comp, device, queue, &projected, changed, |encoder, out| {
                for tile in &textures.tiles {
                    let [tx, ty, tw, th] = tile.rect.map(|v| v as i32);
                    let r = out.rect.intersect(&Rect::from_xywh(tx, ty, tw as u32, th as u32));
                    if r.is_empty() {
                        continue;
                    }
                    let offset = [out.rect.x0 - tx, out.rect.y0 - ty, i32::from(encode_srgb), i32::from(format == FORMAT_HIGH)];
                    let ub =
                        init_buffer(device, "pc_scaled_encode", &offset.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>(), wgpu::BufferUsages::UNIFORM);
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("pc_scaled_encode"),
                        layout: bgl,
                        entries: &[
                            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(out.view) },
                            wgpu::BindGroupEntry { binding: 1, resource: ub.as_entire_binding() },
                        ],
                    });
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("pc_scaled_encode"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &tile.levels[0],
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    pass.set_pipeline(&pipe);
                    pass.set_bind_group(0, &bg, &[]);
                    pass.set_scissor_rect((r.x0 - tx) as u32, (r.y0 - ty) as u32, r.width(), r.height());
                    pass.draw(0..3, 0..1);
                }
            });
            match result {
                Ok(s) => {
                    frame.uploads += s.tiles_uploaded as u64;
                    ok = true;
                }
                Err(e) => log::warn!("scaled canvas GPU fallback: {e}"),
            }
        }
        frame.compositor = compositor;
        if !ok {
            let buffer = photocraft_compose::render(&projected, changed);
            let mut pixels = buffer.px;
            if encode_srgb {
                for p in &mut pixels {
                    for c in p.iter_mut().take(3) {
                        *c = li_color::convert::linear_to_srgb(*c);
                    }
                }
            }
            frame.textures.write(queue, [changed.x0 as u32, changed.y0 as u32], [changed.width(), changed.height()], changed.width(), &texels(format, &pixels));
        }
        frame.textures.regenerate_mips(device, queue, res, [changed.x0 as u32, changed.y0 as u32, changed.x1 as u32, changed.y1 as u32]);
        frame.rebuilds = frame.projection.rebuilds;
        frame.projection_failed = false;
        frame.dependencies = dependencies;
        frame.generation = generation;
        frame.previous = source_doc;
        res.scaled_frames.insert(key, frame);
        scale
    }

    pub(super) fn camera(&self, ppp: f32) -> crate::view_camera::Camera {
        let p = &self.params;
        crate::view_camera::Camera::new(self.rect, p.zoom, p.center, p.flip, p.rotation).device_pixels(ppp)
    }

    /// Shared device mapping, including legacy pixel-aligned upright placement.
    pub(super) fn placement(&self, ppp: f32) -> ([f32; 2], f32) {
        let camera = self.camera(ppp);
        let origin = camera.origin();
        ([origin.x, origin.y], camera.scale())
    }

    pub(super) fn uniforms(&self, screen: [u32; 2], ppp: f32, out_linear: bool, style: &CanvasStyle) -> [f32; VIEW_FLOATS] {
        let p = &self.params;
        let (origin, scale) = self.placement(ppp);
        let [sin, cos] = self.camera(ppp).sin_cos();
        let (mode, lod) = filter_mode(scale);
        let grid = if p.pixel_grid && p.zoom >= 8.0 { 0.16 } else { 0.0 };
        let square = if style.checker_square > 0.0 { (style.checker_square * ppp).round().max(1.0) } else { 0.0 };
        let (l, d, g) = (style.checker_light, style.checker_dark, style.gamut_color);
        // 32-bit preview: linear-light gain 2^exposure (0 = off) and 1 / gamma.
        let (gain, inv_gamma) = match p.hdr {
            Some([e, gm]) if e.is_finite() && gm.is_finite() && gm > 0.0 => (2f32.powf(e.clamp(-20.0, 20.0)), 1.0 / gm.max(0.01)),
            _ => (0.0, 1.0),
        };
        [
            screen[0] as f32,
            screen[1] as f32,
            scale,
            ppp,
            origin[0],
            origin[1],
            p.doc_size[0] as f32,
            p.doc_size[1] as f32,
            mode,
            lod,
            grid,
            square,
            (self.rect.min.x * ppp).round(),
            (self.rect.min.y * ppp).round(),
            p.display as f32,
            if out_linear { 1.0 } else { 0.0 },
            l[0],
            l[1],
            l[2],
            style.gamut_opacity,
            d[0],
            d[1],
            d[2],
            gain,
            g[0],
            g[1],
            g[2],
            inv_gamma,
            if p.flip { -1.0 } else { 1.0 },
            1.0,
            sin,
            cos,
            0.0,
            0.0,
            0.0,
            0.0,
        ]
    }
}

impl CallbackTrait for CanvasCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(res) = resources.get_mut::<Resources>() else { return Vec::new() };
        if !res.health.can_present() {
            return Vec::new();
        }
        let scaled = self.prepare_scaled(device, queue, screen, res);
        let mut data = self.uniforms(screen.size_in_pixels, screen.pixels_per_point, res.out_linear, &res.style);
        data[29] = scaled as f32;
        let high = res.docs.get(&self.params.doc).is_some_and(|d| d.format == FORMAT_HIGH);
        data[32] = if high || self.params.display > 0 || self.params.hdr.is_some() { res.depth_levels } else { 0.0 };
        let (mode, lod) = filter_mode(data[2] / data[29]);
        // Buckets oversample by at most a quarter octave. Sample level zero with its
        // shared gutter instead of mixing independently generated per-tile mip edges.
        data[8] = if res.pixel_previews.contains(&self.params.doc) {
            3.0
        } else if scaled > 1.0 && mode == 0.0 {
            1.0
        } else {
            mode
        };
        data[9] = if scaled > 1.0 { 0.0 } else { lod };
        let (bgl, sampler) = (&res.view_bgl, &res.sampler);
        let v = res.views.entry(self.params.view_key).or_insert_with(|| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pc_canvas_view"),
                size: VIEW_UNIFORM_SIZE,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("pc_canvas_view"),
                layout: bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                ],
            });
            ViewGpu { buffer, bind_group, frame_data: [f32::NAN; VIEW_FLOATS] }
        });
        if v.frame_data != data {
            queue.write_buffer(&v.buffer, 0, &f32_bytes(&data));
            v.frame_data = data;
        }
        Vec::new()
    }

    fn paint(&self, info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &CallbackResources) {
        let Some(res) = resources.get::<Resources>() else { return };
        if !res.health.can_present() {
            return;
        }
        let Some(view) = res.views.get(&self.params.view_key) else { return };
        let [sw, sh] = info.screen_size_px;
        let clip = info.clip_rect_in_pixels();
        if clip.width_px <= 0 || clip.height_px <= 0 || sw == 0 || sh == 0 {
            return;
        }
        // Work in whole-screen pixel coordinates; the scissor does the clipping.
        pass.set_viewport(0.0, 0.0, sw as f32, sh as f32, 0.0, 1.0);
        pass.set_scissor_rect(clip.left_px.max(0) as u32, clip.top_px.max(0) as u32, clip.width_px as u32, clip.height_px as u32);
        pass.set_bind_group(0, &view.bind_group, &[]);
        if self.params.shadow {
            pass.set_pipeline(&res.shadow_pipeline);
            pass.draw(0..6, 0..1);
        }
        let scaled = res.scaled_frames.get(&self.params.view_key).filter(|f| f.source == self.params.doc && view.frame_data[29] > 1.0);
        let Some(doc) = scaled.map(|f| &f.textures).or_else(|| res.docs.get(&self.params.doc)) else { return };
        if scaled.is_none() && doc.size != self.params.doc_size {
            return;
        }
        let camera = self.camera(info.pixels_per_point);
        let (cx0, cy0) = (clip.left_px as f32, clip.top_px as f32);
        let (cx1, cy1) = (cx0 + clip.width_px as f32, cy0 + clip.height_px as f32);
        pass.set_pipeline(&res.tile_pipeline);
        pass.set_bind_group(2, res.luts.get(&(self.params.doc, self.params.output)).unwrap_or(&res.identity_lut), &[]);
        for t in &doc.tiles {
            let raster_scale = view.frame_data[29];
            let [tx, ty, tw, th] = t.rect.map(|v| v as f32 / raster_scale);
            let bounds = camera.screen_bounds(egui::Rect::from_min_size(egui::pos2(tx, ty), egui::vec2(tw, th)));
            if bounds.right() < cx0 || bounds.bottom() < cy0 || bounds.left() > cx1 || bounds.top() > cy1 {
                continue;
            }
            pass.set_bind_group(1, &t.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
    }
}
