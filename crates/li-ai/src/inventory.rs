//! What is installed in ComfyUI, by family. The file lists come from `/object_info` (the loader
//! nodes' choices); each file is identified ([`crate::identify`]) from its header when the model
//! folder is on this machine, else from ComfyUI's `/view_metadata` (safetensors metadata) or the
//! file name. Text encoders, VAEs and adapters a family needs are found by name, as Krita AI
//! Diffusion does: exact names first, then the precision-independent stem, then patterns
//! (`a*b` = both parts present).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::arch::{self, FileKind};
use crate::catalog::InstalledModel;
use crate::comfy::{ObjectInfo, normalized_name};
use crate::family::{Family, FamilyKind, Registry, Role};
use crate::identify::Identity;

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

const WEIGHT_EXTENSIONS: &[&str] = &[".safetensors", ".gguf", ".sft", ".ckpt", ".pt", ".pth", ".bin"];

/// `pattern` matches `name` when every `*`-separated part occurs in it (case-insensitive). A part
/// ending in a weights extension must start the basename or follow `/ _ - .` (`ae.safetensors`
/// matches `flux_ae.safetensors`, not `flux2-vae.safetensors`); a leading `=` means the exact
/// basename.
pub fn pattern_matches(pattern: &str, name: &str) -> bool {
    let n = name.to_ascii_lowercase().replace('\\', "/");
    let base_at = n.rfind('/').map_or(0, |i| i + 1);
    let pattern = pattern.to_ascii_lowercase();
    if let Some(exact) = pattern.strip_prefix('=') {
        return n[base_at..] == *exact;
    }
    pattern.split('*').filter(|p| !p.is_empty()).all(|p| {
        if !WEIGHT_EXTENSIONS.iter().any(|e| p.ends_with(e)) {
            return n.contains(p);
        }
        n.match_indices(p).any(|(i, _)| i >= base_at && (i == base_at || matches!(n.as_bytes()[i - 1], b'/' | b'_' | b'-' | b'.')))
    })
}

/// The installed files for each component the family needs (exact names first, then the
/// precision-independent stems, then patterns in order; among stem or pattern hits the shortest
/// name wins).
pub fn resolve_components(f: &Family, info: &ObjectInfo) -> Vec<(Role, String)> {
    let mut out = Vec::new();
    for c in &f.components {
        let choices = component_choices(info, c.role);
        let exact = c.names.iter().find_map(|want| {
            let w = normalized_name(want);
            choices.iter().find(|x| normalized_name(x) == w).cloned()
        });
        let stem = || {
            c.stems.iter().find_map(|s| {
                let key = crate::stem::stem_key(s).key;
                choices.iter().filter(|x| crate::stem::stem_key(x).key == key).min_by_key(|x| x.len()).cloned()
            })
        };
        let found = exact
            .or_else(stem)
            .or_else(|| c.patterns.iter().find_map(|pat| choices.iter().filter(|x| pattern_matches(pat, x)).min_by_key(|x| x.len()).cloned()));
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

/// Identities of local files, keyed by path and listing kind, valid for a size, modification
/// time and profile generation.
type IdentityCache = BTreeMap<(PathBuf, FileKind), (u64, std::time::SystemTime, u64, Identity)>;
static CACHE: Mutex<IdentityCache> = Mutex::new(BTreeMap::new());

/// Identifies a local file a loader of kind `listed` lists, reading only its header (cached
/// for the live registry). `None` when the header can't be read.
pub fn identify_file(path: &Path, listed: FileKind, reg: &Registry) -> Option<Identity> {
    let meta = std::fs::metadata(path).ok()?;
    let (len, mtime) = (meta.len(), meta.modified().ok()?);
    let generation = crate::family::generation();
    let key = (path.to_owned(), listed);
    let live = std::ptr::eq(reg, crate::family::registry());
    if live
        && let Some((l, m, g, id)) = CACHE.lock().ok()?.get(&key)
        && (*l, *m, *g) == (len, mtime, generation)
    {
        return Some(id.clone());
    }
    let h = arch::read_header(path).ok()?;
    let name = path.file_name()?.to_string_lossy().to_string();
    let id = crate::identify::identify(&name, listed, Some(&h), reg);
    if live && let Ok(mut c) = CACHE.lock() {
        c.insert(key, (len, mtime, generation, id.clone()));
    }
    Some(id)
}

/// Identifies a listed file: its header when local, else ComfyUI's metadata (`metadata`, flat
/// values or a whole safetensors header), else its name.
pub fn identify_listed(name: &str, listed: FileKind, local: Option<&Path>, metadata: Option<&serde_json::Value>, reg: &Registry) -> Identity {
    if let Some(id) = local.and_then(|p| identify_file(p, listed, reg)) {
        return id;
    }
    if let Some(m) = metadata.and_then(|m| m.as_object()) {
        let header = m.contains_key("__metadata__") || m.values().any(|v| v.get("shape").is_some());
        let mut h = if header {
            serde_json::to_vec(m).ok().and_then(|bytes| arch::parse_safetensors_json(&bytes).ok()).unwrap_or_default()
        } else {
            arch::Header::default()
        };
        h.metadata.extend(
            m.iter()
                .filter(|(k, v)| k.as_str() != "__metadata__" && !v.is_object())
                .map(|(k, v)| (k.clone(), v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string()))),
        );
        let mut id = crate::identify::identify(name, listed, Some(&h), reg);
        if !header {
            // Flat metadata says nothing about the tensors: fall back to the name when it is silent.
            id.header_read = false;
            if id.arch.is_none() {
                return crate::identify::identify(name, listed, None, reg);
            }
        }
        return id;
    }
    crate::identify::identify(name, listed, None, reg)
}

/// Finds `name` (as ComfyUI lists it, maybe with a subfolder) under the model folder.
pub fn local_path(model_dir: Option<&Path>, folders: &[&str], name: &str) -> Option<PathBuf> {
    let dir = model_dir?;
    [dir.to_path_buf(), dir.join("models")].into_iter().flat_map(|base| folders.iter().map(move |f| base.join(f).join(name))).find(|p| p.is_file())
}

/// A readable label from a file name (`juggernautXL_v9Rdphoto2Lightning.safetensors` →
/// `juggernautXL v9Rdphoto2Lightning`).
pub fn label_for(name: &str) -> String {
    let base = name.replace('\\', "/");
    let base = base.rsplit('/').next().unwrap_or(&base);
    let stem = base.rsplit_once('.').map_or(base, |(s, _)| s);
    stem.replace(['_', '-'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Architecture of a listed file and why ([`identify_listed`]). `None` when nothing recognises
/// it, or when a file listed as a main model (`kind` checkpoint or diffusion model) is a LoRA.
/// A local header that matches no layout stays unknown: the name never overrides the weights.
pub fn family_of(name: &str, kind: FileKind, local: Option<&Path>, metadata: Option<&serde_json::Value>) -> Option<(String, String)> {
    let id = identify_listed(name, kind, local, metadata, crate::family::registry());
    let wants_model = matches!(kind, FileKind::Checkpoint | FileKind::DiffusionModel);
    if wants_model && id.kind == FileKind::Lora {
        return None;
    }
    id.arch.map(|a| (a, id.evidence))
}

/// Whether a file a main-model loader lists is really a LoRA (its header when local, else its
/// name): such files are left out of the model list.
pub fn listed_as_model_but_lora(name: &str, local: Option<&Path>) -> bool {
    match local.and_then(|p| identify_file(p, FileKind::DiffusionModel, crate::family::registry())) {
        Some(id) => id.kind == FileKind::Lora,
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

/// The installed main weights and LoRAs. An empty family leaves recipe discovery to the refine
/// checker; template-only families stay listed (the template engine runs them). LoRAs, text
/// encoders and VAEs that a model loader lists are not models.
pub fn scan(info: &ObjectInfo, model_dir: Option<&Path>, reg: &Registry) -> (Vec<InstalledModel>, Vec<InstalledLora>) {
    let mut models = Vec::new();
    for (class, input, role, folders) in MAIN_LOADERS {
        for name in info.choices(class, input) {
            let kind = if *role == Role::Checkpoint { FileKind::Checkpoint } else { FileKind::DiffusionModel };
            let local = local_path(model_dir, folders, &name);
            let id = identify_listed(&name, kind, local.as_deref(), None, reg);
            // LoRAs saved into a model folder (or listed through extra model paths) aren't models.
            if id.kind == FileKind::Lora || local.is_none() && arch::name_looks_like_lora(&name) {
                continue;
            }
            if matches!(id.kind, FileKind::TextEncoder | FileKind::Vae) {
                continue;
            }
            let family = id.arch.clone().unwrap_or_default();
            if reg.family(&family).is_some_and(|f| f.kind == FamilyKind::Upscaler) {
                continue;
            }
            let prefix = if *role == Role::Checkpoint { "ckpt" } else { "unet" };
            let key = format!("{prefix}:{name}");
            if models.iter().any(|m: &InstalledModel| m.key == key) {
                continue;
            }
            models.push(InstalledModel {
                key,
                family,
                label: label_for(&name),
                files: [(*role, name)].into(),
                variant: id.variant,
                quant: id.quant.class,
                stem: id.stem,
            });
        }
    }
    let mut loras = Vec::new();
    for name in component_choices(info, Role::Lora) {
        let local = local_path(model_dir, &["loras"], &name);
        let id = identify_listed(&name, FileKind::Lora, local.as_deref(), None, reg);
        loras.push(InstalledLora { name, family: id.arch, evidence: id.evidence });
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
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

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
    fn scan_finds_models_by_name_and_keeps_unknown_files() {
        let (models, loras) = scan(&info(), None, crate::family::registry());
        let fam = |k: &str| models.iter().find(|m| m.key == k).map(|m| m.family.clone());
        assert_eq!(fam("ckpt:juggernautXL_v9.safetensors").as_deref(), Some("sdxl"));
        assert_eq!(fam("unet:flux1-dev-fp8.safetensors").as_deref(), Some("flux1"));
        assert_eq!(fam("unet:qwen_image_edit_2511_bf16.safetensors").as_deref(), Some("qwen-edit"));
        assert_eq!(fam("ckpt:mystery.safetensors").as_deref(), Some(""));
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

#[cfg(test)]
mod ds_metadata_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ds2_metadata_flat_modelspecific_and_kohya_classification() {
        for (metadata, expected) in [
            (json!({"modelspec.architecture":"stable-diffusion-xl-v1-base"}), "sdxl"),
            (json!({"ss_base_model_version":"sdxl_base_v1-0"}), "sdxl"),
            (json!({"ss_base_model_version":"sd_v1"}), "sd15"),
        ] {
            let got = family_of("unknown.safetensors", FileKind::Checkpoint, None, Some(&metadata));
            assert_eq!(got.map(|x| x.0).as_deref(), Some(expected), "{metadata}");
        }
    }

    #[test]
    fn ds2_metadata_safetensors_header_keeps_tensor_shapes_and_metadata() {
        for metadata in [
            json!({"model.diffusion_model.input_blocks.0.0.weight":{"dtype":"F16","shape":[320,4,3,3]},
                   "model.diffusion_model.label_emb.0.0.weight":{"dtype":"F16","shape":[1280,2816]},
                   "conditioner.embedders.0.transformer.text_model.final_layer_norm.weight":{"dtype":"F16","shape":[768]}}),
            json!({"__metadata__":{"modelspec.architecture":"stable-diffusion-xl-v1-base"}}),
            json!({"__metadata__":{"ss_base_model_version":"sdxl_base_v1-0"}}),
        ] {
            assert_eq!(family_of("unknown.safetensors", FileKind::Checkpoint, None, Some(&metadata)).map(|x| x.0).as_deref(), Some("sdxl"), "{metadata}");
        }
        let lora = json!({"__metadata__":{"modelspec.architecture":"stable-diffusion-xl-v1-base/lora"}});
        assert!(family_of("unknown.safetensors", FileKind::Checkpoint, None, Some(&lora)).is_none());
        assert_eq!(family_of("unknown.safetensors", FileKind::Lora, None, Some(&lora)).unwrap().0, "sdxl");
    }
}

/// Download and install an on-device catalog resource. The legacy runner descriptor is an
/// adapter input; all transport uses the shared verified downloader and common FileSpec.
/// Complete verified members survive retries. Invalid members are replaced only after a
/// verified replacement is available, so failed/cancelled downloads leave existing data intact.
pub fn download_on_device(spec: &li_seg::ModelSpec, model_dir: &Path, ctl: &crate::comfy::JobControl, progress: &dyn Fn(u64)) -> anyhow::Result<()> {
    download_on_device_with(spec, model_dir, ctl, progress, &|file, path, on_bytes| {
        crate::download::download_file(&file.url, path, file.bytes, &file.sha256, ctl, on_bytes)
    })
}

/// A private, uniquely owned staging directory. Drop cleans archives, extraction temporaries
/// and unpublished members on success, failure, cancellation or unwind.
struct ModelInstall(PathBuf);
impl ModelInstall {
    fn new(model_dir: &Path) -> anyhow::Result<Self> {
        let parent = model_dir.join("segmentation");
        std::fs::create_dir_all(&parent)?;
        let path = parent.join(format!(".install-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for ModelInstall {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            log::warn!("Could not remove model installation staging {}: {error}", self.0.display());
        }
    }
}

type ModelFetch<'a> = dyn Fn(&crate::catalog::FileSpec, &Path, &dyn Fn(u64)) -> anyhow::Result<()> + 'a;

fn download_on_device_with(
    spec: &li_seg::ModelSpec,
    model_dir: &Path,
    ctl: &crate::comfy::JobControl,
    progress: &dyn Fn(u64),
    fetch: &ModelFetch<'_>,
) -> anyhow::Result<()> {
    use crate::catalog::OnDeviceFiles;
    ctl.check()?;
    let adapted;
    let files = if crate::catalog::on_device_spec(spec.id) == Some(spec) {
        crate::catalog::on_device_files(spec.id).expect("cataloged resource has a file plan")
    } else {
        adapted = OnDeviceFiles::from_spec(spec);
        &adapted
    };
    // Reject unpinned or escaping descriptors before touching the filesystem. Runtime adapters
    // remain public for compatibility, but cannot weaken the catalog's download contract.
    for file in files.installed.iter().chain(files.package.iter()) {
        anyhow::ensure!(
            file.bytes > 0 && file.sha256.len() == 64 && file.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Model file {} has no valid size/SHA-256 pin",
            file.name
        );
        anyhow::ensure!(
            Path::new(&file.name).components().count() == 1 && Path::new(&file.name).components().all(|c| matches!(c, std::path::Component::Normal(_))),
            "Invalid model filename: {}",
            file.name
        );
        anyhow::ensure!(crate::download::host_allowed(&file.url), "Refusing to download from an unexpected host: {}", file.url);
    }
    let installed = |file: &crate::catalog::FileSpec| spec.files().find(|f| f.file == file.name).is_some_and(|f| li_seg::file_installed(model_dir, f));
    if let Some(package) = &files.package {
        if files.installed.iter().all(installed) {
            progress(package.bytes);
            return ctl.check();
        }
        let stage = ModelInstall::new(model_dir)?;
        let archive = stage.0.join(&package.name);
        fetch(package, &archive, progress)?;
        ctl.check()?;
        // Reuse the established ZIP parser and mandatory archive/member pins, but isolate its
        // fixed extraction temporary names from concurrent sessions and existing model files.
        li_seg::denoise::install_bundle(&archive, spec, &stage.0)?;
        ctl.check()?;
        for file in &files.installed {
            ctl.check()?;
            if !installed(file) {
                publish_member(&crate::download::target_path(&stage.0, file), &crate::download::target_path(model_dir, file), file)?;
            }
        }
        return Ok(());
    }
    files.visit(&|file, offset| {
        ctl.check()?;
        if installed(file) {
            progress(offset + file.bytes);
            return ctl.check();
        }
        // Each member gets its own staging directory; a completed member is immediately
        // published so retry does not redownload it if a later companion fails.
        let install = ModelInstall::new(model_dir)?;
        let staged = install.0.join(&file.name);
        fetch(file, &staged, &|n| progress(offset + n))?;
        ctl.check()?;
        publish_member(&staged, &crate::download::target_path(model_dir, file), file)?;
        drop(install);
        Ok(())
    })?;
    Ok(())
}

fn publish_member(staged: &Path, dest: &Path, file: &crate::catalog::FileSpec) -> anyhow::Result<()> {
    // Another session may have completed this member while we fetched it. Preserve its verified
    // file; only the established invalid-member repair path is allowed to remove an old copy.
    if dest.exists() {
        if crate::download::verify_existing(dest, file).unwrap_or(false) {
            return Ok(());
        }
        // Same-volume rename replaces only the known invalid member. Failure leaves the old
        // copy intact, and readers never see an absence between removal and publication.
        return std::fs::rename(staged, dest).map_err(Into::into);
    }
    // Never replace a file that appears concurrently at publication time. An identical install
    // winning the race is success; a different file still fails without changing its bytes.
    match std::fs::hard_link(staged, dest) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && crate::download::verify_existing(dest, file).unwrap_or(false) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod package_tests {
    use super::*;
    use crate::comfy::JobControl;
    use std::cell::{Cell, RefCell};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/fixtures").join(uuid::Uuid::new_v4().to_string());
            std::fs::create_dir_all(dir.join("segmentation")).unwrap();
            Self(dir)
        }
        fn no_staging(&self) {
            assert!(
                std::fs::read_dir(self.0.join("segmentation")).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().starts_with(".install-"))
            );
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn leak(value: String) -> &'static str {
        Box::leak(value.into_boxed_str())
    }
    fn pin(bytes: &[u8]) -> &'static str {
        leak(crate::mock_hub::sha(bytes))
    }
    fn bundle() -> li_seg::ModelSpec {
        li_seg::ModelSpec {
            id: "test-bundle",
            file: "test-main.onnx",
            bytes: 1,
            sha256: pin(b"x"),
            url: "https://huggingface.co/test/model",
            companions: Box::leak(
                vec![
                    li_seg::Companion { file: "test-text.onnx", bytes: 2, sha256: pin(b"xx"), url: "https://huggingface.co/test/text" },
                    li_seg::Companion { file: "test-vocab.json", bytes: 3, sha256: pin(b"xxx"), url: "https://huggingface.co/test/vocab" },
                ]
                .into_boxed_slice(),
            ),
            ..*li_seg::spec("clip-b32-laion").unwrap()
        }
    }

    /// Migrated from the UI retry test, now using real pins and proving failed repairs retain
    /// the old file. Complete members are retained; successful repair still replaces invalid ones.
    #[test]
    fn bundle_retry_preserves_verified_members_and_repairs_after_fetch() {
        let dir = Fixture::new();
        let spec = bundle();
        std::fs::write(li_seg::model_path(&dir.0, &spec), b"x").unwrap();
        let invalid = li_seg::companion_path(&dir.0, "test-text.onnx");
        std::fs::write(&invalid, b"old").unwrap();
        let ctl = JobControl::new();
        let failed = Cell::new(0);
        assert!(
            download_on_device_with(&spec, &dir.0, &ctl, &|_| {}, &|_, _, _| {
                failed.set(failed.get() + 1);
                anyhow::bail!("network failed")
            })
            .is_err()
        );
        assert_eq!(failed.get(), 1);
        assert_eq!(std::fs::read(&invalid).unwrap(), b"old");
        dir.no_staging();
        let fetched = RefCell::new(Vec::new());
        let progress = RefCell::new(Vec::new());
        download_on_device_with(&spec, &dir.0, &ctl, &|n| progress.borrow_mut().push(n), &|file, path, on_bytes| {
            fetched.borrow_mut().push(file.name.clone());
            assert!(!path.exists());
            let bytes = vec![b'x'; file.bytes as usize];
            assert_eq!(crate::mock_hub::sha(&bytes), file.sha256);
            std::fs::write(path, bytes)?;
            on_bytes(file.bytes);
            Ok(())
        })
        .unwrap();
        assert_eq!(fetched.into_inner(), ["test-text.onnx", "test-vocab.json"]);
        assert_eq!(progress.into_inner(), [1, 3, 6]);
        assert!(spec.files().all(|file| li_seg::file_installed(&dir.0, file)));
        download_on_device_with(&spec, &dir.0, &ctl, &|_| {}, &|_, _, _| panic!("verified members must not fetch")).unwrap();
        ctl.cancel();
        assert!(download_on_device_with(&spec, &dir.0, &ctl, &|_| {}, &|_, _, _| panic!("cancel must not fetch")).is_err());
        dir.no_staging();
    }

    #[test]
    fn cancellation_after_fetch_never_publishes_or_deletes_old_members() {
        let dir = Fixture::new();
        let spec = bundle();
        let dest = li_seg::model_path(&dir.0, &spec);
        std::fs::write(&dest, b"old").unwrap();
        let ctl = JobControl::new();
        let error = download_on_device_with(&spec, &dir.0, &ctl, &|_| {}, &|file, path, _| {
            std::fs::write(path, vec![b'x'; file.bytes as usize])?;
            ctl.cancel();
            Ok(())
        })
        .unwrap_err();
        assert!(error.is::<lightcraft_sysmem::jobs::Cancelled>());
        assert_eq!(std::fs::read(dest).unwrap(), b"old");
        dir.no_staging();
    }

    #[test]
    fn invalid_descriptors_fail_before_filesystem_or_fetch() {
        let dir = Fixture::new();
        let spec = bundle();
        for bad in [
            li_seg::ModelSpec { sha256: "", ..spec },
            li_seg::ModelSpec { file: "../escape", ..spec },
            li_seg::ModelSpec { url: "https://untrusted.invalid/model", ..spec },
        ] {
            assert!(download_on_device_with(&bad, &dir.0, &JobControl::new(), &|_| {}, &|_, _, _| panic!("invalid descriptor must not fetch")).is_err());
        }
        assert_eq!(std::fs::read_dir(dir.0.join("segmentation")).unwrap().count(), 0);
    }

    #[test]
    fn real_downloader_retains_completed_member_and_rejects_bad_companion() {
        let dir = Fixture::new();
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let host = server.server_addr().to_string();
        crate::download::allow_test_host(&host);
        let url = leak(format!("http://{host}/member"));
        let spec = li_seg::ModelSpec { url, companions: Box::leak(vec![li_seg::Companion { url, ..bundle().companions[0] }].into_boxed_slice()), ..bundle() };
        let worker = std::thread::spawn(move || {
            for body in [b"x".as_slice(), b"no".as_slice()] {
                server
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap()
                    .expect("download request")
                    .respond(tiny_http::Response::from_data(body))
                    .unwrap();
            }
        });
        let old = li_seg::companion_path(&dir.0, "test-text.onnx");
        std::fs::write(&old, b"old").unwrap();
        let error = download_on_device(&spec, &dir.0, &JobControl::new(), &|_| {}).unwrap_err();
        worker.join().unwrap();
        assert!(error.to_string().contains("checksum"), "{error:#}");
        assert_eq!(std::fs::read(li_seg::model_path(&dir.0, &spec)).unwrap(), b"x");
        assert_eq!(std::fs::read(old).unwrap(), b"old");
        dir.no_staging();
    }

    // The established li-seg ZIP fixture format, stored members only (no new ZIP dependency).
    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, body) in entries {
            let offset = out.len() as u32;
            out.extend(0x0403_4b50u32.to_le_bytes());
            out.extend([20, 0, 0, 0]);
            out.extend([0; 10]);
            out.extend((body.len() as u32).to_le_bytes());
            out.extend((body.len() as u32).to_le_bytes());
            out.extend((name.len() as u16).to_le_bytes());
            out.extend([0; 2]);
            out.extend(name.as_bytes());
            out.extend(*body);
            central.extend(0x0201_4b50u32.to_le_bytes());
            central.extend([20, 0, 20, 0, 0, 0]);
            central.extend([0; 10]);
            central.extend((body.len() as u32).to_le_bytes());
            central.extend((body.len() as u32).to_le_bytes());
            central.extend((name.len() as u16).to_le_bytes());
            central.extend([0; 12]);
            central.extend(offset.to_le_bytes());
            central.extend(name.as_bytes());
        }
        let cd = out.len() as u32;
        out.extend(&central);
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend([0; 4]);
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(cd.to_le_bytes());
        out.extend([0; 2]);
        out
    }

    #[test]
    fn denoise_archive_and_member_pins_install_once_with_complete_cleanup() {
        let dir = Fixture::new();
        let zip = zip_of(&[("pkg/main.onnx", b"x"), ("pkg/text.onnx", b"xx")]);
        let spec = li_seg::ModelSpec {
            bytes: zip.len() as u64,
            sha256: pin(&zip),
            companions: Box::leak(vec![bundle().companions[0]].into_boxed_slice()),
            task: li_seg::Task::Denoise { inner: "pkg/main.onnx", inner_bytes: 1, inner_sha256: pin(b"x"), companion_inners: &["pkg/text.onnx"] },
            ..bundle()
        };
        // Reference the replaced UI's package path on a second tiny fixture: it fetched and
        // extracted on every invocation, including when all installed members were verified.
        let legacy = Fixture::new();
        let legacy_calls = Cell::new(0);
        for _ in 0..2 {
            let archive = legacy.0.join("segmentation").join(format!("{}.dtmodel", spec.id));
            if archive.exists() {
                std::fs::remove_file(&archive).unwrap();
            }
            legacy_calls.set(legacy_calls.get() + 1);
            std::fs::write(&archive, &zip).unwrap();
            li_seg::denoise::install_bundle(&archive, &spec, &legacy.0).unwrap();
            std::fs::remove_file(&archive).unwrap();
        }
        let calls = Cell::new(0);
        download_on_device_with(&spec, &dir.0, &JobControl::new(), &|_| {}, &|file, path, on_bytes| {
            calls.set(calls.get() + 1);
            assert_eq!(file.bytes, zip.len() as u64);
            assert!(file.name.ends_with(".dtmodel"));
            std::fs::write(path, &zip)?;
            on_bytes(file.bytes);
            Ok(())
        })
        .unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(std::fs::read(li_seg::model_path(&dir.0, &spec)).unwrap(), b"x");
        assert_eq!(std::fs::read(li_seg::companion_path(&dir.0, "test-text.onnx")).unwrap(), b"xx");
        dir.no_staging();
        let progress = Cell::new(0);
        download_on_device_with(&spec, &dir.0, &JobControl::new(), &|n| progress.set(n), &|_, _, _| panic!("installed archive must not fetch")).unwrap();
        assert_eq!(progress.get(), zip.len() as u64);
        assert_eq!((legacy_calls.get(), calls.get()), (2, 1));
        for file in spec.files() {
            assert_eq!(std::fs::read(li_seg::companion_path(&legacy.0, file.file)).unwrap(), std::fs::read(li_seg::companion_path(&dir.0, file.file)).unwrap());
        }
        eprintln!(
            "small package sample: {} bytes, initial+retry fetches {} -> {}, transferred bytes {} -> {}",
            zip.len(),
            legacy_calls.get(),
            calls.get(),
            2 * zip.len(),
            zip.len()
        );
        assert_eq!(std::fs::read_dir(dir.0.join("segmentation")).unwrap().count(), 2);
        // A bad inner pin never modifies either installed destination, and the archive is cleaned.
        let bad = li_seg::ModelSpec {
            task: li_seg::Task::Denoise { inner: "pkg/main.onnx", inner_bytes: 1, inner_sha256: pin(b"y"), companion_inners: &["pkg/text.onnx"] },
            ..spec
        };
        assert!(
            download_on_device_with(&bad, &dir.0, &JobControl::new(), &|_| {}, &|_, path, _| {
                std::fs::write(path, &zip)?;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(std::fs::read(li_seg::model_path(&dir.0, &spec)).unwrap(), b"x");
        dir.no_staging();
    }

    #[test]
    fn failed_publication_preserves_old_bytes_and_successful_repair_is_atomic() {
        let dir = Fixture::new();
        let spec = bundle();
        let file = &crate::catalog::OnDeviceFiles::from_spec(&spec).installed[0];
        let dest = li_seg::model_path(&dir.0, &spec);
        std::fs::write(&dest, b"old").unwrap();
        let stage = ModelInstall::new(&dir.0).unwrap();
        let staged = stage.0.join("replacement");
        assert!(publish_member(&staged, &dest, file).is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"old");
        std::fs::write(&staged, b"x").unwrap();
        publish_member(&staged, &dest, file).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"x");
        assert!(!staged.exists(), "repair moved the replacement atomically");
        drop(stage);
        dir.no_staging();
    }

    #[test]
    fn overlapping_installs_isolate_staging_and_keep_matching_publication() {
        let dir = Fixture::new();
        let spec = li_seg::ModelSpec { companions: &[], ..bundle() };
        let ctl = JobControl::new();
        let inner_path = RefCell::new(PathBuf::new());
        download_on_device_with(&spec, &dir.0, &ctl, &|_| {}, &|_, outer, _| {
            download_on_device_with(&spec, &dir.0, &ctl, &|_| {}, &|_, inner, _| {
                assert_ne!(inner, outer);
                *inner_path.borrow_mut() = inner.to_path_buf();
                std::fs::write(inner, b"x")?;
                Ok(())
            })?;
            assert!(!inner_path.borrow().exists(), "other install cleaned only its staging");
            assert_eq!(std::fs::read(li_seg::model_path(&dir.0, &spec)).unwrap(), b"x");
            std::fs::write(outer, b"x")?;
            Ok(())
        })
        .unwrap();
        assert_eq!(std::fs::read_dir(dir.0.join("segmentation")).unwrap().count(), 1);
        dir.no_staging();
    }

    #[test]
    fn archive_cancel_after_fetch_cleans_up_without_installing() {
        let dir = Fixture::new();
        let spec = li_seg::Group::RawDenoise.official();
        let ctl = JobControl::new();
        assert!(
            download_on_device_with(spec, &dir.0, &ctl, &|_| {}, &|_, path, _| {
                std::fs::write(path, b"archive")?;
                ctl.cancel();
                Ok(())
            })
            .unwrap_err()
            .is::<lightcraft_sysmem::jobs::Cancelled>()
        );
        assert_eq!(std::fs::read_dir(dir.0.join("segmentation")).unwrap().count(), 0);
    }
}
