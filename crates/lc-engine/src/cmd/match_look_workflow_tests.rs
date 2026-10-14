use super::*;
use lightcraft_develop::{LayerTools, Mask, MaskComponent, MaskShape};
use lightcraft_raster::Rgba8;

fn setup() -> Session {
    let mut s = Session::with_demo();
    let id = s.active().unwrap();
    s.execute("develop.set", &json!({"control":"light.exposure","value":0.5})).unwrap();
    s.execute("develop.matchLookReferences", &json!({"ids":[id]})).unwrap();
    s.execute("develop.analyzeLook", &json!({})).unwrap();
    s
}
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("match-look-workflow-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn image(&self, name: &str, warm: u8) -> String {
        let image = Rgba8::from_fn(96, 64, |x, y| [40 + x as u8 + warm, 40 + y as u8, 70 + x as u8 / 3, 255]);
        let path = self.0.join(name);
        std::fs::write(&path, lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&image), &Default::default()).unwrap()).unwrap();
        path.to_string_lossy().into_owned()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn disk_reference_recipe_is_portable_offline_and_restores_choices() {
    let files = Scratch::new();
    let path = files.image("finished.png", 15);
    let mut s = Session::with_demo().with_fs();
    let count = s.catalog.photos().count();
    s.execute("develop.matchLookReferences", &json!({"paths":[path],"replace":true})).unwrap();
    assert!(s.match_look.references.is_empty());
    assert_eq!(s.catalog.photos().count(), count);
    s.execute("develop.analyzeLook", &json!({})).unwrap();
    let id = s.match_look.external_references[0].id;
    s.execute("develop.matchLookReferences", &json!({"weightExternalId":id})).unwrap();
    assert!(current_ready(&s));
    assert_eq!(selected_looks(&s).unwrap()[0].importance, 2.0);
    s.match_look.options.strength = 63.0;
    s.match_look.options.new_layer = true;
    s.match_look.options.adjustments.red_curve = false;
    s.match_look.options.adjustments.highlights = false;
    let choices = s.match_look.options;
    let saved_id = s.execute("develop.savedLook", &json!({"name":"Portable finished image"})).unwrap()["id"].as_str().unwrap().to_owned();
    let saved = s.saved_looks.file.looks[0].clone();
    assert_eq!(saved.previews.len(), 1);
    assert!(saved.previews[0].image().is_some());
    let json = serde_json::to_string(&s.saved_looks.file).unwrap();
    assert!(!json.contains(&path));
    std::fs::remove_file(&path).unwrap();
    let mut reloaded = Session::with_demo();
    reloaded.execute("develop.savedLook", &json!({"action":"import","file":s.saved_looks.file})).unwrap();
    let imported_id = reloaded.saved_looks.file.looks[0].id.clone();
    assert_ne!(imported_id, saved_id);
    reloaded.execute("develop.savedLook", &json!({"action":"select","id":imported_id})).unwrap();
    assert_eq!(reloaded.match_look.options, choices);
    assert_eq!(selected_looks(&reloaded).unwrap(), saved.looks);
    assert_eq!(selected_previews(&reloaded), saved.previews);
    reloaded.match_look.options.strength = 48.0;
    reloaded.execute("develop.savedLook", &json!({"name":"Softer portable look"})).unwrap();
    assert_eq!(reloaded.saved_looks.file.looks[1].options.unwrap().strength, 48.0);
    assert_eq!(reloaded.saved_looks.file.looks[1].previews, saved.previews);
    let options = reloaded.match_look.options;
    let result = reloaded.execute("develop.matchLook", &serde_json::to_value(options).unwrap()).unwrap();
    assert!(result["failed"].as_array().unwrap().is_empty());
}

#[test]
fn disk_reference_analysis_rejects_stale_source_and_total_reference_limit() {
    let files = Scratch::new();
    let path = files.image("finished.png", 4);
    let mut s = Session::with_demo().with_fs();
    let refs = s.catalog.photos().take(20).map(|p| p.id).collect::<Vec<_>>();
    s.execute("develop.matchLookReferences", &json!({"ids":refs,"replace":true})).unwrap();
    assert!(s.execute("develop.matchLookReferences", &json!({"paths":[path]})).is_err());
    assert_eq!(s.match_look.references, refs);
    assert!(s.match_look.external_references.is_empty());
    s.execute("develop.matchLookReferences", &json!({"paths":[path],"replace":true})).unwrap();
    let work = prepare_analysis(&mut s).unwrap();
    let analyzed = work.run(&Control::default()).unwrap();
    std::fs::write(&path, b"changed").unwrap();
    assert_eq!(apply_analysis(&mut s, analyzed, None).unwrap()["discarded"], true);
    assert!(!current_ready(&s));
    assert!(prepare_analysis(&mut s).unwrap().run(&Control::default()).is_err());
}

#[test]
fn malformed_preview_and_future_analyzer_are_rejected_without_overwriting_presets() {
    let mut s = setup();
    s.execute("develop.savedLook", &json!({"name":"Valid"})).unwrap();
    let valid = s.saved_looks.file.clone();
    for bad_preview in ["not base64".to_string(), "A".repeat(100_000)] {
        let mut file = valid.clone();
        file.looks[0].previews[0].jpeg = bad_preview;
        assert!(s.execute("develop.savedLook", &json!({"action":"import","file":file})).is_err());
        assert_eq!(s.saved_looks.file.looks, valid.looks);
    }
    let mut file = valid.clone();
    file.looks[0].analyzer_version = Some(LOOK_ANALYZER_VERSION + 1);
    assert!(parse_import(&serde_json::to_vec(&file).unwrap()).is_err());
    let mut legacy = serde_json::to_value(valid).unwrap();
    legacy["version"] = json!(2);
    for look in legacy["looks"].as_array_mut().unwrap() {
        for key in ["options", "previews", "analyzerVersion"] {
            look.as_object_mut().unwrap().remove(key);
        }
    }
    assert!(parse_import(&serde_json::to_vec(&legacy).unwrap()).is_ok());
}

#[test]
fn reapplication_replaces_owned_layer_and_preserves_later_manual_edits_and_undo() {
    let mut s = setup();
    let target = s.catalog.photos().find(|p| p.id != s.active().unwrap()).unwrap().id;
    s.execute("library.select", &json!({"ids":[target]})).unwrap();
    let opts = Options { new_layer: true, strength: 65.0, ..Default::default() };
    s.execute("develop.matchLook", &serde_json::to_value(opts).unwrap()).unwrap();
    let first = s.develop_of(target).unwrap();
    let managed = s.match_look.applied[&target].managed_layer.unwrap();
    let mut manual = (*first).clone();
    let layer = manual.masks.iter_mut().find(|m| m.id == managed).unwrap();
    layer.opacity = 0.8;
    layer.name = "My matching layer".into();
    layer.tools.effects = Some(lightcraft_develop::Effects { clarity: 7.0, ..Default::default() });
    let later = Mask {
        id: managed + 10,
        name: "Later user layer".into(),
        components: vec![MaskComponent { shape: MaskShape::Full, op: Default::default(), invert: false, name: None }],
        tools: LayerTools { light: Some(lightcraft_develop::Light { exposure: 0.1, ..Default::default() }), ..Default::default() },
        ..Default::default()
    };
    manual.masks.push(later.clone());
    manual.vignette.amount = -8.0;
    s.set_develop(target, manual.clone(), "Manual work").unwrap();
    let undo = s.undo.len();
    s.execute("develop.matchLook", &serde_json::to_value(Options { strength: 40.0, ..opts }).unwrap()).unwrap();
    let second = s.develop_of(target).unwrap();
    assert_eq!(second.masks.len(), manual.masks.len());
    assert_eq!(second.masks.iter().map(|m| m.id).collect::<Vec<_>>(), manual.masks.iter().map(|m| m.id).collect::<Vec<_>>());
    assert_eq!(second.masks.last().unwrap(), &later);
    assert_eq!(second.vignette, manual.vignette);
    let layer = second.masks.iter().find(|m| m.id == managed).unwrap();
    assert_eq!(layer.opacity, 0.8);
    assert_eq!(layer.name, "My matching layer");
    assert_eq!(layer.tools.effects, manual.masks[0].tools.effects);
    assert_eq!(s.undo.len(), undo + 1);
    s.execute("develop.matchLookAmount", &json!({"strength":85.0})).unwrap();
    let amount = s.develop_of(target).unwrap();
    assert_eq!(amount.masks.last().unwrap(), &later);
    assert_eq!(amount.masks.iter().find(|m| m.id == managed).unwrap().tools.effects, layer.tools.effects);
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.develop_of(target).unwrap().as_ref(), &manual);
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.develop_of(target).unwrap().as_ref(), second.as_ref());
    assert_eq!(s.match_look.applied[&target].managed_layer, Some(managed));
    assert_eq!(&s.match_look.applied[&target].last, second.as_ref());
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(&s.match_look.applied[&target].last, amount.as_ref());
}

#[test]
fn changed_reference_cannot_commit_an_in_flight_match() {
    let mut s = setup();
    let target = s.catalog.photos().find(|p| p.id != s.active().unwrap()).unwrap().id;
    let before = s.develop_of(target).unwrap();
    let undo = s.undo.len();
    let work = prepare(&mut s, &json!({"ids":[target]})).unwrap();
    let result = work.run(&Control::default(), false);
    let reference = s.match_look.references[0];
    s.execute("develop.matchLookReferences", &json!({"weightId":reference})).unwrap();
    assert_eq!(apply(&mut s, result).unwrap()["discarded"], true);
    assert_eq!(s.develop_of(target).unwrap(), before);
    assert_eq!(s.undo.len(), undo);
}

#[test]
fn references_and_weights_survive_catalog_reload_with_finished_previews() {
    use lightcraft_catalog::MemStore;
    let scratch = Scratch::new();
    let path = scratch.image("external.png", 4);
    let catalog = MemStore::new();
    let files = MemStore::new();
    let open = |s: &mut Session| {
        s.open_library_in(
            crate::library::LibraryStores {
                dir: "reference-workflow".into(),
                catalog: Box::new(catalog.clone()),
                files: Box::new(files.clone()),
                on_disk: false,
            },
            true,
        )
        .unwrap();
    };
    let mut s = Session::new().with_fs();
    open(&mut s);
    let reference = s.active().unwrap();
    s.execute("develop.matchLookReferences", &json!({"ids":[reference]})).unwrap();
    s.execute("develop.matchLookReferences", &json!({"weightId":reference})).unwrap();
    s.execute("develop.matchLookReferences", &json!({"paths":[path]})).unwrap();
    let external = s.match_look.external_references[0].id;
    s.execute("develop.matchLookReferences", &json!({"weightExternalId":external})).unwrap();
    s.execute("develop.analyzeLook", &json!({})).unwrap();
    let looks = selected_looks(&s).unwrap();
    let previews = selected_previews(&s);
    assert_eq!(looks.len(), 2);
    assert!(looks.iter().all(|l| l.importance == 2.0));
    s.close_library().unwrap();
    let mut reopened = Session::new().with_fs();
    open(&mut reopened);
    assert!(current_ready(&reopened));
    assert_eq!(selected_looks(&reopened).unwrap(), looks);
    assert_eq!(selected_previews(&reopened), previews);
    assert_eq!(reopened.match_look.reference_weights[&reference], 2.0);
}

#[test]
fn near_match_verification_releases_its_own_memory_reservation() {
    use lightcraft_sysmem::{HeavyJobs, MemoryNow};
    let jobs = HeavyJobs::new(1, 64 << 20, || MemoryNow { available: 1 << 30, rss: Some(0) });
    let mut primary = jobs.try_admit(32 << 20).unwrap();
    primary.finish_heavy(8 << 20);
    assert!(jobs.try_admit(128 << 20).is_none(), "retained bytes prevent an oversized single verification job");
    release_decode_for_verification(&mut primary);
    let verification = jobs.try_admit(128 << 20).expect("verification must not wait on its own thumbnail reservation");
    assert_eq!(verification.bytes(), 128 << 20);
}
