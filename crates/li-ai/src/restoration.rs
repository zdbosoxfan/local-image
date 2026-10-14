//! Pinned, non-generative restoration through ComfyUI's two core upscale nodes.
//!
//! The embedded profile is the security boundary, independent of downloaded/user profiles.
//! A filename, an online catalogue licence, or a modified family never authorises new weights.
//! The configured local ComfyUI interpreter must recognise the verified weights with main
//! spandrel alone. DDColor currently lives in extra arches and is deliberately refused.

use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::comfy::{ComfyClient, JobControl, ObjectInfo};
use crate::family::{Family, FileSpec};
use crate::settings::AiSettings;

pub mod byo;

pub const PROFILE: &str = include_str!("../families/restoration.json");
pub const NODES: &[&str] = &["LoadImage", "UpscaleModelLoader", "ImageUpscaleWithModel", "SaveImage"];
pub const COLOURISE_UNAVAILABLE: &str = "Colourise requires DDColor support in main spandrel. Local Image does not install architecture packages.";
pub const OLD_PHOTO_UNAVAILABLE: &str = "Old-photo scratch detection is unavailable: Microsoft's PyTorch detector needs a verified Rust-compatible model and is outside ComfyUI core and main spandrel.";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Task {
    #[default]
    Jpeg,
    Scan,
    Denoise,
    Colourise,
    OldPhoto,
}

impl Task {
    pub const ALL: [Self; 5] = [Self::Jpeg, Self::Scan, Self::Denoise, Self::Colourise, Self::OldPhoto];
    pub fn label(self) -> &'static str {
        match self {
            Self::Jpeg => "JPEG cleanup",
            Self::Scan => "Scan cleanup",
            Self::Denoise => "Denoise",
            Self::Colourise => "Colourise",
            Self::OldPhoto => "Old-photo preset",
        }
    }
    pub fn model(self) -> Result<&'static crate::family::ModelDef> {
        let key = match self {
            Self::Jpeg => "restore-jpeg",
            Self::Scan => "restore-scan",
            Self::Denoise => "restore-denoise",
            Self::Colourise => "restore-colourise",
            Self::OldPhoto => bail!(OLD_PHOTO_UNAVAILABLE),
        };
        policy().models.iter().find(|m| m.key == key).context("Missing embedded restoration model")
    }
    pub fn file(self) -> Result<&'static FileSpec> {
        self.model()?.presets.first().and_then(|p| p.files.first()).context("Missing restoration pin")
    }
    pub fn architecture(self) -> Result<&'static str> {
        match self {
            Self::Jpeg => Ok("FBCNN"),
            Self::Scan => Ok("SCUNet"),
            Self::Denoise => Ok("NAFNet"),
            Self::Colourise => Ok("DDColor"),
            Self::OldPhoto => bail!(OLD_PHOTO_UNAVAILABLE),
        }
    }
}

/// Always built-in: remote and user profiles cannot widen this allowlist.
pub fn policy() -> &'static Family {
    static FAMILY: OnceLock<Family> = OnceLock::new();
    FAMILY.get_or_init(|| serde_json::from_str(PROFILE).expect("validated embedded restoration profile"))
}

pub fn pinned_file(name: &str) -> Option<&'static FileSpec> {
    policy().models.iter().flat_map(|m| &m.presets).flat_map(|p| &p.files).find(|f| f.name == name)
}

/// Exception to the browser's safetensors/GGUF rule, scoped to these exact pinned PTH files.
pub fn validate_download(name: &str, folder: &str, url: &str, bytes: u64, sha256: &str) -> Result<()> {
    let pin = pinned_file(name).context("Restoration model is not in the built-in allowlist")?;
    if folder != pin.folder || url != pin.url || bytes != pin.bytes || sha256 != pin.sha256 {
        bail!("Restoration download differs from the built-in size, SHA-256, folder or official source");
    }
    Ok(())
}

/// No fuzzy basename matching: ComfyUI must list the same root file we verified locally.
pub fn graph(task: Task, info: &ObjectInfo) -> Result<Value> {
    let file = task.file()?;
    for node in NODES {
        if !info.has_node(node) {
            bail!("Update ComfyUI: restoration needs the core {node} node");
        }
        let expected_module = if matches!(*node, "LoadImage" | "SaveImage") { "nodes" } else { "comfy_extras.nodes_upscale_model" };
        if info.0[*node]["python_module"].as_str() != Some(expected_module) {
            bail!("Restoration refuses {node}: it is not reported as a ComfyUI core node");
        }
    }
    if !info.choices("UpscaleModelLoader", "model_name").iter().any(|n| n == &file.name) {
        bail!("Install {} through the Restoration model browser, then refresh ComfyUI", task.model()?.label);
    }
    Ok(json!({
        "1": {"class_type": "LoadImage", "inputs": {"image": ""}},
        "2": {"class_type": "UpscaleModelLoader", "inputs": {"model_name": file.name}},
        "3": {"class_type": "ImageUpscaleWithModel", "inputs": {"upscale_model": ["2", 0], "image": ["1", 0]}},
        "4": {"class_type": "SaveImage", "inputs": {"images": ["3", 0], "filename_prefix": "local-image-restoration"}}
    }))
}

#[derive(Debug, Deserialize)]
struct Detection {
    architecture: String,
    main: Vec<String>,
    extra: bool,
    image: bool,
    scale: u32,
    input_channels: u32,
    output_channels: u32,
}

fn validate_detection(task: Task, detection: &Detection) -> Result<()> {
    let expected = task.architecture()?;
    if !detection.main.iter().any(|id| id == &detection.architecture) || detection.architecture != expected {
        bail!(
            "Restoration architecture {} is not the allowed main-spandrel architecture {expected} (user extra arches installed: {})",
            detection.architecture,
            detection.extra
        );
    }
    if !detection.image || detection.scale != 1 || detection.input_channels != 3 || detection.output_channels != 3 {
        bail!("Restoration requires a 1× RGB single-image model");
    }
    Ok(())
}

fn probe_output(command: &mut std::process::Command, ctl: &JobControl, timeout: std::time::Duration) -> Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child =
        command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("Cannot inspect main spandrel in the configured ComfyUI interpreter")?;
    let stdout = child.stdout.take().context("Missing probe stdout")?;
    let stderr = child.stderr.take().context("Missing probe stderr")?;
    std::thread::scope(|scope| {
        let read = |stream: Box<dyn Read + Send>| -> std::io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stream.take(1 << 20).read_to_end(&mut bytes)?;
            Ok(bytes)
        };
        let out = scope.spawn(move || read(Box::new(stdout)));
        let err = scope.spawn(move || read(Box::new(stderr)));
        let start = std::time::Instant::now();
        let status = (|| -> Result<std::process::ExitStatus> {
            loop {
                ctl.check()?;
                if start.elapsed() >= timeout {
                    bail!("Restoration architecture inspection timed out");
                }
                if let Some(status) = child.try_wait()? {
                    return Ok(status);
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        })();
        if status.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let stdout = out.join().map_err(|_| anyhow::anyhow!("Probe stdout reader failed"))??;
        let stderr = err.join().map_err(|_| anyhow::anyhow!("Probe stderr reader failed"))??;
        Ok(std::process::Output { status: status?, stdout, stderr })
    })
}

/// Read-only inspection with the selected ComfyUI's own interpreter/package. It never imports
/// extra arches or custom nodes. PyTorch's weights-only loader reads only the already verified
/// publisher checkpoint; all product orchestration and graph building stay in Rust.
fn detect(task: Task, installation: &crate::setup::Installation, model: &Path, argv: &Value, ctl: &JobControl) -> Result<()> {
    let main = argv.get(0).and_then(Value::as_str).context("Update ComfyUI: restoration needs its server arguments to verify model paths")?;
    if std::fs::canonicalize(main)? != std::fs::canonicalize(installation.code.join("main.py"))? {
        bail!("The running ComfyUI differs from the configured installation; restoration cannot verify its model architecture");
    }
    // Cache only architecture recognition. Every request still verifies the full model bytes,
    // resolves the backend's actual path, checks extra-arches presence and fingerprints package
    // source. Re-importing PyTorch and constructing the same CPU model dominated warm previews.
    static DETECTED: OnceLock<std::sync::Mutex<std::collections::BTreeMap<String, Value>>> = OnceLock::new();
    let cache_key = format!("{}:{}:{}:{}:{}", task.architecture()?, task.file()?.sha256, model.display(), installation.python.display(), argv);
    let cache = DETECTED.get_or_init(Default::default);
    let cached = cache.lock().ok().and_then(|c| c.get(&cache_key).cloned()).unwrap_or(Value::Null);
    const PROBE: &str = r#"
import sys, json, importlib.util, os, itertools, hashlib
expected = os.path.realpath(sys.argv[1])
server_argv = json.loads(sys.argv[2])
cached = json.loads(sys.argv[3])
code = os.path.dirname(os.path.realpath(server_argv[0]))
sys.path.insert(0, code)
sys.argv = server_argv
import comfy.options
comfy.options.enable_args_parsing()
from comfy.cli_args import args
import folder_paths, utils.extra_config
automatic = os.path.join(code, 'extra_model_paths.yaml')
if os.path.isfile(automatic):
    utils.extra_config.load_extra_path_config(automatic)
if args.extra_model_paths_config:
    for config in itertools.chain(*args.extra_model_paths_config):
        utils.extra_config.load_extra_path_config(config)
resolved = os.path.realpath(folder_paths.get_full_path_or_raise('upscale_models', os.path.basename(expected)))
if resolved != expected:
    raise RuntimeError('ComfyUI would load another copy of the restoration model. Remove the conflicting copy or use the app model path.')
extra = importlib.util.find_spec('spandrel_extra_arches') is not None
spec = importlib.util.find_spec('spandrel')
if spec is None:
    raise RuntimeError('Main spandrel is not installed')
root = os.path.dirname(spec.origin)
fingerprint = hashlib.sha256(root.encode())
fingerprint.update(str(extra).encode())
for directory, subdirs, files in os.walk(root):
    subdirs.sort()
    for file in sorted(files):
        if file.endswith('.py'):
            path = os.path.join(directory, file)
            fingerprint.update(path.encode())
            with open(path, 'rb') as source:
                fingerprint.update(source.read())
signature = fingerprint.hexdigest()
if cached and cached.get('signature') == signature:
    print(json.dumps(cached))
    sys.exit(0)
import torch, spandrel
torch.set_num_threads(3)
main = [str(a.architecture.id) for a in spandrel.MAIN_REGISTRY]
state = torch.load(expected, map_location='cpu', weights_only=True)
model = spandrel.ModelLoader().load_from_state_dict(state)
print(json.dumps(dict(signature=signature, extra=extra, architecture=str(model.architecture.id), main=main,
    image=isinstance(model, spandrel.ImageModelDescriptor), scale=model.scale,
    input_channels=model.input_channels, output_channels=model.output_channels)))
"#;
    let mut command = std::process::Command::new(&installation.python);
    command
        .args(["-B", "-I", "-c", PROBE])
        .arg(model)
        .arg(serde_json::to_string(argv)?)
        .arg(serde_json::to_string(&cached)?)
        .current_dir(&installation.code)
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONHOME");
    let output = probe_output(&mut command, ctl, std::time::Duration::from_secs(120))?;
    if !output.status.success() {
        bail!("Main spandrel could not recognise the verified restoration model: {}", String::from_utf8_lossy(&output.stderr));
    }
    let evidence: Value = serde_json::from_slice(&output.stdout).context("Invalid spandrel architecture response")?;
    let detection: Detection = serde_json::from_value(evidence.clone())?;
    validate_detection(task, &detection)?;
    if let Ok(mut c) = cache.lock() {
        c.insert(cache_key, evidence);
    }
    Ok(())
}

/// Verify the actual local bytes before any load or network request; symlinks are refused.
fn verified_path(task: Task, dir: &Path) -> Result<PathBuf> {
    let spec = task.file()?;
    let path = crate::download::target_path(dir, spec);
    let meta = std::fs::symlink_metadata(&path).with_context(|| format!("Install {} through the Restoration model browser", task.model().unwrap().label))?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        bail!("Restoration model must be a regular file in the app's model directory");
    }
    if !crate::download::verify_existing(&path, spec)? {
        bail!("Restoration model is missing");
    }
    Ok(path)
}

/// The real app path: pin verification, main-only architecture detection, core graph, ComfyUI
/// upload/admission/progress/cancel/download, dimension check and original alpha restoration.
pub fn restore(client: &ComfyClient, settings: &AiSettings, task: Task, source: &RgbaImage, ctl: &JobControl) -> Result<RgbaImage> {
    ctl.check()?;
    if source.width() == 0 || source.height() == 0 {
        bail!("Restoration needs a non-empty image");
    }
    let path = verified_path(task, &settings.model_dir())?;
    if settings.host() != client.host() || !matches!(settings.comfy_host.as_str(), "127.0.0.1" | "localhost" | "::1") {
        bail!("Restoration needs the configured local ComfyUI so its main spandrel architecture can be verified");
    }
    let installation = crate::setup::installation(Path::new(&settings.comfy_directory), Some(Path::new(&settings.comfy_python)))
        .context("Choose the local ComfyUI folder and interpreter in Local AI settings to verify restoration architectures")?;
    let stats = client.system_stats()?;
    detect(task, &installation, &path, &stats["system"]["argv"], ctl)?;
    ctl.check()?;
    execute_checked(client, task, source, ctl, &|| {
        // Repeat both pin and backend-path checks after uploads, before prompt submission.
        let path = verified_path(task, &settings.model_dir())?;
        detect(task, &installation, &path, &stats["system"]["argv"], ctl)?;
        verified_path(task, &settings.model_dir())?;
        Ok(())
    })
}

#[cfg(test)]
fn execute(client: &ComfyClient, task: Task, source: &RgbaImage, ctl: &JobControl) -> Result<RgbaImage> {
    execute_checked(client, task, source, ctl, &|| Ok(()))
}

fn execute_checked(client: &ComfyClient, task: Task, source: &RgbaImage, ctl: &JobControl, check: &dyn Fn() -> Result<()>) -> Result<RgbaImage> {
    let graph = graph(task, &client.object_info()?)?;
    let png = crate::imaging::encode_png(source)?;
    let bytes = client.run_when_idle_checked(graph, &[("1".into(), png)], ctl, check)?;
    ctl.check()?;
    let mut result = image::load_from_memory(&bytes)?.to_rgba8();
    if result.dimensions() != source.dimensions() {
        bail!("Restoration returned unexpected dimensions; the source image was not changed");
    }
    result.as_mut().par_chunks_exact_mut(4).zip(source.as_raw().par_chunks_exact(4)).for_each(|(after, before)| {
        after[3] = before[3];
    });
    Ok(result)
}

#[cfg(test)]
mod tests {

    #[test]
    fn ds2_probe_honours_timeout_and_cancellation() {
        const CHILD: &str = "LI_DS2_PROBE_CHILD";
        if std::env::var_os(CHILD).is_some() {
            std::thread::sleep(std::time::Duration::from_secs(20));
            return;
        }
        for cancelled in [true, false] {
            let ctl = JobControl::new();
            if cancelled {
                ctl.cancel();
            }
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command.args(["--exact", "restoration::tests::ds2_probe_honours_timeout_and_cancellation"]).env(CHILD, "1");
            let error = probe_output(&mut command, &ctl, std::time::Duration::from_millis(75)).unwrap_err();
            if cancelled {
                assert!(error.is::<crate::Cancelled>());
            } else {
                assert!(error.to_string().contains("timed out"));
            }
        }
    }

    #[test]
    fn ds2_admission_rejects_changed_models_before_submission() {
        let server = crate::mock::MockComfy::start().unwrap();
        let client = ComfyClient::new(server.host());
        let image = RgbaImage::new(8, 8);
        let error = execute_checked(&client, Task::Jpeg, &image, &JobControl::new(), &|| anyhow::bail!("model changed")).unwrap_err();
        assert!(error.to_string().contains("model changed"));
        assert!(server.prompts().is_empty());
    }

    use super::*;
    use image::Rgba;

    fn info() -> ObjectInfo {
        ObjectInfo(crate::mock::object_info())
    }

    #[test]
    fn restoration_graph_per_task_uses_only_four_core_nodes() {
        for task in [Task::Jpeg, Task::Scan, Task::Denoise, Task::Colourise] {
            let graph = graph(task, &info()).unwrap();
            assert_eq!(graph.as_object().unwrap().len(), 4);
            assert_eq!(graph["2"]["inputs"]["model_name"], task.file().unwrap().name);
            assert_eq!(graph["3"]["inputs"]["image"], json!(["1", 0]));
            assert_eq!(graph["3"]["inputs"]["upscale_model"], json!(["2", 0]));
            assert_eq!(graph["4"]["inputs"]["images"], json!(["3", 0]));
            assert!(graph.as_object().unwrap().values().all(|n| NODES.contains(&n["class_type"].as_str().unwrap())));
            assert!(graph["3"]["inputs"].get("quality_factor").is_none());
        }
        assert!(graph(Task::OldPhoto, &info()).is_err());
        let mut custom = info();
        custom.0["ImageUpscaleWithModel"]["python_module"] = json!("custom_nodes.unknown");
        assert!(graph(Task::Jpeg, &custom).unwrap_err().to_string().contains("core node"));
        let mut renamed = info();
        renamed.0["UpscaleModelLoader"]["input"]["required"]["model_name"] = json!([["sub/fbcnn_color.pth"]]);
        assert!(graph(Task::Jpeg, &renamed).is_err());
    }

    #[test]
    fn restoration_allowlist_rejects_tampering_and_extra_architectures() {
        for model in &policy().models {
            let f = &model.presets[0].files[0];
            validate_download(&f.name, &f.folder, &f.url, f.bytes, &f.sha256).unwrap();
            assert!(validate_download(&f.name, "checkpoints", &f.url, f.bytes, &f.sha256).is_err());
            assert!(validate_download(&f.name, &f.folder, "https://huggingface.co/unvetted/model.pth", f.bytes, &f.sha256).is_err());
            assert!(validate_download(&f.name, &f.folder, &f.url, f.bytes + 1, &f.sha256).is_err());
            assert!(validate_download(&f.name, &f.folder, &f.url, f.bytes, &"0".repeat(64)).is_err());
        }
        assert_eq!(pinned_file("ddcolor_paper_tiny.pth").unwrap().sha256, "81dd643904f4664c3718513e3320ae3db0c567f0d3e18398606659adee7bfc17");
        assert!(pinned_file("ddcolor_artistic.pth").is_none());
        assert!(pinned_file("../fbcnn_color.pth").is_none());
        let mut d =
            Detection { architecture: "FBCNN".into(), main: vec!["FBCNN".into()], extra: false, image: true, scale: 1, input_channels: 3, output_channels: 3 };
        validate_detection(Task::Jpeg, &d).unwrap();
        d.extra = true;
        validate_detection(Task::Jpeg, &d).unwrap(); // A user's BYO package does not disable main architectures.
        d.extra = false;
        d.architecture = "DDColor".into();
        assert!(validate_detection(Task::Jpeg, &d).is_err());
        d.architecture = "FBCNN".into();
        d.main.clear();
        assert!(validate_detection(Task::Jpeg, &d).is_err());
        assert!(validate_detection(Task::Colourise, &d).is_err());
        assert!(Task::OldPhoto.file().is_err());
    }

    #[test]
    fn restoration_missing_and_changed_files_are_refused_before_comfy() {
        let dir = std::env::temp_dir().join(format!("li-restoration-{}", uuid::Uuid::new_v4().simple()));
        let mut settings = AiSettings::default();
        settings.model_directory = dir.to_string_lossy().into();
        let client = ComfyClient::new("127.0.0.1:1");
        let image = RgbaImage::from_pixel(12, 9, Rgba([50, 70, 100, 123]));
        assert!(restore(&client, &settings, Task::Jpeg, &image, &JobControl::new()).unwrap_err().to_string().contains("Install"));
        std::fs::create_dir_all(dir.join("upscale_models")).unwrap();
        std::fs::write(dir.join("upscale_models/fbcnn_color.pth"), b"wrong weights").unwrap();
        assert!(restore(&client, &settings, Task::Jpeg, &image, &JobControl::new()).unwrap_err().to_string().contains("size differs"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restoration_mock_comfy_run_preserves_size_alpha_and_cancel() {
        let server = crate::mock::MockComfy::start().unwrap();
        let client = ComfyClient::new(server.host());
        let source = RgbaImage::from_fn(48, 37, |x, y| Rgba([x as u8, y as u8, 180, ((x + y) % 256) as u8]));
        for task in [Task::Jpeg, Task::Scan, Task::Denoise, Task::Colourise] {
            let out = execute(&client, task, &source, &JobControl::new()).unwrap();
            assert_eq!(out.dimensions(), source.dimensions());
            assert!(out.pixels().zip(source.pixels()).all(|(a, b)| a[3] == b[3]));
            assert_ne!(out, source);
        }
        assert_eq!(server.prompts().len(), 4);
        assert!(execute(&client, Task::OldPhoto, &source, &JobControl::new()).is_err());
        assert_eq!(server.prompts().len(), 4);
        let ctl = JobControl::new();
        ctl.cancel();
        assert!(execute(&client, Task::Jpeg, &source, &ctl).is_err());
        assert_eq!(server.prompts().len(), 4);
    }

    #[test]
    fn restoration_browser_offline_and_strict_install_plan() {
        let cfg = crate::browser::Config::default();
        let query = crate::browser::Query {
            view: crate::browser::View::Family("restoration".into()),
            kind: crate::browser::Kind::Model,
            search: String::new(),
            sources: Vec::new(),
        };
        assert!(crate::browser::urls(&cfg, crate::family::registry(), &query).is_empty());
        let cards = crate::browser::items(crate::family::registry(), &cfg, &query, &[]);
        assert_eq!(cards.len(), 4);
        let net = crate::browser::HttpNet::new(cfg.clone());
        for card in &cards {
            let plan = crate::browser::plan(card, &card.files, None, &ObjectInfo::default(), &cfg, &net).unwrap();
            assert!(!plan.license.is_empty() && plan.license_url.is_some());
            assert_eq!(plan.files[0].sha256, card.files[0].sha256.clone().unwrap());
            let mut installed = info();
            installed.0["UpscaleModelLoader"]["input"]["required"]["model_name"] = json!([[card.files[0].name]]);
            let present = crate::browser::plan(card, &card.files, None, &installed, &cfg, &net).unwrap();
            assert!(present.files.is_empty());
            assert_eq!(present.present, [card.files[0].name.clone()]);
            let mut files = card.files.clone();
            files[0].sha256 = Some("0".repeat(64));
            assert!(crate::browser::plan(card, &files, None, &ObjectInfo::default(), &cfg, &net).is_err());
        }
        let plan = crate::browser::Plan {
            files: vec![crate::browser::PlannedFile {
                name: "unvetted.pth".into(),
                folder: "upscale_models".into(),
                url: "https://huggingface.co/unvetted/model.pth".into(),
                bytes: 5,
                sha256: "0".repeat(64),
            }],
            ..Default::default()
        };
        assert!(crate::browser::install(&plan, Path::new("unused"), &cfg, &JobControl::new(), &|_, _, _| {}).is_err());
    }

    #[test]
    fn restoration_editable_profiles_cannot_widen_allowlist() {
        let dir = std::env::temp_dir().join(format!("li-restoration-profile-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(dir.join("user")).unwrap();
        let mut profile: Value = serde_json::from_str(PROFILE).unwrap();
        profile["version"] = json!(99999);
        profile["models"][0]["presets"][0]["files"][0]["sha256"] = json!("0".repeat(64));
        for folder in [&dir, &dir.join("user")] {
            std::fs::write(folder.join("restoration.json"), serde_json::to_vec(&profile).unwrap()).unwrap();
        }
        let registry = crate::family::load(&dir);
        assert_eq!(registry.family("restoration").unwrap().models[0].presets[0].files[0], *Task::Jpeg.file().unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "requires pinned downloads and idle real RTX 5090 ComfyUI"]
    fn restoration_real_5090_each_supported_model() {
        let dir = std::env::var("LOCAL_IMAGE_RESTORATION_MODELS").expect("test model directory required");
        let host = std::env::var("LOCAL_IMAGE_RESTORATION_COMFY").unwrap_or_else(|_| "127.0.0.1:8188".into());
        let client = ComfyClient::new(&host);
        let stats = client.system_stats().unwrap();
        assert!(
            stats["devices"].as_array().unwrap().iter().any(|d| d["name"].as_str().is_some_and(|n| n.contains("5090")) && d["type"] == "cuda"),
            "real GPU required: {stats}"
        );
        let mut settings = AiSettings::load();
        settings.model_directory = dir;
        settings.comfy_host = "127.0.0.1".into();
        settings.comfy_port = host.rsplit(':').next().unwrap().parse().unwrap();
        let input = RgbaImage::from_fn(65, 49, |x, y| Rgba([40 + (x % 16) as u8, 80 + (y % 16) as u8, 130, ((x + y) % 256) as u8]));
        for task in [Task::Jpeg, Task::Scan, Task::Denoise] {
            assert!(crate::refine::queue_idle(&client).unwrap(), "shared ComfyUI must be idle");
            let now = std::time::Instant::now();
            let output = restore(&client, &settings, task, &input, &JobControl::new()).unwrap();
            assert_eq!(output.dimensions(), input.dimensions());
            assert!(output.pixels().zip(input.pixels()).all(|(a, b)| a[3] == b[3]));
            assert_ne!(output, input);
            eprintln!("RESTORATION_REAL {task:?} {} ms", now.elapsed().as_millis());
        }
        let refusal = restore(&client, &settings, Task::Colourise, &input, &JobControl::new()).unwrap_err();
        assert!(refusal.to_string().contains("Main spandrel"), "{refusal:#}");
        assert!(crate::refine::queue_idle(&client).unwrap());
        eprintln!("RESTORATION_REAL Colourise: verified Apache pin refused by installed main spandrel");
    }
}
