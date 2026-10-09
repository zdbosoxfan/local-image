//! Rich hover tips for the tool strip at the right of Develop: the tool's name and shortcut, one
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
}

impl StripTool {
    pub const ALL: [StripTool; 11] = [
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
        }
    }

    /// The command the button runs.
    pub fn command(self) -> Option<&'static str> {
        match self {
            StripTool::More => None,
            _ => Some(match self {
                StripTool::Presets => "panel.presets",
                StripTool::Edit => "panel.edit",
                StripTool::Crop => "panel.crop",
                StripTool::Remove => "panel.remove",
                StripTool::Masking => "panel.masking",
                StripTool::RedEye => "panel.redeye",
                StripTool::Versions => "panel.versions",
                StripTool::Activity => "panel.activity",
                StripTool::Keywords => "panel.keywords",
                _ => "panel.info",
            }),
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
        }
    }

    /// The shortcut in this platform's notation: from the command table, `None` when unbound.
    pub fn shortcut(self) -> Option<String> {
        let id = self.command()?;
        crate::menus::ui_commands().find(|c| c.0 == id).and_then(|c| c.2).map(|sc| crate::menubar::shortcut_text(sc, cfg!(target_os = "macos")))
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
        StripTool::Edit => Entry { blurb: "Sliders for light, colour, effects, detail and lens corrections.", how: &[] },
        StripTool::Crop => Entry {
            blurb: "Crops, straightens and rotates the photo.",
            how: &["Drag along the horizon to straighten; double-click for Auto.", "Press X to swap the aspect ratio."],
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
        StripTool::RedEye => {
            Entry { blurb: "Fixes red pupils in flash photos.", how: &["Drag over an eye; the pupil inside is found automatically."] }
        }
        StripTool::Versions => Entry {
            blurb: "Saves snapshots of your edit so you can return to them.",
            how: &["Create Version saves the current edit.", "Restore on a version brings it back."],
        },
        StripTool::Activity => Entry { blurb: "Lists the edits made to this photo.", how: &[] },
        StripTool::Keywords => Entry { blurb: "Tags photos with keywords.", how: &["Click photos in the grid to toggle the keyword; Esc stops."] },
        StripTool::Info => Entry { blurb: "Shows the photo's file, camera and exposure details.", how: &[] },
        StripTool::More => Entry { blurb: "Opens the About window.", how: &[] },
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
    shown: f64,
}

/// What the last frame's open tip said (for tests and automation).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    pub frame: u64,
    pub tool: String,
    pub text: String,
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
    if mode == ToolTipMode::Off {
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
    let held = ctx.input(|i| i.pointer.any_down());
    if !resp.hovered() || held {
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
    if now - mem.shown < LINGER || now - since >= DELAY {
        mem.shown = now;
        draw(ui, resp, tool);
        ctx.request_repaint();
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
    for h in &how {
        text.push('\n');
        text.push_str(h);
    }
    ui.ctx().data_mut(|d| d.insert_temp(Id::new("lc-tool-tip-shown"), Shown { frame: ui.ctx().cumulative_pass_nr(), tool: tool.id().into(), text }));
    Area::new(Id::new("lc-tool-tip-area"))
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
                    paint(ui.painter(), slot.shrink(ICON * 0.11), tool.icon(), t.icon);
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
            });
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
            assert!(e.how.len() <= 4, "{t:?}");
            assert!(!t.title().is_empty());
        }
    }

    #[test]
    fn shortcuts_come_from_the_command_table() {
        assert_eq!(StripTool::Edit.shortcut().as_deref(), Some("E"));
        assert_eq!(StripTool::RedEye.shortcut(), None);
        assert!(StripTool::Presets.shortcut().unwrap().contains('P'));
    }
}
