//! Undo/redo history.
//!
//! Because pixel tiles are `Arc`-shared copy-on-write (see `photocraft-raster`), a full
//! [`Document`] clone costs O(layers + tiles) pointer copies, not pixel copies. History therefore
//! stores whole-document snapshots per transaction, which is simple, obviously correct, and the
//! same approach Photoshop's History panel exposes to users (one state per step).
//!
//! Memory is bounded by `max_states` plus an approximate byte budget. Shared tiles and embedded source/metadata allocations are counted once across current,
//! undo and redo states; replacing a source cannot bypass the budget.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use photocraft_doc::{Document, LayerId};

/// The layers a state targeted when it was created: the active ("key") layer and every selected
/// layer. Undo and redo bring them back with the state's document, as the reference app does;
/// selecting layers is not a step of its own, so it doesn't change a state's target.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayerTarget {
    pub active: Option<LayerId>,
    pub selected: Vec<LayerId>,
}

#[derive(Clone, Debug)]
pub struct HistoryState {
    pub label: String,
    pub doc: Arc<Document>,
    /// The layers targeted when this state was created.
    pub layers: LayerTarget,
}

#[derive(Clone, Debug)]
pub struct History {
    /// Past states; the last one is the state *before* the current document.
    undo: VecDeque<HistoryState>,
    redo: Vec<HistoryState>,
    pub max_states: usize,
    /// Backing-memory budget in bytes for the current document plus allocations only history holds
    /// (0 = unlimited). [`History::trim`] drops the oldest states beyond it.
    pub max_bytes: usize,
    /// Label of the step that produced the current document.
    current_label: String,
    /// The layers targeted when the current document was created (opened or edited).
    current_layers: LayerTarget,
}

impl Default for History {
    fn default() -> Self {
        Self::new(50)
    }
}

impl History {
    pub fn new(max_states: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            max_states: max_states.max(1),
            max_bytes: 0,
            current_label: "Open".into(),
            current_layers: LayerTarget::default(),
        }
    }

    /// Record that `before` was replaced by a new current document via step `label`, which left
    /// `layers` targeted.
    pub fn record(&mut self, label: impl Into<String>, before: Arc<Document>, layers: LayerTarget) {
        let prev_label = std::mem::replace(&mut self.current_label, label.into());
        let prev_layers = std::mem::replace(&mut self.current_layers, layers);
        self.undo.push_back(HistoryState { label: prev_label, doc: before, layers: prev_layers });
        self.redo.clear();
        while self.undo.len() > self.max_states {
            self.undo.pop_front();
        }
    }

    /// Rebase resource identity in stored documents without making an edit,
    /// changing labels/targets, or discarding redo. Call on a detached clone
    /// when preparing a filesystem transaction, then publish only on success.
    pub fn map_documents(&mut self, mut map: impl FnMut(&Arc<Document>) -> Arc<Document>) {
        for state in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            state.doc = map(&state.doc);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo_label(&self) -> Option<&str> {
        self.can_undo().then_some(self.current_label.as_str())
    }
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }
    /// Labels of the redo states, the next redo first (the History panel's greyed rows).
    pub fn redo_labels(&self) -> impl Iterator<Item = &str> {
        self.redo.iter().rev().map(|s| s.label.as_str())
    }

    /// Set the layers the current document targets from its creation on: when it is first
    /// opened, or when its step coalesces or changes the selection after recording.
    pub fn set_current_layers(&mut self, layers: LayerTarget) {
        self.current_layers = layers;
    }

    /// Undo: `current` becomes redoable; returns the document to make current and the layers it
    /// targeted when it was created.
    pub fn undo(&mut self, current: Arc<Document>) -> Option<(Arc<Document>, LayerTarget)> {
        let prev = self.undo.pop_back()?;
        let label = std::mem::replace(&mut self.current_label, prev.label);
        let layers = std::mem::replace(&mut self.current_layers, prev.layers.clone());
        self.redo.push(HistoryState { label, doc: current, layers });
        Some((prev.doc, prev.layers))
    }

    /// Redo: the mirror of [`Self::undo`].
    pub fn redo(&mut self, current: Arc<Document>) -> Option<(Arc<Document>, LayerTarget)> {
        let next = self.redo.pop()?;
        let label = std::mem::replace(&mut self.current_label, next.label);
        let layers = std::mem::replace(&mut self.current_layers, next.layers.clone());
        self.undo.push_back(HistoryState { label, doc: current, layers });
        Some((next.doc, next.layers))
    }

    /// Entries for a History panel: past labels oldest→newest, then the current label.
    pub fn entries(&self) -> Vec<String> {
        self.undo.iter().map(|s| s.label.clone()).chain(std::iter::once(self.current_label.clone())).collect()
    }

    /// Document of past entry `i`, indexed like [`History::entries`] (0 = oldest). The last entry is
    /// the current document, which the history does not hold, so it (and any index past it) is `None`.
    pub fn state(&self, i: usize) -> Option<Arc<Document>> {
        self.undo.get(i).map(|s| s.doc.clone())
    }

    /// Number of past states (entries before the current one).
    pub fn past_len(&self) -> usize {
        self.undo.len()
    }

    /// Rename the current step (e.g. after folding several steps into one with [`Self::purge_last`]).
    pub fn set_current_label(&mut self, label: impl Into<String>) {
        self.current_label = label.into();
    }

    /// Forget the most recent undo state and every redo state (Edit › Purge › Undo): the
    /// last step can no longer be undone and its pixels are released.
    pub fn purge_last(&mut self) -> bool {
        self.redo.clear();
        self.undo.pop_back().is_some()
    }

    /// Forget the redo states, e.g. after undoing half of a compound step that failed.
    pub fn clear_redo(&mut self) {
        self.redo.clear();
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Approximate unique backing bytes held by history (not shared with `current`).
    pub fn unique_bytes(&self, current: &Document) -> usize {
        let mut seen = HashSet::new();
        backing_bytes(current, &mut seen);
        self.undo.iter().chain(self.redo.iter()).map(|s| backing_bytes(&s.doc, &mut seen)).sum()
    }

    /// Backing bytes of `current` plus allocations only history holds (what [`History::trim`]
    /// bounds by [`History::max_bytes`]).
    pub fn pixel_bytes(&self, current: &Document) -> usize {
        let mut seen = HashSet::new();
        let own = backing_bytes(current, &mut seen);
        self.undo.iter().chain(self.redo.iter()).fold(own, |n, s| n.saturating_add(backing_bytes(&s.doc, &mut seen)))
    }

    /// Apply [`History::max_states`] immediately, then keep backing memory within
    /// [`History::max_bytes`]: the current document's allocations plus
    /// those only history holds (newest states first). The oldest undo states that don't fit are
    /// dropped; the most recent one is always kept so the last step can be undone. Returns how
    /// many states were dropped.
    pub fn trim(&mut self, current: &Document) -> usize {
        let count_drop = self.undo.len().saturating_sub(self.max_states.max(1));
        self.undo.drain(..count_drop);
        if self.max_bytes == 0 || self.undo.len() <= 1 {
            return count_drop;
        }
        let mut seen = HashSet::new();
        let mut total = backing_bytes(current, &mut seen);
        for s in self.redo.iter().rev() {
            total = total.saturating_add(backing_bytes(&s.doc, &mut seen));
        }
        let mut keep = 0;
        for s in self.undo.iter().rev() {
            total = total.saturating_add(backing_bytes(&s.doc, &mut seen));
            if total > self.max_bytes && keep >= 1 {
                break;
            }
            keep += 1;
        }
        let drop = self.undo.len() - keep;
        self.undo.drain(..drop);
        count_drop + drop
    }
}

/// Large backing allocations retained by a document. Arc identity deduplicates pixels,
/// embedded originals and preserved metadata across both layers and history snapshots.
fn backing_bytes(doc: &Document, seen: &mut HashSet<usize>) -> usize {
    use photocraft_doc::{Adjustment, LayerContent, SmartSource};
    let mut n = 0usize;
    let mut layers: Vec<_> = doc.layers.iter().chain(doc.symbols.iter().flat_map(|s| &s.layers)).collect();
    while let Some(layer) = layers.pop() {
        if let Some(children) = layer.children() {
            layers.extend(children);
        }
        if let Some(surface) = layer.surface() {
            n = n.saturating_add(surface_bytes(surface, seen));
        }
        if let Some(mask) = &layer.mask {
            n = n.saturating_add(surface_bytes(&mask.surface, seen));
        }
        if let Some(fill) = &layer.fill_cache {
            n = n.saturating_add(surface_bytes(&fill.surface, seen));
        }
        if let Some(video) = &layer.video {
            for frame in &video.frames {
                n = n.saturating_add(surface_bytes(frame, seen));
            }
        }
        if let Some(raw) = &layer.effects.psd_raw {
            n = n.saturating_add(arc_bytes(raw, seen));
        }
        for (_, raw) in &layer.psd_blocks {
            n = n.saturating_add(arc_bytes(raw, seen));
        }
        let raw = match &layer.content {
            LayerContent::Smart(smart) => {
                if let SmartSource::Embedded { bytes, .. } = &smart.source {
                    n = n.saturating_add(arc_bytes(bytes, seen));
                }
                if let Some(mask) = &smart.filter_mask {
                    n = n.saturating_add(surface_bytes(&mask.surface, seen));
                }
                smart.psd_raw.as_ref()
            }
            LayerContent::Text(text) => text.psd_raw.as_ref(),
            LayerContent::Shape(shape) => shape.psd_raw.as_ref(),
            LayerContent::Adjustment(Adjustment::ColorLookup { lut: Some(lut), .. }) => {
                n = n.saturating_add(arc_bytes(lut, seen));
                None
            }
            _ => None,
        };
        if let Some(raw) = raw {
            n = n.saturating_add(arc_bytes(raw, seen));
        }
    }
    for channel in doc.channels.iter().chain(&doc.quick_mask) {
        n = n.saturating_add(surface_bytes(&channel.surface, seen));
    }
    if let Some(selection) = &doc.selection {
        n = n.saturating_add(surface_bytes(selection, seen));
    }
    for pattern in &doc.patterns {
        n = n.saturating_add(surface_bytes(&pattern.surface, seen));
    }
    for raw in doc.icc_profile.iter().chain(doc.metadata.exif.iter()) {
        n = n.saturating_add(arc_bytes(raw, seen));
    }
    for (_, _, raw) in &doc.metadata.psd_resources {
        n = n.saturating_add(arc_bytes(raw, seen));
    }
    for (_, _, raw) in &doc.metadata.psd_global_blocks {
        n = n.saturating_add(arc_bytes(raw, seen));
    }
    for path in &doc.paths {
        if let Some(raw) = &path.psd_raw {
            n = n.saturating_add(arc_bytes(raw, seen));
        }
    }
    n
}

fn surface_bytes(surface: &photocraft_doc::Surface, seen: &mut HashSet<usize>) -> usize {
    surface.tiles().filter(|(_, tile)| seen.insert(Arc::as_ptr(tile) as usize)).fold(0usize, |n, (_, tile)| n.saturating_add(tile.bytes().len()))
}
fn arc_bytes<T>(data: &Arc<Vec<T>>, seen: &mut HashSet<usize>) -> usize {
    if seen.insert(Arc::as_ptr(data) as usize) { data.capacity().saturating_mul(std::mem::size_of::<T>()) } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_doc::{Color, ColorMode, Layer, SampleType, Size};

    fn base() -> Document {
        Document::with_background("h", Size::new(64, 64), ColorMode::Rgb, SampleType::U8, Color::WHITE)
    }

    /// Apply an edit the way the engine does: snapshot, mutate a clone, record.
    fn edit(h: &mut History, cur: &mut Arc<Document>, label: &str, f: impl FnOnce(&mut Document)) {
        let before = cur.clone();
        let mut d = (**cur).clone();
        f(&mut d);
        *cur = Arc::new(d);
        h.record(label, before, LayerTarget::default());
    }

    #[test]
    fn vector_symbol_master_tiles_share_and_count_in_history() {
        use photocraft_doc::{SymbolDef, SymbolId};
        let mut d = base();
        let mut master = Layer::raster("Master", d.pixel_format());
        master.surface_mut().unwrap().fill_rect(d.bounds(), &[0.2, 0.3, 0.4, 1.0]);
        let tile = master.surface().unwrap().tiles().next().unwrap().1.bytes().len();
        d.symbols.push(SymbolDef { id: SymbolId(1), name: "Master".into(), layers: vec![Layer::group("Nested", vec![master.clone()])] });
        d.symbols.push(SymbolDef { id: SymbolId(2), name: "Shared tiles".into(), layers: vec![master] });
        let mut cur = Arc::new(d);
        let mut h = History::default();
        assert_eq!(h.pixel_bytes(&cur), 2 * tile, "one root tile plus the shared master tile");
        edit(&mut h, &mut cur, "Edit master", |d| {
            let bounds = d.bounds();
            d.symbols[0].layers[0].children_mut().unwrap()[0].surface_mut().unwrap().fill_rect(bounds, &[0.8, 0.2, 0.1, 1.0]);
        });
        assert_eq!(h.pixel_bytes(&cur), 3 * tile, "changed master tile plus shared old tile and root");
        assert_eq!(h.unique_bytes(&cur), 0, "old master tile is still referenced by another master");
        let before = h.undo(cur.clone()).unwrap().0;
        assert_eq!(before.symbols[0].layers[0].children().unwrap()[0].surface().unwrap().sample_channel(1, 1, 0), 0.2);
        assert_eq!(h.redo(before).unwrap().0, cur);
    }

    #[test]
    fn undo_redo_cycle() {
        let mut h = History::default();
        let mut cur = Arc::new(base());
        edit(&mut h, &mut cur, "New Layer", |d| {
            d.insert_above(None, Layer::raster("L", d.pixel_format()));
        });
        assert_eq!(cur.layers.len(), 2);
        assert_eq!(h.undo_label(), Some("New Layer"));
        cur = h.undo(cur).unwrap().0;
        assert_eq!(cur.layers.len(), 1);
        assert_eq!(h.redo_label(), Some("New Layer"));
        cur = h.redo(cur).unwrap().0;
        assert_eq!(cur.layers.len(), 2);
        assert!(h.redo(cur.clone()).is_none());
    }

    #[test]
    fn undo_and_redo_return_the_layers_each_state_targeted_when_created() {
        let mut h = History::default();
        let open = Arc::new(base());
        let bg = open.layers[0].id;
        let target = |id: LayerId| LayerTarget { active: Some(id), selected: vec![id] };
        h.set_current_layers(target(bg));
        // New Layer targets the new layer; the background is then selected (not a step) and filled.
        let mut d = (*open).clone();
        let new = d.insert_above(None, Layer::raster("L", d.pixel_format()));
        let layered = Arc::new(d);
        h.record("New Layer", open, target(new));
        let mut d = (*layered).clone();
        d.name = "filled".into();
        h.record("Fill", layered, target(bg));
        let (d, layers) = h.undo(Arc::new(d)).unwrap();
        assert_eq!(layers, target(new), "New Layer's state targeted the new layer when created");
        let (d, layers) = h.undo(d).unwrap();
        assert_eq!(layers, target(bg), "the opened document's target");
        let (d, layers) = h.redo(d).unwrap();
        assert_eq!(layers, target(new));
        let (_, layers) = h.redo(d).unwrap();
        assert_eq!(layers, target(bg), "Fill's state targeted the background");
    }

    #[test]
    fn state_accessor_matches_entries() {
        let mut h = History::default();
        let mut cur = Arc::new(base());
        let open = cur.clone();
        edit(&mut h, &mut cur, "A", |d| d.name = "a".into());
        edit(&mut h, &mut cur, "B", |d| d.name = "b".into());
        assert_eq!(h.entries().len(), h.past_len() + 1);
        assert!(Arc::ptr_eq(&h.state(0).unwrap(), &open));
        assert_eq!(h.state(1).unwrap().name, "a");
        assert!(h.state(2).is_none(), "the current document is not held");
        cur = h.undo(cur).unwrap().0;
        h.undo(cur).unwrap();
        assert_eq!(h.redo_labels().collect::<Vec<_>>(), ["A", "B"], "next redo first");
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut h = History::default();
        let mut cur = Arc::new(base());
        edit(&mut h, &mut cur, "A", |d| d.name = "a".into());
        cur = h.undo(cur).unwrap().0;
        edit(&mut h, &mut cur, "B", |d| d.name = "b".into());
        assert!(!h.can_redo());
        assert_eq!(h.entries(), vec!["Open".to_string(), "B".to_string()]);
    }

    #[test]
    fn max_states_bound() {
        let mut h = History::new(3);
        let mut cur = Arc::new(base());
        for i in 0..10 {
            edit(&mut h, &mut cur, &format!("step {i}"), |d| d.name = format!("{i}"));
        }
        let mut n = 0;
        while let Some((d, _)) = h.undo(cur.clone()) {
            cur = d;
            n += 1;
        }
        assert_eq!(n, 3);
        assert_eq!(cur.name, "6");
    }

    #[test]
    fn lowering_history_state_limit_trims_existing_undo_and_preserves_current_and_redo() {
        let tile = 256 * 256 * 4;
        for (budget, kept, removed) in [(0, 3, 2), (4 * tile, 2, 3)] {
            let mut history = History::new(50);
            let mut current = Arc::new(base());
            for n in 0..6 {
                edit(&mut history, &mut current, &format!("paint {n}"), |doc| {
                    doc.name = format!("paint {n}");
                    doc.layers[0].surface_mut().unwrap().write_pixel(1, 1, &[n as f32 / 8.0, 0.0, 0.0, 1.0]);
                });
            }
            current = history.undo(current).unwrap().0;
            let before = current.clone();
            let redo = history.redo_labels().map(str::to_owned).collect::<Vec<_>>();
            history.max_states = 3;
            history.max_bytes = budget;
            assert_eq!(history.trim(&current), removed, "reports count and byte removals together");
            assert_eq!(history.past_len(), kept);
            assert!(Arc::ptr_eq(&before, &current), "changing the limit is not a document edit");
            assert_eq!(history.redo_labels().map(str::to_owned).collect::<Vec<_>>(), redo);
            assert_eq!(history.trim(&current), 0, "applying an unchanged limit is idempotent");
            current = history.undo(current).unwrap().0;
            assert_eq!(current.name, "paint 3", "the most recent undo stays available");
            current = history.redo(current).unwrap().0;
            assert!(Arc::ptr_eq(&before, &current));
            current = history.redo(current).unwrap().0;
            assert_eq!(current.name, "paint 5", "the preexisting redo is preserved");
        }
    }

    #[test]
    fn lowering_history_state_limit_to_zero_keeps_one_undo() {
        let mut history = History::new(50);
        let mut current = Arc::new(base());
        for n in 0..3 {
            edit(&mut history, &mut current, &format!("rename {n}"), |doc| doc.name = n.to_string());
        }
        history.max_states = 0;
        assert_eq!(history.trim(&current), 2);
        assert_eq!(history.past_len(), 1);
        assert_eq!(history.undo(current).unwrap().0.name, "1");
    }

    #[test]
    fn byte_budget_drops_oldest_states_but_keeps_one() {
        let mut h = History::new(50);
        let mut cur = Arc::new(base());
        let tile = 256 * 256 * 4;
        // Each step repaints the background's tile: one unique tile per state.
        for i in 0..6 {
            edit(&mut h, &mut cur, &format!("paint {i}"), |d| {
                let id = d.layers[0].id;
                d.layer_mut(id).unwrap().surface_mut().unwrap().write_pixel(1, 1, &[i as f32 / 8.0, 0.0, 0.0, 1.0]);
            });
        }
        assert_eq!(h.trim(&cur), 0, "unlimited by default");
        assert_eq!(h.past_len(), 6);
        // The current tile plus three history tiles fit.
        h.max_bytes = 4 * tile;
        assert_eq!(h.trim(&cur), 3);
        assert_eq!(h.past_len(), 3);
        assert_eq!(h.unique_bytes(&cur), 3 * tile);
        assert_eq!(h.pixel_bytes(&cur), 4 * tile, "the current tile plus history's");
        assert_eq!(h.entries()[0], "paint 2");
        // A budget smaller than one state still keeps the last step undoable.
        h.max_bytes = 1;
        h.trim(&cur);
        assert_eq!(h.past_len(), 1);
        assert!(h.undo(cur.clone()).is_some());
    }

    #[test]
    fn history_shares_untouched_tiles() {
        let mut h = History::default();
        let mut cur = Arc::new(base());
        assert_eq!(h.unique_bytes(&cur), 0);
        // rename only: no pixel changes, history holds no unique tiles
        edit(&mut h, &mut cur, "Rename", |d| d.name = "x".into());
        assert_eq!(h.unique_bytes(&cur), 0);
        // paint one pixel: exactly one tile becomes unique to history
        edit(&mut h, &mut cur, "Paint", |d| {
            let id = d.layers[0].id;
            d.layer_mut(id).unwrap().surface_mut().unwrap().write_pixel(1, 1, &[0.0, 0.0, 0.0, 1.0]);
        });
        assert_eq!(h.unique_bytes(&cur), 256 * 256 * 4);
    }

    #[test]
    fn history_budget_counts_replaced_embedded_sources_and_shared_psd_payloads_once() {
        use photocraft_doc::{Affine, LayerContent, SmartObject, SmartSource};
        const BYTES: usize = 1 << 20;
        let mut h = History::new(50);
        let mut cur = Arc::new(Document::new("sources", Size::new(1, 1), ColorMode::Rgb, SampleType::U8));
        let mut old = Vec::new();
        for n in 0..8 {
            let bytes = Arc::new(vec![n as u8; BYTES]);
            old.push(Arc::downgrade(&bytes));
            edit(&mut h, &mut cur, "Replace Contents", |doc| {
                // The same payload is preserved in both the smart source and PSD metadata.
                doc.metadata.psd_global_blocks = vec![(*b"8BIM", *b"lnk2", bytes.clone())];
                doc.layers = vec![Layer::new(
                    "source",
                    LayerContent::Smart(SmartObject::new(SmartSource::Embedded { file_name: "source.psd".into(), bytes }, Affine::IDENTITY, None)),
                )];
            });
        }
        assert_eq!(h.pixel_bytes(&cur), 8 * BYTES, "shared metadata and smart source count once");
        h.max_bytes = 3 * BYTES;
        h.trim(&cur);
        assert_eq!(h.past_len(), 2, "current plus two previous originals fit");
        assert_eq!(h.pixel_bytes(&cur), 3 * BYTES);
        assert_eq!(old.iter().filter(|bytes| bytes.strong_count() > 0).count(), 3);
        let shared = h.unique_bytes(&cur);
        edit(&mut h, &mut cur, "Rename", |doc| doc.name = "renamed".into());
        assert_eq!(h.unique_bytes(&cur), shared, "an unchanged source is not charged per history entry");
        h.clear();
        assert_eq!(old.iter().filter(|bytes| bytes.strong_count() > 0).count(), 1);
    }

    #[test]
    fn history_counts_replaced_pattern_tiles_across_undo_redo_without_double_counting() {
        let mut h = History::new(50);
        let mut cur = Arc::new(Document::new("patterns", Size::new(1, 1), ColorMode::Rgb, SampleType::U8));
        for n in 0..4 {
            edit(&mut h, &mut cur, "Replace pattern", |doc| {
                let mut surface = photocraft_doc::Surface::new(photocraft_doc::PixelFormat::RGBA8);
                surface.write_pixel(0, 0, &[n as f32 / 4.0, 1.0, 0.0, 1.0]);
                doc.patterns = vec![photocraft_doc::Pattern::new("pattern", surface, 1, 1)];
            });
        }
        let tile = 256 * 256 * 4;
        assert_eq!(h.pixel_bytes(&cur), 4 * tile);
        edit(&mut h, &mut cur, "Rename", |doc| doc.name = "renamed".into());
        assert_eq!(h.pixel_bytes(&cur), 4 * tile, "unchanged pattern tiles remain shared");
        cur = h.undo(cur).unwrap().0;
        assert_eq!(h.pixel_bytes(&cur), 4 * tile, "redo participates in the same ownership walk");
        h.max_bytes = 2 * tile;
        h.trim(&cur);
        assert_eq!(h.pixel_bytes(&cur), 2 * tile);
    }
}
