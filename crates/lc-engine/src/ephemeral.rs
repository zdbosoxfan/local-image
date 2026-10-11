//! local-image: an **ephemeral photo** for Compositing's Camera Raw Filter. The host hands the
//! Develop module a layer's pixels: they are written to a temporary 16-bit TIFF (Rec.2020
//! primaries, so nothing is clipped) and shown as a photo that exists only in memory. It is never
//! journaled, snapshotted or given an XMP sidecar, and it is not listed in the grid (a `local`
//! record outside any browsed folder). Closing it returns its develop settings, removes it, drops
//! the undo steps that touched it, restores what was selected before and deletes the file.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lightcraft_catalog::{Op, Photo, PhotoId, Source};
use serde_json::Value;

use crate::{EngineError, LibrarySource, Result, Selection, Session};

/// File name prefix of the temporary files (see [`clear_dir`]).
const PREFIX: &str = "camera-raw-";

/// The open ephemeral photo and what to restore when it closes.
#[derive(Debug)]
pub struct Ephemeral {
    pub photo: PhotoId,
    pub file: PathBuf,
    selection: Selection,
    source: LibrarySource,
    previous_active: Option<PhotoId>,
    active_mask: Option<u32>,
    active_spot: Option<usize>,
}

/// Deletes temporary files left in `dir` (by a crash, or an earlier session).
pub fn clear_dir(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if e.file_name().to_string_lossy().starts_with(PREFIX) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// 16-bit RGB TIFF bytes of linear Rec.2020 pixels (sRGB-curve encoded, tagged with a matching ICC
/// profile so the decoder returns the same linear values).
fn encode(width: usize, height: usize, rgb: &[[f32; 3]]) -> Result<Vec<u8>> {
    use lightcraft_codecs::{EncodeImage, EncodeMeta, NamedSpace, Samples, TiffCompression, Trc};
    let enc = |v: f32| (lightcraft_color::transfer::linear_to_srgb(v.clamp(0.0, 1.0)) * 65535.0 + 0.5) as u16;
    let samples: Vec<u16> = rgb.iter().flat_map(|p| p.map(enc)).collect();
    let icc = lightcraft_codecs::icc::write_matrix_trc(&NamedSpace::Rec2020.rgb_space(), &Trc::Srgb);
    let img = EncodeImage::new(width as u32, height as u32, 3, Samples::U16(&samples));
    lightcraft_codecs::encode_tiff(&img, TiffCompression::Deflate, &EncodeMeta { icc: Some(&icc), ..Default::default() })
        .map_err(|e| EngineError::Other(format!("can't write the Camera Raw image: {e}")))
}

/// Whether an operation touches the filter and/or the Library. Global catalog
/// operations count as Library edits even when they name no photos.
pub(crate) fn op_scope(op: &Op, photo: PhotoId) -> (bool, bool) {
    if let Op::Batch { ops } = op {
        return ops.iter().map(|op| op_scope(op, photo)).fold((false, false), |a, b| (a.0 || b.0, a.1 || b.1));
    }
    let mut ids = Vec::new();
    op.photo_ids(&mut |id| ids.push(id));
    (ids.contains(&photo), ids.is_empty() || ids.iter().any(|id| *id != photo))
}

pub(crate) fn only_photo(op: &Op, photo: PhotoId) -> bool {
    op_scope(op, photo) == (true, false)
}

impl Session {
    /// The open ephemeral photo, if any.
    pub fn ephemeral_photo(&self) -> Option<PhotoId> {
        self.ephemeral.as_ref().map(|e| e.photo)
    }

    /// Opens `rgb` (`width × height` linear Rec.2020 pixels) as an ephemeral photo named `name`
    /// with develop `settings` (JSON, missing fields = defaults), writing its file into `dir`, and
    /// makes it the active photo. An ephemeral photo already open is closed first.
    pub fn open_ephemeral(&mut self, dir: &Path, name: &str, width: usize, height: usize, rgb: &[[f32; 3]], settings: &Value) -> Result<PhotoId> {
        if width == 0 || height == 0 || rgb.len() != width * height {
            return Err(EngineError::Other("the Camera Raw image is empty".into()));
        }
        let settings: lightcraft_develop::DevelopSettings =
            serde_json::from_value(settings.clone()).map_err(|e| EngineError::Other(format!("develop settings: {e}")))?;
        self.close_ephemeral();
        let _ = self.end_interaction();
        let bytes = encode(width, height, rgb)?;
        let hash = lightcraft_preview::hash::hash_bytes(&bytes).to_string();
        std::fs::create_dir_all(dir).map_err(|e| EngineError::Other(format!("{}: {e}", dir.display())))?;
        let file = dir.join(format!("{PREFIX}{}.tif", &hash[..16]));
        std::fs::write(&file, &bytes).map_err(|e| EngineError::Other(format!("{}: {e}", file.display())))?;
        let id = self.catalog.alloc_photo_id();
        let now = (self.clock)();
        let mut p = Photo::new(id, Source::File { path: file.to_string_lossy().to_string() }, name, "TIFF", width as u32, height as u32, &now);
        p.file_size = bytes.len() as u64;
        p.content_hash = Some(format!("ephemeral:{hash}"));
        p.local = true;
        p.develop = Arc::new(settings);
        if let Err(e) = self.catalog.apply(Op::AddPhoto { photo: Box::new(p) }) {
            let _ = std::fs::remove_file(&file);
            return Err(e.into());
        }
        self.ephemeral = Some(Ephemeral {
            photo: id,
            file,
            selection: std::mem::replace(&mut self.selection, Selection::single(id)),
            source: self.source,
            previous_active: self.previous_active,
            active_mask: self.active_mask.take(),
            active_spot: self.active_spot.take(),
        });
        Ok(id)
    }

    /// Closes the ephemeral photo: returns its develop settings (JSON), removes it (nothing is
    /// saved), drops its undo steps, restores the selection and deletes its file.
    pub fn close_ephemeral(&mut self) -> Option<Value> {
        self.ephemeral.as_ref()?;
        // Finish under the filter scope so the Library redo remains untouched.
        let _ = self.end_interaction();
        let e = self.ephemeral.take()?;
        let settings = self.develop_of(e.photo).and_then(|d| serde_json::to_value(&*d).ok());
        self.undo.retain(|u| !only_photo(&u.op, e.photo));
        self.redo.retain(|u| !only_photo(&u.op, e.photo));
        let _ = self.catalog.apply(Op::RemovePhoto { id: e.photo });
        self.strip_ephemeral_ops(e.photo);
        self.before.remove(&e.photo);
        self.selection = e.selection;
        self.source = e.source;
        self.previous_active = e.previous_active;
        self.active_mask = e.active_mask;
        self.active_spot = e.active_spot;
        let _ = std::fs::remove_file(&e.file);
        settings
    }

    /// Drops queued log ops that only concern the ephemeral photo (they must never reach disk).
    pub(crate) fn strip_ephemeral_log(&mut self) {
        if let Some(id) = self.ephemeral_photo() {
            self.strip_ephemeral_ops(id);
        }
    }

    fn strip_ephemeral_ops(&mut self, id: PhotoId) {
        fn keep(op: Op, id: PhotoId) -> Option<Op> {
            match op {
                Op::Batch { ops } => {
                    let ops: Vec<Op> = ops.into_iter().filter_map(|o| keep(o, id)).collect();
                    (!ops.is_empty()).then_some(Op::Batch { ops })
                }
                op => {
                    let mut ids = Vec::new();
                    op.photo_ids(&mut |i| ids.push(i));
                    (ids.is_empty() || ids.iter().any(|i| *i != id)).then_some(op)
                }
            }
        }
        let ops = std::mem::take(&mut self.pending_log);
        self.pending_log = ops.into_iter().filter_map(|o| keep(o, id)).collect();
    }

    /// Whether `id` is the ephemeral photo (no sidecar, no auto version).
    pub(crate) fn is_ephemeral(&self, id: PhotoId) -> bool {
        self.ephemeral_photo() == Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lc-ephemeral-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn pixels() -> Vec<[f32; 3]> {
        (0..24 * 16).map(|i| [(i % 24) as f32 / 24.0, (i / 24) as f32 / 16.0, 0.3]).collect()
    }

    #[test]
    fn camera_raw_history_cannot_undo_hidden_library_edits() {
        let d = dir("history-scope");
        let mut s = Session::with_demo();
        let original = s.active().unwrap();
        s.execute("develop.set", &json!({"control":"light.exposure","value":1.2})).unwrap();
        let library = s.develop_of(original).unwrap();
        let library_steps = s.undo.len();
        let photo = s.open_ephemeral(&d, "Filter", 24, 16, &pixels(), &json!({})).unwrap();
        assert!(s.undo_entry().is_none());
        assert!(s.execute("edit.undo", &json!({})).is_err());
        assert_eq!(s.develop_of(original).unwrap(), library);
        for (control, value) in [("light.exposure", 0.5), ("light.contrast", 20.0)] {
            s.execute("develop.set", &json!({"control":control,"value":value})).unwrap();
        }
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(s.execute("edit.undo", &json!({})).is_err());
        assert_eq!(s.develop_of(photo).unwrap().light.exposure, 0.0);
        assert_eq!(s.develop_of(original).unwrap(), library);
        s.close_ephemeral().unwrap();
        assert_eq!(s.undo.len(), library_steps);
        assert!(s.redo.is_empty());
        s.execute("edit.undo", &json!({})).unwrap();
        assert_ne!(s.develop_of(original).unwrap(), library);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn camera_raw_history_preserves_library_redo_even_when_closing_a_live_interaction() {
        for live in [false, true] {
            let d = dir(if live { "redo-live" } else { "redo" });
            let mut s = Session::with_demo();
            let original = s.active().unwrap();
            s.execute("develop.set", &json!({"control":"light.exposure","value":1.2})).unwrap();
            s.execute("edit.undo", &json!({})).unwrap();
            let label = s.redo_entry().unwrap().label.clone();
            let before = s.develop_of(original).unwrap();
            s.open_ephemeral(&d, "Filter", 24, 16, &pixels(), &json!({})).unwrap();
            assert!(s.redo_entry().is_none());
            assert!(s.execute("edit.redo", &json!({})).is_err());
            if live {
                s.begin_interaction("Filter Exposure").unwrap();
            }
            s.execute("develop.set", &json!({"control":"light.exposure","value":0.7})).unwrap();
            let settings = s.close_ephemeral().unwrap();
            assert_eq!(settings["light"]["exposure"].as_f64().unwrap() as f32, 0.7);
            assert_eq!(s.develop_of(original).unwrap(), before);
            assert_eq!(s.redo_entry().unwrap().label, label);
            s.execute("edit.redo", &json!({})).unwrap();
            assert_eq!(s.develop_of(original).unwrap().light.exposure, 1.2);
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn camera_raw_history_keeps_background_library_commits_separate() {
        let d = dir("background-history");
        let mut s = Session::with_demo();
        let original = s.active().unwrap();
        let rating = s.catalog.photo(original).unwrap().rating;
        let photo = s.open_ephemeral(&d, "Filter", 24, 16, &pixels(), &json!({})).unwrap();
        s.execute("develop.set", &json!({"control":"light.exposure","value":0.7})).unwrap();
        s.undo_step().unwrap();
        // This is the commit boundary used by background catalog tasks while the filter is visible.
        let new_rating = if rating == 5 { 4 } else { 5 };
        s.commit("Background Library rating", Op::SetRating { id: original, rating: new_rating }).unwrap();
        assert!(s.redo_entry().is_some(), "a Library edit erased filter Redo");
        s.redo_step().unwrap();
        assert_eq!(s.develop_of(photo).unwrap().light.exposure, 0.7);
        s.undo_step().unwrap();
        assert!(s.undo_step().is_err(), "filter Undo reached the background Library commit");
        assert_eq!(s.catalog.photo(original).unwrap().rating, new_rating);
        s.close_ephemeral().unwrap();
        assert_eq!(s.undo_entry().unwrap().label, "Background Library rating");
        s.undo_step().unwrap();
        assert_eq!(s.catalog.photo(original).unwrap().rating, rating);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn camera_raw_history_rejects_mixed_edits_before_any_catalog_change() {
        let d = dir("mixed-history");
        let mut s = Session::with_demo();
        let original = s.active().unwrap();
        let photo = s.open_ephemeral(&d, "Filter", 24, 16, &pixels(), &json!({})).unwrap();
        let before = serde_json::to_value(&s.catalog).unwrap();
        let result = s.commit("Mixed", Op::Batch { ops: vec![Op::SetRating { id: original, rating: 4 }, Op::SetRating { id: photo, rating: 5 }] });
        assert!(result.is_err());
        assert_eq!(serde_json::to_value(&s.catalog).unwrap(), before);
        s.close_ephemeral().unwrap();
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn camera_raw_history_capacity_does_not_evict_library_history() {
        let d = dir("history-capacity");
        let mut s = Session::with_demo();
        let original = s.active().unwrap();
        s.commit("Keep Library step", Op::SetRating { id: original, rating: 4 }).unwrap();
        let photo = s.open_ephemeral(&d, "Filter", 24, 16, &pixels(), &json!({})).unwrap();
        for i in 0..1002 {
            s.commit("Filter edit", Op::SetRating { id: photo, rating: (i % 5) as u8 }).unwrap();
        }
        assert_eq!(s.undo.len(), 1001);
        s.close_ephemeral().unwrap();
        assert_eq!(s.undo.len(), 1);
        assert_eq!(s.undo_entry().unwrap().label, "Keep Library step");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn edits_of_an_ephemeral_photo_never_reach_the_log_and_closing_restores_everything() {
        let d = dir("log");
        let mut s = Session::with_demo();
        let before_sel = s.selection.clone();
        let photos = s.catalog.len();
        s.drain_log();
        let undo0 = s.undo.len();
        let id = s.open_ephemeral(&d, "Layer 1", 24, 16, &pixels(), &json!({"light": {"exposure": 0.3}})).unwrap();
        assert_eq!(s.active(), Some(id));
        assert_eq!(s.develop_of(id).unwrap().light.exposure, 0.3);
        assert!(s.ephemeral.as_ref().unwrap().file.is_file());
        assert!(!s.visible().contains(&id), "not listed in the grid");
        s.execute("develop.set", &json!({"control": "light.exposure", "value": 1.0})).unwrap();
        s.execute("develop.set", &json!({"control": "light.contrast", "value": 20.0})).unwrap();
        assert!(s.undo.len() > undo0);
        s.strip_ephemeral_log();
        assert!(s.drain_log().is_empty(), "nothing journaled for the ephemeral photo");
        let file = s.ephemeral.as_ref().unwrap().file.clone();
        let settings = s.close_ephemeral().unwrap();
        assert_eq!(settings["light"]["exposure"], 1.0);
        assert_eq!(settings["light"]["contrast"], 20.0);
        assert!(s.catalog.photo(id).is_none());
        assert_eq!(s.catalog.len(), photos);
        assert_eq!(s.undo.len(), undo0, "its undo steps are gone");
        assert_eq!(s.selection, before_sel);
        assert!(!file.exists(), "the temporary file is deleted");
        assert!(s.drain_log().is_empty());
        assert!(s.close_ephemeral().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_file_decodes_back_to_the_same_linear_pixels() {
        let d = dir("decode");
        let mut s = Session::new();
        let px = pixels();
        s.open_ephemeral(&d, "x", 24, 16, &px, &json!({})).unwrap();
        let bytes = std::fs::read(&s.ephemeral.as_ref().unwrap().file).unwrap();
        let (img, info) = crate::files::load_bytes(&bytes, usize::MAX).unwrap();
        assert!(!info.raw);
        assert_eq!((img.width, img.height), (24, 16));
        let worst = img.data.iter().zip(&px).flat_map(|(a, b)| (0..3).map(move |c| (a[c] - b[c]).abs())).fold(0.0f32, f32::max);
        assert!(worst < 2e-3, "{worst}");
        s.close_ephemeral();
        clear_dir(&d);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn clear_dir_removes_leftovers_only() {
        let d = dir("clear");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("camera-raw-abc.tif"), b"x").unwrap();
        std::fs::write(d.join("keep.txt"), b"x").unwrap();
        clear_dir(&d);
        assert!(!d.join("camera-raw-abc.tif").exists() && d.join("keep.txt").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
