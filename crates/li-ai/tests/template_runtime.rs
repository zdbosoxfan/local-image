//! Official templates (ComfyUI 0.42.5, `tests/fixtures/comfy-0.42.5`) through the graph engine:
//! UI-to-API conversion values, canonicalisation and validation of every committed template.

use std::collections::BTreeMap;
use std::path::PathBuf;

use li_ai::arch::FileKind;
use li_ai::builders::{Params, Slot as Upload, Task};
use li_ai::comfy::ObjectInfo;
use li_ai::companions::Why;
use li_ai::graph::{self, canon, ui, validate};
use li_ai::template::compile::{self, Source};
use li_ai::template::index::{self as tindex, DeclaredFile, TemplateEntry};
use li_ai::template::matching::{self, Candidate, Query};
use li_ai::template::roles;
use li_ai::template::{InpaintWay, TemplateRecipe};
use serde_json::{Map, Value, json};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/comfy-0.42.5")
}

fn info() -> ObjectInfo {
    ObjectInfo(serde_json::from_slice(&std::fs::read(fixtures().join("object_info.min.json")).unwrap()).unwrap())
}

fn template(name: &str) -> Value {
    serde_json::from_slice(&std::fs::read(fixtures().join("templates").join(format!("{name}.json"))).unwrap()).unwrap()
}

const ALL: &[&str] = &[
    "image_flux3_klein_t2i",
    "image_flux3_klein_distilled_t2i",
    "image_flux3_klein_image_edit",
    "image_flux3_klein_distilled_image_edit",
    "image_moss_image_t2i",
    "image_moss_image_base_t2i",
    "image_krea2_turbo_t2i",
    "image_krea2_turbo_t2i_int8",
    "image_krea2_turbo_int8_image_style_reference",
    "image_qwen_image_2_1_turbo_t2i",
    "image_qwen_image_2_1_turbo_image_edit",
    "image_flux2_klein_image_edit_9b_base",
    "hidream_e1_1",
    "image_hidream_o1",
];

/// The stem key of the template's first declared main model file.
fn main_stem(t: &Value) -> Option<String> {
    let nodes = t["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(t["definitions"]["subgraphs"].as_array().into_iter().flatten().flat_map(|d| d["nodes"].as_array().into_iter().flatten()));
    nodes
        .flat_map(|n| n["properties"]["models"].as_array().cloned().unwrap_or_default())
        .find(|m| matches!(m["directory"].as_str(), Some("diffusion_models" | "unet" | "checkpoints")))
        .and_then(|m| m["name"].as_str().map(|n| li_ai::stem::stem_key(n).key))
}

fn converted(name: &str) -> ui::Converted {
    ui::to_api(&template(name), &info()).unwrap_or_else(|e| panic!("{name}: {e:#}"))
}

fn canonical(name: &str) -> Vec<canon::Canonical> {
    let t = template(name);
    let stem = main_stem(&t);
    let info = info();
    let opts = canon::Options { info: &info, io_outputs: &[], main_stem: stem.as_deref(), zero: roles::ZERO_CLASSES };
    canon::canonicalise(&converted(name), &opts).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn node_of<'a>(g: &'a Map<String, Value>, class: &str) -> &'a Value {
    g.values().find(|n| n["class_type"] == class).unwrap_or_else(|| panic!("no {class}"))
}

#[test]
fn instance_values_override_stale_inner_widgets() {
    let base = converted("image_flux3_klein_t2i").graph;
    let k = &node_of(&base, "KSampler")["inputs"];
    assert_eq!((&k["steps"], &k["cfg"], &k["sampler_name"], &k["scheduler"]), (&json!(50), &json!(3.5), &json!("euler"), &json!("simple")));
    let distilled = converted("image_flux3_klein_distilled_t2i").graph;
    let k = &node_of(&distilled, "KSampler")["inputs"];
    assert_eq!((&k["steps"], &k["sampler_name"], &k["scheduler"]), (&json!(16), &json!("euler"), &json!("simple")));
    let qwen_edit = converted("image_qwen_image_2_1_turbo_image_edit").graph;
    assert_eq!(node_of(&qwen_edit, "CFGGuider")["inputs"]["cfg"], json!(1), "the instance's cfg 1, not the inner 8");
}

#[test]
fn ordered_instance_values_map_to_widget_inputs() {
    // The Krea style template's instance has no named values: 12 ordered values for its 12
    // widget-typed inputs (the three image inputs take none).
    let g = converted("image_krea2_turbo_int8_image_style_reference").graph;
    let ms = &node_of(&g, "ModelSamplingFlux")["inputs"];
    let latent = &node_of(&g, "EmptyLatentImage")["inputs"];
    assert_eq!((&ms["width"], &ms["height"]), (&latent["width"], &latent["height"]), "the shift follows the same size source as the latent");
    assert!(node_of(&g, "RandomNoise")["inputs"]["noise_seed"].as_u64().is_some());
    let lora = &node_of(&g, "LoraLoaderModelOnly")["inputs"];
    assert_eq!((&lora["lora_name"], &lora["strength_model"]), (&json!("krea2_style_reference.safetensors"), &json!(1)));
    assert_eq!(node_of(&g, "UNETLoader")["inputs"]["unet_name"], "krea2_turbo_int8_convrot.safetensors");
}

#[test]
fn dynamic_combos_take_their_option_and_nested_values() {
    let g = converted("image_flux3_klein_distilled_image_edit").graph;
    let r = &node_of(&g, "ReferenceMultiLatent")["inputs"];
    assert_eq!((&r["resolution"], r["resolution.megapixels"].as_f64()), (&json!("resize"), Some(1.0)));
    let t2i = converted("image_flux3_klein_distilled_t2i").graph;
    let tg = &node_of(&t2i, "TextGenerate")["inputs"];
    assert_eq!((&tg["sampling_mode"], &tg["sampling_mode.top_p"], &tg["sampling_mode.repetition_penalty"]), (&json!("on"), &json!(0.95), &json!(1.05)));
    assert!(tg.get("system_prompt").is_some_and(|v| graph::link_of(v).is_some()), "forceInput system_prompt stays a link");
}

#[test]
fn bypassed_instances_and_muted_branches_produce_no_nodes() {
    let g = converted("image_flux2_klein_image_edit_9b_base").graph;
    assert!(g.keys().all(|id| !id.starts_with("92:")), "the bypassed 2-reference instance has no inner nodes: {:?}", g.keys().collect::<Vec<_>>());
    assert!(g.keys().any(|id| id.starts_with("75:")));
}

#[test]
fn defaults_fill_inputs_older_templates_lack() {
    let mut g = converted("hidream_e1_1").graph;
    canon::fill_defaults(&mut g, &info());
    assert_eq!(node_of(&g, "ImageScaleToTotalPixels")["inputs"]["resolution_steps"], 1);
}

#[test]
fn every_template_canonicalises_and_validates() {
    let info = info();
    for name in ALL {
        for c in canonical(name) {
            let problems: Vec<String> = validate::validate(&c.graph, &info)
                .into_iter()
                .filter(|p| !p.is_file() && !(p.class == "LoadImage" && p.input == "image"))
                .map(|p| p.describe())
                .collect();
            assert!(problems.is_empty(), "{name} {:?}: {problems:?}", c.mode);
            for c2 in c.graph.values() {
                let class = c2["class_type"].as_str().unwrap();
                assert!(
                    !matches!(class, "TextGenerate" | "Flux3LayoutPrompt" | "MOSSImageLayoutPrompt" | "CreateBoundingBoxes" | "PreviewAny" | "ComfySwitchNode"),
                    "{name}: {class} survived"
                );
            }
            let titles: Vec<&str> = c.prompt.iter().filter_map(|(n, _)| c.graph[n]["_meta"]["title"].as_str()).collect();
            assert!(!titles.iter().any(|t| t.to_ascii_lowercase().contains("system")), "{name}: prompt slot is a system prompt");
            if let Some(e) = &c.enhanced {
                let problems: Vec<String> = validate::validate(&e.graph, &info).into_iter().filter(|p| !p.is_file()).map(|p| p.describe()).collect();
                assert!(problems.is_empty(), "{name} enhanced: {problems:?}");
                assert!(!e.prompt.is_empty(), "{name}");
            }
        }
    }
}

#[test]
fn upsamplers_are_kept_only_in_the_enhanced_variant() {
    for name in ["image_flux3_klein_distilled_t2i", "image_flux3_klein_t2i", "image_moss_image_t2i", "image_krea2_turbo_t2i", "image_qwen_image_2_1_turbo_t2i"]
    {
        let c = canonical(name).remove(0);
        assert!(c.enhanced.is_some(), "{name} offers a prompt enhancer");
    }
    assert!(canonical("hidream_e1_1").remove(0).enhanced.is_none());
}

#[test]
fn hidream_o1_splits_into_create_and_edit() {
    let modes: Vec<Option<String>> = canonical("image_hidream_o1").into_iter().map(|c| c.mode).collect();
    assert_eq!(modes, [Some("create".to_owned()), Some("edit".to_owned())]);
}

#[test]
fn unknown_classes_in_pruned_branches_do_not_fail() {
    let mut t = template("image_krea2_turbo_t2i");
    let extra = json!({"id": 9001, "type": "SomeCustomPreview", "mode": 0, "inputs": [], "outputs": [], "widgets_values": [1]});
    t["nodes"].as_array_mut().unwrap().push(extra);
    let info = info();
    let conv = ui::to_api(&t, &info).unwrap();
    assert_eq!(conv.unknown, vec![("9001".to_owned(), "SomeCustomPreview".to_owned())]);
    let stem = main_stem(&t);
    let c = canon::canonicalise(&conv, &canon::Options { info: &info, io_outputs: &[], main_stem: stem.as_deref(), zero: roles::ZERO_CLASSES }).unwrap();
    assert!(!c[0].graph.contains_key("9001"));
}

// Recipes for the owner's installed models (ComfyUI 0.42.5 templates).

const OWNER_UNETS: &[&str] = &[
    "flux-2-klein-9b-fp8.safetensors",
    "flux3-image-klein-base-bf16.safetensors",
    "flux3-image-klein-bf16.safetensors",
    "krea2_turbo_bf16.safetensors",
    "moss_image_sft_bf16.safetensors",
    "qwen_image_2.1_bf16.safetensors",
    "qwen_image_2.1_int8_convrot.safetensors",
    "qwen_image_2.1_turbo_bf16.safetensors",
];
const OWNER_CLIPS: &[&str] = &[
    "ministral-3-3b.safetensors",
    "qwen_3_4b.safetensors",
    "qwen_3_8b_fp8mixed.safetensors",
    "qwen3vl_8b_bf16.safetensors",
    "qwen3vl_8b_int8_convrot.safetensors",
    "qwen3vl_4b_bf16.safetensors",
    "qwen3.5_9b_bf16.safetensors",
];
const OWNER_VAES: &[&str] = &["ae.safetensors", "flux2-vae.safetensors", "qwen_image_2.1_vae_bf16.safetensors", "qwen_image_vae.safetensors"];
const OWNER_LORAS: &[&str] = &["flux-2-klein-object-remove.safetensors", "krea2_style_reference.safetensors", "nar_lora_joint_v9_comfyui.safetensors"];

fn owner_info() -> ObjectInfo {
    let mut info = info();
    info.0["UNETLoader"]["input"]["required"]["unet_name"] = json!([OWNER_UNETS]);
    info.0["CLIPLoader"]["input"]["required"]["clip_name"] = json!([OWNER_CLIPS]);
    info.0["VAELoader"]["input"]["required"]["vae_name"] = json!([OWNER_VAES]);
    info.0["LoraLoaderModelOnly"]["input"]["required"]["lora_name"] = json!([OWNER_LORAS]);
    info
}

fn header(fixture: &str) -> li_ai::arch::Header {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/headers").join(format!("{fixture}.json"));
    let v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    let mut o = serde_json::Map::new();
    for (k, t) in v.as_object().unwrap() {
        if k == "__metadata__" {
            o.insert(k.clone(), t.clone());
        } else if !k.starts_with("__") {
            o.insert(k.clone(), json!({"dtype": t["dtype"], "shape": t["shape"], "data_offsets": [0, 0]}));
        }
    }
    li_ai::arch::parse_safetensors_json(&serde_json::to_vec(&o).unwrap()).unwrap()
}

struct Corpus {
    entries: Vec<TemplateEntry>,
    bodies: BTreeMap<String, Value>,
    declared: BTreeMap<String, Vec<DeclaredFile>>,
}

fn corpus() -> Corpus {
    let index: Value = serde_json::from_slice(&std::fs::read(fixtures().join("templates/index.json")).unwrap()).unwrap();
    let entries = tindex::parse_template_index(&index);
    let bodies: BTreeMap<String, Value> = entries.iter().map(|e| (e.name.clone(), template(&e.name))).collect();
    let declared = bodies.iter().map(|(n, b)| (n.clone(), tindex::declared_files(n, b))).collect();
    Corpus { entries, bodies, declared }
}

/// Matches and compiles the recipe for an installed file, the way model setup does.
fn recipe_for(file: &str, header_fixture: &str) -> TemplateRecipe {
    let reg = li_ai::family::registry();
    let info = owner_info();
    let identity = li_ai::identify::identify(file, FileKind::DiffusionModel, Some(&header(header_fixture)), reg);
    let family = identity.arch.as_deref().and_then(|a| reg.family(a));
    let c = corpus();
    let candidates: Vec<Candidate> = c.entries.iter().map(|e| Candidate { entry: e, declared: &c.declared[&e.name] }).collect();
    let q = Query {
        file,
        identity: &identity,
        polish: family.and_then(|f| f.template.as_ref()),
        foreign_pins: &[],
        twins: &[],
        manual: None,
        server: tindex::Version::parse("0.42.5"),
    };
    let matches = matching::find(&q, &candidates);
    let sources: Vec<Source> = matching::best(&matches)
        .into_iter()
        .map(|m| Source {
            entry: c.entries.iter().find(|e| e.name == m.template).unwrap(),
            body: &c.bodies[&m.template],
            matched: m,
            templates_version: "0.11.96",
        })
        .collect();
    let rt = li_ai::companions::runtime();
    let user = BTreeMap::new();
    let input = compile::Input {
        key: &format!("unet:{file}"),
        file,
        identity: &identity,
        info: &info,
        runtime: rt,
        family,
        gpu: Some("NVIDIA GeForce RTX 5090"),
        user: &user,
        vae_channels: &|_| None,
    };
    compile::compile(&input, &sources).unwrap_or_else(|e| panic!("{file}: {e:?}"))
}

fn sources(r: &TemplateRecipe) -> Vec<(String, String)> {
    r.sources.iter().map(|s| (s.capability.clone(), s.name.clone())).collect()
}

fn binding<'a>(r: &'a TemplateRecipe, role: &str) -> &'a li_ai::companions::Binding {
    r.bindings().into_iter().find(|b| b.role == role).unwrap_or_else(|| panic!("no {role} binding"))
}

/// A graph for comparison: pruned to its output, ids replaced by structure, numbers as floats,
/// seeds, prompts, uploaded image names and file-name prefixes blanked.
fn comparable(g: &Map<String, Value>) -> BTreeMap<String, Value> {
    let mut g = g.clone();
    let output = g.iter().find(|(_, n)| n["class_type"] == "SaveImage").map(|(id, _)| id.clone()).unwrap();
    graph::prune_to(&mut g, &[output.as_str()]);
    for n in g.values_mut() {
        for (k, v) in n["inputs"].as_object_mut().unwrap().iter_mut() {
            if matches!(k.as_str(), "seed" | "noise_seed" | "text" | "prompt" | "filename_prefix" | "image") {
                *v = json!("");
            } else if let Some(x) = v.as_f64().filter(|_| v.is_number()) {
                *v = json!(x);
            }
        }
    }
    graph::normalised(&g)
}

fn verified(name: &str) -> Map<String, Value> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/verified").join(format!("{name}.json"));
    serde_json::from_slice::<Value>(&std::fs::read(p).unwrap()).unwrap().as_object().unwrap().clone()
}

fn built(r: &TemplateRecipe, p: &Params) -> Map<String, Value> {
    r.build(p, &owner_info()).unwrap_or_else(|e| panic!("{e:#}")).graph.as_object().unwrap().clone()
}

fn create(prompt: &str) -> Params<'_> {
    Params::create(prompt, 1024, 1024, 42, 16, 1.0)
}

#[test]
fn flux3_klein_distilled_runs_its_official_templates() {
    let r = recipe_for("flux3-image-klein-bf16.safetensors", "owner__diffusion_models~flux3-image-klein-bf16");
    assert_eq!(sources(&r), [("create".into(), "image_flux3_klein_distilled_t2i".into()), ("edit".into(), "image_flux3_klein_distilled_image_edit".into())]);
    assert!(r.sources.iter().all(|s| s.tier == "stem"));
    let clip = binding(&r, "clip");
    assert_eq!(
        (clip.declared.as_str(), clip.used.as_deref(), clip.why),
        ("qwen3vl_4b_int8_convrot.safetensors", Some("qwen3vl_4b_bf16.safetensors"), Why::SameStem)
    );
    assert_eq!((binding(&r, "vae").used.as_deref(), binding(&r, "vae").why), (Some("flux2-vae.safetensors"), Why::Exact));
    assert!(r.controls.guidance.is_none() && !r.controls.negative, "distilled: cfg fixed at 1, no negative field");
    assert_eq!(r.controls.steps.map(|s| (s.default, s.min, s.max)), Some((16.0, 4.0, 32.0)));
    assert_eq!((r.controls.references, r.inpaint, r.defaults.grid), (10, InpaintWay::Instruction, 16));
    let up = r.upsampler.as_ref().expect("the template offers a prompt enhancer");
    assert!(up.declared_file.starts_with("qwen3.5_9b_flux3_prompt_upsampler") && up.installed.is_none());
    let g = built(&r, &create("a house"));
    assert_eq!(comparable(&g), comparable(&verified("flux3_distilled")), "the create graph is the one verified on the RTX 5090");
    // Create with references: the edit graph on an empty latent (the verified edit shape).
    let p = Params { references: 1, ..create("make it noon") };
    let out = r.build(&p, &owner_info()).unwrap();
    assert_eq!(out.slots, [Upload::Reference(0)]);
    assert_eq!(comparable(out.graph.as_object().unwrap()), comparable(&verified("flux3_edit")));
    // Edit with two references: the source plus image1 and image2 on the autogrow input.
    let out = r.build(&Params { task: Task::Edit, references: 2, ..create("x") }, &owner_info()).unwrap();
    assert_eq!(out.slots, [Upload::Source, Upload::Reference(0), Upload::Reference(1)]);
    let rml = out.graph.as_object().unwrap().values().find(|n| n["class_type"] == "ReferenceMultiLatent").unwrap();
    for k in ["images.image0", "images.image1", "images.image2"] {
        assert!(rml["inputs"][k].is_array(), "{k}");
    }
    assert!(r.build(&Params { task: Task::Edit, references: 11, ..create("x") }, &owner_info()).is_err());
}

#[test]
fn flux3_klein_base_gets_its_guidance_and_edit_cfg_fix() {
    let r = recipe_for("flux3-image-klein-base-bf16.safetensors", "owner__diffusion_models~flux3-image-klein-base-bf16");
    assert_eq!(sources(&r), [("create".into(), "image_flux3_klein_t2i".into()), ("edit".into(), "image_flux3_klein_image_edit".into())]);
    assert_eq!(r.controls.guidance.map(|g| (g.default, g.max)), Some((3.5, 7.0)));
    assert!(r.controls.negative);
    let g = built(&r, &Params { steps: 50, cfg: 3.5, ..create("a house") });
    assert_eq!(comparable(&g), comparable(&verified("flux3_base")));
    let edit = r.edit.as_ref().unwrap();
    let cfg = &edit.graph[&edit.slots.cfg[0].0]["inputs"]["cfg"];
    assert_eq!(cfg.as_f64(), Some(3.5), "the base edit template's cfg 1 is corrected to 3.5");
}

#[test]
fn moss_sft_uses_the_installed_encoder_and_an_equivalent_vae() {
    let r = recipe_for("moss_image_sft_bf16.safetensors", "owner__diffusion_models~moss_image_sft_bf16");
    assert_eq!(sources(&r), [("create".into(), "image_moss_image_t2i".into())]);
    let clip = binding(&r, "clip");
    assert_eq!((clip.used.as_deref(), clip.why), (Some("qwen3.5_9b_bf16.safetensors"), Why::SameStem));
    let vae = binding(&r, "vae");
    assert_eq!((vae.declared.as_str(), vae.used.as_deref(), vae.why), ("flux2-vae_small.safetensors", Some("flux2-vae.safetensors"), Why::Equivalent));
    assert_eq!(r.defaults.negative, "nsfw, worst quality, bad anatomy, watermark, jpeg artifact");
    assert!(r.controls.negative);
    let g = built(&r, &Params { steps: 25, cfg: 2.5, ..create("a house") });
    assert!(!g.values().any(|n| n["class_type"] == "SkipLayerGuidanceDiTSimple" || n["class_type"] == "TextGenerate"));
    // The verified graph had skip-layer guidance on; the official SFT template has it off.
    let mut hand = verified("moss");
    let slg = hand.iter().find(|(_, n)| n["class_type"] == "SkipLayerGuidanceDiTSimple").map(|(id, _)| id.clone()).unwrap();
    let model = hand[&slg]["inputs"]["model"].clone();
    graph::rewire(&mut hand, (&slg, 0), &model, &[]);
    hand.remove(&slg);
    assert_eq!(comparable(&g), comparable(&hand));
    let family = compile::family_for(&r, li_ai::family::registry().family("moss-image"), "moss-image", "MOSS-Image");
    assert!(family.capabilities.create && !family.capabilities.edit && family.capabilities.inpaint);
    assert_eq!(family.prompt.negative, r.defaults.negative);
}

#[test]
fn krea2_turbo_creates_and_takes_style_references() {
    let r = recipe_for("krea2_turbo_bf16.safetensors", "Comfy-Org_Krea-2__diffusion_models~krea2_turbo_bf16");
    let caps: Vec<String> = sources(&r).into_iter().map(|(c, _)| c).collect();
    assert_eq!(caps, ["create", "style"]);
    assert!(r.sources[0].name.starts_with("image_krea2_turbo_t2i"));
    let clip = binding(&r, "clip");
    assert_eq!(clip.used.as_deref(), Some("qwen3vl_4b_bf16.safetensors"));
    assert_eq!(binding(&r, "vae").used.as_deref(), Some("qwen_image_vae.safetensors"));
    assert!(r.controls.guidance.is_none());
    let g = built(&r, &Params { steps: 8, ..create("a house") });
    assert_eq!(comparable(&g), comparable(&verified("krea")));
    let out = r.build(&Params { references: 1, width: 1216, height: 832, ..create("in this style") }, &owner_info()).unwrap();
    let g = out.graph.as_object().unwrap();
    let ms = g.values().find(|n| n["class_type"] == "ModelSamplingFlux").unwrap();
    assert_eq!((ms["inputs"]["width"].as_u64(), ms["inputs"]["height"].as_u64()), (Some(1216), Some(832)), "the shift follows the requested size");
    let lora = g.values().find(|n| n["class_type"] == "LoraLoaderModelOnly").unwrap();
    assert_eq!((lora["inputs"]["lora_name"].as_str(), lora["inputs"]["strength_model"].as_f64()), (Some("krea2_style_reference.safetensors"), Some(1.0)));
    assert_eq!(out.slots, [Upload::Reference(0)]);
    assert!(r.build(&Params { references: 4, ..create("x") }, &owner_info()).is_err(), "three style images at most");
}

#[test]
fn qwen_21_turbo_runs_its_template_with_fixed_sigmas() {
    let r = recipe_for("qwen_image_2.1_turbo_bf16.safetensors", "Comfy-Org_Qwen-Image-2.1__diffusion_models~qwen_image_2.1_turbo_bf16");
    assert_eq!(sources(&r), [("create".into(), "image_qwen_image_2_1_turbo_t2i".into()), ("edit".into(), "image_qwen_image_2_1_turbo_image_edit".into())]);
    assert!(r.controls.steps.is_none() && r.controls.guidance.is_none(), "8 fixed steps at cfg 1");
    assert_eq!(binding(&r, "clip").used.as_deref(), Some("qwen3vl_8b_int8_convrot.safetensors"));
    let g = built(&r, &create("a house"));
    assert!(g.values().any(|n| n["class_type"] == "ManualSigmas") && g.values().any(|n| n["class_type"] == "SamplerCustomAdvanced"));
    let refine = built(&r, &Params { task: Task::Refine { denoise: 0.4 }, ..create("a house") });
    let split = refine.values().find(|n| n["class_type"] == "SplitSigmasDenoise").expect("fixed sigmas are split for the strength");
    assert!((split["inputs"]["denoise"].as_f64().unwrap() - 0.4).abs() < 1e-6);
    let edit = r.build(&Params { task: Task::Edit, references: 2, ..create("x") }, &owner_info()).unwrap();
    let enc = edit.graph.as_object().unwrap().values().find(|n| n["class_type"] == "TextEncodeQwenImage21").unwrap();
    for k in ["images.image_1", "images.image_2", "images.image_3"] {
        assert!(enc["inputs"][k].is_array(), "{k}");
    }
}

#[test]
fn every_derived_graph_validates() {
    for (file, fixture) in [
        ("flux3-image-klein-bf16.safetensors", "owner__diffusion_models~flux3-image-klein-bf16"),
        ("moss_image_sft_bf16.safetensors", "owner__diffusion_models~moss_image_sft_bf16"),
        ("krea2_turbo_bf16.safetensors", "Comfy-Org_Krea-2__diffusion_models~krea2_turbo_bf16"),
    ] {
        let r = recipe_for(file, fixture);
        let loras = [li_ai::workflows::LoraUse { name: "krea2_style_reference.safetensors".into(), strength: 0.7 }];
        for p in [
            Params { task: Task::Refine { denoise: 0.35 }, ..create("x") },
            Params { task: Task::Inpaint { denoise: 1.0 }, ..create("x") },
            Params { loras: &loras, ..create("x") },
        ] {
            let g = built(&r, &p);
            let problems: Vec<String> = validate::validate(&g, &owner_info()).into_iter().filter(|p| !(p.class == "LoadImage")).map(|p| p.describe()).collect();
            assert!(problems.is_empty(), "{file} {:?}: {problems:?}", p.task);
        }
        let fill = r.build(&Params { task: Task::Inpaint { denoise: 1.0 }, ..create("x") }, &owner_info()).unwrap();
        assert_eq!(fill.slots, [Upload::Source, Upload::Mask], "{file}");
        let g = fill.graph.as_object().unwrap();
        assert!(g.values().any(|n| n["class_type"] == "SetLatentNoiseMask") && g.values().any(|n| n["class_type"] == "DifferentialDiffusion"), "{file}");
        let styled = built(&r, &Params { loras: &loras, ..create("x") });
        let model_users: Vec<&Value> = styled.values().filter(|n| n["inputs"]["model"].is_array()).collect();
        assert!(
            model_users
                .iter()
                .all(|n| styled[n["inputs"]["model"][0].as_str().unwrap()]["class_type"] != "UNETLoader" || n["class_type"] == "LoraLoaderModelOnly"),
            "{file}: every model consumer reads the LoRA chain"
        );
    }
}
