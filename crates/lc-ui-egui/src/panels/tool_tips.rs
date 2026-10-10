//! Rich hover tips for the Library/Develop strip and the on-photo tool buttons: the tool's name and shortcut, one
//! sentence on what it does and a few "how to use" lines, beside a larger icon. The text is one
//! table, [`entry`], so nothing is scattered through the strip code.
//!
//! Settings › Interface › Tool tips picks Rich (this), Simple (name and shortcut in a plain
//! tooltip) or Off. A short delay precedes the first tip; moving to another tool while one is
//! open shows its tip straight away; a tip hides while a button is held.
//!
//! Shortcuts are read from the command table (`menus::UI_COMMANDS`), the same source the menus
//! and the key handler use, never typed here. The how-to lines were read off the panels' own
//! hints and tooltips.

use egui::{Area, Frame, Id, Order, Response, Sense, Stroke, vec2};
use serde::{Deserialize, Serialize};

use crate::icons::{Icon, paint};
use crate::theme::Tokens;

/// Seconds before the first tip opens.
pub const DELAY: f64 = 0.5;
/// A tip counts as still open this long after it was last drawn.
pub const LINGER: f64 = 0.3;
/// Widest a tip gets.
pub const MAX_WIDTH: f32 = 320.0;
/// Size of the icon slot at the left of a tip (coloured icons will fill it later).
pub const ICON: f32 = 32.0;

/// How much a tool's hover tip shows (Settings › Interface).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolTipMode {
    #[default]
    Rich,
    Simple,
    Off,
}

/// The tools of the strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StripTool {
    Presets,
    Edit,
    Crop,
    Remove,
    Masking,
    RedEye,
    Versions,
    Activity,
    Keywords,
    Info,
    More,
    WbPicker,
    Straighten,
    Guided,
    RemoveAi,
    RemovePixels,
    Heal,
    Clone,
    MaskObject,
    MaskPrompt,
    MaskSubject,
    MaskSky,
    MaskBackground,
    MaskDepth,
    MaskBrush,
    MaskLinear,
    MaskRadial,
    MaskLuminance,
    MaskColor,
    Target,
    PointColor,
    PetEye,
    FilmBase,
}

impl StripTool {
    pub const ALL: [StripTool; 33] = [
        StripTool::Presets,
        StripTool::Edit,
        StripTool::Crop,
        StripTool::Remove,
        StripTool::Masking,
        StripTool::RedEye,
        StripTool::Versions,
        StripTool::Activity,
        StripTool::Keywords,
        StripTool::Info,
        StripTool::More,
        StripTool::WbPicker,
        StripTool::Straighten,
        StripTool::Guided,
        StripTool::RemoveAi,
        StripTool::RemovePixels,
        StripTool::Heal,
        StripTool::Clone,
        StripTool::MaskObject,
        StripTool::MaskPrompt,
        StripTool::MaskSubject,
        StripTool::MaskSky,
        StripTool::MaskBackground,
        StripTool::MaskDepth,
        StripTool::MaskBrush,
        StripTool::MaskLinear,
        StripTool::MaskRadial,
        StripTool::MaskLuminance,
        StripTool::MaskColor,
        StripTool::Target,
        StripTool::PointColor,
        StripTool::PetEye,
        StripTool::FilmBase,
    ];

    /// The strip button's automation id (`icon:{id}`).
    pub fn id(self) -> &'static str {
        match self {
            StripTool::Presets => "presets",
            StripTool::Edit => "edit",
            StripTool::Crop => "crop",
            StripTool::Remove => "remove",
            StripTool::Masking => "masking",
            StripTool::RedEye => "redeye",
            StripTool::Versions => "versions",
            StripTool::Activity => "activity",
            StripTool::Keywords => "keywords",
            StripTool::Info => "info",
            StripTool::More => "more",
            StripTool::WbPicker => "wbPicker",
            StripTool::Straighten => "straighten",
            StripTool::Guided => "guidedUpright",
            StripTool::RemoveAi => "ai",
            StripTool::RemovePixels => "removePixels",
            StripTool::Heal => "heal",
            StripTool::Clone => "clone",
            StripTool::MaskObject => "object",
            StripTool::MaskPrompt => "prompt",
            StripTool::MaskSubject => "subject",
            StripTool::MaskSky => "sky",
            StripTool::MaskBackground => "background",
            StripTool::MaskDepth => "depth",
            StripTool::MaskBrush => "brush",
            StripTool::MaskLinear => "linear",
            StripTool::MaskRadial => "radial",
            StripTool::MaskLuminance => "luminanceRange",
            StripTool::MaskColor => "colorRange",
            StripTool::Target => "target",
            StripTool::PointColor => "pointColorPicker",
            StripTool::PetEye => "petEye",
            StripTool::FilmBase => "negDmin",
        }
    }

    /// The command the button runs.
    pub fn command(self) -> Option<&'static str> {
        match self {
            Self::Presets => Some("panel.presets"),
            Self::Edit => Some("panel.edit"),
            Self::Crop => Some("panel.crop"),
            Self::Remove => Some("panel.remove"),
            Self::Masking => Some("panel.masking"),
            Self::RedEye => Some("panel.redeye"),
            Self::Versions => Some("panel.versions"),
            Self::Activity => Some("panel.activity"),
            Self::Keywords => Some("panel.keywords"),
            Self::Info => Some("panel.info"),
            Self::WbPicker => Some("tool.wbPicker"),
            Self::Guided => Some("tool.guidedUpright"),
            Self::MaskBrush => Some("tool.brush"),
            Self::MaskLinear => Some("tool.linear"),
            Self::MaskRadial => Some("tool.radial"),
            _ => None,
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            StripTool::Presets => Icon::Presets,
            StripTool::Edit => Icon::Sliders,
            StripTool::Crop => Icon::Crop,
            StripTool::Remove => Icon::Eraser,
            StripTool::Masking => Icon::Mask,
            StripTool::RedEye => Icon::Eye,
            StripTool::Versions => Icon::Versions,
            StripTool::Activity => Icon::Activity,
            StripTool::Keywords => Icon::Tag,
            StripTool::Info => Icon::Info,
            StripTool::More => Icon::More,
            StripTool::WbPicker => Icon::Picker,
            StripTool::Straighten => Icon::Rotate,
            StripTool::Guided => Icon::Crop,
            StripTool::RemoveAi => Icon::Eraser,
            StripTool::RemovePixels => Icon::Eraser,
            StripTool::Heal => Icon::Brush,
            StripTool::Clone => Icon::Brush,
            StripTool::MaskObject => Icon::Subject,
            StripTool::MaskPrompt => Icon::Subject,
            StripTool::MaskSubject => Icon::Subject,
            StripTool::MaskSky => Icon::Sky,
            StripTool::MaskBackground => Icon::Subject,
            StripTool::MaskDepth => Icon::Sliders,
            StripTool::MaskBrush => Icon::Brush,
            StripTool::MaskLinear => Icon::Linear,
            StripTool::MaskRadial => Icon::Radial,
            StripTool::MaskLuminance => Icon::Sliders,
            StripTool::MaskColor => Icon::Picker,
            StripTool::Target => Icon::Target,
            StripTool::PointColor => Icon::Picker,
            StripTool::PetEye => Icon::Eye,
            StripTool::FilmBase => Icon::Picker,
        }
    }

    /// The tool's name (English; shown through `tr`).
    pub fn title(self) -> &'static str {
        match self {
            StripTool::Presets => "Presets",
            StripTool::Edit => "Edit",
            StripTool::Crop => "Crop & Rotate",
            StripTool::Remove => "Remove",
            StripTool::Masking => "Masking",
            StripTool::RedEye => "Red Eye",
            StripTool::Versions => "Versions",
            StripTool::Activity => "History & Activity",
            StripTool::Keywords => "Keywords",
            StripTool::Info => "Info",
            StripTool::More => "More",
            StripTool::WbPicker => "White Balance Selector",
            StripTool::Straighten => "Straighten Tool",
            StripTool::Guided => "Guided Upright",
            StripTool::RemoveAi => "AI Remove",
            StripTool::RemovePixels => "Remove",
            StripTool::Heal => "Heal",
            StripTool::Clone => "Clone",
            StripTool::MaskObject => "Object",
            StripTool::MaskPrompt => "Describe",
            StripTool::MaskSubject => "Subject",
            StripTool::MaskSky => "Sky",
            StripTool::MaskBackground => "Background",
            StripTool::MaskDepth => "Depth Range",
            StripTool::MaskBrush => "Brush",
            StripTool::MaskLinear => "Linear Gradient",
            StripTool::MaskRadial => "Radial Gradient",
            StripTool::MaskLuminance => "Luminance Range",
            StripTool::MaskColor => "Color Range",
            StripTool::Target => "Targeted Adjustment Tool",
            StripTool::PointColor => "Point Color Picker",
            StripTool::PetEye => "Pet Eye",
            StripTool::FilmBase => "Film Base Picker",
        }
    }

    /// The mask tiles and retouch mode buttons share these named groups.
    pub fn group(self) -> &'static [Self] {
        use StripTool::*;
        if [RemoveAi, RemovePixels, Heal, Clone].contains(&self) {
            &[RemoveAi, RemovePixels, Heal, Clone]
        } else if [
            MaskObject,
            MaskPrompt,
            MaskSubject,
            MaskSky,
            MaskBackground,
            MaskDepth,
            MaskBrush,
            MaskLinear,
            MaskRadial,
            MaskLuminance,
            MaskColor,
        ]
        .contains(&self)
        {
            &[MaskObject, MaskPrompt, MaskSubject, MaskSky, MaskBackground, MaskDepth, MaskBrush, MaskLinear, MaskRadial, MaskLuminance, MaskColor]
        } else if [RedEye, PetEye].contains(&self) {
            &[RedEye, PetEye]
        } else {
            &[]
        }
    }

    pub fn mask_kind(kind: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| !t.group().is_empty() && t.id() == kind)
    }

    /// The shortcut in this platform's notation: from the command table, `None` when unbound.
    pub fn shortcut(self) -> Option<String> {
        crate::shortcuts::shortcut_label(self.command()?)
    }
}

/// The words of one tip (English sources, translated when shown).
pub struct Entry {
    pub blurb: &'static str,
    pub how: &'static [&'static str],
}

/// The tip text of a strip tool. Exhaustive, so a new tool cannot ship without one.
pub fn entry(tool: StripTool) -> Entry {
    match tool {
        StripTool::Presets => Entry {
            blurb: "Looks you can put on the photo in one click.",
            how: &["Rest on a preset to preview it on the photo.", "Click it to apply; the Amount slider sets its strength."],
        },
        StripTool::Edit => Entry {
            blurb: "Sliders for light, colour, effects, detail and lens corrections.",
            how: &["Open a section to see its sliders.", "Drag a slider to adjust; double-click it to reset."],
        },
        StripTool::Crop => Entry {
            blurb: "Crops, straightens and rotates the photo.",
            how: &[
                "Drag the crop handles to change the frame.",
                "Choose Straighten Tool, then drag along the horizon or double-click for Auto.",
                "Press X to swap the aspect ratio.",
            ],
        },
        StripTool::Remove => Entry {
            blurb: "Removes dust, blemishes and distractions.",
            how: &[
                "Paint over what you want gone.",
                "Choose AI, Remove, Heal or Clone at the top of the panel.",
                "Press / to pick a new source for the selected spot.",
            ],
        },
        StripTool::Masking => Entry {
            blurb: "Applies adjustments to just part of the photo.",
            how: &["Choose a mask type, then edit the mask it makes.", "Use + and − on a mask to add to it or subtract from it."],
        },
        StripTool::RedEye => Entry {
            blurb: "Fixes red pupils in flash photos.",
            how: &["Drag over an eye; the pupil inside is found automatically.", "Adjust Pupil Size and Darken to refine the correction."],
        },
        StripTool::PetEye => Entry {
            blurb: "Corrects flash-lit pupils in animal photos.",
            how: &["Drag over an eye; the pupil inside is found automatically.", "Adjust Pupil Size and Darken to refine the correction."],
        },
        StripTool::Versions => Entry {
            blurb: "Saves snapshots of your edit so you can return to them.",
            how: &["Create Version saves the current edit.", "Restore on a version brings it back."],
        },
        StripTool::Activity => Entry {
            blurb: "Lists the edits made to this photo.",
            how: &["Open the panel to review edit history.", "Click a history entry to return to that edit."],
        },
        StripTool::Keywords => Entry {
            blurb: "Tags photos with keywords.",
            how: &["Choose a keyword in the Keywords panel.", "Use Keyword Painter to toggle it on grid photos; Esc stops."],
        },
        StripTool::Info => Entry {
            blurb: "Shows the photo's file, camera and exposure details.",
            how: &["Select a photo, then open Info.", "Scroll the panel to review and edit metadata."],
        },
        StripTool::More => Entry {
            blurb: "Opens the About window.",
            how: &["Click to see the app version and credits.", "Close the window to return to your photos."],
        },
        StripTool::WbPicker => Entry {
            blurb: "Sets white balance from a neutral area of the photo.",
            how: &["Click a neutral grey or white area.", "Choose the tool again to stop sampling."],
        },
        StripTool::Straighten => Entry {
            blurb: "Straightens the photo using a line you draw.",
            how: &["Drag along the horizon to straighten.", "Double-click for Auto Straighten."],
        },
        StripTool::Guided => Entry {
            blurb: "Corrects perspective using horizontal and vertical guides.",
            how: &["Drag guides along lines that should be horizontal or vertical.", "Use the handles to adjust existing guides."],
        },
        StripTool::RemoveAi => Entry {
            blurb: "Repairs distractions with the AI removal model.",
            how: &["Paint or draw a lasso over the distraction.", "Hold {alt} to subtract from the removal area, then click Remove."],
        },
        StripTool::RemovePixels => Entry {
            blurb: "Removes a painted area using surrounding pixels.",
            how: &["Paint over the distraction.", "Select a spot to adjust its size and feather."],
        },
        StripTool::Heal => Entry {
            blurb: "Blends surrounding texture into blemishes.",
            how: &["Paint over the blemish.", "Set brush Size and Feather before painting."],
        },
        StripTool::Clone => Entry {
            blurb: "Copies pixels from a source area over a painted spot.",
            how: &["Paint over the area to replace.", "Drag the selected spot's source marker to choose clean pixels."],
        },
        StripTool::MaskObject => Entry {
            blurb: "Selects an object for local adjustments with AI.",
            how: &["Click the object or draw a box around it.", "Use Add or Subtract to refine the mask."],
        },
        StripTool::MaskPrompt => Entry {
            blurb: "Selects parts of the photo from your description.",
            how: &["Type what you want to select, then click Select.", "Use Add or Subtract to refine the mask."],
        },
        StripTool::MaskSubject | StripTool::MaskSky | StripTool::MaskBackground | StripTool::MaskDepth => Entry {
            blurb: "Creates an automatic mask for local adjustments.",
            how: &["Click to create the mask from the photo.", "Use Add or Subtract to refine the mask."],
        },
        StripTool::MaskBrush => Entry {
            blurb: "Paints a mask for local adjustments.",
            how: &["Drag to paint the mask.", "Hold {alt} to switch between adding and erasing."],
        },
        StripTool::MaskLinear => Entry {
            blurb: "Makes a mask that fades along a straight gradient.",
            how: &["Drag the gradient's handles to set its start and end.", "Drag the pin to move the gradient."],
        },
        StripTool::MaskRadial => Entry {
            blurb: "Makes an elliptical mask for local adjustments.",
            how: &["Drag the ellipse's handles to resize it.", "Drag its pin to move it; {alt}-drag a handle to resize both axes."],
        },
        StripTool::MaskLuminance => Entry {
            blurb: "Masks pixels within a brightness range.",
            how: &["Choose the luminance range with the panel sliders.", "Use Smoothness to soften the range edges."],
        },
        StripTool::MaskColor => Entry {
            blurb: "Masks pixels matching sampled colours.",
            how: &["Click the photo to sample a colour.", "{shift}-click adds another sample."],
        },
        StripTool::Target => Entry {
            blurb: "Adjusts a colour or tonal range directly on the photo.",
            how: &["Choose the target tool, then drag vertically on the photo.", "Choose it again to stop adjusting."],
        },
        StripTool::FilmBase => Entry {
            blurb: "Samples the unexposed film base for negative conversion.",
            how: &["Drag over the unexposed film rim, or click it, to sample.", "Choose the tool again to stop sampling."],
        },
        StripTool::PointColor => Entry {
            blurb: "Samples a colour for Point Color adjustments.",
            how: &["Click the photo to add a colour sample.", "Use the Point Color sliders to adjust the sampled range."],
        },
    }
}

/// Fill the `{shift}` and `{alt}` placeholders with the platform's modifier names.
fn mods(s: &str) -> String {
    let mac = cfg!(target_os = "macos");
    s.replace("{shift}", &crate::menubar::shortcut_text("Shift", mac)).replace("{alt}", &crate::menubar::shortcut_text("Alt", mac))
}

#[derive(Clone, Copy, Default)]
struct Hover {
    over: Option<(Id, f64)>,
    shown: Option<f64>,
}

/// What the last frame's open tip said (for tests and automation).
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub frame: u64,
    pub tool: String,
    pub text: String,
    pub bounds: egui::Rect,
}

/// The tip open now (drawn in this or the previous frame), if any.
pub fn shown(ctx: &egui::Context) -> Option<Shown> {
    let s: Shown = ctx.data(|d| d.get_temp(Id::new("lc-tool-tip-shown")))?;
    (s.frame + 1 >= ctx.cumulative_pass_nr()).then_some(s)
}

/// Attach the tip of `tool` to its strip button.
pub fn attach(mode: ToolTipMode, ui: &egui::Ui, resp: &Response, tool: StripTool) {
    let title = crate::i18n::tr(tool.title());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, resp.enabled(), title));
    if mode == ToolTipMode::Off || ui.ctx().input(|i| i.pointer.any_down()) || egui::Popup::is_any_open(ui.ctx()) {
        ui.ctx().data_mut(|d| {
            d.remove::<Hover>(Id::new("lc-tool-tip-hover"));
            d.remove::<Shown>(Id::new("lc-tool-tip-shown"));
        });
        return;
    }
    if mode == ToolTipMode::Simple {
        let tip = match tool.shortcut() {
            Some(k) => format!("{title} ({k})"),
            None => title.to_string(),
        };
        resp.clone().on_hover_text(tip);
        return;
    }
    let ctx = ui.ctx();
    let now = ctx.input(|i| i.time);
    let mem_id = Id::new("lc-tool-tip-hover");
    let mut mem: Hover = ctx.data(|d| d.get_temp(mem_id)).unwrap_or_default();
    if !resp.hovered() {
        if mem.over.is_some_and(|(id, _)| id == resp.id) {
            mem.over = None;
            ctx.data_mut(|d| d.insert_temp(mem_id, mem));
        }
        return;
    }
    let since = match mem.over {
        Some((id, t0)) if id == resp.id => t0,
        _ => {
            mem.over = Some((resp.id, now));
            now
        }
    };
    if mem.shown.is_some_and(|last| now - last < LINGER) || now - since >= DELAY {
        mem.shown = Some(now);
        draw(ui, resp, tool);
    } else {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((DELAY - (now - since)).max(0.01)));
    }
    ctx.data_mut(|d| d.insert_temp(mem_id, mem));
}

fn draw(ui: &egui::Ui, resp: &Response, tool: StripTool) {
    let t = Tokens::get(ui.ctx());
    let ent = entry(tool);
    let title = crate::i18n::tr(tool.title());
    let key = tool.shortcut();
    let blurb = crate::i18n::tr(ent.blurb);
    let how: Vec<String> = ent.how.iter().map(|h| mods(crate::i18n::tr(h))).collect();
    let screen = ui.ctx().content_rect();
    let width = MAX_WIDTH.min(screen.width() - 16.0);
    // The strip is at the window's right edge: the tip opens to the left of the button.
    let pos = egui::pos2(resp.rect.left() - 8.0, resp.rect.top());
    let mut text = match &key {
        Some(k) => format!("{title} ({k})\n{blurb}"),
        None => format!("{title}\n{blurb}"),
    };
    let group = tool.group().iter().filter(|t| **t != tool).map(|t| crate::i18n::tr(t.title())).collect::<Vec<_>>();
    let group = (!group.is_empty()).then(|| format!("{} {}", crate::i18n::tr("Also in this group:"), group.join(", ")));
    for h in &how {
        text.push('\n');
        text.push_str(h);
    }
    if let Some(group) = &group {
        text.push_str(&format!("\n{group}"));
    }
    let frame = ui.ctx().cumulative_pass_nr();
    ui.ctx().data_mut(|d| d.insert_temp(Id::new("lc-tool-tip-shown"), Shown { frame, tool: tool.id().into(), text, bounds: egui::Rect::NOTHING }));
    let area = Area::new(Id::new("lc-tool-tip-area"))
        .order(Order::Tooltip)
        .interactable(false)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(pos)
        .constrain_to(screen)
        .show(ui.ctx(), |ui| {
            Frame::popup(ui.style()).fill(t.chrome).stroke(Stroke::new(1.0, t.button_border)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.set_max_width(width - 20.0);
                ui.horizontal_top(|ui| {
                    let (slot, _) = ui.allocate_exact_size(vec2(ICON, ICON), Sense::hover());
                    if let Some(image) = crate::icons::tool_name(tool.id()).and_then(|name| crate::icons::tool_icon(ui, name, ICON)) {
                        image.paint_at(ui, slot);
                    } else {
                        paint(ui.painter(), slot.shrink(ICON * 0.11), tool.icon(), t.icon);
                    }
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.set_max_width(width - 20.0 - ICON - 8.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new(title).font(t.semibold(13.5)).color(t.text));
                            if let Some(k) = &key {
                                chip(ui, &t, k);
                            }
                        });
                        ui.add_space(2.0);
                        ui.add(egui::Label::new(egui::RichText::new(blurb).font(t.font(12.5)).color(t.text)).wrap());
                    });
                });
                if !how.is_empty() {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(crate::i18n::tr("How to use")).font(t.semibold(11.5)).color(t.text_dim));
                    for line in &how {
                        ui.horizontal_top(|ui| {
                            ui.label(egui::RichText::new("•").font(t.font(12.0)).color(t.text_disabled));
                            ui.add(egui::Label::new(egui::RichText::new(line).font(t.font(12.0)).color(t.text_dim)).wrap());
                        });
                    }
                }
                if let Some(group) = &group {
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(egui::RichText::new(group).size(11.5).color(t.text_dim)).wrap());
                }
            });
        });
    ui.ctx().data_mut(|d| {
        if let Some(mut tip) = d.get_temp::<Shown>(Id::new("lc-tool-tip-shown")) {
            tip.bounds = area.response.rect;
            d.insert_temp(Id::new("lc-tool-tip-shown"), tip);
        }
    });
}

/// The shortcut chip: a small rounded key cap.
fn chip(ui: &mut egui::Ui, t: &Tokens, key: &str) {
    let galley = ui.painter().layout_no_wrap(key.to_string(), t.semibold(11.5), t.text);
    let size = (galley.size() + vec2(10.0, 4.0)).max(vec2(20.0, 18.0));
    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(r, 4.0, t.field);
    ui.painter().rect_stroke(r, 4.0, Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
    ui.painter().galley(r.center() - galley.size() / 2.0, galley, t.text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_strip_tool_has_a_complete_entry() {
        for t in StripTool::ALL {
            let e = entry(t);
            assert!(e.blurb.len() > 10, "{t:?}");
            assert!((2..=4).contains(&e.how.len()), "{t:?}");
            assert!(!t.title().is_empty());
            assert!(crate::icons::tool_name(t.id()).is_some(), "{t:?} needs a colour tooltip icon");
        }
    }

    #[test]
    fn shortcuts_come_from_the_command_table() {
        assert_eq!(StripTool::Edit.shortcut().as_deref(), Some("E"));
        assert_eq!(StripTool::RedEye.shortcut(), None);
        assert!(StripTool::Presets.shortcut().unwrap().contains('P'));
    }
}
