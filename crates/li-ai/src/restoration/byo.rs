//! Detection and execution of user-installed models. There are deliberately no download URLs,
//! installers or attribution entries here. A detected filename is a hint, not a licence audit.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::comfy::{ComfyClient, JobControl, ObjectInfo};

pub const NOTICE: &str = "Non-commercial licence: the authors restrict commercial use of the model; check the licence before using results in paid work";
pub const DATA_NOTICE: &str =
    "RestoreFormer++ code is Apache-2.0. Its FFHQ training data has non-commercial terms; this is a background risk, not a declared output licence.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Led,
    ZeroDce,
    ZeroDcePlus,
    StableSr,
    CodeFormer,
    PromptIr,
    Gfpgan,
    RestoreFormerPlus,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Led => "LED",
            Self::ZeroDce => "Zero-DCE",
            Self::ZeroDcePlus => "Zero-DCE++",
            Self::StableSr => "StableSR",
            Self::CodeFormer => "CodeFormer",
            Self::PromptIr => "PromptIR",
            Self::Gfpgan => "GFPGAN v1.4",
            Self::RestoreFormerPlus => "RestoreFormer++",
        }
    }
    pub fn licence(self) -> (&'static str, &'static str) {
        match self {
            Self::Led => ("CC BY-NC 4.0", "https://github.com/Srameo/LED#-license"),
            Self::ZeroDce => ("CC BY-NC 4.0", "https://github.com/Li-Chongyi/Zero-DCE#license"),
            Self::ZeroDcePlus => ("CC BY-NC 4.0", "https://github.com/Li-Chongyi/Zero-DCE_extension#license"),
            Self::StableSr => ("S-Lab 1.0", "https://github.com/IceClear/StableSR/blob/main/LICENSE.txt"),
            Self::CodeFormer => ("S-Lab 1.0", "https://github.com/sczhou/CodeFormer/blob/master/LICENSE"),
            Self::PromptIr => ("Academic Public License", "https://github.com/va1shn9v/PromptIR/blob/main/LICENSE.md"),
            Self::Gfpgan => ("Apache-2.0 + NVIDIA StyleGAN2 (non-commercial)", "https://github.com/TencentARC/GFPGAN/blob/master/LICENSE"),
            Self::RestoreFormerPlus => ("Apache-2.0 code / FFHQ training data", "https://github.com/wzhouxiff/RestoreFormerPlusPlus/blob/main/LICENSE"),
        }
    }
    fn face(self) -> bool {
        matches!(self, Self::CodeFormer | Self::Gfpgan | Self::RestoreFormerPlus)
    }
}

pub fn excluded(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("supir") || lower.contains("hypir")
}

/// Conservative name recognition: generic Epoch99/last.ckpt files need a named folder/node.
/// LEDNet and earlier GFPGAN/RestoreFormer versions are not the owner-approved BYO models.
pub fn recognise(text: &str) -> Option<Kind> {
    if excluded(text) {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    let n: String = lower.chars().filter(char::is_ascii_alphanumeric).collect();
    if n.contains("restoreformerplusplus") || n.contains("restoreformerpp") || lower.contains("restoreformer++") {
        Some(Kind::RestoreFormerPlus)
    } else if n.contains("gfpganv14") || n.contains("gfpgan14") {
        Some(Kind::Gfpgan)
    } else if n.contains("codeformer") {
        Some(Kind::CodeFormer)
    } else if n.contains("stablesr") {
        Some(Kind::StableSr)
    } else if n.contains("promptir") {
        Some(Kind::PromptIr)
    } else if n.contains("zerodce") {
        Some(if lower.contains("++") || n.contains("zerodcepp") || n.contains("zerodceplus") || n.contains("zerodceextension") {
            Kind::ZeroDcePlus
        } else {
            Kind::ZeroDce
        })
    } else if !n.contains("lednet") && lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|s| s == "led") {
        Some(Kind::Led)
    } else {
        None
    }
}

/// Used by every app download entry point, including generic catalogue cards.
pub fn external_only(text: &str) -> bool {
    excluded(text) || recognise(text).is_some()
}

#[derive(Clone, Debug)]
pub struct InstalledFile {
    pub folder: String,
    pub name: String,
}

/// Read-only folder discovery. Never follows symlinks or creates model directories.
pub fn scan(roots: &[PathBuf]) -> Vec<InstalledFile> {
    fn walk(dir: &Path, relative: &Path, folder: &str, depth: usize, out: &mut Vec<InstalledFile>) {
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let next = relative.join(entry.file_name());
            if kind.is_dir() {
                walk(&entry.path(), &next, folder, depth - 1, out);
            } else if kind.is_file() && matches!(entry.path().extension().and_then(|s| s.to_str()), Some("pth" | "pt" | "ckpt" | "safetensors")) {
                let name = next.to_string_lossy().replace('\\', "/");
                if !excluded(&name) {
                    out.push(InstalledFile { folder: folder.into(), name });
                }
            }
        }
    }
    let mut out = Vec::new();
    for root in roots {
        let mut folders: BTreeSet<String> =
            ["upscale_models", "facerestore_models", "stablesr", "checkpoints", "facedetection", "facedetection_models", "restoration_models"]
                .into_iter()
                .map(str::to_owned)
                .collect();
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type().is_ok_and(|t| t.is_dir()) && recognise(&name).is_some() {
                    folders.insert(name);
                }
            }
        }
        for folder in folders {
            walk(&root.join(&folder), Path::new(""), &folder, 4, &mut out);
        }
    }
    out
}

pub fn inventory(client: &ComfyClient, settings: &crate::AiSettings) -> Vec<InstalledFile> {
    // The known face node reads its auxiliaries from folder_paths.models_dir, not extra paths.
    // Do not mistake a file in the app's different model root for that dependency.
    let mut files = scan(&[settings.model_dir()]);
    files.retain(|f| !matches!(f.folder.as_str(), "facedetection" | "facedetection_models"));
    if matches!(settings.comfy_host.as_str(), "localhost" | "127.0.0.1" | "::1")
        && settings.host() == client.host()
        && let Ok(stats) = client.system_stats()
        && let Some(args) = stats["system"]["argv"].as_array()
    {
        let option = |key: &str| args.windows(2).find(|p| p[0] == key).and_then(|p| p[1].as_str()).map(PathBuf::from);
        let root = option("--models-directory")
            .or_else(|| option("--base-directory").map(|p| p.join("models")))
            .or_else(|| args.first().and_then(Value::as_str).and_then(|p| Path::new(p).parent()).map(|p| p.join("models")));
        if let Some(root) = root {
            files.extend(scan(&[root]));
        }
    }
    files
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Adapter {
    Core,
    Direct { node: String, file_input: String },
    Pair { loader: String, file_input: String, processor: String, model_input: String },
    StableSr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub kind: Kind,
    pub file: String,
    pub folder: String,
    pub adapter: Option<Adapter>,
    pub reason: String,
}
impl Model {
    pub fn identity(&self, host: &str) -> String {
        // Include policy/notice revision: an acknowledgement cannot silently survive changed terms.
        format!("v1:{host}:{:?}:{}:{}:{}:{NOTICE}", self.kind, self.folder, self.file, self.kind.licence().1)
    }
    pub fn label(&self) -> String {
        format!("{} · {}", self.kind.label(), self.file)
    }
}

fn required<'a>(info: &'a ObjectInfo, node: &str) -> Option<&'a serde_json::Map<String, Value>> {
    info.0.get(node)?.get("input")?.get("required")?.as_object()
}
fn output_image(info: &ObjectInfo, node: &str) -> Option<usize> {
    info.0[node]["output"].as_array()?.iter().position(|v| v == "IMAGE")
}
fn primitives(fields: &serde_json::Map<String, Value>, linked: &[&str]) -> Option<serde_json::Map<String, Value>> {
    let mut inputs = serde_json::Map::new();
    for (key, spec) in fields {
        if linked.contains(&key.as_str()) {
            continue;
        }
        let options = spec[0].as_array().or_else(|| if spec[0] == "COMBO" { spec[1]["options"].as_array() } else { None });
        let value = if let Some(options) = options {
            spec[1].get("default").cloned().or_else(|| options.first().cloned())?
        } else {
            match spec[0].as_str()? {
                "STRING" | "INT" | "FLOAT" | "BOOLEAN" => spec[1].get("default").cloned()?,
                _ => return None,
            }
        };
        // Never choose a community node's first-run model downloader.
        if value.as_str().is_some_and(|s| excluded(s) || s.starts_with("https://") || s.starts_with("http://"))
            || ((key.to_ascii_lowercase().contains("download") || key.to_ascii_lowercase().contains("install")) && value == true)
        {
            return None;
        }
        inputs.insert(key.clone(), value);
    }
    Some(inputs)
}

/// Combine advertised loader choices with folder-only discoveries. A file without a compatible
/// installed node is displayed with a reason, never sent to an invented node or installer.
pub fn detect(info: &ObjectInfo, files: &[InstalledFile]) -> Vec<Model> {
    let mut models = Vec::new();
    let empty = serde_json::Map::new();
    let nodes = info.0.as_object().unwrap_or(&empty);
    for (node, _) in nodes {
        if excluded(node) {
            continue;
        }
        let Some(fields) = required(info, node) else {
            continue;
        };
        for input in fields.keys() {
            let choices = info.choices(node, input);
            for file in choices.iter().map(String::as_str) {
                if excluded(file) || !matches!(Path::new(file).extension().and_then(|s| s.to_str()), Some("pth" | "pt" | "ckpt" | "safetensors")) {
                    continue;
                }
                let Some(kind) = recognise(file).or_else(|| recognise(&format!("{node}/{input}/{file}"))) else {
                    continue;
                };
                let folder = if node == "UpscaleModelLoader" {
                    "upscale_models"
                } else if node == "FaceRestoreModelLoader" {
                    "facerestore_models"
                } else if node == "ApplyStableSRUpscaler" {
                    "stablesr"
                } else {
                    "user models"
                };
                let mut adapter = None;
                if node == "UpscaleModelLoader"
                    && info.has_node("ImageUpscaleWithModel")
                    && info.0[node]["python_module"] == "comfy_extras.nodes_upscale_model"
                    && info.0["ImageUpscaleWithModel"]["python_module"] == "comfy_extras.nodes_upscale_model"
                    && !kind.face()
                    && kind != Kind::StableSr
                {
                    adapter = Some(Adapter::Core);
                } else if node == "ApplyStableSRUpscaler" && input == "stablesr_model" {
                    adapter = Some(Adapter::StableSr);
                } else if output_image(info, node).is_some() {
                    if let Some(image) = fields.iter().find(|(_, s)| s[0] == "IMAGE").map(|(k, _)| k.as_str())
                        && primitives(fields, &[input, image]).is_some()
                    {
                        adapter = Some(Adapter::Direct { node: node.clone(), file_input: input.clone() });
                    }
                } else if let Some(model_type) = info.0[node]["output"][0].as_str() {
                    for (processor, _) in nodes {
                        if excluded(processor) || output_image(info, processor).is_none() {
                            continue;
                        }
                        let Some(p) = required(info, processor) else {
                            continue;
                        };
                        let image = p.iter().find(|(_, s)| s[0] == "IMAGE").map(|(k, _)| k.as_str());
                        let model = p.iter().find(|(_, s)| s[0] == model_type).map(|(k, _)| k.as_str());
                        if let (Some(image), Some(model)) = (image, model)
                            && primitives(fields, &[input]).is_some()
                            && primitives(p, &[image, model]).is_some()
                        {
                            adapter = Some(Adapter::Pair {
                                loader: node.clone(),
                                file_input: input.clone(),
                                processor: processor.clone(),
                                model_input: model.into(),
                            });
                            break;
                        }
                    }
                }
                let mut reason = String::new();
                // Core and the two known loaders enumerate actual installed files. An unknown
                // plugin can instead advertise a hardcoded download catalogue: require a local
                // folder discovery too before enabling its reflected workflow.
                let known_loader = matches!(node.as_str(), "UpscaleModelLoader" | "FaceRestoreModelLoader" | "ApplyStableSRUpscaler");
                if !known_loader && !files.iter().any(|f| f.name == file) {
                    continue;
                }
                if kind.face() {
                    let auxiliary = ["detection_Resnet50_Final.pth", "parsing_parsenet.pth"]
                        .iter()
                        .all(|name| files.iter().any(|f| matches!(f.folder.as_str(), "facedetection" | "facedetection_models") && f.name == *name));
                    let supported = matches!(&adapter, Some(Adapter::Pair { loader, processor, .. }) if loader == "FaceRestoreModelLoader" && matches!(processor.as_str(), "FaceRestoreCFWithModel" | "FaceRestoreWithModel"));
                    if supported && (!auxiliary || kind == Kind::RestoreFormerPlus) {
                        adapter = None;
                        reason =
                            "A compatible face-restoration node and preinstalled detector/parser weights are required. No automatic downloads are allowed."
                                .into();
                    }
                }
                if adapter.is_none() && reason.is_empty() {
                    reason = "No compatible image-restoration node is installed for this model.".into();
                }
                // Unknown loaders are admitted through local discovery, so use that discovery's
                // folder identity rather than creating a second synthetic "user models" entry.
                let folders: Vec<_> = if known_loader { vec![folder] } else { files.iter().filter(|f| f.name == file).map(|f| f.folder.as_str()).collect() };
                for folder in folders {
                    models.push(Model { kind, file: file.into(), folder: folder.into(), adapter: adapter.clone(), reason: reason.clone() });
                }
            }
        }
    }
    for file in files {
        if excluded(&file.name) {
            continue;
        }
        let Some(kind) = recognise(&format!("{}/{}", file.folder, file.name)) else {
            continue;
        };
        if !models.iter().any(|m| m.kind == kind && m.file == file.name && m.folder == file.folder) {
            models.push(Model {
                kind,
                file: file.name.clone(),
                folder: file.folder.clone(),
                adapter: None,
                reason: "The file is installed, but ComfyUI does not advertise a compatible loader.".into(),
            });
        }
    }
    models.sort_by_key(|m| (m.kind.label(), m.folder.clone(), m.file.clone(), m.adapter.is_none()));
    models.dedup_by(|a, b| a.kind == b.kind && a.folder == b.folder && a.file == b.file);
    models
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Acknowledgements {
    #[serde(default)]
    ids: BTreeSet<String>,
}
impl Acknowledgements {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn contains(&self, model: &Model, host: &str) -> bool {
        self.ids.contains(&model.identity(host))
    }
    pub fn acknowledge(&mut self, model: &Model, host: &str, path: &Path) -> Result<()> {
        let mut next = self.clone();
        next.ids.insert(model.identity(host));
        crate::settings::write_atomic(path, &serde_json::to_vec_pretty(&next)?)?;
        *self = next;
        Ok(())
    }
    pub fn path() -> PathBuf {
        crate::settings::state_dir().join("restoration-byo-acknowledgements.json")
    }
}

pub fn graph(model: &Model, host: &str, acks: &Acknowledgements, info: &ObjectInfo, files: &[InstalledFile], checkpoint: &str) -> Result<Value> {
    if excluded(&model.file) || recognise(&format!("{}/{}", model.kind.label(), model.file)) != Some(model.kind) {
        bail!("This model is not offered in Restoration");
    }
    if !acks.contains(model, host) {
        bail!("Acknowledge this model's licence before previewing");
    }
    let current = detect(info, files);
    let adapter = model.adapter.as_ref().context("No compatible node is installed")?;
    let forbidden_node = match adapter {
        Adapter::Direct { node, .. } => excluded(node),
        Adapter::Pair { loader, processor, .. } => excluded(loader) || excluded(processor),
        Adapter::Core | Adapter::StableSr => false,
    };
    if forbidden_node || excluded(&model.folder) {
        bail!("This model is not offered in Restoration");
    }
    if !current.iter().any(|m| m.kind == model.kind && m.file == model.file && m.adapter.as_ref() == Some(adapter)) {
        bail!("Installed model or node changed; detect models again");
    }
    for node in ["LoadImage", "SaveImage"] {
        if !info.has_node(node) {
            bail!("ComfyUI needs {node}");
        }
    }
    let mut graph = json!({"1":{"class_type":"LoadImage","inputs":{"image":""}}});
    let image_output = match adapter {
        Adapter::Core => {
            graph["2"] = json!({"class_type":"UpscaleModelLoader","inputs":{"model_name":model.file}});
            graph["3"] = json!({"class_type":"ImageUpscaleWithModel","inputs":{"upscale_model":["2",0],"image":["1",0]}});
            json!(["3", 0])
        }
        Adapter::Direct { node, file_input } => {
            let fields = required(info, node).context("Node changed")?;
            let image = fields.iter().find(|(_, s)| s[0] == "IMAGE").context("Node has no image input")?.0;
            let mut inputs = primitives(fields, &[file_input, image]).context("Unsupported node inputs")?;
            inputs.insert(file_input.clone(), json!(model.file));
            inputs.insert(image.clone(), json!(["1", 0]));
            graph["2"] = json!({"class_type":node,"inputs":inputs});
            json!(["2", output_image(info, node).context("Node has no image output")?])
        }
        Adapter::Pair { loader, file_input, processor, model_input } => {
            if excluded(loader) || excluded(processor) || !info.choices(loader, file_input).contains(&model.file) {
                bail!("Installed model or node changed");
            }
            let mut load = primitives(required(info, loader).context("Loader changed")?, &[file_input]).context("Unsupported loader inputs")?;
            load.insert(file_input.clone(), json!(model.file));
            let fields = required(info, processor).context("Processor changed")?;
            let image = fields.iter().find(|(_, s)| s[0] == "IMAGE").context("Missing image input")?.0;
            let mut inputs = primitives(fields, &[image, model_input]).context("Unsupported processor inputs")?;
            inputs.insert(image.clone(), json!(["1", 0]));
            inputs.insert(model_input.clone(), json!(["2", 0]));
            if model.kind.face() && fields.contains_key("facedetection") {
                inputs.insert("facedetection".into(), json!("retinaface_resnet50"));
            }
            graph["2"] = json!({"class_type":loader,"inputs":load});
            graph["3"] = json!({"class_type":processor,"inputs":inputs});
            json!(["3", output_image(info, processor).context("No image output")?])
        }
        Adapter::StableSr => {
            if excluded(checkpoint) || !info.choices("CheckpointLoaderSimple", "ckpt_name").iter().any(|s| s == checkpoint) {
                bail!("Choose an installed StableSR-compatible diffusion checkpoint");
            }
            for node in ["CheckpointLoaderSimple", "VAEEncode", "CLIPTextEncode", "KSampler", "VAEDecode"] {
                if !info.has_node(node) {
                    bail!("StableSR needs the installed {node} node");
                }
            }
            graph["2"] = json!({"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":checkpoint}});
            graph["3"] = json!({"class_type":"VAEEncode","inputs":{"pixels":["1",0],"vae":["2",2]}});
            graph["4"] = json!({"class_type":"ApplyStableSRUpscaler","inputs":{"model":["2",0],"stablesr_model":model.file,"latent_image":["3",0]}});
            // Sampling/conditioning follow the installed plugin's upstream non-turbo example.
            graph["5"] = json!({"class_type":"CLIPTextEncode","inputs":{"text":"(masterpiece), (best quality), (realistic),(very clear)","clip":["2",1]}});
            graph["8"] = json!({"class_type":"CLIPTextEncode","inputs":{"text":"3d, cartoon, anime, sketches, (worst quality), (low quality)","clip":["2",1]}});
            graph["6"] = json!({"class_type":"KSampler","inputs":{"model":["4",0],"positive":["5",0],"negative":["8",0],"latent_image":["3",0],"seed":0,"steps":20,"cfg":8.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0}});
            graph["7"] = json!({"class_type":"VAEDecode","inputs":{"samples":["6",0],"vae":["2",2]}});
            json!(["7", 0])
        }
    };
    graph["9"] = json!({"class_type":"SaveImage","inputs":{"images":image_output,"filename_prefix":"local-image-byo-restoration"}});
    Ok(graph)
}

pub fn restore(
    client: &ComfyClient,
    settings: &crate::AiSettings,
    model: &Model,
    acks: &Acknowledgements,
    source: &RgbaImage,
    checkpoint: &str,
    ctl: &JobControl,
) -> Result<RgbaImage> {
    ctl.check()?;
    if !acks.contains(model, client.host()) {
        bail!("Acknowledge this model's licence before previewing");
    }
    // Recheck availability, including face auxiliaries, immediately before submission.
    let info = client.object_info()?;
    let files = inventory(client, settings);
    if !detect(&info, &files).iter().any(|m| m == model && m.adapter.is_some()) {
        bail!("Installed model or node changed; detect models again");
    }
    let graph = graph(model, client.host(), acks, &info, &files, checkpoint)?;
    let png = crate::imaging::encode_png(source)?;
    let bytes = client.run_when_idle(graph, &[("1".into(), png)], ctl)?;
    ctl.check()?;
    let result = image::load_from_memory(&bytes)?.to_rgba8();
    // StableSR may round odd dimensions; BYO upscalers may enlarge. Keep the dialog/document size.
    let mut result = if result.dimensions() == source.dimensions() {
        result
    } else {
        li_raster::resize_rgba8(&result, source.width(), source.height(), li_raster::Filter::Lanczos3)
    };
    for (a, b) in result.pixels_mut().zip(source.pixels()) {
        a[3] = b[3];
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> ObjectInfo {
        let mut info = ObjectInfo(crate::mock::object_info());
        info.0["UpscaleModelLoader"]["input"]["required"]["model_name"] =
            json!([["LED_model.pth", "Zero-DCE.pth", "Zero-DCE++.pth", "PromptIR.pth", "SUPIR_LED.pth", "HYPIR-PromptIR.pth"]]);
        info.0["FaceRestoreModelLoader"] =
            json!({"input":{"required":{"model_name":[["codeformer.pth","GFPGANv1.4.pth","RestoreFormer++.ckpt"]]}},"output":["FACERESTORE_MODEL"]});
        info.0["FaceRestoreCFWithModel"] = json!({"input":{"required":{"facerestore_model":["FACERESTORE_MODEL"],"image":["IMAGE"],"facedetection":[["retinaface_resnet50"]],"codeformer_fidelity":["FLOAT",{"default":0.5}]}},"output":["IMAGE"]});
        info.0["ApplyStableSRUpscaler"] = json!({"input":{"required":{"model":["MODEL"],"stablesr_model":[["stablesr_000117.ckpt"]]}},"output":["MODEL"]});
        info.0["RestoreFormerPlusPlus"] = json!({"input":{"required":{"model_name":[["RestoreFormer++.ckpt"]],"image":["IMAGE"]}},"output":["IMAGE"]});
        info
    }
    fn aux() -> Vec<InstalledFile> {
        let mut files: Vec<_> = ["detection_Resnet50_Final.pth", "parsing_parsenet.pth"]
            .iter()
            .map(|n| InstalledFile { folder: "facedetection".into(), name: (*n).into() })
            .collect();
        files.push(InstalledFile { folder: "facerestore_models".into(), name: "RestoreFormer++.ckpt".into() });
        files
    }
    fn temp() -> PathBuf {
        std::env::temp_dir().join(format!("li-byo-{}.json", uuid::Uuid::new_v4()))
    }

    #[test]
    fn restoration_byo_detects_all_approved_kinds_and_excludes_suppixel() {
        let models = detect(&info(), &aux());
        assert_eq!(models.len(), 8, "{models:?}");
        assert!(models.iter().all(|m| m.adapter.is_some() && !excluded(&m.file)));
        assert!(models.iter().all(|m| !m.kind.licence().0.is_empty() && m.kind.licence().1.starts_with("https://")));
        let mut modern = info();
        let options = modern.0["UpscaleModelLoader"]["input"]["required"]["model_name"][0].clone();
        modern.0["UpscaleModelLoader"]["input"]["required"]["model_name"] = json!(["COMBO", {"options":options}]);
        assert_eq!(detect(&modern, &aux()), models, "V3 combo metadata is supported");
        assert!(detect(&info(), &[]).iter().filter(|m| matches!(m.kind, Kind::CodeFormer | Kind::Gfpgan)).all(|m| m.adapter.is_none()));
        assert!(detect(&info(), &[]).iter().find(|m| m.kind == Kind::RestoreFormerPlus).unwrap().adapter.is_none());
        let mut download_node = info();
        download_node.0["RestoreFormerPlusPlus"]["input"]["required"]["auto_download"] = json!(["BOOLEAN", {"default":true}]);
        assert!(detect(&download_node, &aux()).iter().find(|m| m.kind == Kind::RestoreFormerPlus).unwrap().adapter.is_none());
        let mut catalogue = info();
        catalogue.0["RestoreFormerPlusPlus"]["input"]["required"]["version"] = json!([["RestoreFormer++"]]);
        assert_eq!(detect(&catalogue, &aux()).len(), 8, "only file choices count, not catalogue/version options");
        let hardcoded_catalogue = ObjectInfo(json!({"CommunityPromptIR": {
            "input":{"required":{"model_name":[["PromptIR.pth"]],"image":["IMAGE"]}},"output":["IMAGE"]
        }}));
        assert!(detect(&hardcoded_catalogue, &[]).is_empty(), "a remote catalogue is not an installed model");
        assert!(recognise("GFPGANv1.3.pth").is_none());
        assert!(recognise("4x_LEDNet.pth").is_none());
        assert!(recognise("Epoch99.pth").is_none());
        let files = [
            InstalledFile { folder: "upscale_models".into(), name: "LED/offline.pth".into() },
            InstalledFile { folder: "stablesr".into(), name: "SUPIR_stablesr.ckpt".into() },
        ];
        let offline = detect(&ObjectInfo::default(), &files);
        assert_eq!(offline.len(), 1);
        assert!(offline[0].adapter.is_none());
        let dir = temp().with_extension("models");
        std::fs::create_dir_all(dir.join("PromptIR")).unwrap();
        std::fs::write(dir.join("PromptIR/Epoch99.pth"), b"mock user-installed model").unwrap();
        std::fs::write(dir.join("PromptIR/SUPIR.pth"), b"excluded").unwrap();
        let folder_models = detect(&ObjectInfo::default(), &scan(std::slice::from_ref(&dir)));
        assert_eq!(folder_models.len(), 1);
        assert_eq!(folder_models[0].kind, Kind::PromptIr);
        assert!(folder_models[0].adapter.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restoration_byo_acknowledgement_is_persisted_per_model_and_host() {
        let info = info();
        let models = detect(&info, &aux());
        let path = temp();
        let mut acks = Acknowledgements::load(&path);
        let model = &models[0];
        assert!(graph(model, "host", &acks, &info, &aux(), "").unwrap_err().to_string().contains("Acknowledge"));
        acks.acknowledge(model, "host", &path).unwrap();
        let loaded = Acknowledgements::load(&path);
        assert!(loaded.contains(model, "host"));
        assert!(!loaded.contains(model, "another-host"));
        assert!(!loaded.contains(&models[1], "host"));
        for model in &models {
            acks.acknowledge(model, "host", &path).unwrap();
            let checkpoint = info.choices("CheckpointLoaderSimple", "ckpt_name")[0].clone();
            let graph = graph(model, "host", &acks, &info, &aux(), &checkpoint).unwrap();
            assert_eq!(graph["1"]["class_type"], "LoadImage");
            assert_eq!(graph["9"]["class_type"], "SaveImage");
            assert!(graph.as_object().unwrap().values().all(|n| !excluded(n["class_type"].as_str().unwrap())));
        }
        let mut forbidden = model.clone();
        forbidden.file = "SUPIR_LED.pth".into();
        acks.acknowledge(&forbidden, "host", &path).unwrap();
        assert!(graph(&forbidden, "host", &acks, &info, &aux(), "").is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn restoration_byo_download_entry_points_refuse_all_user_only_models() {
        let cfg = crate::browser::Config::default();
        let net = crate::browser::HttpNet::new(cfg.clone());
        for name in [
            "LED.pth",
            "Zero-DCE.safetensors",
            "Zero-DCE++.pth",
            "StableSR.ckpt",
            "codeformer.pth",
            "PromptIR.safetensors",
            "GFPGANv1.4.pth",
            "RestoreFormer++.ckpt",
            "SUPIR.safetensors",
            "HYPIR.safetensors",
        ] {
            let url = format!("https://huggingface.co/user/model/resolve/main/{name}");
            let f = crate::browser::CatalogFile {
                name: name.into(),
                folder: "upscale_models".into(),
                url: url.clone(),
                bytes: Some(4),
                sha256: Some("0".repeat(64)),
                ..Default::default()
            };
            let item = crate::browser::Item { id: name.into(), files: vec![f.clone()], ..Default::default() };
            assert!(crate::browser::plan(&item, &[f], None, &ObjectInfo::default(), &cfg, &net).is_err());
            let plan = crate::browser::Plan {
                files: vec![crate::browser::PlannedFile { name: name.into(), folder: "upscale_models".into(), url, bytes: 4, sha256: "0".repeat(64) }],
                ..Default::default()
            };
            assert!(crate::browser::install(&plan, Path::new("unused"), &cfg, &JobControl::new(), &|_, _, _| {}).is_err());
        }
    }

    #[test]
    fn restoration_byo_mock_run_is_gated_before_submission() {
        let server = crate::mock::MockComfy::start().unwrap();
        let client = ComfyClient::new(server.host());
        let dir = temp().with_extension("");
        std::fs::create_dir_all(dir.join("upscale_models")).unwrap();
        std::fs::write(dir.join("upscale_models/LED_model.safetensors"), b"mock user-installed weights").unwrap();
        server.set_model_dir(&dir);
        let mut settings = crate::AiSettings::load();
        settings.model_directory = dir.to_string_lossy().into();
        let info = client.object_info().unwrap();
        let model = detect(&info, &inventory(&client, &settings)).remove(0);
        let source = RgbaImage::from_pixel(20, 13, image::Rgba([30, 60, 90, 123]));
        let acks = Acknowledgements::default();
        assert!(restore(&client, &settings, &model, &acks, &source, "", &JobControl::new()).unwrap_err().to_string().contains("Acknowledge"));
        assert!(server.prompts().is_empty());
        let mut acks = acks;
        let path = temp();
        acks.acknowledge(&model, client.host(), &path).unwrap();
        let output = restore(&client, &settings, &model, &acks, &source, "", &JobControl::new()).unwrap();
        assert_eq!(output.dimensions(), source.dimensions());
        assert!(output.pixels().zip(source.pixels()).all(|(a, b)| a[3] == b[3]));
        assert_eq!(server.prompts().len(), 1);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
#[test]
fn ds2_byo_same_name_in_different_folders_remains_visible() {
    let files = [
        InstalledFile { folder: "folder-a".into(), name: "CodeFormer.pth".into() },
        InstalledFile { folder: "folder-b".into(), name: "CodeFormer.pth".into() },
    ];
    let models = detect(&ObjectInfo::default(), &files);
    assert_eq!(models.len(), 2);
    assert_ne!(models[0].identity("host"), models[1].identity("host"));
}
