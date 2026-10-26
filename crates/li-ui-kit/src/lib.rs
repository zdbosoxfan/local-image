//! Shared design-system backend. UI crates retain compatibility views and domain-specific controls.
#![forbid(unsafe_code)]
#![expect(clippy::allow_attributes_without_reason, clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

pub mod color_icon_data;
pub mod color_icons;
pub mod file_dialog;
pub mod fonts;
pub mod icon_data;
pub mod icons;
pub mod library;
pub mod library_icons;
pub mod mono_tool_icons;
pub mod theme;
pub mod tool_icon_data;
pub mod vector_icon_data;

pub mod histogram;
pub mod notices;
pub mod progress;
pub mod slider;
pub mod widget_style;
pub mod widgets;
