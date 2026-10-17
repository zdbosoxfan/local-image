//! Paint callback of the Move tool's fast path: cached layer-stack planes drawn at their offsets.
use super::*;

pub(super) struct MoveCallback {
    pub(super) base: CanvasCallback,
    pub(super) planes: Vec<MovePlane>,
}

impl MoveCallback {
    pub(super) fn callbacks(&self) -> Vec<CanvasCallback> {
        self.planes
            .iter()
            .map(|plane| {
                let mut params = self.base.params;
                params.doc = plane.key;
                params.doc_size = plane.size;
                params.center = [params.center[0] - (plane.origin[0] + plane.offset[0]) as f32, params.center[1] - (plane.origin[1] + plane.offset[1]) as f32];
                params.view_key = egui::Id::new(("move-plane", params.view_key, plane.key)).value();
                params.shadow = false;
                params.display = 0;
                params.hdr = None;
                params.pixel_grid = false;
                CanvasCallback { rect: self.base.rect, params }
            })
            .collect()
    }
}

impl CallbackTrait for MoveCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let _ = self.base.prepare(device, queue, screen, encoder, resources);
        let callbacks = self.callbacks();
        for cb in &callbacks {
            let _ = cb.prepare(device, queue, screen, encoder, resources);
        }
        let Some(res) = resources.get_mut::<Resources>() else { return Vec::new() };
        let size = screen.size_in_pixels;
        if !res.health.can_present() || size.contains(&0) {
            return Vec::new();
        }
        let key = self.base.params.view_key;
        if res.move_frames.get(&key).is_none_or(|f| f.size != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("pc_move_viewport"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: res.move_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let r = [0.0, 0.0, size[0] as f32, size[1] as f32];
            let tile = init_buffer(device, "pc_move_viewport_tile", &f32_bytes(&[r, r].concat()), wgpu::BufferUsages::UNIFORM);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("pc_move_viewport"),
                layout: &res.tile_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: tile.as_entire_binding() },
                ],
            });
            if let Some(old) = res.move_frames.remove(&key) {
                for key in old.view_keys {
                    if !callbacks.iter().any(|cb| cb.params.view_key == key) {
                        res.views.remove(&key);
                    }
                }
            }
            res.move_frames.insert(key, MoveFrame { size, view, bind_group, sources: Vec::new(), view_keys: Vec::new() });
        }
        let new_keys: Vec<_> = callbacks.iter().map(|cb| cb.params.view_key).collect();
        if let Some(old) = res.move_frames.get(&key) {
            for old_key in &old.view_keys {
                if !new_keys.contains(old_key) {
                    res.views.remove(old_key);
                }
            }
        }
        let Some(frame) = res.move_frames.get_mut(&key) else { return Vec::new() };
        frame.sources = self.planes.iter().map(|p| p.key).collect();
        frame.view_keys = new_keys;
        let ppp = screen.pixels_per_point;
        let rect = self.base.rect;
        let x0 = (rect.left() * ppp).round().clamp(0.0, size[0] as f32) as u32;
        let y0 = (rect.top() * ppp).round().clamp(0.0, size[1] as f32) as u32;
        let x1 = (rect.right() * ppp).round().clamp(x0 as f32, size[0] as f32) as u32;
        let y1 = (rect.bottom() * ppp).round().clamp(y0 as f32, size[1] as f32) as u32;
        if x0 == x1 || y0 == y1 {
            return Vec::new();
        }
        let mut command = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pc_move_viewport") });
        {
            let mut pass = command.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pc_move_viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame.view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&res.move_plane_pipeline);
            pass.set_viewport(0.0, 0.0, size[0] as f32, size[1] as f32, 0.0, 1.0);
            pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
            pass.set_bind_group(2, &res.identity_lut, &[]);
            for cb in &callbacks {
                let Some(view) = res.views.get(&cb.params.view_key) else { continue };
                let scaled = res.scaled_frames.get(&cb.params.view_key).filter(|f| f.source == cb.params.doc && view.frame_data[29] > 1.0);
                let Some(doc) = scaled.map(|f| &f.textures).or_else(|| res.docs.get(&cb.params.doc)) else { continue };
                pass.set_bind_group(0, &view.bind_group, &[]);
                let camera = cb.camera(ppp);
                for tile in &doc.tiles {
                    let [tx, ty, tw, th] = tile.rect.map(|v| v as f32 / view.frame_data[29]);
                    let bounds = camera.screen_bounds(egui::Rect::from_min_size(egui::pos2(tx, ty), egui::vec2(tw, th)));
                    if bounds.right() < x0 as f32 || bounds.bottom() < y0 as f32 || bounds.left() > x1 as f32 || bounds.top() > y1 as f32 {
                        continue;
                    }
                    pass.set_bind_group(1, &tile.bind_group, &[]);
                    pass.draw(0..6, 0..1);
                }
            }
        }
        vec![command.finish()]
    }

    fn paint(&self, info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &CallbackResources) {
        let Some(res) = resources.get::<Resources>() else { return };
        if !res.health.can_present() {
            return;
        }
        let key = self.base.params.view_key;
        let (Some(view), Some(frame)) = (res.views.get(&key), res.move_frames.get(&key)) else { return };
        let clip = info.clip_rect_in_pixels();
        if clip.width_px <= 0 || clip.height_px <= 0 {
            return;
        }
        pass.set_viewport(0.0, 0.0, info.screen_size_px[0] as f32, info.screen_size_px[1] as f32, 0.0, 1.0);
        pass.set_scissor_rect(clip.left_px.max(0) as u32, clip.top_px.max(0) as u32, clip.width_px as u32, clip.height_px as u32);
        pass.set_bind_group(0, &view.bind_group, &[]);
        if self.base.params.shadow {
            pass.set_pipeline(&res.shadow_pipeline);
            pass.draw(0..6, 0..1);
        }
        pass.set_pipeline(&res.move_final_pipeline);
        pass.set_bind_group(1, &frame.bind_group, &[]);
        pass.set_bind_group(2, res.luts.get(&(self.base.params.doc, self.base.params.output)).unwrap_or(&res.identity_lut), &[]);
        pass.draw(0..6, 0..1);
    }
}
