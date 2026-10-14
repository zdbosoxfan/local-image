//! Library automation tests: `lightcraft-mcp`'s `tests.rs`, `headless.rs` and `e2e.rs` re-targeted
//! at [`LibraryHeadless`] and the `library_*` tools (J5), plus the workspace policy they now share
//! with the editor's automation.

use std::path::{Path, PathBuf};

use lightcraft_engine::Session;
use serde_json::{Value, json};

use super::tools::{LibraryReply, call_tool, tool_definitions};
use super::{LibraryHeadless, expand_paths, library_command_allowed};
use crate::AuthorizedWorkspace;

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

fn demo() -> LibraryHeadless {
    LibraryHeadless::from_session(Session::with_demo().with_fs())
}

fn fresh_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("j05-library-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn png(rgba: [u8; 4], side: usize) -> Vec<u8> {
    let img = lightcraft_raster::Rgba8 { width: side, height: side, data: vec![rgba; side * side] };
    lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &lightcraft_codecs::EncodeMeta::default()).unwrap()
}

fn ok(r: Result<LibraryReply, String>) -> LibraryReply {
    r.unwrap_or_else(|e| panic!("tool failed: {e}"))
}

fn json_of(r: Result<LibraryReply, String>) -> Value {
    ok(r).json().clone()
}

/// Mean sRGB value of an encoded image (and its size).
fn mean_of(reply: &LibraryReply) -> (f64, u32, u32) {
    let LibraryReply::Image { bytes, mime, .. } = reply else { panic!("no image in {reply:?}") };
    assert_eq!(*mime, "image/png");
    assert_eq!(&bytes[1..4], b"PNG");
    let d = lightcraft_codecs::decode(bytes, Default::default()).unwrap();
    let rgba = d.to_srgb8();
    let sum: u64 = rgba.data.iter().map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64).sum();
    (sum as f64 / (3 * rgba.data.len()) as f64, d.width, d.height)
}

// ---------------------------------------------------------------- re-targeted lightcraft-mcp tests

#[test]
fn j05_tool_definitions_are_valid_mcp_tools() {
    let defs = tool_definitions();
    let names: Vec<&str> = defs.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for n in [
        "command_list",
        "command_run",
        "import",
        "query_photos",
        "select_photos",
        "list_controls",
        "get_develop",
        "set_develop",
        "apply_preset",
        "crop",
        "render_photo",
        "export",
    ] {
        assert!(names.contains(&format!("library_{n}").as_str()), "{n}");
    }
    for t in &defs {
        let n = t["name"].as_str().unwrap();
        assert!(n.len() <= 64 && n.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'), "{n}");
        assert_eq!(t["inputSchema"]["type"], "object");
        assert!(t["description"].as_str().is_some_and(|d| !d.is_empty()));
    }
    // Every command stays reachable: command_list lists the registry plus app.export.
    let mut lib = demo();
    let list = json_of(call_tool(&mut lib, "library_command_list", &json!({})));
    let ids: Vec<&str> = list.as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
    assert_eq!(ids.len(), lib.session.commands().len() + 1);
    assert!(ids.contains(&"app.export") && ids.contains(&"photo.rate"));
    let filtered = json_of(call_tool(&mut lib, "library_command_list", &json!({"filter": "RATE"})));
    assert!(filtered.as_array().unwrap().iter().all(|c| format!("{} {}", c["id"], c["label"]).to_lowercase().contains("rate")));
}

#[test]
fn j05_errors_are_tool_errors() {
    let mut lib = demo();
    assert!(call_tool(&mut lib, "library_nope", &json!({})).unwrap_err().contains("unknown Library tool"));
    let e = call_tool(&mut lib, "library_command_run", &json!({"command": "no.such"})).unwrap_err();
    assert!(e.contains("unknown command"), "{e}");
    assert!(call_tool(&mut lib, "library_command_run", &json!({})).is_err());
    assert!(call_tool(&mut lib, "library_import", &json!([1])).is_err(), "arguments must be an object");
    // A UI method needs a window.
    assert!(lib.call("ui.screenshot", json!({})).unwrap_err().contains("headless"));
}

#[test]
fn j05_command_run_rates_and_undoes() {
    let mut lib = demo();
    let first = lib.session.visible_cloned()[0];
    ok(call_tool(&mut lib, "library_command_run", &json!({"command": "library.select", "params": {"ids": [first.0]}})));
    ok(call_tool(&mut lib, "library_command_run", &json!({"command": "photo.rate", "params": {"rating": 4}})));
    assert_eq!(lib.session.catalog.photo(first).unwrap().rating, 4);
    ok(call_tool(&mut lib, "library_command_run", &json!({"command": "edit.undo"})));
    assert_eq!(lib.session.catalog.photo(first).unwrap().rating, 0);
}

#[test]
fn j05_set_develop_values_and_settings() {
    let mut lib = demo();
    let first = lib.session.visible_cloned()[0];
    let r = json_of(call_tool(&mut lib, "library_set_develop", &json!({"id": first.0, "values": {"light.exposure": 1.25}})));
    assert_eq!(r["controls"][0]["value"], 1.25);
    assert_eq!(lib.session.develop_of(first).unwrap().light.exposure, 1.25);
    ok(call_tool(&mut lib, "library_set_develop", &json!({"settings": {"light": {"contrast": 30.0}}})));
    let d = lib.session.develop_of(first).unwrap();
    assert_eq!((d.light.exposure, d.light.contrast), (1.25, 30.0));
    assert!(call_tool(&mut lib, "library_set_develop", &json!({})).is_err());
    assert!(call_tool(&mut lib, "library_set_develop", &json!({"values": {"no.such": 1}})).is_err());
}

#[test]
fn j05_crop_needs_a_rect_angle_or_reset() {
    let mut lib = demo();
    let first = lib.session.visible_cloned()[0];
    // A guessed parameter must not succeed silently.
    assert!(call_tool(&mut lib, "library_crop", &json!({"id": first.0, "aspect": "1:1"})).is_err());
    assert!(call_tool(&mut lib, "library_crop", &json!({"id": first.0, "reset": false})).is_err());
    ok(call_tool(&mut lib, "library_crop", &json!({"id": first.0, "rect": [0.1, 0.0, 0.9, 1.0]})));
    ok(call_tool(&mut lib, "library_crop", &json!({"id": first.0, "reset": true})));
}

/// M0.9 acceptance (was `lc-mcp/tests/e2e.rs`): set exposure through the tools and look at the
/// rendered result.
#[test]
fn j05_set_exposure_and_render() {
    let mut lib = demo();
    let q = json_of(call_tool(&mut lib, "library_query_photos", &json!({"limit": 1})));
    let id = q["photos"][0]["id"].as_u64().expect("a photo");
    let before = ok(call_tool(&mut lib, "library_render_photo", &json!({"id": id, "size": 256})));
    let r = json_of(call_tool(&mut lib, "library_set_develop", &json!({"id": id, "values": {"light.exposure": 1.5}})));
    assert_eq!(r["controls"][0]["value"], 1.5);
    assert_eq!(json_of(call_tool(&mut lib, "library_get_develop", &json!({"id": id})))["light"]["exposure"], 1.5);
    let brighter = ok(call_tool(&mut lib, "library_render_photo", &json!({"id": id, "size": 256})));
    ok(call_tool(&mut lib, "library_command_run", &json!({"command": "develop.set", "params": {"control": "light.exposure", "value": -1.5}})));
    let darker = ok(call_tool(&mut lib, "library_render_photo", &json!({"size": 256})));
    let ((b, w, h), (br, w2, h2), (d, _, _)) = (mean_of(&before), mean_of(&brighter), mean_of(&darker));
    assert_eq!((w, h), (w2, h2));
    assert!(w.max(h) <= 256 && w.max(h) >= 200, "{w}x{h}");
    assert!(br > b + 15.0, "exposure +1.5 should brighten: {b} → {br}");
    assert!(d < b - 15.0, "exposure -1.5 should darken: {b} → {d}");
    // JPEG on request.
    let LibraryReply::Image { mime, .. } = ok(call_tool(&mut lib, "library_render_photo", &json!({"size": 64, "format": "jpeg"}))) else { panic!() };
    assert_eq!(mime, "image/jpeg");
}

/// The `import` helper passes the copy options through: a folder template files the copy.
#[test]
fn j05_import_tool_copies_with_a_folder_template() {
    let base = fresh_dir("import-tpl");
    let (src, dest) = (base.join("card"), base.join("out"));
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("a.png"), png([20, 3, 9, 255], 8)).unwrap();
    let mut lib = demo();
    lib.session.clock = Box::new(|| "2026-01-14T05:58:48".to_string());
    let args = json!({"paths": [src.to_string_lossy()], "mode": "copy", "destination": dest.to_string_lossy(), "organize": "{date:%Y}/{date:%Y%m%d}"});
    let r = json_of(call_tool(&mut lib, "library_import", &args));
    assert_eq!(r["files"], 1);
    assert!(dest.join("2026").join("20260114").join("a.png").is_file());
    let _ = std::fs::remove_dir_all(&base);
}

/// The `import` helper moves: renamed into the folder template, the source removed.
#[test]
fn j05_import_tool_moves_with_a_folder_template() {
    let base = fresh_dir("import-move");
    let (src, dest) = (base.join("card"), base.join("out"));
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("sample.png"), png([21, 3, 9, 255], 8)).unwrap();
    let mut lib = demo();
    lib.session.clock = Box::new(|| "2026-01-14T05:58:48".to_string());
    let args = json!({"paths": [src.to_string_lossy()], "mode": "move", "destination": dest.to_string_lossy(),
        "organize": "{date:%Y}/{date:%Y%m%d}", "rename": "{date:%Y%m%d}_{seq:3}"});
    ok(call_tool(&mut lib, "library_import", &args));
    assert!(dest.join("2026").join("20260114").join("20260114_001.png").is_file());
    assert!(!src.join("sample.png").exists(), "moved");
    let _ = std::fs::remove_dir_all(&base);
}

/// Issue #93: `render_photo`, `ui.render` and `app.export` with an exact `path` must not replace
/// a photo's original.
#[test]
fn j05_path_writes_never_replace_an_original() {
    let base = fresh_dir("guard");
    let bytes = png([90, 30, 9, 255], 8);
    let orig = base.join("a.png");
    std::fs::write(&orig, &bytes).unwrap();
    let mut lib = demo();
    ok(call_tool(&mut lib, "library_import", &json!({"paths": [orig.to_string_lossy()]})));
    let id = lib.session.catalog.photos().find(|p| p.file_name == "a.png").unwrap().id.0;
    let path = orig.to_string_lossy().to_string();
    assert!(call_tool(&mut lib, "library_render_photo", &json!({"id": id, "path": path})).is_err());
    assert!(lib.call("ui.render", json!({"id": id, "path": path})).unwrap_err().contains("original"));
    let e = lib.call("app.export", json!({"ids": [id], "path": path})).unwrap_err();
    assert!(e.contains("never writes over an original"), "{e}");
    assert_eq!(std::fs::read(&orig).unwrap(), bytes, "the original is untouched");
    // another path still works
    ok(call_tool(&mut lib, "library_render_photo", &json!({"id": id, "path": base.join("render.png").to_string_lossy()})));
    assert!(base.join("render.png").is_file());
    let _ = std::fs::remove_dir_all(&base);
}

/// `app.export` without `ids` exports the selection (as the desktop app does), else the active photo.
#[test]
fn j05_export_follows_the_selection() {
    let dir = fresh_dir("export-selection");
    let mut lib = demo();
    let ids: Vec<u64> = lib.session.visible_cloned().iter().take(3).map(|p| p.0).collect();
    lib.call("engine.execute", json!({"command": "library.select", "params": {"ids": ids}})).unwrap();
    let r = lib.call("engine.execute", json!({"command": "app.export", "params": {"dir": dir.to_string_lossy(), "longEdge": 64, "format": "png"}})).unwrap();
    assert_eq!(r["files"].as_array().map(Vec::len), Some(3), "{r}");
    lib.call("engine.execute", json!({"command": "library.select", "params": {"ids": [ids[0]]}})).unwrap();
    let r = lib.call("engine.execute", json!({"command": "app.export", "params": {"dir": dir.to_string_lossy(), "longEdge": 64, "format": "png"}})).unwrap();
    assert_eq!(r["files"].as_array().map(Vec::len), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The export file-name template takes the Rename Photos tokens.
#[test]
fn j05_export_naming_tokens() {
    let dir = fresh_dir("export-naming");
    let mut lib = demo();
    let ids: Vec<u64> = lib.session.visible_cloned().iter().take(2).map(|p| p.0).collect();
    for (id, title) in ids.iter().zip(["Harbour", "Hills"]) {
        lib.call("engine.execute", json!({"command": "photo.setMeta", "params": {"ids": [id], "title": title}})).unwrap();
        lib.call("engine.execute", json!({"command": "photo.rate", "params": {"ids": [id], "rating": 4}})).unwrap();
    }
    let params = json!({"dir": dir.to_string_lossy(), "ids": ids, "longEdge": 32, "format": "png", "naming": "{title}_{rating}star_{seq:2}", "startNumber": 5});
    let r = lib.call("engine.execute", json!({"command": "app.export", "params": params})).unwrap();
    let mut names: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    names.sort();
    assert_eq!(names, ["Harbour_4star_05.png", "Hills_4star_06.png"], "{r}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn j05_folder_expansion_uses_shared_registry() -> TestResult {
    let dir = fresh_dir("extension-registry");
    for name in ["a.HEIC", "b.CRW", "c.PSB", "d.QOI", "unsupported.avif", "notes.txt"] {
        std::fs::write(dir.join(name), [])?;
    }
    let files = expand_paths(&[dir.to_string_lossy().into_owned()]);
    for (name, included) in [
        ("a.HEIC", lightcraft_codecs::is_readable_extension("heic", true)),
        ("b.CRW", true),
        ("c.PSB", true),
        ("d.QOI", true),
        ("unsupported.avif", false),
        ("notes.txt", false),
    ] {
        assert_eq!(files.iter().any(|f| f.ends_with(name)), included, "{name}");
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

// ---------------------------------------------------------------- the shared automation policy

/// A workspace with `shots/` (two photos and a note) and an `out/` folder.
fn workspace(tag: &str) -> (PathBuf, LibraryHeadless) {
    let root = fresh_dir(tag);
    std::fs::create_dir_all(root.join("shots/day2")).unwrap();
    std::fs::create_dir_all(root.join("out")).unwrap();
    std::fs::write(root.join("shots/a.png"), png([200, 60, 30, 255], 48)).unwrap();
    std::fs::write(root.join("shots/day2/b.png"), png([30, 60, 200, 255], 32)).unwrap();
    std::fs::write(root.join("shots/notes.txt"), b"not a photo").unwrap();
    let ws = AuthorizedWorkspace::new(Some(&root), Some(&root)).unwrap();
    (root, LibraryHeadless::with_workspace(ws))
}

fn no_host_paths(v: &Value, root: &Path) {
    let root = std::fs::canonicalize(root).unwrap().to_string_lossy().into_owned();
    assert!(!v.to_string().contains(&root), "a result leaked the host root: {v}");
}

#[test]
fn j05_workspace_import_develop_render_export_through_capabilities() {
    let (root, mut lib) = workspace("ws-roundtrip");
    let r = json_of(call_tool(&mut lib, "library_import", &json!({"paths": ["shots"]})));
    assert_eq!(r["files"], 2, "folders keep photos only: {r}");
    no_host_paths(&r, &root);
    let photos = json_of(call_tool(&mut lib, "library_query_photos", &json!({})));
    no_host_paths(&photos, &root);
    let a = photos["photos"].as_array().unwrap().iter().find(|p| p["fileName"] == "a.png" || p["name"] == "a.png").cloned();
    let id = a.and_then(|p| p["id"].as_u64()).unwrap_or_else(|| photos["photos"][0]["id"].as_u64().unwrap());
    ok(call_tool(&mut lib, "library_set_develop", &json!({"id": id, "values": {"light.exposure": 0.5}})));
    let shot = ok(call_tool(&mut lib, "library_render_photo", &json!({"id": id, "size": 32, "path": "out/look.png"})));
    assert_eq!(shot.json()["path"], "out/look.png");
    assert!(root.join("out/look.png").is_file());
    let r = json_of(call_tool(&mut lib, "library_export", &json!({"id": id, "dir": "out", "longEdge": 24, "format": "jpeg"})));
    no_host_paths(&r, &root);
    let written = r["path"].as_str().unwrap_or_default();
    assert!(written.starts_with("out/") && root.join(written).is_file(), "{r}");
    let r = json_of(call_tool(&mut lib, "library_export", &json!({"id": id, "path": "out/exact.png", "longEdge": 0})));
    assert_eq!(r["path"], "out/exact.png");
    assert!(root.join("out/exact.png").is_file());
}

#[test]
fn j05_workspace_never_replaces_an_original_and_rejects_escapes() {
    let (root, mut lib) = workspace("ws-guard");
    let original = std::fs::read(root.join("shots/a.png")).unwrap();
    ok(call_tool(&mut lib, "library_import", &json!({"paths": "shots/a.png"})));
    assert!(call_tool(&mut lib, "library_render_photo", &json!({"path": "shots/a.png"})).is_err());
    let e = call_tool(&mut lib, "library_export", &json!({"path": "shots/a.png"})).unwrap_err();
    assert!(e.contains("original"), "{e}");
    assert_eq!(std::fs::read(root.join("shots/a.png")).unwrap(), original);
    // Paths stay below the roots.
    let outside = root.parent().unwrap().join(format!("j05-outside-{}.png", std::process::id()));
    std::fs::write(&outside, png([1, 2, 3, 255], 8)).unwrap();
    for p in [outside.to_string_lossy().into_owned(), "../x.png".into(), "shots/../shots/a.png".into(), "C:/x.png".into()] {
        assert!(call_tool(&mut lib, "library_import", &json!({"paths": [p]})).is_err(), "{p}");
        assert!(call_tool(&mut lib, "library_export", &json!({"path": p})).is_err(), "{p}");
    }
    let _ = std::fs::remove_file(outside);
    // Copy / move imports touch originals on disk: refused.
    let e = call_tool(&mut lib, "library_import", &json!({"paths": ["shots"], "mode": "move", "destination": "out"})).unwrap_err();
    assert!(e.contains("mode `move`"), "{e}");
    assert!(root.join("shots/a.png").is_file());
}

#[cfg(unix)]
#[test]
fn j05_workspace_reads_never_follow_a_link_out_of_the_root() {
    let (root, mut lib) = workspace("ws-symlink");
    let outside = fresh_dir("ws-symlink-outside");
    std::fs::write(outside.join("secret.png"), png([5, 5, 5, 255], 8)).unwrap();
    std::os::unix::fs::symlink(outside.join("secret.png"), root.join("shots/link.png")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("shots/linked-dir")).unwrap();
    // Folder scans skip links; naming the link fails.
    let r = json_of(call_tool(&mut lib, "library_import", &json!({"paths": ["shots"]})));
    assert_eq!(r["files"], 2, "{r}");
    assert!(call_tool(&mut lib, "library_import", &json!({"paths": ["shots/link.png"]})).is_err());
    assert!(call_tool(&mut lib, "library_import", &json!({"paths": ["shots/linked-dir/secret.png"]})).is_err());
    let _ = std::fs::remove_dir_all(outside);
}

#[test]
fn j05_untrusted_library_commands_are_deny_by_default() {
    let (_root, mut lib) = workspace("ws-commands");
    for id in [
        "library.browse",
        "library.autoImport",
        "photo.saveMetadataToFile",
        "library.toggleAutoWriteXmp",
        "preset.import",
        "preset.export",
        "profile.import",
        "photo.convertToDng",
        "photo.duplicate",
        "photo.rename",
        "folder.move",
        "merge.hdr",
        "smartSort.export",
        "segment.model.download",
        "photo.saveOverOriginal",
    ] {
        assert!(!library_command_allowed(id), "{id}");
        let e = call_tool(&mut lib, "library_command_run", &json!({"command": id, "params": {"path": "x"}})).unwrap_err();
        assert!(e.contains("disabled"), "{id}: {e}");
    }
    let list = json_of(call_tool(&mut lib, "library_command_list", &json!({})));
    let ids: Vec<&str> = list.as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
    assert!(!ids.contains(&"library.browse") && ids.contains(&"photo.rate") && ids.contains(&"library.import") && ids.contains(&"app.export"));
    // Every listed command id exists or is a mapped method; nothing denied is listed.
    for id in &ids {
        assert!(library_command_allowed(id) || matches!(*id, "library.import" | "export.checkTarget" | "app.export"), "{id}");
    }
    // `library.import` through command_run takes the same workspace route as the helper.
    let r = json_of(call_tool(&mut lib, "library_command_run", &json!({"command": "library.import", "params": {"paths": ["shots/a.png"]}})));
    assert_eq!(r["files"], 1, "{r}");
}

#[test]
fn j05_no_roots_no_files() {
    let mut lib = LibraryHeadless::new();
    assert!(call_tool(&mut lib, "library_import", &json!({"paths": ["a.png"]})).unwrap_err().contains("not granted"));
    assert!(call_tool(&mut lib, "library_export", &json!({"path": "a.png"})).is_err());
    assert!(lib.open_library(Path::new("anywhere")).is_err());
    // Read-only commands still answer.
    assert!(call_tool(&mut lib, "library_query_photos", &json!({})).is_ok());
}
