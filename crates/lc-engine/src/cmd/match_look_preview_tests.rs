use super::*;

fn fixture(new_layer: bool, consistent_batch: bool) -> (Session, Vec<PhotoId>, Vec<Input>, Options, Arc<AtomicUsize>) {
    let mut session = Session::with_demo();
    let ids = session.catalog.photos().take(3).map(|photo| photo.id).collect::<Vec<_>>();
    let image = |cast: [f32; 3]| {
        Arc::new(lightcraft_raster::Rgb32f::from_fn(32, 24, |x, y| {
            let l = 0.04 + x as f32 / 96.0;
            let p = match y / 6 {
                0 => [l; 3],
                1 => [l * 1.3, l * 0.75, l * 0.6],
                2 => [l * 0.6, l * 0.9, l * 1.25],
                _ => [l * 0.7, l, l * 0.75],
            };
            std::array::from_fn(|c| p[c] * cast[c])
        }))
    };
    for (index, id) in ids.iter().enumerate() {
        let mut settings = DevelopSettings::default();
        settings.color.saturation = index as f64 * 18.0 - 18.0;
        settings.grading.midtones.hue = 35.0 + index as f64 * 100.0;
        settings.grading.midtones.sat = index as f64 * 5.0;
        settings.masks.push(lightcraft_develop::Mask { id: 7, name: "Existing".into(), ..Default::default() });
        session.set_develop(*id, settings, "Fixture").unwrap();
    }
    let mut desired = DevelopSettings::default();
    desired.light.exposure = 0.35;
    desired.color.saturation = -25.0;
    desired.grading.shadows.hue = 210.0;
    desired.grading.shadows.sat = 8.0;
    let opts = Options { new_layer, consistent_batch, strength: 57.0, light_contrast: false, ..Default::default() };
    let looks = vec![match_look::analyze(&image([1.0; 3]), &SourceInfo::default(), &desired)];
    session.saved_looks.file.looks.push(SavedLook {
        id: "preview-context-fixture".into(),
        name: "Preview fixture".into(),
        reference_count: looks.len(),
        looks,
        options: Some(opts),
        ..Default::default()
    });
    session.match_look.selected = Some("preview-context-fixture".into());
    session.execute("library.select", &json!({"ids":ids,"active":ids[2]})).unwrap();
    let decodes = Arc::new(AtomicUsize::new(0));
    let mut inputs = ids.iter().map(|id| input(&mut session, *id).unwrap()).collect::<Vec<_>>();
    for (index, input) in inputs.iter_mut().enumerate() {
        let pixels = image([[0.5, 0.8, 1.3], [1.0; 3], [1.5, 0.9, 0.6]][index]);
        let decodes = decodes.clone();
        input.verification = SourceRef::Loaded(Box::new(DecodedSource::new(pixels.clone(), Some(SourceInfo::default()))));
        // The synthetic File loader replaces a demo photo's pixels. Install its
        // verification level too, so warm Apply audits the same fixture rather
        // than decoding the unrelated original demo scene at Preview resolution.
        session.media.insert_source(input.identity.id, SourceLevel::Preview, DecodedSource::new(pixels.clone(), Some(SourceInfo::default())));
        input.source = SourceRef::File {
            path: format!("preview-context-{index}"),
            max_edge: 32,
            fallback: None,
            loader: Some(Arc::new(move |_, _| {
                decodes.fetch_add(1, Ordering::Relaxed);
                Ok(((*pixels).clone(), SourceInfo::default()))
            })),
        };
    }
    (session, ids, inputs, opts, decodes)
}
fn params(ids: &[PhotoId], opts: Options, preview: Option<PhotoId>) -> Value {
    let mut params = serde_json::to_value(opts).unwrap();
    params["ids"] = json!(ids);
    if let Some(id) = preview {
        params["previewId"] = json!(id);
    }
    params
}

#[test]
fn consistent_preview_uses_full_batch_and_exact_apply_settings_for_active_target() {
    for new_layer in [false, true] {
        let (mut session, ids, inputs, opts, decodes) = fixture(new_layer, true);
        let active = ids[2];
        let mut work = prepare(&mut session, &params(&ids, opts, Some(active))).unwrap();
        work.inputs = inputs.clone();
        assert_eq!(work.len(), 3);
        assert_eq!(work.progress_len(true), 4);
        let control = Control::default();
        let mut preview = work.run(&control, true);
        assert_eq!(control.completed.load(Ordering::Relaxed), 4);
        assert_eq!(preview.rows.iter().map(|row| row.0.id).collect::<Vec<_>>(), vec![active]);
        assert_eq!(preview.applications.len(), 1, "only the active target should be refit after full-batch style selection");
        assert_eq!(preview.decoded.len(), 3);
        assert_eq!(decodes.load(Ordering::Relaxed), 3);
        let context = preview.preview_context.clone().unwrap();
        assert!(context.current(&session));
        retain_decoded(&mut session, &mut preview);
        assert!(
            ids.iter().all(|id| matches!(input(&mut session, *id).unwrap().source, SourceRef::Loaded(_))),
            "cache all captured batch pixels for Apply"
        );
        let work = prepare(&mut session, &params(&ids, opts, None)).unwrap();
        assert_eq!(work.progress_len(false), 3);
        let applied = work.run(&Control::default(), false);
        assert_eq!(decodes.load(Ordering::Relaxed), 3, "Apply must reuse full-preview batch decoding");
        let expected = applied.rows.iter().find(|row| row.0.id == active).unwrap();
        assert_eq!(preview.rows[0], *expected);
        assert_eq!(preview.applications[&active].full, applied.applications[&active].full);
        assert_eq!(
            preview.per_photo[0]["sharedColorSource"],
            applied.per_photo.iter().find(|row| row["id"] == json!(active)).unwrap()["sharedColorSource"]
        );
        let source = session.source_now(active, SourceLevel::Thumb).unwrap();
        let pixels =
            lightcraft_pipeline::render(&source, &SourceInfo::default(), &expected.1, &lightcraft_pipeline::RenderRequest::fit(400, 400)).image;
        assert_eq!(preview.preview.as_ref(), Some(&pixels));
        // Regression premise: selecting style from only the active photo changes the result.
        let mut narrowed = prepare(&mut session, &params(&[active], opts, Some(active))).unwrap();
        narrowed.selection_ids = session.targets(&Value::Null);
        let narrowed = narrowed.run(&Control::default(), true);
        assert_ne!(narrowed.rows[0].1, expected.1, "fixture must distinguish full-batch style selection from the old narrowed preview");
        let before = ids.iter().map(|id| session.develop_of(*id).unwrap()).collect::<Vec<_>>();
        assert_eq!(apply(&mut session, applied).unwrap()["changed"], 3);
        assert!(!context.current(&session));
        session.execute("edit.undo", &json!({})).unwrap();
        assert!(context.current(&session));
        for (id, settings) in ids.iter().zip(before) {
            assert_eq!(session.develop_of(*id).unwrap(), settings);
        }
    }
}

#[test]
fn preview_context_rejects_non_active_edits_selection_look_and_library_changes() {
    let (mut session, ids, inputs, opts, _) = fixture(false, true);
    let mut work = prepare(&mut session, &params(&ids, opts, Some(ids[2]))).unwrap();
    work.inputs = inputs;
    // An edit to another batch member while work runs invalidates even an unchanged active target.
    let original = session.develop_of(ids[0]).unwrap();
    let mut edited = (*original).clone();
    edited.light.contrast = 31.0;
    session.set_develop(ids[0], edited, "Concurrent non-active edit").unwrap();
    let preview = work.run(&Control::default(), true);
    let context = preview.preview_context.unwrap();
    assert!(!context.current(&session));
    session.execute("edit.undo", &json!({})).unwrap();
    assert!(context.current(&session));
    session.execute("library.select", &json!({"ids":[ids[1],ids[2]],"active":ids[2]})).unwrap();
    assert!(!context.current(&session));
    session.execute("library.select", &json!({"ids":ids,"active":ids[1]})).unwrap();
    assert!(!context.current(&session));
    session.execute("library.select", &json!({"ids":ids,"active":ids[2]})).unwrap();
    assert!(context.current(&session));
    session.saved_looks.file.looks[0].looks[0].importance = 2.0;
    assert!(!context.current(&session));
    session.saved_looks.file.looks[0].looks[0].importance = 1.0;
    assert!(context.current(&session));
    session.library_generation += 1;
    assert!(!context.current(&session));
}

#[test]
fn preview_cancellation_returns_no_partial_batch_or_image_and_target_is_validated() {
    let (mut session, ids, mut inputs, opts, _) = fixture(true, true);
    assert!(prepare(&mut session, &params(&ids[1..], opts, Some(ids[0]))).is_err());
    let cancel = Arc::new(Control::default());
    let stop = cancel.clone();
    inputs[0].source = SourceRef::File {
        path: "cancel-preview-stage-one".into(),
        max_edge: 16,
        fallback: None,
        loader: Some(Arc::new(move |_, _| {
            stop.cancel.store(true, Ordering::Relaxed);
            Ok((lightcraft_raster::Rgb32f::from_fn(16, 12, |_, _| [0.1, 0.2, 0.3]), SourceInfo::default()))
        })),
    };
    let mut work = prepare(&mut session, &params(&ids, opts, Some(ids[2]))).unwrap();
    work.inputs = inputs;
    let result = work.run(&cancel, true);
    assert!(result.rows.is_empty() && result.applications.is_empty());
    assert!(result.preview.is_none() && result.preview_before.is_none());
}

#[test]
fn independent_preview_uses_same_solver_as_batch_apply() {
    for new_layer in [false, true] {
        let (mut session, ids, inputs, opts, _) = fixture(new_layer, false);
        let mut preview_work = prepare(&mut session, &params(&ids, opts, Some(ids[2]))).unwrap();
        preview_work.inputs = inputs.clone();
        let mut apply_work = prepare(&mut session, &params(&ids, opts, None)).unwrap();
        apply_work.inputs = inputs;
        assert_eq!(preview_work.progress_len(true), 1);
        let control = Control::default();
        let preview = preview_work.run(&control, true);
        let apply = apply_work.run(&Control::default(), false);
        assert_eq!(control.completed.load(Ordering::Relaxed), 1);
        assert_eq!(preview.rows.len(), 1);
        assert_eq!(preview.rows[0], *apply.rows.iter().find(|row| row.0.id == ids[2]).unwrap());
    }
}

#[test]
fn shared_style_reuse_skips_stage_one_for_apply_active_and_amount_changes() {
    for new_layer in [false, true] {
        let (mut session, ids, inputs, opts, _) = fixture(new_layer, true);
        let mut initial = prepare(&mut session, &params(&ids, opts, Some(ids[2]))).unwrap();
        initial.inputs = inputs;
        let mut first = initial.run(&Control::default(), true);
        assert!(!first.cache_used && first.shared_style.is_some(), "complete stage one must produce reusable evidence");
        retain_decoded(&mut session, &mut first);
        assert!(session.match_look.shared_style.is_some());
        for amount in [opts.strength, 83.0] {
            let options = Options { strength: amount, ..opts };
            for active in [ids[1], ids[2]] {
                session.execute("library.select", &json!({"ids": ids, "active": active})).unwrap();
                let cached = prepare(&mut session, &params(&ids, options, Some(active))).unwrap();
                assert!(cached.cached_style.is_some());
                let control = Control::default();
                let mut result = cached.run(&control, true);
                assert!(result.cache_used && result.failed.is_empty());
                assert_eq!(control.completed.load(Ordering::Relaxed), ids.len() + 1);
                assert!(result.preview_context.as_ref().unwrap().current(&session));
                let mut fresh = prepare(&mut session, &params(&ids, options, Some(active))).unwrap();
                fresh.cached_style = None;
                let fresh = fresh.run(&Control::default(), true);
                assert_eq!(result.rows, fresh.rows);
                assert_eq!(result.preview, fresh.preview);
                assert_eq!(result.preview_before, fresh.preview_before);
                assert!(result.renders < fresh.renders, "reuse must skip full-batch representative fitting");
                retain_decoded(&mut session, &mut result);
            }
            let cached = prepare(&mut session, &params(&ids, options, None)).unwrap().run(&Control::default(), false);
            assert!(cached.cache_used);
            let mut fresh = prepare(&mut session, &params(&ids, options, None)).unwrap();
            fresh.cached_style = None;
            let fresh = fresh.run(&Control::default(), false);
            assert_eq!(cached.rows, fresh.rows);
            for id in &ids {
                assert_eq!(cached.applications[id].full, fresh.applications[id].full);
            }
            let before = ids.iter().map(|id| session.develop_of(*id).unwrap()).collect::<Vec<_>>();
            assert_eq!(apply(&mut session, cached).unwrap()["changed"], json!(ids.len()));
            session.execute("edit.undo", &json!({})).unwrap();
            for (id, settings) in ids.iter().zip(before) {
                assert_eq!(session.develop_of(*id).unwrap(), settings);
            }
        }
    }
}

#[test]
fn cached_preview_returns_only_bounded_cold_active_source_for_media_warming() {
    let (mut session, ids, inputs, opts, decodes) = fixture(false, true);
    let active = ids[2];
    let mut initial = prepare(&mut session, &params(&ids, opts, Some(active))).unwrap();
    initial.inputs = inputs.clone();
    initial.source_budget = 0;
    let mut first = initial.run(&Control::default(), true);
    retain_decoded(&mut session, &mut first);
    session.media.clear_sources();
    let mut work = prepare(&mut session, &params(&ids, opts, Some(active))).unwrap();
    work.inputs = inputs.clone();
    let before = decodes.load(Ordering::Relaxed);
    let mut result = work.run(&Control::default(), true);
    assert!(result.cache_used && result.failed.is_empty());
    assert_eq!(decodes.load(Ordering::Relaxed) - before, ids.len(), "the bounded active source is reused for refit");
    assert_eq!(result.decoded.len(), 1);
    assert_eq!(result.decoded[0].0, active);
    let active_pixels = Arc::downgrade(&result.decoded[0].1.image);
    retain_decoded(&mut session, &mut result);
    let warm = prepare(&mut session, &params(&ids, opts, Some(active))).unwrap();
    let source = &warm.inputs.iter().find(|input| input.identity.id == active).unwrap().source;
    assert!(matches!(source, SourceRef::Loaded(_)));
    assert!(Arc::ptr_eq(&source.load_source().unwrap().image, &active_pixels.upgrade().unwrap()));
    drop(warm);
    session.media.clear_sources();
    assert!(active_pixels.upgrade().is_none(), "the style cache's rebound witness must not pin the warmed source");

    let mut oversize = prepare(&mut session, &params(&ids, opts, Some(active))).unwrap();
    oversize.inputs = inputs;
    oversize.source_budget = 0;
    let result = oversize.run(&Control::default(), true);
    assert!(result.cache_used && result.decoded.is_empty(), "oversize sources remain streaming/reloaded rather than retained");
}

#[test]
fn shared_style_reuse_validates_evicted_pixels_facts_and_late_mismatches() {
    let (mut session, ids, inputs, opts, _) = fixture(false, true);
    let mut initial = prepare(&mut session, &params(&ids, opts, Some(ids[2]))).unwrap();
    initial.inputs = inputs.clone();
    initial.source_budget = 0;
    let mut first = initial.run(&Control::default(), true);
    assert!(first.decoded.is_empty() && first.shared_style.is_some());
    retain_decoded(&mut session, &mut first);
    session.media.clear_sources();
    let mut cached = prepare(&mut session, &params(&ids, opts, None)).unwrap();
    cached.inputs = inputs.clone();
    cached.source_budget = 0;
    let result = cached.run(&Control::default(), false);
    assert!(result.cache_used && result.decoded.is_empty() && result.failed.is_empty());
    // The final member changes only after earlier members have produced speculative rows.
    for changed_facts in [false, true] {
        let mut changed = inputs.clone();
        let pixels = changed[2].verification.load_source().unwrap().image;
        changed[2].source = SourceRef::File {
            path: "late-change".into(),
            max_edge: 32,
            fallback: None,
            loader: Some(Arc::new(move |_, _| {
                let mut image = (*pixels).clone();
                let mut info = SourceInfo::default();
                if changed_facts {
                    info.sensor_scale = 2.0;
                } else {
                    image.data[0][0] += 0.02;
                }
                Ok((image, info))
            })),
        };
        let mut cached = prepare(&mut session, &params(&ids, opts, None)).unwrap();
        cached.inputs = changed.clone();
        cached.source_budget = 0;
        let control = Control::default();
        let started = web_time::Instant::now();
        let result = cached.run(&control, false);
        let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert!(
            result.elapsed_ms >= wall_ms * 0.95,
            "fallback wall time must include speculative validation/refits: {} vs {wall_ms}",
            result.elapsed_ms
        );
        assert!(!result.cache_used && result.cache_invalidated && result.failed.is_empty());
        assert_eq!(control.completed.load(Ordering::Relaxed), ids.len(), "fallback must not accumulate speculative progress");
        let mut fresh = prepare(&mut session, &params(&ids, opts, None)).unwrap();
        fresh.inputs = changed;
        fresh.cached_style = None;
        fresh.source_budget = 0;
        let fresh = fresh.run(&Control::default(), false);
        assert_eq!(result.rows, fresh.rows, "late mismatch must discard all speculative cached settings");
    }
}

#[test]
fn shared_style_reuse_requires_every_larger_audit_and_does_not_publish_on_cancel() {
    let (mut session, ids, mut inputs, opts, _) = fixture(false, true);
    let source = inputs[1].verification.load_source().unwrap();
    let mut desired = (*inputs[1].baseline).clone();
    desired.light.exposure += 0.03;
    session.saved_looks.file.looks[0].looks = vec![match_look::analyze(&source.image, &SourceInfo::default(), &desired)];
    let pixels = Arc::new(lightcraft_raster::resample::fit(&source.image, 64, 48, lightcraft_raster::resample::Filter::Box));
    // fit never upscales: use an exact nearest-neighbour duplicate as a larger audit fixture.
    let pixels = Arc::new(lightcraft_raster::Rgb32f::from_fn(64, 48, |x, y| pixels.get(x / 2, y / 2)));
    let changed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let audit_calls = Arc::new(AtomicUsize::new(0));
    inputs[1].verification = SourceRef::File {
        path: "larger-audit".into(),
        max_edge: 64,
        fallback: None,
        loader: Some(Arc::new({
            let changed = changed.clone();
            let audit_calls = audit_calls.clone();
            move |_, _| {
                audit_calls.fetch_add(1, Ordering::Relaxed);
                let mut image = (*pixels).clone();
                if changed.load(Ordering::Relaxed) {
                    image.data[0][1] += 0.03;
                }
                Ok((image, SourceInfo::default()))
            }
        })),
    };
    let mut initial = prepare(&mut session, &params(&ids, opts, Some(ids[2]))).unwrap();
    initial.inputs = inputs.clone();
    initial.source_budget = 0;
    let mut first = initial.run(&Control::default(), true);
    let cache = first.shared_style.as_ref().expect("complete audited stage one");
    assert!(cache.verification.contains_key(&ids[1]), "the fixture must actually perform the larger first-stage audit");
    retain_decoded(&mut session, &mut first);
    session.media.clear_sources();
    let initial_calls = audit_calls.load(Ordering::Relaxed);
    let mut cached = prepare(&mut session, &params(&ids, opts, Some(ids[2]))).unwrap();
    cached.inputs = inputs.clone();
    cached.source_budget = 0;
    assert!(cached.run(&Control::default(), true).cache_used);
    assert!(audit_calls.load(Ordering::Relaxed) > initial_calls, "non-active audit witnesses must be validated too");
    changed.store(true, Ordering::Relaxed);
    let mut invalid = prepare(&mut session, &params(&ids, opts, None)).unwrap();
    invalid.inputs = inputs.clone();
    invalid.source_budget = 0;
    let invalid = invalid.run(&Control::default(), false);
    assert!(!invalid.cache_used && invalid.cache_invalidated);
    changed.store(false, Ordering::Relaxed);
    let mut failed_audit_inputs = inputs.clone();
    failed_audit_inputs[1].verification = SourceRef::File {
        path: "unreadable-larger-audit".into(),
        max_edge: 64,
        fallback: None,
        loader: Some(Arc::new(|_, _| Err("Audit read failed".into()))),
    };
    let mut failed_audit = prepare(&mut session, &params(&ids, opts, None)).unwrap();
    failed_audit.inputs = failed_audit_inputs;
    failed_audit.cached_style = None;
    let failed_audit = failed_audit.run(&Control::default(), false);
    assert!(failed_audit.shared_style.is_none(), "a failed required audit must disable cache publication");
    let prior = session.match_look.shared_style.clone().unwrap();
    let control = Control::default();
    control.cancel.store(true, Ordering::Relaxed);
    let mut cancelled = prepare(&mut session, &params(&ids, opts, None)).unwrap();
    cancelled.inputs = inputs;
    let mut cancelled = cancelled.run(&control, false);
    assert!(cancelled.rows.is_empty() && cancelled.shared_style.is_none());
    retain_decoded(&mut session, &mut cancelled);
    assert!(Arc::ptr_eq(session.match_look.shared_style.as_ref().unwrap(), &prior));
}

#[test]
fn incomplete_or_cancelled_stage_one_never_publishes_a_shared_style() {
    let (mut session, ids, inputs, opts, _) = fixture(false, true);
    let mut incomplete = prepare(&mut session, &params(&ids, opts, None)).unwrap();
    incomplete.inputs = inputs.clone();
    incomplete.failed.push(json!([999, "Incomplete target"]));
    let mut incomplete = incomplete.run(&Control::default(), false);
    assert!(incomplete.shared_style.is_none());
    retain_decoded(&mut session, &mut incomplete);
    assert!(session.match_look.shared_style.is_none());
    let mut cancelled = prepare(&mut session, &params(&ids, opts, None)).unwrap();
    cancelled.inputs = inputs;
    let control = Control::default();
    control.cancel.store(true, Ordering::Relaxed);
    let mut cancelled = cancelled.run(&control, false);
    assert!(cancelled.shared_style.is_none() && cancelled.rows.is_empty());
    retain_decoded(&mut session, &mut cancelled);
    assert!(session.match_look.shared_style.is_none());
}

#[test]
fn cancelled_independent_apply_discards_genuine_partial_work_and_creates_no_undo() {
    let (mut session, ids, mut inputs, opts, decodes) = fixture(false, false);
    let control = Arc::new(Control::default());
    let image = inputs[1].verification.load_source().unwrap().image;
    inputs[1].source = SourceRef::File {
        path: "cancel-after-first-fit".into(),
        max_edge: 32,
        fallback: None,
        loader: Some(Arc::new({
            let control = control.clone();
            move |_, _| {
                assert_eq!(control.completed.load(Ordering::Relaxed), 1, "one target must have finished before cancellation");
                control.cancel.store(true, Ordering::Relaxed);
                Ok(((*image).clone(), SourceInfo::default()))
            }
        })),
    };
    let mut work = prepare(&mut session, &params(&ids, opts, None)).unwrap();
    work.inputs = inputs;
    let before = ids.iter().map(|id| session.develop_of(*id).unwrap()).collect::<Vec<_>>();
    let undo = session.undo.len();
    let result = work.run_with_mode(&control, false, false);
    assert_eq!(decodes.load(Ordering::Relaxed), 1, "the first target was loaded and fitted");
    assert!(result.rows.is_empty() && result.applications.is_empty() && result.shared_style.is_none());
    assert_eq!(apply(&mut session, result).unwrap()["changed"], 0);
    assert_eq!(session.undo.len(), undo);
    for (id, settings) in ids.iter().zip(before) {
        assert_eq!(session.develop_of(*id).unwrap(), settings);
    }
}

#[test]
fn larger_verification_releases_decode_slot_but_preserves_all_admitted_bytes() {
    const MIB: u64 = 1 << 20;
    let jobs = lightcraft_sysmem::HeavyJobs::new(2, 100 * MIB, || lightcraft_sysmem::MemoryNow { available: (1 << 30) + 100 * MIB, rss: Some(0) });
    let mut audit = jobs.try_admit(40 * MIB).unwrap();
    let second_decode = jobs.try_admit(30 * MIB).unwrap();
    assert!(jobs.try_admit(10 * MIB).is_none(), "two decode slots are occupied before verification load finishes");
    finish_verification_decode(&mut audit);
    assert_eq!(audit.bytes(), 40 * MIB, "CPU audit retains its entire forecast");
    assert_eq!(jobs.in_flight(), (1, 70 * MIB));
    let third_decode = jobs.try_admit(10 * MIB).expect("CPU audit must not hold a decode slot");
    assert_eq!(jobs.in_flight(), (2, 80 * MIB));
    drop(third_decode);
    assert!(jobs.try_admit(40 * MIB).is_none(), "free decode slots do not bypass memory pressure");
    drop(audit);
    assert_eq!(jobs.in_flight(), (1, 30 * MIB));
    let after_audit = jobs.try_admit(40 * MIB).unwrap();
    drop(after_audit);
    drop(second_decode);
    assert_eq!(jobs.in_flight(), (0, 0));
}

#[test]
fn larger_verification_resource_transition_preserves_near_match_results_failure_and_cancel() {
    let (mut session, ids, inputs, opts, _) = fixture(false, false);
    let id = ids[1];
    let mut input = inputs[1].clone();
    let source = input.source.load_source().unwrap();
    let info = source.info_or(input.info.clone());
    let mut desired = (*input.baseline).clone();
    desired.light.exposure += 0.03;
    session.saved_looks.file.looks[0].looks = vec![match_look::analyze(&source.image, &info, &desired)];
    let references = selected_looks(&session).unwrap();
    let verified = Arc::new(lightcraft_raster::Rgb32f::from_fn(64, 48, |x, y| source.image.get(x / 2, y / 2)));
    input.verification = SourceRef::Loaded(Box::new(DecodedSource::new(verified.clone(), Some(info.clone()))));
    // Reproduce the original solve and the unchanged larger-audit decision as oracle.
    let solved = match_look::solve_batch_with_faces(&source.image, &info, &input.baseline, &references, opts, &input.faces, &|| false).unwrap();
    assert!(solved.before_error < 0.05 && solved.settings != *input.identity.settings, "fixture must enter the actual audit branch");
    let errors = lightcraft_raster::sequential(|| {
        legacy_near_match_errors(&verified, &info, &input.identity.settings, &solved.settings, &references, &solved.weights, opts, solved.similarity)
    });
    let (expected, expected_full) = if errors.1 >= errors.0 {
        ((*input.identity.settings).clone(), (*input.identity.settings).clone())
    } else {
        (solved.settings.clone(), solved.full_settings.clone())
    };
    let mut work = prepare(&mut session, &params(&[id], opts, None)).unwrap();
    work.inputs = vec![input.clone()];
    let result = work.run_with_mode(&Control::default(), false, false);
    assert_eq!(result.audited.iter().copied().collect::<Vec<_>>(), vec![id]);
    assert_eq!(result.renders, solved.renders + 2);
    assert_eq!(serde_json::to_vec(&result.rows[0].1).unwrap(), serde_json::to_vec(&expected).unwrap());
    assert_eq!(serde_json::to_vec(&result.applications[&id].full).unwrap(), serde_json::to_vec(&expected_full).unwrap());
    assert!(result.verification_evidence.contains_key(&id));

    let mut failed = input.clone();
    failed.verification = SourceRef::File {
        path: "failed-audit".into(),
        max_edge: 64,
        fallback: None,
        loader: Some(Arc::new(|_, _| Err("audit decode failed".into()))),
    };
    let mut work = prepare(&mut session, &params(&[id], opts, None)).unwrap();
    work.inputs = vec![failed];
    let failed = work.run_with_mode(&Control::default(), false, false);
    assert_eq!(failed.rows[0].1, *input.identity.settings);
    assert_eq!(failed.applications[&id].full, *input.identity.settings);
    assert_eq!(failed.renders, solved.renders);
    assert!(failed.audited.contains(&id) && !failed.verification_evidence.contains_key(&id));

    let control = Arc::new(Control::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let mut cancelled = input;
    cancelled.verification = SourceRef::File {
        path: "cancel-during-audit-load".into(),
        max_edge: 64,
        fallback: None,
        loader: Some(Arc::new({
            let control = control.clone();
            let calls = calls.clone();
            move |_, _| {
                calls.fetch_add(1, Ordering::Relaxed);
                control.cancel.store(true, Ordering::Relaxed);
                Ok(((*verified).clone(), info.clone()))
            }
        })),
    };
    let mut work = prepare(&mut session, &params(&[id], opts, None)).unwrap();
    work.inputs = vec![cancelled];
    let undo = session.undo.len();
    let result = work.run_with_mode(&control, false, false);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(result.rows.is_empty() && result.applications.is_empty());
    assert_eq!(apply(&mut session, result).unwrap()["changed"], 0);
    assert_eq!(session.undo.len(), undo);
}

// Original complete public-measure audit, retained independently as an exact oracle.
#[allow(clippy::too_many_arguments)]
fn legacy_near_match_errors(
    src: &Arc<lightcraft_raster::Rgb32f>,
    info: &SourceInfo,
    before: &DevelopSettings,
    after: &DevelopSettings,
    refs: &[Look],
    weights: &[f64],
    opts: Options,
    similarity: f64,
) -> (f64, f64) {
    let edge = 1024.min(src.width.max(src.height));
    let req = lightcraft_pipeline::RenderRequest::fit(edge, edge);
    let a = match_look::measure(&lightcraft_pipeline::render(src, info, before, &req).image);
    let b = match_look::measure(&lightcraft_pipeline::render(src, info, after, &req).image);
    let (goal, _) = match_look::reachable_goal(&a, &match_look::blend(refs, weights), opts, similarity);
    (match_look::distance(&a, &goal, opts), match_look::distance(&b, &goal, opts))
}
#[test]
fn options_aware_near_audit_matches_original_full_measure_errors_and_decisions() {
    let src = Arc::new(lightcraft_raster::Rgb32f::from_fn(80, 60, |x, y| {
        let v = 0.02 + x as f32 / 120.0;
        if y < 30 { [v; 3] } else { [v * 1.2, v * 0.7, v * 0.5] }
    }));
    let info = SourceInfo {
        raw: true,
        sensor_scale: 7.0,
        raw_clip_level: Some(0.99),
        clip_confidence: Some(Arc::new(lightcraft_pipeline::ClipConfidence { width: 19, height: 13, data: vec![0.7; 19 * 13] })),
        ..Default::default()
    };
    let before = DevelopSettings::default();
    let mut edited = before.clone();
    edited.light.exposure = 0.04;
    edited.light.highlights = -15.0;
    let mut layered = before.clone();
    layered.masks.push(lightcraft_develop::Mask {
        id: 7,
        name: "Audit layer".into(),
        components: vec![lightcraft_develop::MaskComponent {
            shape: lightcraft_develop::MaskShape::Full,
            op: Default::default(),
            invert: false,
            name: None,
        }],
        tools: lightcraft_develop::LayerTools { light: Some(edited.light), ..Default::default() },
        ..Default::default()
    });
    let mut mono = edited.clone();
    mono.treatment = lightcraft_develop::Treatment::Bw;
    let refs = [match_look::analyze(&src, &info, &edited), match_look::analyze(&src, &info, &mono)];
    for appearance_aware in [false, true] {
        for (index, after) in [&before, &edited, &layered, &mono].into_iter().enumerate() {
            let opts = Options {
                appearance_aware,
                consistent_batch: index % 2 == 0,
                region_aware: index % 2 != 0,
                new_layer: index == 2,
                normalize: index != 3,
                keep_exposure: index == 3,
                keep_white_balance: index == 2,
                strength: [0.0, 57.0, 100.0, 125.0][index],
                ..Default::default()
            };
            let (new, old) = lightcraft_raster::sequential(|| {
                (
                    near_match_errors(&src, &info, &before, after, &refs, &[0.3, 0.7], opts, 0.91),
                    legacy_near_match_errors(&src, &info, &before, after, &refs, &[0.3, 0.7], opts, 0.91),
                )
            });
            assert_eq!([new.0, new.1].map(f64::to_bits), [old.0, old.1].map(f64::to_bits));
            assert_eq!(new.1 >= new.0, old.1 >= old.0);
            if index == 0 {
                assert_eq!(new.0.to_bits(), new.1.to_bits());
                assert!(new.1 >= new.0);
            }
        }
    }
}
