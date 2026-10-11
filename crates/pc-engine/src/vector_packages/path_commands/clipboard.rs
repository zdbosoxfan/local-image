//! Replayable vector clipboard through the existing native codec. It preserves complete layer
//! trees, masks, effects and live metadata without changing the shared raster/OS clipboard.
//! The UI owns clipboard lifetime. Copy does not journal; paste publishes once.
//! Placement follows VectorCraft cmd/edit.rs, d522c1d7be4035bd4f4a84cd6ebfca44f5155092,
//! MIT OR Apache-2.0; existing licenses/vectorcraft-NOTICE retained.
use super::*;
use photocraft_doc::vector_design::symbol_validation;
const MAX_CLIP_BYTES: usize = 64 << 20;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clipboard {
    pub native: Vec<u8>,
}
fn vector_tree(layer: &Layer) -> Result<()> {
    match &layer.content {
        LayerContent::Shape(sh) => validate_path(&sh.path)?,
        LayerContent::Text(_) | LayerContent::Symbol(_) => {}
        LayerContent::Group(g) => {
            for child in &g.children {
                vector_tree(child)?;
            }
        }
        _ => return Err(bad("vector clipboard requires vector layers")),
    }
    Ok(())
}
fn cache_budget(layer: &Layer, total: &mut u128) -> Result<()> {
    for surface in layer.surface().into_iter().chain(layer.mask.iter().map(|m| &m.surface)) {
        let b = surface.content_bounds();
        *total += u128::from(b.width()) * u128::from(b.height()) * surface.format().bytes_per_pixel() as u128;
        if *total > MAX_CACHE_BYTES {
            return Err(bad("vector clipboard caches exceed 512 MiB"));
        }
    }
    if let Some(children) = layer.children() {
        for child in children {
            cache_budget(child, total)?;
        }
    }
    Ok(())
}
impl Clipboard {
    /// Snapshot selected roots in document paint order, including their native resources.
    pub fn copy(s: &Session, ids: &[LayerId]) -> Result<Self> {
        let source = &s.active().ok_or(EngineError::NoDocument)?.doc;
        if ids.is_empty() || ids.len() > MAX_TARGETS {
            return Err(bad("select 1–256 vector layers to copy"));
        }
        let mut positioned = ids.iter().map(|id| source.path_of(*id).map(|p| (p, *id)).ok_or(EngineError::NoLayer(*id))).collect::<Result<Vec<_>>>()?;
        positioned.sort();
        for pair in positioned.windows(2) {
            if pair[1].0.starts_with(&pair[0].0) {
                return Err(bad("clipboard selection contains duplicate or descendant targets"));
            }
        }
        // A fresh native document avoids copying raster selections, channels and unrelated data.
        let mut doc = Document::new("Vector clipboard", source.size, source.mode, source.depth);
        doc.resolution_dpi = source.resolution_dpi;
        doc.icc_profile = source.icc_profile.clone();
        doc.global_light = source.global_light;
        doc.symbols = source.symbols.clone();
        doc.patterns = source.patterns.clone();
        doc.text_styles = source.text_styles.clone();
        doc.color_table = source.color_table.clone();
        doc.duotone = source.duotone.clone();
        let mut bytes = 0;
        for (_, id) in positioned {
            let l = source.layer(id).ok_or(EngineError::NoLayer(id))?;
            vector_tree(l)?;
            cache_budget(l, &mut bytes)?;
            doc.layers.push(l.clone());
        }
        for def in &doc.symbols {
            for l in &def.layers {
                cache_budget(l, &mut bytes)?;
            }
        }
        symbol_validation::document(&doc).map_err(|e| bad(e.to_string()))?;
        let native = photocraft_format::save_to_bytes(&doc, &Default::default()).map_err(|e| bad(e.to_string()))?;
        if native.len() > MAX_CLIP_BYTES {
            return Err(bad("vector clipboard exceeds 64 MiB"));
        }
        Ok(Self { native })
    }
    pub fn paste(&self, s: &mut Session, capture: &Capture, front: bool) -> Result<Value> {
        s.execute(if front { "object.pasteFront" } else { "object.pasteBack" }, json!({"capture":capture,"clipboard":self}))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Paste {
    capture: Capture,
    clipboard: Clipboard,
}
fn paste(s: &mut Session, v: &Value, front: bool) -> Result<Value> {
    let r: Paste = serde_json::from_value(v.clone()).map_err(|e| bad(e.to_string()))?;
    r.capture.check_paste(s)?;
    if r.clipboard.native.len() > MAX_CLIP_BYTES {
        return Err(bad("vector clipboard exceeds 64 MiB"));
    }
    let mut clip = photocraft_format::load_from_bytes(&r.clipboard.native).map_err(|e| bad(format!("invalid vector clipboard: {e}")))?;
    let source = &s.active().ok_or(EngineError::NoDocument)?.doc;
    if clip.layers.is_empty() || clip.layers.len() > MAX_TARGETS {
        return Err(bad("invalid vector clipboard target count"));
    }
    // Existing caches carry document-relative paint, text and effect context. Refuse incompatible
    // contexts until the owners expose a complete cache/paint coordinate rebake, never rasterize.
    if clip.size != source.size
        || clip.pixel_format() != source.pixel_format()
        || clip.resolution_dpi != source.resolution_dpi
        || clip.icc_profile != source.icc_profile
        || clip.global_light != source.global_light
        || clip.color_table != source.color_table
        || clip.duotone != source.duotone
        || clip.text_styles != source.text_styles
    {
        return Err(bad("vector paste across different canvas/color/type contexts requires the shared projection provider"));
    }
    let mut doc = (**source).clone();
    let mut paths = r.capture.layers.iter().map(|id| doc.path_of(*id).ok_or(EngineError::NoLayer(*id))).collect::<Result<Vec<_>>>()?;
    paths.sort();
    let (parent, index) = if let Some(first) = paths.first() {
        let parent = first[..first.len() - 1].to_vec();
        if paths.iter().any(|p| p.len() != first.len() || p[..p.len() - 1] != parent) {
            return Err(bad("paste targets must share a parent"));
        }
        let index = if front { paths[paths.len() - 1][first.len() - 1] + 1 } else { first[first.len() - 1] };
        (parent, index)
    } else {
        (Vec::new(), if front { doc.layers.len() } else { 0 })
    };
    for pattern in std::mem::take(&mut clip.patterns) {
        if doc.patterns.iter().any(|p| p.display_name().eq_ignore_ascii_case(pattern.display_name()) && *p != pattern) {
            return Err(bad("clipboard pattern name collides with different destination paint"));
        }
        if let Some(existing) = doc.patterns.iter().find(|p| p.id == pattern.id) {
            if *existing != pattern {
                return Err(bad("clipboard pattern ID collides with different destination paint"));
            }
        } else {
            doc.patterns.push(pattern);
        }
    }
    // The symbol owner transfers only reachable masters and remaps IDs against the
    // destination resource space. Do not duplicate the owner's graph-transfer algorithm.
    let mut copies = clip.clone();
    super::super::symbols::transfer_definitions(&clip, &doc, &mut copies)?;
    let mut bytes = 0;
    for def in &copies.symbols {
        for layer in &def.layers {
            cache_budget(layer, &mut bytes)?;
        }
    }
    doc.symbols.extend(copies.symbols);
    let mut layers = Vec::new();
    let mut ids = Vec::new();
    for l in copies.layers {
        vector_tree(&l)?;
        cache_budget(&l, &mut bytes)?;
        let mut copy = l.duplicate();
        // Shape caches are prepared before publishing; authoritative live data stays intact.
        fn prepare(doc: &Document, l: &mut Layer, bytes: &mut u128) -> Result<()> {
            if matches!(l.content, LayerContent::Shape(_)) && l.vector.clip.is_none() {
                let mut display = l.clone();
                display.content = LayerContent::Shape(super::super::expand::project_shape_base(l)?);
                super::super::expand::refresh_tree(doc, &mut display, bytes)?;
                if let (LayerContent::Shape(sh), LayerContent::Shape(rendered)) = (&mut l.content, &display.content) {
                    sh.cache = rendered.cache.clone();
                }
            }
            if let Some(children) = l.children_mut() {
                for child in children {
                    prepare(doc, child, bytes)?;
                }
            }
            Ok(())
        }
        prepare(&doc, &mut copy, &mut bytes)?;
        ids.push(copy.id);
        layers.push(copy);
    }
    let siblings = if parent.is_empty() {
        &mut doc.layers
    } else {
        doc.layer_at_mut(&parent).and_then(Layer::children_mut).ok_or_else(|| bad("paste parent unavailable"))?
    };
    siblings.splice(index..index, layers);
    symbol_validation::document(&doc).map_err(|e| bad(e.to_string()))?;
    publish(s, &r.capture, if front { "Paste in Front" } else { "Paste in Back" }, Prepared { document: doc, layers: ids, original_points: 0, new_points: 0 })
}
pub fn specs() -> Vec<CommandSpec> {
    [("object.pasteFront", "Paste in Front", true), ("object.pasteBack", "Paste in Back", false)]
        .into_iter()
        .map(|(id, label, front)| CommandSpec {
            id,
            label,
            menu: &[],
            shortcut: None,
            params: "{capture,clipboard:{native:[u8]}}",
            enabled: crate::vector_cmds::has_doc,
            run: if front { |s, v| paste(s, v, true) } else { |s, v| paste(s, v, false) },
            journal: true,
        })
        .collect()
}
