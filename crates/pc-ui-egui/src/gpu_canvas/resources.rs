//! GPU resources kept in the renderer's callback resources: pipelines, per-view uniforms, document
//! textures (tiles and mips), display LUTs and scaled-view frames.
use super::*;

// GPU resources

pub(super) type EncodePool = Vec<(wgpu::Buffer, Option<(wgpu::TextureView, wgpu::BindGroup)>)>;

pub(super) struct ScaledSource {
    pub(super) doc: photocraft_doc::Document,
    pub(super) encode_srgb: bool,
    pub(super) generation: u64,
    pub(super) dependencies: HashMap<photocraft_doc::LayerId, u64>,
}

pub(super) struct ScaledFrame {
    pub(super) source: u64,
    pub(super) previous: photocraft_doc::Document,
    pub(super) dependencies: HashMap<photocraft_doc::LayerId, u64>,
    pub(super) generation: u64,
    pub(super) scale: f64,
    pub(super) rect: photocraft_geom::Rect,
    pub(super) textures: DocTextures,
    pub(super) projection: scaled_view::cpu::ProjectionCache,
    /// The exact generation/scale/region stored here could not be projected; show 1x.
    pub(super) projection_failed: bool,
    // Base and scaled surfaces share layer IDs but have different pixels. Each
    // view must retain its own upload cache instead of evicting the 1x pages.
    pub(super) compositor: Option<photocraft_gpu::Compositor>,
    pub(super) uploads: u64,
    pub(super) rebuilds: u64,
}

pub(super) struct Resources {
    pub(super) pixel_previews: std::collections::HashSet<u64>,
    pub(super) scaled_sources: HashMap<u64, ScaledSource>,
    pub(super) scaled_frames: HashMap<u64, ScaledFrame>,
    /// Pipeline construction is device-wide; a document refusal must not poison it.
    pub(super) scaled_compositor_failed: Option<photocraft_gpu::Unsupported>,
    #[cfg(test)]
    pub(super) scaled_compositor_creations: u64,
    #[cfg(test)]
    pub(super) scaled_projection_attempts: u64,
    pub(super) encode_pool: EncodePool,
    pub(super) retry_cpu: bool,
    pub(super) view_bgl: wgpu::BindGroupLayout,
    pub(super) tile_bgl: wgpu::BindGroupLayout,
    pub(super) shadow_pipeline: wgpu::RenderPipeline,
    pub(super) tile_pipeline: wgpu::RenderPipeline,
    pub(super) move_plane_pipeline: wgpu::RenderPipeline,
    pub(super) move_final_pipeline: wgpu::RenderPipeline,
    pub(super) move_format: wgpu::TextureFormat,
    pub(super) move_frames: HashMap<u64, MoveFrame>,
    pub(super) sampler: wgpu::Sampler,
    pub(super) mip_bgl: wgpu::BindGroupLayout,
    pub(super) mip_pipeline: wgpu::RenderPipeline,
    /// `mip_pipeline` and `encode_pipeline` for `Rgba16Float` tiles (when the adapter supports them).
    pub(super) mip_pipeline_high: Option<wgpu::RenderPipeline>,
    pub(super) encode_pipeline_high: Option<wgpu::RenderPipeline>,
    pub(super) mip_sampler: wgpu::Sampler,
    pub(super) docs: HashMap<u64, DocTextures>,
    pub(super) views: HashMap<u64, ViewGpu>,
    pub(super) out_linear: bool,
    pub(super) depth_levels: f32,
    /// The wgpu layer compositor (created on first use).
    pub(super) compositor: Option<photocraft_gpu::Compositor>,
    /// Why the wgpu compositor couldn't be created (then the CPU compositor is used).
    pub(super) compositor_failed: Option<photocraft_gpu::Unsupported>,
    /// The device's health: the paint callback issues no GPU work once it's lost.
    pub(super) health: DeviceHealth,
    /// GPU memory the compositor may hold (`None`: its default), and the document area the
    /// view shows; applied before every composite.
    pub(super) compositor_budget: Option<u64>,
    pub(super) compositor_focus: Option<photocraft_geom::Rect>,
    pub(super) encode_bgl: wgpu::BindGroupLayout,
    pub(super) encode_pipeline: wgpu::RenderPipeline,
    pub(super) lut_bgl: wgpu::BindGroupLayout,
    /// Display LUTs per (document, display) (monitor profile, Proof Colors / Gamut Warning);
    /// `identity_lut` otherwise.
    pub(super) luts: HashMap<(u64, u32), wgpu::BindGroup>,
    /// Signatures belong to their renderer resources, including identity transforms (mode 0).
    pub(super) display_lut_signatures: HashMap<(u64, u32), (u64, u8)>,
    pub(super) identity_lut: wgpu::BindGroup,
    /// Transparency checkerboard and gamut warning colours (Preferences › Transparency & Gamut).
    pub(super) style: CanvasStyle,
}

/// Closed views must stop taking a share of the live compositors' cache budget.
pub(super) fn rebalance_view_budgets(res: &mut Resources) {
    let Some(budget) = res.compositor_budget else { return };
    let share = budget / (res.scaled_frames.len() as u64 + 1);
    if let Some(compositor) = &mut res.compositor {
        compositor.set_memory_budget(share);
    }
    for frame in res.scaled_frames.values_mut() {
        if let Some(compositor) = &mut frame.compositor {
            compositor.set_memory_budget(share);
        }
    }
}

pub(super) fn clear_move_resources(res: &mut Resources) {
    for (key, frame) in res.move_frames.drain() {
        for view in frame.view_keys {
            res.views.remove(&view);
        }
        res.views.remove(&key);
    }
}

pub(super) struct MoveFrame {
    pub(super) size: [u32; 2],
    pub(super) view: wgpu::TextureView,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) sources: Vec<u64>,
    pub(super) view_keys: Vec<u64>,
}

/// Canvas colours from Preferences › Transparency & Gamut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasStyle {
    /// Checker square side in points (0 = no checkerboard: transparency shows white).
    pub checker_square: f32,
    pub checker_light: [f32; 3],
    pub checker_dark: [f32; 3],
    pub gamut_color: [f32; 3],
    /// Gamut warning opacity 0..1.
    pub gamut_opacity: f32,
}

impl Default for CanvasStyle {
    fn default() -> Self {
        Self { checker_square: 8.0, checker_light: [1.0; 3], checker_dark: [0.8; 3], gamut_color: [0.5; 3], gamut_opacity: 1.0 }
    }
}

/// A `size`³ RGBA8 3D texture bound for the canvas shader.
pub(super) fn lut_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bgl: &wgpu::BindGroupLayout,
    size: u32,
    bytes: &[u8],
    format: wgpu::TextureFormat,
) -> wgpu::BindGroup {
    let extent = wgpu::Extent3d { width: size, height: size, depth_or_array_layers: size };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pc_display_lut"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        bytes,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size * texel_bytes(format) as u32), rows_per_image: Some(size) },
        extent,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("pc_display_lut"),
        layout: bgl,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
    })
}

/// The 2³ identity LUT (unused unless a document has none and `display` is set).
pub(super) fn identity_lut_bytes() -> Vec<u8> {
    (0..8).flat_map(|i| [if i & 1 != 0 { 255 } else { 0 }, if i & 2 != 0 { 255 } else { 0 }, if i & 4 != 0 { 255 } else { 0 }, 0]).collect()
}

pub(super) struct ViewGpu {
    pub(super) buffer: wgpu::Buffer,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) frame_data: [f32; VIEW_FLOATS],
}

#[derive(Clone)]
pub(super) struct DocTextures {
    pub(super) size: [u32; 2],
    pub(super) format: wgpu::TextureFormat,
    pub(super) tiles: Vec<Tile>,
}

#[derive(Clone)]
pub(super) struct Tile {
    /// Document-pixel rect: x, y, w, h.
    pub(super) rect: [u32; 4],
    pub(super) texture: wgpu::Texture,
    pub(super) levels: Vec<wgpu::TextureView>,
    pub(super) mip_groups: Vec<wgpu::BindGroup>,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) _uniform: wgpu::Buffer,
}

pub(super) fn uniform_entry(binding: u32, size: u64, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: NonZeroU64::new(size) },
        count: None,
    }
}
pub(super) fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            multisampled: false,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}
pub(super) fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

pub(super) fn f32_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    (vs, fs): (&str, &str),
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState { module, entry_point: Some(vs), buffers: &[], compilation_options: Default::default() },
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
        depth_stencil: None,
        multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl Resources {
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue, target: wgpu::TextureFormat, high: bool) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pc_canvas"),
            source: wgpu::ShaderSource::Wgsl(li_gpu::wgsl::source(CANVAS_WGSL).into()),
        });
        let vis = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let view_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pc_canvas_view"),
            entries: &[uniform_entry(0, VIEW_UNIFORM_SIZE, vis), sampler_entry(1)],
        });
        let tile_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pc_canvas_tile"),
            entries: &[texture_entry(0), uniform_entry(1, TILE_UNIFORM_SIZE, vis)],
        });
        let lut_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                multisampled: false,
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D3,
            },
            count: None,
        };
        let lut_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("pc_display_lut"), entries: &[lut_entry] });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pc_canvas"),
            bind_group_layouts: &[Some(&view_bgl), Some(&tile_bgl), Some(&lut_bgl)],
            immediate_size: 0,
        });
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pc_canvas_shadow"),
            bind_group_layouts: &[Some(&view_bgl)],
            immediate_size: 0,
        });
        // Premultiplied "over", with egui's alpha rule (keeps a transparent window's alpha sane).
        let blend = wgpu::BlendState {
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
        };
        let shadow_pipeline = pipeline(device, "pc_canvas_shadow", &shadow_layout, &module, ("vs_shadow", "fs_shadow"), target, Some(blend));
        // The final shader composites its checker/background and returns alpha=1. Opaque
        // stores are equivalent to "over" and avoid an extra fixed-function precision step.
        let tile_pipeline = pipeline(device, "pc_canvas_tile", &layout, &module, ("vs_tile", "fs_tile"), target, None);
        let move_format = if high { FORMAT_HIGH } else { FORMAT };
        let move_plane_pipeline = pipeline(device, "pc_move_plane", &layout, &module, ("vs_tile", "fs_plane"), move_format, Some(blend));
        let move_final_pipeline = pipeline(device, "pc_move_final", &layout, &module, ("vs_move", "fs_move"), target, None);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pc_canvas_trilinear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let mip_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("pc_mip"), source: wgpu::ShaderSource::Wgsl(MIP_WGSL.into()) });
        let mip_bgl =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("pc_mip"), entries: &[texture_entry(0), sampler_entry(1)] });
        let mip_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("pc_mip"), bind_group_layouts: &[Some(&mip_bgl)], immediate_size: 0 });
        let mip_pipeline = pipeline(device, "pc_mip", &mip_layout, &mip_module, ("vs", "fs"), FORMAT, None);
        let mip_pipeline_high = high.then(|| pipeline(device, "pc_mip_16f", &mip_layout, &mip_module, ("vs", "fs"), FORMAT_HIGH, None));
        let mip_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pc_mip"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Compositor output (straight RGBA32F chunk) -> premultiplied RGBA8 display tile.
        let encode_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pc_encode"),
            source: wgpu::ShaderSource::Wgsl(li_gpu::wgsl::source(ENCODE_WGSL).into()),
        });
        let unfilterable = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                multisampled: false,
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        };
        let encode_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pc_encode"),
            entries: &[unfilterable, uniform_entry(1, 16, wgpu::ShaderStages::FRAGMENT)],
        });
        let encode_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pc_encode"),
            bind_group_layouts: &[Some(&encode_bgl)],
            immediate_size: 0,
        });
        let encode_pipeline = pipeline(device, "pc_encode", &encode_layout, &encode_module, ("vs", "fs"), FORMAT, None);
        let encode_pipeline_high = high.then(|| pipeline(device, "pc_encode_16f", &encode_layout, &encode_module, ("vs", "fs"), FORMAT_HIGH, None));
        let identity_lut = lut_bind_group(device, queue, &lut_bgl, 2, &identity_lut_bytes(), FORMAT);
        Self {
            pixel_previews: Default::default(),
            scaled_sources: HashMap::new(),
            scaled_frames: HashMap::new(),
            scaled_compositor_failed: None,
            #[cfg(test)]
            scaled_compositor_creations: 0,
            #[cfg(test)]
            scaled_projection_attempts: 0,
            encode_pool: Vec::new(),
            retry_cpu: false,
            lut_bgl,
            luts: HashMap::new(),
            display_lut_signatures: HashMap::new(),
            identity_lut,
            style: CanvasStyle::default(),
            view_bgl,
            tile_bgl,
            shadow_pipeline,
            tile_pipeline,
            move_plane_pipeline,
            move_final_pipeline,
            move_format,
            move_frames: HashMap::new(),
            sampler,
            mip_bgl,
            mip_pipeline,
            mip_pipeline_high,
            encode_pipeline_high,
            mip_sampler,
            docs: HashMap::new(),
            views: HashMap::new(),
            out_linear: target.is_srgb(),
            depth_levels: if target == wgpu::TextureFormat::Rgb10a2Unorm { 1023.0 } else { 255.0 },
            compositor: None,
            compositor_failed: None,
            health: DeviceHealth::new(),
            compositor_budget: None,
            compositor_focus: None,
            encode_bgl,
            encode_pipeline,
        }
    }

    /// The pipeline encoding compositor output into a `format` tile.
    pub(super) fn encode_pipeline(&self, format: wgpu::TextureFormat) -> Option<&wgpu::RenderPipeline> {
        if format == FORMAT_HIGH { self.encode_pipeline_high.as_ref() } else { Some(&self.encode_pipeline) }
    }

    /// The mip-chain pipeline for `format` tiles.
    pub(super) fn mip_pipeline(&self, format: wgpu::TextureFormat) -> Option<&wgpu::RenderPipeline> {
        if format == FORMAT_HIGH { self.mip_pipeline_high.as_ref() } else { Some(&self.mip_pipeline) }
    }
}

pub(super) fn mip_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

/// Mip levels no larger than this on either side are redrawn whole on every edit rather than just
/// their damage (see [`mip_damage`]). Every corrupted level measured was at most 64 px; this
/// leaves a factor of two.
const WHOLE_MIP_SIDE: u32 = 128;

/// The rect (x0, y0, x1, y1) of a `lw`×`lh` mip level to redraw when the given rect changed in the level
/// above: that damage halved and padded by one texel for odd-size rounding, or the whole level
/// when it is at most [`WHOLE_MIP_SIDE`] on either side. On AMD's Vulkan driver a pass that draws
/// only part of a level of 64 px or less corrupts texels outside its scissor, at the level's
/// top-left (RX 6900 XT, driver 26.8.1; DX12 on the same card is fine). Such a level costs next
/// to nothing to redraw, so this holds for every adapter; the levels after it are smaller still,
/// so the rest of the chain is redrawn whole too.
pub(super) fn mip_damage([x0, y0, x1, y1]: [u32; 4], lw: u32, lh: u32) -> [u32; 4] {
    if lw.max(lh) <= WHOLE_MIP_SIDE {
        return [0, 0, lw, lh];
    }
    [(x0 / 2).saturating_sub(1), (y0 / 2).saturating_sub(1), (x1.div_ceil(2) + 1).min(lw), (y1.div_ceil(2) + 1).min(lh)]
}

/// A buffer holding `contents`, like `DeviceExt::create_buffer_init` but without its panic when
/// the device was lost (an invalid buffer has no mapped range): the buffer is then just invalid.
pub(super) fn init_buffer(device: &wgpu::Device, label: &str, contents: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    let size = (contents.len() as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT).max(wgpu::COPY_BUFFER_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: true });
    if let Ok(mut view) = buffer.slice(..).get_mapped_range_mut()
        && view.len() >= contents.len()
    {
        view.slice(..contents.len()).copy_from_slice(contents);
    }
    buffer.unmap();
    buffer
}

impl DocTextures {
    pub(super) fn new(device: &wgpu::Device, res: &Resources, size: [u32; 2], tile: u32, format: wgpu::TextureFormat) -> Self {
        Self::region(device, res, size, tile, format, photocraft_geom::Rect::from_xywh(0, 0, size[0], size[1]), 0)
    }

    pub(super) fn region(
        device: &wgpu::Device,
        res: &Resources,
        size: [u32; 2],
        tile: u32,
        format: wgpu::TextureFormat,
        region: photocraft_geom::Rect,
        gutter: u32,
    ) -> Self {
        let mut tiles = Vec::new();
        for ty in (region.y0 as u32..region.y1 as u32).step_by(tile as usize) {
            for tx in (region.x0 as u32..region.x1 as u32).step_by(tile as usize) {
                let (cw, ch) = (tile.min(region.x1 as u32 - tx), tile.min(region.y1 as u32 - ty));
                let (sx, sy) = (tx.saturating_sub(gutter), ty.saturating_sub(gutter));
                let (w, h) = ((tx + cw + gutter).min(size[0]) - sx, (ty + ch + gutter).min(size[1]) - sy);
                let levels_n = mip_count(w, h);
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("pc_canvas_tile"),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: levels_n,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let levels: Vec<_> = (0..levels_n)
                    .map(|l| texture.create_view(&wgpu::TextureViewDescriptor { base_mip_level: l, mip_level_count: Some(1), ..Default::default() }))
                    .collect();
                let mip_groups = levels
                    .iter()
                    .take(levels.len().saturating_sub(1))
                    .map(|view| {
                        device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("pc_mip"),
                            layout: &res.mip_bgl,
                            entries: &[
                                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&res.mip_sampler) },
                            ],
                        })
                    })
                    .collect();
                let full = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let uniform = init_buffer(
                    device,
                    "pc_canvas_tile",
                    &f32_bytes(&[tx as f32, ty as f32, cw as f32, ch as f32, sx as f32, sy as f32, w as f32, h as f32]),
                    wgpu::BufferUsages::UNIFORM,
                );
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("pc_canvas_tile"),
                    layout: &res.tile_bgl,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&full) },
                        wgpu::BindGroupEntry { binding: 1, resource: uniform.as_entire_binding() },
                    ],
                });
                tiles.push(Tile { rect: [sx, sy, w, h], texture, levels, mip_groups, bind_group, _uniform: uniform });
            }
        }
        Self { size, format, tiles }
    }

    /// Write `size` pixels at document `origin` from `texels` (in the texture format, row stride
    /// `stride` pixels) into every overlapping tile's level 0.
    pub(super) fn write(&self, queue: &wgpu::Queue, origin: [u32; 2], size: [u32; 2], stride: u32, texels: &[u8]) {
        let bpp = texel_bytes(self.format);
        let (x0, y0, x1, y1) = (origin[0], origin[1], origin[0] + size[0], origin[1] + size[1]);
        for t in &self.tiles {
            let [tx, ty, tw, th] = t.rect;
            let (ix0, iy0, ix1, iy1) = (x0.max(tx), y0.max(ty), x1.min(tx + tw), y1.min(ty + th));
            if ix0 >= ix1 || iy0 >= iy1 {
                continue;
            }
            let offset = ((iy0 - y0) as u64 * stride as u64 + (ix0 - x0) as u64) * bpp;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &t.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: ix0 - tx, y: iy0 - ty, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                texels,
                wgpu::TexelCopyBufferLayout { offset, bytes_per_row: Some(stride * bpp as u32), rows_per_image: Some(iy1 - iy0) },
                wgpu::Extent3d { width: ix1 - ix0, height: iy1 - iy0, depth_or_array_layers: 1 },
            );
        }
    }

    /// Rebuild the mip chain inside document rect `r` (x0, y0, x1, y1) with 2×2 box filtering.
    pub(super) fn regenerate_mips(&self, device: &wgpu::Device, queue: &wgpu::Queue, res: &Resources, r: [u32; 4]) {
        let Some(mip_pipeline) = res.mip_pipeline(self.format) else { return };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pc_mips") });
        self.encode_mips(&mut encoder, mip_pipeline, r);
        queue.submit([encoder.finish()]);
    }

    pub(super) fn encode_mips(&self, encoder: &mut wgpu::CommandEncoder, mip_pipeline: &wgpu::RenderPipeline, r: [u32; 4]) {
        for t in &self.tiles {
            let [tx, ty, tw, th] = t.rect;
            let (mut x0, mut y0, mut x1, mut y1) = (r[0].max(tx), r[1].max(ty), r[2].min(tx + tw), r[3].min(ty + th));
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            (x0, y0, x1, y1) = (x0 - tx, y0 - ty, x1 - tx, y1 - ty);
            for level in 1..t.levels.len() {
                let (lw, lh) = ((tw >> level).max(1), (th >> level).max(1));
                [x0, y0, x1, y1] = mip_damage([x0, y0, x1, y1], lw, lh);
                let bg = &t.mip_groups[level - 1];
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("pc_mip"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &t.levels[level],
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(mip_pipeline);
                pass.set_bind_group(0, bg, &[]);
                pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
                pass.draw(0..3, 0..1);
            }
        }
    }
}
