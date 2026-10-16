//! GPU integration tests for the UI crate, in one binary. Every test here creates a wgpu device
//! (several simulate device loss), and concurrent wgpu instances in one process crash some
//! drivers (#194), so all tests take this one process-wide lock. Kept apart from `it` so the
//! CPU-only tests are not serialised behind it.
use egui::{Event, Key, Modifiers, PointerButton};
use photocraft_doc::{Document, Fill, LayerContent, ShapeStroke};
use photocraft_engine::Session;
use photocraft_ui_egui::{PhotocraftApp, anchor_tools::*, canvas, gpu_canvas, state};

mod adjust_preview_gpu;
mod camera_raw_scope;
mod canvas_16f;
mod canvas_flip_gpu;
mod color_managed_canvas;
mod drag_preview_canvas;
mod ds_findings;
mod gpu_canvas_perf;
mod gpu_device_loss;
mod layout_perf;
mod live_stroke_canvas;
mod move_canvas;
mod shared_tools_perf;
mod vector_integration;

static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu_lock() -> std::sync::MutexGuard<'static, ()> {
    GPU_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[path = "../../src/vector_packages/anchor_tools/app_gpu.rs"]
mod anchor_tools_app;

#[path = "../../src/vector_packages/symbols/app_gpu.rs"]
mod symbol_instances;

#[path = "../../src/vector_packages/drawing/app_gpu.rs"]
mod live_shapes_app_gpu;

#[path = "../pen/app_acceptance.rs"]
mod pen_app_acceptance;

#[path = "../../src/vector_packages/corners/app_gpu_tests.rs"]
mod corners_app_gpu;

#[path = "../../src/vector_packages/constructive/app_gpu.rs"]
mod constructive_app_gpu;

#[path = "../../src/vector_packages/width/app_gpu_tests.rs"]
mod width_profile_strokes;

#[path = "../path_text/app_acceptance.rs"]
mod path_text_app_acceptance;

#[path = "../appearance/app_acceptance.rs"]
mod appearance_app_acceptance;

#[path = "../../../pc-engine/src/vector_packages/svg_import/app_acceptance.rs"]
mod svg_import_app_acceptance;

#[path = "../../../pc-export-common/cases/app_gpu.rs"]
mod export_app_gpu;

#[path = "../../../pc-svg/tests/export/app_gpu.rs"]
mod svg_export_app_gpu;

#[path = "../../../pc-engine/src/vector_packages/pdf_import/app_gpu.rs"]
mod pdf_import_app_acceptance;

#[path = "../../../pc-pdf/tests/export/app_gpu.rs"]
mod pdf_export_app_gpu;

#[path = "../../../pc-io/tests/vector_fallback/app_gpu.rs"]
mod psd_advanced_fallback;

#[path = "../../src/vector_packages/affinity/app_gpu.rs"]
mod affinity_app;

#[path = "../../../pc-text/tests/frame_compose/app_gpu.rs"]
mod frame30;

#[path = "../../../pc-pathops/tests/quality/app_gpu.rs"]
mod geometry_quality_31;

#[path = "../../src/vector_packages/clip_compound/app_gpu.rs"]
mod clip_compound;

#[path = "../../src/vector_packages/blob_eraser/app_gpu.rs"]
mod blob_eraser40_app_gpu;

#[path = "../../src/vector_packages/measure/app_gpu.rs"]
mod measure44;

#[path = "../../src/vector_packages/pencil/gpu_tests.rs"]
mod pencil_app_gpu;

#[path = "../../src/vector_packages/blend/app_acceptance.rs"]
mod blend_app_acceptance;
#[path = "../../src/vector_packages/layout/app_acceptance.rs"]
mod layout_app_acceptance;
