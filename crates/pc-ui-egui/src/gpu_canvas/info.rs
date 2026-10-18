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
    /// Memory Usage and GPU Memory (bytes) and the document `gpu_budget_mb` was computed for.
    #[serde(skip)]
    pub gpu_budget_inputs: (u64, u64, Option<photocraft_doc::DocId>),
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

/// Why a document whose display textures don't fit [`canvas_budget`] is drawn on the CPU canvas.
pub const OVER_CANVAS_BUDGET: &str = "the canvas exceeds the GPU texture budget; drawing it on the CPU canvas";

/// [`Refresh::kind`] when the document's display textures don't fit [`canvas_budget`]: nothing
/// was created, and the caller draws the CPU canvas instead.
pub const REFRESH_OVER_BUDGET: &str = "over-budget";

/// GPU memory the wgpu compositor may hold for layer pages and effect maps.
///
/// `fixed` (Preferences › Performance › GPU Memory, bytes) sets it when it isn't 0, within
/// [`vram_share`] of a discrete GPU's own memory (`vram`, from `li_gpu::memory::video_memory`).
/// Otherwise it is Memory Usage (`allowance`) within a quarter of physical memory (`ram`; 16 GB
/// assumed when unknown) and the VRAM share. The document's pixels and History (`pixels`) come
/// off Memory Usage unless the card's own memory is known: a discrete card doesn't hold them,
/// while integrated and software adapters (and a card whose memory can't be read) share the
/// system's. Never below [`MIN_GPU_BUDGET`].
pub fn memory_budget(allowance: u64, pixels: u64, ram: Option<u64>, vram: Option<u64>, fixed: u64) -> u64 {
    let card = vram.map_or(u64::MAX, vram_share);
    if fixed > 0 {
        return fixed.min(card).max(MIN_GPU_BUDGET);
    }
    let charged = if vram.is_some() { 0 } else { pixels };
    allowance.saturating_sub(charged).min(ram.unwrap_or(16 << 30) / 4).min(card).max(MIN_GPU_BUDGET)
}

/// The share of a discrete GPU's memory each of [`memory_budget`] and [`canvas_budget`] may plan
/// for: three eighths, so together they leave a quarter of the card to the window and other
/// applications.
pub fn vram_share(vram: u64) -> u64 {
    vram / 8 * 3
}

/// GPU bytes one document's display texture set (every tile and mip level) may take: a quarter
/// of physical memory (`ram`; 16 GB assumed when unknown) and at most [`vram_share`] of a discrete
/// GPU's own memory, at least [`MIN_GPU_BUDGET`]. The set covers the whole canvas at full
/// resolution, so its size follows the declared canvas, not the layer pixels.
pub fn canvas_budget(ram: Option<u64>, vram: Option<u64>) -> u64 {
    (ram.unwrap_or(16 << 30) / 4).min(vram.map_or(u64::MAX, vram_share)).max(MIN_GPU_BUDGET)
}

/// Price every tile and mip level without allocating or iterating over the canvas area.
pub fn canvas_texture_bytes(size: [u32; 2], tile: u32, bpp: u64) -> u64 {
    if tile == 0 {
        return u64::MAX;
    }
    let [w, h] = size;
    let axis = |side| [(tile, u64::from(side / tile)), (side % tile, 1)];
    let mut bytes = 0u64;
    for (w, nx) in axis(w) {
        for (h, ny) in axis(h) {
            if w == 0 || h == 0 {
                continue;
            }
            let (mut w, mut h) = (w, h);
            let mut pixels = 0u64;
            loop {
                pixels = pixels.saturating_add(u64::from(w) * u64::from(h));
                if w == 1 && h == 1 {
                    break;
                }
                w = (w / 2).max(1);
                h = (h / 2).max(1);
            }
            bytes = bytes.saturating_add(pixels.saturating_mul(nx).saturating_mul(ny).saturating_mul(bpp));
        }
    }
    bytes
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

#[cfg(test)]
mod memory_tests {
    use super::*;

    #[test]
    fn discrete_memory_excludes_document_pixels_and_caps_explicit_budgets() {
        const G: u64 = 1 << 30;
        assert_eq!(memory_budget(8 * G, 3 * G, Some(48 * G), Some(24 * G), 0), 8 * G);
        assert_eq!(memory_budget(8 * G, 3 * G, Some(48 * G), None, 0), 5 * G);
        assert_eq!(memory_budget(8 * G, 9 * G, Some(16 * G), None, 12 * G), 12 * G);
        assert_eq!(memory_budget(8 * G, 0, Some(96 * G), Some(8 * G), 12 * G), 3 * G);
        assert_eq!(canvas_budget(Some(96 * G), Some(8 * G)), 3 * G);
        assert_eq!(memory_budget(0, u64::MAX, None, None, 0), MIN_GPU_BUDGET);
    }

    #[test]
    fn canvas_pricing_counts_partial_tiles_and_mips_without_overflow() {
        for size in [[1, 1], [5, 3], [17, 25], [128, 128]] {
            for tile in [1, 3, 16, 256] {
                let mut expected = 0;
                for y in (0..size[1]).step_by(tile as usize) {
                    for x in (0..size[0]).step_by(tile as usize) {
                        let (w, h) = (tile.min(size[0] - x), tile.min(size[1] - y));
                        for level in 0..super::super::mip_count(w, h) {
                            expected += u64::from((w >> level).max(1)) * u64::from((h >> level).max(1)) * 8;
                        }
                    }
                }
                assert_eq!(canvas_texture_bytes(size, tile, 8), expected);
            }
        }
        assert_eq!(canvas_texture_bytes([u32::MAX; 2], 8192, 8), u64::MAX);
        assert!(canvas_texture_bytes([300_000; 2], 8192, 4) > canvas_budget(Some(1 << 40), None));
    }
}
