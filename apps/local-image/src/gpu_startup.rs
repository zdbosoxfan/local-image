//! Crash-safe GPU startup (#4).
//!
//! A graphics driver that segfaults or hangs while the window's wgpu device is created or the
//! first frames are drawn (the Intel Vulkan driver `igvk64.dll` on Windows) can't be caught
//! in-process. So before the device exists the app writes a *marker* (`gpu-starting.json` in the
//! config directory: backend, attempt and, once chosen, the adapter) and holds an OS file lock on
//! it; it clears the marker once the first frames have rendered. If the next launch finds a marker
//! that no running process holds, the previous start died (or was force-closed) before it
//! finished, and this one moves along the platform's chain:
//!
//! - Windows: Vulkan → DX12 → CPU
//! - Linux and other Unix: Vulkan → Vulkan again → compatible GPU → CPU
//! - macOS: Metal → CPU
//!
//! On Linux a start that didn't finish isn't necessarily a driver crash (a hang the user closed,
//! the session ending, the out-of-memory killer), so the same backend gets one more try first.
//! The next step is the **compatible GPU** set (`gl`): wgpu's Vulkan *and* GL backends, with OpenGL
//! adapters ranked first, the adapter that failed ranked last and software adapters only after
//! every hardware one. (GL alone could leave nothing able to present: wgpu's GL backend can't
//! present to some Wayland surfaces, which used to drop such machines straight to the CPU.)
//!
//! A start whose renderer can't initialise at all (no adapter can present: eframe's
//! `Error::Wgpu`, before any app code runs) re-launches itself (`main.rs`). On Linux it first tries
//! the compatible GPU set ([`after_init_failure`], [`RETRY_ENV`]), and only then the CPU renderer.
//!
//! The choice that worked is remembered in `performance.gpuBackend` (Preferences › Performance,
//! where it can be reset), and why in `gpu-recovery.json` ([`Recovery`]), which keeps the adapter
//! to avoid for the compatible set and the reason the UI shows while GPU acceleration stays off.
//! With `auto`, Intel adapters on Windows use DX12. `WGPU_BACKEND` still overrides everything, and
//! `--safe-gpu` forces the CPU path for one launch.
//!
//! "CPU" composites on the CPU and draws the window with a software adapter where the platform
//! has one (WARP on Windows, llvmpipe or lavapipe elsewhere).

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use eframe::{egui_wgpu, wgpu};
use photocraft_engine::prefs::{GpuBackend, RenderingMode};
use serde_json::{Value, json};

/// The marker's file name in the config directory.
pub const MARKER_FILE: &str = "gpu-starting.json";
/// Why GPU acceleration is reduced or off after a failure (see [`Recovery`]).
pub const RECOVERY_FILE: &str = "gpu-recovery.json";
/// Set on the process re-launched after the renderer couldn't initialise: the backend to try.
pub const RETRY_ENV: &str = "LOCAL_IMAGE_GPU_RETRY";
/// With [`RETRY_ENV`]: why the previous process couldn't initialise.
pub const RETRY_REASON_ENV: &str = "LOCAL_IMAGE_GPU_RETRY_REASON";
/// PCI vendor id of Intel.
pub const INTEL: u32 = 0x8086;
/// Largest marker read (a marker is a few hundred bytes).
const MARKER_MAX: u64 = 64 << 10;

/// Platform families with different backend chains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    /// Linux, the BSDs and other Unix.
    Other,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Mac
        } else {
            Os::Other
        }
    }
}

/// What a start records before touching the driver.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marker {
    /// The plan's backend (`auto`, `dx12`, …), or `env:<value>` under `WGPU_BACKEND`.
    pub backend: String,
    /// The chosen adapter's name, backend and driver (empty until the adapter is chosen).
    pub adapter: String,
    pub adapter_backend: String,
    pub driver: String,
    pub version: String,
    /// 0 for the first start on this plan, 1 for the Linux retry of the same backend.
    pub attempt: u32,
}

impl Marker {
    pub fn to_json(&self) -> String {
        json!({
            "backend": self.backend,
            "adapter": self.adapter,
            "adapterBackend": self.adapter_backend,
            "driver": self.driver,
            "version": self.version,
            "attempt": self.attempt,
        })
        .to_string()
    }

    /// Parse a marker leniently: anything unreadable is a marker of an unknown backend.
    pub fn parse(text: &str) -> Self {
        let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").chars().take(256).collect::<String>();
        let attempt = v.get("attempt").and_then(Value::as_u64).map_or(0, |a| a.min(9) as u32);
        Marker { backend: s("backend"), adapter: s("adapter"), adapter_backend: s("adapterBackend"), driver: s("driver"), version: s("version"), attempt }
    }

    /// The backend the crashed start planned (`auto` for an empty or unreadable marker), `None`
    /// under `WGPU_BACKEND`.
    pub fn planned(&self) -> Option<GpuBackend> {
        if self.backend.is_empty() { Some(GpuBackend::Auto) } else { GpuBackend::parse(&self.backend) }
    }

    /// The backend the crashed start was using: the adapter's when it got that far with `auto`,
    /// else the plan's. `None` for a start under `WGPU_BACKEND` (the user's own override).
    pub fn tried(&self) -> Option<GpuBackend> {
        let planned = self.planned()?;
        match (planned, GpuBackend::parse(&self.adapter_backend)) {
            (GpuBackend::Auto, Some(actual)) => Some(actual),
            _ => Some(planned),
        }
    }
}

/// An adapter a failed start used, ranked last by the compatible GPU set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Avoid {
    /// The adapter's name as wgpu reports it.
    pub adapter: String,
    /// Its backend (`vulkan`, `gl`, …; empty: any).
    pub backend: String,
}

impl Avoid {
    fn from_marker(m: &Marker) -> Option<Self> {
        (!m.adapter.is_empty()).then(|| Avoid { adapter: m.adapter.clone(), backend: m.adapter_backend.clone() })
    }

    /// Whether `name` on `backend` is this adapter.
    pub fn matches(&self, name: &str, backend: &str) -> bool {
        self.adapter == name && (self.backend.is_empty() || self.backend == backend)
    }
}

/// The backend this launch uses and why.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub backend: GpuBackend,
    /// `WGPU_BACKEND`, which picks the backends itself.
    pub env: Option<String>,
    /// Why this isn't simply the preference.
    pub reason: Option<String>,
    /// Store `backend` in the preferences once the start succeeded (a crash fallback).
    pub remember: bool,
    /// Retry number on the same backend (written to the marker; see [`Marker::attempt`]).
    pub attempt: u32,
    /// An adapter to rank last (the compatible GPU set after a failure).
    pub avoid: Option<Avoid>,
}

/// A backend the platform has (others fall back to `auto`).
pub fn sanitize(pref: GpuBackend, os: Os) -> GpuBackend {
    use GpuBackend::*;
    match (os, pref) {
        (Os::Windows, Metal) | (Os::Mac, Vulkan | Dx12 | Gl) | (Os::Other, Dx12 | Metal) => Auto,
        (_, b) => b,
    }
}

/// The next safer backend after a start on `tried` crashed. On Linux `gl` is the compatible GPU
/// set (Vulkan and GL, see [`backends`]), not GL alone.
pub fn next_safer(tried: GpuBackend, os: Os) -> GpuBackend {
    use GpuBackend::*;
    match (os, tried) {
        (Os::Windows, Auto | Vulkan | Gl | Metal) => Dx12,
        (Os::Windows, Dx12 | Cpu) => Cpu,
        (Os::Mac, _) => Cpu,
        (Os::Other, Auto | Vulkan | Dx12 | Metal) => Gl,
        (Os::Other, Gl | Cpu) => Cpu,
    }
}

/// Decide this launch's backend from the preference, a marker left by a start that crashed,
/// `WGPU_BACKEND` and `--safe-gpu`.
pub fn plan(pref: GpuBackend, crashed: Option<&Marker>, env: Option<&str>, safe_gpu: bool, os: Os) -> Plan {
    if let Some(v) = env.map(str::trim).filter(|v| !v.is_empty()) {
        let backend = if safe_gpu { GpuBackend::Cpu } else { GpuBackend::Auto };
        return Plan { backend, env: Some(v.to_string()), reason: Some(format!("WGPU_BACKEND={v} overrides the backend")), remember: false, ..Plan::default() };
    }
    if safe_gpu {
        return Plan {
            backend: GpuBackend::Cpu,
            env: None,
            reason: Some("--safe-gpu: CPU renderer for this launch".into()),
            remember: false,
            ..Plan::default()
        };
    }
    let pref = sanitize(pref, os);
    if let Some((m, tried)) = crashed.and_then(|m| Some((m, m.tried()?))) {
        let on = if m.adapter.is_empty() { String::new() } else { format!(" ({})", m.adapter) };
        // Linux: a start that didn't finish may have been a hang the user closed, the session
        // ending or the out-of-memory killer; the same backend gets one more try.
        if let Some(again) = retry_same(m, os) {
            return Plan {
                backend: again,
                reason: Some(format!("the previous start didn't finish on {}{on}; trying it once more", tried.name())),
                attempt: m.attempt + 1,
                ..Plan::default()
            };
        }
        let next = next_safer(tried, os);
        let avoid = if os == Os::Other && next == GpuBackend::Gl { Avoid::from_marker(m) } else { None };
        return Plan {
            backend: next,
            env: None,
            reason: Some(format!("the previous start didn't finish on {}{on}; using {}", tried.name(), describe(next, os))),
            remember: next != pref,
            attempt: 0,
            avoid,
        };
    }
    Plan { backend: pref, ..Plan::default() }
}

/// The backend to try again after a start that didn't finish, if the platform retries it: Linux
/// gives `auto`/`vulkan` one more try before moving down the chain.
pub fn retry_same(m: &Marker, os: Os) -> Option<GpuBackend> {
    let planned = m.planned()?;
    (os == Os::Other && m.attempt == 0 && matches!(planned, GpuBackend::Auto | GpuBackend::Vulkan)).then_some(planned)
}

/// How the UI names a backend choice (`gl` on Linux is the compatible set).
pub fn describe(b: GpuBackend, os: Os) -> &'static str {
    match (b, os) {
        (GpuBackend::Gl, Os::Other) => "the compatible GPU mode (OpenGL or Vulkan, another adapter first)",
        (GpuBackend::Cpu, _) => "the CPU renderer",
        (b, _) => b.name(),
    }
}

/// The GPU option to re-launch with after the renderer couldn't initialise on `plan` (eframe's
/// `Error::Wgpu`: no adapter could present). Linux tries the compatible GPU set before the CPU;
/// Windows and macOS go straight to the CPU renderer, as before. `None`: use the CPU renderer.
pub fn after_init_failure(plan: &Plan, os: Os) -> Option<GpuBackend> {
    (os == Os::Other && plan.env.is_none() && matches!(plan.backend, GpuBackend::Auto | GpuBackend::Vulkan)).then_some(GpuBackend::Gl)
}

/// The plan for a process re-launched by [`after_init_failure`]: `retry` is [`RETRY_ENV`],
/// `why` [`RETRY_REASON_ENV`]. Keeps `plan` when there is no valid retry or the user forced a
/// choice (`WGPU_BACKEND`, `--safe-gpu`, CPU mode).
pub fn with_init_retry(plan: Plan, retry: Option<&str>, why: Option<&str>, os: Os) -> Plan {
    let Some(next) = retry.and_then(GpuBackend::parse).map(|b| sanitize(b, os)) else { return plan };
    if plan.env.is_some() || plan.backend == GpuBackend::Cpu || next == GpuBackend::Cpu {
        return plan;
    }
    let why = why.map(str::trim).filter(|w| !w.is_empty()).map(|w| format!(" ({w})")).unwrap_or_default();
    Plan {
        backend: next,
        reason: Some(format!("the graphics adapter couldn't open the window{why}; trying {}", describe(next, os))),
        remember: next != plan.backend,
        ..Plan::default()
    }
}

/// Apply a saved [`Recovery`] to `plan`: the compatible GPU set keeps avoiding the adapter that
/// failed in an earlier run.
pub fn with_recovery(mut plan: Plan, recovery: Option<&Recovery>, os: Os) -> Plan {
    if os == Os::Other && plan.backend == GpuBackend::Gl && plan.env.is_none() && plan.avoid.is_none() {
        plan.avoid = recovery.and_then(|r| r.avoid.clone());
    }
    plan
}

/// Resolve the rendering policy before the window exists. Explicit GPU/Automatic selections
/// reset a legacy CPU backend to automatic; CPU keeps software-preferred window presentation.
pub fn plan_with_mode(pref: GpuBackend, mode: RenderingMode, crashed: Option<&Marker>, env: Option<&str>, safe_gpu: bool, os: Os) -> Plan {
    let backend = match mode {
        RenderingMode::Cpu => GpuBackend::Cpu,
        RenderingMode::Auto | RenderingMode::Gpu if pref == GpuBackend::Cpu => GpuBackend::Auto,
        _ => pref,
    };
    let env = if safe_gpu || mode == RenderingMode::Cpu { None } else { env };
    let crashed = if mode == RenderingMode::Cpu { None } else { crashed };
    plan(backend, crashed, env, safe_gpu, os)
}

/// Instance backends for `plan` (`None`: egui's default, which honours `WGPU_BACKEND`).
pub fn backends(plan: &Plan, os: Os) -> Option<wgpu::Backends> {
    if plan.env.is_some() {
        return None;
    }
    Some(match plan.backend {
        GpuBackend::Auto => return None,
        GpuBackend::Vulkan => wgpu::Backends::VULKAN,
        GpuBackend::Dx12 => wgpu::Backends::DX12,
        GpuBackend::Metal => wgpu::Backends::METAL,
        // Linux: the compatible GPU set. GL alone can't present to some Wayland surfaces ("gl not
        // compatible with provided surface"), which left nothing but the CPU renderer.
        GpuBackend::Gl if os == Os::Other => wgpu::Backends::VULKAN | wgpu::Backends::GL,
        GpuBackend::Gl => wgpu::Backends::GL,
        GpuBackend::Cpu => match os {
            Os::Windows => wgpu::Backends::DX12,
            Os::Mac => wgpu::Backends::METAL,
            // local-image: GL alone could leave no backend able to present (EGL/GLX missing while
            // Vulkan's lavapipe works, as on headless and some Wayland setups), and the window then
            // never opened again after "Keep Using CPU". Software adapters still rank first.
            Os::Other => wgpu::Backends::VULKAN | wgpu::Backends::GL,
        },
    })
}

/// The DX12 shader compiler (#712). wgpu's default loads `dxcompiler.dll` by name, and the
/// Windows DLL search falls through to the current directory and `PATH`, so another program's DXC
/// build (a browser's, an SDK's) got loaded and failed device creation or crashed the first shader
/// compile. Use a `dxcompiler.dll` shipped beside the executable, by its full path; otherwise FXC
/// (`d3dcompiler_47.dll`, part of Windows). `WGPU_DX12_COMPILER` still picks one explicitly.
pub fn dx12_compiler(env: Option<&str>, exe_dir: Option<&Path>) -> wgpu::Dx12Compiler {
    if let Some(c) = env.and_then(|v| v.trim().parse().ok()) {
        return c;
    }
    let beside_exe = exe_dir.map(|d| d.join("dxcompiler.dll")).filter(|p| p.is_file());
    match beside_exe.as_deref().and_then(Path::to_str) {
        Some(p) => wgpu::Dx12Compiler::DynamicDxc { dxc_path: p.to_string() },
        None => wgpu::Dx12Compiler::Fxc,
    }
}

/// What adapter selection needs to know about an adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub vendor: u32,
    pub device_type: wgpu::DeviceType,
    pub backend: wgpu::Backend,
    /// Can present to the window.
    pub surface_ok: bool,
    /// The adapter a failed start used ([`Plan::avoid`]).
    pub avoid: bool,
}

/// Rank of an adapter (lower is better): an adapter to avoid last; high-performance device types
/// first (egui's power preference) or software adapters first for `cpu`; then backends, with DX12
/// before Vulkan for Intel on Windows. The Linux compatible set (`gl`) ranks every hardware adapter
/// before software ones, and OpenGL adapters before Vulkan ones.
fn rank(a: &Candidate, backend: GpuBackend, os: Os) -> (u8, u8, u8, u8, u8) {
    let ty = match a.device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Other => 3,
        wgpu::DeviceType::Cpu => 4,
    };
    let ty = match (backend, a.device_type) {
        (GpuBackend::Cpu, wgpu::DeviceType::Cpu) => 0,
        (GpuBackend::Cpu, _) => ty + 1,
        _ => ty,
    };
    let be = match a.backend {
        wgpu::Backend::Dx12 if os == Os::Windows && a.vendor == INTEL => 0,
        wgpu::Backend::Vulkan | wgpu::Backend::Metal => 1,
        wgpu::Backend::Dx12 => 2,
        wgpu::Backend::Gl => 3,
        wgpu::Backend::BrowserWebGpu | wgpu::Backend::Noop => 4,
    };
    let compatible = backend == GpuBackend::Gl && os == Os::Other;
    let software = u8::from(compatible && a.device_type == wgpu::DeviceType::Cpu);
    let not_gl = u8::from(compatible && a.backend != wgpu::Backend::Gl);
    (u8::from(a.avoid), software, not_gl, ty, be)
}

/// The adapter to use among `adapters` (`None` when there is none).
pub fn pick(adapters: &[Candidate], backend: GpuBackend, os: Os) -> Option<usize> {
    adapters.iter().enumerate().filter(|(_, a)| a.surface_ok).min_by_key(|(i, a)| (rank(a, backend, os), *i)).map(|(i, _)| i)
}

/// Whether picking `chosen` applied the Intel-on-Windows DX12 default (a Vulkan adapter of the
/// same vendor was available).
pub fn intel_dx12_applied(adapters: &[Candidate], chosen: usize, os: Os) -> bool {
    let Some(c) = adapters.get(chosen) else { return false };
    os == Os::Windows
        && c.vendor == INTEL
        && c.backend == wgpu::Backend::Dx12
        && adapters.iter().any(|a| a.vendor == INTEL && a.backend == wgpu::Backend::Vulkan)
}

/// Configure eframe's wgpu setup for `plan`: instance backends, the DX12 shader compiler, adapter
/// selection (Intel on Windows, software adapters for `cpu`) and the marker update once the
/// adapter is chosen. `note` receives a remark for System Info (e.g. the Intel default).
pub fn configure(setup: &mut egui_wgpu::WgpuSetup, plan: &Plan, os: Os, sentinel: SharedSentinel, note: Arc<Mutex<Option<String>>>) {
    if let egui_wgpu::WgpuSetup::CreateNew(create) = setup {
        if let Some(b) = backends(plan, os) {
            create.instance_descriptor.backends = b;
        }
        if os == Os::Windows {
            let exe = std::env::current_exe().ok();
            let env = std::env::var("WGPU_DX12_COMPILER").ok();
            create.instance_descriptor.backend_options.dx12.shader_compiler = dx12_compiler(env.as_deref(), exe.as_deref().and_then(Path::parent));
        }
        let select = plan.env.is_none()
            && (plan.backend == GpuBackend::Cpu
                || (plan.backend == GpuBackend::Auto && os == Os::Windows)
                || (plan.backend == GpuBackend::Gl && os == Os::Other)
                || plan.avoid.is_some());
        if select {
            let backend = plan.backend;
            let avoid = plan.avoid.clone();
            create.native_adapter_selector = Some(Arc::new(move |adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>| {
                let cands: Vec<Candidate> = adapters
                    .iter()
                    .map(|a| {
                        let i = a.get_info();
                        Candidate {
                            vendor: i.vendor,
                            device_type: i.device_type,
                            backend: i.backend,
                            surface_ok: surface.is_none_or(|s| a.is_surface_supported(s)),
                            avoid: avoid.as_ref().is_some_and(|v| v.matches(&i.name, photocraft_ui_egui::gpu_canvas::backend_name(i.backend))),
                        }
                    })
                    .collect();
                for (a, c) in adapters.iter().zip(&cands) {
                    let i = a.get_info();
                    log::info!(
                        "graphics adapter: {} ({:?}, {:?}){}{}",
                        i.name,
                        i.backend,
                        i.device_type,
                        if c.surface_ok { "" } else { ", can't present to the window" },
                        if c.avoid { ", failed in an earlier start" } else { "" }
                    );
                }
                let i = pick(&cands, backend, os).ok_or_else(|| "no graphics adapter can present to the window".to_string())?;
                log::info!("GPU startup: chose {}", adapters.get(i).map(|a| a.get_info().name).unwrap_or_default());
                if backend == GpuBackend::Cpu && cands.get(i).is_some_and(|c| c.device_type != wgpu::DeviceType::Cpu) {
                    *note.lock().unwrap_or_else(PoisonError::into_inner) =
                        Some("CPU image rendering; no software graphics adapter is available, so the window uses hardware graphics".into());
                }
                if backend == GpuBackend::Auto && intel_dx12_applied(&cands, i, os) {
                    *note.lock().unwrap_or_else(PoisonError::into_inner) =
                        Some("Intel graphics on Windows: using DirectX 12 (the Intel Vulkan driver is known to crash)".into());
                }
                adapters.get(i).cloned().ok_or_else(|| "no graphics adapter found".to_string())
            }));
        }
    }
    photocraft_ui_egui::gpu_canvas::use_adapter_limits_with(setup, move |adapter| {
        let info = adapter.get_info();
        let mut guard = sentinel.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(s) = guard.as_mut() {
            let mut m = s.marker.clone();
            m.adapter = info.name.clone();
            m.adapter_backend = photocraft_ui_egui::gpu_canvas::backend_name(info.backend).to_string();
            m.driver = [info.driver.as_str(), info.driver_info.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ");
            if let Err(e) = s.write(m) {
                log::warn!("GPU startup marker: {e}");
            }
        }
    });
}

/// What a previous start left behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Previous {
    /// Nothing: it finished starting (or never ran).
    Clean,
    /// Its marker, unlocked: it died before rendering its first frames.
    Crashed(Marker),
    /// Another instance is starting right now (it holds the lock).
    Busy,
}

impl Previous {
    pub fn crashed(&self) -> Option<&Marker> {
        match self {
            Previous::Crashed(m) => Some(m),
            _ => None,
        }
    }
}

/// The locked marker file of this start.
#[derive(Debug)]
pub struct Sentinel {
    path: PathBuf,
    file: Option<File>,
    marker: Marker,
}

/// The sentinel, shared with eframe's adapter hook and the app's started hook.
pub type SharedSentinel = Arc<Mutex<Option<Sentinel>>>;

impl Sentinel {
    /// Open (creating) the marker in `dir` and lock it. Returns what a previous start left and,
    /// unless another instance holds the lock or the directory isn't writable, this start's
    /// sentinel (not yet written: see [`Sentinel::write`]).
    pub fn begin(dir: &Path) -> (Previous, Option<Sentinel>) {
        let path = dir.join(MARKER_FILE);
        if let Err(e) = std::fs::create_dir_all(dir) {
            log::warn!("GPU startup marker: {e}");
            return (Previous::Clean, None);
        }
        let mut file = match OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("GPU startup marker {}: {e}", path.display());
                return (Previous::Clean, None);
            }
        };
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return (Previous::Busy, None),
            Err(std::fs::TryLockError::Error(e)) => {
                log::warn!("GPU startup marker lock: {e}");
                return (Previous::Clean, None);
            }
        }
        let mut text = String::new();
        let _ = (&mut file).take(MARKER_MAX).read_to_string(&mut text);
        let previous = if text.trim().is_empty() { Previous::Clean } else { Previous::Crashed(Marker::parse(&text)) };
        (previous, Some(Sentinel { path, file: Some(file), marker: Marker::default() }))
    }

    /// Replace the marker's contents with `m`.
    pub fn write(&mut self, m: Marker) -> std::io::Result<()> {
        let f = self.file.as_mut().ok_or_else(|| std::io::Error::other("marker closed"))?;
        f.set_len(0)?;
        f.seek(SeekFrom::Start(0))?;
        f.write_all(m.to_json().as_bytes())?;
        f.flush()?;
        self.marker = m;
        Ok(())
    }

    /// The start succeeded: clear the marker and release the lock.
    pub fn finish(mut self) {
        if let Some(f) = self.file.take() {
            // Empty means "no crash" even if removing fails (another instance has it open).
            let _ = f.set_len(0);
            drop(f);
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Read the policy leniently at startup, including old settings without renderingMode.
pub fn read_rendering_prefs(path: Option<&Path>) -> (GpuBackend, RenderingMode) {
    let v: Value = path.and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let perf = v.get("performance");
    let backend = perf.and_then(|p| p.get("gpuBackend")).and_then(Value::as_str).and_then(GpuBackend::parse).unwrap_or_default();
    let use_gpu = perf.and_then(|p| p.get("useGpu")).and_then(Value::as_bool).unwrap_or(true);
    let explicit = perf.and_then(|p| p.get("renderingMode")).and_then(Value::as_str).and_then(RenderingMode::parse);
    let mode = explicit.unwrap_or(if !use_gpu || backend == GpuBackend::Cpu { RenderingMode::Cpu } else { RenderingMode::Auto });
    (backend, mode)
}

/// Why GPU acceleration is reduced or off after a failure (`gpu-recovery.json` in the config
/// directory), written once the fallback start succeeded. While it applies ([`Recovery::applies`])
/// the UI keeps saying why the app is in CPU mode, and the Linux compatible set keeps avoiding the
/// adapter that failed. Choosing GPU or Automatic again (Retry GPU) makes it stale; the next
/// launch removes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recovery {
    pub reason: String,
    pub avoid: Option<Avoid>,
}

impl Recovery {
    pub fn to_json(&self) -> String {
        json!({
            "reason": self.reason,
            "avoidAdapter": self.avoid.as_ref().map(|a| a.adapter.as_str()),
            "avoidBackend": self.avoid.as_ref().map(|a| a.backend.as_str()),
        })
        .to_string()
    }

    /// Parse leniently; `None` when there is no reason (nothing to tell the user).
    pub fn parse(text: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(text).ok()?;
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(|s| s.chars().take(1024).collect::<String>());
        let reason = s("reason").filter(|r| !r.trim().is_empty())?;
        let avoid = s("avoidAdapter").filter(|a| !a.is_empty()).map(|adapter| Avoid { adapter, backend: s("avoidBackend").unwrap_or_default() });
        Some(Recovery { reason, avoid })
    }

    pub fn load(dir: &Path) -> Option<Self> {
        let f = File::open(dir.join(RECOVERY_FILE)).ok()?;
        let mut text = String::new();
        f.take(MARKER_MAX).read_to_string(&mut text).ok()?;
        Self::parse(&text)
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(RECOVERY_FILE), self.to_json())
    }

    pub fn clear(dir: &Path) {
        let _ = std::fs::remove_file(dir.join(RECOVERY_FILE));
    }

    /// Whether a saved recovery still describes the preferences: CPU mode, or (Linux) the
    /// compatible GPU set it chose.
    pub fn applies(pref: GpuBackend, mode: RenderingMode, os: Os) -> bool {
        mode == RenderingMode::Cpu || (os == Os::Other && pref == GpuBackend::Gl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use GpuBackend::*;

    fn crashed(backend: &str, adapter_backend: &str) -> Marker {
        Marker { backend: backend.into(), adapter: "Intel(R) UHD Graphics".into(), adapter_backend: adapter_backend.into(), ..Default::default() }
    }

    #[test]
    fn cpu_policy_ignores_hardware_backend_override() {
        let p = plan_with_mode(Vulkan, RenderingMode::Cpu, None, Some("vulkan"), false, Os::Windows);
        assert_eq!(p.backend, Cpu);
        assert!(p.env.is_none());
        assert_eq!(plan_with_mode(Vulkan, RenderingMode::Cpu, Some(&crashed("vulkan", "vulkan")), None, false, Os::Windows).backend, Cpu);
        let p = plan_with_mode(Auto, RenderingMode::Gpu, None, Some("vulkan"), true, Os::Windows);
        assert_eq!(p.backend, Cpu);
        assert!(p.env.is_none());
    }

    #[test]
    fn no_marker_uses_the_preference() {
        for os in [Os::Windows, Os::Mac, Os::Other] {
            let p = plan(Auto, None, None, false, os);
            assert_eq!(p, Plan { backend: Auto, ..Plan::default() });
        }
        assert_eq!(plan(Dx12, None, None, false, Os::Windows).backend, Dx12);
        assert_eq!(plan(Gl, None, None, false, Os::Other).backend, Gl);
        // A backend the platform lacks is `auto`.
        assert_eq!(plan(Metal, None, None, false, Os::Windows).backend, Auto);
        assert_eq!(plan(Dx12, None, None, false, Os::Mac).backend, Auto);
        assert_eq!(plan(Dx12, None, None, false, Os::Other).backend, Auto);
    }

    #[test]
    fn a_crash_moves_down_each_chain_and_is_remembered() {
        // Windows: Vulkan → DX12 → CPU.
        let p = plan(Auto, Some(&crashed("auto", "vulkan")), None, false, Os::Windows);
        assert_eq!((p.backend, p.remember), (Dx12, true));
        assert!(p.reason.as_deref().unwrap_or("").contains("vulkan (Intel(R) UHD Graphics)"), "{p:?}");
        assert_eq!(plan(Dx12, Some(&crashed("dx12", "dx12")), None, false, Os::Windows).backend, Cpu);
        // Crashed before the adapter was chosen (enumeration): from `auto`.
        assert_eq!(plan(Auto, Some(&crashed("auto", "")), None, false, Os::Windows).backend, Dx12);
        // `auto` that had already picked DX12 (the Intel default) goes to the CPU.
        assert_eq!(plan(Auto, Some(&crashed("auto", "dx12")), None, false, Os::Windows).backend, Cpu);
        // Linux: Vulkan → Vulkan again → the compatible GPU set (avoiding that adapter) → CPU.
        let p = plan(Auto, Some(&crashed("auto", "vulkan")), None, false, Os::Other);
        assert_eq!((p.backend, p.attempt, p.remember, p.avoid.is_none()), (Auto, 1, false, true), "{p:?}");
        assert!(p.reason.as_deref().unwrap_or("").contains("once more"), "{p:?}");
        let again = Marker { attempt: 1, ..crashed("auto", "vulkan") };
        let p = plan(Auto, Some(&again), None, false, Os::Other);
        assert_eq!((p.backend, p.attempt, p.remember), (Gl, 0, true));
        assert_eq!(p.avoid, Some(Avoid { adapter: "Intel(R) UHD Graphics".into(), backend: "vulkan".into() }));
        assert_eq!(backends(&p, Os::Other), Some(wgpu::Backends::VULKAN | wgpu::Backends::GL), "never GL alone");
        let vulkan_again = Marker { attempt: 1, ..crashed("vulkan", "vulkan") };
        assert_eq!(plan(Vulkan, Some(&crashed("vulkan", "vulkan")), None, false, Os::Other).backend, Vulkan);
        assert_eq!(plan(Vulkan, Some(&vulkan_again), None, false, Os::Other).backend, Gl);
        assert_eq!(plan(Gl, Some(&crashed("gl", "gl")), None, false, Os::Other).backend, Cpu);
        // `auto` that had picked a GL adapter: retried once, then the CPU.
        assert_eq!(plan(Auto, Some(&crashed("auto", "gl")), None, false, Os::Other).backend, Auto);
        assert_eq!(plan(Auto, Some(&Marker { attempt: 1, ..crashed("auto", "gl") }), None, false, Os::Other).backend, Cpu);
        // Windows and macOS don't retry: the attempt count changes nothing there.
        assert_eq!(plan(Auto, Some(&Marker { attempt: 1, ..crashed("auto", "vulkan") }), None, false, Os::Windows).backend, Dx12);
        // macOS: Metal → CPU.
        assert_eq!(plan(Auto, Some(&crashed("auto", "metal")), None, false, Os::Mac).backend, Cpu);
        // The CPU path stays there, and isn't "remembered" again.
        let p = plan(Cpu, Some(&crashed("cpu", "dx12")), None, false, Os::Windows);
        assert_eq!((p.backend, p.remember), (Cpu, false));
        // An unreadable marker still counts as a crash.
        assert_eq!(plan(Auto, Some(&Marker::parse("{garbage")), None, false, Os::Windows).backend, Dx12);
    }

    #[test]
    fn env_override_beats_everything() {
        let p = plan(Cpu, Some(&crashed("auto", "vulkan")), Some("vulkan"), false, Os::Windows);
        assert_eq!(p.env.as_deref(), Some("vulkan"));
        assert_eq!(p.backend, Auto);
        assert!(!p.remember);
        assert_eq!(backends(&p, Os::Windows), None, "egui reads WGPU_BACKEND itself");
        // A crash under the override doesn't count against `auto` later.
        let p = plan(Auto, Some(&Marker { backend: "env:vulkan".into(), ..Default::default() }), None, false, Os::Windows);
        assert_eq!(p.backend, Auto);
        // Empty means unset.
        assert_eq!(plan(Auto, None, Some("  "), false, Os::Windows).env, None);
        // --safe-gpu with the override: CPU canvas, the override's backends.
        assert_eq!(plan(Auto, None, Some("dx12"), true, Os::Windows).backend, Cpu);
    }

    #[test]
    fn safe_gpu_forces_the_cpu_for_one_launch() {
        let p = plan(Vulkan, Some(&crashed("vulkan", "vulkan")), None, true, Os::Other);
        assert_eq!((p.backend, p.remember), (Cpu, false));
        assert_eq!(backends(&p, Os::Windows), Some(wgpu::Backends::DX12));
        assert_eq!(backends(&p, Os::Other), Some(wgpu::Backends::VULKAN | wgpu::Backends::GL));
        assert_eq!(backends(&p, Os::Mac), Some(wgpu::Backends::METAL));
    }

    fn cand(vendor: u32, device_type: wgpu::DeviceType, backend: wgpu::Backend) -> Candidate {
        Candidate { vendor, device_type, backend, surface_ok: true, avoid: false }
    }

    #[test]
    fn intel_on_windows_defaults_to_dx12() {
        use wgpu::{Backend as B, DeviceType as T};
        let intel = [cand(INTEL, T::IntegratedGpu, B::Vulkan), cand(INTEL, T::IntegratedGpu, B::Dx12), cand(INTEL, T::IntegratedGpu, B::Gl)];
        let i = pick(&intel, Auto, Os::Windows).unwrap();
        assert_eq!(intel[i].backend, B::Dx12);
        assert!(intel_dx12_applied(&intel, i, Os::Windows));
        // Elsewhere (and for other vendors) Vulkan stays first.
        assert_eq!(intel[pick(&intel, Auto, Os::Other).unwrap()].backend, B::Vulkan);
        let nv = [cand(0x10de, T::DiscreteGpu, B::Dx12), cand(0x10de, T::DiscreteGpu, B::Vulkan)];
        let i = pick(&nv, Auto, Os::Windows).unwrap();
        assert_eq!(nv[i].backend, B::Vulkan);
        assert!(!intel_dx12_applied(&nv, i, Os::Windows));
        // Intel iGPU + discrete GPU: the discrete one wins (high performance), on Vulkan.
        let hybrid = [intel[0], intel[1], nv[1]];
        assert_eq!(pick(&hybrid, Auto, Os::Windows), Some(2));
        // Adapters that can't present lose to ones that can.
        let mut only = [cand(0x10de, T::DiscreteGpu, B::Vulkan), intel[1]];
        only[0].surface_ok = false;
        assert_eq!(pick(&only, Auto, Os::Windows), Some(1));
        assert_eq!(pick(&[], Auto, Os::Windows), None);
    }

    #[test]
    fn cpu_prefers_software_adapters() {
        use wgpu::{Backend as B, DeviceType as T};
        let a = [cand(INTEL, T::IntegratedGpu, B::Dx12), cand(0x1414, T::Cpu, B::Dx12)];
        assert_eq!(pick(&a, Cpu, Os::Windows), Some(1));
        // No software adapter: still something.
        assert_eq!(pick(&a[..1], Cpu, Os::Windows), Some(0));
    }

    #[test]
    fn adapters_must_present_even_when_none_support_the_surface() {
        let mut a = cand(INTEL, wgpu::DeviceType::IntegratedGpu, wgpu::Backend::Dx12);
        a.surface_ok = false;
        assert_eq!(pick(&[a], Auto, Os::Windows), None);
        assert_eq!(pick(&[a], Cpu, Os::Windows), None);
    }

    #[test]
    fn explicit_policy_overrides_legacy_cpu_backend() {
        assert_eq!(plan_with_mode(Cpu, RenderingMode::Gpu, None, None, false, Os::Mac).backend, Auto);
        assert_eq!(plan_with_mode(Metal, RenderingMode::Cpu, None, None, false, Os::Mac).backend, Cpu);
        assert_eq!(plan_with_mode(Auto, RenderingMode::Gpu, None, None, true, Os::Mac).backend, Cpu);
    }

    #[test]
    fn marker_round_trip_and_lenient_parse() {
        let m = Marker {
            backend: "auto".into(),
            adapter: "A \"quoted\" GPU".into(),
            adapter_backend: "vulkan".into(),
            driver: "1.2".into(),
            version: "0.1".into(),
            attempt: 1,
        };
        assert_eq!(Marker::parse(&m.to_json()), m);
        assert_eq!(Marker::parse(r#"{"attempt": "x"}"#).attempt, 0);
        assert_eq!(Marker::parse(""), Marker::default());
        assert_eq!(Marker::parse("[1,2]").tried(), Some(Auto));
        assert_eq!(Marker::parse(r#"{"backend": 7}"#).tried(), Some(Auto));
        assert_eq!(Marker::parse(r#"{"backend": "env:dx12"}"#).tried(), None);
        let long = format!(r#"{{"adapter": "{}"}}"#, "x".repeat(10_000));
        assert_eq!(Marker::parse(&long).adapter.len(), 256);
    }

    /// The reported failure (Wayland + NVIDIA): the fallback after an unfinished Vulkan start was
    /// GL alone, which can't present there, so the app ended up on the CPU renderer.
    #[test]
    fn linux_compatible_set_presents_and_skips_the_failed_adapter() {
        use wgpu::{Backend as B, DeviceType as T};
        let compat = Plan { backend: Gl, ..Plan::default() };
        assert_eq!(backends(&compat, Os::Other), Some(wgpu::Backends::VULKAN | wgpu::Backends::GL));
        assert_eq!(backends(&compat, Os::Windows), Some(wgpu::Backends::GL), "other platforms unchanged");
        const NV: u32 = 0x10de;
        const AMD: u32 = 0x1002;
        let mut nv_vk = cand(NV, T::DiscreteGpu, B::Vulkan);
        let amd_vk = cand(AMD, T::IntegratedGpu, B::Vulkan);
        let llvmpipe = cand(0x10005, T::Cpu, B::Gl);
        let mut nv_gl = cand(NV, T::DiscreteGpu, B::Gl);
        // GL can't present to this Wayland surface; the NVIDIA Vulkan adapter failed last time.
        nv_gl.surface_ok = false;
        nv_vk.avoid = true;
        let machine = [nv_vk, amd_vk, llvmpipe, nv_gl];
        assert_eq!(pick(&machine, Gl, Os::Other), Some(1), "the other hardware adapter, not software");
        // Nothing to avoid: still a hardware adapter before llvmpipe.
        let fresh = [cand(NV, T::DiscreteGpu, B::Vulkan), amd_vk, llvmpipe, nv_gl];
        assert_eq!(pick(&fresh, Gl, Os::Other), Some(0));
        // Where hardware GL presents, the compatible set uses it.
        let gl_ok = [cand(NV, T::DiscreteGpu, B::Vulkan), cand(NV, T::DiscreteGpu, B::Gl), llvmpipe];
        assert_eq!(pick(&gl_ok, Gl, Os::Other), Some(1));
        // Only the failed adapter and software left: software, rather than the same failure.
        assert_eq!(pick(&[nv_vk, llvmpipe], Gl, Os::Other), Some(1));
        // `Avoid` matches name and backend.
        let a = Avoid { adapter: "NVIDIA GeForce RTX 5090".into(), backend: "vulkan".into() };
        assert!(a.matches("NVIDIA GeForce RTX 5090", "vulkan"));
        assert!(!a.matches("NVIDIA GeForce RTX 5090", "gl"));
        assert!(Avoid { backend: String::new(), ..a.clone() }.matches("NVIDIA GeForce RTX 5090", "gl"));
    }

    #[test]
    fn renderer_init_failure_tries_the_compatible_set_before_the_cpu_on_linux() {
        let auto = plan(Auto, None, None, false, Os::Other);
        assert_eq!(after_init_failure(&auto, Os::Other), Some(Gl));
        assert_eq!(after_init_failure(&plan(Vulkan, None, None, false, Os::Other), Os::Other), Some(Gl));
        // Already compatible, the CPU, an override, or another platform: the CPU renderer next.
        assert_eq!(after_init_failure(&plan(Gl, None, None, false, Os::Other), Os::Other), None);
        assert_eq!(after_init_failure(&plan(Auto, None, None, true, Os::Other), Os::Other), None);
        assert_eq!(after_init_failure(&plan(Auto, None, Some("vulkan"), false, Os::Other), Os::Other), None);
        assert_eq!(after_init_failure(&plan(Auto, None, None, false, Os::Windows), Os::Windows), None);
        assert_eq!(after_init_failure(&plan(Auto, None, None, false, Os::Mac), Os::Mac), None);

        // The re-launched process.
        let p = with_init_retry(auto.clone(), Some("gl"), Some("WGPU error: No suitable graphics adapter found"), Os::Other);
        assert_eq!((p.backend, p.remember, p.env.as_deref()), (Gl, true, None));
        assert!(p.reason.as_deref().unwrap_or("").contains("No suitable graphics adapter"), "{p:?}");
        assert_eq!(with_init_retry(auto.clone(), None, None, Os::Other), auto);
        assert_eq!(with_init_retry(auto.clone(), Some("quantum"), None, Os::Other), auto);
        assert_eq!(with_init_retry(auto.clone(), Some("cpu"), None, Os::Other), auto);
        let safe = plan(Auto, None, None, true, Os::Other);
        assert_eq!(with_init_retry(safe.clone(), Some("gl"), None, Os::Other), safe);
        let env = plan(Auto, None, Some("vulkan"), false, Os::Other);
        assert_eq!(with_init_retry(env.clone(), Some("gl"), None, Os::Other), env);
    }

    #[test]
    fn recovery_round_trips_and_applies_while_the_fallback_is_in_use() {
        let dir = temp_dir("recovery");
        assert_eq!(Recovery::load(&dir), None);
        let r = Recovery { reason: "WGPU error: \"none\"".into(), avoid: Some(Avoid { adapter: "NVIDIA".into(), backend: "vulkan".into() }) };
        r.save(&dir).unwrap();
        assert_eq!(Recovery::load(&dir), Some(r.clone()));
        Recovery::clear(&dir);
        assert_eq!(Recovery::load(&dir), None);
        assert_eq!(Recovery::parse(r#"{"reason": "  "}"#), None, "nothing to say");
        assert_eq!(Recovery::parse("{oops"), None);
        assert_eq!(Recovery::parse(r#"{"reason": "x", "avoidAdapter": ""}"#), Some(Recovery { reason: "x".into(), avoid: None }));

        assert!(Recovery::applies(Auto, RenderingMode::Cpu, Os::Windows));
        assert!(Recovery::applies(Gl, RenderingMode::Gpu, Os::Other));
        assert!(!Recovery::applies(Auto, RenderingMode::Gpu, Os::Other), "Retry GPU makes it stale");
        assert!(!Recovery::applies(Gl, RenderingMode::Auto, Os::Windows));

        // The compatible set remembered in the preferences keeps avoiding the adapter.
        let p = with_recovery(plan(Gl, None, None, false, Os::Other), Some(&r), Os::Other);
        assert_eq!(p.avoid, r.avoid);
        assert_eq!(with_recovery(plan(Auto, None, None, false, Os::Other), Some(&r), Os::Other).avoid, None);
        assert_eq!(with_recovery(plan(Gl, None, None, false, Os::Windows), Some(&r), Os::Windows).avoid, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("photocraft-gpu-startup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn sentinel_detects_a_crash_and_clears_on_success() {
        let dir = temp_dir("cycle");
        // First launch: nothing left behind.
        let (prev, s) = Sentinel::begin(&dir);
        assert_eq!(prev, Previous::Clean);
        let mut s = s.expect("sentinel");
        s.write(Marker { backend: "auto".into(), ..Default::default() }).unwrap();
        s.write(Marker { backend: "auto".into(), adapter: "GPU".into(), adapter_backend: "vulkan".into(), ..Default::default() }).unwrap();
        // While it is starting, a second instance sees it busy, not crashed.
        let (busy, none) = Sentinel::begin(&dir);
        assert_eq!(busy, Previous::Busy);
        assert!(none.is_none());
        // The process dies in the driver: the lock goes, the marker stays.
        drop(s);
        // Another test in this binary spawns a process: between its fork and exec the child briefly
        // shares this file's flock, so `begin` can see the lock as busy. Retry for a moment.
        let (mut prev, mut s) = Sentinel::begin(&dir);
        for _ in 0..50 {
            if prev != Previous::Busy {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            (prev, s) = Sentinel::begin(&dir);
        }
        let m = prev.crashed().cloned().expect("crash detected");
        assert_eq!((m.adapter.as_str(), m.tried()), ("GPU", Some(Vulkan)));
        // This start succeeds: the next one finds nothing.
        let mut s = s.expect("sentinel");
        s.write(Marker { backend: "dx12".into(), ..Default::default() }).unwrap();
        s.finish();
        assert!(!dir.join(MARKER_FILE).exists());
        let (prev, _s) = Sentinel::begin(&dir);
        assert_eq!(prev, Previous::Clean);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unwritable_config_dir_is_not_fatal() {
        let dir = temp_dir("file");
        std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
        std::fs::write(&dir, b"not a directory").unwrap();
        let (prev, s) = Sentinel::begin(&dir.join("sub"));
        assert_eq!(prev, Previous::Clean);
        assert!(s.is_none());
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn prefs_are_read_leniently() {
        let dir = temp_dir("prefs");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("preferences.json");
        assert_eq!(read_rendering_prefs(None), (Auto, RenderingMode::Auto));
        assert_eq!(read_rendering_prefs(Some(&p)), (Auto, RenderingMode::Auto));
        std::fs::write(&p, r#"{"performance": {"gpuBackend": "dx12", "useGpu": false}, "interface": {}}"#).unwrap();
        assert_eq!(read_rendering_prefs(Some(&p)), (Dx12, RenderingMode::Cpu));
        std::fs::write(&p, r#"{"performance": {"gpuBackend": "quantum"}}"#).unwrap();
        assert_eq!(read_rendering_prefs(Some(&p)), (Auto, RenderingMode::Auto));
        std::fs::write(&p, r#"{"performance": {"useGpu": false, "renderingMode": "gpu"}}"#).unwrap();
        assert_eq!(read_rendering_prefs(Some(&p)), (Auto, RenderingMode::Gpu));
        std::fs::write(&p, "{not json").unwrap();
        assert_eq!(read_rendering_prefs(Some(&p)), (Auto, RenderingMode::Auto));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #712: never a `dxcompiler.dll` found through the DLL search path (current directory, PATH).
    #[test]
    fn dx12_compiler_is_fxc_or_dxc_beside_the_exe() {
        use wgpu::Dx12Compiler as C;
        let dir = temp_dir("dxc");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(matches!(dx12_compiler(None, None), C::Fxc));
        assert!(matches!(dx12_compiler(None, Some(&dir)), C::Fxc));
        std::fs::write(dir.join("dxcompiler.dll"), b"MZ").unwrap();
        let expected = dir.join("dxcompiler.dll");
        assert!(matches!(dx12_compiler(None, Some(&dir)), C::DynamicDxc { dxc_path } if Path::new(&dxc_path) == expected));
        // An explicit WGPU_DX12_COMPILER wins; an unknown value is ignored.
        assert!(matches!(dx12_compiler(Some(" FXC "), Some(&dir)), C::Fxc));
        assert!(matches!(dx12_compiler(Some("dxc"), None), C::DynamicDxc { dxc_path } if dxc_path == "dxcompiler.dll"));
        assert!(matches!(dx12_compiler(Some("quantum"), None), C::Fxc));
        let _ = std::fs::remove_dir_all(&dir);

        // The app's setup replaces wgpu's default (`Auto`: DXC by name from the search path).
        let mut setup = egui_wgpu::WgpuSetup::without_display_handle();
        let plan = plan(Auto, None, None, false, Os::Windows);
        configure(&mut setup, &plan, Os::Windows, Arc::new(Mutex::new(None)), Arc::default());
        let egui_wgpu::WgpuSetup::CreateNew(create) = &setup else { panic!("a new instance") };
        if std::env::var_os("WGPU_DX12_COMPILER").is_none() {
            let safe = match &create.instance_descriptor.backend_options.dx12.shader_compiler {
                C::Fxc => true,
                C::DynamicDxc { dxc_path } => Path::new(dxc_path).is_absolute(),
                C::StaticDxc | C::Auto => false,
            };
            assert!(safe, "{:?}", create.instance_descriptor.backend_options.dx12.shader_compiler);
        }
    }
}
