//! Floating selection: the selected pixels of a layer cut out and moved as a piece, without
//! touching the document until the piece is dropped.
//!
//! - `select.float {"dx","dy"}` cuts the selected pixels of the active layer into a floating piece
//!   (the first time) and moves it by whole pixels. Further calls move the same piece; nothing new
//!   is cut.
//! - `select.drop` drops the piece into its layer and moves the selection with it, as one history
//!   step.
//! - Any other command drops the piece first (so it lands exactly where it was shown), except Undo,
//!   which puts it back where it was cut (nothing was changed yet).
//!
//! While floating, the document is unchanged; [`displayed`] gives the document with the piece at
//! its offset, for the canvas. The selection stays where it was until the drop.

use std::sync::Arc;

use photocraft_color::PixelFormat;
use photocraft_doc::{Document, LayerContent, LayerId};
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{DocState, EngineError, Result, Session};

/// A layer split by its selection: the layer with the selected pixels cut out, and the cut pixels
/// (cropped to the selection). Splitting touches only the selection's area, and placing the piece
/// ([`CutParts::moved`]) only the piece's: the cut-out layer's other tiles are shared.
#[derive(Clone, Debug)]
pub struct CutParts {
    pub layer: LayerId,
    rest: Surface,
    piece: Surface,
}

impl CutParts {
    /// Split `layer` of `doc` by its selection. Errors without a selection or pixels.
    pub fn new(doc: &Document, layer: LayerId) -> Result<Self> {
        Self::with_copy(doc, layer, false)
    }

    /// Lift selected pixels, optionally leaving the originals in place (Alt/Option + Move).
    pub fn with_copy(doc: &Document, layer: LayerId, copy: bool) -> Result<Self> {
        let sel = doc.selection.as_ref().ok_or_else(|| EngineError::Other("no selection".into()))?;
        let l = doc.layer(layer).ok_or(EngineError::NoLayer(layer))?;
        let LayerContent::Raster(surf) = &l.content else {
            return Err(EngineError::Other("the layer has no pixels to move".into()));
        };
        let fmt = surf.format();
        let with_alpha = PixelFormat::new(fmt.mode, fmt.sample, true);
        let base = if fmt == with_alpha { surf.clone() } else { surf.convert(with_alpha) };
        let mut rest = base.clone();
        let bounds = sel.content_bounds().intersect(&base.content_bounds());
        let piece = cut_piece(&base, sel, bounds, with_alpha)?;
        if !copy && !bounds.is_empty() {
            crate::pixels::clear_surface(&mut rest, bounds, Some(sel))?;
        }
        Ok(Self { layer, rest, piece })
    }

    /// `doc` with the piece moved by whole pixels (dx, dy) over the cut-out layer.
    pub fn moved(&self, doc: &Document, dx: i32, dy: i32) -> Result<Document> {
        let mut d = doc.clone();
        let surf = d
            .layer_mut(self.layer)
            .ok_or(EngineError::NoLayer(self.layer))?
            .surface_mut()
            .ok_or_else(|| EngineError::Other("the layer has no pixels to move".into()))?;
        let mut out = self.rest.clone();
        crate::transform_cmds::composite_over(&mut out, &photocraft_algo::resample::translate_surface(&self.piece, dx, dy))?;
        *surf = out;
        Ok(d)
    }
}

/// Extract the selected pixels of `base` over `b` into a `with_alpha` surface, tile by tile in
/// parallel. Fully selected tiles on an alpha surface share `base`'s `Arc<Tile>` directly.
fn cut_piece(base: &Surface, sel: &Surface, b: photocraft_geom::Rect, with_alpha: PixelFormat) -> Result<Surface> {
    use photocraft_color::{SampleType, read_sample};
    use photocraft_geom::{TILE_SIZE, TileCoord};
    use photocraft_raster::Tile;
    use rayon::prelude::*;

    let mut piece = Surface::new(with_alpha);
    let src_fmt = base.format();
    let same_fmt = src_fmt == with_alpha;
    let n = with_alpha.channels();
    let a = n - 1;
    let dst_bpp = with_alpha.bytes_per_pixel();
    let src_bpp = src_fmt.bytes_per_pixel();
    let sb = with_alpha.sample.bytes();
    let alpha_off = dst_bpp - sb;
    let src_empty_default = same_fmt && base.default_bytes().iter().all(|&v| v == 0);
    let oom = |error: photocraft_raster::AllocationError| crate::allocation::error("floating selection", &error.to_string());
    let mut coords = crate::allocation::capacity(b.tiles().count(), "floating selection")?;
    coords.extend(b.tiles());
    let mut results = crate::allocation::capacity(coords.len(), "floating selection")?;
    coords
        .into_par_iter()
        .map(|tc| -> Result<Option<(TileCoord, Arc<Tile>)>> {
            let tr = tc.rect().intersect(&b);
            let mask = crate::fill_cmds::Mask::of(sel, tc);
            if mask.empty(tr) {
                return Ok(None);
            }
            let src_tile = base.tile(tc);
            if same_fmt && src_tile.is_none() && src_empty_default {
                return Ok(None);
            }
            if same_fmt
                && tr == tc.rect()
                && mask.full(tr)
                && let Some(t) = src_tile
            {
                return Ok(Some((tc, t.clone())));
            }
            let src_bytes = src_tile.map_or(base.default_bytes(), |t| t.bytes());
            let mut out = piece.try_default_tile().map_err(oom)?;
            let out_bytes = out.bytes_mut();
            let mut px = [0.0f32; 8];
            let mut has_content = false;
            for y in tr.y0..tr.y1 {
                let row = (y - tc.ty * TILE_SIZE) as usize * TILE_SIZE as usize;
                for x in tr.x0..tr.x1 {
                    let k = mask.at(x, y);
                    if k <= 0.0 {
                        continue;
                    }
                    let idx = row + (x - tc.tx * TILE_SIZE) as usize;
                    let dst_i = idx * dst_bpp;
                    let Some(dst_px) = out_bytes.get_mut(dst_i..dst_i + dst_bpp) else { continue };
                    let src_i = if src_bytes.len() == src_bpp { 0 } else { idx * src_bpp };
                    let Some(src_px) = src_bytes.get(src_i..src_i + src_bpp) else { continue };
                    if same_fmt && k >= 1.0 {
                        if src_px.iter().any(|&v| v != 0) {
                            dst_px.copy_from_slice(src_px);
                            has_content = true;
                        }
                        continue;
                    }
                    if same_fmt {
                        for (c, v) in px.iter_mut().enumerate().take(n) {
                            *v = read_sample(src_px, src_fmt.sample, c);
                        }
                        px[a] *= k;
                    } else {
                        for (c, v) in px.iter_mut().enumerate().take(a) {
                            *v = read_sample(src_px, src_fmt.sample, c);
                        }
                        px[a] = k;
                    }
                    if px[a] <= 0.0 {
                        continue;
                    }
                    photocraft_raster::encode_pixel(&with_alpha, &px[..n], dst_px);
                    let non_zero = match with_alpha.sample {
                        SampleType::U8 => dst_px.get(alpha_off) != Some(&0) || dst_px.iter().any(|&v| v != 0),
                        SampleType::U16 => dst_px.get(alpha_off..dst_bpp) != Some(&[0, 0]) || dst_px.iter().any(|&v| v != 0),
                        SampleType::F32 => dst_px.iter().any(|&v| v != 0),
                    };
                    has_content |= non_zero;
                }
            }
            Ok(has_content.then(|| (tc, Arc::new(out))))
        })
        .collect_into_vec(&mut results);
    let mut tiles = crate::allocation::capacity(results.len(), "floating selection")?;
    for result in results {
        tiles.extend(result?);
    }
    piece.put_tiles(tiles);
    Ok(piece)
}

/// Move the active layer's selected pixels and the selection together, atomically. Unlike a
/// floating marquee gesture, a Move-tool drag is complete when its button is released.
fn move_pixels(s: &mut Session, p: &Value) -> Result<Value> {
    let (dx, dy) = (int_param(p, "dx"), int_param(p, "dy"));
    if (dx, dy) == (0, 0) {
        return Ok(Value::Null);
    }
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let layer = st.active_layer.ok_or_else(|| EngineError::Other("no active layer".into()))?;
    let copy = p.get("copy").and_then(Value::as_bool).unwrap_or(false);
    let parts = CutParts::with_copy(&st.doc, layer, copy)?;
    let before = st.doc.clone();
    s.edit(if copy { "Copy Selected Pixels" } else { "Move Selected Pixels" }, |doc, _| {
        *doc = parts.moved(doc, dx, dy)?;
        doc.selection = doc.selection.as_ref().map(|sel| crate::layer_multi_cmds::shift_surface(sel, dx, dy));
        Ok(())
    })?;
    crate::layer_multi_cmds::note_damage(s, &before, &[layer]);
    Ok(json!({"offset": [dx, dy], "copy": copy}))
}

/// A piece floating `offset` whole pixels from where it was cut, over document `revision`.
#[derive(Clone, Debug)]
pub struct Floating {
    pub layer: LayerId,
    pub offset: (i32, i32),
    pub revision: u64,
    pub parts: Arc<CutParts>,
}

/// The floating piece of `st`, if it is still valid (the document hasn't changed under it).
pub fn floating(st: &DocState) -> Option<&Floating> {
    st.floating.as_ref().filter(|f| f.revision == st.revision && st.doc.selection.is_some())
}

/// The document as shown while a piece floats `extra` further than its offset (a drag of it in
/// progress); `None` when nothing floats.
pub fn displayed(st: &DocState, extra: (i32, i32)) -> Option<Document> {
    let f = floating(st)?;
    f.parts.moved(&st.doc, f.offset.0 + extra.0, f.offset.1 + extra.1).ok()
}

fn int_param(p: &Value, key: &str) -> i32 {
    p.get(key).and_then(Value::as_f64).filter(|v| v.is_finite()).map_or(0, |v| v.round().clamp(-1e7, 1e7) as i32)
}

fn can_float(s: &Session) -> std::result::Result<(), String> {
    let st = s.active().ok_or("no document")?;
    if floating(st).is_some() {
        return Ok(());
    }
    if st.doc.selection.is_none() {
        return Err("there is no selection".into());
    }
    let l = st.active_layer.and_then(|id| st.doc.layer(id)).ok_or("no active layer")?;
    if !matches!(l.content, LayerContent::Raster(_)) {
        return Err("the active layer has no pixels to move".into());
    }
    let locks = st.doc.effective_locks(l.id);
    if locks.all || locks.position {
        return Err(format!("layer \"{}\" is locked", l.name));
    }
    Ok(())
}

fn float(s: &mut Session, p: &Value) -> Result<Value> {
    let (dx, dy) = (int_param(p, "dx"), int_param(p, "dy"));
    let st = s.active_mut().ok_or(EngineError::NoDocument)?;
    if floating(st).is_none() {
        let layer = st.active_layer.ok_or_else(|| EngineError::Other("no active layer".into()))?;
        let parts = CutParts::new(&st.doc, layer)?;
        st.floating = Some(Floating { layer, offset: (0, 0), revision: st.revision, parts: Arc::new(parts) });
    }
    let f = st.floating.as_mut().ok_or(EngineError::NoDocument)?;
    f.offset = (f.offset.0.saturating_add(dx), f.offset.1.saturating_add(dy));
    Ok(json!({"layer": f.layer.0, "offset": [f.offset.0, f.offset.1]}))
}

/// Drop the floating piece into its layer and move the selection with it (one history step).
pub fn drop_floating(s: &mut Session) -> Result<Value> {
    let Some(st) = s.active_mut() else { return Ok(json!({"dropped": false})) };
    let Some(f) = floating(st).cloned() else {
        st.floating = None;
        return Ok(json!({"dropped": false}));
    };
    st.floating = None;
    let (dx, dy) = f.offset;
    if (dx, dy) == (0, 0) {
        return Ok(json!({"dropped": true, "offset": [0, 0]}));
    }
    s.edit("Move Selected Pixels", |doc, _| {
        let moved = f.parts.moved(doc, dx, dy)?;
        let surf = moved.layer(f.layer).and_then(|l| l.surface()).cloned().ok_or(EngineError::NoLayer(f.layer))?;
        *doc.layer_mut(f.layer).ok_or(EngineError::NoLayer(f.layer))?.surface_mut().ok_or(EngineError::NoLayer(f.layer))? = surf;
        doc.selection = doc.selection.as_ref().map(|sel| photocraft_algo::resample::translate_surface(sel, dx, dy));
        Ok(())
    })?;
    Ok(json!({"dropped": true, "offset": [dx, dy]}))
}

/// Commands that leave a floating piece floating: its own, and ones that don't touch documents.
fn can_move_pixels(s: &Session) -> std::result::Result<(), String> {
    if s.active().is_some_and(|st| floating(st).is_some()) {
        return Err("drop the floating selection before moving selected pixels".into());
    }
    can_float(s)
}

fn keeps_floating(id: &str) -> bool {
    matches!(id, "select.float" | "select.drop" | "select.movePixels") || ["view.", "window.", "help.", "prefs."].iter().any(|p| id.starts_with(p))
}

/// Before any command (`Session::dispatch`): Undo puts a floating piece back (and is used up:
/// returns its result); every other command but the floating ones drops it first.
pub(crate) fn before_command(s: &mut Session, id: &str) -> Result<Option<Value>> {
    let Some(st) = s.active_mut() else { return Ok(None) };
    if floating(st).is_none() {
        st.floating = None;
        return Ok(None);
    }
    match id {
        "edit.undo" | "edit.stepBackward" => {
            st.floating = None;
            Ok(Some(json!({"floating": "returned"})))
        }
        _ if keeps_floating(id) => Ok(None),
        _ => drop_floating(s).map(|_| None),
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "select.movePixels",
            label: "Move Selected Pixels",
            menu: &[],
            shortcut: None,
            params: r##"{"dx":px=0,"dy":px=0,"copy":false} (Move tool: move/copy selected pixels and selection, one undo step)"##,
            enabled: can_move_pixels,
            journal: true,
            run: move_pixels,
        },
        CommandSpec {
            id: "select.float",
            label: "Float Selection",
            menu: &[],
            shortcut: None,
            params: r##"{"dx":px=0,"dy":px=0} → {layer, offset} (cuts the selected pixels of the active layer into a floating piece the first time, then moves it by whole pixels; dropped by select.drop or any other command, put back by edit.undo)"##,
            enabled: can_float,
            journal: true,
            run: float,
        },
        CommandSpec {
            id: "select.drop",
            label: "Drop Floating Selection",
            menu: &[],
            shortcut: None,
            params: r##"{} → {dropped, offset} (drops the floating piece into its layer and moves the selection with it: one history step)"##,
            enabled: |s| s.active().map(|_| ()).ok_or_else(|| "no document".into()),
            journal: true,
            run: |s, _| drop_floating(s),
        },
    ]
}

#[cfg(test)]
mod tests;
