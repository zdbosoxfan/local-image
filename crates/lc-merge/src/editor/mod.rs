//! Shared photo-editor layer and rendered-source adapters.
//! The native raw merge lives in `crate::hdr`; these preserve rendered response recovery
//! and layer placement semantics while sharing registration and numerical kernels.
pub mod features;
pub mod geom;
pub mod hdr;
pub mod panorama;
pub mod support;
