//! Test binary that compiles the UI snapping module directly.
//! Native app routing is a separate gate; these tests do not claim it wired.
#[path = "../../src/vector_packages/snapping/tests_gpu.rs"]
mod gpu;
#[path = "../../src/gpu_test_support.rs"]
mod gpu_test_support;
#[path = "../../src/vector_packages/snapping.rs"]
mod snapping;
