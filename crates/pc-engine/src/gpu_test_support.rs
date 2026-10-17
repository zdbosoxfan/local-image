//! Engine GPU test support on the one shared strict real-GPU prelude (`li-testkit::gpu`).
//! Tests serialise with `li_testkit::gpu::lock()`. [`compositor_device`] is the engine GPU parity
//! gates' prelude: a hardware Vulkan adapter, or Metal on macOS (Apple Silicon passes), never a
//! software adapter, whose accumulation format is `Rgba32Float`. `LI_GPU_TEST_ADAPTER=<name>` pins
//! one card (e.g. `RTX 5090`).

/// The strict hardware device for a compositor parity gate named `label`. Panics (a gate
/// failure, never a skip) when the machine has no acceptable GPU.
pub fn compositor_device(label: &str) -> (wgpu::Device, wgpu::Queue) {
    checked(li_testkit::gpu::hardware_device(label), label)
}

/// [`compositor_device`] with the adapter's full limits (gates that build large textures).
pub fn compositor_device_adapter_limits(label: &str) -> (wgpu::Device, wgpu::Queue) {
    checked(li_testkit::gpu::hardware_device_with(label, |a| wgpu::DeviceDescriptor { required_limits: a.limits(), ..Default::default() }), label)
}

fn checked(gpu: li_testkit::gpu::HardwareGpu, label: &str) -> (wgpu::Device, wgpu::Queue) {
    assert_eq!(
        photocraft_gpu::Compositor::preferred_acc_format(&gpu.adapter),
        wgpu::TextureFormat::Rgba32Float,
        "{label}: Rgba32Float accumulation required: {:?}",
        gpu.info
    );
    (gpu.device, gpu.queue)
}
