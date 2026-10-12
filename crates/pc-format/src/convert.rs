//! Document ⇄ manifest conversion. Binary data is routed through a
//! [`Sink`] (save) or [`Fetch`] (load) keyed by BLAKE3 hash.

use std::sync::Arc;

use photocraft_color::{PixelFormat, SampleType};
use photocraft_doc::{
    AlphaChannel, CompAppearance, CompLayerState, DocId, Document, Effects, FillCache, Group, Layer, LayerComp, LayerContent, LayerId, LayerMask, Metadata,
    NamedPath, Pattern, ShapeLayer, SmartObject, SmartSource, TextLayer,
};
use photocraft_geom::{TILE_SIZE, TileCoord};
use photocraft_raster::{Surface, Tile, decode_pixel, encode_pixel};

use crate::manifest::*;
use crate::{FormatError, Result};

/// Deepest group nesting a bundle may hold — the one nesting limit every consumer of the layer
/// tree shares; see [`photocraft_doc::MAX_GROUP_DEPTH`].
pub use photocraft_doc::MAX_GROUP_DEPTH;

fn too_deep() -> FormatError {
    FormatError::LimitExceeded(format!("layer groups nested deeper than {MAX_GROUP_DEPTH}"))
}

/// Refuses to save a document the loader would reject for its group nesting.
pub(crate) fn check_nesting(layers: &[Layer]) -> Result<()> {
    // Bounded recursion: it stops at the first layer past the limit.
    fn too_deep_at(layers: &[Layer], depth: usize) -> bool {
        (depth > MAX_GROUP_DEPTH && !layers.is_empty())
            || layers.iter().any(|l| matches!(&l.content, LayerContent::Group(g) if too_deep_at(&g.children, depth + 1)))
    }
    if too_deep_at(layers, 0) { Err(too_deep()) } else { Ok(()) }
}

pub(crate) trait Sink {
    /// Register a tile, returning its hash.
    fn tile(&mut self, format: PixelFormat, tile: &Arc<Tile>) -> Hash;
    fn blob(&mut self, data: &Arc<Vec<u8>>) -> Hash;
}

pub(crate) trait Fetch {
    /// Canonical (little-endian) tile bytes of the given expected length.
    fn tile(&mut self, hash: &str, len: usize) -> Result<Vec<u8>>;
    fn blob(&mut self, hash: &str) -> Result<Arc<Vec<u8>>>;
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(H[(b >> 4) as usize] as char);
        s.push(H[(b & 15) as usize] as char);
    }
    s
}

pub(crate) fn unhex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return Err(FormatError::corrupt(format!("bad hex `{s}`")));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2).unwrap_or("x"), 16).map_err(|_| FormatError::corrupt(format!("bad hex `{s}`"))))
        .collect()
}

fn key4(s: &str) -> Result<[u8; 4]> {
    unhex(s)?.try_into().map_err(|_| FormatError::corrupt(format!("bad 4-byte key `{s}`")))
}

pub(crate) fn is_valid_hash(h: &str) -> bool {
    h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Convert native-endian sample bytes to little-endian in place (and back:
/// the swap is an involution).
pub(crate) fn swap_to_le(bytes: &mut [u8], sample: SampleType) {
    if cfg!(target_endian = "big") {
        match sample {
            SampleType::U8 => {}
            SampleType::U16 => bytes.as_chunks_mut::<2>().0.iter_mut().for_each(|c| c.reverse()),
            SampleType::F32 => bytes.as_chunks_mut::<4>().0.iter_mut().for_each(|c| c.reverse()),
        }
    }
}

pub(crate) fn tile_len(format: &PixelFormat) -> usize {
    (TILE_SIZE * TILE_SIZE) as usize * format.bytes_per_pixel()
}

// ---------------------------------------------------------------------------
// Save
// ---------------------------------------------------------------------------

fn surface_m(s: &Surface, sink: &mut dyn Sink) -> SurfaceM {
    let f = s.format();
    let mut dp = vec![0u8; f.bytes_per_pixel()];
    encode_pixel(&f, &s.default_pixel(), &mut dp);
    swap_to_le(&mut dp, f.sample);
    SurfaceM { format: f, default: hex(&dp), tiles: s.tiles().map(|(c, t)| TileRef { tx: c.tx, ty: c.ty, hash: sink.tile(f, t) }).collect() }
}

fn opt_blob(b: &Option<Arc<Vec<u8>>>, sink: &mut dyn Sink) -> Option<Hash> {
    b.as_ref().map(|b| sink.blob(b))
}

fn video_m(v: &photocraft_doc::VideoData, sink: &mut dyn Sink) -> crate::manifest::VideoDataM {
    crate::manifest::VideoDataM {
        frames: v.frames.iter().map(|f| surface_m(f, sink)).collect(),
        source: v.source.clone(),
        fps: v.fps,
        show_altered: v.show_altered,
    }
}

fn layer_m(l: &Layer, sink: &mut dyn Sink) -> LayerM {
    let content = match &l.content {
        LayerContent::Symbol(s) => ContentM::Symbol { symbol: s.symbol, transform: s.transform, cache: s.cache.as_ref().map(|c| surface_m(c, sink)) },
        LayerContent::Raster(s) => ContentM::Raster { surface: surface_m(s, sink) },
        LayerContent::Group(g) => ContentM::Group {
            isolation: g.isolation,
            children: g.children.iter().map(|c| layer_m(c, sink)).collect(),
            expanded: g.expanded,
            artboard: g.artboard.clone(),
        },
        LayerContent::Adjustment(a) => ContentM::Adjustment { adjustment: a.clone() },
        LayerContent::Fill(f) => ContentM::Fill { fill: f.clone() },
        LayerContent::Text(t) => ContentM::Text {
            text: t.text.clone(),
            font_family: t.font_family.clone(),
            size_pt: t.size_pt,
            color: t.color,
            transform: t.transform,
            cache: t.cache.as_ref().map(|s| surface_m(s, sink)),
            psd_raw: opt_blob(&t.psd_raw, sink),
            runs: t.runs.clone(),
            paragraphs: t.paragraphs.clone(),
            shape: t.shape,
            orientation: t.orientation,
            antialias: t.antialias,
            warp: t.warp.clone(),
        },
        LayerContent::Shape(s) => ContentM::Shape {
            fill: s.fill.clone(),
            cache: s.cache.as_ref().map(|c| surface_m(c, sink)),
            psd_raw: opt_blob(&s.psd_raw, sink),
            path: s.path.clone(),
            stroke: s.stroke.clone(),
            live: s.live.clone(),
        },
        LayerContent::Smart(s) => ContentM::Smart {
            source: match &s.source {
                SmartSource::Embedded { file_name, bytes } => SmartSourceM::Embedded { file_name: file_name.clone(), blob: sink.blob(bytes) },
                SmartSource::Linked { path } => SmartSourceM::Linked { path: path.clone() },
            },
            transform: s.transform,
            smart_filters: s.smart_filters.clone(),
            cache: s.cache.as_ref().map(|c| surface_m(c, sink)),
            psd_raw: opt_blob(&s.psd_raw, sink),
            filters_enabled: s.filters_enabled,
            filter_mask: s.filter_mask.as_ref().map(|m| MaskM {
                surface: surface_m(&m.surface, sink),
                enabled: m.enabled,
                linked: m.linked,
                density: m.density,
                feather: m.feather,
            }),
            warp: s.warp.clone(),
            develop: s.develop.clone(),
            stack_mode: s.stack_mode,
            perspective: s.perspective,
        },
    };
    LayerM {
        vector: l.vector.clone(),
        id: l.id.0,
        name: l.name.clone(),
        visible: l.visible,
        locks: l.locks,
        blend: l.blend,
        opacity: l.opacity,
        fill_opacity: l.fill_opacity,
        clipped: l.clipped,
        mask: l.mask.as_ref().map(|m| MaskM {
            surface: surface_m(&m.surface, sink),
            enabled: m.enabled,
            linked: m.linked,
            density: m.density,
            feather: m.feather,
        }),
        vector_mask: l.vector_mask.clone(),
        effects: EffectsM {
            enabled: l.effects.enabled,
            items: l.effects.items.clone(),
            psd_raw: opt_blob(&l.effects.psd_raw, sink),
            reference: l.effects.reference,
        },
        label: l.label,
        content,
        psd_blocks: l.psd_blocks.iter().map(|(k, d)| (hex(k), sink.blob(d))).collect(),
        psd_id: l.psd_id,
        fill_cache: l.fill_cache.as_ref().map(|fc| FillCacheM { fill: fc.fill.clone(), surface: surface_m(&fc.surface, sink) }),
        link_group: l.link_group,
        excluded_channels: l.excluded_channels,
        blend_if: l.blend_if.clone(),
        video: l.video.as_ref().map(|v| video_m(v, sink)),
        generation: l.generation.clone(),
    }
}

fn channel_m(c: &AlphaChannel, sink: &mut dyn Sink) -> ChannelM {
    // Destructured so a new `AlphaChannel` field fails to compile here until it is saved.
    let AlphaChannel { name, surface, spot, color, opacity, indicates } = c;
    ChannelM { name: name.clone(), surface: surface_m(surface, sink), spot: *spot, color: *color, opacity: *opacity, indicates: *indicates }
}

pub(crate) fn doc_m(d: &Document, sink: &mut dyn Sink) -> DocM {
    DocM {
        graphic_styles: d.graphic_styles.clone(),
        swatches: d.swatches.clone(),
        drawing_scale: d.drawing_scale,
        design_units: d.design_units,
        symbols: d.symbols.iter().map(|s| SymbolDefM { id: s.id, name: s.name.clone(), layers: s.layers.iter().map(|l| layer_m(l, sink)).collect() }).collect(),
        id: d.id.0,
        name: d.name.clone(),
        size: d.size,
        resolution_dpi: d.resolution_dpi,
        mode: d.mode,
        depth: d.depth,
        icc_profile: opt_blob(&d.icc_profile, sink),
        layers: d.layers.iter().map(|l| layer_m(l, sink)).collect(),
        channels: d.channels.iter().map(|c| channel_m(c, sink)).collect(),
        guides: d.guides.clone(),
        selection: d.selection.as_ref().map(|s| surface_m(s, sink)),
        metadata: MetadataM {
            xmp: d.metadata.xmp.clone(),
            exif: opt_blob(&d.metadata.exif, sink),
            psd_resources: d.metadata.psd_resources.iter().map(|(id, n, b)| (*id, n.clone(), sink.blob(b))).collect(),
            psd_global_blocks: d.metadata.psd_global_blocks.iter().map(|(s, k, b)| (hex(s), hex(k), sink.blob(b))).collect(),
        },
        global_light: d.global_light,
        paths: d.paths.iter().map(|p| NamedPathM { name: p.name.clone(), path: p.path.clone(), psd_raw: opt_blob(&p.psd_raw, sink) }).collect(),
        work_path: d.work_path.clone(),
        clipping_path: d.clipping_path.clone(),
        quick_mask: d.quick_mask.as_ref().map(|c| channel_m(c, sink)),
        patterns: d
            .patterns
            .iter()
            .map(|p| PatternM { id: p.id.clone(), name: p.name.clone(), width: p.width, height: p.height, surface: surface_m(&p.surface, sink) })
            .collect(),
        color_table: d.color_table.clone(),
        duotone: d.duotone.clone(),
        variables: d.variables.clone(),
        timeline: d.timeline.clone(),
        layer_comps: d.layer_comps.iter().map(|c| comp_m(c, sink)).collect(),
        last_applied_comp: d.last_applied_comp,
        last_document_state: d.last_document_state.as_ref().map(|c| comp_m(c, sink)),
        measurement: d.measurement.clone(),
        notes: d.notes.clone(),
        text_styles: d.text_styles.clone(),
        slices: d.slices.clone(),
    }
}

fn effects_m(e: &Effects, sink: &mut dyn Sink) -> EffectsM {
    EffectsM { enabled: e.enabled, items: e.items.clone(), psd_raw: opt_blob(&e.psd_raw, sink), reference: e.reference }
}

fn comp_m(c: &LayerComp, sink: &mut dyn Sink) -> LayerCompM {
    // Destructured so a new `LayerComp` field fails to compile here until it is saved.
    let LayerComp { id, name, comment, apply_visibility, apply_position, apply_appearance, states } = c;
    LayerCompM {
        id: *id,
        name: name.clone(),
        comment: comment.clone(),
        apply_visibility: *apply_visibility,
        apply_position: *apply_position,
        apply_appearance: *apply_appearance,
        states: states
            .iter()
            .map(|s| {
                let CompLayerState { layer, visible, position, appearance } = s;
                CompStateM {
                    layer: layer.0,
                    visible: *visible,
                    position: *position,
                    appearance: appearance.as_ref().map(|a| {
                        let CompAppearance { blend, opacity, fill_opacity, effects } = a;
                        CompAppearanceM { blend: *blend, opacity: *opacity, fill_opacity: *fill_opacity, effects: effects_m(effects, sink) }
                    }),
                }
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Load
// ---------------------------------------------------------------------------

pub(crate) struct Loader<'a> {
    pub fetch: &'a mut dyn Fetch,
    pub preserve_ids: bool,
    pub max_id: u64,
    /// Stored layer id → loaded id (differs when ids are remapped), for layer comp states.
    pub id_map: std::collections::HashMap<u64, LayerId>,
}

impl Loader<'_> {
    fn video(&mut self, m: &crate::manifest::VideoDataM) -> Result<photocraft_doc::VideoData> {
        let frames = m.frames.iter().map(|f| self.surface(f)).collect::<Result<Vec<_>>>()?;
        Ok(photocraft_doc::VideoData { frames, source: m.source.clone(), fps: m.fps, show_altered: m.show_altered })
    }

    fn surface(&mut self, m: &SurfaceM) -> Result<Surface> {
        let f = m.format;
        let mut dp = unhex(&m.default)?;
        if dp.len() != f.bytes_per_pixel() {
            return Err(FormatError::corrupt("default pixel has wrong size for its format"));
        }
        swap_to_le(&mut dp, f.sample);
        let mut s = Surface::with_default(f, &decode_pixel(&f, &dp));
        let len = tile_len(&f);
        for t in &m.tiles {
            if t.tx.checked_mul(photocraft_geom::TILE_SIZE).and_then(|x| x.checked_add(photocraft_geom::TILE_SIZE)).is_none()
                || t.ty.checked_mul(photocraft_geom::TILE_SIZE).and_then(|y| y.checked_add(photocraft_geom::TILE_SIZE)).is_none()
            {
                return Err(FormatError::corrupt("tile coordinates exceed document bounds"));
            }
            let c = TileCoord::new(t.tx, t.ty);
            if s.tile(c).is_some() {
                return Err(FormatError::corrupt(format!("duplicate tile ({}, {})", t.tx, t.ty)));
            }
            let mut bytes = self.fetch.tile(&t.hash, len)?;
            swap_to_le(&mut bytes, f.sample);
            s.tile_mut(c).bytes_mut().copy_from_slice(&bytes);
        }
        Ok(s)
    }

    fn opt_surface(&mut self, m: &Option<SurfaceM>) -> Result<Option<Surface>> {
        m.as_ref().map(|s| self.surface(s)).transpose()
    }

    fn opt_blob(&mut self, h: &Option<Hash>) -> Result<Option<Arc<Vec<u8>>>> {
        h.as_ref().map(|h| self.fetch.blob(h)).transpose()
    }

    fn id(&mut self, raw: u64) -> LayerId {
        let id = if self.preserve_ids {
            self.max_id = self.max_id.max(raw);
            LayerId(raw)
        } else {
            LayerId::fresh()
        };
        self.id_map.insert(raw, id);
        id
    }

    fn comp(&mut self, m: &LayerCompM) -> Result<LayerComp> {
        let mut states = Vec::with_capacity(m.states.len());
        for s in &m.states {
            let appearance = match &s.appearance {
                Some(a) => Some(CompAppearance {
                    blend: a.blend,
                    opacity: a.opacity,
                    fill_opacity: a.fill_opacity,
                    effects: Effects {
                        enabled: a.effects.enabled,
                        items: a.effects.items.clone(),
                        psd_raw: self.opt_blob(&a.effects.psd_raw)?,
                        reference: a.effects.reference,
                    },
                }),
                None => None,
            };
            // States of layers deleted before the save keep their (now dangling) id, which
            // `layerComp.updateWarnings` reports; it must not collide with a live layer.
            let layer = self.id_map.get(&s.layer).copied().unwrap_or(LayerId(u64::MAX - s.layer.min(u64::MAX / 2)));
            states.push(CompLayerState { layer, visible: s.visible, position: s.position, appearance });
        }
        Ok(LayerComp {
            id: m.id,
            name: m.name.clone(),
            comment: m.comment.clone(),
            apply_visibility: m.apply_visibility,
            apply_position: m.apply_position,
            apply_appearance: m.apply_appearance,
            states,
        })
    }

    fn layer(&mut self, m: &LayerM, depth: usize) -> Result<Layer> {
        if depth > MAX_GROUP_DEPTH {
            return Err(too_deep());
        }
        // Children first, from this small frame: the conversion's large locals stay off the stack
        // that grows with each nesting level.
        let mut children = Vec::new();
        if let ContentM::Group { children: kids, .. } = &m.content {
            children.reserve(kids.len());
            for c in kids {
                children.push(self.layer(c, depth + 1)?);
            }
        }
        self.layer_with(m, children)
    }

    /// One layer, given its already converted group children.
    #[inline(never)]
    fn layer_with(&mut self, m: &LayerM, children: Vec<Layer>) -> Result<Layer> {
        let content = match &m.content {
            ContentM::Symbol { symbol, transform, cache } => {
                LayerContent::Symbol(photocraft_doc::SymbolInstance { symbol: *symbol, transform: *transform, cache: self.opt_surface(cache)? })
            }
            ContentM::Raster { surface } => LayerContent::Raster(self.surface(surface)?),
            ContentM::Group { expanded, artboard, isolation, .. } => {
                LayerContent::Group(Group { artboard: artboard.clone(), children, expanded: *expanded, isolation: *isolation })
            }
            ContentM::Adjustment { adjustment } => LayerContent::Adjustment(adjustment.clone()),
            ContentM::Fill { fill } => LayerContent::Fill(fill.clone()),
            ContentM::Text { text, font_family, size_pt, color, transform, cache, psd_raw, runs, paragraphs, shape, orientation, antialias, warp } => {
                LayerContent::Text(TextLayer {
                    text: text.clone(),
                    font_family: font_family.clone(),
                    size_pt: *size_pt,
                    color: *color,
                    transform: *transform,
                    cache: self.opt_surface(cache)?,
                    psd_raw: self.opt_blob(psd_raw)?,
                    runs: runs.clone(),
                    paragraphs: paragraphs.clone(),
                    shape: *shape,
                    orientation: *orientation,
                    antialias: *antialias,
                    warp: warp.clone(),
                })
            }
            ContentM::Shape { fill, cache, psd_raw, path, stroke, live } => LayerContent::Shape(ShapeLayer {
                path: path.clone(),
                fill: fill.clone(),
                stroke: stroke.clone(),
                live: live.clone(),
                cache: self.opt_surface(cache)?,
                psd_raw: self.opt_blob(psd_raw)?,
            }),
            ContentM::Smart { source, transform, smart_filters, cache, psd_raw, filters_enabled, filter_mask, warp, stack_mode, perspective, develop } => {
                LayerContent::Smart(SmartObject {
                    source: match source {
                        SmartSourceM::Embedded { file_name, blob } => SmartSource::Embedded { file_name: file_name.clone(), bytes: self.fetch.blob(blob)? },
                        SmartSourceM::Linked { path } => SmartSource::Linked { path: path.clone() },
                    },
                    transform: *transform,
                    smart_filters: smart_filters.clone(),
                    cache: self.opt_surface(cache)?,
                    psd_raw: self.opt_blob(psd_raw)?,
                    filters_enabled: *filters_enabled,
                    filter_mask: match filter_mask {
                        Some(mm) => Some(LayerMask {
                            surface: self.surface(&mm.surface)?,
                            enabled: mm.enabled,
                            linked: mm.linked,
                            density: mm.density,
                            feather: mm.feather,
                        }),
                        None => None,
                    },
                    warp: warp.clone().filter(|w| w.mesh.as_ref().is_none_or(|m| m.is_valid())),
                    stack_mode: *stack_mode,
                    perspective: perspective.filter(|p| p.iter().all(|v| v.is_finite())),
                    develop: develop.clone(),
                })
            }
        };
        let mask = match &m.mask {
            Some(mm) => {
                Some(LayerMask { surface: self.surface(&mm.surface)?, enabled: mm.enabled, linked: mm.linked, density: mm.density, feather: mm.feather })
            }
            None => None,
        };
        let fill_cache = match &m.fill_cache {
            Some(fc) => Some(FillCache { fill: fc.fill.clone(), surface: self.surface(&fc.surface)? }),
            None => None,
        };
        let mut psd_blocks = Vec::with_capacity(m.psd_blocks.len());
        for (k, h) in &m.psd_blocks {
            psd_blocks.push((key4(k)?, self.fetch.blob(h)?));
        }
        Ok(Layer {
            vector: m.vector.clone(),
            id: self.id(m.id),
            name: m.name.clone(),
            visible: m.visible,
            locks: m.locks,
            blend: m.blend,
            opacity: m.opacity,
            fill_opacity: m.fill_opacity,
            clipped: m.clipped,
            mask,
            vector_mask: m.vector_mask.clone(),
            effects: Effects {
                enabled: m.effects.enabled,
                items: m.effects.items.clone(),
                psd_raw: self.opt_blob(&m.effects.psd_raw)?,
                reference: m.effects.reference,
            },
            label: m.label,
            content,
            psd_blocks,
            psd_id: m.psd_id,
            fill_cache,
            link_group: m.link_group,
            excluded_channels: m.excluded_channels,
            blend_if: m.blend_if.clone(),
            video: m.video.as_ref().map(|v| self.video(v)).transpose()?,
            generation: m.generation.clone(),
        })
    }

    pub(crate) fn document(&mut self, m: &DocM) -> Result<Document> {
        validate_symbols_manifest(m)?;
        let layers = m.layers.iter().map(|l| self.layer(l, 0)).collect::<Result<Vec<_>>>()?;
        let mut channels = Vec::with_capacity(m.channels.len());
        for c in &m.channels {
            channels.push(self.channel(c)?);
        }
        let quick_mask = m.quick_mask.as_ref().map(|c| self.channel(c)).transpose()?;
        let mut md = Metadata { xmp: m.metadata.xmp.clone(), exif: self.opt_blob(&m.metadata.exif)?, psd_resources: Vec::new(), psd_global_blocks: Vec::new() };
        for (id, n, h) in &m.metadata.psd_resources {
            md.psd_resources.push((*id, n.clone(), self.fetch.blob(h)?));
        }
        for (s, k, h) in &m.metadata.psd_global_blocks {
            md.psd_global_blocks.push((key4(s)?, key4(k)?, self.fetch.blob(h)?));
        }
        let mut paths = Vec::with_capacity(m.paths.len());
        for p in &m.paths {
            paths.push(NamedPath { name: p.name.clone(), path: p.path.clone(), psd_raw: self.opt_blob(&p.psd_raw)? });
        }
        let mut patterns = Vec::with_capacity(m.patterns.len());
        for p in &m.patterns {
            patterns.push(Pattern { id: p.id.clone(), name: p.name.clone(), width: p.width, height: p.height, surface: self.surface(&p.surface)? });
        }
        let layer_comps = m.layer_comps.iter().map(|c| self.comp(c)).collect::<Result<Vec<_>>>()?;
        let last_document_state = m.last_document_state.as_ref().map(|c| self.comp(c)).transpose()?;
        // Layer-based slices follow their layer through id remapping; a dangling one becomes a
        // user slice (its rect stays where it was).
        let mut slices = m.slices.clone();
        for sl in &mut slices.list {
            if let Some(l) = sl.layer {
                sl.layer = self.id_map.get(&l.0).copied();
                if sl.layer.is_none() && sl.origin == photocraft_doc::SliceOrigin::Layer {
                    sl.origin = photocraft_doc::SliceOrigin::User;
                }
            }
        }
        let id = if self.preserve_ids {
            self.max_id = self.max_id.max(m.id);
            DocId(m.id)
        } else {
            DocId::fresh()
        };
        let mut doc = Document {
            graphic_styles: m.graphic_styles.clone(),
            swatches: m.swatches.clone(),
            drawing_scale: m.drawing_scale,
            design_units: m.design_units,
            symbols: m
                .symbols
                .iter()
                .map(|s| {
                    Ok(photocraft_doc::SymbolDef {
                        id: s.id,
                        name: s.name.clone(),
                        layers: s.layers.iter().map(|l| self.layer(l, 0)).collect::<Result<Vec<_>>>()?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            id,
            name: m.name.clone(),
            size: m.size,
            resolution_dpi: m.resolution_dpi,
            mode: m.mode,
            depth: m.depth,
            icc_profile: self.opt_blob(&m.icc_profile)?,
            layers,
            channels,
            guides: m.guides.clone(),
            selection: self.opt_surface(&m.selection)?,
            metadata: md,
            global_light: m.global_light,
            paths,
            work_path: m.work_path.clone(),
            clipping_path: m.clipping_path.clone(),
            quick_mask,
            patterns,
            color_table: m.color_table.clone(),
            duotone: m.duotone.clone(),
            variables: m.variables.clone(),
            timeline: m.timeline.clone(),
            layer_comps,
            last_applied_comp: m.last_applied_comp,
            last_document_state,
            measurement: m.measurement.clone(),
            notes: m.notes.clone(),
            text_styles: m.text_styles.clone(),
            slices,
        };
        if !doc.symbols.is_empty() || doc.walk().iter().any(|(_, _, l)| matches!(l.content, LayerContent::Symbol(_))) {
            photocraft_doc::vector_design::symbol_validation::document(&doc).map_err(|e| FormatError::corrupt(e.to_string()))?;
        }
        doc.swatches.remap_layers(&self.id_map).map_err(FormatError::corrupt)?;
        photocraft_doc::vector_design::validation::document(&doc).map_err(FormatError::corrupt)?;
        Ok(doc)
    }

    fn channel(&mut self, c: &ChannelM) -> Result<AlphaChannel> {
        Ok(AlphaChannel { name: c.name.clone(), surface: self.surface(&c.surface)?, spot: c.spot, color: c.color, opacity: c.opacity, indicates: c.indicates })
    }
}

/// Advance the global id counter past `max` so ids loaded from a file never
/// collide with ids minted later in this process. Returns `false` if `max`
/// is implausibly far ahead (the caller then remaps ids instead).
pub(crate) fn reserve_ids_through(max: u64) -> bool {
    let probe = LayerId::fresh().0;
    if max < probe {
        return true;
    }
    if max - probe > 10_000_000 {
        return false;
    }
    photocraft_doc::ensure_ids_above(max);
    true
}

/// Symbol variant preflight, before allocating caches or remapping any stored layer IDs.
fn validate_symbols_manifest(doc: &DocM) -> Result<()> {
    use photocraft_doc::vector_design::symbol_validation as validation;
    use std::collections::{BTreeMap, BTreeSet};
    fn collect(layers: &[LayerM], ids: &mut BTreeSet<u64>, refs: &mut Vec<photocraft_doc::SymbolId>, depth: usize) -> Result<()> {
        if depth > MAX_GROUP_DEPTH && !layers.is_empty() {
            return Err(too_deep());
        }
        for l in layers {
            if l.id == 0 || !ids.insert(l.id) {
                return Err(FormatError::corrupt("duplicate or zero layer ID"));
            }
            if ids.len() > validation::MAX_OBJECTS {
                return Err(FormatError::LimitExceeded("symbol object budget".into()));
            }
            match &l.content {
                ContentM::Group { children, .. } => collect(children, ids, refs, depth + 1)?,
                ContentM::Symbol { symbol, transform, .. } => {
                    validation::affine(*transform).map_err(|e| FormatError::corrupt(e.to_string()))?;
                    refs.push(*symbol);
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn has_instances(layers: &[LayerM]) -> Result<bool> {
        let mut pending = vec![(layers, 0usize)];
        let mut found = false;
        while let Some((layers, depth)) = pending.pop() {
            if depth > MAX_GROUP_DEPTH && !layers.is_empty() {
                return Err(too_deep());
            }
            for layer in layers {
                match &layer.content {
                    ContentM::Symbol { .. } => found = true,
                    ContentM::Group { children, .. } => pending.push((children.as_slice(), depth + 1)),
                    _ => {}
                }
            }
        }
        Ok(found)
    }
    if doc.symbols.is_empty() && !has_instances(&doc.layers)? {
        return Ok(());
    }
    let mut graph = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut root = Vec::new();
    collect(&doc.layers, &mut ids, &mut root, 0)?;
    for d in &doc.symbols {
        if d.id.0 == 0 || graph.contains_key(&d.id.0) {
            return Err(FormatError::corrupt("duplicate or zero symbol ID"));
        }
        let mut refs = Vec::new();
        collect(&d.layers, &mut ids, &mut refs, 0)?;
        graph.insert(d.id.0, refs);
    }
    validation::graph_depths(&graph).map_err(|e| FormatError::corrupt(e.to_string()))?;
    for id in root {
        if !graph.contains_key(&id.0) {
            return Err(FormatError::corrupt("dangling symbol instance"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod symbol_tests {
    use super::*;
    use crate::{LoadOptions, PcraftWriter, SaveOptions, load_from_bytes, load_from_bytes_with, load_path, load_path_with, save_to_bytes};
    use photocraft_doc::{Affine, ColorMode, SampleType, Size, SymbolDef, SymbolId, SymbolInstance};
    fn fixture() -> Document {
        let mut d = Document::new("Symbols native", Size::new(24, 24), ColorMode::Rgb, SampleType::U16);
        let mut art = Layer::raster("Art", d.pixel_format());
        art.surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(2, 3, 9, 11), &[0.1, 0.2, 0.7, 0.6]);
        let cache = art.surface().unwrap().clone();
        d.symbols.push(SymbolDef { id: SymbolId(17), name: "Master".into(), layers: vec![art] });
        d.layers.push(Layer::new(
            "Instance",
            LayerContent::Symbol(SymbolInstance { symbol: SymbolId(17), transform: Affine::translate(2., 3.), cache: Some(cache) }),
        ));
        d
    }
    #[test]
    fn symbols_cached_instances_zip_directory_and_layer_id_remap() {
        let d = fixture();
        let options = SaveOptions::default();
        let bytes = save_to_bytes(&d, &options).unwrap();
        assert_eq!(load_from_bytes(&bytes).unwrap(), d);
        let remap = load_from_bytes_with(&bytes, &LoadOptions { preserve_ids: false, ..Default::default() }).unwrap();
        assert_ne!(remap.layers[0].id, d.layers[0].id);
        assert_ne!(remap.symbols[0].layers[0].id, d.symbols[0].layers[0].id);
        assert_eq!(remap.layers[0].content, d.layers[0].content);
        assert_eq!(remap.symbols[0].id, d.symbols[0].id);
        let dir = std::env::current_dir().unwrap().join("target").join(format!("symbols-native-{}-{}", std::process::id(), d.id.0));
        std::fs::create_dir_all(&dir).unwrap();
        let mut writer = PcraftWriter::new();
        writer.save_dir(&d, &dir, &options).unwrap();
        writer.save_dir(&d, &dir, &options).unwrap();
        assert_eq!(load_path(&dir).unwrap(), d);
        let remap = load_path_with(&dir, &LoadOptions { preserve_ids: false, ..Default::default() }).unwrap();
        assert_ne!(remap.symbols[0].layers[0].id, d.symbols[0].layers[0].id);
        assert_eq!(remap.layers[0].content, d.layers[0].content);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn symbols_native_rejects_cycles_duplicate_ids_and_dangling_instances_before_remap() {
        let d = fixture();
        let bytes = save_to_bytes(&d, &Default::default()).unwrap();
        let m = crate::read_manifest(&bytes).unwrap();
        for n in 0..6 {
            let mut manifest = m.clone();
            match n {
                0 => manifest.document.symbols.push(manifest.document.symbols[0].clone()),
                1 => manifest.document.symbols[0].layers[0].id = manifest.document.layers[0].id,
                2 => {
                    if let ContentM::Symbol { symbol, .. } = &mut manifest.document.layers[0].content {
                        *symbol = SymbolId(999);
                    }
                }
                3 => manifest.document.symbols[0].layers.push(LayerM { id: 991, ..manifest.document.layers[0].clone() }),
                _ => {
                    if let ContentM::Symbol { transform, .. } = &mut manifest.document.layers[0].content {
                        *transform = if n == 4 { Affine::scale(0.) } else { Affine::translate(1e20, 0.) };
                    }
                }
            }
            let mut zip = crate::zip::ZipWriter::new();
            zip.add("manifest.json", &serde_json::to_vec(&manifest).unwrap()).unwrap();
            let bytes = zip.finish().unwrap();
            let e = load_from_bytes_with(&bytes, &LoadOptions { preserve_ids: false, ..Default::default() }).unwrap_err();
            assert!(matches!(e, FormatError::Corrupt(_)), "{e:?}");
            let cause = ["symbol ID", "layer ID", "dangling", "cycle", "transform", "transform"][n];
            assert!(e.to_string().contains(cause), "graph error must precede the deliberately missing tile payloads: {e}");
        }
    }
    #[test]
    fn symbols_frozen_v1_fixture_loads_and_v3_writer_is_guarded() {
        let d = crate::load_from_bytes(include_bytes!("../tests/fixtures/vector-v1.pcraft")).unwrap();
        assert_eq!(d.layers.len(), 2);
        assert!(d.symbols.is_empty());
        assert!(d.layers[0].vector_mask.is_some());
        assert!(!d.paths.is_empty());
        let m = crate::read_manifest(&save_to_bytes(&d, &Default::default()).unwrap()).unwrap();
        assert_eq!(m.format_version, 3);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod hostile_tile_tests {
    use super::*;
    struct NoFetch;
    impl Fetch for NoFetch {
        fn tile(&mut self, _: &str, _: usize) -> Result<Vec<u8>> {
            Err(FormatError::corrupt("tile fetch must not be reached"))
        }
        fn blob(&mut self, _: &str) -> Result<Arc<Vec<u8>>> {
            Err(FormatError::corrupt("unused"))
        }
    }
    #[test]
    fn extreme_tile_coordinates_are_rejected_before_fetch() {
        let mut fetch = NoFetch;
        let mut loader = Loader { fetch: &mut fetch, preserve_ids: true, max_id: 0, id_map: Default::default() };
        for tx in [i32::MIN, i32::MAX] {
            let surface = SurfaceM { format: PixelFormat::RGBA8, default: "00000000".into(), tiles: vec![TileRef { tx, ty: 0, hash: "0".repeat(64) }] };
            assert!(loader.surface(&surface).err().is_some_and(|e| e.to_string().contains("coordinates")));
        }
    }
}
