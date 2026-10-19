use super::*;

#[test]
fn video_folder_picker_cancels_without_running_and_explicit_paths_bypass_it() {
    use std::cell::Cell;
    use std::rc::Rc;
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let mut app = PhotocraftApp::new(
        photocraft_engine::Session::new(),
        crate::Services {
            file_dialog: Some(crate::file_dialog::answering(move |request| {
                assert_eq!(request, crate::FileDialogRequest::Folder);
                count.set(count.get() + 1);
                None
            })),
            ..Default::default()
        },
    );
    app.run("file.new", json!({"width": 2, "height": 2})).unwrap();
    let ctx = egui::Context::default();
    for (command, param) in [("file.export.renderVideo", "dir"), ("file.import.videoFramesToLayers", "path")] {
        assert_eq!(invoke(&mut app, &ctx, command, json!({})).unwrap()["cancelled"], true);
        let before = calls.get();
        let _ = invoke(&mut app, &ctx, command, json!({param: "", "format": "invalid"}));
        assert_eq!(calls.get(), before);
    }
    assert_eq!(calls.get(), 2);
}

#[test]
fn render_video_uses_the_selected_folder_and_preserves_options() {
    let dir = std::env::temp_dir().join(format!(
        "li-render-video-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let path = dir.to_string_lossy().into_owned();
    let mut app = PhotocraftApp::new(
        photocraft_engine::Session::new(),
        crate::Services {
            file_dialog: Some(crate::file_dialog::answering(move |_| Some(crate::FileDialogAnswer::Paths(vec![path.clone()])))),
            ..Default::default()
        },
    );
    app.run("file.new", json!({"width": 2, "height": 2})).unwrap();
    let result = invoke(&mut app, &egui::Context::default(), "file.export.renderVideo", json!({"format": "bmp"})).unwrap();
    assert_eq!(result["frames"], 1);
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|entry| entry.unwrap().path()).collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].extension().unwrap(), "bmp");
    let imported = invoke(&mut app, &egui::Context::default(), "file.import.videoFramesToLayers", json!({})).unwrap();
    assert_eq!(imported["layers"], 1);
    std::fs::remove_dir_all(dir).unwrap();
}
