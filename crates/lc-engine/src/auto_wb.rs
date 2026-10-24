//! White balance modes that a paste, sync, Auto Sync or preset brings to other photos, resolved
//! for each photo: Auto is measured on the photo's own source and a named mode takes its values.
//! A source that isn't decoded yet is measured by the app's background task when
//! [`Session::defer_auto_wb`] is on, so these commands never decode raws on the UI thread.

use lightcraft_catalog::{Op, PhotoId};
use lightcraft_develop::{DevelopSettings, WbMode};
use lightcraft_pipeline::SourceInfo;
use serde_json::{Value, json};

use crate::media::{SourceLevel, SourceRef};
use crate::{Result, Session};

/// The background task's label and the undo step of what it measured.
pub const AUTO_WB: &str = "Auto White Balance";

/// Photos whose Auto white balance waits to be measured, with what a worker needs to measure them.
pub struct AutoWbWork {
    generation: u64,
    inputs: Vec<(PhotoId, SourceRef, SourceInfo)>,
}

/// Auto white balances (K, tint) measured by [`AutoWbWork::run_job`].
pub struct AutoWbMeasured {
    generation: u64,
    values: Vec<(PhotoId, (f64, f64))>,
}

impl AutoWbWork {
    pub fn len(&self) -> usize {
        self.inputs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    /// Measure photo by photo (the app's task): stops at the next photo once `job` is cancelled and
    /// reports `step` = photos done / total. A photo whose source can't be read is left out.
    pub fn run_job<S>(self, job: &lightcraft_sysmem::JobCtl<S>) -> AutoWbMeasured {
        let total = u32::try_from(self.inputs.len()).unwrap_or(u32::MAX);
        job.set_step(0, total);
        let mut values = Vec::new();
        for (done, (id, source, header)) in self.inputs.into_iter().enumerate() {
            if job.is_cancelled() {
                break;
            }
            match crate::guard::catch(AUTO_WB, || source.load_source()).and_then(|r| r) {
                Ok(src) => values.push((id, lightcraft_pipeline::auto::auto_wb(&src.image, &src.info_or(header)))),
                Err(e) => log::warn!("Auto white balance kept the copied values for photo {}: {e}", id.0),
            }
            job.set_step(u32::try_from(done + 1).unwrap_or(u32::MAX), total);
        }
        AutoWbMeasured { generation: self.generation, values }
    }
}

impl Session {
    /// Resolve the white balance mode that `partial` brings to photo `id` in its new `settings`:
    /// Auto measured on the photo's own source, a named mode or As Shot with the photo's values,
    /// Custom as copied.
    pub(crate) fn resolve_transferred_wb(&mut self, id: PhotoId, partial: &Value, settings: &mut DevelopSettings) {
        if partial.get("wb").and_then(|w| w.get("mode")).is_none() {
            return;
        }
        let resolved = match settings.wb.mode {
            WbMode::Custom => None,
            WbMode::Auto => self.measure_auto_wb(id),
            mode => lightcraft_pipeline::local::mode_wb(&self.source_info(id), mode),
        };
        if let Some((temp, tint)) = resolved {
            settings.wb.temp = temp;
            settings.wb.tint = tint;
        }
    }

    /// Photo `id`'s Auto white balance from its decoded source. Without one it is decoded now, or
    /// with [`Session::defer_auto_wb`] queued for [`Session::take_auto_wb_work`]; `None` keeps the
    /// copied numbers meanwhile.
    fn measure_auto_wb(&mut self, id: PhotoId) -> Option<(f64, f64)> {
        let photo = self.catalog.photo(id)?.clone();
        if let SourceRef::Loaded(src) = self.media.source_ref(&photo, SourceLevel::Thumb) {
            return Some(lightcraft_pipeline::auto::auto_wb(&src.image, &self.source_info(id)));
        }
        if self.defer_auto_wb {
            let entry = (self.library_generation, id);
            if !self.pending_auto_wb.contains(&entry) {
                self.pending_auto_wb.push(entry);
            }
            return None;
        }
        match self.source_now(id, SourceLevel::Thumb) {
            Ok(src) => Some(lightcraft_pipeline::auto::auto_wb(&src, &self.source_info(id))),
            Err(e) => {
                log::warn!("Auto white balance kept the copied values for photo {}: {e}", id.0);
                None
            }
        }
    }

    /// The queued Auto white balance measurements of photos still set to Auto, for a worker;
    /// `None` when nothing waits.
    pub fn take_auto_wb_work(&mut self) -> Option<AutoWbWork> {
        let generation = self.library_generation;
        let mut inputs = Vec::new();
        for (g, id) in std::mem::take(&mut self.pending_auto_wb) {
            let Some(photo) = self.catalog.photo(id).filter(|p| g == generation && p.develop.wb.mode == WbMode::Auto).cloned() else {
                continue;
            };
            let source = self.media.source_ref(&photo, SourceLevel::Thumb);
            inputs.push((id, source, self.source_info(id)));
        }
        (!inputs.is_empty()).then_some(AutoWbWork { generation, inputs })
    }

    /// Store measured Auto white balances on the photos still set to Auto, as one undo step. A
    /// photo being edited (a slider drag) waits for the next round; results for another library
    /// are dropped. Returns `{changed}`.
    pub fn apply_auto_wb(&mut self, measured: AutoWbMeasured) -> Result<Value> {
        if measured.generation != self.library_generation {
            return Ok(json!({"changed": 0}));
        }
        let mut ops = Vec::new();
        for (id, (temp, tint)) in measured.values {
            let Some(d) = self.develop_of(id).filter(|d| d.wb.mode == WbMode::Auto && (d.wb.temp, d.wb.tint) != (temp, tint)) else {
                continue;
            };
            if self.interaction.as_ref().is_some_and(|i| i.photo == id) {
                self.pending_auto_wb.push((measured.generation, id));
                continue;
            }
            let mut d = (*d).clone();
            d.wb.temp = temp;
            d.wb.tint = tint;
            ops.extend(self.develop_op(id, d, AUTO_WB));
        }
        let n = ops.len();
        if n > 0 {
            self.commit(AUTO_WB, Op::Batch { ops })?;
        }
        Ok(json!({"changed": n}))
    }
}
