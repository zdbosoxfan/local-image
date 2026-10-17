//! Measured competitor gate; missing references are an error, not success.
use crate::common;
use common::TestResult;
use common::{Variant, case, gates, reference_score, score};
use pc_trace::{Image, Params, Preset, cutout::trace};
#[test]
fn cutout_logo_available_competitor_fidelity_and_node_gates() -> TestResult {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut ours = Vec::new();
    let mut competitors = vec![Vec::new(), Vec::new()];
    for index in 0..3 {
        for variant in Variant::ALL {
            let c = case(index, 256, variant)?;
            let start = std::time::Instant::now();
            let t = trace(Image { width: 256, height: 256, rgba: c.input.as_raw() }, &Params::for_preset(Preset::Logo))?;
            let s = score(&c, &t, start.elapsed().as_secs_f64())?;
            eprintln!("cutout/{} fidelity={} nodes={}", c.id, s.fidelity, s.nodes);
            ours.push(s);
            for (scores, tool) in competitors.iter_mut().zip(["vtracer-alpha4", "vtracer-065"]) {
                let file = root.join("tests/references").join(tool).join("logo").join(format!("{}.svg", c.id));
                if !file.is_file() {
                    return Err(format!("missing required competitor {}", file.display()).into());
                }
                scores.push(reference_score(&c, &file)?);
            }
        }
    }
    let dir = root.join("../../target/cutout27");
    std::fs::create_dir_all(&dir)?;
    let gate = gates(&ours, &competitors, true);
    std::fs::write(
        dir.join("competitor-scores.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"scope":"Logo / 3 synthetic fixtures / 256px / 6 variants", "ours":ours,"competitors":competitors,"gate":gate.as_ref().map(|_| "pass".to_string()).unwrap_or_else(|e| e.to_string())}),
        )?,
    )?;
    gate
}
