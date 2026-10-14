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
