//! Device ownership follows egui-wgpu's RenderState: clone handles, never request a second
//! device for compute. Headless callers use the same registry without a window.
use crate::DeviceHealth;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Mutex, PoisonError};

#[derive(Clone, Debug)]
pub struct SharedDevice {
    id: u64,
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub health: DeviceHealth,
}

impl SharedDevice {
    /// Wrap handles and install the only pair of device callbacks.
    pub fn new(instance: wgpu::Instance, adapter: wgpu::Adapter, device: wgpu::Device, queue: wgpu::Queue, health: DeviceHealth) -> Self {
        health.attach(&device);
        static NEXT_DEVICE: AtomicU64 = AtomicU64::new(1);
        Self { id: NEXT_DEVICE.fetch_add(1, Ordering::Relaxed), instance, adapter, device, queue, health }
    }

    /// Identity across instances, where raw wgpu device IDs may compare equal.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn compute_reason(&self) -> Option<String> {
        let info = self.adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            return Some(format!("software adapter ({}) skipped: the CPU pipeline is faster", info.name));
        }
        if !self.adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::COMPUTE_SHADERS) {
            return Some(format!("{} does not support compute shaders", info.name));
        }
        let count = self.device.limits().max_storage_buffers_per_shader_stage;
        (count < 10).then(|| format!("{} has too few storage buffers per shader stage ({count} < 10)", info.name))
    }
}

struct Registry {
    window: bool,
    compute: bool,
    device: Option<Arc<SharedDevice>>,
}
impl Default for Registry {
    fn default() -> Self {
        Self { window: false, compute: true, device: None }
    }
}
#[cfg(not(target_arch = "wasm32"))]
static REGISTRY: Mutex<Registry> = Mutex::new(Registry { window: false, compute: true, device: None });

#[cfg(target_arch = "wasm32")]
thread_local! {
    // WebGPU handles are not Send/Sync. The browser renderer and callbacks own them on
    // their JS thread; don't rely on egui's optional fragile-send-sync feature.
    static REGISTRY: std::cell::RefCell<Registry> = std::cell::RefCell::new(Registry::default());
}

fn with_registry<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    #[cfg(not(target_arch = "wasm32"))]
    {
        f(&mut REGISTRY.lock().unwrap_or_else(PoisonError::into_inner))
    }
    #[cfg(target_arch = "wasm32")]
    {
        REGISTRY.with(|registry| f(&mut registry.borrow_mut()))
    }
}

/// Reserve ownership before window initialization or worker startup. Compute must wait for
/// the shell's device, including CPU/compatible-window launches; it cannot bypass that policy.
pub fn reserve_window() {
    reserve_window_with_compute(true);
}

/// Reserve the shell's launch policy. CPU/safe launches may still need a hardware window;
/// its compute consumers must not bypass that choice with a private device or kernels.
pub fn reserve_window_with_compute(allowed: bool) {
    with_registry(|registry| {
        registry.window = true;
        registry.compute = allowed;
    });
}

pub fn compute_allowed() -> bool {
    with_registry(|registry| !registry.window || registry.compute)
}

/// Publish the window handles. Repeated consumers of the same device reuse its health without
/// replacing callbacks; a genuinely new window device has isolated fault state.
pub fn install_window(instance: &wgpu::Instance, adapter: &wgpu::Adapter, device: &wgpu::Device, queue: &wgpu::Queue) -> Arc<SharedDevice> {
    with_registry(|registry| {
        registry.window = true;
        if let Some(existing) = &registry.device
            && existing.instance == *instance
            && existing.device == *device
        {
            return existing.clone();
        }
        let health = registry.device.as_ref().map_or_else(DeviceHealth::new, |old| old.health.replacement());
        let shared = Arc::new(SharedDevice::new(instance.clone(), adapter.clone(), device.clone(), queue.clone(), health));
        registry.device = Some(shared.clone());
        shared
    })
}

/// Request the union of the renderer/compute limits on hardware. GL retains egui's downlevel
/// base (compute may be unavailable there), with large texture/buffer support as before.
pub fn device_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
    let available = adapter.limits();
    if adapter.get_info().backend == wgpu::Backend::Gl {
        wgpu::Limits {
            max_texture_dimension_2d: available.max_texture_dimension_2d,
            max_buffer_size: available.max_buffer_size,
            ..wgpu::Limits::downlevel_webgl2_defaults()
        }
    } else {
        available
    }
}

/// Obtain the shared compute device. Faulted window devices stay faulted until the shell
/// supplies a replacement; creating a private device would leave the window on a dead one.
#[cfg(not(target_arch = "wasm32"))]
pub fn acquire(backends: wgpu::Backends, health: DeviceHealth) -> Result<Arc<SharedDevice>, String> {
    let mut registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    if registry.window && !registry.compute {
        return Err("GPU compute disabled by the shell's CPU rendering policy".into());
    }
    if let Some(existing) = &registry.device
        && (registry.window || existing.health.can_present())
    {
        // A recoverable breaker reuses the same device, including standalone callers.
        existing.health.resume_retry(web_time::Instant::now(), None);
        return Ok(existing.clone());
    }
    if registry.window {
        return Err("waiting for the window's shared GPU device".into());
    }
    let shared = Arc::new(create_headless(backends, false, health)?);
    registry.device = Some(shared.clone());
    Ok(shared)
}

/// Explicit test/CLI constructor. Software adapters are opt-in for numerical CI only.
#[cfg(not(target_arch = "wasm32"))]
pub fn create_headless(backends: wgpu::Backends, software: bool, health: DeviceHealth) -> Result<SharedDevice, String> {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = backends;
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: software,
        ..Default::default()
    }))
    .map_err(|e| format!("no GPU adapter among {backends:?} ({e})"))?;
    if !software && adapter.get_info().device_type == wgpu::DeviceType::Cpu {
        return Err(format!("software adapter ({}) skipped", adapter.get_info().name));
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Local Image shared device"),
        required_limits: device_limits(&adapter),
        ..Default::default()
    }))
    .map_err(|e| format!("device creation failed ({e})"))?;
    Ok(SharedDevice::new(instance, adapter, device, queue, health))
}

/// Release only headless ownership when its compute context shuts down. Window ownership
/// remains with eframe until native exit, so a subsequent compute warm-up shares it again.
pub fn release_headless() {
    with_registry(|registry| {
        if !registry.window {
            registry.device = None;
        }
    });
}

/// Snapshot for identity checks and diagnostics; no driver calls or device creation.
pub fn current() -> Option<Arc<SharedDevice>> {
    with_registry(|registry| registry.device.clone())
}

/// After the shell has stopped all workers and finished its queue, release its service lease.
/// Outstanding resource/consumer handles remain valid until those owners drop them.
pub fn release_window() {
    with_registry(|registry| {
        registry.device = None;
        registry.window = false;
        registry.compute = true;
    });
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[test]
fn cpu_window_policy_does_not_create_private_compute_device() {
    let _lock = crate::TEST_INIT_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    reserve_window_with_compute(false);
    assert!(!compute_allowed());
    let result = acquire(wgpu::Backends::PRIMARY, DeviceHealth::new());
    assert!(result.is_err());
    assert!(current().is_none(), "a CPU window launch must not initialize another device");
    release_window();
    assert!(compute_allowed());
}
