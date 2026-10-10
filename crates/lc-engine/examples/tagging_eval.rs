//! Opt-in, offline evaluation through Smart Sort's real decode/render/analyze/classify path.
//! Run once per model, in a separate process to measure peak RSS. No network or owner photos.
use lightcraft_catalog::{Op, Photo, Source};
use lightcraft_engine::{
    Session,
    smart_sort::{self, Category, SortPreset, TaggingModel, classify::Classifier},
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("usage: tagging_eval MODEL MANIFEST IMAGE_DIR MODEL_DIR".into());
    }
    let choice: TaggingModel = serde_json::from_value(json!(args[1]))?;
    let rows: Vec<Value> = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let root = std::fs::canonicalize(&args[3])?;
    let mut session = Session::new();
    let (loader, probe) = lightcraft_engine::files::fs_hooks();
    session.media.file_loader = Some(loader);
    session.media.file_probe = Some(probe);
    session.media.preview_loader = Some(lightcraft_engine::files::fs_preview_loader());
    eprintln!("render GPU: {:?}", lightcraft_gpu::adapter_name());
    session.quick_seg_dir = Some(PathBuf::from(&args[4]));
    session.smart.set_model(choice);
    let mut ids = Vec::new();
    for row in &rows {
        let file = row["file"].as_str().ok_or("file")?;
        let path = root.join(file);
        let bytes = std::fs::read(&path)?;
        let thumb = lightcraft_codecs::decode_thumbnail(&bytes, 1024)?;
        let id = session.catalog.alloc_photo_id();
        let mut photo = Photo::new(
            id,
            Source::File { path: path.to_string_lossy().into() },
            file,
            "JPEG",
            thumb.image.width as u32,
            thumb.image.height as u32,
            "2026-10-10",
        );
        photo.content_hash = Some(row["sha256"].as_str().ok_or("sha256")?.into());
        session.commit("Evaluation fixture", Op::AddPhoto { photo: Box::new(photo) })?;
        ids.push(id);
    }
    let start = Instant::now();
    let tagger = session.smart.tagger(session.quick_seg_dir.as_deref())?;
    let load_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    let result = smart_sort::analyze(&mut session, &ids, &AtomicBool::new(false), &|n, total| eprintln!("{n}/{total}"))?;
    let analysis_ms = start.elapsed().as_secs_f64() * 1000.;
    if !result.failed.is_empty() {
        return Err(format!("analysis failed on {} photos; first: {:?}", result.failed.len(), result.failed.first()).into());
    }
    let mut presets = smart_sort::builtin_presets();
    presets.retain(|p| p.name != "Custom");
    presets.push(SortPreset {
        name: "General".into(),
        categories: [
            ("People", vec!["portrait of a person", "people"]),
            ("Landscape", vec!["mountain landscape", "natural landscape"]),
            ("Food", vec!["food", "meal"]),
            ("Architecture", vec!["building architecture"]),
            ("Animals", vec!["wildlife animal"]),
            ("Events", vec!["festival", "parade"]),
        ]
        .into_iter()
        .map(|(name, prompts)| Category { name: name.into(), prompts: prompts.into_iter().map(str::to_string).collect(), ..Default::default() })
        .collect(),
        ..Default::default()
    });
    let start = Instant::now();
    let mut metrics = Vec::new();
    let mut predictions = Vec::new();
    for preset in presets {
        let classifier = Classifier::new(tagger.as_ref(), &session.smart.store, &preset)?;
        let mut top1 = 0;
        let mut top3 = 0;
        let mut n = 0;
        let mut categories = std::collections::BTreeMap::<String, (usize, usize, usize)>::new();
        for (row, id) in rows.iter().zip(&ids).filter(|(r, _)| r["preset"] == preset.name) {
            let key = lightcraft_engine::media::content_key(session.catalog.photo(*id).ok_or("photo")?);
            let predicted = classifier.classify(&key, session.smart.store.get(tagger.model_id(), &key), None)?;
            let ranked = &predicted.ranked;
            let label = row["label"].as_str().ok_or("label")?;
            top1 += usize::from(ranked.first().is_some_and(|r| r == label));
            top3 += usize::from(ranked.iter().take(3).any(|r| r == label));
            n += 1;
            for cat in &preset.categories {
                let gold = label == cat.name;
                let selected = predicted.assigned.contains(&cat.name);
                let counts = categories.entry(cat.name.clone()).or_default();
                counts.0 += usize::from(gold && selected);
                counts.1 += usize::from(!gold && selected);
                counts.2 += usize::from(gold && !selected);
            }
            predictions.push(json!({"id":row["id"],"preset":preset.name,"gold":label,"ranked":ranked,"assigned":predicted.assigned}));
        }
        metrics.push(json!({"preset":preset.name,"n":n,"top1":top1,"top3":top3,"top1_accuracy":top1 as f64/n.max(1)as f64,"top3_accuracy":top3 as f64/n.max(1)as f64,"categories":categories.into_iter().map(|(name,(tp,fp,fn_))|json!({"name":name,"tp":tp,"fp":fp,"fn":fn_,"precision":tp as f64/(tp+fp).max(1)as f64,"recall":tp as f64/(tp+fn_).max(1)as f64})).collect::<Vec<_>>()}));
    }
    let classify_ms = start.elapsed().as_secs_f64() * 1000.;
    let preset = smart_sort::builtin_presets().remove(0);
    let start = Instant::now();
    let classifier = Classifier::new(tagger.as_ref(), &session.smart.store, &preset)?;
    for id in &ids {
        let key = lightcraft_engine::media::content_key(session.catalog.photo(*id).ok_or("photo")?);
        let _ = classifier.classify(&key, session.smart.store.get(tagger.model_id(), &key), None)?;
    }
    let interactive_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    let repeat = smart_sort::analyze(&mut session, &ids, &AtomicBool::new(false), &|_, _| {})?;
    let resume_ms = start.elapsed().as_secs_f64() * 1000.;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"model":choice.id(),"photos":rows.len(),"analysed":result.analysed,"load_ms":load_ms,"analysis_ms":analysis_ms,"ms_per_photo":analysis_ms/rows.len().max(1)as f64,"classify_ms":classify_ms,"interactive_reclassify_ms":interactive_ms,"resume_ms":resume_ms,"repeat_analysed":repeat.analysed,"download_bytes":li_seg::spec(choice.id()).ok_or("model")?.download_bytes(),"metrics":metrics,"predictions":predictions})
        )?
    );
    Ok(())
}
