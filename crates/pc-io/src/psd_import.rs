//! PSD/PSB → [`Document`].

use std::sync::Arc;

use photocraft_color::{BlendMode, ColorMode, PixelFormat, SampleType};
use photocraft_doc::{AlphaChannel, Document, Effects, FillCache, Group, Layer, LayerContent, LayerMask, ShapeLayer, SmartObject, SmartSource, TextLayer};
use photocraft_geom::{Rect, Size, TILE_SIZE};
use photocraft_psd::layer::{CHANNEL_REAL_USER_MASK, CHANNEL_TRANSPARENCY, CHANNEL_USER_MASK};
use photocraft_psd::resources::ids;
use photocraft_psd::tagged::BlockData;
use photocraft_psd::{ColorMode as PsdMode, LayerNode, LayerRecord, PsdFile, TaggedBlock};
use photocraft_raster::Surface;

use crate::adjust_map::{self, ADJUSTMENT_KEYS};
use crate::blocks;
use crate::pixels::{interleave, max_sample, sample_for_depth, zero_sample};

/// Resources that are mapped to document fields or regenerated on export.
pub(crate) const MAPPED_RESOURCES: [u16; 19] = [
    ids::GLOBAL_ANGLE,
    ids::GLOBAL_ALTITUDE,
    ids::RESOLUTION_INFO,
    ids::ICC_PROFILE,
    ids::XMP,
    ids::EXIF,
    ids::VERSION_INFO,
    ids::THUMBNAIL,
    ids::THUMBNAIL_PS4,
    ids::LAYER_STATE,
    ids::LAYER_GROUP_INFO,
    1032, // grid and guides
    1045, // unicode alpha names
    1006, // pascal alpha names
    1069, // layer selection ids
    1072, // layer group(s) enabled id
    crate::channel_map::DISPLAY_INFO,
    crate::channel_map::DISPLAY_INFO_OLD,
    crate::channel_map::QUICK_MASK_INFO,
];

/// Blocks regenerated from document fields on export; not kept in
/// `Layer::psd_blocks`.
pub(crate) const REGENERATED: [&[u8; 4]; 10] = [b"luni", b"lyid", b"lsct", b"lsdk", b"iOpa", b"lspf", b"lclr", b"lfx2", b"lfxs", &crate::psd_generation::KEY];

pub(crate) struct Ctx<'a> {
    pub file: &'a PsdFile,
    pub fmt: PixelFormat,
    pub mask_fmt: PixelFormat,
    pub cc: usize,
    pub cmyk: bool,
    pub warnings: Vec<String>,
    /// Document resolution (type sizes are converted to points with it).
    pub dpi: f32,
    /// The parsed `Txt2` block (type settings EngineData lacks, e.g. optical kerning).
    pub txt2: Option<photocraft_text::engine_data::Value>,
    /// Smart-filter caches from the global `FEid`/`FXid` blocks (filter masks, by placed id).
    pub filter_effects: Vec<photocraft_psd::filter_effects::FilterEffectsItem>,
    pub filter_effects_error: Option<String>,
    /// Cancellation and progress for background opens (checked per layer record).
    pub ctl: lightcraft_sysmem::jobs::Interrupt<'a>,
    /// Layer records decoded so far, out of `total` (progress).
    pub done: usize,
    pub total: usize,
    pub remaining: usize,
    pub fatal: Option<String>,
    pub generation_remaining: usize,
}

fn doc_mode(m: PsdMode) -> Option<ColorMode> {
    Some(match m {
        PsdMode::Bitmap => ColorMode::Bitmap,
        PsdMode::Grayscale => ColorMode::Grayscale,
        PsdMode::Indexed => ColorMode::Indexed,
        PsdMode::Rgb => ColorMode::Rgb,
        PsdMode::Cmyk => ColorMode::Cmyk,
        PsdMode::Multichannel => ColorMode::Multichannel,
        PsdMode::Duotone => ColorMode::Duotone,
        PsdMode::Lab => ColorMode::Lab,
        PsdMode::Unknown(_) => return None,
    })
}

// Real-mask metadata without a -3 channel still selects the synthetic -2 mask.
fn selected_real_mask(rec: &LayerRecord) -> Option<photocraft_psd::RealMask> {
    rec.channel(CHANNEL_REAL_USER_MASK)?;
    rec.layer_mask()?.real
}

/// The fill of a plain shape layer (fill block + vector path, no stroke) to import as a fill layer
/// with a vector mask: when its mask parameters give the vector mask a density below 100 % or a
/// feather, or when it stores no pixels and fills with a pattern.
fn soft_shape_fill(rec: &LayerRecord, has_vector: bool, fill_key: Option<&[u8; 4]>) -> Option<photocraft_doc::Fill> {
    if !has_vector || rec.block(b"vstk").is_some() || rec.block(b"vscg").is_some() {
        return None;
    }
    let k = fill_key?;
    let fill = rec.block(k).and_then(|b| blocks::parse_fill(k, &b.data))?;
    let p = rec.layer_mask().and_then(|m| m.parameters);
    let soft = p.is_some_and(|p| p.vector_density.is_some_and(|d| d < 255) || p.vector_feather.is_some_and(|f| f > 0.0));
    // Without stored pixels a pattern-filled shape cannot be rasterized on its own (the
    // compositor resolves the document's patterns for fill layers).
    let unrendered_pattern = (rec.rect.is_empty() || rec.rect.size().is_err()) && matches!(fill, photocraft_doc::Fill::Pattern { .. });
    (soft || unrendered_pattern).then_some(fill)
}

impl Ctx<'_> {
    fn charge(&mut self, w: usize, h: usize, channels: usize) -> bool {
        if self.fatal.is_some() {
            return false;
        }
        let cost =
            w.saturating_add(128).saturating_mul(h.saturating_add(128)).saturating_mul(channels).saturating_mul(self.fmt.sample.bytes()).saturating_mul(3);
        if cost > self.remaining {
            self.fatal = Some("PSD import exceeds the decoded image memory limit; no document was opened".into());
            return false;
        }
        self.remaining -= cost;
        true
    }

    fn warn(&mut self, s: impl Into<String>) {
        self.warnings.push(s.into());
    }

    fn channel_plane(&mut self, rec: &LayerRecord, id: i16, name: &str) -> Option<Vec<u8>> {
        if self.fatal.is_some() {
            return None;
        }
        rec.channel(id)?;
        match rec.decode_channel(id, self.file.header.depth, self.file.header.version) {
            Ok(p) => Some(p),
            Err(e) => {
                self.fatal = Some(format!("layer \"{name}\": channel {id} could not be decoded ({e}); no document was opened"));
                None
            }
        }
    }

    fn record_surface(&mut self, rec: &LayerRecord, name: &str) -> Surface {
        let r = rec.rect;
        if r.is_empty() || r.size().is_err() || rec.channels.is_empty() {
            return Surface::new(self.fmt);
        }
        let (w, h) = r.size().unwrap_or((0, 0));
        // Only channel data justifies a buffer: a bare rectangle can declare 300000^2 pixels in a
        // few bytes (upstream #755), and a layer holding only mask channels has no pixels either.
        if !(0..self.cc as i16).chain([CHANNEL_TRANSPARENCY]).any(|id| rec.channel(id).is_some()) {
            self.warn(format!("layer \"{name}\": no channel data for its {w}x{h} bounds; treated as empty"));
            return Surface::new(self.fmt);
        }
        if !self.charge(w, h, self.cc + 1) {
            return Surface::new(self.fmt);
        }
        let s = self.fmt.sample;
        let mut planes = Vec::with_capacity(self.cc + 1);
        for c in 0..self.cc {
            planes.push(self.channel_plane(rec, c as i16, name));
        }
        planes.push(self.channel_plane(rec, CHANNEL_TRANSPARENCY, name));
        // A failed required channel leaves this temporary layer empty; the caller refuses
        // the whole conversion instead of exposing an incomplete editable document.
        let ids = (0..self.cc as i16).chain([CHANNEL_TRANSPARENCY]);
        if ids.zip(&planes).any(|(id, p)| p.is_none() && rec.channel(id).is_some()) {
            return Surface::new(self.fmt);
        }
        let refs: Vec<Option<&[u8]>> = planes.iter().map(|p| p.as_deref()).collect();
        let mut fill: Vec<Vec<u8>> = vec![zero_sample(s); self.cc];
        fill.push(max_sample(s));
        let mut invert = vec![self.cmyk; self.cc];
        invert.push(false);
        let mut bytes = interleave(&refs, &fill, w * h, s, &invert);
        if self.fmt.mode == ColorMode::Lab && s == SampleType::U16 {
            crate::pixels::lab16_chroma(&mut bytes, self.cc + 1, true);
        }
        let mut surf = Surface::from_interleaved(self.fmt, Rect::new(r.left, r.top, r.right, r.bottom), &bytes);
        surf.prune();
        surf
    }

    fn record_mask(&mut self, rec: &LayerRecord, name: &str) -> Option<LayerMask> {
        let m = rec.layer_mask()?;
        let (id, rect, default, flags) = match selected_real_mask(rec) {
            Some(real) => (CHANNEL_REAL_USER_MASK, real.rect, real.background, real.flags),
            _ => {
                rec.channel(CHANNEL_USER_MASK)?;
                (CHANNEL_USER_MASK, m.rect, m.default_color, m.flags)
            }
        };
        let mut surface = Surface::with_default(self.mask_fmt, &[f32::from(default) / 255.0]);
        if !rect.is_empty()
            && let Ok((w, h)) = rect.size()
            && self.charge(w, h, 1)
            && let Some(plane) = self.channel_plane(rec, id, name)
        {
            let s = self.mask_fmt.sample;
            let bytes = interleave(&[Some(&plane)], &[zero_sample(s)], w * h, s, &[false]);
            surface.write_interleaved(Rect::new(rect.left, rect.top, rect.right, rect.bottom), &bytes);
            surface.prune();
        }
        let params = m.parameters;
        Some(LayerMask {
            surface,
            enabled: flags & 2 == 0,
            linked: flags & 1 == 0,
            density: params.and_then(|p| p.user_density).map_or(1.0, |d| f32::from(d) / 255.0),
            feather: params.and_then(|p| p.user_feather).map_or(0.0, |f| f as f32),
        })
    }

    /// The filter mask of the smart object whose `placed` id is `placed`, from the `FEid` cache:
    /// document coordinates, white beyond its bounds (`filterMaskExtendWithWhite`). An all-white
    /// mask remains an explicitly stored mask.
    fn filter_mask(&mut self, placed: &str, stack: &crate::smart_map::FilterStack, name: &str) -> Option<LayerMask> {
        let Some(index) = self.filter_effects.iter().position(|i| i.id == placed) else {
            if let Some(error) = &self.filter_effects_error {
                self.fatal = Some(format!("layer \"{name}\": the smart filter mask could not be read ({error}); no document was opened"));
            }
            return None;
        };
        let (rect, _) = self.filter_effects[index].mask.as_ref()?;
        let (w, h) = match rect.size() {
            Ok(size) => size,
            Err(error) => {
                self.fatal = Some(format!("layer \"{name}\": invalid smart filter mask bounds ({error}); no document was opened"));
                return None;
            }
        };
        if !self.charge(w, h, 1) {
            return None;
        }
        match crate::smart_map::mask_from_item(&self.filter_effects[index], self.mask_fmt.sample, stack) {
            Ok(m) => m,
            Err(e) => {
                self.fatal = Some(format!("layer \"{name}\": {e}; no document was opened"));
                None
            }
        }
    }

    fn apply_common(&mut self, l: &mut Layer, rec: &LayerRecord, blend_override: Option<photocraft_psd::BlendMode>) {
        match crate::psd_generation::restore(rec, &mut self.generation_remaining) {
            Ok(value) => l.generation = value,
            Err(warning) => self.warn(format!("layer \"{}\": {warning}", l.name)),
        }
        l.visible = rec.is_visible();
        l.opacity = f32::from(rec.opacity) / 255.0;
        l.fill_opacity = f32::from(rec.fill_opacity()) / 255.0;
        l.clipped = rec.clipping != 0;
        let key = blend_override.unwrap_or(rec.blend_mode).key();
        l.blend = match BlendMode::from_psd_key(key) {
            Some(b) => b,
            None => {
                self.warn(format!("layer \"{}\": unknown blend mode {:?}; using Normal", l.name, String::from_utf8_lossy(&key)));
                BlendMode::Normal
            }
        };
        if let Some(Ok(BlockData::Protection(v))) = rec.block(b"lspf").and_then(TaggedBlock::parsed) {
            l.locks = blocks::locks_from_lspf(v);
        }
        if rec.flags.transparency_protected() {
            l.locks.transparency = true;
        }
        if let Some(Ok(BlockData::SheetColor(v))) = rec.block(b"lclr").and_then(TaggedBlock::parsed) {
            l.label = blocks::label_from_index(v);
        }
        // `lmfx` (multiple instances per kind) supersedes `lfx2` when present;
        // it stays in `psd_blocks`, `lfx2` in `Effects::psd_raw`.
        // Groups store their effects under `lfxs` (same layout as `lfx2`).
        let single = rec.block(b"lfx2").or_else(|| rec.block(b"lfxs"));
        if let Some(fx) = rec.block(b"lmfx").or(single) {
            let (enabled, items) = crate::effects_map::parse_lfx2(&fx.data).unwrap_or_else(|| (blocks::effects_enabled(&fx.data), Vec::new()));
            l.effects = Effects { enabled, items, psd_raw: single.map(|b| Arc::new(b.data.clone())), reference: None };
        } else if let Some(fx) = rec.block(b"lrFX")
            && !rec.section_type().is_folder()
            && let Some((enabled, items)) = crate::effects_map::parse_lrfx(&fx.data)
        {
            // Legacy effects only on non-group layers (Photoshop ignores
            // them on groups, see psd-tools effects/shape-fx.psd).
            l.effects = Effects { enabled, items, psd_raw: None, reference: None };
        }
        // Effects reference point (`fxrp`: two f64, x then y); also left in `psd_blocks`, where
        // export overwrites it from the field.
        if let Some(b) = rec.block(b"fxrp")
            && let (Some(x), Some(y)) = (b.data.get(..8), b.data.get(8..16))
        {
            let f = |s: &[u8]| f64::from_be_bytes(s.try_into().unwrap_or([0; 8]));
            l.effects.reference = Some((f(x), f(y)));
        }
        // Advanced Blending channel restrictions (`brst`: the u32 ids of channels left out);
        // also kept in `psd_blocks`, where export rewrites it from the field.
        if let Some(b) = rec.block(b"brst") {
            l.excluded_channels = crate::blocks::parse_brst(&b.data);
        }
        // Blend If lives in the layer record's blending ranges.
        l.blend_if = crate::blocks::blend_if_from_ranges(&rec.blending_ranges);
        l.psd_id = rec.layer_id();
        let name = l.name.clone();
        l.mask = self.record_mask(rec, &name);
    }

    fn layer_from_record(&mut self, rec: &LayerRecord) -> Layer {
        let name = rec.name();
        let fill_key = [b"SoCo", b"GdFl", b"PtFl"].into_iter().find(|k| rec.block(k).is_some());
        let vector_key = [b"vsms", b"vmsk"].into_iter().find(|k| rec.block(k).is_some());
        let adj_key = ADJUSTMENT_KEYS.into_iter().find(|k| rec.block(k).is_some());
        let smart_key = [b"SoLd", b"PlLd", b"SoLE"].into_iter().find(|k| rec.block(k).is_some());
        let blocks = preserved_blocks(rec);
        // `psd_raw` shares the Arc of the matching `psd_blocks` entry.
        let principal = |k: &[u8; 4]| blocks.iter().find(|(bk, _)| bk == k).map(|(_, d)| d.clone());
        let mut fill_cache = None;

        let merged_vector_cache = rec.block(&crate::vector_map::fallback::CACHE_KEY).is_some_and(|b| crate::vector_map::fallback::is_cache_block(&b.data));
        let content = if merged_vector_cache {
            LayerContent::Raster(self.record_surface(rec, &name))
        } else if let Some(k) = adj_key {
            let data = rec.block(k).map(|b| b.data.clone()).unwrap_or_default();
            let cged = rec.block(b"CgEd").map(|b| &b.data[..]);
            LayerContent::Adjustment(adjust_map::parse(
                k,
                &data,
                cged,
                match self.fmt.mode {
                    ColorMode::Rgb => adjust_map::Channels::Rgb,
                    ColorMode::Grayscale => adjust_map::Channels::Gray,
                    ColorMode::Cmyk => adjust_map::Channels::Cmyk,
                    ColorMode::Lab => adjust_map::Channels::Lab,
                    _ => adjust_map::Channels::Other,
                },
            ))
        } else if rec.block(b"TySh").is_some() {
            // Typed model from TySh/EngineData (photocraft-text); Photoshop's pixels stay the cache.
            let data = rec.block(b"TySh").map(|b| b.data.clone()).unwrap_or_default();
            let mut t = photocraft_text::psd::text_layer_from_tysh(&data, self.dpi).unwrap_or_else(|| {
                let (text, transform) = blocks::parse_tysh(&data).unwrap_or_default();
                TextLayer { text, transform, ..Default::default() }
            });
            if let Some(txt2) = &self.txt2 {
                photocraft_text::psd::apply_txt2(&mut t, &data, txt2);
            }
            t.cache = Some(self.record_surface(rec, &name));
            t.psd_raw = principal(b"TySh");
            LayerContent::Text(t)
        } else if let Some(k) = smart_key {
            let (id, transform) = rec.block(k).map(|b| blocks::parse_smart(k, &b.data)).unwrap_or_default();
            // Smart filters (`filterFX` in the placed-layer data) and their mask (`FEid`).
            let placed = rec.block(b"SoLd").or_else(|| rec.block(b"SoLE")).and_then(|b| crate::smart_map::parse_sold(&b.data));
            let stack = placed.as_ref().and_then(|p| p.stack.clone()).unwrap_or_default();
            let filter_mask = match &placed {
                Some(p) if !stack.filters.is_empty() => self.filter_mask(&p.placed, &stack, &name),
                _ => None,
            };
            LayerContent::Smart(SmartObject {
                source: SmartSource::Linked { path: id },
                transform,
                smart_filters: stack.filters,
                cache: Some(self.record_surface(rec, &name)),
                psd_raw: principal(k),
                filters_enabled: stack.enabled,
                filter_mask,
                warp: rec.block(k).and_then(|b| blocks::parse_placed_warp(k, &b.data)),
                stack_mode: None,
                // Distort / Perspective: the fourth corner (the affine `transform` drops it).
                perspective: rec.block(k).and_then(|b| blocks::parse_smart_perspective(k, &b.data)),
                develop: None,
            })
        } else if let Some(f) = soft_shape_fill(rec, vector_key.is_some(), fill_key) {
            // A shape whose vector mask has a density or feather is a fill layer seen through a
            // soft vector mask: the fill shows beyond the path, which the stored pixels (the
            // shape alone) lack. Import it as exactly that.
            LayerContent::Fill(f)
        } else if (vector_key.is_some() && (fill_key.is_some() || rec.block(b"vstk").is_some())) || rec.block(b"vscg").is_some() {
            let fill = fill_key.and_then(|k| rec.block(k).and_then(|b| blocks::parse_fill(k, &b.data)));
            let mut sh = ShapeLayer { fill, cache: Some(self.record_surface(rec, &name)), psd_raw: vector_key.and_then(principal), ..Default::default() };
            let lookup = |k: &[u8; 4]| rec.block(k).map(|b| b.data.clone());
            crate::vector_map::shape_from_blocks(&mut sh, &lookup, self.file.header.width, self.file.header.height, self.dpi);
            // No stored pixels (32-bit documents, files saved without layer pixels): render the
            // shape from its path and fill, as Photoshop does when it opens the file.
            if (rec.rect.is_empty() || rec.rect.size().is_err()) && (!sh.path.subpaths.is_empty() || sh.path.inverted) {
                let canvas = Rect::new(0, 0, self.file.header.width as i32, self.file.header.height as i32);
                if self.charge(self.file.header.width as usize, self.file.header.height as usize, self.fmt.channels()) {
                    sh.cache = Some(photocraft_vector::render_shape(&sh, self.fmt, canvas));
                }
            }
            LayerContent::Shape(sh)
        } else if let Some(k) = fill_key {
            match rec.block(k).and_then(|b| blocks::parse_fill(k, &b.data)) {
                Some(f) => {
                    // Keep Photoshop's rendering of the fill so it composites exactly.
                    if !rec.rect.is_empty() {
                        fill_cache = Some(FillCache { fill: f.clone(), surface: self.record_surface(rec, &name) });
                    }
                    LayerContent::Fill(f)
                }
                None => {
                    self.warn(format!("layer \"{name}\": unreadable {} fill; imported as pixels", String::from_utf8_lossy(k)));
                    LayerContent::Raster(self.record_surface(rec, &name))
                }
            }
        } else {
            LayerContent::Raster(self.record_surface(rec, &name))
        };
        let rendered = matches!(content, LayerContent::Shape(_) | LayerContent::Text(_) | LayerContent::Smart(_));
        let mut l = Layer::new(name, content);
        l.psd_blocks = blocks;
        l.fill_cache = fill_cache;
        self.apply_common(&mut l, rec, None);
        // Mask flag bit 3: the user mask was rendered from vector data. For
        // shape/text/smart layers the cached pixels already include that
        // coverage, so applying it again would double-mask.
        if rendered && selected_real_mask(rec).is_none() && rec.layer_mask().is_some_and(|m| m.flags & 8 != 0) {
            l.mask = None;
        }
        if !rec.block(&crate::vector_map::fallback::CACHE_KEY).is_some_and(|b| crate::vector_map::fallback::cache_has_shape_proxy(&b.data))
            && !matches!(l.content, LayerContent::Shape(_))
        {
            self.apply_vector_mask(&mut l, rec);
        }
        let canvas = Rect::new(0, 0, self.file.header.width as i32, self.file.header.height as i32);
        if let Err(warning) = crate::psd_editable::restore(&mut l, rec, self.fmt, canvas, &mut self.remaining) {
            self.warn(warning);
        }
        l
    }

    /// Typed vector mask from `vsms`/`vmsk` (non-shape layers). A user mask that Photoshop
    /// rendered from the vector data (flag bit 3, no separate real mask) is dropped: the
    /// compositor rasterizes the vector mask itself.
    fn apply_vector_mask(&mut self, l: &mut Layer, rec: &LayerRecord) {
        let Some(b) = rec.block(b"vsms").or_else(|| rec.block(b"vmsk")) else { return };
        let Some(mut vm) = crate::vector_map::vector_mask_from_block(&b.data, self.file.header.width, self.file.header.height) else { return };
        if let Some(m) = rec.layer_mask() {
            if let Some(p) = m.parameters {
                vm.density = p.vector_density.map_or(1.0, |d| f32::from(d) / 255.0);
                vm.feather = p.vector_feather.map_or(0.0, |f| f as f32);
            }
            if m.flags & 8 != 0 && selected_real_mask(rec).is_none() {
                l.mask = None;
            }
        }
        l.vector_mask = Some(vm);
    }

    fn build(&mut self, nodes: &[LayerNode], depth: usize) -> Vec<Layer> {
        let layers = self.file.layers();
        let ctl = self.ctl;
        nodes
            .iter()
            // A cancelled open stops decoding; the caller discards the partial document.
            .take_while(|_| !ctl.cancelled())
            .map(|n| match n {
                LayerNode::Layer { index } => {
                    let l = self.layer_from_record(&layers[*index]);
                    self.done += 1;
                    // Layers are 10–95 % of an open (the merged image and channels the rest).
                    self.ctl.progress(0.1 + 0.85 * self.done as f32 / self.total.max(1) as f32);
                    l
                }
                LayerNode::Group { index, children, .. } => {
                    let rec = &layers[*index];
                    // The recursion here (and in every later consumer of the tree) is bounded by
                    // the document model's nesting cap; a deeper subtree is not imported.
                    let children = if depth >= photocraft_doc::MAX_GROUP_DEPTH {
                        self.warn(format!(
                            "group `{}` nests deeper than {} groups; its contents were not imported",
                            rec.name(),
                            photocraft_doc::MAX_GROUP_DEPTH
                        ));
                        Vec::new()
                    } else {
                        self.build(children, depth + 1)
                    };
                    let sd = rec.section_divider();
                    let expanded = sd.is_none_or(|s| s.kind != photocraft_psd::SectionType::ClosedFolder);
                    let artboard = crate::comps_map::ARTBOARD_KEYS.iter().find_map(|k| rec.block(k)).and_then(|b| crate::comps_map::parse_artboard(&b.data));
                    let mut l =
                        Layer::new(rec.name(), LayerContent::Group(Group { children, expanded, artboard, isolation: photocraft_doc::GroupIsolation::Auto }));
                    l.psd_blocks = preserved_blocks(rec);
                    self.apply_common(&mut l, rec, sd.and_then(|s| s.blend_mode));
                    self.apply_vector_mask(&mut l, rec);
                    l
                }
            })
            .collect()
    }
}

/// Sets `Layer::link_group` from resource 1026's per-record ids (`nodes` and `layers` correspond).
fn apply_link_groups(nodes: &[LayerNode], layers: &mut [Layer], ids: &[u16]) {
    fn rec(nodes: &[LayerNode], layers: &mut [Layer], ids: &[u16], depth: usize) {
        for (n, l) in nodes.iter().zip(layers.iter_mut()) {
            let index = match n {
                LayerNode::Layer { index } => *index,
                LayerNode::Group { index, children, .. } => {
                    // Matches the importer's nesting cap: deeper children were not imported.
                    if depth < photocraft_doc::MAX_GROUP_DEPTH
                        && let LayerContent::Group(g) = &mut l.content
                    {
                        rec(children, &mut g.children, ids, depth + 1);
                    }
                    *index
                }
            };
            l.link_group = ids.get(index).copied().filter(|&g| g != 0).map(u64::from);
        }
    }
    rec(nodes, layers, ids, 0);
}

/// Rejects a file whose groups nest deeper than the document model's cap, with the actionable
/// limit error. Nesting past the cap could never be saved (.pcraft refuses it).
pub(crate) fn check_group_depth(file: &PsdFile) -> Result<(), crate::IoError> {
    if group_depth(file) > photocraft_doc::MAX_GROUP_DEPTH {
        return Err(crate::IoError::Unsupported(format!("layer groups nested deeper than {}", photocraft_doc::MAX_GROUP_DEPTH)));
    }
    Ok(())
}

/// Deepest group nesting of the file's layer records (a layer inside this many groups is the
/// deepest), computed like `layer_tree`'s stack — iteratively, so no file can make it recurse.
pub(crate) fn group_depth(file: &PsdFile) -> usize {
    use photocraft_psd::tagged::SectionType;
    let (mut open, mut depth) = (0usize, 0usize);
    for rec in file.layers() {
        match rec.section_type() {
            SectionType::BoundingDivider => {
                open += 1;
                depth = depth.max(open);
            }
            SectionType::OpenFolder | SectionType::ClosedFolder => open = open.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

fn preserved_blocks(rec: &LayerRecord) -> Vec<([u8; 4], Arc<Vec<u8>>)> {
    rec.blocks.iter().filter(|b| !REGENERATED.contains(&&b.key)).map(|b| (b.key, Arc::new(b.data.clone()))).collect()
}

fn be_to_ne_plane(plane: &[u8], s: SampleType, n: usize) -> Vec<u8> {
    interleave(&[Some(plane)], &[zero_sample(s)], n, s, &[false])
}

fn unicode_names(data: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 4 <= data.len() {
        let n = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as usize;
        at += 4;
        let Some(b) = data.get(at..at + n * 2) else { break };
        let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        out.push(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string());
        at += n * 2;
    }
    out
}

fn parse_guides(data: &[u8], doc: &mut Document) {
    let rd = |at: usize| data.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let Some(count) = rd(12) else { return };
    for i in 0..count as usize {
        let at = 16 + i * 5;
        let (Some(loc), Some(&dir)) = (rd(at), data.get(at + 4)) else { break };
        let pos = loc as i32 as f32 / 32.0;
        if dir == 1 {
            doc.guides.horizontal.push(pos);
        } else {
            doc.guides.vertical.push(pos);
        }
    }
}

/// Converts a parsed PSD into a complete document. Required pixel/mask failures
/// are fatal: a partial editable document must never overwrite its source.
pub fn psd_to_document(file: &PsdFile) -> Result<(Document, Vec<String>), crate::IoError> {
    psd_to_document_with(file, &lightcraft_sysmem::jobs::Interrupt::NONE)
}

/// The whole conversion is bounded as well as each decoder allocation. Use the
/// configured machine budget without letting one import consume it all.
fn conversion_budget() -> usize {
    let bytes = (lightcraft_sysmem::profile().memory_budget() / 4).clamp(512 << 20, 4 << 30);
    usize::try_from(bytes).unwrap_or(usize::MAX)
}

/// [`psd_to_document`] for a cancellable background open.
pub fn psd_to_document_with(file: &PsdFile, ctl: &lightcraft_sysmem::jobs::Interrupt) -> Result<(Document, Vec<String>), crate::IoError> {
    psd_to_document_with_budget(file, ctl, conversion_budget())
}

fn psd_to_document_with_budget(file: &PsdFile, ctl: &lightcraft_sysmem::jobs::Interrupt, budget: usize) -> Result<(Document, Vec<String>), crate::IoError> {
    check_group_depth(file)?;
    let h = &file.header;
    let mut warnings = Vec::new();
    let mode = doc_mode(h.color_mode).unwrap_or_else(|| {
        warnings.push(format!("unknown color mode {}; importing as RGB", h.color_mode.as_u16()));
        ColorMode::Rgb
    });
    let layered = matches!(mode, ColorMode::Grayscale | ColorMode::Rgb | ColorMode::Cmyk | ColorMode::Lab) && h.depth != 1;
    let multichannel = mode == ColorMode::Multichannel && h.depth != 1;
    let depth = if layered || multichannel { sample_for_depth(h.depth) } else { SampleType::U8 };
    let mut doc = Document::new("Untitled", Size::new(h.width, h.height), mode, depth);

    // Resources.
    for r in &file.resources {
        match r.id {
            ids::RESOLUTION_INFO => {
                if let Ok(ri) = photocraft_psd::ResolutionInfo::from_bytes(&r.data) {
                    let f = if ri.h_res_unit == 2 { 2.54 } else { 1.0 };
                    doc.resolution_dpi = (ri.h_res() * f) as f32;
                }
            }
            ids::ICC_PROFILE => doc.icc_profile = Some(Arc::new(r.data.clone())),
            ids::XMP => doc.metadata.xmp = Some(String::from_utf8_lossy(&r.data).into_owned()),
            ids::EXIF => doc.metadata.exif = Some(Arc::new(r.data.clone())),
            1032 => parse_guides(&r.data, &mut doc),
            ids::GLOBAL_ANGLE | ids::GLOBAL_ALTITUDE => {
                if let Some(b) = r.data.get(..4) {
                    let v = i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f32;
                    if r.id == ids::GLOBAL_ANGLE {
                        doc.global_light.angle = v;
                    } else {
                        doc.global_light.altitude = v;
                    }
                }
            }
            id if crate::vector_map::SAVED_PATHS.contains(&id) => match crate::vector_map::path_from_resource(&r.data, h.width, h.height) {
                Some(path) => doc.paths.push(photocraft_doc::NamedPath {
                    name: String::from_utf8_lossy(&r.name).into_owned(),
                    path,
                    psd_raw: Some(Arc::new(r.data.clone())),
                }),
                None => doc.metadata.psd_resources.push((id, String::from_utf8_lossy(&r.name).into_owned(), Arc::new(r.data.clone()))),
            },
            crate::vector_map::WORK_PATH => {
                doc.work_path = crate::vector_map::path_from_resource(&r.data, h.width, h.height);
            }
            crate::vector_map::CLIPPING_PATH => {
                // Kept raw (layout beyond the name is not modelled); the name is exposed.
                let n = usize::from(r.data.first().copied().unwrap_or(0));
                let name = r.data.get(1..1 + n).map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default();
                doc.clipping_path = Some(photocraft_doc::ClippingPath { name, flatness: 0.0 });
                doc.metadata.psd_resources.push((r.id, String::from_utf8_lossy(&r.name).into_owned(), Arc::new(r.data.clone())));
            }
            id if MAPPED_RESOURCES.contains(&id) => {}
            id => doc.metadata.psd_resources.push((id, String::from_utf8_lossy(&r.name).into_owned(), Arc::new(r.data.clone()))),
        }
    }
    for b in &file.global_blocks {
        doc.metadata.psd_global_blocks.push((b.signature, b.key, Arc::new(b.data.clone())));
    }
    doc.patterns = crate::pattern_map::from_global_blocks(&doc);
    // Notes (`Anno`) and the measurement scale (resource 1074); raw data stays for verbatim export.
    doc.notes = crate::annotations_map::notes_from_blocks(&doc);
    if crate::annotations_map::anno_unreadable(&doc) {
        warnings.push("the notes (Anno block) could not be read; they are kept unchanged on PSD save unless a note is added".into());
    }
    if let Some(scale) = crate::annotations_map::raw_scale(&doc) {
        doc.measurement.scale = scale;
    }

    let fmt = doc.pixel_format();
    let cc = fmt.mode.color_channels();
    let mut cx = Ctx {
        file,
        fmt,
        mask_fmt: PixelFormat::new(ColorMode::Grayscale, depth, false),
        cc,
        cmyk: fmt.mode == ColorMode::Cmyk,
        warnings,
        dpi: doc.resolution_dpi,
        txt2: file.global_blocks.iter().find(|b| &b.key == b"Txt2").and_then(|b| photocraft_text::psd::parse_txt2(&b.data)),
        filter_effects: Vec::new(),
        filter_effects_error: None,
        ctl: *ctl,
        done: 0,
        total: file.layers().len(),
        remaining: budget,
        fatal: None,
        generation_remaining: crate::psd_generation::DOCUMENT_BYTES,
    };
    for b in file.global_blocks.iter().filter(|b| matches!(&b.key, b"FEid" | b"FXid")) {
        // Smart-filter caches can be large: a cancelled open stops between blocks.
        if ctl.cancelled() {
            return Err(crate::IoError::Cancelled);
        }
        match photocraft_psd::filter_effects::FilterEffects::parse(&b.data) {
            Ok(fx) => cx.filter_effects.extend(fx.items),
            Err(e) => {
                let message = format!("smart filter masks ({}) could not be read: {e}", b.key_str());
                cx.filter_effects_error = Some(message.clone());
                cx.warn(message);
            }
        }
    }
    if ctl.cancelled() {
        return Err(crate::IoError::Cancelled);
    }

    let (w, hh) = (h.width as usize, h.height as usize);
    let n = w * hh;
    let canvas = Rect::new(0, 0, h.width as i32, h.height as i32);
    // The merged composite is decoded only when something consumes it: a flattened or
    // multichannel file (it *is* the image), or extra alpha/spot channels behind the colour
    // ones. A normal layered Photoshop save carries a merged composite nobody reads - roughly
    // half the file's bytes, decoded and thrown away on every open before this.
    let extra = usize::from(h.channels) > cc + usize::from(file.merged_has_alpha());
    let merged = if !layered || file.layers().is_empty() || extra {
        let m = if cx.charge(w, hh, usize::from(h.channels).max(cc + 1).max(8)) {
            file.decode_merged()
        } else {
            Err(photocraft_psd::PsdError::LimitExceeded("PSD import memory budget"))
        };
        if ctl.cancelled() {
            return Err(crate::IoError::Cancelled);
        }
        ctl.progress(0.1);
        if let Err(e) = &m {
            return Err(crate::IoError::Unsupported(format!("merged image could not be decoded ({e}); no document was opened")));
        }
        Some(m)
    } else {
        None
    };

    if multichannel {
        // Ink channels only (no layers); see `multichannel_map`.
        if let Some(Ok(all)) = &merged {
            let names = file.resource(1045).map(|r| unicode_names(&r.data)).unwrap_or_default();
            crate::multichannel_map::import(file, all, &mut doc, &names, &mut cx.warnings);
        }
        if !file.layers().is_empty() {
            cx.warn(format!("Multichannel documents have no layers: {} layer records were not imported", file.layers().len()));
        }
    } else if layered {
        // Capped like `build` (upstream #2940): a group past the document's nesting cap keeps no
        // children, so no file can make this tree, or anything walking or dropping it, recurse deeper.
        let tree = file.layer_tree_max_depth(photocraft_doc::MAX_GROUP_DEPTH);
        doc.layers = cx.build(&tree, 0);
        // local-image: smart objects that are Develop layers get their develop link back
        crate::smart_map::resolve_develop_layers(&mut doc);
        // Layer › Link Layers: resource 1026 holds one group id per layer record (0 = unlinked).
        if let Some(Ok(photocraft_psd::resources::ResourceData::LayerGroupInfo(groups))) =
            file.resources.iter().find(|r| r.id == ids::LAYER_GROUP_INFO).and_then(photocraft_psd::resources::ImageResource::parsed)
        {
            apply_link_groups(&tree, &mut doc.layers, &groups);
        }
        if doc.layers.is_empty()
            && let Some(Ok(all)) = &merged
        {
            // Flattened file: the merged image becomes the background layer.
            let plane = h.row_bytes() * hh;
            let alpha_idx = if file.merged_has_alpha() { Some(cc) } else { None };
            let mut planes: Vec<Option<&[u8]>> = (0..cc).map(|c| all.get(c * plane..(c + 1) * plane)).collect();
            planes.push(alpha_idx.and_then(|a| all.get(a * plane..(a + 1) * plane)));
            let mut fill = vec![zero_sample(depth); cc];
            fill.push(max_sample(depth));
            let mut inv = vec![cx.cmyk; cc];
            inv.push(false);
            let mut bytes = interleave(&planes, &fill, n, depth, &inv);
            if fmt.mode == ColorMode::Lab && depth == SampleType::U16 {
                crate::pixels::lab16_chroma(&mut bytes, cc + 1, true);
            }
            let mut s = Surface::from_interleaved(fmt, canvas, &bytes);
            if alpha_idx.is_some() {
                // Undo Photoshop's white matting of the merged image.
                // A band of tile rows at a time (no full-size float copy of the image).
                let white = photocraft_raster::from_rgba(&fmt, [1.0, 1.0, 1.0, 1.0]);
                let mut vals = Vec::new();
                let mut y = canvas.y0;
                while y < canvas.y1 {
                    let band = Rect::new(canvas.x0, y, canvas.x1, y.saturating_add(TILE_SIZE).min(canvas.y1));
                    s.read_region_into(band, &mut vals);
                    for px in vals.chunks_exact_mut(cc + 1) {
                        let a = px[cc];
                        for c in 0..cc {
                            px[c] = if a <= 0.0 { 0.0 } else { crate::pixels::unmatte(px[c], a, white[c]) };
                        }
                    }
                    s.write_region(band, &vals);
                    y = band.y1;
                }
            }
            s.prune();
            let mut bg = Layer::new("Background", LayerContent::Raster(s));
            if alpha_idx.is_none() {
                bg.locks.transparency = true;
                bg.locks.position = true;
            }
            doc.layers.push(bg);
        }
    } else {
        if !file.layers().is_empty() {
            cx.warn(format!("{:?} documents are imported flattened: {} layer records were not imported", h.color_mode, file.layers().len()));
        } else {
            cx.warn(format!("{:?} {}-bit document converted to {:?} 8-bit for editing", h.color_mode, h.depth, fmt.mode));
        }
        if h.color_mode == PsdMode::Indexed && file.color_mode_data.len() >= 768 {
            // Planar palette: 256 reds, 256 greens, 256 blues (the Color Table).
            let m = &file.color_mode_data;
            let colors = (0..256).map(|i| [m[i], m[256 + i], m[512 + i]]).collect();
            doc.color_table = Some(photocraft_doc::ColorTable { colors, transparent: None });
        } else if h.color_mode == PsdMode::Duotone && !file.color_mode_data.is_empty() {
            // The duotone ink block is undocumented: keep it raw; the image shows as its gray plate.
            doc.duotone =
                Some(photocraft_doc::Duotone { inks: vec![photocraft_doc::DuotoneInk::new("Black", [0.0; 3])], psd_raw: Some(file.color_mode_data.clone()) });
            cx.warn("duotone inks are not interpreted (shown as grayscale)");
        } else if !file.color_mode_data.is_empty() {
            cx.warn("color mode data is not preserved");
        }
        let rgba = if merged.as_ref().is_some_and(|r| r.is_err()) {
            Err(photocraft_psd::PsdError::LimitExceeded("PSD import memory budget"))
        } else {
            file.composite_rgba8()
        }
        .or_else(|_| {
            // Multichannel: show the first channels as RGB.
            let all = merged.clone().ok_or_else(|| photocraft_psd::PsdError::Invalid("no merged image".into()))??;
            let plane = h.row_bytes() * hh;
            let mut data = vec![255u8; n * 4];
            for c in 0..3.min(usize::from(h.channels)) {
                let p = photocraft_psd::pixels::plane_to_u8(&all[c * plane..(c + 1) * plane], h.depth, w, hh)?;
                for i in 0..n {
                    data[i * 4 + c] = p[i];
                }
            }
            Ok::<_, photocraft_psd::PsdError>(photocraft_psd::RgbaImage { left: 0, top: 0, width: h.width, height: h.height, data })
        });
        let img = rgba.map_err(|e| crate::IoError::Unsupported(format!("merged image could not be decoded ({e}); no document was opened")))?;
        {
            let mut s = Surface::new(fmt);
            let vals: Vec<f32> = img
                .data
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| {
                    let v = photocraft_raster::from_rgba(&fmt, [p[0], p[1], p[2], p[3]].map(|x| f32::from(x) / 255.0));
                    v.into_iter()
                })
                .collect();
            s.write_region(canvas, &vals);
            s.prune();
            doc.layers.push(Layer::new("Background", LayerContent::Raster(s)));
        }
    }

    // Extra (alpha / spot) channels of the merged image.
    if layered && let Some(Ok(all)) = &merged {
        let first = cc + usize::from(file.merged_has_alpha());
        let plane = h.row_bytes() * hh;
        let names = file.resource(1045).map(|r| unicode_names(&r.data)).unwrap_or_default();
        // Photoshop lists every extra channel, the merged transparency first when there is one.
        let skip = usize::from(file.merged_has_alpha() && names.len() == usize::from(h.channels).saturating_sub(cc));
        let names = names.get(skip..).unwrap_or_default();
        for (k, idx) in (first..usize::from(h.channels)).enumerate() {
            let Some(p) = all.get(idx * plane..(idx + 1) * plane) else { break };
            let mut s = Surface::new(cx.mask_fmt);
            s.write_interleaved(canvas, &be_to_ne_plane(p, depth, n));
            s.prune();
            let name = names.get(k).cloned().unwrap_or_else(|| format!("Alpha {}", k + 1));
            doc.channels.push(AlphaChannel::new(name, s));
        }
        // Channel options (overlay colour, opacity, colour indicates) and spot inks.
        if let Some(r) = file.resource(crate::channel_map::DISPLAY_INFO) {
            crate::channel_map::apply_display_info_skipping(&r.data, true, skip, &mut doc.channels);
        } else if let Some(r) = file.resource(crate::channel_map::DISPLAY_INFO_OLD) {
            crate::channel_map::apply_display_info_skipping(&r.data, false, skip, &mut doc.channels);
        }
        // Saved in Quick Mask mode: that channel becomes the document's Quick Mask again.
        if let Some(r) = file.resource(crate::channel_map::QUICK_MASK_INFO)
            && r.data.len() >= 2
            && let Some(k) = usize::from(u16::from_be_bytes([r.data[0], r.data[1]])).checked_sub(first)
            && k < doc.channels.len()
        {
            doc.quick_mask = Some(doc.channels.remove(k));
        }
    }

    // Layer comps (resource 1065 + per-layer `cmls`); the raw data stays for verbatim export.
    let raw_comps = doc.metadata.psd_resources.iter().find(|(id, _, _)| *id == crate::comps_map::LAYER_COMPS).map(|(_, _, d)| d.clone());
    (doc.layer_comps, doc.last_applied_comp, doc.last_document_state) = crate::comps_map::comps_from_psd(raw_comps.as_deref().map(Vec::as_slice), &doc);
    // Slices (resource 1050), after layer ids are known; the raw data stays for verbatim export.
    crate::slices_map::import(&mut doc);
    // Character and paragraph styles from the type layers' engine data.
    cx.warnings.extend(crate::text_styles_map::import(&mut doc));

    if ctl.cancelled() {
        return Err(crate::IoError::Cancelled);
    }
    if let Some(error) = cx.fatal {
        return Err(crate::IoError::Unsupported(error));
    }
    ctl.progress(1.0);
    Ok((doc, cx.warnings))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod file_input_regressions {
    use super::*;

    fn layered_document() -> Document {
        let mut doc = Document::new("budget fixture", Size::new(2, 2), ColorMode::Rgb, SampleType::U8);
        for (name, color) in [("bottom", [0.2, 0.4, 0.6, 1.0]), ("top", [0.8, 0.6, 0.4, 1.0])] {
            let mut surface = Surface::new(doc.pixel_format());
            surface.write_region(doc.bounds(), &color.repeat(4));
            doc.layers.push(Layer::new(name, LayerContent::Raster(surface)));
        }
        let mut mask = LayerMask::reveal_all();
        mask.surface.write_region(doc.bounds(), &[0.2, 0.4, 0.6, 0.8]);
        doc.layers[0].mask = Some(mask);
        doc
    }

    fn parsed_file(doc: &Document, force_psb: bool) -> PsdFile {
        let (file, _) = crate::document_to_psd_with(doc, &crate::PsdExportOptions { force_psb, ..Default::default() });
        PsdFile::from_bytes(&file.to_bytes().unwrap()).unwrap()
    }

    #[test]
    fn psd_psb_multilayer_and_mask_budget_failure_never_returns_partial_document() {
        let original = layered_document();
        // Two 2x2 RGBA layers and one grayscale mask, with the conversion's padding
        // and temporary-buffer allowance; well below one MiB for both file versions.
        let exact_budget = 130 * 130 * (4 + 4 + 1) * 3;
        for force_psb in [false, true] {
            let file = parsed_file(&original, force_psb);
            for budget in [exact_budget - 1, 130 * 130 * 4 * 3] {
                let error = psd_to_document_with_budget(&file, &lightcraft_sysmem::jobs::Interrupt::NONE, budget).unwrap_err();
                assert!(error.to_string().contains("no document was opened"), "{error}");
                assert!(error.to_string().contains("memory limit"), "{error}");
            }
            let (complete, _) = psd_to_document_with_budget(&file, &lightcraft_sysmem::jobs::Interrupt::NONE, exact_budget).unwrap();
            assert_eq!(complete.layers.len(), 2);
            for layer in &original.layers {
                let loaded = complete.layers.iter().find(|candidate| candidate.name == layer.name).unwrap();
                assert_eq!(loaded.surface().unwrap().to_interleaved(original.bounds()), layer.surface().unwrap().to_interleaved(original.bounds()));
                assert_eq!(
                    loaded.mask.as_ref().map(|mask| mask.surface.to_interleaved(original.bounds())),
                    layer.mask.as_ref().map(|mask| mask.surface.to_interleaved(original.bounds()))
                );
            }
            assert_eq!(crate::import("complete.psd", &file.to_bytes().unwrap()).unwrap().document.layers.len(), 2);
        }
    }

    #[test]
    fn psd_psb_flattened_budget_failure_is_an_error_and_exact_budget_keeps_pixels() {
        let original = layered_document();
        let exact_budget = 130 * 130 * 8 * 3;
        for force_psb in [false, true] {
            let mut file = parsed_file(&original, force_psb);
            file.layer_info = None;
            let file = PsdFile::from_bytes(&file.to_bytes().unwrap()).unwrap();
            assert!(file.layers().is_empty());
            let error = psd_to_document_with_budget(&file, &lightcraft_sysmem::jobs::Interrupt::NONE, exact_budget - 1).unwrap_err();
            assert!(error.to_string().contains("merged image could not be decoded"), "{error}");
            assert!(error.to_string().contains("no document was opened"), "{error}");
            let (complete, _) = psd_to_document_with_budget(&file, &lightcraft_sysmem::jobs::Interrupt::NONE, exact_budget).unwrap();
            assert_eq!(complete.layers.len(), 1);
            assert_eq!(
                complete.layers[0].surface().unwrap().to_interleaved(original.bounds()),
                original.layers[1].surface().unwrap().to_interleaved(original.bounds())
            );
            let mut corrupt = file;
            corrupt.image_data.data.clear();
            assert!(psd_to_document(&corrupt).is_err());
            assert!(crate::import("broken.psd", &corrupt.to_bytes().unwrap()).is_err());
        }
    }

    #[test]
    fn psd_psb_required_layer_and_mask_decode_failures_are_errors() {
        let original = layered_document();
        for force_psb in [false, true] {
            for channel_id in [0, CHANNEL_USER_MASK] {
                let mut file = parsed_file(&original, force_psb);
                let channel = file.layers_mut().iter_mut().flat_map(|layer| &mut layer.channels).find(|channel| channel.id == channel_id).unwrap();
                channel.compression = Some(photocraft_psd::Compression::Raw);
                channel.data.clear();
                let error = psd_to_document(&file).unwrap_err();
                assert!(error.to_string().contains(&format!("channel {channel_id} could not be decoded")), "{error}");
                assert!(error.to_string().contains("no document was opened"), "{error}");
                assert!(crate::import("broken.psd", &file.to_bytes().unwrap()).is_err());
            }
        }
    }

    fn smart_mask_file(force_psb: bool) -> PsdFile {
        use photocraft_psd::filter_effects::{EffectsPlane, FilterEffects, FilterEffectsItem};
        let mut doc = layered_document();
        doc.layers.truncate(1);
        doc.layers[0].mask = None;
        let mut file = parsed_file(&doc, force_psb);
        let stack = crate::smart_map::FilterStack {
            filters: vec![photocraft_doc::SmartFilter {
                command: "filter.blur.gaussianBlur".into(),
                params: serde_json::json!({"radius": 1.0}),
                blend: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
            }],
            ..Default::default()
        };
        let mut warnings = Vec::new();
        let spec = crate::smart_map::PlacedSpec {
            idnt: "synthetic-source",
            placed: "synthetic-placement",
            transform: photocraft_geom::Affine::IDENTITY,
            perspective: None,
            size: (2.0, 2.0),
            dpi: 72.0,
            warp: None,
            filter_fx: Some(crate::smart_map::filter_fx(&stack, "smart", &mut warnings)),
        };
        file.layers_mut()[0].blocks.push(TaggedBlock::new(*b"SoLd", crate::smart_map::sold_bytes(None, &spec, &mut warnings)));
        let rect = photocraft_psd::Rect { top: 0, left: 0, bottom: 2, right: 2 };
        let item = FilterEffectsItem {
            id: spec.placed.into(),
            version: 1,
            rect,
            depth: 8,
            max_channels: 0,
            slots: vec![None, None],
            mask: Some((rect, EffectsPlane::encode(photocraft_psd::Compression::Raw, &[51, 102, 153, 204], 2, 2, 8).unwrap())),
        };
        file.global_blocks.push(TaggedBlock::new(*b"FEid", FilterEffects { version: 3, items: vec![item] }.to_bytes()));
        PsdFile::from_bytes(&file.to_bytes().unwrap()).unwrap()
    }

    #[test]
    fn psd_psb_smart_filter_mask_is_budgeted_and_required_decode_errors_are_fatal() {
        use photocraft_psd::filter_effects::FilterEffects;
        let exact_budget = 130 * 130 * (4 + 1) * 3;
        for force_psb in [false, true] {
            let file = smart_mask_file(force_psb);
            assert!(psd_to_document_with_budget(&file, &lightcraft_sysmem::jobs::Interrupt::NONE, exact_budget - 1).is_err());
            let (complete, _) = psd_to_document_with_budget(&file, &lightcraft_sysmem::jobs::Interrupt::NONE, exact_budget).unwrap();
            let LayerContent::Smart(smart) = &complete.layers[0].content else { panic!("expected a smart object") };
            assert_eq!(smart.filter_mask.as_ref().unwrap().surface.to_interleaved(complete.bounds()), [51, 102, 153, 204]);
            for malformed_block in [false, true] {
                let mut broken = file.clone();
                let block = broken.global_blocks.iter_mut().find(|block| &block.key == b"FEid").unwrap();
                if malformed_block {
                    block.data.clear();
                } else {
                    let mut effects = FilterEffects::parse(&block.data).unwrap();
                    effects.items[0].mask.as_mut().unwrap().1.data.clear();
                    block.data = effects.to_bytes();
                }
                let error = psd_to_document(&broken).unwrap_err();
                assert!(error.to_string().contains("smart filter mask"), "{error}");
                assert!(error.to_string().contains("no document was opened"), "{error}");
                assert!(crate::import("broken-smart.psd", &broken.to_bytes().unwrap()).is_err());
            }
        }
    }

    fn file() -> PsdFile {
        let doc = Document::new("test", Size::new(1, 1), ColorMode::Rgb, SampleType::U8);
        crate::document_to_psd(&doc)
    }
    #[test]
    fn channel_free_huge_layer_is_empty() {
        let mut file = file();
        file.layer_info = Some(photocraft_psd::LayerInfo {
            merged_alpha: false,
            layers: vec![LayerRecord { rect: photocraft_psd::Rect { top: 0, left: 0, bottom: 300_000, right: 300_000 }, ..Default::default() }],
            padding: None,
        });
        let (doc, _) = psd_to_document(&file).unwrap();
        assert!(doc.layers[0].surface().unwrap().tiles().next().is_none());
    }
    #[test]
    fn oversized_flattened_psd_is_refused_before_decode() {
        let mut file = file();
        file.header.width = 300_000;
        file.header.height = 300_000;
        let error = psd_to_document(&file).unwrap_err();
        assert!(error.to_string().contains("memory budget"));
    }
    #[test]
    fn layers_share_one_conversion_budget() {
        let file = file();
        let fmt = PixelFormat::RGBA8;
        let mut ctx = Ctx {
            file: &file,
            fmt,
            mask_fmt: fmt,
            cc: 3,
            cmyk: false,
            warnings: vec![],
            dpi: 72.0,
            txt2: None,
            filter_effects: vec![],
            filter_effects_error: None,
            ctl: lightcraft_sysmem::jobs::Interrupt::NONE,
            done: 0,
            total: 0,
            remaining: 400_000,
            fatal: None,
            generation_remaining: crate::psd_generation::DOCUMENT_BYTES,
        };
        assert!(ctx.charge(1, 1, 4));
        assert!(ctx.charge(1, 1, 4));
        assert!(!ctx.charge(1, 1, 4));
    }
}
