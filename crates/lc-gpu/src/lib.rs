//! The LightCraft develop pipeline on the GPU (wgpu compute, WGSL kernels).
//!
//! The CPU pipeline (`lightcraft-pipeline`) is the reference: every kernel here is a port of a CPU
//! stage, both read the same resolved parameters ([`lightcraft_pipeline::Plan`],
//! [`lightcraft_pipeline::finish::FinishParams`]), and the equivalence tests (`tests/`) render the
//! same settings on both and bound the difference in 8-bit previews and 16-bit output. Stages
//! without a kernel run on the CPU inside the same render (per-stage hybrid); see `docs/gpu-pipeline.md`.
//!
//! Which backends wgpu may load (DX12 only on Windows; `LIGHTCRAFT_GPU_BACKEND`, `WGPU_BACKEND`) and
//! the crash sentinel around device creation: [`backend`] (issue #136).
//!
//! Use [`render`]: it returns `None` when the GPU is unavailable, disabled (`LIGHTCRAFT_GPU=0`,
//! `LIGHTCRAFT_GPU_BACKEND=off` or [`set_enabled`]), the render does not fit the device, or the device reported an error, ran out
//! of memory or returned an incomplete image — callers then render on the CPU. [`unavailable_reason`]
//! and [`last_fallback`] say why (`ui.inspect` → `perf.gpuReason` / `perf.gpuFallback`).
//! The browser build has no GPU path yet (WebGPU device creation is asynchronous): everything here
//! compiles to the CPU fallback on wasm32.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::{RenderRequest, Rendered, SourceInfo, StageCache};
use lightcraft_raster::Rgb32f;

#[cfg(not(target_arch = "wasm32"))]
pub mod backend;
#[cfg(not(target_arch = "wasm32"))]
mod capture;
#[cfg(not(target_arch = "wasm32"))]
mod ctx;
#[cfg(not(target_arch = "wasm32"))]
pub mod denoise;
#[cfg(not(target_arch = "wasm32"))]
mod params;
#[cfg(not(target_arch = "wasm32"))]
mod primary;
#[cfg(not(target_arch = "wasm32"))]
mod render;

#[cfg(not(target_arch = "wasm32"))]
pub use render::GpuStages;

static ENABLED: AtomicBool = AtomicBool::new(true);
#[cfg(not(target_arch = "wasm32"))]
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The latest render that fell back to the CPU, and why.
static LAST_FALLBACK: Mutex<Option<String>> = Mutex::new(None);

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn record_fallback(reason: String) {
    *LAST_FALLBACK.lock().unwrap_or_else(|e| e.into_inner()) = Some(reason);
}

/// Why renders do not use the GPU, or `None` when they do (or will, once the device is created).
/// E.g. "disabled by LIGHTCRAFT_GPU=0", "software adapter (llvmpipe …) skipped", "device lost …".
/// Never blocks.
pub fn unavailable_reason() -> Option<String> {
    if env_disabled() {
        #[cfg(not(target_arch = "wasm32"))]
        if backend::env_off() {
            return Some("disabled by LIGHTCRAFT_GPU_BACKEND=off".into());
        }
        return Some("disabled by LIGHTCRAFT_GPU=0".into());
    }
    if !ENABLED.load(Ordering::Relaxed) {
        return Some("disabled by the GPU rendering preference (app.gpu)".into());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        match GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
            Some(Err(e)) => Some(e.reason.clone()),
            Some(Ok(gpu)) if gpu.health.needs_retry() => Some(format!(
                "waiting to retry after a GPU failure: {}",
                gpu.health.fault().map_or_else(|| gpu.health.last_error().unwrap_or_default(), |f| f.to_string())
            )),
            _ => None,
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        Some("the browser build has no GPU path yet".into())
    }
}

/// A device failure awaiting a fresh context (not a user/environment GPU-off preference).
pub fn recovering() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        health_state().is_some_and(|h| h.needs_retry())
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}

/// The latest render that fell back from the GPU to the CPU and why (device limit, out of memory,
/// device error, incomplete or blank result), if any.
pub fn last_fallback() -> Option<String> {
    LAST_FALLBACK.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// A simulated GPU failure for the next [`render`] on this thread (tests of the CPU fallback).
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// A command the device rejects (validation error).
    Validation,
    /// The per-pixel stage's work never runs (what a driver that silently drops or resets a
    /// submission looks like: the readback holds no image).
    DropWork,
    /// A storage-buffer limit of this many bytes for this render.
    Limit(u64),
}

thread_local! {
    static CPU_STAGES: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
    static FAULT: std::cell::Cell<Option<Fault>> = const { std::cell::Cell::new(None) };
}

/// Host pixel/statistical stages used inside the most recent GPU render on this thread.
/// Metadata resolution, output histogram and requested overlays are not pixel processing stages.
pub fn last_cpu_stages() -> Vec<&'static str> {
    CPU_STAGES.with(|stages| stages.borrow().clone())
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn record_cpu_stage(stage: &'static str) {
    CPU_STAGES.with(|stages| {
        let mut stages = stages.borrow_mut();
        if !stages.contains(&stage) {
            stages.push(stage);
        }
    });
}

/// Make the next [`render`] on this thread fail with `f` (tests).
#[doc(hidden)]
pub fn inject_fault(f: Fault) {
    FAULT.with(|c| c.set(Some(f)));
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn take_fault() -> Option<Fault> {
    FAULT.with(|c| c.take())
}

/// Use the GPU again after a failure (tests that inject faults).
#[doc(hidden)]
pub fn reset_failures() {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(h) = health_state() {
        h.retry_now();
    }
    *LAST_FALLBACK.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Allow or forbid GPU rendering at runtime (a preference). `LIGHTCRAFT_GPU=0` forbids it for the
/// whole process regardless.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Whether GPU rendering is allowed (environment, preference, no earlier failure).
pub fn enabled() -> bool {
    let allowed = ENABLED.load(Ordering::Relaxed) && !env_disabled();
    #[cfg(not(target_arch = "wasm32"))]
    {
        allowed
            && health_state().is_none_or(|h| {
                !h.needs_retry()
                    || (!lightcraft_sysmem::gpu::pressured() && h.retry_ready(std::time::Instant::now(), Some(lightcraft_sysmem::gpu::idle_epoch())))
            })
    }
    #[cfg(target_arch = "wasm32")]
    {
        allowed
    }
}

/// `LIGHTCRAFT_GPU=0` (or `LIGHTCRAFT_GPU_BACKEND=off`): no GPU for the whole process.
fn env_disabled() -> bool {
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OFF.get_or_init(|| {
        let off = std::env::var("LIGHTCRAFT_GPU").is_ok_and(|v| matches!(v.trim(), "0" | "off" | "false" | "no"));
        #[cfg(not(target_arch = "wasm32"))]
        let off = off || backend::env_off();
        off
    })
}

#[cfg(not(target_arch = "wasm32"))]
static GPU: Mutex<Option<Result<Arc<ctx::Gpu>, InitFailure>>> = Mutex::new(None);
#[cfg(not(target_arch = "wasm32"))]
struct InitFailure {
    reason: String,
    health: photocraft_gpu::DeviceHealth,
}

#[cfg(not(target_arch = "wasm32"))]
fn health_state() -> Option<photocraft_gpu::DeviceHealth> {
    GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref().map(|r| match r {
        Ok(gpu) => gpu.health.clone(),
        Err(error) => error.health.clone(),
    })
}

/// None means initialization/replacement is needed; Some(None) means backoff/VRAM blocks it.
#[cfg(not(target_arch = "wasm32"))]
fn cached_device() -> Option<Option<Arc<ctx::Gpu>>> {
    let cache = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let result = cache.as_ref()?;
    let health = match result {
        Ok(gpu) => &gpu.health,
        Err(error) => &error.health,
    };
    if !health.needs_retry() {
        return Some(result.as_ref().ok().cloned());
    }
    let now = std::time::Instant::now();
    let epoch = Some(lightcraft_sysmem::gpu::idle_epoch());
    if lightcraft_sysmem::gpu::pressured() || !health.retry_ready(now, epoch) {
        return Some(None);
    }
    if health.resume_retry(now, epoch) {
        return Some(result.as_ref().ok().cloned());
    }
    None
}
#[cfg(not(target_arch = "wasm32"))]
static GPU_INIT: Mutex<()> = Mutex::new(());
#[cfg(not(target_arch = "wasm32"))]
static POOL_REQUESTED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(ctx::DEFAULT_POOL_BYTES);

#[cfg(all(test, not(target_arch = "wasm32")))]
static TEST_INIT_LOCK: Mutex<()> = Mutex::new(());

/// Unit tests serialize creation with the crash-marker test's global configuration.
#[cfg(all(test, not(target_arch = "wasm32")))]
fn test_device() -> Option<TestGpu> {
    let lock = TEST_INIT_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    device().map(|gpu| TestGpu { gpu, _lock: lock })
}

/// Keep initialization, renders and native device destruction inside one test
/// lock. Fields drop in order: the final device lease must drop before the lock.
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) struct TestGpu {
    gpu: Arc<ctx::Gpu>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(all(test, not(target_arch = "wasm32")))]
impl std::ops::Deref for TestGpu {
    type Target = ctx::Gpu;
    fn deref(&self) -> &Self::Target {
        &self.gpu
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
impl Drop for TestGpu {
    fn drop(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let finished = shutdown(std::time::Duration::from_secs(120))
            && self
                .gpu
                .device
                .poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(deadline.saturating_duration_since(std::time::Instant::now())) })
                .is_ok();
        assert!(finished || std::thread::panicking(), "GPU test teardown did not finish");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn device() -> Option<Arc<ctx::Gpu>> {
    // turned off (preference, environment): don't even load the driver (issue #136)
    if env_disabled() || !ENABLED.load(Ordering::Relaxed) {
        return existing_device();
    }
    if let Some(result) = cached_device() {
        return result;
    }
    let _work = ctx::InitWork::new();
    let _init = GPU_INIT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(result) = cached_device() {
        return result;
    }
    let health = health_state().map_or_else(photocraft_gpu::DeviceHealth::new, |previous| previous.replacement());
    let old = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    GENERATION.fetch_add(1, Ordering::AcqRel);
    drop(old);
    let result = (|| {
        let Some(backends) = backend::compute_backends() else { return Err("disabled by LIGHTCRAFT_GPU_BACKEND=off".into()) };
        backend::with_init_marker(backends, || {
            std::panic::catch_unwind(|| ctx::Gpu::new(backends, health.clone())).unwrap_or_else(|_| Err("device creation panicked".into()))
        })
    })()
    .map(Arc::new)
    .map_err(|reason| {
        health.mark(photocraft_gpu::Fault::Error(reason.clone()));
        InitFailure { reason, health }
    });
    let gpu = result.as_ref().ok().cloned();
    *GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(result);
    gpu
}

/// The device if it has been created (never creates it).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn existing_device() -> Option<Arc<ctx::Gpu>> {
    GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref().and_then(|g| g.as_ref().ok()).cloned()
}

/// Create the device and compile the kernels on a background thread now (once the window is up),
/// so the first render doesn't wait ~0.3–0.4 s for it and the UI thread never does. Does nothing
/// while GPU rendering is turned off.
pub fn warm_up() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Several views/tests can request warm-up before the cache publishes the device.
        // Spawn only one compiler thread, and include it in the exit barrier from the moment
        // it is requested (not only once a render starts or the worker is scheduled).
        static STARTED: AtomicBool = AtomicBool::new(false);
        if !env_disabled()
            && ENABLED.load(Ordering::Relaxed)
            && !ready()
            && STARTED.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok()
        {
            struct Started(&'static AtomicBool);
            impl Drop for Started {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Release);
                }
            }
            let started = Started(&STARTED);
            let _ = spawn_init_worker(move || {
                let _started = started;
                let _ = device();
            });
        }
    }
}

/// The counted waiter never touches the driver. Its compiler child is fully joined (including
/// thread-local destruction) before InitWork ends, so the timed exit barrier covers teardown
/// without entering an unbounded join on the exiting UI thread.
#[cfg(not(target_arch = "wasm32"))]
fn spawn_init_worker(init: impl FnOnce() + Send + 'static) -> std::io::Result<std::thread::JoinHandle<()>> {
    let work = ctx::InitWork::new();
    std::thread::Builder::new().name("lc-gpu-init".into()).spawn(move || {
        let _work = work;
        if let Ok(compiler) = std::thread::Builder::new().name("lc-gpu-compile".into()).spawn(init) {
            let _ = compiler.join();
        }
    })
}

/// Has device creation finished (successfully or not)? Never blocks — for status displays.
pub fn ready() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_disabled() || GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_some()
    }
    #[cfg(target_arch = "wasm32")]
    {
        true
    }
}

/// Wait (up to `timeout`) for background device initialization, every running GPU render and
/// the device's queue; false if work remains or polling fails. Stop scheduling work first,
/// then call it before the process (or a test) exits:
/// NVIDIA's driver crashes (SIGSEGV in `libnvidia-glcore`, GPU Xid 13 "Illegal Instruction
/// Encoding") when it is torn down while a background thread is still recording or submitting.
pub fn quiesce(timeout: std::time::Duration) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        ctx::quiesce(timeout)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = timeout;
        true
    }
}

/// Finish work and release the cached compute context before native exit
/// handlers unload the driver. Stop scheduling work and drop render caches and
/// denoisers first. Outstanding leases/resources stay valid until their owners
/// drop them; callers must release those too to complete native teardown.
/// Unlike [`quiesce`], this discards compiled kernels; a later use initializes
/// a new context. A timeout leaves the cache intact.
pub fn shutdown(timeout: std::time::Duration) -> bool {
    if !quiesce(timeout) {
        return false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _init = GPU_INIT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let gpu = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        GENERATION.fetch_add(1, Ordering::AcqRel);
        // Dropping buffers may consult the cache: never destroy the context
        // while holding the cache mutex.
        drop(gpu);
    }
    true
}

/// Whether a usable GPU adapter exists and GPU rendering is enabled (creates the device on first
/// use).
pub fn available() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        enabled() && device().is_some()
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}

/// The adapter's name and backend, e.g. "Apple M4 Pro (Metal)".
pub fn adapter_name() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        device().map(|g| format!("{} ({:?})", g.info.name, g.info.backend))
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// Device buffers held by the renderer.
#[cfg(not(target_arch = "wasm32"))]
pub use ctx::GpuMemory;

/// Device buffers held by the renderer.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuMemory {
    pub allocated: u64,
    pub pooled: u64,
    pub retired: u64,
}

/// Device memory held by the renderer's buffers (zero without a device).
pub fn memory() -> GpuMemory {
    #[cfg(not(target_arch = "wasm32"))]
    {
        ctx::memory()
    }
    #[cfg(target_arch = "wasm32")]
    {
        GpuMemory::default()
    }
}

/// Keep at most `bytes` of recycled buffers in the free pool from now on (trimming it now).
pub fn set_pool_limit(bytes: u64) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        POOL_REQUESTED.store(bytes, Ordering::Relaxed);
        apply_pool_budget(bytes);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = bytes;
}

#[cfg(not(target_arch = "wasm32"))]
fn apply_pool_budget(requested: u64) {
    let bytes = lightcraft_sysmem::gpu::cache_budget(requested);
    ctx::POOL_LIMIT.store(bytes, Ordering::Relaxed);
    trim_pool(bytes);
}

/// Free recycled buffers until at most `keep` bytes stay pooled (e.g. when the app goes idle).
pub fn trim_pool(keep: u64) {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(g) = existing_device() {
        g.trim(keep);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = keep;
}

/// Release a view's device caches without waiting for a render worker. Used when local AI
/// reserves VRAM while the Develop view is idle. Returns false if a cache is currently busy.
pub fn trim_stages(stages: &StageCache) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        stages.peek_extension::<GpuStages>().is_none_or(|g| g.try_clear())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = stages;
        true
    }
}

/// Device bytes held by a view's GPU stages (kept with its [`StageCache`]); 0 when it has none.
pub fn stage_bytes(stages: &StageCache) -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    {
        stages.peek_extension::<GpuStages>().map(|g| g.bytes()).unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = stages;
        0
    }
}

/// Render `src` with `s` on the GPU, reusing the device-resident stages kept with `stages` (the
/// view's CPU stage cache). `None`: render on the CPU instead.
pub fn render(src: &Arc<Rgb32f>, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest, stages: Option<&StageCache>) -> Option<Rendered> {
    render_hs_candidate(src, info, s, req, stages, lightcraft_pipeline::primary::DEFAULT_HS_METHOD)
}

/// Internal owner-review/testing entry point; the choice is not persisted or exposed in UI.
pub fn render_hs_candidate(
    src: &Arc<Rgb32f>,
    info: &SourceInfo,
    s: &DevelopSettings,
    req: &RenderRequest,
    stages: Option<&StageCache>,
    method: lightcraft_pipeline::primary::HsMethod,
) -> Option<Rendered> {
    CPU_STAGES.with(|stages| stages.borrow_mut().clear());
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Soft proofs still use the CPU.
        if !enabled() || req.proof.is_some() {
            return None;
        }
        let ext = stages.map(|c| c.extension::<GpuStages>());
        if lightcraft_sysmem::gpu::pressured() {
            if let Some(e) = &ext {
                e.clear();
            }
            ctx::POOL_LIMIT.store(0, Ordering::Relaxed);
            trim_pool(0);
            record_fallback("GPU memory reserved for a local ComfyUI job; using CPU temporarily".into());
            return None;
        }
        let gpu = device()?;
        gpu.health.tick();
        gpu.health.begin_operation(lightcraft_sysmem::gpu::idle_epoch());
        let errors_before = gpu.health.errors();
        if let Some(e) = &ext {
            e.prepare(GENERATION.load(Ordering::Acquire));
        }
        apply_pool_budget(POOL_REQUESTED.load(Ordering::Relaxed));
        let fault = take_fault();
        let _ = ctx::take_failure(); // nothing left over from an earlier render on this thread
        let (r, scoped) = {
            let _scope = ctx::RenderScope::new(&gpu);
            let errors = ctx::ErrorScopes::push(&gpu);
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render::render(&gpu, src, info, s, req, ext.as_deref(), fault, method)));
            (r, errors.pop())
        };
        // An error the device raised explains the readback failures that follow it; a buffer
        // over the limit explains the errors its placeholder raises.
        let failure = match (ctx::take_failure(), scoped) {
            (Some(f), _) if f.kind == ctx::FailKind::Limit => Some(f),
            (f, scoped) => scoped.or(f),
        };
        match (r, failure) {
            (Err(_), _) => {
                log::error!("gpu: render failed; using the CPU pipeline until retry");
                gpu.health.operation_failed("a GPU render panicked", lightcraft_sysmem::gpu::idle_epoch());
                if let Some(e) = &ext {
                    e.clear();
                }
                gpu.trim(0);
                record_fallback("a GPU render panicked; GPU retry pending".into());
                None
            }
            (Ok(r), Some(f)) => {
                // the view's cached device stages may hold results of the failed passes
                if let Some(e) = &ext {
                    e.clear();
                }
                // A shared device can run out of memory on full-size spatial scratch even
                // though every buffer fits its advertised limits. Retry deep output once with
                // smaller bindings: existing hybrid stages (notably haze) then prepare on CPU.
                // The cap also bounds recursion and preserves the usual CPU fallback if retry
                // cannot fit. No device failure is latched for an allocation failure.
                const RETRY_BINDING_BYTES: u64 = 512 << 20;
                if f.kind == ctx::FailKind::OutOfMemory && req.depth == lightcraft_pipeline::OutputDepth::U16 && gpu.limit() > RETRY_BINDING_BYTES {
                    gpu.trim(0);
                    let _limit = ctx::LimitOverride::new(RETRY_BINDING_BYTES);
                    return render_hs_candidate(src, info, s, req, stages, method);
                }
                let what = match &r {
                    Some(r) => format!("{}×{}", r.image.width, r.image.height),
                    None => "GPU render".into(),
                };
                let reason = match f.kind {
                    ctx::FailKind::Fatal => {
                        gpu.health.operation_failed(&f.reason, lightcraft_sysmem::gpu::idle_epoch());
                        format!("{what}: {}; GPU retry pending", f.reason)
                    }
                    ctx::FailKind::OutOfMemory => {
                        gpu.trim(0);
                        gpu.health.operation_failed(&f.reason, lightcraft_sysmem::gpu::idle_epoch());
                        format!("{what}: {}", f.reason)
                    }
                    ctx::FailKind::Limit | ctx::FailKind::Blank => format!("{what}: {}", f.reason),
                };
                log::warn!("gpu: rendering on the CPU instead: {reason}");
                if render::profiling() {
                    eprintln!("  gpu fallback to the CPU: {reason}");
                }
                record_fallback(reason);
                None
            }
            // a device error on another thread or a lost device during the render
            (Ok(Some(_)), None) if !gpu.health.is_ok() || gpu.health.errors() != errors_before => {
                record_fallback(format!("device failure during the render: {}", unavailable_reason().unwrap_or_default()));
                None
            }
            (Ok(r), None) => {
                if r.is_some() {
                    gpu.health.operation_succeeded();
                }
                if let Some(e) = &ext
                    && e.bytes() as u64 > lightcraft_sysmem::gpu::cache_budget(1 << 30)
                {
                    e.clear();
                }
                r
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (src, info, s, req, stages, method);
        None
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod warm_up_exit_tests {
    #[test]
    fn quiesce_includes_worker_thread_local_destruction() {
        let _test = super::TEST_INIT_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        use std::cell::RefCell;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, mpsc};
        use std::time::Duration;

        struct Teardown {
            entered: mpsc::Sender<()>,
            release: mpsc::Receiver<()>,
            done: Arc<AtomicBool>,
        }
        impl Drop for Teardown {
            fn drop(&mut self) {
                let _ = self.entered.send(());
                let _ = self.release.recv_timeout(Duration::from_secs(10));
                self.done.store(true, Ordering::Release);
            }
        }
        thread_local! {
            static EXIT: RefCell<Option<Teardown>> = const { RefCell::new(None) };
        }
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let done = Arc::new(AtomicBool::new(false));
        let teardown = Teardown { entered: entered_tx, release: release_rx, done: done.clone() };
        let worker = super::spawn_init_worker(move || {
            EXIT.with(|exit| *exit.borrow_mut() = Some(teardown));
        })
        .unwrap();
        // The compiler function has ended; thread-local cleanup is still blocked.
        entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(!super::quiesce(Duration::ZERO));
        assert!(!done.load(Ordering::Acquire));
        release_tx.send(()).unwrap();
        assert!(super::quiesce(Duration::from_secs(20)));
        assert!(done.load(Ordering::Acquire));
        worker.join().unwrap();
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod context_exit_tests {
    use super::*;
    use std::time::Duration;

    fn hardware() -> Option<Arc<ctx::Gpu>> {
        let gpu = device();
        assert!(gpu.is_some() || std::env::var_os("LC_REQUIRE_GPU").is_none(), "GPU required: {:?}", unavailable_reason());
        if let Some(gpu) = &gpu {
            eprintln!("context teardown: {} ({:?})", gpu.info.name, gpu.info.backend);
        }
        gpu
    }

    #[test]
    fn shutdown_releases_cached_context_and_pooled_buffers() {
        let _test = TEST_INIT_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(gpu) = hardware() else { return };
        let weak = Arc::downgrade(&gpu);
        drop(gpu.buffer(16));
        assert!(ctx::memory().pooled > 0);
        assert!(shutdown(Duration::from_secs(120)));
        assert!(!ready());
        assert!(existing_device().is_none());
        assert!(weak.upgrade().is_some(), "an active render lease stays valid");
        drop(gpu);
        assert!(weak.upgrade().is_none(), "the static cache must not retain the context at native exit");
        assert_eq!(ctx::memory().allocated, 0);
        assert_eq!(ctx::memory().pooled, 0, "released pools must leave no counted bytes");
    }

    #[test]
    fn old_device_buffers_are_not_recycled_on_a_new_context() {
        let _test = TEST_INIT_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(old) = hardware() else { return };
        let old_buffer = old.buffer(16);
        assert!(shutdown(Duration::from_secs(120)));
        let new = hardware().unwrap();
        assert!(!Arc::ptr_eq(&old, &new), "shutdown must allow a fresh context");
        drop(old_buffer);
        assert_eq!(ctx::memory().pooled, 0, "an old-device buffer entered the new-device pool");
        drop(old);
        assert!(shutdown(Duration::from_secs(120)));
        drop(new);
        assert_eq!(ctx::memory().allocated, 0);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod recovery_tests {
    #[test]
    #[ignore = "real device-loss recovery; run explicitly with LC_REQUIRE_GPU=1"]
    fn gpu_device_loss_recreates_context_and_invalidates_view_buffers() {
        use super::*;
        use std::time::Duration;
        let _guard = TEST_INIT_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        reset_failures();
        let old = device().expect("real GPU required");
        assert_ne!(old.info.device_type, wgpu::DeviceType::Cpu);
        eprintln!("device-loss recovery adapter: {} ({:?})", old.info.name, old.info.backend);
        let src = Arc::new(lightcraft_scenes::demo_library()[0].render(513, 337));
        let info = SourceInfo::default();
        let settings = DevelopSettings::default();
        let req = RenderRequest::fit(513, 337);
        let stages = StageCache::default();
        let reference = render(&src, &info, &settings, &req, Some(&stages)).expect("initial GPU render");
        let generation = GENERATION.load(Ordering::Acquire);
        old.device.destroy();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| old.device.poll(wgpu::PollType::Poll)));
        assert!(!old.health.is_ok(), "device-lost callback must close admission");
        assert!(quiesce(Duration::ZERO), "a known lost device must not be polled during teardown");
        assert!(render(&src, &info, &settings, &req, Some(&stages)).is_none());
        std::thread::sleep(Duration::from_millis(4100));
        let recovered = render(&src, &info, &settings, &req, Some(&stages)).expect("automatic retry recreates device");
        assert!(GENERATION.load(Ordering::Acquire) > generation);
        assert_eq!(reference.image, recovered.image);
        drop(stages);
        drop(old);
        assert!(shutdown(Duration::from_secs(120)));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod cr_float_recovery_tests {
    #[test]
    #[ignore = "requires a real hardware GPU; destroys only this test process's compute device"]
    fn cr_gpu_float_device_loss_recovers_cached_deep_output() {
        use super::*;
        use std::time::Duration;
        let _guard = TEST_INIT_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let old = device().expect("real GPU required");
        assert_ne!(old.info.device_type, wgpu::DeviceType::Cpu);
        println!("Float device-loss adapter: {} ({:?})", old.info.name, old.info.backend);
        let src = Arc::new(lightcraft_scenes::demo_library()[0].render(192, 128));
        let info = SourceInfo::default();
        let mut settings = DevelopSettings::default();
        settings.light.exposure = 0.5;
        settings.effects.texture = 25.0;
        let req = RenderRequest {
            space: lightcraft_pipeline::OutputSpace::Rec2020,
            depth: lightcraft_pipeline::OutputDepth::F32Linear,
            ..RenderRequest::fit(192, 128)
        };
        let stages = StageCache::default();
        let reference = render(&src, &info, &settings, &req, Some(&stages)).expect("initial float GPU render");
        let generation = GENERATION.load(Ordering::Acquire);
        old.device.destroy();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| old.device.poll(wgpu::PollType::Poll)));
        assert!(!old.health.is_ok(), "the device-loss callback must close admission");
        assert!(render(&src, &info, &settings, &req, Some(&stages)).is_none(), "caller must use CPU while device is lost");
        std::thread::sleep(Duration::from_millis(4100));
        let recovered = render(&src, &info, &settings, &req, Some(&stages)).expect("automatic retry must recreate device");
        assert!(GENERATION.load(Ordering::Acquire) > generation);
        assert_eq!(reference.deep.unwrap().samples, recovered.deep.unwrap().samples, "replacement must invalidate/rebuild cached float stages");
        drop(stages);
        drop(old);
        assert!(shutdown(Duration::from_secs(120)));
    }
}
