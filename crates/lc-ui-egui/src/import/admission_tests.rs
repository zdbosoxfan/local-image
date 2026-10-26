use super::*;

#[test]
fn a_second_import_keeps_the_existing_worker_and_browse_scan() {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
    let review = ImportDialog::new(vec![ImportCandidate { path: "second.png".into(), ..Default::default() }]);
    app.import = Some(ImportTask::new(vec!["first.png".into()], json!({"mode":"move"}), app.session.library_generation, false));
    assert!(start(&mut app, &review).unwrap_err().contains("import"));
    assert_eq!(app.import.as_ref().unwrap().queue, vec!["first.png".to_string()]);
    assert_eq!(app.import.as_ref().unwrap().params["mode"], "move");
    assert!(open(&mut app, vec!["second.png".into()]).is_err());
    app.import = None;
    let (tx, rx) = std::sync::mpsc::channel();
    let progress = std::sync::Arc::new(ScanProgress::default());
    app.scan = Some(ScanTask {
        library_generation: app.session.library_generation,
        progress: progress.clone(),
        rx,
        copy: false,
        browse: true,
        sources: vec![],
        scoped: false,
        scoped_ready: None,
    });
    assert!(start(&mut app, &review).is_err());
    assert!(start_paths(&mut app, vec!["second.png".into()]).is_err());
    assert!(app.scan.is_some());
    assert!(!progress.cancel.load(Ordering::Relaxed));
    drop(tx);
}
