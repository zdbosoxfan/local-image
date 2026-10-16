//! The one strict real-GPU test prelude.
//!
//! GPU parity gates must run on real hardware and fail (never skip, never fall back to a software
//! adapter) when it is missing. This replaces the per-test "request an RTX 5090" preludes:
//! - backend: Vulkan, or Metal on macOS;
//! - adapter: a hardware GPU (discrete or integrated, so Apple Silicon passes); CPU/software
//!   adapters (llvmpipe, WARP, SwiftShader) and virtual/unknown devices fail;
//! - `LI_GPU_TEST_ADAPTER=<text>` pins the adapter name (e.g. `RTX 5090` on an integration box)
//!   so a gate can still insist on one specific card without hard-coding it in every test.
//!
//! Tests that share one process serialise device lifetimes with [`lock`].

use std::sync::{Mutex, MutexGuard};

static GPU: Mutex<()> = Mutex::new(());

/// Serialise GPU tests in one process: overlapping Vulkan device lifetimes are flaky on some
/// drivers. Hold the guard for the whole test.
pub fn lock() -> MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Environment variable that, when set, requires the adapter name to contain its value.
pub const ADAPTER_PIN_ENV: &str = "LI_GPU_TEST_ADAPTER";

/// The backend a strict gate runs on: Metal on macOS, Vulkan elsewhere.
pub fn required_backend() -> wgpu::Backend {
    if cfg!(target_os = "macos") { wgpu::Backend::Metal } else { wgpu::Backend::Vulkan }
}

fn required_backends() -> wgpu::Backends {
    if cfg!(target_os = "macos") { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN }
}

/// Why `info` is not an acceptable strict-gate adapter, or `Ok` when it is. `pin` is the
/// optional required name fragment ([`ADAPTER_PIN_ENV`]).
pub fn check_adapter(info: &wgpu::AdapterInfo, pin: Option<&str>) -> Result<(), String> {
    if info.backend != required_backend() {
        return Err(format!("{:?} backend required, got {info:?}", required_backend()));
    }
    if !matches!(info.device_type, wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu) {
        return Err(format!("hardware GPU required (software/virtual adapters never pass): {info:?}"));
    }
    if let Some(pin) = pin.map(str::trim).filter(|p| !p.is_empty())
        && !info.name.contains(pin)
    {
        return Err(format!("{ADAPTER_PIN_ENV}={pin:?} but the adapter is {info:?}"));
    }
    Ok(())
}

/// A validated hardware adapter with its device and queue.
pub struct HardwareGpu {
    pub adapter: wgpu::Adapter,
    pub info: wgpu::AdapterInfo,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// Request the high-performance hardware adapter and a default device, panicking with a clear
/// message when the machine has none (a gate failure, not a skip). `label` names the gate in the
/// adapter line printed to stderr. Does not take [`lock`]; callers hold it.
pub fn hardware_device(label: &str) -> HardwareGpu {
    try_hardware_device(label).unwrap_or_else(|e| panic!("{label}: real GPU required: {e}"))
}

/// [`hardware_device`] returning the failure instead of panicking.
pub fn try_hardware_device(label: &str) -> Result<HardwareGpu, String> {
    try_hardware_device_with(label, |_| wgpu::DeviceDescriptor::default())
}

/// [`hardware_device`] with a caller-built device descriptor (e.g. the adapter's full limits).
pub fn hardware_device_with(label: &str, desc: impl FnOnce(&wgpu::Adapter) -> wgpu::DeviceDescriptor<'static>) -> HardwareGpu {
    try_hardware_device_with(label, desc).unwrap_or_else(|e| panic!("{label}: real GPU required: {e}"))
}

/// [`hardware_device_with`] returning the failure instead of panicking.
pub fn try_hardware_device_with(label: &str, desc: impl FnOnce(&wgpu::Adapter) -> wgpu::DeviceDescriptor<'static>) -> Result<HardwareGpu, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: required_backends(), ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .map_err(|e| format!("no {:?} adapter: {e}", required_backend()))?;
    let info = adapter.get_info();
    let pin = std::env::var(ADAPTER_PIN_ENV).ok();
    check_adapter(&info, pin.as_deref())?;
    eprintln!("{label} adapter: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&desc(&adapter))).map_err(|e| format!("request_device: {e}"))?;
    Ok(HardwareGpu { adapter, info, device, queue })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, backend: wgpu::Backend, device_type: wgpu::DeviceType) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo { name: name.into(), ..wgpu::AdapterInfo::new(device_type, backend) }
    }

    #[test]
    fn testkit_gpu_gate_accepts_any_hardware_gpu_incl_apple_metal() {
        let b = required_backend();
        assert!(check_adapter(&info("NVIDIA GeForce RTX 5090", b, wgpu::DeviceType::DiscreteGpu), None).is_ok());
        assert!(check_adapter(&info("Apple M4 Max", b, wgpu::DeviceType::IntegratedGpu), None).is_ok());
        assert!(check_adapter(&info("AMD Radeon RX 7900", b, wgpu::DeviceType::DiscreteGpu), Some("")).is_ok());
        // Pinned name still enforced when asked for.
        assert!(check_adapter(&info("NVIDIA GeForce RTX 5090", b, wgpu::DeviceType::DiscreteGpu), Some("RTX 5090")).is_ok());
        assert!(check_adapter(&info("Apple M4 Max", b, wgpu::DeviceType::IntegratedGpu), Some("RTX 5090")).is_err());
    }

    #[test]
    fn testkit_gpu_gate_rejects_software_and_wrong_backend() {
        let b = required_backend();
        assert!(check_adapter(&info("llvmpipe (LLVM 19)", b, wgpu::DeviceType::Cpu), None).is_err());
        assert!(check_adapter(&info("virtio", b, wgpu::DeviceType::VirtualGpu), None).is_err());
        assert!(check_adapter(&info("unknown", b, wgpu::DeviceType::Other), None).is_err());
        assert!(check_adapter(&info("NVIDIA GeForce RTX 5090", wgpu::Backend::Gl, wgpu::DeviceType::DiscreteGpu), None).is_err());
    }
}
