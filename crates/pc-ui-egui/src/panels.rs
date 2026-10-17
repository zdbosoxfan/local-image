//! Chrome around the canvas: title bar, options bar, toolbar, status bar, dock cards, Properties.

use egui::{Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};
use photocraft_color::BlendMode;
use photocraft_doc::{Layer, LayerContent, LayerId};
use serde_json::{Value, json};

use crate::state::Tool;
use crate::theme::{self, Tokens};
use crate::{PhotocraftApp, icons, widgets};

mod color;
mod docks;
mod layers;
mod options_bar;
mod properties;
mod title;
mod toolbar;

use self::color::*;
pub use self::docks::*;
use self::layers::*;
pub use self::options_bar::*;
pub use self::properties::*;
pub use self::title::*;
pub use self::toolbar::*;

#[path = "swatches_panel.rs"]
pub(crate) mod swatches_panel;

#[cfg(test)]
#[path = "theme_tool_parity_tests.rs"]
mod theme_tool_parity_tests;

#[cfg(test)]
mod tests;
