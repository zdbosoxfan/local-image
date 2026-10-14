//! Local-only photographic acceptance runner. All inputs are read-only, outputs explicit.
//! Usage: matchlook_acceptance MANIFEST_JSON OUTPUT_DIRECTORY [CASE_ID]
//! The manifest has cases [{id,target,final,reference,multiple:[paths],editedRaw,editedSettings}].
//! Held-out finals are exported for human/independent patch review and never fitted.
use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::{RenderRequest, SourceInfo, match_look, render};
use lightcraft_raster::{Rgb32f, Rgba8};
use serde_json::{Value, json};
use std::{collections::HashMap, error::Error, path::Path, sync::Arc};
type Source = (Arc<Rgb32f>, SourceInfo);
fn load(cache: &mut HashMap<String, Source>, path: &str) -> Result<Source, Box<dyn Error>> {
    if let Some(s) = cache.get(path) {
        return Ok(s.clone());
    }
    let (image, info) = lightcraft_engine::files::load_vec(std::fs::read(path)?, 1536)?;
    let source = (Arc::new(image), info);
    cache.insert(path.into(), source.clone());
    Ok(source)
}
fn save(path: &Path, image: &Rgba8) -> Result<(), Box<dyn Error>> {
    std::fs::write(path, lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(image), &Default::default())?)?;
    Ok(())
}
fn pixel_drift(a: &Rgba8, b: &Rgba8) -> Value {
    let changes: Vec<_> = a.data.iter().zip(&b.data).flat_map(|(a, b)| (0..3).map(move |c| a[c].abs_diff(b[c]) as usize)).collect();
    json!({"meanEncodedCodeChange":changes.iter().sum::<usize>() as f64 / changes.len().max(1) as f64,"maxEncodedCodeChange":changes.iter().max().copied().unwrap_or(0)})
}
fn main() -> Result<(), Box<dyn Error>> {
    lightcraft_sysmem::init(Default::default());
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err("usage: matchlook_acceptance MANIFEST OUTPUT [CASE_ID]".into());
    }
    let manifest: Value = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    let output = Path::new(&args[1]);
    std::fs::create_dir_all(output)?;
    let base = DevelopSettings::default();
    let req = RenderRequest::fit(1024, 1024);
    let mut cache = HashMap::new();
    let mut cases = Vec::new();
    if let Some(near) = manifest["nearCases"].as_array() {
        for item in near {
            let (source, info) = load(&mut cache, item["target"].as_str().ok_or("near target missing")?)?;
            let settings: DevelopSettings = serde_json::from_slice(&std::fs::read(item["settings"].as_str().ok_or("near settings missing")?)?)?;
            let before = render(&source, &info, &settings, &req).image;
            for ev in [0.0, 0.03] {
                let mut desired = settings.clone();
                desired.light.exposure += ev;
                let look = match_look::analyze(&source, &info, &desired);
                let solved = match_look::solve(&source, &info, &settings, &[look], Default::default(), &|| false).ok_or("near solve failed")?;
                let image = render(&source, &info, &solved.settings, &req).image;
                cases.push(json!({"id":item["id"],"referenceExposureDelta":ev,"settingsIdentical":settings==solved.settings,"renderDrift":pixel_drift(&before,&image),"diagnosticBefore":solved.before_error,"diagnosticAfter":solved.after_error}));
            }
        }
        std::fs::write(output.join("near-match.json"), serde_json::to_vec_pretty(&cases)?)?;
        return Ok(());
    }
    if let Some(renders) = manifest["renders"].as_array() {
        for item in renders {
            let (source, info) = load(&mut cache, item["target"].as_str().ok_or("render target missing")?)?;
            let settings: DevelopSettings = serde_json::from_slice(&std::fs::read(item["settings"].as_str().ok_or("render settings missing")?)?)?;
            save(&output.join(format!("{}.png", item["id"].as_str().ok_or("render id missing")?)), &render(&source, &info, &settings, &req).image)?;
        }
        return Ok(());
    }
    for case in manifest["cases"].as_array().ok_or("missing cases")? {
        let id = case["id"].as_str().ok_or("missing id")?;
        if args.get(2).is_some_and(|filter| filter != id) {
            continue;
        }
        let out = output.join(id);
        std::fs::create_dir_all(&out)?;
        let (target, info) = load(&mut cache, case["target"].as_str().ok_or("missing target")?)?;
        let (final_src, final_info) = load(&mut cache, case["final"].as_str().ok_or("missing final")?)?;
        save(&out.join("heldout-final.png"), &render(&final_src, &final_info, &base, &req).image)?;
        save(&out.join("before.png"), &render(&target, &info, &base, &req).image)?;
        let (ref_src, ref_info) = load(&mut cache, case["reference"].as_str().ok_or("missing reference")?)?;
        let reference = match_look::analyze(&ref_src, &ref_info, &base);
        save(&out.join("reference.png"), &render(&ref_src, &ref_info, &base, &req).image)?;
        let mut refs = Vec::new();
        for path in case["multiple"].as_array().ok_or("missing multiple")? {
            let (s, i) = load(&mut cache, path.as_str().ok_or("invalid multiple path")?)?;
            refs.push(match_look::analyze(&s, &i, &base));
        }
        let (edited, edited_info) = load(&mut cache, case["editedRaw"].as_str().ok_or("missing editedRaw")?)?;
        let edited_settings: DevelopSettings =
            serde_json::from_slice(&std::fs::read(case["editedSettings"].as_str().ok_or("missing editedSettings")?)?)?;
        let edited_look = match_look::analyze(&edited, &edited_info, &edited_settings);
        save(&out.join("edited-raw-reference.png"), &render(&edited, &edited_info, &edited_settings, &req).image)?;
        let sync = render(&target, &info, &edited_settings, &req).image;
        save(&out.join("static-sync.png"), &sync)?;
        let mut results = Vec::new();
        for (name, looks, appearance) in [
            ("single", vec![reference.clone()], false),
            ("multiple", refs, false),
            ("appearance", vec![reference.clone()], true),
            ("edited-raw", vec![edited_look], false),
        ] {
            let opts = match_look::Options { appearance_aware: appearance, ..Default::default() };
            let t = std::time::Instant::now();
            let solved = match_look::solve(&target, &info, &base, &looks, opts, &|| false).ok_or("solve failed")?;
            let solve_ms = t.elapsed().as_millis();
            let image = render(&target, &info, &solved.settings, &req).image;
            save(&out.join(format!("{name}.png")), &image)?;
            std::fs::write(out.join(format!("{name}.json")), serde_json::to_vec_pretty(&solved.settings)?)?;
            let drift = if name == "single" {
                let duplicate = match_look::solve(&target, &info, &base, &[looks[0].clone(), looks[0].clone()], opts, &|| false)
                    .ok_or("duplicate solve failed")?;
                let dupimage = render(&target, &info, &duplicate.settings, &req).image;
                let repeat = match_look::solve(&target, &info, &solved.settings, &looks, opts, &|| false).ok_or("repeat solve failed")?;
                let repeatimage = render(&target, &info, &repeat.settings, &req).image;
                save(&out.join("repeated-pipeline-match.png"), &repeatimage)?;
                std::fs::write(out.join("repeated-pipeline-match.json"), serde_json::to_vec_pretty(&repeat.settings)?)?;
                json!({"duplicateReferenceDrift":pixel_drift(&image,&dupimage),"repeatedPipelineMatchDrift":pixel_drift(&image,&repeatimage),"duplicateSettingsIdentical":duplicate.settings==solved.settings,"repeatSettingsIdentical":repeat.settings==solved.settings})
            } else {
                Value::Null
            };
            results.push(json!({"variant":name,"elapsedMs":t.elapsed().as_millis(),"solveMs":solve_ms,"renders":solved.renders,"similarity":solved.similarity,"weights":solved.weights,"referenceLightness":looks.iter().map(|l|l.features[..5].to_vec()).collect::<Vec<_>>(),"goalLightness":solved.goal.features[..5],"solverDiagnosticBefore":solved.before_error,"solverDiagnosticAfter":solved.after_error,"drift":drift}));
            println!("{id} {name} complete");
            std::fs::write(out.join("diagnostics.json"), serde_json::to_vec_pretty(&results)?)?;
        }
        cases.push(json!({"case":case,"results":results}));
        std::fs::write(output.join("evaluation.json"), serde_json::to_vec_pretty(&cases)?)?;
    }
    Ok(())
}
