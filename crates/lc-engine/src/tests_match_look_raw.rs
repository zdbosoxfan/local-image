//! Reproducible CC0 RAW app-path validation. Originals stay outside git.
use crate::{Session, cmd::match_look as engine, media::SourceLevel};
use lightcraft_catalog::{MemStore, PhotoId};
use lightcraft_develop::{DevelopSettings, WbMode, Wheel};
use lightcraft_pipeline::{
    RenderRequest,
    match_look::{self, Options},
};
use serde_json::json;

fn imported() -> (Session, Vec<PhotoId>) {
    let dir = std::env::var("LOCAL_IMAGE_TEST_RAW_DIR").expect("set LOCAL_IMAGE_TEST_RAW_DIR to CC0 raws");
    let mut paths = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| ["arw", "cr2", "cr3", "nef", "raf", "dng", "rw2"].contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
        })
        .collect::<Vec<_>>();
    paths.sort();
    assert!(paths.len() >= 6, "six CC0 raws from different makers required");
    let mut s = Session::new().with_fs();
    s.open_library_in(
        crate::library::LibraryStores {
            dir: "raw-matchlook-test".into(),
            catalog: Box::new(MemStore::new()),
            files: Box::new(MemStore::new()),
            on_disk: false,
        },
        false,
    )
    .unwrap();
    let r = s.execute("library.import", &json!({"paths":paths,"mode":"add"})).unwrap();
    assert_eq!(r["failed"], json!([]), "{r}");
    let ids = s
        .catalog
        .photos()
        .filter(|p| {
            if !p.develops_raw() {
                println!("RAW_UNSUPPORTED {} {:?}", p.file_name, p.preview_only);
            }
            p.develops_raw()
        })
        .map(|p| p.id)
        .collect::<Vec<_>>();
    assert!(ids.len() >= 6, "six supported sensor raws required");
    (s, ids)
}
fn full(s: &mut Session, id: PhotoId, d: &DevelopSettings, edge: usize) -> match_look::Look {
    let photo = s.catalog.photo(id).unwrap().clone();
    let src = s.media.source_ref(&photo, SourceLevel::Preview).load_source().unwrap();
    match_look::measure(&lightcraft_pipeline::render(&src.image, &src.info_or(s.source_info(id)), d, &RenderRequest::fit(edge, edge)).image)
}
fn photo_image(s: &mut Session, id: PhotoId, d: &DevelopSettings, edge: usize) -> lightcraft_raster::Rgba8 {
    let photo = s.catalog.photo(id).unwrap().clone();
    let src = s.media.source_ref(&photo, SourceLevel::Preview).load_source().unwrap();
    lightcraft_pipeline::render(&src.image, &src.info_or(s.source_info(id)), d, &RenderRequest::fit(edge, edge)).image
}
fn sheet_parts(
    name: &str,
    mut metadata: serde_json::Value,
    references: &[(String, lightcraft_raster::Rgba8)],
    before: &lightcraft_raster::Rgba8,
    after: &lightcraft_raster::Rgba8,
) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/matchlook-final-sheets/data").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let save = |name: &str, image: &lightcraft_raster::Rgba8| {
        std::fs::write(dir.join(name), lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(image), &Default::default()).unwrap())
            .unwrap()
    };
    save("before.png", before);
    save("after.png", after);
    let refs = references
        .iter()
        .enumerate()
        .map(|(i, (label, image))| {
            let file = format!("ref-{i}.png");
            save(&file, image);
            json!({"label":label,"file":file})
        })
        .collect::<Vec<_>>();
    metadata["references"] = json!(refs);
    metadata["before"] = json!("before.png");
    metadata["after"] = json!("after.png");
    metadata["name"] = json!(name);
    std::fs::write(dir.join("sheet.json"), serde_json::to_vec_pretty(&metadata).unwrap()).unwrap();
}
// Preserve the original RAW distribution gate's 30-feature quantity. New texture-frequency /
// palette observables are audited separately; they are not a silent change of its threshold.
fn distribution_distance(a: &match_look::Look, b: &match_look::Look, opts: Options) -> f64 {
    let mut a = a.clone();
    let mut b = b.clone();
    a.support[30..].fill(0.0);
    b.support[30..].fill(0.0);
    match_look::distance(&a, &b, opts)
}
#[test]
#[ignore = "set LOCAL_IMAGE_TEST_RAW_DIR to a folder of CC0 raws"]
fn match_look_real_raw_end_to_end() {
    lightcraft_sysmem::init(Default::default());
    let (mut s, ids) = imported();
    let mut failures = Vec::new();
    let mut quality_metrics = Vec::new();
    let mut phase_metrics = Vec::new();
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/matchlook-revision3");
    std::fs::create_dir_all(&output).unwrap();
    let mut sheet = lightcraft_raster::Rgba8::new(960, ids.len().div_ceil(3) * 320);
    for (index, id) in ids.iter().enumerate() {
        let photo = s.catalog.photo(*id).unwrap().clone();
        if let lightcraft_catalog::Source::File { path } = &photo.source {
            let bytes = std::fs::read(path).unwrap();
            let raw = lightcraft_raw::decode(&bytes).unwrap();
            println!(
                "RAW_BIN id={} thumb_bin={:?} preview_bin={:?}",
                id.0,
                crate::files::bin_factor(&raw, 512),
                crate::files::bin_factor(&raw, 2560)
            );
        }

        let source = s.media.source_ref(&photo, SourceLevel::Preview).load_source().unwrap();
        let thumb = s.media.source_ref(&photo, SourceLevel::Thumb).load_source().unwrap();
        println!(
            "RAW_SCALE id={} thumb_sensor_scale={} preview_sensor_scale={}",
            id.0,
            thumb.info_or(s.source_info(*id)).sensor_scale,
            source.info_or(s.source_info(*id)).sensor_scale
        );
        let image = lightcraft_pipeline::render(
            &source.image,
            &source.info_or(s.source_info(*id)),
            &s.develop_of(*id).unwrap(),
            &RenderRequest::fit(320, 320),
        )
        .image;
        for y in 0..image.height {
            for x in 0..image.width {
                sheet.set(index % 3 * 320 + x, index / 3 * 320 + y, image.get(x, y));
            }
        }
        let facts = source.info_or(s.source_info(*id));
        println!(
            "RAW_FIXTURE id={} file={} camera={} sensor_decode={} relative_wb={} clip={} look_curve={}",
            id.0,
            s.catalog.photo(*id).unwrap().file_name,
            s.catalog.photo(*id).unwrap().meta.camera,
            facts.raw,
            facts.relative_wb,
            facts.raw_clip_level.is_some(),
            facts.look_curve.is_some()
        );
    }
    std::fs::write(
        output.join("raw-native-contact-sheet.png"),
        lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&sheet), &Default::default()).unwrap(),
    )
    .unwrap();
    let mut a = (*s.develop_of(ids[0]).unwrap()).clone();
    a.wb.mode = WbMode::Tungsten;
    a.light.exposure = 0.4;
    a.light.contrast = 20.0;
    a.grading.shadows = Wheel { hue: 220.0, sat: 15.0, lum: 0.0 };
    s.set_develop(ids[0], a.clone(), "Reference A").unwrap();
    let mut b = (*s.develop_of(ids[1]).unwrap()).clone();
    b.wb.mode = WbMode::Custom;
    b.wb.temp = s.source_info(ids[1]).as_shot_temp * 1.25;
    b.light.exposure = -0.3;
    b.color.saturation = -15.0;
    s.set_develop(ids[1], b, "Reference B").unwrap();
    s.execute("develop.matchLookReferences", &json!({"ids":[ids[0],ids[1]],"replace":true})).unwrap();
    s.execute("develop.analyzeLook", &json!({})).unwrap();
    let refs = engine::selected_looks(&s).unwrap();
    let reference_images = ids[..2]
        .iter()
        .enumerate()
        .map(|(n, id)| {
            let label =
                format!("Analyzed reference {} · {} · {}", n + 1, s.catalog.photo(*id).unwrap().meta.camera, s.catalog.photo(*id).unwrap().file_name);
            let settings = s.develop_of(*id).unwrap();
            (label, photo_image(&mut s, *id, &settings, 640))
        })
        .collect::<Vec<_>>();

    let mut crop = (*s.develop_of(ids[2]).unwrap()).clone();
    crop.crop.geometry.rect = lightcraft_geom::Rect::from_xywh(0.25, 0.25, 0.5, 0.5);
    s.set_develop(ids[2], crop, "Cropped target").unwrap();
    let mut layered = (*s.develop_of(ids[3]).unwrap()).clone();
    layered.masks.push(lightcraft_develop::Mask {
        id: 1,
        name: "Existing Filled layer".into(),
        components: vec![lightcraft_develop::MaskComponent {
            shape: lightcraft_develop::MaskShape::Full,
            op: Default::default(),
            invert: false,
            name: None,
        }],
        tools: lightcraft_develop::LayerTools {
            light: Some(lightcraft_develop::Light { contrast: 10.0, ..Default::default() }),
            ..Default::default()
        },
        ..Default::default()
    });
    s.set_develop(ids[3], layered, "Existing Filled layer").unwrap();
    let mut raw = (*s.develop_of(ids[4]).unwrap()).clone();
    raw.raw.demosaic = lightcraft_develop::Demosaic::Bilinear;
    s.set_develop(ids[4], raw, "Nondefault raw processing").unwrap();
    for layer in [false, true] {
        for consistent in [false, true] {
            let p = json!({"ids":ids[2..],"newLayer":layer,"consistentBatch":consistent});
            let work = engine::prepare(&mut s, &p).unwrap();
            let serial = engine::prepare(&mut s, &p).unwrap().run_serial(&engine::Control::default());
            let result = work.run(&engine::Control::default(), false);
            assert_eq!(serial.rows, result.rows, "serial and parallel edits must be identical");
            if !layer && !consistent {
                // elapsed_ms already measures the parallel batch wall time. Dividing by
                // workers again would overstate throughput. This remains an extrapolation;
                // the separate 500-photo app wall benchmark is authoritative.
                let equivalent = result.elapsed_ms / result.rows.len() as f64 * 500.0;
                if equivalent > 90_000.0 {
                    failures.push(format!("cold normalized equivalent500 {equivalent:.2} ms (app wall benchmark is authoritative)"));
                }
            }
            println!("RAW_PHASES workers={} per_photo={:?}", result.workers, result.per_photo);
            for timing in &result.per_photo {
                phase_metrics.push(json!({"newLayer":layer,"consistentBatch":consistent,"timing":timing}));
            }
            assert!(result.failed.is_empty(), "{:?}", result.failed);
            println!(
                "RAW_BATCH layer={layer} consistent={consistent} photos={} elapsed_ms={:.2} load_ms={:.2} solve_ms={:.2} equivalent500_ms={:.2}",
                result.rows.len(),
                result.elapsed_ms,
                result.load_ms,
                result.solve_ms,
                result.elapsed_ms / result.rows.len() as f64 * 500.0
            );
            let opts = Options { new_layer: layer, consistent_batch: consistent, ..Default::default() };
            for (identity, d) in &result.rows {
                let record = &result.applications[&identity.id];
                let reference = match_look::blend(&refs, &record.weights);
                let before = full(&mut s, identity.id, &identity.settings, 1024);
                let (goal, reach) = match_look::reachable_goal(&before, &reference, opts, record.similarity);
                let after = full(&mut s, identity.id, d, 1024);
                let sync = full(&mut s, identity.id, &a, 1024);
                let photo = s.catalog.photo(identity.id).unwrap().clone();
                let thumb = s.media.source_ref(&photo, SourceLevel::Thumb).load_source().unwrap();
                let proxy = match_look::analyze(&thumb.image, &thumb.info_or(s.source_info(identity.id)), d);
                println!(
                    "RAW_FIT id={} fit={:.4} reach={reach:.4} similarity={:.4} wb_ratio={:.4}",
                    identity.id.0, record.fit, record.similarity, record.wb_ratio
                );
                let deltas = [0, 2, 4].map(|i| (proxy.features[i] - after.features[i]).abs());
                println!(
                    "RAW_QUALITY id={} camera={} error={:.6} sync={:.6} L5={:.4} L50={:.4} L95={:.4} proxy={deltas:?} temp={:.1} contrast={:.1} saturation={:.1}",
                    identity.id.0,
                    s.catalog.photo(identity.id).unwrap().meta.camera,
                    distribution_distance(&after, &goal, opts),
                    distribution_distance(&sync, &goal, opts),
                    (after.features[0] - goal.features[0]).abs(),
                    (after.features[2] - goal.features[2]).abs(),
                    (after.features[4] - goal.features[4]).abs(),
                    d.wb.temp,
                    d.light.contrast,
                    d.color.saturation
                );
                let error = distribution_distance(&after, &goal, opts);
                let sync_error = distribution_distance(&sync, &goal, opts);
                let before_error = distribution_distance(&before, &goal, opts);
                let tonal_error = |look: &match_look::Look| {
                    [4.0, 1.0, 5.0, 1.0, 4.0].iter().enumerate().map(|(i, w)| (look.features[i] - goal.features[i]).powi(2) * w).sum::<f64>()
                };
                let gate = error < sync_error * 0.5
                    && (reach >= 0.6 || tonal_error(&after) < tonal_error(&before) * 0.5)
                    && (after.features[2] - goal.features[2]).abs() < 0.04
                    && [0, 4].iter().all(|i| (after.features[*i] - goal.features[*i]).abs() < 0.08)
                    && deltas.iter().all(|v| *v < 0.03)
                    && (2050.0..48000.0).contains(&d.wb.temp);
                let sheet_before = photo_image(&mut s, identity.id, &identity.settings, 1024);
                let sheet_after = photo_image(&mut s, identity.id, d, 1024);
                sheet_parts(
                    &format!("raw-{}-layer{layer}-consistent{consistent}", identity.id.0),
                    json!({"kind":"RAW modes","camera":photo.meta.camera,"mode":format!("{} · {}",if layer {"Layer"} else {"Global"},if consistent {"Consistent"} else {"Similarity"}),"fit":record.fit,"reach":reach,"errorBefore":before_error,"errorAfter":error,"extendedError":match_look::distance(&after,&goal,opts)}),
                    &reference_images,
                    &sheet_before,
                    &sheet_after,
                );
                quality_metrics.push(json!({"id":identity.id,"camera":s.catalog.photo(identity.id).unwrap().meta.camera,"newLayer":layer,"consistentBatch":consistent,"tonalError":tonal_error(&after),"beforeTonalError":tonal_error(&before),"extendedError":match_look::distance(&after,&goal,opts),"error":error,"referenceError":match_look::distance(&after,&reference,opts),"beforeReferenceError":match_look::distance(&before,&reference,opts),"beforeError":before_error,"syncError":sync_error,"fit":record.fit,"reach":reach,"similarity":record.similarity,"wbRatio":record.wb_ratio,"l5Error":(after.features[0]-goal.features[0]).abs(),"l50Error":(after.features[2]-goal.features[2]).abs(),"l95Error":(after.features[4]-goal.features[4]).abs(),"proxyDeltas":deltas,"kelvin":d.wb.temp,"contrast":d.light.contrast,"saturation":d.color.saturation,"pass":gate}));
                if !gate {
                    failures.push(format!(
                        "quality id={} layer={layer} consistent={consistent} error={error:.6} sync={sync_error:.6} proxy={deltas:?}",
                        identity.id.0
                    ));
                }
                assert!(d.wb.temp.is_finite() && (2000.0..=50000.0).contains(&d.wb.temp));
                assert!(d.light.contrast.abs() <= 60.0);
                assert!(d.color.saturation.abs() <= 55.0);
                assert!(after.valid());
            }
            let undo = s.undo.len();
            assert_eq!(engine::apply(&mut s, result).unwrap()["changed"], ids.len() - 2);
            assert_eq!(s.undo.len(), undo + 1);
            s.execute("edit.undo", &json!({})).unwrap();
        }
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/matchlook-revision3/export");
    std::fs::create_dir_all(&dir).unwrap();
    let exported = crate::export::export_photo(
        &mut s,
        ids[0],
        &crate::export::ExportOptions::from_json(&json!({"format":"jpeg","width":1024,"height":1024})),
        1,
    )
    .unwrap();
    let jpeg_path = dir.join("reference-a.jpg");
    std::fs::write(&jpeg_path, exported.bytes).unwrap();
    let imported = s.execute("library.import", &json!({"paths":[jpeg_path],"mode":"add"})).unwrap();
    assert_eq!(imported["failed"], json!([]));
    let jpeg = s.catalog.photos().find(|p| p.format.eq_ignore_ascii_case("JPEG") || p.format.eq_ignore_ascii_case("JPG")).unwrap().id;
    s.execute("develop.matchLookReferences", &json!({"ids":[jpeg],"replace":true})).unwrap();
    s.execute("develop.analyzeLook", &json!({})).unwrap();
    let result = engine::prepare(&mut s, &json!({"ids":[ids[2]],"consistentBatch":true})).unwrap().run(&engine::Control::default(), false);
    let goal = engine::selected_looks(&s).unwrap()[0].clone();
    let d = &result.rows[0].1;
    let after = full(&mut s, ids[2], d, 1024);
    let sync = full(&mut s, ids[2], &a, 1024);
    println!(
        "RAW_JPEG_REFERENCE error={:.6} sync={:.6} L5={:.4} L50={:.4} L95={:.4}",
        distribution_distance(&after, &goal, Options::default()),
        distribution_distance(&sync, &goal, Options::default()),
        (after.features[0] - goal.features[0]).abs(),
        (after.features[2] - goal.features[2]).abs(),
        (after.features[4] - goal.features[4]).abs()
    );
    if distribution_distance(&after, &goal, Options::default()) >= 0.5 * distribution_distance(&sync, &goal, Options::default())
        || (after.features[2] - goal.features[2]).abs() >= 0.04
        || [0, 4].iter().any(|i| (after.features[*i] - goal.features[*i]).abs() >= 0.08)
    {
        failures.push("JPEG-reference/raw-target quality gate".into());
    }
    s.execute("develop.matchLookReferences", &json!({"ids":[ids[0],ids[1]],"replace":true})).unwrap();
    s.execute("develop.analyzeLook", &json!({})).unwrap();
    let result = engine::prepare(&mut s, &json!({"ids":[ids[2],jpeg]})).unwrap().run(&engine::Control::default(), false);
    assert_eq!(result.rows.len(), 2);
    assert!(result.failed.is_empty());
    for (identity, d) in &result.rows {
        let _src = s.source_now(identity.id, SourceLevel::Preview).unwrap();
        let info = s.source_info(identity.id);
        println!(
            "RAW_MIXED id={} raw={} relative_wb={} shot_temp={} solved_temp={}",
            identity.id.0, info.raw, info.relative_wb, info.as_shot_temp, d.wb.temp
        );
        assert_eq!(info.raw, identity.id != jpeg);
        assert!(d.wb.temp.is_finite());
    }
    let preview_only = s.catalog.photos().find(|p| p.preview_only.is_some()).unwrap().id;
    let embedded = engine::prepare(&mut s, &json!({"ids":[preview_only]})).unwrap().run(&engine::Control::default(), false);
    assert_eq!(embedded.per_photo[0]["previewOnly"], true);
    let summary = engine::apply(&mut s, embedded).unwrap();
    assert_eq!(summary["previewOnly"], 1);
    println!("RAW_EMBEDDED_PREVIEW count={} weak={}", summary["previewOnly"], summary["weak"]);
    let all = engine::prepare(&mut s, &json!({"ids":ids})).unwrap().run(&engine::Control::default(), false);
    for (identity, settings) in &all.rows {
        let before = full(&mut s, identity.id, &identity.settings, 1024);
        let after = full(&mut s, identity.id, settings, 1024);
        let record = &all.applications[&identity.id];
        let reference = match_look::blend(&engine::selected_looks(&s).unwrap(), &record.weights);
        let (goal, reach) = match_look::reachable_goal(&before, &reference, Options::default(), record.similarity);
        println!(
            "RAW_ALL_CAMERAS id={} camera={} before={:.6} after={:.6} reference_error={:.6} reach={reach:.4}",
            identity.id.0,
            s.catalog.photo(identity.id).unwrap().meta.camera,
            match_look::distance(&before, &goal, Options::default()),
            distribution_distance(&after, &goal, Options::default()),
            match_look::distance(&after, &reference, Options::default())
        );
        let error_before = match_look::distance(&before, &goal, Options::default());
        let error_after = match_look::distance(&after, &goal, Options::default());
        if error_before < 0.05 {
            assert!(
                error_after <= error_before + 1e-12,
                "already close stays close: camera {} {error_before} -> {error_after}",
                s.catalog.photo(identity.id).unwrap().meta.camera
            );
        }
        let sheet_before = photo_image(&mut s, identity.id, &identity.settings, 1024);
        let sheet_after = photo_image(&mut s, identity.id, settings, 1024);
        sheet_parts(
            &format!("raw-all-{}", identity.id.0),
            json!({"kind":"RAW all cameras","camera":s.catalog.photo(identity.id).unwrap().meta.camera,"mode":"Global · Similarity","fit":record.fit,"reach":reach,"errorBefore":error_before,"errorAfter":error_after,"alreadyClose":all.per_photo.iter().find(|r| r["id"]==json!(identity.id)).is_some_and(|r| r["alreadyClose"]==true)}),
            &reference_images,
            &sheet_before,
            &sheet_after,
        );
    }
    let mode = if std::env::var_os("LOCAL_IMAGE_MATCHLOOK_SERIAL").is_some() { "serial" } else { "parallel" };
    std::fs::write(
        output.join(format!("raw-{mode}-metrics.json")),
        serde_json::to_vec_pretty(&json!({"quality":quality_metrics,"timings":phase_metrics,"failures":failures})).unwrap(),
    )
    .unwrap();
    let report = failures.join("\n");
    let file = if std::env::var_os("LOCAL_IMAGE_MATCHLOOK_SERIAL").is_some() { "raw-serial-gates.txt" } else { "raw-parallel-gates.txt" };
    std::fs::write(dir.parent().unwrap().join(file), &report).unwrap();
    assert!(failures.is_empty(), "RAW ACCEPTANCE GATES (all metrics were recorded):\n{report}");
}

#[test]
#[ignore = "native RAW render/solver profiling; set LOCAL_IMAGE_TEST_RAW_DIR"]
fn match_look_real_raw_preview_profile() {
    lightcraft_sysmem::init(Default::default());
    let (mut s, ids) = imported();
    let id = ids[3];
    let photo = s.catalog.photo(id).unwrap().clone();
    let decoded = s.media.source_ref(&photo, SourceLevel::Thumb).load_source().unwrap();
    let info = decoded.info_or(s.source_info(id));
    let base = s.develop_of(id).unwrap();
    println!("RAW_PROFILE source={}x{} scale={}", decoded.image.width, decoded.image.height, info.sensor_scale);
    for edge in [60, 256, 512] {
        let source = std::sync::Arc::new(lightcraft_raster::resample::fit(&decoded.image, edge, edge, lightcraft_raster::resample::Filter::Box));
        let mut facts = info.clone();
        facts.sensor_scale *= decoded.image.width.max(decoded.image.height) as f32 / source.width.max(source.height) as f32;
        let req = RenderRequest { depth: lightcraft_pipeline::OutputDepth::F32Linear, ..RenderRequest::fit(edge, edge) };
        for clarity in [0.0, 10.0] {
            let mut d = (*base).clone();
            d.effects.clarity = clarity;
            d.light.shadows = 8.0;
            let time = std::time::Instant::now();
            let render = lightcraft_sysmem::in_background_pool(|| lightcraft_pipeline::render(&source, &facts, &d, &req));
            let render_ms = time.elapsed().as_secs_f64() * 1000.0;
            let time = std::time::Instant::now();
            let look = match_look::measure_deep(render.deep.as_ref().unwrap());
            println!(
                "RAW_PROFILE edge={edge} clarity={clarity} render_ms={render_ms:.3} measure_ms={:.3} L50={}",
                time.elapsed().as_secs_f64() * 1000.0,
                look.features[2]
            );
        }
    }
    let mut desired = (*base).clone();
    desired.light.exposure = 0.4;
    desired.light.contrast = 20.0;
    desired.grading.shadows = Wheel { hue: 35.0, sat: 12.0, lum: 0.0 };
    let reference = match_look::analyze(&decoded.image, &info, &desired);
    let mut previous = None;
    let mut times = Vec::new();
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .start_handler(|_| {
                lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Background);
            })
            .build()
            .unwrap();
        let time = std::time::Instant::now();
        let result = pool
            .install(|| match_look::solve(&decoded.image, &info, &base, std::slice::from_ref(&reference), Options::default(), &|| false).unwrap());
        let ms = time.elapsed().as_secs_f64() * 1000.0;
        times.push(ms);
        if let Some(settings) = previous.take() {
            assert_eq!(settings, result.settings);
        }
        previous = Some(result.settings);
        println!("RAW_PREVIEW threads={threads} elapsed_ms={ms:.3} renders={}", result.renders);
    }
    println!("RAW_PREVIEW four_to_one_ratio={:.4}", times[1] / times[0]);
}
