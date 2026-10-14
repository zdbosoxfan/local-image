//! App workflow reuse: an exact full-batch witness can skip representative selection, while
//! normalization/light and active previews still use the ordinary locked-style solver.
use super::*;
use batch_cache::SharedStyle;

pub(super) fn retained_working_bytes(source: &DecodedSource) -> u64 {
    (crate::media::decoded_source_bytes(source) as u64).saturating_add((source.image.data.len() as u64).saturating_mul(20)).saturating_add(16 << 20)
}

/// The main source supplies fallback decoder facts for legacy smart previews. The old
/// heavy reservation is released before the larger audit; no nested heavy slot is acquired.
pub(super) fn validate_audit<'a>(
    input: &Input,
    source: &DecodedSource,
    info: &SourceInfo,
    permit: &mut lightcraft_sysmem::Permit<'a>,
    stop: &dyn Fn() -> bool,
) -> std::result::Result<Option<batch_cache::SourceEvidence>, String> {
    let Some(expected) = &input.verification_check else { return Ok(None) };
    let retained = retained_working_bytes(source);
    release_decode_for_verification(permit);
    let evidence = {
        let _audit = lightcraft_sysmem::heavy_jobs().admit(input.verification_bytes.saturating_add(retained), stop).ok_or("Cancelled")?;
        let verified = lightcraft_sysmem::in_background_pool(|| input.verification.load_source())?;
        let effective = verified.info_or(info.clone());
        expected.validated(&verified, &effective).ok_or("Larger verification source changed; recompute representative style")?
    };
    if stop() {
        return Err("Cancelled".into());
    }
    let mut solve = lightcraft_sysmem::heavy_jobs().admit(retained, stop).ok_or("Cancelled")?;
    solve.finish_heavy(retained);
    *permit = solve;
    Ok(Some(evidence))
}

impl SolveWork {
    pub(super) fn complete_shared_style(
        &self,
        first: &Matched,
        representative: PhotoId,
        selected: &DevelopSettings,
        control: &Control,
    ) -> Option<Arc<SharedStyle>> {
        if control.cancel.load(Ordering::Relaxed) || !self.failed.is_empty() || !first.failed.is_empty() || first.source_changed {
            return None;
        }
        let key = self.batch_key.clone()?;
        if !key.resources_current() {
            return None;
        }
        let solved = first.rows.iter().map(|(identity, _)| identity.id).collect::<Vec<_>>();
        let audited = first.audited.iter().copied().collect::<Vec<_>>();
        SharedStyle::complete(
            key,
            selected,
            self.opts,
            representative,
            &solved,
            &audited,
            first.source_evidence.clone(),
            first.verification_evidence.clone(),
        )
        .map(Arc::new)
    }

    fn locked_work(&self, inputs: Vec<Input>, cache: &SharedStyle) -> SolveWork {
        SolveWork {
            generation: self.generation,
            inputs,
            looks: self.looks.clone(),
            opts: self.opts,
            shared_color: Some(cache.color.clone()),
            batch_key: None,
            cached_style: None,
            preview_id: self.preview_id,
            selection_ids: self.selection_ids.clone(),
            report_progress: self.report_progress,
            source_budget: self.source_budget,
            failed: Vec::new(),
        }
    }

    fn cancelled_reuse(&self) -> Matched {
        Matched {
            decoded: Vec::new(),
            source_evidence: Default::default(),
            source_changed: false,
            audited: Default::default(),
            verification_evidence: Default::default(),
            shared_style: None,
            cache_options: None,
            cache_invalidated: false,
            cache_used: false,
            generation: self.generation,
            reference_looks: self.looks.clone(),
            rows: Vec::new(),
            applications: Default::default(),
            failed: Vec::new(),
            preview: None,
            preview_before: None,
            preview_context: None,
            elapsed_ms: 0.0,
            load_ms: 0.0,
            solve_ms: 0.0,
            renders: 0,
            workers: 1,
            per_photo: Vec::new(),
        }
    }

    fn checked_input(input: &Input, cache: &SharedStyle) -> Input {
        let mut input = input.clone();
        input.reload_check = Some(cache.sources.get(&input.identity.id).cloned().map_or(ReloadCheck::Unverifiable, ReloadCheck::Exact));
        input.verification_check = cache.verification.get(&input.identity.id).cloned();
        input
    }

    /// Return None only when an ordinary fresh solve must replace the speculative attempt.
    /// No session edits or cache publication happen on this worker path.
    pub(super) fn try_shared_style(&self, control: &Control, preview: bool, parallel: bool) -> Option<Matched> {
        let cache = self.cached_style.as_ref()?;
        if !self.failed.is_empty() || !cache.metadata_current(self.generation, &self.inputs, &self.looks, self.opts) {
            return None;
        }
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
        let started = web_time::Instant::now();
        let attempted = if preview {
            self.cached_preview(cache, control)
        } else {
            let inputs = self.inputs.iter().map(|input| Self::checked_input(input, cache)).collect();
            Ok(self.locked_work(inputs, cache).run_independent(control, false, parallel))
        };
        if control.cancel.load(Ordering::Relaxed) {
            return Some(self.cancelled_reuse());
        }
        let mut result = match attempted {
            Ok(result)
                if !result.source_changed
                    && result.failed.is_empty()
                    && result.rows.len() == if preview { 1 } else { self.inputs.len() }
                    && cache.metadata_current(self.generation, &self.inputs, &self.looks, self.opts) =>
            {
                result
            }
            _ => {
                // A late mismatch invalidates every speculative row. Restart the normal
                // progress recipe, rather than publishing partial cached-style edits.
                if self.report_progress {
                    control.completed.store(0, Ordering::Relaxed);
                }
                return None;
            }
        };
        result.cache_used = true;
        // Publication follows complete validation. Rebound weak tokens improve the
        // next warm check without retaining any image, confidence plane or profile.
        let mut refreshed = (**cache).clone();
        refreshed.sources.extend(result.source_evidence.clone());
        refreshed.verification.extend(result.verification_evidence.clone());
        result.shared_style = Some(Arc::new(refreshed));
        result.cache_options = Some(self.opts);
        result.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        for row in &mut result.per_photo {
            row["consistentBatch"] = json!(true);
            row["sharedColorSource"] = json!(cache.representative);
            row["sharedStyleReused"] = json!(true);
        }
        Some(result)
    }

    fn cached_preview(&self, cache: &SharedStyle, control: &Control) -> std::result::Result<Matched, String> {
        let stop = || control.cancel.load(Ordering::Relaxed);
        let mut active = None;
        let mut sources = std::collections::BTreeMap::new();
        let mut verification = std::collections::BTreeMap::new();
        let loaded_at = web_time::Instant::now();
        for input in &self.inputs {
            if stop() {
                return Err("Cancelled".into());
            }
            let checked = Self::checked_input(input, cache);
            let mut permit = lightcraft_sysmem::heavy_jobs().admit(checked.bytes, &stop).ok_or("Cancelled")?;
            let source = lightcraft_sysmem::in_background_pool(|| checked.source.load_source())?;
            let info = source.info_or(checked.info.clone());
            let evidence = cache
                .sources
                .get(&input.identity.id)
                .and_then(|evidence| evidence.validated(&source, &info))
                .ok_or("Batch source changed; recompute representative style")?;
            sources.insert(input.identity.id, evidence);
            permit.finish_heavy(retained_working_bytes(&source));
            if let Some(evidence) = validate_audit(&checked, &source, &info, &mut permit, &stop)? {
                verification.insert(input.identity.id, evidence);
            }
            if Some(input.identity.id) == self.preview_id && crate::media::decoded_source_bytes(&source) <= self.source_budget {
                active = Some(source);
            }
            if self.report_progress {
                control.completed.fetch_add(1, Ordering::Relaxed);
            }
        }
        if stop() || !cache.key.resources_current() {
            return Err("Cancelled or camera resources changed".into());
        }
        let mut input = self.inputs.iter().find(|input| Some(input.identity.id) == self.preview_id).cloned().ok_or("Missing preview target")?;
        // Only the bounded active source survives validation. Return a cold source
        // to the ordinary MediaCache warming boundary after the locked refit.
        let warm_active = !matches!(input.source, SourceRef::Loaded(_));
        let retained_active = active.clone();
        if let Some(source) = active {
            input.bytes = retained_working_bytes(&source);
            input.source = SourceRef::Loaded(Box::new(source));
        }
        // All audit witnesses passed. A non-retained active source is reloaded and checked
        // again before refit; no second large audit is required for the same witness set.
        input.reload_check = Some(sources.get(&input.identity.id).cloned().map_or(ReloadCheck::Unverifiable, ReloadCheck::Exact));
        input.verification_check = None;
        let validation_ms = loaded_at.elapsed().as_secs_f64() * 1000.0;
        let mut result = self.locked_work(vec![input], cache).run_independent(control, true, false);
        result.load_ms += validation_ms;
        // Prefer the final active refit's tokens if that source needed another load.
        sources.extend(std::mem::take(&mut result.source_evidence));
        result.source_evidence = sources;
        result.verification_evidence = verification;
        if warm_active
            && !result.source_changed
            && result.failed.is_empty()
            && !stop()
            && let (Some(id), Some(source)) = (self.preview_id, retained_active)
        {
            let mut retained = RetainedSources::new(self.source_budget);
            for (index, (retained_id, retained_source)) in result.decoded.drain(..).enumerate() {
                retained.insert(index, retained_id, retained_source);
            }
            retained.insert(self.inputs.len(), id, source);
            result.decoded = retained.into_vec();
        }
        Ok(result)
    }
}

/// UI/session boundary shared by Preview and Apply. Rebuild only cheap catalog snapshots;
/// no file reads or pixel validation belong here.
pub(super) fn publish_shared_style(session: &mut Session, result: &Matched) {
    if session.library_generation != result.generation || selected_looks(session).as_deref() != Some(result.reference_looks.as_ref()) {
        return;
    }
    if result.cache_invalidated {
        session.match_look.shared_style = None;
    }
    let (Some(cache), Some(options)) = (&result.shared_style, result.cache_options) else { return };
    let inputs = cache
        .key
        .targets
        .iter()
        .map(|&id| {
            let mut input = super::input(session, id).ok()?;
            prepare_reapplication(session, &mut input, options);
            Some(input)
        })
        .collect::<Option<Vec<_>>>();
    if inputs.as_ref().is_some_and(|inputs| cache.metadata_current(session.library_generation, inputs, &result.reference_looks, options)) {
        session.match_look.shared_style = Some(cache.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "timing diagnostic; run explicitly with --ignored --nocapture"]
    fn shared_style_source_witness_reports_warm_and_evicted_validation_cost() {
        for (width, height) in [(512, 384), (2560, 1920)] {
            // Fixture construction and warmup are outside every reported interval.
            let image = Arc::new(lightcraft_raster::Rgb32f::from_fn(width, height, |x, y| {
                [0.1 + x as f32 / (2 * width) as f32, 0.2 + y as f32 / (2 * height) as f32, 0.3]
            }));
            let table = lightcraft_color::profile::HsvTable {
                hue_divisions: 16,
                sat_divisions: 16,
                val_divisions: 16,
                data: vec![[0.0, 1.0, 1.0]; 4096],
                srgb_value: false,
            };
            let info = SourceInfo {
                raw: true,
                sensor_scale: 12.0,
                raw_clip_level: Some(0.99),
                clip_confidence: Some(Arc::new(lightcraft_pipeline::ClipConfidence { width, height, data: vec![0.0; width * height] })),
                camera_profile: Some(Arc::new(lightcraft_color::profile::ProfileLook {
                    hue_sat_map: [Some(table.clone()), Some(table.clone())],
                    look_table: Some(table),
                    tone_curve: None,
                })),
                ..Default::default()
            };
            let source = DecodedSource::new(image, Some(info.clone()));
            let mut cold_info = info.clone();
            cold_info.clip_confidence = info.clip_confidence.as_ref().map(|clip| Arc::new((**clip).clone()));
            cold_info.camera_profile = info.camera_profile.as_ref().map(|profile| Arc::new((**profile).clone()));
            let cold = DecodedSource::new(Arc::new((*source.image).clone()), Some(cold_info.clone()));
            let evidence = batch_cache::SourceEvidence::capture(&source, &info).unwrap();
            assert!(evidence.matches(&source, &info) && evidence.matches(&cold, &cold_info));
            let started = web_time::Instant::now();
            for _ in 0..2 {
                assert!(batch_cache::SourceEvidence::capture(&source, &info).is_some());
            }
            let capture_ms = started.elapsed().as_secs_f64() * 1000.0 / 2.0;
            let started = web_time::Instant::now();
            for _ in 0..20 {
                assert!(evidence.matches(&source, &info));
            }
            let warm_ms = started.elapsed().as_secs_f64() * 1000.0 / 20.0;
            let started = web_time::Instant::now();
            for _ in 0..3 {
                assert!(evidence.matches(&cold, &cold_info));
            }
            let cold_ms = started.elapsed().as_secs_f64() * 1000.0 / 3.0;
            eprintln!(
                "source witness {width}x{height} + full clip + 3x4096-node profile: capture={capture_ms:.3}ms/item, warm={warm_ms:.4}ms/item, evicted-identical={cold_ms:.3}ms/item; owned source bytes={}",
                crate::media::decoded_source_bytes(&source)
            );
        }
    }
}
