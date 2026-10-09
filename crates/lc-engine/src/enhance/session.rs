//! The session side of AI Remove: applying finished jobs as undo steps, the state
//! of a photo's AI results for the UI, and what a render uses ([`for_render`]).

use std::sync::Arc;

use lightcraft_catalog::PhotoId;
use lightcraft_develop::{AiPatch, DevelopSettings, Spot, SpotMode};
use lightcraft_preview::Hash128;
use lightcraft_raster::Rgb32f;
use serde::Serialize;

use super::store::{self, Kind};
use super::{CANCELLED, JobKind, Outcome, remove};
use crate::Session;

/// What [`Session::enhance_poll`] did.
#[derive(Debug, Default)]
pub struct Polled {
    /// Settings changed (repaint).
    pub changed: bool,
    /// Finished jobs, as "AI Remove", "Regenerate"… (for a toast).
    pub done: Vec<String>,
    /// Errors of failed jobs.
    pub errors: Vec<String>,
}

impl Session {
    /// Apply finished AI jobs: each becomes one undo step on its photo ("AI Remove",
    /// "Regenerate"), on top of the photo's settings as they are now. Waits while a
    /// slider drag or brush stroke is in progress. Call it from the frame loop.
    pub fn enhance_poll(&mut self) -> Polled {
        let mut polled = Polled::default();
        if self.interaction.is_some() || !self.enhance.jobs().iter().any(|j| j.finished()) {
            return polled;
        }
        for (job, r) in self.enhance.take_finished() {
            let outcome = match r {
                Ok(o) => o,
                Err(e) if e == CANCELLED => continue,
                Err(e) => {
                    polled.errors.push(format!("{}: {e}", job.label));
                    continue;
                }
            };
            let photo = job.photo;
            let kind = job.kind.clone();
            let label = job.label.clone();
            let r = self.execute_fn("enhance.apply", |s| s.apply_outcome(photo, &kind, outcome, &label).map(|_| serde_json::Value::Null));
            match r {
                Ok(_) => {
                    polled.changed = true;
                    polled.done.push(label);
                }
                Err(e) => polled.errors.push(format!("{label}: {e}")),
            }
        }
        polled
    }

    fn apply_outcome(&mut self, photo: PhotoId, kind: &JobKind, outcome: Outcome, label: &str) -> crate::Result<()> {
        let mut d = (*self.develop_of(photo).ok_or_else(|| crate::EngineError::Other("the photo is gone".into()))?).clone();
        match outcome {
            Outcome::Remove(r) => {
                let patch = AiPatch { key: r.key, source: r.source, rect: r.rect, engine: r.engine, seed: r.seed, geometry: r.geometry };
                match kind {
                    JobKind::Regenerate { spot } => {
                        let sp = d
                            .spots
                            .get_mut(*spot)
                            .filter(|s| s.is_ai())
                            .ok_or_else(|| crate::EngineError::Other("the AI removal was deleted".into()))?;
                        sp.patch = Some(patch);
                    }
                    _ => {
                        let st = r.stroke;
                        d.spots.push(Spot {
                            mode: SpotMode::Ai,
                            points: st.points,
                            polygon: st.polygon,
                            size: st.size,
                            feather: st.feather,
                            opacity: st.opacity,
                            source_offset: None,
                            mask: st.mask,
                            patch: Some(patch),
                        });
                    }
                }
                let n = d.spots.len();
                self.set_develop(photo, d, label)?;
                if self.active() == Some(photo) && !matches!(kind, JobKind::Regenerate { .. }) {
                    self.active_spot = Some(n - 1);
                }
            }
        }
        Ok(())
    }
}

/// An AI spot's patch, as Develop shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PatchState {
    /// In place and current.
    Ok,
    /// Made under another geometry (rotation, lens, perspective): it may not line up.
    Stale,
    /// The generated pixels aren't in the library's store (shown without them).
    Missing,
    /// Made from another photo (settings copied over by hand): never applied.
    Foreign,
}

/// The state of AI spot `spot` of photo `id`.
pub fn patch_state(s: &Session, id: PhotoId, spot: &Spot) -> PatchState {
    let (Some(p), Some(patch)) = (s.catalog.photo(id), spot.patch.as_ref()) else { return PatchState::Missing };
    if patch.source != super::source_hash(p) {
        return PatchState::Foreign;
    }
    if !store::exists(Kind::Remove, &patch.key) {
        return PatchState::Missing;
    }
    if patch.geometry != remove::geometry_tag(&p.develop) { PatchState::Stale } else { PatchState::Ok }
}

/// A render uses settings without AI patches made from other photos.
/// Unchanged inputs come back as they are (same buffers).
pub fn for_render(src: &Arc<Rgb32f>, s: &Arc<DevelopSettings>, source: Option<Hash128>) -> (Arc<Rgb32f>, Arc<DevelopSettings>) {
    let hex = source.map(|h| h.to_string());
    let foreign = |p: &AiPatch| hex.as_deref() != Some(p.source.as_str());
    let settings = if s.spots.iter().any(|sp| sp.patch.as_ref().is_some_and(foreign)) {
        let mut d = (**s).clone();
        for sp in d.spots.iter_mut() {
            if sp.patch.as_ref().is_some_and(foreign) {
                sp.patch = None;
            }
        }
        Arc::new(d)
    } else {
        s.clone()
    };
    (src.clone(), settings)
}
