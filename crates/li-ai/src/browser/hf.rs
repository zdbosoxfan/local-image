//! Hugging Face repositories as installs: the ComfyUI folder each file belongs in (from its path
//! in the repository), which files are not ComfyUI weights (diffusers layouts, shards), the
//! precisions of one model a card offers, and the optional files (LoRAs, a prompt enhancer), so an
//! install takes one precision that fits the GPU into the right folders, and extras only when
//! asked for, instead of every file into one.

use std::collections::BTreeMap;

use super::{CatalogFile, Config, Item, Kind, Net};
use crate::family::Registry;
use crate::identify::QuantClass;

/// Repository folder names and the ComfyUI folder they mean.
const FOLDERS: &[(&str, &str)] = &[
    ("diffusion_models", "diffusion_models"),
    ("unet", "diffusion_models"),
    ("checkpoints", "checkpoints"),
    ("text_encoders", "text_encoders"),
    ("clip", "text_encoders"),
    ("vae", "vae"),
    ("loras", "loras"),
    ("upscale_models", "upscale_models"),
    ("clip_vision", "clip_vision"),
    ("style_models", "style_models"),
    ("model_patches", "model_patches"),
    ("controlnet", "controlnet"),
];

/// The ComfyUI folder a repository path names (`split_files/text_encoders/x.safetensors` →
/// `text_encoders`): its nearest folder that is one.
pub fn folder_from_path(path: &str) -> Option<&'static str> {
    let dirs: Vec<&str> = path.split('/').collect();
    dirs[..dirs.len().saturating_sub(1)].iter().rev().find_map(|d| FOLDERS.iter().find(|(k, _)| d.eq_ignore_ascii_case(k)).map(|(_, f)| *f))
}

/// Whether `-NNNNN-of-NNNNN` marks a shard.
fn is_shard(name: &str) -> bool {
    let n = name.as_bytes();
    (0..n.len()).any(|i| {
        n[i..].len() >= 15
            && n[i] == b'-'
            && n[i + 1..i + 6].iter().all(u8::is_ascii_digit)
            && &n[i + 6..i + 10] == b"-of-"
            && n[i + 10..i + 15].iter().all(u8::is_ascii_digit)
    })
}

/// A repository file ComfyUI doesn't load as one weights file: a diffusers layout
/// (`diffusion_pytorch_model*`, `model*.safetensors` under `text_encoder*/`, `transformer/`,
/// `unet/` or `vae/`) or one shard of a split checkpoint.
pub fn not_comfy(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let mut parts: Vec<&str> = lower.split('/').collect();
    let name = parts.pop().unwrap_or_default();
    let diffusers_dir = parts.iter().any(|d| d.starts_with("text_encoder") || matches!(*d, "transformer" | "unet" | "vae"));
    name.starts_with("diffusion_pytorch_model") || diffusers_dir && name.starts_with("model") || is_shard(name)
}

/// The folder for a repository file of a `family` card: the folder its path names, else the
/// role of a family component it is named as (a root `ae.safetensors` is FLUX.1's VAE), else the
/// family's main model folder.
pub fn file_folder(reg: &Registry, family: Option<&str>, path: &str) -> &'static str {
    if let Some(f) = folder_from_path(path) {
        return f;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    let component = family
        .and_then(|f| reg.family(f))
        .and_then(|f| f.components.iter().find(|c| c.names.iter().any(|n| n.eq_ignore_ascii_case(name))))
        .map(|c| c.role.folder());
    component.unwrap_or_else(|| super::model_folder(reg, family, path))
}

/// The precision a file name says.
fn quant(name: &str) -> Option<QuantClass> {
    crate::stem::name_quant(&crate::stem::stem_key(name).quant_tokens)
}

/// Full precision first, then 8-bit, 4-bit with 8-bit activations, Blackwell's 4-bit, then GGUF.
fn rank(q: Option<&QuantClass>) -> u8 {
    match q {
        None | Some(QuantClass::Bf16 | QuantClass::Fp16 | QuantClass::Fp32) => 0,
        Some(QuantClass::Fp8Mixed | QuantClass::Fp8Scaled | QuantClass::Fp8E4m3fn | QuantClass::Fp8E5m2 | QuantClass::Mxfp8) => 1,
        Some(QuantClass::Int8Convrot | QuantClass::Int8) => 2,
        Some(QuantClass::W4a8) => 3,
        Some(QuantClass::Nvfp4 | QuantClass::Fp4Mixed) => 4,
        Some(QuantClass::Gguf(_) | QuantClass::Other(_)) => 5,
    }
}

/// Whether a precision runs only on a Blackwell GPU (NVFP4, FP4).
fn blackwell_only(q: Option<&QuantClass>) -> bool {
    q.and_then(|q| crate::companions::runtime().quant_rule(q).cloned()).is_some_and(|r| r.block && r.needs.as_deref() == Some("blackwell"))
}

fn is_main(f: &CatalogFile) -> bool {
    matches!(f.folder.as_str(), "diffusion_models" | "checkpoints")
}

/// The card's main model files when it offers more than one (each precision, or each model of the
/// repository), best first.
pub fn variants(item: &Item) -> Vec<&CatalogFile> {
    let mut v: Vec<&CatalogFile> = item.files.iter().filter(|f| is_main(f)).collect();
    if v.len() < 2 {
        return Vec::new();
    }
    v.sort_by_key(|f| (rank(quant(&f.name).as_ref()), f.name.to_ascii_lowercase()));
    v
}

/// The precision to offer first: the highest one that fits `vram` by model setup's memory estimate
/// (a file of unknown size is taken to fit), never a Blackwell-only one on another GPU; when none
/// fits, the smallest.
pub fn default_variant(item: &Item, blackwell: bool, vram: Option<u64>) -> Option<String> {
    let v = variants(item);
    let runs: Vec<&CatalogFile> = v.iter().copied().filter(|f| blackwell || !blackwell_only(quant(&f.name).as_ref())).collect();
    let fits = |f: &CatalogFile| match (vram, f.size()) {
        (Some(total), Some(bytes)) => crate::modelsetup::check::memory_need(bytes, 0, 1.0) <= 0.95 * total as f64,
        _ => true,
    };
    runs.iter().find(|f| fits(f)).or_else(|| runs.iter().min_by_key(|f| f.size().unwrap_or(u64::MAX))).or(v.first()).map(|f| f.name.clone())
}

/// Whether a file is a template's prompt enhancer (an LLM that rewrites the prompt: an
/// "upsampler", Qwen Image 2.1's `_pe_` models), which is never installed unless asked for.
pub fn is_prompt_enhancer(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("upsampler") || n.contains("enhancer") || n.split(['_', '-', '.']).any(|t| t == "pe")
}

/// Whether a model card's file installs only when ticked: a LoRA or a prompt enhancer.
pub fn optional(f: &CatalogFile) -> bool {
    f.folder == "loras" || is_prompt_enhancer(&f.name)
}

/// Of each file that ships in several precisions (same folder and stem), the one closest to the
/// precision rank `want`.
fn closest<'a>(files: impl Iterator<Item = &'a CatalogFile>, want: u8) -> Vec<CatalogFile> {
    let distance = |a: &CatalogFile| rank(quant(&a.name).as_ref()).abs_diff(want);
    let mut out: Vec<CatalogFile> = Vec::new();
    for f in files {
        let stem = crate::stem::stem_key(&f.name).key;
        match out.iter().position(|x| x.folder == f.folder && crate::stem::stem_key(&x.name).key == stem) {
            Some(i) if distance(f) < distance(&out[i]) => out[i] = f.clone(),
            Some(_) => {}
            None => out.push(f.clone()),
        }
    }
    out
}

/// A model card's main file `main` (its first when `None`) and that file's precision rank; `None`
/// for a card without a main file (a LoRA or restoration card), which installs as listed.
fn model_card<'a>(item: &'a Item, main: Option<&'a str>) -> Option<(Option<&'a str>, u8)> {
    let first = item.files.iter().find(|f| is_main(f)).map(|f| f.name.as_str());
    (item.kind != Some(Kind::Lora) && first.is_some()).then(|| (main, rank(quant(main.or(first).unwrap_or_default()).as_ref())))
}

/// What installing a card takes: of its main files `main` (every one when `None`), of each other
/// file it ships in several precisions the one closest to the main file's, and of the optional
/// files only those `extras` names.
pub fn selection(item: &Item, main: Option<&str>, extras: &[String]) -> Item {
    let Some((main, want)) = model_card(item, main) else { return item.clone() };
    let mut files: Vec<CatalogFile> = item.files.iter().filter(|f| is_main(f) && main.is_none_or(|m| f.name == m)).cloned().collect();
    files.extend(closest(item.files.iter().filter(|f| !is_main(f) && (!optional(f) || extras.contains(&f.name))), want));
    Item { files, ..item.clone() }
}

/// The optional files a card offers beside `main` (each in the precision closest to it).
pub fn optional_files(item: &Item, main: Option<&str>) -> Vec<CatalogFile> {
    let Some((_, want)) = model_card(item, main) else { return Vec::new() };
    closest(item.files.iter().filter(|f| !is_main(f) && optional(f)), want)
}

/// The card with each Hugging Face file's size from its repository's listing (a repository that
/// can't be listed leaves its files as they were).
pub fn with_sizes(item: &Item, cfg: &Config, net: &dyn Net) -> Item {
    let mut out = item.clone();
    let mut trees = BTreeMap::new();
    for f in &mut out.files {
        let Some((repo, path)) = f.hf_path(&cfg.hf) else { continue };
        let tree = trees.entry(repo.clone()).or_insert_with(|| super::hf_tree(cfg, net, &repo).ok());
        if let Some((size, _)) = tree.as_ref().and_then(|t| t.get(&path)) {
            f.approx_bytes = Some(*size);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_paths_name_the_folder_and_diffusers_files_are_left_out() {
        assert_eq!(folder_from_path("split_files/text_encoders/qwen_3_4b.safetensors"), Some("text_encoders"));
        assert_eq!(folder_from_path("split_files/diffusion_models/flux-2-klein-4b.safetensors"), Some("diffusion_models"));
        assert_eq!(folder_from_path("clip/t5xxl_fp8.safetensors"), Some("text_encoders"));
        assert_eq!(folder_from_path("flux1-dev.safetensors"), None);
        for diffusers in [
            "transformer/diffusion_pytorch_model-00001-of-00003.safetensors",
            "text_encoder_2/model-00001-of-00002.safetensors",
            "vae/diffusion_pytorch_model.safetensors",
            "unet/diffusion_pytorch_model.fp16.safetensors",
            "text_encoder/model.safetensors",
            "big_model-00002-of-00004.safetensors",
        ] {
            assert!(not_comfy(diffusers), "{diffusers}");
        }
        for comfy in ["flux1-dev.safetensors", "ae.safetensors", "split_files/vae/flux2-vae.safetensors", "model_card_image.safetensors"] {
            assert!(!not_comfy(comfy), "{comfy}");
        }
    }

    #[test]
    fn a_card_installs_one_precision_and_its_closest_companions() {
        let file = |name: &str, folder: &str| CatalogFile { name: name.into(), folder: folder.into(), ..Default::default() };
        let item = Item {
            files: vec![
                file("flux3-image-klein-nvfp4.safetensors", "diffusion_models"),
                file("flux3-image-klein-int8-convrot.safetensors", "diffusion_models"),
                file("flux3-image-klein-bf16.safetensors", "diffusion_models"),
                file("qwen3vl_4b_bf16.safetensors", "text_encoders"),
                file("qwen3vl_4b_int8_convrot.safetensors", "text_encoders"),
                file("qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors", "text_encoders"),
                file("flux2-vae.safetensors", "vae"),
                file("flux3_klein_style.safetensors", "loras"),
            ],
            ..Default::default()
        };
        let names: Vec<&str> = variants(&item).iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["flux3-image-klein-bf16.safetensors", "flux3-image-klein-int8-convrot.safetensors", "flux3-image-klein-nvfp4.safetensors"]);
        assert_eq!(default_variant(&item, false, None).as_deref(), Some("flux3-image-klein-bf16.safetensors"));
        let int8 = selection(&item, Some("flux3-image-klein-int8-convrot.safetensors"), &[]);
        let names: Vec<&str> = int8.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            ["flux3-image-klein-int8-convrot.safetensors", "qwen3vl_4b_int8_convrot.safetensors", "flux2-vae.safetensors"],
            "no enhancer or LoRA unasked"
        );
        let optional: Vec<String> = optional_files(&item, Some("flux3-image-klein-int8-convrot.safetensors")).into_iter().map(|f| f.name).collect();
        assert_eq!(optional, ["qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors", "flux3_klein_style.safetensors"]);
        let asked = selection(&item, Some("flux3-image-klein-int8-convrot.safetensors"), &optional[..1]);
        assert!(asked.files.iter().any(|f| f.name == optional[0]) && !asked.files.iter().any(|f| f.name == optional[1]));
        let nvfp4_only = Item {
            files: vec![file("krea2_turbo_nvfp4.safetensors", "diffusion_models"), file("krea2_turbo_bf16.safetensors", "diffusion_models")],
            ..Default::default()
        };
        assert_eq!(default_variant(&nvfp4_only, false, None).as_deref(), Some("krea2_turbo_bf16.safetensors"));
        let blackwell_first = Item {
            files: vec![file("krea2_turbo_nvfp4.safetensors", "diffusion_models"), file("krea2_turbo_int8.safetensors", "diffusion_models")],
            ..Default::default()
        };
        assert_eq!(default_variant(&blackwell_first, false, None).as_deref(), Some("krea2_turbo_int8.safetensors"), "never NVFP4 off Blackwell");
        assert_eq!(default_variant(&blackwell_first, true, None).as_deref(), Some("krea2_turbo_int8.safetensors"), "the higher precision first");
        assert!(variants(&Item { files: vec![file("only.safetensors", "checkpoints")], ..Default::default() }).is_empty());
        // A LoRA card installs as listed.
        let lora = Item { kind: Some(Kind::Lora), files: vec![file("a.safetensors", "loras"), file("b.safetensors", "loras")], ..Default::default() };
        assert_eq!(selection(&lora, None, &[]).files.len(), 2);
    }

    #[test]
    fn the_default_precision_is_the_largest_that_fits_the_gpu() {
        let sized = |name: &str, gb: u64| CatalogFile {
            name: name.into(),
            folder: "diffusion_models".into(),
            approx_bytes: Some(gb * 1_000_000_000),
            ..Default::default()
        };
        let item =
            Item { files: vec![sized("qwen_image_2.1_bf16.safetensors", 28), sized("qwen_image_2.1_int8_convrot.safetensors", 14)], ..Default::default() };
        let gb = |n: u64| Some(n * 1_000_000_000);
        assert_eq!(default_variant(&item, true, gb(48)).as_deref(), Some("qwen_image_2.1_bf16.safetensors"));
        assert_eq!(default_variant(&item, true, gb(24)).as_deref(), Some("qwen_image_2.1_int8_convrot.safetensors"));
        assert_eq!(default_variant(&item, true, gb(8)).as_deref(), Some("qwen_image_2.1_int8_convrot.safetensors"), "the smallest when none fits");
        assert!(is_prompt_enhancer("qwen3.5_9b_qwen_image_2.1_pe_t2i.int8_convrot.safetensors"));
        assert!(!is_prompt_enhancer("qwen3.5_9b_bf16.safetensors") && !is_prompt_enhancer("qwen3vl_8b_int8_convrot.safetensors"));
    }
}
