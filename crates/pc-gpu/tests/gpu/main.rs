//! All GPU integration tests in one binary. They require a hardware GPU (`li_testkit::gpu`), and
//! every test that opens a device holds `li_testkit::gpu::lock()`: concurrent wgpu instances in
//! one process segfault on some drivers.
mod device_loss;
mod parity;
mod vector_mask;
