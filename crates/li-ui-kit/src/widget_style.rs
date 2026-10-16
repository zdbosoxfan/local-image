//! Inherited widget geometry/type tokens. Profiles keep the two established layout grammars.
//! Colours come from theme::Tokens; these dimensions are UI points, independent of hardware.
use egui::{Vec2, vec2};

pub const STROKE: f32 = 1.0;
pub const FOCUS_OUTSET: f32 = 2.0;
pub const FOCUS_STROKE: f32 = 2.0;
pub const EDITOR_BUTTON_FONT: f32 = 13.0;
pub const EDITOR_BUTTON_PADDING: f32 = 28.0;
pub const EDITOR_BUTTON_HEIGHT: f32 = 30.0;
pub const PRO_BUTTON_HEIGHT: f32 = 28.0;
pub const PRO_OUTLINE_STROKE: f32 = 1.5;
pub const LABEL_FONT: f32 = 11.5;
pub const TEXT_BUTTON_PADDING: f32 = 18.0;
pub const TEXT_BUTTON_MIN_WIDTH: f32 = 37.0;
pub const LIBRARY_RADIUS: f32 = 4.0;
pub const SWATCH_RADIUS: f32 = 3.0;
pub const SEGMENT_GAP: f32 = 4.0;
pub const SEGMENT_MIN_WIDTH: f32 = 24.0;
pub const VERTICAL_SEPARATOR_WIDTH: f32 = 9.0;
pub const HEADER_HEIGHT: f32 = 52.0;
pub const HEADER_TEXT_INSET: f32 = 44.0;
pub const HEADER_FONT: f32 = 14.0;
pub const HEADER_CHEVRON_INSET: f32 = 30.0;
pub const HEADER_CHEVRON_SIZE: f32 = 14.0;
pub const HEADER_EYE_INSET: f32 = 34.0;
pub const HEADER_EYE_SIZE: f32 = 18.0;
pub const ICON_CHROME_MAX_SIZE: f32 = 32.0;
pub const DROPDOWN_HEIGHT: f32 = 420.0;
pub const DROPDOWN_ARROW_OFFSET: Vec2 = vec2(-7.0, 1.0);
pub const DROPDOWN_ARROW_SIZE: f32 = 12.0;
pub const CHEVRON_HALF_WIDTH: f32 = 3.2;
pub const CHEVRON_STROKE: f32 = 1.3;
pub const CHECKBOX_SIZE: f32 = 14.0;
pub const CHECKBOX_RADIUS: f32 = 2.0;
pub const CHECKBOX_GAP: f32 = 6.0;
pub const CHECKBOX_OUTLINE: f32 = 1.5;
pub const CHECKBOX_CHECK_STROKE: f32 = 1.8;
pub const CHECKBOX_CHECK_OFFSETS: [Vec2; 3] = [vec2(3.0, 0.5), vec2(-1.0, -3.5), vec2(-3.0, 3.5)];
pub const CHECKER_MIN_CELL: f32 = 3.0;
pub const CHECKER_MAX_CELL: f32 = 6.0;

pub fn text_button_size(label_width: f32) -> Vec2 {
    vec2((label_width + TEXT_BUTTON_PADDING).max(TEXT_BUTTON_MIN_WIDTH), crate::theme::PILL_TAB_HEIGHT)
}

/// Notice row and details geometry inherited from the editor. Library uses the same details view.
pub const NOTICE_ROW_HEIGHT: f32 = 32.0;
pub const NOTICE_ROW_MARGIN: egui::Margin = egui::Margin::symmetric(8, 2);
pub const NOTICE_HISTORY_WIDTH: f32 = 620.0;
pub const NOTICE_HISTORY_VIEWPORT_MARGIN: f32 = 40.0;
pub const NOTICE_HISTORY_MIN_WIDTH: f32 = 240.0;
pub const NOTICE_HISTORY_BOTTOM_RESERVE: f32 = 140.0;
pub const NOTICE_HISTORY_MIN_HEIGHT: f32 = 120.0;
/// The original Library HUD width cap and bottom offset, now shared with its History action.
pub const NOTICE_HUD_MAX_WIDTH: f32 = 720.0;
pub const NOTICE_HUD_BOTTOM_OFFSET: f32 = 60.0;

/// Inherited slider profile metrics shared by the editor and Develop painters.
pub const SLIDER_HOLLOW_RADIUS: f32 = 7.0;
pub const SLIDER_HOLLOW_TRACK_HEIGHT: f32 = 2.0;
pub const SLIDER_HOLLOW_TRACK_RADIUS: f32 = 1.0;
pub const SLIDER_HOLLOW_GRADIENT_HEIGHT: f32 = 3.0;
pub const SLIDER_HOLLOW_GAP_EXTRA: f32 = 4.0;
pub const SLIDER_HOLLOW_GAP_HEIGHT: f32 = 6.0;
pub const SLIDER_THUMB_STROKE: f32 = 2.0;
pub const SLIDER_EDITOR_MIN_WIDTH: f32 = 60.0;
pub const SLIDER_EDITOR_HEIGHT: f32 = 18.0;
pub const SLIDER_EDITOR_TRACK_INSET: f32 = 14.0;
pub const SLIDER_EDITOR_GRADIENT_HEIGHT: f32 = 5.0;
pub const SLIDER_EDITOR_TRACK_HEIGHT: f32 = 3.0;
pub const SLIDER_EDITOR_TRACK_RADIUS: f32 = 2.0;
pub const SLIDER_CLASSIC_KNOB_SIZE: Vec2 = vec2(10.0, 16.0);
pub const SLIDER_PRO_KNOB_RADIUS: f32 = 6.0;
pub const SLIDER_STUDIO_KNOB_RADIUS: f32 = 7.0;
pub const SLIDER_KNOB_SHADOW_OFFSET: Vec2 = vec2(0.0, 1.0);
pub const SLIDER_KNOB_SHADOW_GROWTH: f32 = 0.5;
pub const SLIDER_KNOB_HOVER_RADIUS: f32 = 9.5;
pub const SLIDER_KNOB_HOVER_STROKE: f32 = 2.0;
