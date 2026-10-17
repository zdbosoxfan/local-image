//! GPU adapter info, UI-thread timings and the compositor's memory budget.
use super::*;

/// UI-thread timings (CPU side), exponentially smoothed.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Perf {
    /// Average CPU time building the UI per frame (ms).
    pub ui_ms: f64,
    /// Worst UI frame of the last ~second (ms).
    pub ui_max_ms: f64,
    /// Last canvas refresh: CPU composite (ms).
    pub composite_ms: f64,
    /// Last canvas refresh: conversion + upload (ms).
    pub upload_ms: f64,
    /// Last canvas refresh kind: "full", "rect" or "none".
    pub last_refresh: &'static str,
    /// Pixels in the last refresh.
    pub last_refresh_px: u64,
    /// Whether the GPU canvas path is active.
    pub gpu: bool,
    /// Layer tiles uploaded by the last GPU composite.
    pub gpu_uploads: u64,
    /// Why the last refresh fell back to the CPU compositor (None = GPU composited).
    pub gpu_fallback: Option<String>,
    /// Adapter, backend, driver and fallback state (Help › System Info).
    pub gpu_info: GpuInfo,
    /// GPU memory the compositor may hold for layer pages and effect maps (MB; see
    /// [`memory_budget`]).
    pub gpu_budget_mb: u64,
    /// The Memory Usage allowance (bytes) `gpu_budget_mb` was computed from.
    #[serde(skip)]
    pub gpu_budget_allowance: u64,
    /// Last command run through the app and its duration (ms).
    pub last_command: String,
    pub command_ms: f64,
    /// Most recent cost of occasional UI work (outline tracing, thumbnails, …), in ms.
    pub spans: std::collections::BTreeMap<&'static str, f64>,
    #[serde(skip)]
    pub(super) max_decay: f64,
}

/// What the app renders with and why (Help › System Info, Preferences › Performance, the `perf`
/// control-channel output).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    /// Adapter name ("" before the window opens).
    pub adapter: String,
    pub output_depth: u8,
    pub output_note: String,
    /// wgpu backend of the window's device ("vulkan", "dx12", "metal", "gl", …).
    pub backend: String,
    /// Device type ("discreteGpu", "integratedGpu", "cpu", …).
    pub device_type: String,
    /// Driver name and version as the driver reports them.
    pub driver: String,
    /// `performance.gpuBackend` at launch.
    pub preference: String,
    /// The backend startup chose ("auto", "dx12", "cpu", …, or "env" for `WGPU_BACKEND`).
    pub selected: String,
    /// Why startup didn't use the preference as is (previous start crashed, `--safe-gpu`,
    /// `WGPU_BACKEND`, Intel on Windows).
    pub fallback: Option<String>,
    /// "gpu" (wgpu canvas and compositor) or "cpu" (CPU compositor, egui textures).
    pub canvas: String,
    /// Why the GPU canvas was dropped during this session (device lost or a GPU error).
    pub lost: Option<String>,
    /// Why the app is in CPU mode because of a GPU failure (this launch's or an earlier one's
    /// that left CPU mode on). The status bar and Preferences › Performance keep showing it.
    pub recovery: Option<String>,
}

impl GpuInfo {
    /// Fill the adapter fields from `info`.
    pub fn set_adapter(&mut self, info: &wgpu::AdapterInfo) {
        self.adapter = info.name.clone();
        self.backend = backend_name(info.backend).to_string();
        self.device_type = match info.device_type {
            wgpu::DeviceType::DiscreteGpu => "discreteGpu",
            wgpu::DeviceType::IntegratedGpu => "integratedGpu",
            wgpu::DeviceType::VirtualGpu => "virtualGpu",
            wgpu::DeviceType::Cpu => "cpu",
            wgpu::DeviceType::Other => "other",
        }
        .to_string();
        self.driver = [info.driver.as_str(), info.driver_info.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ");
    }

    /// Human-readable lines for Help › System Info.
    pub fn lines(&self) -> Vec<String> {
        let or = |s: &str, d: &str| if s.is_empty() { d.to_string() } else { s.to_string() };
        let mut v = vec![
            format!(
                "Display output: {} client surface{}",
                if self.output_depth == 0 { "unknown".to_string() } else { format!("{}-bit", self.output_depth) },
                if self.output_note.is_empty() { String::new() } else { format!(" — {}", self.output_note) }
            ),
            format!("Graphics adapter: {}", or(&self.adapter, "unknown")),
            format!("Backend: {} ({})", or(&self.backend, "unknown"), or(&self.device_type, "unknown type")),
            format!("Driver: {}", or(&self.driver, "unknown")),
            format!("GPU backend preference: {}", or(&self.preference, "auto")),
            format!("Selected at launch: {}", or(&self.selected, "auto")),
            format!("Image compositor: {}", if self.canvas == "gpu" { "GPU" } else { "CPU" }),
            format!(
                "Window renderer: {}",
                if self.device_type == "cpu" {
                    "Software adapter"
                } else if self.adapter.is_empty() {
                    "Unknown"
                } else {
                    "Graphics adapter"
                }
            ),
        ];
        if let Some(f) = &self.fallback {
            v.push(format!("Fallback: {f}"));
        }
        if let Some(l) = &self.lost {
            v.push(format!("This session: {l}"));
        }
        if let Some(r) = self.recovery.as_ref().filter(|r| self.fallback.as_ref() != Some(*r)) {
            v.push(format!("CPU mode after a GPU failure: {r}"));
        }
        v
    }

    /// Why GPU acceleration is off because of a failure, when it is (`None` when the GPU canvas
    /// is in use or the user chose CPU rendering).
    pub fn failure(&self) -> Option<&str> {
        if self.canvas == "gpu" {
            return None;
        }
        self.lost.as_deref().or(self.recovery.as_deref())
    }
}

/// The preference name of a wgpu backend.
pub fn backend_name(b: wgpu::Backend) -> &'static str {
    match b {
        wgpu::Backend::Vulkan => "vulkan",
        wgpu::Backend::Dx12 => "dx12",
        wgpu::Backend::Metal => "metal",
        wgpu::Backend::Gl => "gl",
        wgpu::Backend::BrowserWebGpu => "webgpu",
        wgpu::Backend::Noop => "noop",
    }
}

impl Perf {
    pub fn record(&mut self, kind: &'static str, px: u64, composite_ms: f64, upload_ms: f64) {
        self.last_refresh = kind;
        self.last_refresh_px = px;
        self.composite_ms = composite_ms;
        self.upload_ms = upload_ms;
        if !kind.starts_with("gpu") {
            self.gpu_uploads = 0;
        } else {
            self.gpu_fallback = None;
        }
    }
    pub fn span(&mut self, name: &'static str, ms: f64) {
        self.spans.insert(name, (ms * 100.0).round() / 100.0);
    }
    pub fn frame(&mut self, ms: f64) {
        self.ui_ms = if self.ui_ms == 0.0 { ms } else { self.ui_ms * 0.9 + ms * 0.1 };
        self.max_decay += 1.0;
        if ms > self.ui_max_ms || self.max_decay > 60.0 {
            self.ui_max_ms = ms;
            self.max_decay = 0.0;
        }
    }
}

/// Floor of the compositor's memory budget: enough for a viewport's pages of a dozen layers.
pub const MIN_GPU_BUDGET: u64 = 512 << 20;

/// GPU memory the wgpu compositor may hold for layer pages and effect maps: what Memory Usage
/// (`allowance`, Preferences › Performance) leaves after the document's pixels and History
/// (`pixels`), at most a quarter of physical memory (`ram`; 16 GB assumed when unknown), and at
/// least [`MIN_GPU_BUDGET`]. On unified-memory machines these textures share RAM with the
/// document, so a huge document keeps its layers on the GPU only as far as memory allows; the
/// rest is uploaded per refresh as the view needs it.
pub fn memory_budget(allowance: u64, pixels: u64, ram: Option<u64>) -> u64 {
    let ram = ram.unwrap_or(16 << 30);
    allowance.saturating_sub(pixels).min(ram / 4).max(MIN_GPU_BUDGET)
}

/// Physical memory of this machine in bytes, if known. `PHOTOCRAFT_RAM_MB` overrides it (to
/// simulate a smaller machine).
pub fn physical_memory() -> Option<u64> {
    static RAM: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *RAM.get_or_init(|| {
        if let Some(mb) = std::env::var("PHOTOCRAFT_RAM_MB").ok().and_then(|v| v.trim().parse::<u64>().ok()) {
            return mb.checked_mul(1 << 20);
        }
        detect_physical_memory()
    })
}

#[cfg(target_os = "macos")]
pub(super) fn detect_physical_memory() -> Option<u64> {
    let out = std::process::Command::new("/usr/sbin/sysctl").args(["-n", "hw.memsize"]).output().ok()?;
    String::from_utf8(out.stdout).ok()?.trim().parse().ok()
}

#[cfg(target_os = "linux")]
pub(super) fn detect_physical_memory() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb: u64 = info.lines().find_map(|l| l.strip_prefix("MemTotal:"))?.trim().trim_end_matches("kB").trim().parse().ok()?;
    kb.checked_mul(1024)
}

#[cfg(target_os = "freebsd")]
pub(super) fn detect_physical_memory() -> Option<u64> {
    let out = std::process::Command::new("/sbin/sysctl").args(["-n", "hw.physmem"]).output().ok()?;
    String::from_utf8(out.stdout).ok()?.trim().parse().ok()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "freebsd")))]
pub(super) fn detect_physical_memory() -> Option<u64> {
    None
}
