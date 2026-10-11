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
    pub(crate) job_errors: Vec<(u64, String)>,
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
                    let error = format!("{}: {e}", job.label);
                    polled.job_errors.push((job.id, error.clone()));
                    polled.errors.push(error);
                    continue;
                }
            };
            let photo = job.photo;
            let kind = job.kind.clone();
            let label = job.label.clone();
            let r = self.execute_fn("enhance.apply", |s| {
                s.apply_outcome(photo, &kind, job.target_patch.as_deref(), outcome, &label).map(serde_json::Value::Bool)
            });
            match r {
                Ok(serde_json::Value::Bool(true)) => {
                    polled.changed = true;
                    polled.done.push(label);
                }
                Ok(_) => {}
                Err(e) => {
                    let error = format!("{label}: {e}");
                    polled.job_errors.push((job.id, error.clone()));
                    polled.errors.push(error);
                }
            }
        }
        polled
    }

    fn apply_outcome(&mut self, photo: PhotoId, kind: &JobKind, target_patch: Option<&str>, outcome: Outcome, label: &str) -> crate::Result<bool> {
        let mut d = (*self.develop_of(photo).ok_or_else(|| crate::EngineError::Other("the photo is gone".into()))?).clone();
        match outcome {
            Outcome::Denoise(r) => {
                let key = lightcraft_develop::AiKey::parse(&r.key).ok_or_else(|| crate::EngineError::Other("invalid denoise key".into()))?;
                let source = lightcraft_develop::AiKey::parse(&r.source).ok_or_else(|| crate::EngineError::Other("invalid denoise source".into()))?;
                // A reloaded/replaced photo must never receive the old worker's result.
                if self.catalog.photo(photo).is_none_or(|p| super::denoise::source_hash(p) != r.source) {
                    return Ok(false);
                }
                let first_result = d.enhance.ai.is_none();
                d.enhance.ai = Some(lightcraft_develop::DenoiseRef { key, source, model: r.family, preview: r.preview });
                if first_result && d.enhance.denoise == 0.0 {
                    d.enhance.denoise = 100.0;
                }
                self.set_develop(photo, d, label)?;
            }
            Outcome::DenoiseExport(_) => return Ok(false),
            Outcome::Remove(r) => {
                if self.catalog.photo(photo).is_none_or(|p| super::source_hash(p) != r.source) {
                    return Ok(false);
                }
                if r.geometry != remove::geometry_tag(&d) {
                    return Err(crate::EngineError::Other("The photo geometry changed while AI Remove was running; try again.".into()));
                }
                let patch = AiPatch {
                    key: r.key,
                    source: r.source,
                    rect: r.rect,
                    engine: r.engine,
                    seed: r.seed,
                    geometry: r.geometry,
                    orientation: Default::default(),
                };
                match kind {
                    JobKind::Regenerate { .. } => {
                        let sp = d
                            .spots
                            .iter_mut()
                            .find(|s| s.is_ai() && s.patch.as_ref().is_some_and(|p| Some(p.key.as_str()) == target_patch))
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
        Ok(true)
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
    let mixed = settings
        .enhance
        .ai
        .filter(|a| {
            hex.as_deref() == Some(&a.source.to_string())
                && (settings.enhance.denoise_model == lightcraft_develop::DenoiseModel::Auto || settings.enhance.denoise_model == a.model)
                && !settings.disabled_sections.iter().any(|s| s == "detail")
        })
        .and_then(|a| super::denoise::denoised_source(src, &a.key.to_string(), settings.enhance.denoise));
    (mixed.unwrap_or_else(|| src.clone()), settings)
}
