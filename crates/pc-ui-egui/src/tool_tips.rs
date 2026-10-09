//! Rich hover tips for the toolbar's tool buttons: the tool's name and shortcut, one sentence on
//! what it does, a few "how to use" lines and (for a flyout group) its siblings, beside a larger
//! icon. The text is one table, [`entry`], so nothing is scattered through the toolbar code.
//!
//! Interface › Tool tips picks Rich (this), Simple (the name and shortcut in a plain tooltip) or
//! Off. Timing is Photoshop's: a short delay before the first tip, then moving to another tool
//! shows its tip straight away; the tip hides while a button is held (a drag, a long press) and
//! while a flyout is open.
//!
//! Every gesture line was read off the tool's implementation (the modifier tables in `canvas`,
//! `tool_feedback`, `retouch_ui`, `lasso_ui`, `magnetic_lasso_ui`, `crop_ui`, `move_mods`,
//! `direct_select`, `stroke_constraint`); a tool whose gestures are not special has none.
//! Modifier names are the platform's (`⌥`/`Alt`, `⇧`/`Shift`, `⌘`/`Ctrl`).

use egui::{Area, Frame, Id, Order, Response, Sense, Stroke, vec2};
use photocraft_algo::liquify::LiquifyTool;
use photocraft_engine::prefs::ToolTips;

use crate::icons;
use crate::state::Tool;
use crate::theme::Tokens;

/// Seconds before the first tip opens.
pub const DELAY: f64 = 0.5;
/// A tip counts as still open this long after it was last drawn: moving on to the next tool in
/// that time shows its tip at once.
pub const LINGER: f64 = 0.3;
/// Widest a tip gets.
pub const MAX_WIDTH: f32 = 320.0;
/// Size of the icon slot at the left of a tip (coloured icons will fill it later).
pub const ICON: f32 = 32.0;

/// The words of one tip.
pub struct Entry {
    pub blurb: String,
    pub how: Vec<String>,
}

/// Fill the `{alt}`, `{shift}` and `{cmd}` placeholders with the platform's modifier names.
fn mods(s: &str) -> String {
    crate::i18n::fmt(s, &[("alt", &crate::shortcuts::pretty("Alt")), ("shift", &crate::shortcuts::pretty("Shift")), ("cmd", &crate::shortcuts::pretty("Cmd"))])
}

fn e(blurb: &str, how: &[&str]) -> Entry {
    Entry { blurb: blurb.to_string(), how: how.iter().map(|h| mods(h)).collect() }
}

/// The tip text of `tool`. Exhaustive, so a new tool cannot ship without one.
pub fn entry(tool: Tool) -> Entry {
    let mut entry = match tool {
        Tool::Move => e(
            tl!("Moves the selected layers or the selection."),
            &[
                tl!("Drag to move; hold {shift} to keep to one axis."),
                tl!("{alt}-drag to move a copy."),
                tl!("{cmd}-click picks the layer under the pointer (it switches Auto-Select for that click)."),
                tl!("Arrow keys nudge by 1 pixel, {shift}+arrow by 10."),
            ],
        ),
        Tool::RectMarquee => e(
            tl!("Selects a rectangular area."),
            &[tl!("Drag to select; {shift}-drag adds to the selection, {alt}-drag subtracts."), tl!("Hold {shift} and {alt} together to intersect.")],
        ),
        Tool::EllipseMarquee => e(
            tl!("Selects an oval or circular area."),
            &[tl!("Drag to select; {shift}-drag adds to the selection, {alt}-drag subtracts."), tl!("Hold {shift} and {alt} together to intersect.")],
        ),
        Tool::Lasso => e(
            tl!("Selects by drawing a freehand outline."),
            &[
                tl!("Drag to draw; release to close the outline."),
                tl!("Hold {alt} while drawing for straight segments; let go to go freehand again."),
                tl!("Hold {shift} before you start to add, {alt} to subtract."),
            ],
        ),
        Tool::PolygonLasso => e(
            tl!("Selects with a straight-sided outline."),
            &[
                tl!("Click to place each corner."),
                tl!("Click the first point, or double-click, to close the shape."),
                tl!("Hold {shift} for the first click to add, {alt} to subtract."),
            ],
        ),
        Tool::MagneticLasso => e(
            tl!("Selects by following the edge of an object."),
            &[
                tl!("Click to start, then move along the edge; click to fasten a point."),
                tl!("{alt}-click adds a straight segment; drag for freehand."),
                tl!("Double-click, or click the first point, to close the outline."),
            ],
        ),
        Tool::ObjectSelection => e(
            tl!("Selects an object with the local AI model."),
            &[tl!("Click an object, or drag a box around it."), tl!("{shift} adds to the selection, {alt} subtracts.")],
        ),
        Tool::QuickSelection => {
            e(tl!("Paints a selection that grows out to the edges."), &[tl!("Drag over the area you want."), tl!("Hold {alt} while dragging to subtract.")])
        }
        Tool::MagicWand => e(
            tl!("Selects pixels of a similar colour with one click."),
            &[
                tl!("Click a colour; Tolerance and Contiguous are in the options bar."),
                tl!("{shift}-click adds, {alt}-click subtracts, {shift}+{alt}-click intersects."),
            ],
        ),
        Tool::Crop => e(
            tl!("Trims the image to a rectangle."),
            &[
                tl!("Drag to draw the crop, then drag the handles to adjust."),
                tl!("Hold {shift} to keep the ratio and {alt} to resize from the centre."),
                tl!("Press Enter to apply, Esc to cancel."),
            ],
        ),
        Tool::PerspectiveCrop => e(
            tl!("Crops to a four-cornered shape and straightens it, as for a photographed page or building."),
            &[tl!("Drag a frame, then drag its four corners onto the subject."), tl!("Press Enter to apply, Esc to cancel.")],
        ),
        Tool::Slice => e(
            tl!("Divides the image into slices for web export."),
            &[tl!("Drag a rectangle to create a slice."), tl!("Use Slice Select to move an existing slice.")],
        ),
        Tool::SliceSelect => e(tl!("Selects and adjusts existing slices."), &[tl!("Click a slice to select it."), tl!("Drag the selected slice to move it.")]),
        Tool::Eyedropper => {
            e(tl!("Samples a colour from the image."), &[tl!("Click to set the foreground colour."), tl!("{alt}-click sets the background colour.")])
        }
        Tool::Ruler => e(
            tl!("Measures distances and angles on the image."),
            &[tl!("Drag between two points to measure."), tl!("{alt}-drag an endpoint to measure an angle.")],
        ),
        Tool::Note => e(
            tl!("Adds a text note to the document."),
            &[tl!("Click to add a note and edit its text in the Notes panel."), tl!("Drag a note marker to move it.")],
        ),
        Tool::Count => e(
            tl!("Counts items in the image by clicking on each."),
            &[tl!("Click each item to add a numbered marker."), tl!("{alt}-click a marker to remove it.")],
        ),
        Tool::SpotHealing => e(
            tl!("Removes a blemish by blending in the pixels around it."),
            &[tl!("Paint over the spot."), tl!("Hold {shift} for a straight line from the last stroke.")],
        ),
        Tool::Healing => e(
            tl!("Repairs an area using sampled pixels, matching light and texture."),
            &[tl!("{alt}-click to set the source point."), tl!("Then paint over the area to repair.")],
        ),
        Tool::Patch => e(
            tl!("Repairs a selected area from another area."),
            &[tl!("Draw around the flaw, then drag the selection onto clean pixels."), tl!("Drag from inside an existing selection to move it.")],
        ),
        Tool::ContentAwareMove => e(
            tl!("Moves a selected object and fills the gap it leaves."),
            &[tl!("Draw around the object, then drag the selection to its new place."), tl!("Choose Move or Extend in the options bar.")],
        ),
        Tool::RedEye => e(
            tl!("Removes red from the pupils in flash photos."),
            &[tl!("Click on the red pupil."), tl!("Set Pupil Size and Darken Amount in the options bar.")],
        ),
        Tool::Brush => e(
            tl!("Paints soft or hard strokes in the foreground colour."),
            &[
                tl!("Drag to paint; hold {shift} to keep a straight line."),
                tl!("{shift}-click draws a line from the end of the last stroke."),
                tl!("Hold {alt} to pick a colour from the image."),
            ],
        ),
        Tool::Pencil => e(
            tl!("Paints hard-edged strokes."),
            &[
                tl!("Drag to paint; hold {shift} to keep a straight line."),
                tl!("{shift}-click draws a line from the end of the last stroke."),
                tl!("Hold {alt} to pick a colour from the image."),
            ],
        ),
        Tool::MixerBrush => {
            e(tl!("Paints with colours that mix with the canvas, like wet paint."), &[tl!("Drag to paint; hold {shift} to keep a straight line.")])
        }
        Tool::CloneStamp => e(
            tl!("Paints with pixels copied from another part of the image."),
            &[tl!("{alt}-click to set the source point."), tl!("Then paint where you want the copy.")],
        ),
        Tool::HistoryBrush => e(tl!("Paints back the document’s opening state."), &[tl!("Drag to paint; hold {shift} to keep a straight line.")]),
        Tool::Eraser => e(tl!("Erases pixels as you drag."), &[tl!("Drag to erase; hold {shift} to keep a straight line.")]),
        Tool::BackgroundEraser => e(tl!("Erases the background colour under the brush, keeping edges."), &[tl!("Drag along the edge of the subject.")]),
        Tool::MagicEraser => e(
            tl!("Erases all pixels of a similar colour with one click."),
            &[tl!("Click the colour to erase."), tl!("Set Tolerance and Contiguous in the options bar.")],
        ),
        Tool::Gradient => e(
            tl!("Fills with a blend between colours."),
            &[
                tl!("Drag from where the blend starts to where it ends."),
                tl!("Hold {shift} to snap the angle to 45 degrees."),
                tl!("Hold {alt} to pick a colour from the image."),
            ],
        ),
        Tool::PaintBucket => e(
            tl!("Fills an area of similar colour with the foreground colour."),
            &[tl!("Click inside the area to fill."), tl!("Hold {alt} to pick a colour from the image.")],
        ),
        Tool::Blur => e(tl!("Softens the pixels you paint over."), &[tl!("Drag to blur; hold {shift} to keep a straight line.")]),
        Tool::Sharpen => e(tl!("Sharpens the pixels you paint over."), &[tl!("Drag to sharpen; hold {shift} to keep a straight line.")]),
        Tool::Smudge => e(tl!("Smears pixels in the direction you drag, like wet paint."), &[tl!("Drag to smudge; hold {shift} to keep a straight line.")]),
        Tool::Dodge => e(tl!("Lightens the areas you paint over."), &[tl!("Drag to lighten; hold {shift} to keep a straight line.")]),
        Tool::Burn => e(tl!("Darkens the areas you paint over."), &[tl!("Drag to darken; hold {shift} to keep a straight line.")]),
        Tool::Sponge => {
            e(tl!("Raises or lowers the colour saturation where you paint."), &[tl!("Drag to change saturation; hold {shift} to keep a straight line.")])
        }
        Tool::Pen => e(
            tl!("Draws precise paths and shapes with anchor points."),
            &[tl!("Click to add corners; drag to create smooth handles."), tl!("Click the first point to close, Enter to finish an open path.")],
        ),
        Tool::PathSelection => {
            e(tl!("Selects and moves whole paths."), &[tl!("Choose the active shape, work path or targeted vector mask."), tl!("Drag to move its whole path.")])
        }
        Tool::DirectSelection => e(
            tl!("Selects and edits individual anchor points and handles."),
            &[
                tl!("Click an anchor or handle; drag a box to select several."),
                tl!("{shift}-click toggles an anchor in the selection."),
                tl!("{alt}-drag a handle to move it on its own."),
            ],
        ),
        Tool::Type => e(tl!("Adds text to the image."), &[tl!("Click to type a line of text."), tl!("Drag to make a box for paragraph text.")]),
        Tool::VerticalType => {
            e(tl!("Adds text that runs top to bottom."), &[tl!("Click to type a line of text."), tl!("Drag to make a box for paragraph text.")])
        }
        Tool::Rectangle => e(
            tl!("Draws rectangles as editable shape layers."),
            &[tl!("Drag to draw; {shift} keeps equal width and height."), tl!("Hold {alt} to draw from the centre.")],
        ),
        Tool::EllipseShape => e(
            tl!("Draws ellipses and circles as editable shape layers."),
            &[tl!("Drag to draw; {shift} keeps equal width and height."), tl!("Hold {alt} to draw from the centre.")],
        ),
        Tool::Triangle => e(
            tl!("Draws triangles as editable shape layers."),
            &[tl!("Drag to draw; {shift} keeps equal width and height."), tl!("Hold {alt} to draw from the centre.")],
        ),
        Tool::Polygon => e(
            tl!("Draws polygons with any number of sides."),
            &[tl!("Drag to draw; {shift} keeps equal width and height."), tl!("Hold {alt} to draw from the centre.")],
        ),
        Tool::Line => e(
            tl!("Draws straight lines as editable shape layers."),
            &[tl!("Drag between the endpoints."), tl!("Hold {shift} to snap the angle to 45 degrees.")],
        ),
        Tool::CustomShape => e(
            tl!("Draws a shape chosen from the shape library."),
            &[tl!("Drag to draw; {shift} keeps equal width and height."), tl!("Hold {alt} to draw from the centre.")],
        ),
        Tool::Hand => e(
            tl!("Pans the view around the image."),
            &[
                tl!("Drag to move the image."),
                tl!("Hold Space with any other tool to pan temporarily."),
                tl!("You can also drag with the middle mouse button to pan."),
            ],
        ),
        Tool::Zoom => e(
            tl!("Magnifies or reduces the view."),
            &[tl!("Click to zoom in, {alt}-click to zoom out."), tl!("Drag right to zoom in and left to zoom out, or drag a box when Scrubby Zoom is off.")],
        ),
        Tool::AiRemove => e(
            tl!("Removes distractions with the local AI model."),
            &[tl!("Paint over what you want gone."), tl!("When you let go, the repair lands on its own layer.")],
        ),
        Tool::AiCutout => e(
            tl!("Cuts out the subject with the local AI model."),
            &[
                tl!("Use Remove Background in the options bar to make the mask."),
                tl!("Choose Erase or Restore in the options bar, then paint to refine the mask."),
            ],
        ),
    };
    if entry.how.len() < 2 && tool.is_brushlike() {
        entry.how.push(tl!("Set the brush size in the options bar before painting.").into());
    }
    entry
}

/// The tool's single-key shortcut (`B`), or `None` for a tool with no key.
pub fn shortcut(session: &photocraft_engine::Session, tool: Tool) -> Option<String> {
    crate::shortcuts::tool_shortcut(session, tool).map(|s| crate::shortcuts::pretty(&s))
}

/// Per-frame hover memory, in the context's temp data.
#[derive(Clone, Copy, Default)]
struct Hover {
    /// The button the pointer is on and since when.
    over: Option<(Id, f64)>,
    /// When a tip was last drawn.
    shown: Option<f64>,
}

/// What a tip shows, independent of which tool strip it belongs to.
pub struct Spec {
    pub title: String,
    pub key: Option<String>,
    pub icon: &'static str,
    pub entry: Entry,
    /// Names of the other tools in the same flyout group.
    pub group: Vec<String>,
}

/// The spec of a toolbar tool; `group` is the whole flyout slot.
pub fn spec(session: &photocraft_engine::Session, tool: Tool, group: &[Tool]) -> Spec {
    let mut entry = entry(tool);
    if tool == Tool::Hand {
        let key = session.prefs().shortcut("tools.temporary.hand", Some("Space"));
        if let Some(key) = key {
            entry.how[1] = crate::i18n::fmt(tl!("Hold {key} with any other tool to pan temporarily."), &[("key", &crate::shortcuts::pretty(key))]);
        } else {
            entry.how.remove(1);
        }
    }
    Spec {
        title: tl!(tool.label()).to_string(),
        key: shortcut(session, tool),
        icon: icons::tool_icon_name(tool),
        entry,
        group: group.iter().filter(|g| **g != tool).map(|g| tl!(g.label()).to_string()).collect(),
    }
}

/// Attach the tip of `tool` to its button. `group` is the whole flyout slot (the button's tool
/// included); `blocked` is true while a flyout is open. The tip is drawn here.
pub fn attach(session: &photocraft_engine::Session, ui: &egui::Ui, resp: &Response, tool: Tool, group: &[Tool], blocked: bool) {
    attach_spec(session, ui, resp, blocked, tl!(tool.label()), shortcut(session, tool), || spec(session, tool, group));
}

/// The Liquify strip's tip text.
pub fn liquify_entry(tool: LiquifyTool) -> Entry {
    let mut entry = match tool {
        LiquifyTool::ForwardWarp => e(tl!("Pushes pixels forward as you drag."), &[tl!("Drag across the area to push it along.")]),
        LiquifyTool::Reconstruct => {
            e(tl!("Undoes warping where you paint."), &[tl!("Paint over a distorted area to restore it; hold still to keep restoring.")])
        }
        LiquifyTool::Smooth => e(tl!("Smooths out the distortion where you paint."), &[tl!("Paint over an area; hold still to keep smoothing.")]),
        LiquifyTool::TwirlCw => e(tl!("Spins pixels clockwise."), &[tl!("Click or hold still to keep twirling.")]),
        LiquifyTool::TwirlCcw => e(tl!("Spins pixels counterclockwise."), &[tl!("Click or hold still to keep twirling.")]),
        LiquifyTool::Pucker => e(tl!("Pulls pixels in toward the centre of the brush."), &[tl!("Click or hold still to keep pinching.")]),
        LiquifyTool::Bloat => e(tl!("Pushes pixels out from the centre of the brush."), &[tl!("Click or hold still to keep swelling.")]),
        LiquifyTool::PushLeft => e(tl!("Shifts pixels sideways as you drag."), &[tl!("Drag to move pixels to the left of your direction.")]),
        LiquifyTool::Freeze => e(tl!("Protects areas from being warped."), &[tl!("Paint over the areas to protect.")]),
        LiquifyTool::Thaw => e(tl!("Removes protection from frozen areas."), &[tl!("Paint over the frozen areas to release them.")]),
        LiquifyTool::LassoMask => e(tl!("Freezes an area you outline."), &[tl!("Drag to freeze an outlined area."), tl!("{alt}-drag to thaw it.")]),
        LiquifyTool::ReconstructAll | LiquifyTool::FreezeAll | LiquifyTool::ThawAll | LiquifyTool::InvertFreeze => {
            e(tl!("Changes the whole image at once."), &[])
        }
    };
    if entry.how.len() < 2 {
        entry.how.push(tl!("Set Size and Pressure in Brush Tool Options.").into());
    }
    entry
}

/// Attach the tip of a Liquify tool; its strip has no groups.
pub fn attach_liquify(session: &photocraft_engine::Session, ui: &egui::Ui, resp: &Response, tool: LiquifyTool, key: &str, icon: &'static str) {
    let key = crate::shortcuts::local_tool_shortcut(session, &crate::liquify_ui::shortcut_id(tool), key).map(|s| crate::shortcuts::pretty(&s));
    let k2 = key.clone();
    attach_spec(session, ui, resp, false, tl!(tool.label()), key, || Spec {
        title: tl!(tool.label()).to_string(),
        key: k2,
        icon,
        entry: liquify_entry(tool),
        group: Vec::new(),
    });
}

/// Camera Raw has a modal key map, separate from the main toolbar.
#[derive(Clone, Copy, Debug)]
pub(crate) enum CameraRawTool {
    Zoom,
    Hand,
    Sampler,
}

impl CameraRawTool {
    pub(crate) const ALL: [Self; 3] = [Self::Zoom, Self::Hand, Self::Sampler];
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Zoom => tl!("Zoom Tool"),
            Self::Hand => tl!("Hand Tool"),
            Self::Sampler => tl!("Color Sampler Tool"),
        }
    }
    pub(crate) fn shortcut_id(self) -> &'static str {
        match self {
            Self::Zoom => "tools.cameraRaw.zoom",
            Self::Hand => "tools.cameraRaw.hand",
            Self::Sampler => "tools.cameraRaw.sampler",
        }
    }
    pub(crate) fn default_shortcut(self) -> &'static str {
        match self {
            Self::Zoom => "Z",
            Self::Hand => "H",
            Self::Sampler => "S",
        }
    }
    pub(crate) fn binding(self, session: &photocraft_engine::Session) -> Option<String> {
        crate::shortcuts::local_tool_shortcut(session, self.shortcut_id(), self.default_shortcut())
    }
    pub(crate) fn icon(self) -> &'static str {
        match self {
            Self::Zoom => "zoom-in",
            Self::Hand => "hand",
            Self::Sampler => "pipette",
        }
    }
    fn entry(self) -> Entry {
        match self {
            Self::Zoom => e(
                tl!("Magnifies or reduces the Camera Raw preview."),
                &[
                    tl!("Click to switch between Fit and 100%; {alt}-click zooms out."),
                    tl!("Drag right to zoom in and left to zoom out; {cmd}-drag draws a zoom box."),
                    tl!("Double-click the tool button to fit the image."),
                ],
            ),
            Self::Hand => e(
                tl!("Pans the Camera Raw preview."),
                &[tl!("Drag to move the image; hold Space to pan with another tool."), tl!("Double-click the image or tool button to fit the image.")],
            ),
            Self::Sampler => e(
                tl!("Places colour readouts on the Camera Raw preview."),
                &[tl!("Click the image to add a sampler, then drag its marker to move it."), tl!("{alt}-click a marker to remove it.")],
            ),
        }
    }
}

pub(crate) fn attach_camera_raw(session: &photocraft_engine::Session, ui: &egui::Ui, resp: &Response, tool: CameraRawTool) {
    let key = tool.binding(session).map(|s| crate::shortcuts::pretty(&s));
    attach_spec(session, ui, resp, false, tool.title(), key.clone(), || Spec {
        title: tool.title().into(),
        key,
        icon: tool.icon(),
        entry: tool.entry(),
        group: Vec::new(),
    });
}

/// The tip for a button, per Interface › Tool tips.
fn attach_spec(
    session: &photocraft_engine::Session,
    ui: &egui::Ui,
    resp: &Response,
    blocked: bool,
    title: &str,
    key: Option<String>,
    make: impl FnOnce() -> Spec,
) {
    let prefs = session.prefs();
    let on = prefs.interface.show_tooltips && prefs.tools.show_tooltips;
    let style = prefs.interface.tool_tips;
    // The button's accessible name is its tool's name, whatever the tip does.
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, resp.enabled(), title));
    let blocked = blocked || ui.ctx().input(|i| i.pointer.any_down()) || egui::Popup::is_any_open(ui.ctx());
    if !on || style == ToolTips::Off || blocked {
        ui.ctx().data_mut(|d| {
            d.remove::<Hover>(Id::new("tool-tip-hover"));
            d.remove::<Shown>(Id::new("tool-tip-shown"));
        });
        return;
    }
    if style == ToolTips::Simple {
        let tip = match &key {
            Some(k) => format!("{title}  ({k})"),
            None => title.to_string(),
        };
        resp.clone().on_hover_text(tip);
        return;
    }
    let ctx = ui.ctx();
    let now = ctx.input(|i| i.time);
    let mem_id = Id::new("tool-tip-hover");
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
        draw(ui, resp, &make());
    } else {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((DELAY - (now - since)).max(0.01)));
    }
    ctx.data_mut(|d| d.insert_temp(mem_id, mem));
}

/// What the open tip says (title, shortcut, sentence, how-to lines, group), for tests and
/// automation; valid for the frame it was drawn in and the next.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub frame: u64,
    pub title: String,
    pub key: Option<String>,
    pub text: String,
    pub bounds: egui::Rect,
}

/// The tip open now, if any.
pub fn shown(ctx: &egui::Context) -> Option<Shown> {
    let s: Shown = ctx.data(|d| d.get_temp(Id::new("tool-tip-shown")))?;
    (s.frame + 1 >= ctx.cumulative_pass_nr()).then_some(s)
}

fn draw(ui: &egui::Ui, resp: &Response, sp: &Spec) {
    let t = Tokens::get(ui.ctx());
    {
        let mut text = sp.entry.blurb.clone();
        for h in &sp.entry.how {
            text.push('\n');
            text.push_str(h);
        }
        if !sp.group.is_empty() {
            text.push_str(&format!("\n{} {}", tl!("Also in this group:"), sp.group.join(", ")));
        }
        let s = Shown { frame: ui.ctx().cumulative_pass_nr(), title: sp.title.clone(), key: sp.key.clone(), text, bounds: egui::Rect::NOTHING };
        ui.ctx().data_mut(|d| d.insert_temp(Id::new("tool-tip-shown"), s));
    }
    let screen = ui.ctx().content_rect();
    let width = MAX_WIDTH.min(screen.width() - 16.0);
    let pos = resp.rect.right_top() + vec2(8.0, 0.0);
    let area = Area::new(Id::new("tool-tip-area")).order(Order::Tooltip).interactable(false).fixed_pos(pos).constrain_to(screen).show(ui.ctx(), |ui| {
        Frame::popup(ui.style()).fill(t.card).stroke(Stroke::new(1.0, t.card_border)).corner_radius(t.radius).inner_margin(egui::Margin::same(10)).show(
            ui,
            |ui| {
                ui.set_max_width(width - 20.0);
                ui.horizontal_top(|ui| {
                    let (slot, _) = ui.allocate_exact_size(vec2(ICON, ICON), Sense::hover());
                    icons::tool_icon(ui, sp.icon, ICON).paint_at(ui, slot);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.set_max_width(width - 20.0 - ICON - 8.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new(&sp.title).font(crate::theme::semibold(13.5)).color(t.text));
                            if let Some(k) = &sp.key {
                                chip(ui, &t, k);
                            }
                        });
                        ui.add_space(2.0);
                        ui.add(egui::Label::new(egui::RichText::new(&sp.entry.blurb).size(12.5).color(t.text)).wrap());
                    });
                });
                if !sp.entry.how.is_empty() {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(tl!("How to use")).font(crate::theme::semibold(11.5)).color(t.text_dim));
                    for line in &sp.entry.how {
                        ui.horizontal_top(|ui| {
                            ui.label(egui::RichText::new("•").size(12.0).color(t.text_faint));
                            ui.add(egui::Label::new(egui::RichText::new(line).size(12.0).color(t.text_dim)).wrap());
                        });
                    }
                }
                if !sp.group.is_empty() {
                    ui.add_space(6.0);
                    let line = format!("{} {}", tl!("Also in this group:"), sp.group.join(", "));
                    ui.add(egui::Label::new(egui::RichText::new(line).size(11.5).color(t.text_faint)).wrap());
                }
            },
        );
    });
    ui.ctx().data_mut(|d| {
        if let Some(mut tip) = d.get_temp::<Shown>(Id::new("tool-tip-shown")) {
            tip.bounds = area.response.rect;
            d.insert_temp(Id::new("tool-tip-shown"), tip);
        }
    });
}

/// The shortcut chip: a small rounded key cap.
fn chip(ui: &mut egui::Ui, t: &Tokens, key: &str) {
    let galley = ui.painter().layout_no_wrap(key.to_string(), crate::theme::semibold(11.5), t.text);
    let size = galley.size() + vec2(10.0, 4.0);
    let (r, _) = ui.allocate_exact_size(size.max(vec2(20.0, 18.0)), Sense::hover());
    ui.painter().rect_filled(r, 4.0, t.field);
    ui.painter().rect_stroke(r, 4.0, Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
    ui.painter().galley(r.center() - galley.size() / 2.0, galley, t.text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_complete_entry() {
        for t in LiquifyTool::ALL {
            let entry = liquify_entry(t);
            assert!(entry.blurb.len() > 10, "{t:?}");
            assert!((2..=4).contains(&entry.how.len()), "{t:?}");
        }
        for t in CameraRawTool::ALL {
            let entry = t.entry();
            assert!(entry.blurb.len() > 10, "{t:?}");
            assert!((2..=4).contains(&entry.how.len()), "{t:?}");
        }
        for t in Tool::ALL {
            let en = entry(t);
            assert!(en.blurb.len() > 10, "{t:?} has no description");
            assert!((2..=4).contains(&en.how.len()), "{t:?}: two to four verified how-to lines");
            for h in &en.how {
                assert!(!h.contains('{'), "{t:?}: unfilled placeholder in {h}");
            }
        }
    }

    use crate::PhotocraftApp;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use serde_json::json;

    fn harness() -> Harness<'static, PhotocraftApp> {
        Harness::builder().with_size(egui::vec2(1100.0, 760.0)).with_step_dt(1.0 / 60.0).with_max_steps(64).build_eframe(|cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default())
        })
    }

    fn settle(h: &mut Harness<'_, PhotocraftApp>) {
        for _ in 0..40 {
            h.run_steps(1);
        }
    }

    #[test]
    fn tool_tips_icon_pixels_follow_colour_preference_and_keep_text() {
        use crate::tool_icon_tests::cpu;
        #[derive(Default)]
        struct CpuRenderer(cpu::TextureStore);
        impl egui_kittest::TestRenderer for CpuRenderer {
            fn handle_delta(&mut self, delta: &mut egui::TexturesDelta) {
                self.0.apply(std::mem::take(delta));
            }
            fn render(&mut self, ctx: &egui::Context, output: &egui::FullOutput) -> Result<image::RgbaImage, String> {
                let scale = output.pixels_per_point;
                let screen = ctx.content_rect().size() * scale;
                let primitives = ctx.tessellate(output.shapes.clone(), scale);
                let painted = cpu::paint(&primitives, &self.0, [screen.x as usize, screen.y as usize], scale, egui::Color32::from_gray(31));
                let pixels = painted.pixels.iter().flat_map(|p| p.to_array()).collect();
                image::RgbaImage::from_raw(painted.size[0] as u32, painted.size[1] as u32, pixels).ok_or_else(|| "invalid CPU image size".into())
            }
        }
        let mut h =
            Harness::builder().with_size(vec2(1100.0, 760.0)).with_step_dt(1.0 / 60.0).with_max_steps(64).renderer(CpuRenderer::default()).build_eframe(|cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default())
            });
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Brush Tool").hover();
        settle(&mut h);
        let icon_pixels = |h: &mut Harness<'_, PhotocraftApp>| {
            let tip = shown(&h.ctx).unwrap();
            let scale = h.output().pixels_per_point;
            let image = h.render().unwrap();
            let slot = egui::Rect::from_min_size(tip.bounds.min + vec2(10.0, 10.0), vec2(ICON, ICON));
            let min = slot.min * scale;
            let max = slot.max * scale;
            let pixels = &image;
            (min.y.max(0.0) as u32..(max.y as u32).min(image.height()))
                .flat_map(|y| {
                    (min.x.max(0.0) as u32..(max.x as u32).min(image.width())).map(move |x| {
                        let p = pixels.get_pixel(x, y);
                        egui::Color32::from_rgb(p[0], p[1], p[2])
                    })
                })
                .collect::<Vec<_>>()
        };
        let tip = shown(&h.ctx).unwrap();
        assert_eq!(tip.key.as_deref(), Some("B"));
        let colour = icon_pixels(&mut h);
        let saturation = |pixels: &[egui::Color32]| pixels.iter().filter(|p| p.r().max(p.g()).max(p.b()) - p.r().min(p.g()).min(p.b()) > 40).count();
        assert!(saturation(&colour) > 20, "the rich tip has a colour icon");
        h.state_mut().run("prefs.set", json!({"values": {"interface.toolIcons": "monochrome"}})).unwrap();
        h.run_steps(4);
        assert_eq!(shown(&h.ctx).unwrap().text, tip.text);
        let mono = icon_pixels(&mut h);
        assert_eq!(saturation(&mono), 0, "the rich tip respects Monochrome");
        let background = Tokens::get(&h.ctx).card;
        assert!(mono.iter().filter(|p| p.r().abs_diff(background.r()) > 25).count() > 20, "the monochrome glyph stays visible");
    }

    #[test]
    fn hovering_a_tool_shows_the_rich_tip_after_a_delay() {
        let mut h = harness();
        h.run_steps(2);
        h.get_by_label("Brush Tool").hover();
        h.run_steps(1);
        assert!(shown(&h.ctx).is_none(), "no tip before the delay");
        settle(&mut h);
        let tip = shown(&h.ctx).expect("the tip opened");
        assert_eq!(tip.title, "Brush Tool");
        assert!(tip.bounds.width() <= MAX_WIDTH + 2.0, "{:?}", tip.bounds);
        assert_eq!(tip.key.as_deref(), Some(Tool::Brush.key().to_string().as_str()), "the chip shows the tool's real key");
        for line in &entry(Tool::Brush).how {
            assert!(tip.text.contains(line.as_str()), "{line}");
            h.get_by_label(line);
        }
        h.get_by_label(&entry(Tool::Brush).blurb);
        // Grouped tools name their siblings.
        h.get_by_label("Lasso Tool").hover();
        settle(&mut h);
        let tip = shown(&h.ctx).unwrap();
        assert_eq!(tip.title, "Lasso Tool");
        assert!(tip.text.contains("Also in this group: Polygonal Lasso Tool, Magnetic Lasso Tool"), "{}", tip.text);
    }

    #[test]
    fn moving_to_another_tool_shows_its_tip_at_once() {
        let mut h = harness();
        h.run_steps(2);
        h.get_by_label("Brush Tool").hover();
        settle(&mut h);
        assert_eq!(shown(&h.ctx).unwrap().title, "Brush Tool");
        h.get_by_label("Clone Stamp Tool").hover();
        h.run_steps(2);
        assert_eq!(shown(&h.ctx).map(|s| s.title).as_deref(), Some("Clone Stamp Tool"));
    }

    #[test]
    fn simple_and_off_show_no_rich_tip() {
        for mode in ["simple", "off"] {
            let mut h = harness();
            h.state_mut().run("prefs.set", json!({"values": {"interface.toolTips": mode}})).unwrap();
            h.run_steps(2);
            h.get_by_label("Brush Tool").hover();
            settle(&mut h);
            assert!(shown(&h.ctx).is_none(), "{mode}");
            assert!(h.query_by_label(entry(Tool::Brush).blurb.as_str()).is_none(), "{mode}: no description");
        }
    }

    #[test]
    fn tool_tips_show_remapped_keys_and_hidden_tools_still_switch_by_key() {
        let mut h = harness();
        h.state_mut().run("edit.keyboardShortcuts", json!({"set": {"tools.select.Brush": "F6"}})).unwrap();
        h.state_mut().run("toolset.select", json!({"id": "ai"})).unwrap();
        h.key_press(egui::Key::F6);
        h.run_steps(3);
        assert_eq!(h.state().ui.tool, Tool::Brush);
        h.get_by_label("Brush Tool").hover();
        settle(&mut h);
        assert_eq!(shown(&h.ctx).unwrap().key.as_deref(), Some("F6"));
        h.state_mut().run("edit.keyboardShortcuts", json!({"set": {"tools.select.Brush": ""}})).unwrap();
        h.run_steps(2);
        assert_eq!(shown(&h.ctx).unwrap().key, None);
        h.key_press(egui::Key::V);
        h.run_steps(2);
        h.key_press(egui::Key::B);
        h.run_steps(2);
        assert_eq!(h.state().ui.tool, Tool::Move, "removed binding does not fire its default");
    }

    #[test]
    fn tool_tips_hand_usage_respects_remapped_and_unbound_temporary_keys() {
        let mut h = harness();
        h.state_mut().run("edit.keyboardShortcuts", json!({"set": {"tools.temporary.hand": "F6"}})).unwrap();
        h.run_steps(2);
        h.get_by_label("Hand Tool").hover();
        settle(&mut h);
        let tip = shown(&h.ctx).unwrap();
        assert_eq!(tip.key.as_deref(), Some("H"));
        assert!(tip.text.contains("Hold F6 with any other tool to pan temporarily."));
        h.state_mut().run("edit.keyboardShortcuts", json!({"set": {"tools.temporary.hand": ""}})).unwrap();
        h.run_steps(3);
        let tip = shown(&h.ctx).unwrap();
        assert!(!tip.text.contains("Hold F6") && !tip.text.contains("Hold Space"));
        h.get_by_label("Drag to move the image.");
        h.get_by_label("You can also drag with the middle mouse button to pan.");
    }

    #[test]
    fn tool_tips_hide_during_pointer_holds_and_flyouts() {
        let mut h = harness();
        h.run_steps(2);
        h.get_by_label("Lasso Tool").hover();
        settle(&mut h);
        assert!(shown(&h.ctx).is_some());
        let pos = h.get_by_role_and_label(egui::accesskit::Role::Button, "Lasso Tool").rect().center();
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(3);
        assert!(shown(&h.ctx).is_none());
        h.run_steps(24); // the long press opens the flyout
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(3);
        h.get_by_label("Polygonal Lasso Tool");
        h.get_by_label("Brush Tool").hover();
        settle(&mut h);
        assert!(shown(&h.ctx).is_none(), "a flyout blocks tips on every toolbar tool");
        h.get_by_label("Polygonal Lasso Tool").click();
        h.run_steps(3);
        assert_eq!(h.state().ui.tool, Tool::PolygonLasso);
    }

    #[test]
    fn tool_tips_simple_shows_only_name_and_effective_shortcut() {
        let mut h = harness();
        h.state_mut().run("prefs.set", json!({"values": {"interface.toolTips": "simple"}})).unwrap();
        h.state_mut().run("edit.keyboardShortcuts", json!({"set": {"tools.select.Brush": "F6"}})).unwrap();
        h.run_steps(2);
        h.get_by_label("Brush Tool").hover();
        h.run_steps(90);
        h.get_by_label("Brush Tool  (F6)");
        assert!(h.query_by_label(entry(Tool::Brush).blurb.as_str()).is_none());
        h.state_mut().run("prefs.set", json!({"values": {"interface.toolTips": "off"}})).unwrap();
        h.run_steps(3);
        assert!(h.query_by_label("Brush Tool  (F6)").is_none());
    }

    #[test]
    fn tool_tips_camera_raw_and_liquify_use_their_modal_bindings() {
        let mut h = harness();
        h.state_mut().run("file.new", json!({"width": 16, "height": 16})).unwrap();
        h.state_mut().run("layer.new.layer", json!({})).unwrap();
        h.state_mut().run("edit.fill", json!({"color": "#808080"})).unwrap();
        let ctx = h.ctx.clone();
        crate::menus::invoke(h.state_mut(), &ctx, "filter.cameraRaw", json!({})).unwrap();
        h.state_mut().session.edit_prefs(|p| {
            p.shortcuts.insert("tools.cameraRaw.sampler".into(), "F6".into());
        });
        h.run_steps(4);
        h.get_all_by_label("Color Sampler Tool").last().unwrap().hover();
        settle(&mut h);
        assert_eq!(shown(&h.ctx).unwrap().key.as_deref(), Some("F6"));
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Color Sampler Tool").click();
        h.run_steps(2);
        assert!(h.state().ui.camera_raw_scope.sampler_tool);
        h.key_press(egui::Key::F6);
        h.run_steps(2);
        assert!(!h.state().ui.camera_raw_scope.sampler_tool);
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        crate::menus::invoke(h.state_mut(), &ctx, "filter.liquify", json!({})).unwrap();
        h.state_mut().session.edit_prefs(|p| {
            p.shortcuts.insert("tools.liquify.Freeze".into(), "F6".into());
        });
        h.run_steps(4);
        h.get_by_label(tl!(LiquifyTool::Freeze.label())).hover();
        settle(&mut h);
        assert_eq!(shown(&h.ctx).unwrap().key.as_deref(), Some("F6"));
        h.key_press(egui::Key::F6);
        h.run_steps(2);
        assert_eq!(h.state().distort.liquify.as_ref().unwrap().opts.tool, LiquifyTool::Freeze);
    }

    #[test]
    fn tool_tips_preferences_dropdown_changes_the_mode() {
        let mut h = harness();
        crate::prefs_ui::open_preferences(h.state_mut(), "interface");
        h.run_steps(4);
        let combo = h
            .query_all_by_role(egui::accesskit::Role::ComboBox)
            .find(|n| n.value().as_deref() == Some("Rich (name, shortcut and how to use)"))
            .expect("Tool tips dropdown");
        combo.click();
        h.run_steps(3);
        h.get_by_label("Simple (name and shortcut)").click();
        h.run_steps(3);
        h.get_by_label("Apply").click();
        h.run_steps(3);
        assert_eq!(h.state().session.prefs().interface.tool_tips, ToolTips::Simple);
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
    }

    #[test]
    fn the_setting_defaults_to_rich_and_old_prefs_load() {
        let p: photocraft_engine::prefs::Preferences = serde_json::from_value(json!({"interface": {"theme": "pro"}})).unwrap();
        assert_eq!(p.interface.tool_tips, ToolTips::Rich);
        let mut s = photocraft_engine::Session::new();
        s.execute("prefs.set", json!({"values": {"interface.toolTips": "simple"}})).unwrap();
        assert_eq!(s.prefs().interface.tool_tips, ToolTips::Simple);
        assert!(s.execute("prefs.set", json!({"values": {"interface.toolTips": "loud"}})).is_err());
    }

    #[test]
    fn tips_name_the_keys_the_shortcut_handler_uses() {
        for t in Tool::ALL {
            let sp = spec(&photocraft_engine::Session::new(), t, &[t]);
            assert_eq!(sp.key, (t.key() != '\0').then(|| t.key().to_string()), "{t:?}");
        }
    }
}
