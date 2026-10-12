//! Preserve linked-source identity across a flattened file replacement.
use super::*;
use crate::DocState;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

struct SourcePath {
    path: String,
    canonical: Option<PathBuf>,
    matches: HashMap<String, bool>,
}
impl SourcePath {
    fn new(path: &str) -> Self {
        Self { path: path.into(), canonical: std::fs::canonicalize(path).ok(), matches: HashMap::new() }
    }
    fn matches(&mut self, path: &str) -> bool {
        if self.path == path {
            return true;
        }
        *self.matches.entry(path.into()).or_insert_with(|| self.canonical.as_ref().zip(std::fs::canonicalize(path).ok().as_ref()).is_some_and(|(a, b)| a == b))
    }
}
fn layers_mut(layers: &mut [Layer], f: &mut impl FnMut(&mut Layer) -> bool) -> bool {
    let mut changed = false;
    for layer in layers {
        changed |= f(layer);
        if let Some(children) = layer.children_mut() {
            changed |= layers_mut(children, f);
        }
    }
    changed
}
fn map_doc(doc: &Arc<Document>, memo: &mut HashMap<usize, Arc<Document>>, edit: &mut impl FnMut(&mut Layer) -> bool) -> Arc<Document> {
    let key = Arc::as_ptr(doc) as usize;
    if let Some(mapped) = memo.get(&key) {
        return mapped.clone();
    }
    let mut copy = (**doc).clone();
    let mut changed = layers_mut(&mut copy.layers, edit);
    for symbol in &mut copy.symbols {
        changed |= layers_mut(&mut symbol.layers, edit);
    }
    let mapped = if changed { Arc::new(copy) } else { doc.clone() };
    memo.insert(key, mapped.clone());
    mapped
}
fn map_state(state: &DocState, memo: &mut HashMap<usize, Arc<Document>>, edit: &mut impl FnMut(&mut Layer) -> bool) -> DocState {
    let mut out = state.clone();
    out.doc = map_doc(&out.doc, memo, edit);
    // A saved project still contains its old linked path. Resource preservation must
    // be saved before closing even when no pixel-edit revision changed. Already dirty
    // documents retain their saved-revision marker and full undo history.
    if !Arc::ptr_eq(&out.doc, &state.doc) && !state.is_dirty() {
        out.saved_revision = 0;
    }
    out.history.map_documents(|doc| map_doc(doc, memo, edit));
    for snapshot in &mut out.snapshots {
        snapshot.doc = map_doc(&snapshot.doc, memo, edit);
    }
    out
}

/// Prepared off to the side before touching the source file. Publication cannot
/// fail and preserves history labels, selection and edit revisions. Clean documents
/// whose source changes become unsaved so the preserved bytes survive normal closure.
pub struct PreparedSourceOverwrite {
    states: Vec<DocState>,
}
impl PreparedSourceOverwrite {
    /// Use immediately after a synchronous successful file commit; no UI/event
    /// processing may be interleaved with prepare/commit/publication.
    pub fn publish(self, session: &mut Session) {
        session.docs = self.states;
    }
}
impl Session {
    pub fn prepare_source_overwrite(&self, path: &str, original: Arc<Vec<u8>>) -> Result<PreparedSourceOverwrite> {
        if original.len() as u64 > photocraft_format::read::MAX_FILE_BYTES {
            return Err(EngineError::Other("Original source exceeds the file memory limit".into()));
        }
        let mut source = SourcePath::new(path);
        let mut affected = HashSet::new();
        let mut keys = HashSet::new();
        for state in &self.docs {
            let mut inspect = (*state.doc).clone();
            let mut scan = |layer: &mut Layer| {
                if let LayerContent::Smart(smart) = &layer.content {
                    let direct = matches!(&smart.source,SmartSource::Linked{path} if source.matches(path));
                    let linked = source_of(smart).is_some_and(|path| source.matches(path));
                    if direct || linked {
                        affected.insert(state.doc.id);
                        keys.insert(layer_key(state.doc.id, layer.id));
                        if let Some(link) = &smart.develop
                            && let Some(photo) = link.photo
                        {
                            keys.insert(photo_key(link.library.as_deref(), photo));
                        }
                    }
                }
                false
            };
            layers_mut(&mut inspect.layers, &mut scan);
            for symbol in &mut inspect.symbols {
                layers_mut(&mut symbol.layers, &mut scan);
            }
        }
        if self.jobs.conflicts_source_rebase(&affected, &keys, |path| source.matches(path)) || self.live_commit.is_some() {
            return Err(EngineError::Other("Wait for editing operations using this source to finish before overwriting it".into()));
        }
        let name = Path::new(path).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "original".into());
        let mut memo = HashMap::new();
        let mut freeze = |layer: &mut Layer| {
            let LayerContent::Smart(smart) = &mut layer.content else {
                return false;
            };
            if !matches!(&smart.source,SmartSource::Linked{path} if source.matches(path)) {
                return false;
            }
            // Legacy Develop links derive their origin from the linked source. Keep that
            // identity before the source becomes embedded.
            if let SmartSource::Linked { path } = &smart.source
                && let Some(link) = &mut smart.develop
                && link.source.is_none()
            {
                link.source = Some(path.clone().into());
            }
            smart.source = SmartSource::Embedded { file_name: name.clone(), bytes: original.clone() };
            smart.psd_raw = None;
            layer.psd_blocks.retain(|(key, _)| ![b"SoLd", b"SoLE", b"PlLd"].contains(&key));
            true
        };
        let states = self.docs.iter().map(|state| map_state(state, &mut memo, &mut freeze)).collect();
        Ok(PreparedSourceOverwrite { states })
    }

    /// A filesystem overwrite reset is bookkeeping, not a new Develop edit.
    /// Rebase every undo/redo/snapshot copy as well as current documents.
    pub fn acknowledge_library_overwrite(&mut self, library: &str, photo: u64, path: &str, settings: &Value) {
        let baseline = Arc::new(settings.clone());
        let mut memo = HashMap::new();
        let mut acknowledge = |layer: &mut Layer| {
            let LayerContent::Smart(smart) = &mut layer.content else {
                return false;
            };
            if source_of(smart) != Some(path) {
                return false;
            }
            let Some(link) = smart.develop.as_mut().filter(|link| link.library.as_deref() == Some(library) && link.photo == Some(photo)) else {
                return false;
            };
            if link.library_baseline.as_deref() == Some(settings) {
                return false;
            }
            link.library_baseline = Some(baseline.clone());
            true
        };
        self.docs = self.docs.iter().map(|state| map_state(state, &mut memo, &mut acknowledge)).collect();
    }
}
