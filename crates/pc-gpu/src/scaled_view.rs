//! Scaled view execution uses the real compositor's passes and chunk sink.
//! The caller owns the detached pc-compose projection and display texture encoding.
use super::wgpu;
use photocraft_doc::Document;
use photocraft_geom::Rect;

pub fn render_scaled(
    compositor: &mut super::Compositor,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    projected: &Document,
    visible_device_rect: Rect,
    sink: impl FnMut(&mut wgpu::CommandEncoder, super::ChunkOut<'_>),
) -> Result<super::Stats, super::Unsupported> {
    if u64::from(visible_device_rect.width()) * u64::from(visible_device_rect.height()) > 16_777_216 {
        return Err(super::Unsupported("scaled view exceeds visible pixel budget".into()));
    }
    compositor.render(device, queue, projected, visible_device_rect, sink)
}
