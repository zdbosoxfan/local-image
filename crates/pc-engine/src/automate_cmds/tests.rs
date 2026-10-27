use super::*;

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("photocraft-automate-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().into_owned()
}

/// Writes `n` small PNGs (w×h, grey level i·40) into `dir`.
fn images(dir: &str, n: usize, w: u32, h: u32) -> Vec<String> {
    (0..n)
        .map(|i| {
            let mut s = Session::new();
            let v = (i as f32 * 40.0 + 20.0) / 255.0;
            s.execute(
                "file.new",
                json!({"width": w, "height": h, "background": format!("#{:02x}{:02x}{:02x}", (v * 255.0) as u8, (v * 255.0) as u8, (v * 255.0) as u8)}),
            )
            .unwrap();
            let path = format!("{dir}/img{i}.png");
            crate::file_cmds::save_doc(&s.active().unwrap().doc, &path, None).unwrap();
            path
        })
        .collect()
}

#[test]
fn scripts_parse_json_and_lines() {
    let a = parse_script(r#"[["file.new", {"width": 10}], {"command": "layer.flattenImage"}, "edit.undo"]"#).unwrap();
    assert_eq!(a.len(), 3);
    assert_eq!(a[0].1["width"], 10);
    let b = parse_script("# make a doc\nfile.new {\"width\": 12, \"height\": 8}\n\nimage.imageRotation.90cw\n").unwrap();
    assert_eq!(b.len(), 2);
    assert_eq!(b[1].0, "image.imageRotation.90cw");
    let c = parse_script(r#"{"photocraftDroplet":1,"action":{"steps":[["file.new",{}]]}}"#).unwrap();
    assert_eq!(c.len(), 1);
    assert!(parse_script("file.new {bad json").is_err());
}

#[test]
fn browse_runs_a_script_file() {
    let dir = tmp("browse");
    let path = format!("{dir}/make.txt");
    std::fs::write(&path, "file.new {\"width\": 30, \"height\": 20}\nimage.imageRotation.90cw\n").unwrap();
    let mut s = Session::new();
    let r = s.execute("file.scripts.browse", json!({"path": path})).unwrap();
    assert_eq!(r["ok"], true);
    let d = &s.active().unwrap().doc;
    assert_eq!((d.size.width, d.size.height), (20, 30));
    assert!(s.execute("file.scripts.browse", json!({"script": "no.such.command"})).is_err());
    // A failing step stops the script and is reported.
    let r = s.execute("file.scripts.browse", json!({"steps": [["layer.delete", {"layer": 999_999}], ["image.imageRotation.90cw", {}]]})).unwrap();
    assert_eq!(r["results"].as_array().unwrap().len(), 1);
}

#[test]
fn script_events_fire_on_new_and_close() {
    let mut s = Session::new();
    let r = s
        .execute("file.scripts.scriptEventsManager", json!({"add": {"event": "newDocument", "steps": [["image.imageRotation.90cw", {}]], "name": "Rotate"}}))
        .unwrap();
    assert_eq!(r["enabled"], true);
    assert_eq!(r["bindings"].as_array().unwrap().len(), 1);
    assert!(s.execute("file.scripts.scriptEventsManager", json!({"add": {"event": "nope", "steps": []}})).is_err());
    assert!(s.execute("file.scripts.scriptEventsManager", json!({"add": {"event": "print", "steps": [["no.such", {}]]}})).is_err());
    s.execute("file.new", json!({"width": 40, "height": 10})).unwrap();
    let d = &s.active().unwrap().doc;
    assert_eq!((d.size.width, d.size.height), (10, 40), "the New Document action rotated it");
    assert_eq!(s.file_menu.event_log.len(), 1);
    // Events don't re-enter: the script's own commands fire nothing.
    s.execute("file.scripts.scriptEventsManager", json!({"add": {"event": "closeDocument", "steps": [["file.new", {"width": 5, "height": 5}]]}})).unwrap();
    s.execute("file.close", json!({})).unwrap();
    assert_eq!(s.documents().len(), 1, "closing ran the script, which made a new document");
    assert_eq!(s.documents()[0].doc.size.width, 5, "…without the New Document rotation");
    // Disabled: nothing fires.
    s.execute("file.scripts.scriptEventsManager", json!({"enabled": false})).unwrap();
    s.execute("file.new", json!({"width": 40, "height": 10})).unwrap();
    assert_eq!(s.active().unwrap().doc.size.width, 40);
    let r = s.execute("file.scripts.scriptEventsManager", json!({"removeAll": true})).unwrap();
    assert!(r["bindings"].as_array().unwrap().is_empty());
    assert_eq!(r["events"].as_array().unwrap().len(), EVENTS.len());
}

#[test]
fn droplets_are_written_and_run() {
    let dir = tmp("droplet");
    let inputs = images(&dir, 2, 40, 20);
    let mut s = Session::new();
    let path = format!("{dir}/Rotate");
    let r = s
        .execute(
            "file.automate.createDroplet",
            json!({"path": path, "steps": [["image.imageRotation.90cw", {}]], "format": "png", "output": format!("{dir}/out")}),
        )
        .unwrap();
    let dp = r["path"].as_str().unwrap().to_string();
    assert!(dp.ends_with("Rotate.pcdroplet"));
    let v: Value = serde_json::from_slice(&std::fs::read(&dp).unwrap()).unwrap();
    assert_eq!(v["photocraftDroplet"], 1);
    #[cfg(unix)]
    {
        let shim = r["shim"].as_str().unwrap();
        let text = std::fs::read_to_string(shim).unwrap();
        assert!(text.starts_with("#!/bin/sh") && text.contains("droplet"));
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(std::fs::metadata(shim).unwrap().permissions().mode() & 0o111, 0, "executable");
    }
    let r = s.execute("file.automate.runDroplet", json!({"droplet": dp, "input": [dir.clone()]})).unwrap();
    assert_eq!(r["files"].as_array().unwrap().len(), inputs.len(), "{r}");
    let out = photocraft_codecs::decode(&std::fs::read(format!("{dir}/out/img0.png")).unwrap()).unwrap();
    assert_eq!(out.dimensions(), (20, 40));
    assert!(s.execute("file.automate.createDroplet", json!({"path": path, "steps": [["bogus", {}]]})).is_err());
}

#[test]
fn statistics_makes_a_stack_mode_smart_object() {
    let dir = tmp("stats");
    let files = images(&dir, 3, 16, 12);
    let mut s = Session::new();
    let r = s.execute("file.scripts.statistics", json!({"mode": "mean", "input": files})).unwrap();
    assert_eq!(r["images"], 3);
    let d = &s.active().unwrap().doc;
    assert_eq!(d.layers.len(), 1);
    let LayerContent::Smart(so) = &d.layers[0].content else { panic!("not a smart object") };
    assert_eq!(so.stack_mode, Some(photocraft_doc::StackMode::Mean));
    // Mean of 20, 60, 100 = 60.
    let px = photocraft_compose::flatten(d).px[0];
    assert!((px[0] * 255.0 - 60.0).abs() < 1.5, "{px:?}");
    assert!(s.execute("file.scripts.statistics", json!({"mode": "average", "input": dir})).is_err());
}

#[test]
fn contact_sheet_places_thumbnails_with_captions() {
    let dir = tmp("contact");
    images(&dir, 5, 60, 30);
    for depth in [8, 16] {
        let mut s = Session::new();
        let r = s
            .execute("file.automate.contactSheetII", json!({"input": dir, "units": "pixels", "width": 400, "height": 300, "resolution": 72, "columns": 2, "rows": 2, "depth": depth, "fontSize": 10, "rotateForBestFit": true}))
            .unwrap();
        assert_eq!(r["pages"], 2, "5 images on 2×2 sheets");
        assert_eq!(r["images"], 5);
        let first = r["documents"][0].as_u64().unwrap() as usize;
        let d = &s.documents()[first].doc;
        assert_eq!((d.size.width, d.size.height), (400, 300));
        let names: Vec<&str> = d.layers.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"img0") && names.contains(&"img3"), "{names:?}");
        let texts = d.layers.iter().filter(|l| matches!(l.content, LayerContent::Text(_))).count();
        assert_eq!(texts, 4, "a caption per image");
        let thumb = d.layers.iter().find(|l| l.name == "img0" && matches!(l.content, LayerContent::Raster(_))).unwrap();
        let b = thumb.surface().unwrap().content_bounds();
        assert!(b.width() <= 200 && b.height() < 150, "fits its cell: {b:?}");
        // Flattened variant.
        let r = s
            .execute(
                "file.automate.contactSheetII",
                json!({"input": dir, "units": "pixels", "width": 200, "height": 200, "columns": 3, "rows": 2, "caption": false, "flatten": true}),
            )
            .unwrap();
        let d = &s.documents()[r["documents"][0].as_u64().unwrap() as usize].doc;
        assert_eq!(d.layers.len(), 1);
    }
}

/// A droplet keeps the action's recorded saves, which mark the result as in Batch, so one made from
/// an action ending in Save As and Close writes its files instead of failing on every one.
#[test]
fn droplets_keep_the_recorded_save_that_marks_the_result() {
    let dir = tmp("droplet-save-close");
    let inputs = images(&dir, 1, 4, 2);
    let mut s = Session::new();
    let steps = json!([["image.adjustments.invert", {}], ["file.saveAs", {}], ["file.close", {}]]);
    let p = json!({"path": format!("{dir}/Invert"), "steps": steps, "format": "png", "output": format!("{dir}/out"), "shim": false});
    let dp = s.execute("file.automate.createDroplet", p).unwrap()["path"].as_str().unwrap().to_string();
    let r = s.execute("file.automate.runDroplet", json!({"droplet": dp, "input": [dir.clone()]})).unwrap();
    assert_eq!(r["errors"], json!([]), "{r}");
    assert_eq!(r["files"].as_array().unwrap().len(), inputs.len(), "{r}");
    let out = photocraft_io::import("img0.png", &std::fs::read(format!("{dir}/out/img0.png")).unwrap()).unwrap().document;
    let px = photocraft_compose::render(&out, photocraft_geom::Rect::new(1, 1, 2, 2)).px[0];
    assert!((px[0] - 235.0 / 255.0).abs() < 1e-3, "img0's gray 20 inverted: {px:?}");
}

fn script_calling(dir: &str, name: &str, callees: &[String]) -> String {
    let path = format!("{dir}/{name}.txt");
    let body: String = callees.iter().map(|c| format!("file.scripts.browse {}\n", json!({"path": c}))).collect();
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn scripts_calling_scripts_share_one_step_budget() {
    let dir = tmp("fanout");
    let mut below = script_calling(&dir, "leaf", &[]);
    std::fs::write(&below, "actions.list {}\n").unwrap();
    for level in 0..8 {
        below = script_calling(&dir, &format!("level{level}"), &vec![below; 6]);
    }
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 8, "height": 8})).unwrap();
    let r = s.execute("file.scripts.browse", json!({"path": below})).unwrap();
    assert_eq!(r["ok"], false, "{r}");
    assert!(r.to_string().contains("steps in all"), "{r}");
    let next = s.execute("file.scripts.browse", json!({"script": "actions.list {}"})).unwrap();
    assert_eq!(next["ok"], true);
}

#[test]
fn scripts_nested_deeper_than_the_limit_fail() {
    let dir = tmp("deep");
    let mut below = script_calling(&dir, "leaf", &[]);
    for level in 0..20 {
        below = script_calling(&dir, &format!("level{level}"), &[below]);
    }
    let mut s = Session::new();
    let r = s.execute("file.scripts.browse", json!({"path": below})).unwrap();
    assert_eq!(r["ok"], false, "{r}");
    assert!(r.to_string().contains("nested more than"), "{r}");
    assert_eq!(s.actions.script_depth, 0);
}

#[test]
fn nested_script_failure_stops_the_caller_before_later_edits() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 8, "height": 8})).unwrap();
    let r = s
        .execute(
            "file.scripts.browse",
            json!({"steps": [
                ["file.scripts.browse", {"steps": [["layer.select", {"layer": "missing"}]]}],
                ["layer.new.layer", {}]
            ]}),
        )
        .unwrap();
    assert_eq!(r["ok"], false);
    assert_eq!(r["results"].as_array().unwrap().len(), 1);
    assert_eq!(s.active().unwrap().doc.layers.len(), 1);
}

#[test]
fn recursive_batches_carry_the_depth_and_report_failure() {
    let dir = tmp("recursive-batch");
    let inputs = images(&dir, 1, 4, 2);
    let script = format!("{dir}/again.txt");
    let batch = json!({"input": inputs, "output": format!("{dir}/out"), "format": "png", "steps": [["file.scripts.browse", {"path": script}]]});
    std::fs::write(&script, format!("file.automate.batch {batch}\n")).unwrap();
    let mut s = Session::new();
    let r = s.execute("file.automate.batch", batch).unwrap();
    assert_eq!(r["files"], json!([]));
    assert!(r["errors"][0]["error"].as_str().unwrap().contains("nested more than"), "{r}");
    assert_eq!(s.actions.script_depth, 0);
    assert!(!std::path::Path::new(&format!("{dir}/out/img0.png")).exists());
}

#[test]
fn batch_steps_share_the_script_budget_across_inputs() {
    let dir = tmp("carried-budget");
    let inputs = images(&dir, 2, 4, 2);
    let mut s = Session::new();
    s.actions.script_depth = 1;
    s.actions.script_steps = MAX_SCRIPT_STEPS - 6;
    let r = s
        .execute(
            "file.automate.batch",
            json!({
                "input": inputs, "output": format!("{dir}/out"), "format": "png",
                "steps": [["file.scripts.browse", {"steps": [["actions.list", {}], ["actions.list", {}]]}]]
            }),
        )
        .unwrap();
    assert_eq!(r["files"].as_array().unwrap().len(), 1, "the first nested script spends the remaining budget: {r}");
    assert_eq!(r["errors"].as_array().unwrap().len(), 1);
    assert_eq!(s.actions.script_steps, MAX_SCRIPT_STEPS);
    assert_eq!(s.actions.script_depth, 1);
}

#[test]
fn played_actions_share_their_calling_scripts_step_budget() {
    let mut s = Session::new();
    s.actions.script_depth = 1;
    s.actions.script_steps = MAX_SCRIPT_STEPS - 3;
    s.actions
        .list
        .push(crate::actions_cmds::Action { name: "queries".into(), steps: vec![("actions.list".into(), json!({})), ("actions.list".into(), json!({}))] });
    let r = s.execute("file.scripts.browse", json!({"steps": [["actions.play", {"action": "queries"}], ["actions.play", {"action": "queries"}]]})).unwrap();
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["results"][0]["result"]["ran"], 2);
    assert!(r["results"][1]["error"].as_str().unwrap().contains("steps in all"), "{r}");
    assert_eq!(s.actions.script_steps, MAX_SCRIPT_STEPS);
    assert_eq!(s.actions.script_depth, 1);
    assert_eq!(s.actions.playing, 0);
}

#[test]
fn batch_reserves_its_input_work_before_reading_files() {
    let mut s = Session::new();
    s.actions.script_depth = 1;
    s.actions.script_steps = MAX_SCRIPT_STEPS - 1;
    let error = s
        .execute("file.automate.batch", json!({"input": ["missing-first.png", "missing-second.png"], "output": "unused", "steps": []}))
        .unwrap_err()
        .to_string();
    assert!(error.contains("step limit"), "the budget fails before either path is opened: {error}");
    assert_eq!(s.actions.script_steps, MAX_SCRIPT_STEPS - 1);
    assert_eq!(s.actions.script_depth, 1);
}
