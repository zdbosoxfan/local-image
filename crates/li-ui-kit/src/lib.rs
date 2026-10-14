//! Shared design-system backend. UI crates retain compatibility views and domain-specific controls.
#![forbid(unsafe_code)]
pub mod color_icon_data;
pub mod color_icons;
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
pub mod slider;
pub mod widget_style;
pub mod widgets;
