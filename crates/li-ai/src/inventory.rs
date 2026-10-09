//! What is installed in ComfyUI, by family. The file lists come from `/object_info` (the loader
//! nodes' choices); each file's family comes from its header ([`crate::arch`]) when the model
//! folder is on this machine, else from ComfyUI's `/view_metadata` (safetensors metadata) or the
//! file name. Text encoders, VAEs and adapters a family needs are found by name, as Krita AI
//! Diffusion does: exact names first, then substring patterns (`a*b` = both parts present).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::arch::{self, FileKind};
use crate::catalog::InstalledModel;
use crate::comfy::{ObjectInfo, normalized_name};
use crate::family::{Family, Registry, Role};

/// Where ComfyUI lists each kind of weights file.
const MAIN_LOADERS: &[(&str, &str, Role, &[&str])] = &[
    ("CheckpointLoaderSimple", "ckpt_name", Role::Checkpoint, &["checkpoints"]),
    ("UNETLoader", "unet_name", Role::Unet, &["diffusion_models", "unet"]),
    ("UnetLoaderGGUF", "unet_name", Role::Unet, &["unet", "diffusion_models", "unet_gguf"]),
];

/// The loader inputs that list files of a component role.
fn component_choices(info: &ObjectInfo, role: Role) -> Vec<String> {
    let mut v = match role {
        Role::Clip | Role::Clip2 | Role::Clip3 | Role::Clip4 => {
            let mut v = info.choices("CLIPLoader", "clip_name");
            v.extend(info.choices("DualCLIPLoader", "clip_name1"));
            v.extend(info.choices("CLIPLoaderGGUF", "clip_name"));
            v
        }
        Role::Vae => info.choices("VAELoader", "vae_name"),
        Role::ClipVision => info.choices("CLIPVisionLoader", "clip_name"),
        Role::StyleModel => info.choices("StyleModelLoader", "style_model_name"),
        Role::Upscaler => info.choices("UpscaleModelLoader", "model_name"),
        Role::Lora => info.choices("LoraLoader", "lora_name"),
        Role::Unet => info.choices("UNETLoader", "unet_name"),
        Role::Checkpoint => info.choices("CheckpointLoaderSimple", "ckpt_name"),
    };
    v.sort();
    v.dedup();
    v
}

/// `pattern` matches `name` when every `*`-separated part occurs in it (case-insensitive).
pub fn pattern_matches(pattern: &str, name: &str) -> bool {
    let n = name.to_ascii_lowercase().replace('\\', "/");
    pattern.to_ascii_lowercase().split('*').filter(|p| !p.is_empty()).all(|p| n.contains(p))
}

/// The installed files for each component the family needs (exact names first, then patterns in
/// order; among pattern hits the shortest name wins).
pub fn resolve_components(f: &Family, info: &ObjectInfo) -> Vec<(Role, String)> {
    let mut out = Vec::new();
    for c in &f.components {
        let choices = component_choices(info, c.role);
        let exact = c.names.iter().find_map(|want| {
            let w = normalized_name(want);
            choices.iter().find(|x| normalized_name(x) == w).cloned()
        });
        let found = exact.or_else(|| c.patterns.iter().find_map(|pat| choices.iter().filter(|x| pattern_matches(pat, x)).min_by_key(|x| x.len()).cloned()));
        if let Some(name) = found {
            out.push((c.role, name));
        }
    }
    // Redux / IP-Adapter helpers (only used when references are given).
    if f.pipeline.reference == crate::family::RefMethod::Redux {
        if let Some(n) = component_choices(info, Role::ClipVision).into_iter().find(|n| pattern_matches("sigclip_vision", n)) {
            out.push((Role::ClipVision, n));
        }
        if let Some(n) = component_choices(info, Role::StyleModel).into_iter().find(|n| pattern_matches("redux", n)) {
            out.push((Role::StyleModel, n));
        }
    }
    out
}

/// A detection cache keyed by path, size and modification time.
static CACHE: Mutex<BTreeMap<PathBuf, (u64, std::time::SystemTime, arch::Detected)>> = Mutex::new(BTreeMap::new());

/// Classifies a local file, reading only its header (cached).
pub fn detect_file(path: &Path) -> Option<arch::Detected> {
    let meta = std::fs::metadata(path).ok()?;
    let (len, mtime) = (meta.len(), meta.modified().ok()?);
    if let Some((l, m, d)) = CACHE.lock().ok()?.get(path)
        && *l == len
        && *m == mtime
    {
        return Some(d.clone());
    }
    let h = arch::read_header(path).ok()?;
    let name = path.file_name()?.to_string_lossy().to_string();
    let d = arch::classify(&h, &name);
    if let Ok(mut c) = CACHE.lock() {
        c.insert(path.to_owned(), (len, mtime, d.clone()));
    }
    Some(d)
}

/// Finds `name` (as ComfyUI lists it, maybe with a subfolder) under the model folder.
fn local_path(model_dir: Option<&Path>, folders: &[&str], name: &str) -> Option<PathBuf> {
    let dir = model_dir?;
    folders.iter().map(|f| dir.join(f).join(name)).find(|p| p.is_file())
}

/// A readable label from a file name (`juggernautXL_v9Rdphoto2Lightning.safetensors` →
/// `juggernautXL v9Rdphoto2Lightning`).
pub fn label_for(name: &str) -> String {
    let base = name.replace('\\', "/");
    let base = base.rsplit('/').next().unwrap_or(&base);
    let stem = base.rsplit_once('.').map_or(base, |(s, _)| s);
    stem.replace(['_', '-'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Family of a listed file: its header if local, else ComfyUI's metadata (`metadata`), else its
/// name. `None` when nothing recognises it — or when a file listed as a main model (`kind`
/// checkpoint or diffusion model) turns out to be a LoRA.
pub fn family_of(name: &str, kind: FileKind, local: Option<&Path>, metadata: Option<&serde_json::Value>) -> Option<(String, String)> {
    let wants_model = matches!(kind, FileKind::Checkpoint | FileKind::DiffusionModel);
    if let Some(p) = local
        && let Some(d) = detect_file(p)
    {
        if wants_model && d.kind == FileKind::Lora {
            return None;
        }
        if let Some(f) = d.family {
            return Some((f, d.evidence));
        }
    }
    if let Some(m) = metadata.and_then(|m| m.as_object()) {
        let meta: BTreeMap<String, String> = m.iter().map(|(k, v)| (k.clone(), v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string()))).collect();
        let h = arch::Header { metadata: meta, ..Default::default() };
        let d = arch::classify(&h, name);
        if wants_model && d.kind == FileKind::Lora {
            return None;
        }
        if let Some(f) = d.family {
            return Some((f, d.evidence));
        }
    }
    arch::guess_from_name(name, kind).map(|f| (f, "file name".to_owned()))
}

/// Whether a file a main-model loader lists is really a LoRA (its header when local, else its
/// name): such files are left out of the model list.
pub fn listed_as_model_but_lora(name: &str, local: Option<&Path>) -> bool {
    match local.and_then(detect_file) {
        Some(d) => d.kind == FileKind::Lora,
        None => arch::name_looks_like_lora(name),
    }
}

/// One LoRA found installed.
#[derive(Clone, Debug, PartialEq)]
pub struct InstalledLora {
    pub name: String,
    pub family: Option<String>,
    pub evidence: String,
}

/// The installed models (main weights with a recognised family) and LoRAs.
pub fn scan(info: &ObjectInfo, model_dir: Option<&Path>, reg: &Registry) -> (Vec<InstalledModel>, Vec<InstalledLora>) {
    let mut models = Vec::new();
    for (class, input, role, folders) in MAIN_LOADERS {
        for name in info.choices(class, input) {
            let kind = if *role == Role::Checkpoint { FileKind::Checkpoint } else { FileKind::DiffusionModel };
            let local = local_path(model_dir, folders, &name);
            // LoRAs saved into a model folder (or listed through extra model paths) aren't models.
            if listed_as_model_but_lora(&name, local.as_deref()) {
                continue;
            }
            let Some((family, _)) = family_of(&name, kind, local.as_deref(), None) else { continue };
            // A checkpoint-loading family found as a bare diffusion model (or the reverse) is still
            // listed; the loader follows the file.
            let Some(f) = reg.family(&family) else { continue };
            if f.kind != crate::family::FamilyKind::Image {
                continue;
            }
            let prefix = if *role == Role::Checkpoint { "ckpt" } else { "unet" };
            let key = format!("{prefix}:{name}");
            if models.iter().any(|m: &InstalledModel| m.key == key) {
                continue;
            }
            models.push(InstalledModel { key, family, label: label_for(&name), files: [(*role, name)].into() });
        }
    }
    let mut loras = Vec::new();
    for name in component_choices(info, Role::Lora) {
        let local = local_path(model_dir, &["loras"], &name);
        let found = family_of(&name, FileKind::Lora, local.as_deref(), None);
        loras.push(InstalledLora { name, family: found.as_ref().map(|f| f.0.clone()), evidence: found.map(|f| f.1).unwrap_or_default() });
    }
    (models, loras)
}

static LORAS: Mutex<Vec<InstalledLora>> = Mutex::new(Vec::new());

/// Scans and publishes the result: installed models join the catalogue, LoRAs are kept for the
/// style pickers. Returns true when the installed models changed.
pub fn refresh(info: &ObjectInfo, model_dir: Option<&Path>) -> bool {
    let (models, loras) = scan(info, model_dir, crate::family::registry());
    if let Ok(mut l) = LORAS.lock() {
        *l = loras;
    }
    crate::catalog::set_installed(models)
}

/// The installed LoRAs from the last [`refresh`].
pub fn loras() -> Vec<InstalledLora> {
    LORAS.lock().map(|l| l.clone()).unwrap_or_default()
}

/// The installed LoRAs that fit a family (unknown-family LoRAs are offered too, marked).
pub fn loras_for(f: &Family) -> Vec<(InstalledLora, bool)> {
    loras()
        .into_iter()
        .filter_map(|l| match &l.family {
            Some(fam) if f.lora_fits(fam) => Some((l, true)),
            Some(_) => None,
            None if !f.lora.loader.is_empty() => Some((l, false)),
            None => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn info() -> ObjectInfo {
        ObjectInfo(json!({
            "CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["juggernautXL_v9.safetensors", "dreamshaper_8.safetensors", "mystery.safetensors"]]}}},
            "UNETLoader": {"input": {"required": {"unet_name": [["flux1-dev-fp8.safetensors", "qwen_image_edit_2511_bf16.safetensors", "flux-2-klein-4b.safetensors"]]}}},
            "CLIPLoader": {"input": {"required": {"clip_name": [["qwen_2.5_vl_7b_fp8_scaled.safetensors", "qwen_3_4b.safetensors", "clip_l.safetensors", "t5xxl_fp8_e4m3fn_scaled.safetensors"]]}}},
            "DualCLIPLoader": {"input": {"required": {"clip_name1": [["clip_l.safetensors", "t5xxl_fp8_e4m3fn_scaled.safetensors"]]}}},
            "VAELoader": {"input": {"required": {"vae_name": [["ae.safetensors", "qwen_image_vae.safetensors", "flux2-vae.safetensors"]]}}},
            "LoraLoader": {"input": {"required": {"lora_name": [["pony_style.safetensors", "flux_realism.safetensors", "misc.safetensors"]]}}},
        }))
    }

    #[test]
    fn scan_finds_models_by_name_and_skips_unknown_files() {
        let (models, loras) = scan(&info(), None, crate::family::registry());
        let fam = |k: &str| models.iter().find(|m| m.key == k).map(|m| m.family.clone());
        assert_eq!(fam("ckpt:juggernautXL_v9.safetensors").as_deref(), Some("sdxl"));
        assert_eq!(fam("unet:flux1-dev-fp8.safetensors").as_deref(), Some("flux1"));
        assert_eq!(fam("unet:qwen_image_edit_2511_bf16.safetensors").as_deref(), Some("qwen-edit"));
        assert!(fam("ckpt:mystery.safetensors").is_none());
        assert_eq!(loras.iter().find(|l| l.name == "pony_style.safetensors").unwrap().family.as_deref(), Some("pony"));
    }

    /// LoRAs that sit in a model folder (a LoRA collection installed as a "model", or extra model
    /// paths) are not listed as models: by name when the file is remote, by header when local.
    #[test]
    fn loras_in_model_folders_are_not_models() {
        let dir = std::env::temp_dir().join(format!("li-inv-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(dir.join("diffusion_models")).unwrap();
        // A neutrally named file whose tensors are a FLUX LoRA's.
        let header = serde_json::to_vec(&json!({
            "double_blocks.18.img_attn.qkv.lora_A.weight": {"dtype": "F16", "shape": [16, 3072], "data_offsets": [0, 0]},
            "double_blocks.18.img_attn.qkv.lora_B.weight": {"dtype": "F16", "shape": [9216, 16], "data_offsets": [0, 0]}
        }))
        .unwrap();
        let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
        bytes.extend(header);
        std::fs::write(dir.join("diffusion_models").join("flux_ink.safetensors"), bytes).unwrap();
        let info = ObjectInfo(json!({
            "CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["juggernautXL_v9.safetensors", "sdxl_detail_LoRA.safetensors"]]}}},
            "UNETLoader": {"input": {"required": {"unet_name": [["flux1-dev-fp8.safetensors", "flux_realism_lora.safetensors", "loras/qwen_style.safetensors", "flux_ink.safetensors"]]}}},
            "LoraLoader": {"input": {"required": {"lora_name": [["flux_realism_lora.safetensors"]]}}},
        }));
        let (models, loras) = scan(&info, Some(&dir), crate::family::registry());
        let keys: Vec<&str> = models.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(keys, vec!["ckpt:juggernautXL_v9.safetensors", "unet:flux1-dev-fp8.safetensors"], "{keys:?}");
        // the LoRA loader's own list is unchanged
        assert_eq!(loras.len(), 1);
        assert_eq!(loras[0].family.as_deref(), Some("flux1"));
        // and the local header decides for the main loaders too
        let p = dir.join("diffusion_models").join("flux_ink.safetensors");
        assert!(listed_as_model_but_lora("flux_ink.safetensors", Some(&p)));
        assert_eq!(family_of("flux_ink.safetensors", FileKind::DiffusionModel, Some(&p), None), None);
        assert_eq!(family_of("flux_ink.safetensors", FileKind::Lora, Some(&p), None).map(|f| f.0).as_deref(), Some("flux1"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn components_resolve_by_name_then_pattern() {
        let reg = crate::family::registry();
        let flux = resolve_components(reg.family("flux1").unwrap(), &info());
        assert!(flux.contains(&(Role::Clip, "clip_l.safetensors".into())));
        assert!(flux.contains(&(Role::Clip2, "t5xxl_fp8_e4m3fn_scaled.safetensors".into())));
        assert!(flux.contains(&(Role::Vae, "ae.safetensors".into())));
        let qwen = resolve_components(reg.family("qwen-edit").unwrap(), &info());
        assert!(qwen.contains(&(Role::Vae, "qwen_image_vae.safetensors".into())));
        assert!(pattern_matches("anima*lllite*inpaint", "models/Anima-LLLite-Inpaint.safetensors"));
        assert_eq!(label_for("sub/juggernaut_XL-v9.safetensors"), "juggernaut XL v9");
    }

    #[test]
    fn local_headers_decide_the_family() {
        let dir = std::env::temp_dir().join(format!("li-inv-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(dir.join("checkpoints")).unwrap();
        // A file named like nothing in particular, but with SDXL tensors.
        let header = serde_json::to_vec(&json!({
            "model.diffusion_model.input_blocks.0.0.weight": {"dtype": "F16", "shape": [320, 4, 3, 3], "data_offsets": [0, 0]},
            "model.diffusion_model.label_emb.0.0.weight": {"dtype": "F16", "shape": [1280, 2816], "data_offsets": [0, 0]},
            "conditioner.embedders.0.transformer.text_model.final_layer_norm.weight": {"dtype": "F16", "shape": [768], "data_offsets": [0, 0]}
        }))
        .unwrap();
        let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
        bytes.extend(header);
        std::fs::write(dir.join("checkpoints").join("mystery.safetensors"), bytes).unwrap();
        let (models, _) = scan(&info(), Some(&dir), crate::family::registry());
        assert_eq!(models.iter().find(|m| m.key == "ckpt:mystery.safetensors").map(|m| m.family.as_str()), Some("sdxl"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
