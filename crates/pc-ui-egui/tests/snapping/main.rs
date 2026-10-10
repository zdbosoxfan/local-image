//! Package-owned test binary: compiles the UI module without editing 00's registry.
//! Native app routing is a separate integration gate; these tests do not claim it wired.
#[path = "../../src/vector_packages/snapping/tests_gpu.rs"]
mod gpu;
#[path = "../../src/vector_packages/snapping.rs"]
mod snapping;
