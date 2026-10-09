//! Single integration-test binary for this crate (one link instead of one per file).
mod brush_smoothing;
mod camera_raw_noise_clamp;
mod eraser_locked;
mod gallery_colours;
mod gauss_small_sigma;
mod guide_layout_limit;
mod hdr_gamma_direction;
mod keyboard_shortcut_bulk;
mod panic_hunt;
mod path_blur_speed_clamp;
#[cfg(feature = "corpus")]
mod photoshop_oracles;
mod prefs_usage;
mod rendering_preferences;
mod smart_psd;
mod text_gamma;
mod unicode_colors;
mod vectorize;
