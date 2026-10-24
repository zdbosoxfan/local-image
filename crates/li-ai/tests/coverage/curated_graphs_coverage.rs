//! The curated builder graphs (Qwen Image 2.1, FLUX.2 Klein, AI Remove, Z-Image Turbo, SDXL,
//! SeedVR2) must not change when support for other models changes. Each case is compared with
//! the graph captured in `tests/fixtures/curated-graphs/`; `LI_AI_BLESS=1` rewrites the fixtures.

use std::collections::BTreeMap;
use std::path::PathBuf;

use li_ai::builders::{self, Params, Task};
use li_ai::family::{self, Family, InpaintMethod, Registry, Role};
use li_ai::workflows::{self, Files};
use serde_json::Value;

fn registry() -> Registry {
    let raw: BTreeMap<String, Value> = family::EMBEDDED.iter().map(|(k, t)| ((*k).to_owned(), serde_json::from_str(t).unwrap())).collect();
    Registry { families: family::build(&raw).unwrap(), ..Default::default() }
}

fn curated(reg: &Registry, key: &str, variant: &str) -> (Family, Files) {
    let (f, m) = reg.model(key).unwrap_or_else(|| panic!("{key}"));
    let preset = m.presets.iter().find(|p| p.variant == variant).unwrap_or_else(|| panic!("{key}:{variant}"));
    (f.resolved_for(m), preset.files.iter().map(|f| (f.role, f.name.clone())).collect())
}

fn fill_task(f: &Family) -> Task {
    match f.pipeline.inpaint {
        InpaintMethod::Instruction => Task::Edit,
        InpaintMethod::ModelConditioning => Task::Inpaint { denoise: 1.0 },
        _ => Task::Inpaint { denoise: 1.0 },
    }
}

fn graphs() -> Vec<(String, Value)> {
    let reg = registry();
    let mut out = Vec::new();
    let mut tasks = |name: &str, f: &Family, files: &Files| {
        let steps = f.sampling.steps.default as u32;
        let cfg = f.sampling.cfg.default;
        let base = Params { negative: "blurry", ..Params::create("a red ball on a white table", 1024, 768, 42, steps, cfg) };
        let cases = [
            ("create", Params { ..base.clone() }),
            ("edit-2refs", Params { task: Task::Edit, references: 2, ..base.clone() }),
            ("refine-035", Params { task: Task::Refine { denoise: 0.35 }, ..base.clone() }),
            ("fill", Params { task: fill_task(f), ..base.clone() }),
        ];
        for (task, p) in cases {
            if task == "edit-2refs" && !f.capabilities.edit {
                continue;
            }
            out.push((format!("{name}.{task}"), builders::build(f, files, &p).graph));
        }
    };
    for (key, variant) in [("qwen", "bf16"), ("qwen", "int8"), ("flux2-klein-4b", "bf16"), ("flux2-klein-9b", "fp8"), ("z-image-turbo", "bf16")] {
        let (f, files) = curated(&reg, key, variant);
        tasks(&format!("{key}-{variant}"), &f, &files);
    }
    let sdxl = reg.family("sdxl").unwrap().resolved_for_file("juggernautXL_v9.safetensors", None);
    tasks("sdxl-checkpoint", &sdxl, &[(Role::Checkpoint, "juggernautXL_v9.safetensors".to_owned())].into());
    let (_, remove) = curated(&reg, "klein-remove", "bf16");
    out.push(("klein-remove.remove".into(), workflows::klein_remove(&remove, 768, 512, 42).graph));
    let (_, seedvr2) = curated(&reg, "seedvr2", "fp16");
    out.push(("seedvr2.upscale".into(), workflows::seedvr2(&seedvr2, 42).graph));
    out
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/curated-graphs")
}

#[test]
fn curated_builder_graphs_match_the_captured_goldens() {
    let bless = std::env::var_os("LI_AI_BLESS").is_some();
    let mut failures = Vec::new();
    for (name, graph) in graphs() {
        let path = dir().join(format!("{name}.json"));
        if bless {
            std::fs::create_dir_all(dir()).unwrap();
            std::fs::write(&path, serde_json::to_string_pretty(&graph).unwrap() + "\n").unwrap();
            continue;
        }
        let want: Value = serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap();
        if want != graph {
            failures.push(format!("{name}: expected {want}\n got {graph}"));
        }
    }
    assert!(failures.is_empty(), "curated graphs changed:\n{}", failures.join("\n"));
}
