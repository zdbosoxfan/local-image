//! Finding, starting and stopping a local ComfyUI installation, and the GPU guide.
//!
//! An installation is a folder with `main.py`, `folder_paths.py` and `comfy/`; the interpreter is
//! the portable `python_embeded`, a `.venv`/`venv`, or ComfyUI Desktop's `standalone-env`. The app
//! starts it on loopback with our model folder added through an extra-model-paths file, and keeps
//! every file ComfyUI creates in our profile.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result, bail};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::settings::{AiSettings, data_root, write_atomic};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installation {
    pub id: String,
    pub code: PathBuf,
    pub python: PathBuf,
    pub kind: &'static str,
}

impl Installation {
    pub fn label(&self) -> String {
        format!("ComfyUI {} — {}", self.kind, self.code.display())
    }
}

fn is_code_root(p: &Path) -> bool {
    p.join("main.py").is_file() && p.join("folder_paths.py").is_file() && p.join("comfy").is_dir()
}

/// Recognises a ComfyUI folder (or its portable parent); never guesses an arbitrary executable.
pub fn installation(path: &Path, python: Option<&Path>) -> Option<Installation> {
    for code in [path.to_path_buf(), path.join("ComfyUI"), path.join("ComfyUI_windows_portable").join("ComfyUI")] {
        if !is_code_root(&code) {
            continue;
        }
        let parent = code.parent().map(Path::to_path_buf).unwrap_or_else(|| code.clone());
        let portable = parent.join("python_embeded").join("python.exe");
        let standalone_win = parent.join("standalone-env").join("python.exe");
        let mut candidates: Vec<PathBuf> = python.map(|p| vec![p.to_path_buf()]).unwrap_or_default();
        candidates.extend([portable.clone(), code.join(".venv/Scripts/python.exe"), parent.join(".venv/Scripts/python.exe"), standalone_win.clone()]);
        if !cfg!(windows) {
            for base in [&code, &parent] {
                for name in [".venv", "venv", "standalone-env"] {
                    candidates.push(base.join(name).join("bin").join("python"));
                }
            }
        }
        let interp = candidates.into_iter().find(|p| p.is_file())?;
        let kind = if interp == portable {
            "portable"
        } else if interp == standalone_win || interp.components().any(|c| c.as_os_str() == "standalone-env") {
            "desktop"
        } else {
            "source"
        };
        let id = hex::encode(&Sha256::digest(code.to_string_lossy().to_lowercase().as_bytes())[..12]);
        return Some(Installation { id, code, python: interp, kind });
    }
    None
}

/// Every installation found in the usual places, the configured one first.
pub fn detect(settings: &AiSettings) -> Vec<Installation> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if !settings.comfy_directory.is_empty() {
        candidates.push(PathBuf::from(&settings.comfy_directory));
    }
    candidates.extend([
        data_root().join("ai").join("LocalImage-ComfyUI"),
        home.join("ComfyUI"),
        home.join("ComfyUI_windows_portable"),
        home.join("Documents").join("ComfyUI"),
        home.join("Documents").join("ComfyUI_windows_portable"),
        home.join("Downloads").join("ComfyUI_windows_portable"),
        home.join(".local/share/comfyui/ComfyUI"),
    ]);
    if let Ok(v) = std::env::var("LOCAL_IMAGE_COMFY_CANDIDATES") {
        candidates.extend(std::env::split_paths(&v));
    }
    for parent in [home.join("ComfyUI-Installs"), dirs::data_local_dir().unwrap_or_default().join("Comfy-Desktop").join("ComfyUI-Installs")] {
        if let Ok(rd) = std::fs::read_dir(parent) {
            for e in rd.flatten().take(40) {
                candidates.push(e.path());
            }
        }
    }
    let python = (!settings.comfy_python.is_empty()).then(|| PathBuf::from(&settings.comfy_python));
    let mut out: Vec<Installation> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let py = if i == 0 { python.as_deref() } else { None };
        if let Some(inst) = installation(c, py)
            && !out.iter().any(|o| o.id == inst.id)
        {
            out.push(inst);
        }
    }
    out
}

/// Writes the extra-model-paths file that adds our model folder to ComfyUI.
pub fn write_model_paths(settings: &AiSettings) -> Result<PathBuf> {
    let path = data_root().join("state").join("local-image-model-paths.yaml");
    let models = settings.model_dir();
    // JSON is valid YAML; ComfyUI reads it as such.
    let v = serde_json::json!({ "local_image": {
        "base_path": models, "diffusion_models": "diffusion_models", "text_encoders": "text_encoders",
        "vae": "vae", "loras": "loras", "checkpoints": "checkpoints" } });
    write_atomic(&path, serde_json::to_string_pretty(&v)?.as_bytes())?;
    Ok(path)
}

/// The command line that starts `inst` on loopback.
pub fn launch_command(inst: &Installation, settings: &AiSettings) -> Result<Command> {
    if !is_code_root(&inst.code) || !inst.python.is_file() {
        bail!("The selected ComfyUI installation is incomplete. Choose another folder.");
    }
    let extra = write_model_paths(settings)?;
    let mut cmd = Command::new(&inst.python);
    cmd.arg("-s").arg(inst.code.join("main.py")).args(["--listen", "127.0.0.1", "--port", &settings.comfy_port.to_string(), "--disable-auto-launch"]);
    cmd.arg("--extra-model-paths-config").arg(extra);
    if inst.kind == "portable" {
        cmd.arg("--windows-standalone-build");
    }
    let runtime = data_root().join("comfy-runtime").join(&inst.id);
    for name in ["input", "output", "temp", "user"] {
        let d = runtime.join(name);
        std::fs::create_dir_all(&d)?;
        cmd.arg(format!("--{name}-directory")).arg(d);
    }
    cmd.current_dir(&inst.code);
    for var in ["LD_LIBRARY_PATH", "QT_PLUGIN_PATH", "PYTHONHOME", "PYTHONPATH"] {
        cmd.env_remove(var);
    }
    Ok(cmd)
}

/// Starts ComfyUI with its output in `logs/comfyui.log`.
pub fn start(inst: &Installation, settings: &AiSettings) -> Result<Child> {
    let mut cmd = launch_command(inst, settings)?;
    let logs = data_root().join("logs");
    std::fs::create_dir_all(&logs)?;
    let log = std::fs::File::create(logs.join("comfyui.log"))?;
    cmd.stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log);
    cmd.spawn().with_context(|| format!("Could not start {}", inst.python.display()))
}

/// One GPU as reported by ComfyUI or `nvidia-smi`.
#[derive(Clone, Debug, PartialEq)]
pub struct Gpu {
    pub name: String,
    pub vram_total: u64,
    pub vram_used: Option<u64>,
    pub utilization: Option<u32>,
}

/// Devices from ComfyUI's `/system_stats` (non-CPU only).
pub fn gpus_from_stats(stats: &Value) -> Vec<Gpu> {
    stats
        .get("devices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|d| d.get("type").and_then(Value::as_str) != Some("cpu"))
        .map(|d| {
            let raw = d.get("name").and_then(Value::as_str).unwrap_or("GPU");
            let mut name = raw.to_owned();
            if let Some(rest) = name.strip_prefix("cuda:")
                && let Some(i) = rest.find(' ')
            {
                name = rest[i + 1..].to_owned();
            }
            if let Some(i) = name.find(" : cudaMalloc") {
                name.truncate(i);
            }
            let total = d.get("vram_total").and_then(Value::as_u64).unwrap_or(0);
            let free = d.get("vram_free").and_then(Value::as_u64);
            Gpu { name: name.trim().to_owned(), vram_total: total, vram_used: free.map(|f| total.saturating_sub(f)), utilization: None }
        })
        .collect()
}

/// NVIDIA GPUs via `nvidia-smi` (fast, works without ComfyUI).
pub fn gpus_from_nvidia_smi() -> Vec<Gpu> {
    let Ok(out) = Command::new("nvidia-smi").args(["--query-gpu=name,memory.total,memory.used,utilization.gpu", "--format=csv,noheader,nounits"]).output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            let mib = |s: &str| s.parse::<u64>().ok().map(|m| m * 1024 * 1024);
            Some(Gpu {
                name: f.first()?.to_string(),
                vram_total: mib(f.get(1)?)?,
                vram_used: f.get(2).and_then(|s| mib(s)),
                utilization: f.get(3).and_then(|s| s.parse().ok()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_source_install_with_a_venv() {
        let root = std::env::temp_dir().join(format!("li-comfy-{}", uuid::Uuid::new_v4()));
        let code = root.join("ComfyUI");
        std::fs::create_dir_all(code.join("comfy")).unwrap();
        std::fs::write(code.join("main.py"), "").unwrap();
        std::fs::write(code.join("folder_paths.py"), "").unwrap();
        assert!(installation(&root, None).is_none(), "no interpreter yet");
        let py = if cfg!(windows) { code.join(".venv/Scripts/python.exe") } else { code.join(".venv/bin/python") };
        std::fs::create_dir_all(py.parent().unwrap()).unwrap();
        std::fs::write(&py, "").unwrap();
        let inst = installation(&root, None).unwrap();
        assert_eq!(inst.code, code);
        assert_eq!(inst.kind, "source");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn parses_system_stats_devices() {
        let v = serde_json::json!({"devices": [{"name": "cuda:0 NVIDIA GeForce RTX 5090 : cudaMallocAsync", "type": "cuda", "vram_total": 34359738368u64, "vram_free": 30000000000u64}, {"name": "cpu", "type": "cpu"}]});
        let g = gpus_from_stats(&v);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].name, "NVIDIA GeForce RTX 5090");
        assert_eq!(g[0].vram_used, Some(34359738368 - 30000000000));
    }
}
