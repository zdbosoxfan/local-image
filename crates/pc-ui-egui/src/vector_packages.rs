//! Shared feature implementations. Tool identity is independent of editing persona.
pub mod affinity;
pub mod appearance;
pub mod corners;
pub mod drawing;
pub mod layout;
pub mod path_text;
pub mod pdf_export;
pub mod pen;
pub mod pencil;
pub mod selection;
pub mod snapping;
pub mod svg_export;
pub mod svg_import;

#[path = "vector_packages/layout/app_hooks.rs"]
pub mod layout_app_hooks;
#[path = "vector_packages/path_text/app_hooks.rs"]
pub mod path_text_app_hooks;
