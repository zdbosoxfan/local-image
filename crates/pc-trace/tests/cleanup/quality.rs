use super::{TestResult, common, metrics};
use crate::{
    Image, Params, Preset,
    cleanup::{self, Options, Upscale},
};

#[test]
fn cleanup_logo_same_input_quality_and_every_toggle() -> TestResult {
    let options = [
        ("axes", Options { axes: true, ..Options::default() }),
        ("collinear", Options { collinear: true, ..Options::default() }),
        ("right_angles", Options { right_angles: true, ..Options::default() }),
        ("circles", Options { circles: true, ..Options::default() }),
        ("ellipses", Options { ellipses: true, ..Options::default() }),
        ("arcs", Options { arcs: true, ..Options::default() }),
        ("symmetry", Options { symmetry: true, ..Options::default() }),
        ("aa_palette", Options { aa_palette: true, ..Options::default() }),
        ("prefilter", Options { prefilter: true, ..Options::default() }),
        ("small_holes", Options { small_holes: true, ..Options::default() }),
        ("upscale2", Options { upscale: Upscale::X2, ..Options::default() }),
        ("upscale4", Options { upscale: Upscale::X4, ..Options::default() }),
        ("logo", Options::logo()),
    ];
    let p = Params::for_preset(Preset::Logo);
    let mut measurements = Vec::new();
    let mut total_before = 0;
    let mut total_after = 0;
    let mut losses = Vec::new();
    for index in 0..6 {
        let case = common::case(index, 256, common::Variant::Clean)?;
        let image = Image { width: 256, height: 256, rgba: case.input.as_raw() };
        let baseline = crate::trace(image, &p)?;
        let fidelity = metrics::fidelity(&case.clean, &common::render(&baseline))?.fidelity;
        for (name, opts) in &options {
            let result = cleanup::trace(image, &p, opts)?;
            let score = metrics::fidelity(&case.clean, &common::render(&result.trace))?.fidelity;
            let loss = fidelity - score;
            eprintln!("cleanup {}/{name}: nodes {} -> {}, fidelity loss {loss:.6}, {:?}", case.id, baseline.nodes, result.trace.nodes, result.stats);
            measurements.push(serde_json::json!({"case":case.id,"toggle":name,"baseline_nodes":baseline.nodes,"nodes":result.trace.nodes,"fidelity_loss":loss,"stats":result.stats}));
            for l in &result.trace.layers {
                assert!(metrics::is_finite(&l.path));
                let crossings = metrics::self_intersections(&l.path);
                if !crossings.is_empty() {
                    // T1's coarse topology guard has a known tiny-cusp limit. A toggle
                    // may inherit its exact unchanged path, but never introduce a crossing.
                    let bytes = serde_json::to_vec(&l.path)?;
                    let inherited = baseline.layers.iter().any(|b| b.color == l.color && serde_json::to_vec(&b.path).is_ok_and(|v| v == bytes));
                    assert!(inherited && *name != "logo", "{} / {name}: {crossings:?}", case.id);
                }
            }
            if *name == "logo" {
                total_before += baseline.nodes;
                total_after += result.trace.nodes;
                losses.push(loss);
            }
        }
    }
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/cleanup-26");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("toggles.json"), serde_json::to_vec_pretty(&measurements)?)?;
    let reduction = 1. - total_after as f64 / total_before as f64;
    let max_loss = losses.into_iter().fold(f64::NEG_INFINITY, f64::max);
    eprintln!("Logo fast subset: reduction {reduction:.5}, max fidelity loss {max_loss:.6}");
    assert!(reduction >= 0.15, "Logo cleanup node reduction {reduction:.5} < .15");
    assert!(max_loss <= 0.002, "Logo cleanup loss {max_loss:.6} > .002");
    Ok(())
}

/// Missing references are an error. This gate never turns an absent corpus into a pass.
#[test]
#[ignore = "requires the 15 external competitor SVGs and 5 licensed scans of the release corpus"]
fn cleanup_full_competitor_gate() -> TestResult {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let corpus = common::external_cases_for(Preset::Logo)?;
    if corpus.len() < 20 {
        return Err(format!("missing corpus: supplied {} of required 15 external SVGs + 5 licensed scans", corpus.len()).into());
    }
    let mut ids = Vec::new();
    for index in 0..30 {
        for size in [256, 512, 1024] {
            for variant in common::Variant::ALL {
                ids.push(format!("logo-{index:02}-{size}-{}", variant.name()));
            }
        }
    }
    ids.extend(corpus.iter().map(|c| c.id.clone()));
    for tool in ["vtracer-alpha4", "vtracer-065"] {
        let dir = root.join("tests/references").join(tool).join("logo");
        if ids.iter().any(|id| !dir.join(format!("{id}.svg")).is_file()) {
            return Err(format!("missing complete {tool}/logo reference set").into());
        }
    }
    if cfg!(debug_assertions) {
        return Err("run full gate with --release".into());
    }
    let p = Params::for_preset(Preset::Logo);
    let mut baseline_nodes = 0;
    let mut cleaned_nodes = 0;
    let mut ours = Vec::new();
    let mut competitors = vec![Vec::new(), Vec::new()];
    let mut evaluate = |case: common::Case| -> TestResult {
        let image = Image { width: case.input.width(), height: case.input.height(), rgba: case.input.as_raw() };
        let baseline = crate::trace(image, &p)?;
        let cleaned = cleanup::trace(image, &p, &Options::logo())?;
        let before = common::score(&case, &baseline, 0.)?;
        let after = common::score(&case, &cleaned.trace, 0.)?;
        assert!(before.fidelity - after.fidelity <= 0.002, "{} cleanup fidelity regressed", case.id);
        baseline_nodes += baseline.nodes;
        cleaned_nodes += cleaned.trace.nodes;
        ours.push(after);
        for (i, tool) in ["vtracer-alpha4", "vtracer-065"].into_iter().enumerate() {
            competitors[i].push(common::reference_score(&case, &root.join("tests/references").join(tool).join("logo").join(format!("{}.svg", case.id)))?);
        }
        Ok(())
    };
    for index in 0..30 {
        for size in [256, 512, 1024] {
            for variant in common::Variant::ALL {
                evaluate(common::case(index, size, variant)?)?;
            }
        }
    }
    for case in corpus {
        evaluate(case)?;
    }
    assert!(cleaned_nodes as f64 <= baseline_nodes as f64 * 0.85);
    common::gates(&ours, &competitors, true)?;
    Ok(())
}
