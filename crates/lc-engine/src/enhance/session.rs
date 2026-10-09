//! The session side of AI Remove and AI Denoise: applying finished jobs as undo steps, the state
//! of a photo's AI results for the UI, and what a render uses ([`for_render`]).

use std::sync::Arc;

use lightcraft_catalog::PhotoId;
use lightcraft_develop::{AiKey, AiPatch, DenoiseRef, DevelopSettings, Spot, SpotMode};
use lightcraft_preview::Hash128;
use lightcraft_raster::Rgb32f;
use serde::Serialize;

use super::store::{self, Kind};
use super::{CANCELLED, JobKind, Outcome, denoise, remove};
use crate::Session;

/// What [`Session::enhance_poll`] did.
#[derive(Debug, Default)]
pub struct Polled {
    /// Settings changed (repaint).
    pub changed: bool,
    /// Finished jobs, as "AI Remove", "AI Denoise"… (for a toast).
    pub done: Vec<String>,
    /// Errors of failed jobs.
    pub errors: Vec<String>,
}

/// The default Denoise amount when Denoise first runs on a photo.
pub const DEFAULT_AMOUNT: f64 = 60.0;

impl Session {
    /// Apply finished AI jobs: each becomes one undo step on its photo ("AI Remove",
    /// "Regenerate", "AI Denoise"), on top of the photo's settings as they are now. Waits while a
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
            Outcome::Denoise(r) => {
                let (Some(key), Some(source)) = (AiKey::parse(&r.key), AiKey::parse(&r.source)) else {
                    return Err(crate::EngineError::Other("bad Denoise result key".into()));
                };
                d.enhance.ai = Some(DenoiseRef { key, source });
                if d.enhance.denoise <= 0.0 {
                    d.enhance.denoise = DEFAULT_AMOUNT;
                }
                self.set_develop(photo, d, label)?;
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
    if patch.source != denoise::source_hash(p) {
        return PatchState::Foreign;
    }
    if !store::exists(Kind::Remove, &patch.key) {
        return PatchState::Missing;
    }
    if patch.geometry != remove::geometry_tag(&p.develop) { PatchState::Stale } else { PatchState::Ok }
}

/// AI Denoise on a photo, as Develop shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum DenoiseState {
    /// Not for this photo (`why`).
    Unavailable {
        why: String,
    },
    /// The model isn't installed (`download`: its download, when one ran).
    NoModel {
        download: Option<super::Download>,
    },
    /// Ready to run.
    Ready,
    Running {
        progress: f32,
        job: u64,
    },
    /// The result is in use.
    Done {
        amount: f64,
    },
    /// The settings refer to a result the store doesn't have: run it again.
    Missing,
}

/// The AI Denoise state of photo `id`.
pub fn denoise_state(s: &Session, id: PhotoId) -> DenoiseState {
    if let Err(why) = denoise::can_denoise(s, id) {
        return DenoiseState::Unavailable { why };
    }
    if let Some(j) = s.enhance.running_for(id).find(|j| j.kind == JobKind::Denoise) {
        return DenoiseState::Running { progress: j.ctl.progress(), job: j.id };
    }
    let (Some(p), Some(d)) = (s.catalog.photo(id), s.develop_of(id)) else { return DenoiseState::Unavailable { why: "no such photo".into() } };
    if let Some(r) = d.enhance.ai
        && r.source.to_string() == denoise::source_hash(p)
    {
        return if denoise::result_exists(&r.key.to_string()) { DenoiseState::Done { amount: d.enhance.denoise } } else { DenoiseState::Missing };
    }
    if s.denoiser().is_err() {
        let download = s.enhance.host.as_ref().and_then(|h| h.model_download(li_seg::denoise::DENOISE_ID));
        return DenoiseState::NoModel { download };
    }
    DenoiseState::Ready
}

/// What a render of a photo with content hash `source` uses: its AI Denoise result mixed into the
/// decoded source by the Denoise amount (when there is one for this photo), and settings without
/// AI patches made from other photos. Unchanged inputs come back as they are (same buffers).
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
    let img = match s.enhance.ai {
        Some(r) if s.enhance.denoise > 0.0 && s.section_enabled("detail") && hex.as_deref() == Some(r.source.to_string().as_str()) => {
            denoise::denoised_source(src, &r.key.to_string(), s.enhance.denoise)
        }
        _ => None,
    };
    (img.unwrap_or_else(|| src.clone()), settings)
}
