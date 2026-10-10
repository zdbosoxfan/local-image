//! Original Local Image tool artwork, GPL-3.0-or-later. No vendor paths or images.
//! One geometry source produces both styles; named material tokens change only the paint.
use std::path::Path;

use anyhow::{Context, Result, bail};

pub const TOOLS: &[(&str, &str)] = &[
    ("activity", "History"),
    ("arrow-left-right", "Content Aware Move"),
    ("background-mask", "Background Mask"),
    ("blend", "Gradient"),
    ("bloat", "Bloat"),
    ("blur", "Blur"),
    ("brush", "Brush"),
    ("circle", "Ellipse Shape"),
    ("circle-dashed", "Ellipse Marquee"),
    ("circle-dot", "Count"),
    ("color-range", "Colour Range"),
    ("crop", "Crop"),
    ("custom-shape", "Custom Shape"),
    ("depth-range", "Depth Range"),
    ("develop-edit", "Develop Edit"),
    ("direct-select", "Direct Selection"),
    ("dodge", "Dodge"),
    ("edit-toolbar", "Edit Toolbar"),
    ("eraser", "Eraser"),
    ("eraser-background", "Background Eraser"),
    ("eraser-magic", "Magic Eraser"),
    ("flame", "Burn"),
    ("forward-warp", "Forward Warp"),
    ("freeze", "Freeze Mask"),
    ("freeze-all", "Freeze All"),
    ("freeze-lasso", "Freeze Lasso"),
    ("guided-upright", "Guided Upright"),
    ("hand", "Hand"),
    ("healing", "Healing"),
    ("history-brush", "History Brush"),
    ("info", "Info"),
    ("invert-freeze", "Invert Freeze"),
    ("keywords", "Keywords"),
    ("landscape-mask", "Landscape Mask"),
    ("lasso", "Lasso"),
    ("lasso-magnetic", "Magnetic Lasso"),
    ("lasso-select", "Patch"),
    ("linear-mask", "Linear Mask"),
    ("luminance-range", "Luminance Range"),
    ("mask", "Masking"),
    ("message-square", "Note"),
    ("more", "More"),
    ("mouse-pointer-2", "Path Selection"),
    ("move", "Move"),
    ("object-mask", "Object Mask"),
    ("object-selection", "Object Selection"),
    ("paint-bucket", "Paint Bucket"),
    ("palette", "Mixer Brush"),
    ("pen-tool", "Pen"),
    ("pencil", "Pencil"),
    ("pentagon", "Polygonal Lasso"),
    ("people-mask", "People Mask"),
    ("perspective-crop", "Perspective Crop"),
    ("pipette", "Eyedropper"),
    ("presets", "Presets"),
    ("prompt-mask", "Prompt Mask"),
    ("pucker", "Pucker"),
    ("push-left", "Push Left"),
    ("quick-mask", "Quick Mask"),
    ("quick-selection", "Quick Selection"),
    ("radial-mask", "Radial Mask"),
    ("reconstruct", "Reconstruct"),
    ("reconstruct-all", "Reconstruct All"),
    ("rectangle-horizontal", "Rectangle Shape"),
    ("red-eye", "Red Eye"),
    ("remove", "Spot Removal"),
    ("ruler", "Ruler"),
    ("scissors", "AI Cutout"),
    ("shape-polygon", "Polygon Shape"),
    ("shape-triangle", "Triangle Shape"),
    ("sharpen", "Sharpen"),
    ("sky-mask", "Sky Mask"),
    ("slash", "Line Shape"),
    ("slice-knife", "Slice"),
    ("slice-select", "Slice Select"),
    ("smooth", "Smooth"),
    ("smudge", "Smudge"),
    ("sparkles", "AI Remove"),
    ("sponge", "Sponge"),
    ("spot-healing", "Spot Healing"),
    ("square-dashed", "Rectangle Marquee"),
    ("stamp", "Clone Stamp"),
    ("straighten", "Straighten"),
    ("subject-mask", "Subject Mask"),
    ("thaw", "Thaw Mask"),
    ("thaw-all", "Thaw All"),
    ("twirl-ccw", "Twirl CCW"),
    ("twirl-cw", "Twirl CW"),
    ("targeted-adjust", "Targeted Adjustment"),
    ("type", "Type"),
    ("versions", "Versions"),
    ("vertical-type", "Vertical Type"),
    ("wand-sparkles", "Magic Wand"),
    ("zoom-in", "Zoom"),
];

fn path(d: &str) -> String {
    // Closed object contours become filled faces; open ropes/marks remain hollow bands.
    // Split explicit moveto subpaths so interior marks aren't implicitly filled by SVG.
    d.split('M')
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            let d = format!("M{}", s.trim());
            if d.ends_with('Z') { fill(&d) } else { wire(&format!("<path d=\"{d}\"/>")) }
        })
        .collect()
}
fn fill(d: &str) -> String {
    solid(&format!("<path d=\"{d}\" fill-rule=\"evenodd\"/>"))
}
fn solid(shape: &str) -> String {
    format!(
        "<g fill=\"@a@\" stroke=\"@edge@\" stroke-width=\"1.43\" paint-order=\"stroke fill\">{shape}</g>\n<g fill=\"none\" stroke=\"@shine@\" stroke-width=\".7\" transform=\"translate(.2 .2)\">{shape}</g>\n"
    )
}
fn wire(shape: &str) -> String {
    // Three SVG strokes make an outlined, gradient-filled band, never a thin colour line.
    band(shape, 3.5, 2.2, 0.5)
}
fn fine_wire(shape: &str) -> String {
    band(shape, 2.0, 0.9, 0.3)
}
fn band(shape: &str, width: f32, face: f32, highlight: f32) -> String {
    format!(
        "<g fill=\"none\" stroke=\"@edge@\" stroke-width=\"{width}\">{shape}</g>\n<g fill=\"none\" stroke=\"@wire@\" stroke-width=\"{face}\">{shape}</g>\n<g fill=\"none\" stroke=\"@shine@\" stroke-width=\"{highlight}\">{shape}</g>\n"
    )
}
fn detail(d: &str) -> String {
    format!("<path d=\"{d}\" fill=\"none\" stroke=\"@edge@\" stroke-width=\"1.1\"/>\n")
}
fn cutout(id: &str, drawing: String, holes: &str) -> String {
    // SVG luminance masks preserve negative details in both material styles.
    format!(
        "<defs><mask id=\"{id}\" maskUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"24\" height=\"24\"><rect width=\"24\" height=\"24\" fill=\"white\" stroke=\"none\"/>{holes}</mask></defs>\n<g mask=\"url(#{id})\">{drawing}</g>\n"
    )
}
fn slit(d: &str) -> String {
    format!("<path d=\"{d}\" fill=\"none\" stroke=\"black\" stroke-width=\"1.15\" stroke-linecap=\"round\"/>\n")
}
fn circle(x: f32, y: f32, r: f32) -> String {
    wire(&format!("<circle cx=\"{x}\" cy=\"{y}\" r=\"{r}\"/>"))
}
fn solid_circle(x: f32, y: f32, r: f32) -> String {
    solid(&format!("<circle cx=\"{x}\" cy=\"{y}\" r=\"{r}\"/>"))
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> String {
    wire(&format!("<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\".6\"/>"))
}
fn dash(svg: String) -> String {
    format!("<g stroke-dasharray=\"2.2 2.5\" stroke-linecap=\"butt\">{svg}</g>\n")
}
fn neutral(svg: String) -> String {
    svg.replace("@a@", "@n@").replace("@wire@", "@metal_wire@")
}
fn small(svg: String) -> String {
    format!("<g transform=\"translate(0 4) scale(.72)\">{svg}</g>\n")
}
fn sparkle() -> String {
    fill("M18 1L19.3 4L22 5L19.3 6L18 9L16.7 6L14 5L16.7 4Z") + &fill("M21 9L22 10L21 11L20 10Z")
}
fn pointer() -> String {
    fill("M3 2L3 19L7.5 14.5L11 21L14 19.5L10.5 13L17 13Z")
}
fn badge_pointer() -> String {
    fill("M15 13V22L18 19L21 19Z")
}
fn brush() -> String {
    neutral(fill("M11 13L18 3Q19 1.5 20.5 3Q22 4 20 6L14 15Z")) + &fill("M10 12Q15 13 14 16Q12.5 21 3 21Q6.5 18.5 7 15.5Q7.5 12.5 10 12Z")
}
fn eraser() -> String {
    fill("M3 15L12 4Q13 3 14 4L21 10L12 21H8Z") + &neutral(fill("M3 15L8 9L16 16L12 21H8Z")) + &detail("M8 9L16 16") + &path("M12 21H21")
}
fn lasso() -> String {
    path("M5 16C-1 12 3 4 12 4C23 4 23 14 14 16C8 18 3 17 4 14C5 11 8 14 7 18L5 22")
}
fn bandage() -> String {
    fill("M3 14L14 3Q16 1 18 3L21 6Q23 8 21 10L10 21Q8 23 6 21L3 18Q1 16 3 14Z M8 12L12 8L16 12L12 16Z")
}
fn nib() -> String {
    cutout(
        "nib-slits",
        fill("M12 21L4 10L8 3H16L20 10Z") + &neutral(fill("M8 3H16L17.5 6H6.5Z")),
        &format!("<circle cx=\"12\" cy=\"10\" r=\"1.6\" fill=\"black\" stroke=\"none\"/>{}", slit("M12 11V21")),
    )
}
fn lock(open: bool) -> String {
    let shackle = if open { "M8 10V6C8 0 18 0 18 6" } else { "M7 10V7C7 0 17 0 17 7V10" };
    neutral(path(shackle)) + &fill("M5 10H19V21H5Z M11 12H13V15H11Z") + &detail("M6 17H18")
}
fn bust() -> String {
    solid_circle(12.0, 7.0, 3.2) + &path("M5 21V19C5 12 19 12 19 19V21Z")
}
fn crop() -> String {
    path("M6 2V18H22") + &path("M2 6H18V22")
}
fn knife() -> String {
    path("M10 13L18 3L21 6L14 16Z M10 13L3 21L9 19L14 16")
}
fn dropper() -> String {
    path("M4 20L5 15L14 6L18 10L9 19Z M12 5L19 12") + &fill("M14 5L17 2Q19 0 21 3Q23 5 20 7L18 9Z")
}
fn spiral(clockwise: bool) -> String {
    let svg = path("M20 8C18 1 7 1 4 8C0 17 9 24 17 19C23 15 18 7 12 8C7 9 8 16 12 16C16 16 15 12 13 12 M20 3V8H15");
    if clockwise { svg } else { format!("<g transform=\"translate(24 0) scale(-1 1)\">{svg}</g>") }
}
fn body(name: &str) -> String {
    match name {
        "move" => path("M13 2V13 M10 5L13 2L16 5 M6 9H22 M9 6L6 9L9 12 M19 6L22 9L19 12 M13 12V21 M10 18L13 21L16 18") + &small(pointer()),
        "square-dashed" => dash(rect(3.0, 4.0, 18.0, 16.0)),
        "circle-dashed" => dash(wire("<ellipse cx=\"12\" cy=\"12\" rx=\"9\" ry=\"8\"/>")),
        "lasso" => lasso(),
        "pentagon" => path("M5 17L3 8L12 3L21 9L17 18L5 17 M5 17L7 21L10 19"),
        "lasso-magnetic" => small(lasso()) + &path("M16 12V17C16 21 22 21 22 17V12 M16 15H18 M20 15H22"),
        "quick-selection" => dash(circle(7.5, 16.5, 5.5)) + &small(brush()),
        "object-selection" => dash(path("M21 10V3H3V21H11")) + &badge_pointer(),
        "wand-sparkles" => fill("M2 20L15 7L17 9L4 22Z") + &sparkle(),
        "crop" => crop(),
        "perspective-crop" => crop() + &neutral(fine_wire("<path d=\"M9 9L15 8L15 15L9 16Z\"/>")) + &detail("M12 9V15 M9 12L15 11"),
        "straighten" => format!("<g transform=\"rotate(-20 12 12)\">{}{}</g>", rect(3.0, 8.0, 18.0, 8.0), circle(12.0, 12.0, 2.0)) + &path("M4 21H20"),
        "pipette" => dropper(),
        "color-range" => small(dropper()) + &circle(18.0, 17.0, 3.0) + &path("M18 12V14 M18 20V22 M13 17H15 M21 17H23"),
        "healing" => bandage(),
        "spot-healing" => dash(circle(7.0, 17.0, 5.0)) + &small(bandage()),
        "lasso-select" => dash(wire("<path d=\"M3 8L13 3L18 13L8 20Z\"/>")) + &detail("M8 7L14 15 M6 12L12 9") + &path("M12 18H22 M19 15L22 18L19 21"),
        "arrow-left-right" => {
            neutral(dash(rect(2.0, 2.0, 20.0, 20.0)))
                + &fill("M5 4H12L9.7 6.3L17.7 14.3L20 12V19H13L15.3 16.7L7.3 8.7L5 11Z")
                + &fill("M20 4V11L17.7 8.7L9.7 16.7L12 19H5V12L7.3 14.3L15.3 6.3L13 4Z")
        }
        "sparkles" => small(brush()) + &sparkle(),
        "remove" => small(bandage()) + &path("M16 17H22"),
        "red-eye" => {
            wire("<path d=\"M2 12C7 3 17 3 22 12C17 21 7 21 2 12Z\"/>") + &fine_wire("<circle cx=\"12\" cy=\"12\" r=\"3.5\"/>") + &detail("M12 6V18 M6 12H18")
        }
        "stamp" => path("M9 14V10C4 3 8 2 12 2C16 2 20 3 15 10V14 M5 15H19L21 21H3Z M3 18H21"),
        "history-brush" => small(brush()) + &path("M16 13C24 13 24 3 18 3H14 M16 1L14 3L16 5"),
        "brush" => brush(),
        "pencil" => path("M3 21L5 15L17 3L21 7L9 19Z M5 15L9 19 M15 5L19 9"),
        "palette" => {
            small(brush())
                + &path("M17 10C17 10 14 14 14 16C14 20 20 20 20 16C20 14 17 10 17 10Z")
                + &neutral(path("M21 7C21 7 19 10 19 11C19 14 23 14 23 11C23 10 21 7 21 7Z"))
        }
        "eraser" => eraser(),
        "eraser-background" => small(eraser()) + &neutral(fill("M16 3H19V6.5H16Z M19 6.5H22V10H19Z")),
        "eraser-magic" => small(eraser()) + &sparkle(),
        "blend" => String::from(
            "<defs><linearGradient id=\"gradient\"><stop stop-color=\"@top@\"/><stop offset=\"1\" stop-color=\"@bottom@\" stop-opacity=\".08\"/></linearGradient></defs>\n<rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\".6\" fill=\"url(#gradient)\"/>\n<path d=\"M4 9V4H11\" fill=\"none\" stroke=\"@highlight@\" stroke-opacity=\".28\" stroke-width=\"1.1\"/>\n",
        ),
        "paint-bucket" => path("M3 11L10 4L19 13L12 20Z M6 8L3 5Q3 1 7 3L16 12 M3 11H17 M20 14Q14 22 20 22Q25 22 20 14Z"),
        "blur" => path("M12 2C10 6 4 11 4 15C4 25 20 25 20 15C20 11 14 6 12 2Z"),
        "sharpen" => path("M12 2L22 21H2Z M12 2V16L22 21 M12 16L2 21"),
        "smudge" => {
            cutout(
                "smudge-fingers",
                fill("M6 21L3 15Q1 11 4 11L7 15V6Q7 2 10 3V12Q13 8 14 11Q17 9 18 12Q21 11 21 15L18 21Z"),
                &slit("M10 10V13 M14 11V14 M18 12V15"),
            ) + &path("M2 22H15")
        }
        "dodge" => neutral(fill("M10.5 11.5L12.5 10.5L21 20L19.5 21Z")) + &solid_circle(8.0, 8.0, 5.5),
        "flame" => cutout(
            "burn-fingers",
            fill("M4 21L2 14Q2 11 5 12L8 16L5 12L7 6Q9 2 11 5Q14 3 15 6Q18 5 19 8Q22 8 22 11L19 17Q15 21 10 21Z"),
            &slit("M11 5L9 10 M15 6L13 10 M19 8L17 12"),
        ),
        "sponge" => fill(
            "M4 5Q7 3 9 5Q12 3 15 5Q18 3 21 6L20 20Q16 22 13 20Q9 22 6 20Q3 21 3 18Z M6.2 9a1.8 1.8 0 1 0 3.6 0a1.8 1.8 0 1 0-3.6 0Z M14 10a2 2 0 1 0 4 0a2 2 0 1 0-4 0Z M7 16a2 2 0 1 0 4 0a2 2 0 1 0-4 0Z M14.3 17a1.7 1.7 0 1 0 3.4 0a1.7 1.7 0 1 0-3.4 0Z",
        ),
        "pen-tool" => nib(),
        "type" => fill("M3 3H21V8H19L18 5H14V19L17 20V22H7V20L10 19V5H6L5 8H3Z"),
        "vertical-type" => format!("<g transform=\"scale(.75 1)\">{}</g>", body("type")) + &path("M21 3V21 M18 18L21 21L23 18"),
        "mouse-pointer-2" => pointer(),
        "direct-select" => wire("<path d=\"M3 2L3 19L7.5 14.5L11 21L14 19.5L10.5 13L17 13Z\"/>"),
        "rectangle-horizontal" => fill("M3 5H21V19H3Z"),
        "circle" => solid("<ellipse cx=\"12\" cy=\"12\" rx=\"10\" ry=\"8\"/>"),
        "shape-triangle" => fill("M12 3L22 21H2Z"),
        "shape-polygon" => fill("M12 2L22 9L18 21H6L2 9Z"),
        "slash" => wire("<path d=\"M3 21L21 3\"/>"),
        "custom-shape" => fill("M12 2L15 8L22 9L17 14L18 22L12 18L6 22L7 14L2 9L9 8Z"),
        "hand" => cutout(
            "hand-fingers",
            fill("M7 21L3 14Q1 10 4 10L7 13V5Q7 2 10 4V11V3Q10 1 13 2V11V4Q13 2 16 4V12V7Q16 4 19 6V15Q19 19 16 21Z"),
            &slit("M10 4V11 M13 3V11 M16 5V12"),
        ),
        "zoom-in" => circle(10.0, 10.0, 7.0) + &path("M15 15L22 22 M6 10H14 M10 6V14"),
        "ruler" => cutout("ruler-ticks", fill("M2 15L16 2L22 8L8 22Z"), &slit("M6 12L9 15 M9 9L11 11 M12 6L15 9 M15 3L17 5")),
        "circle-dot" => circle(12.0, 10.0, 8.0) + &path("M8 19L12 22L16 19 M10 7L12 5V14 M9 14H15"),
        "message-square" => cutout("note-fold", fill("M4 3H21V15L15 21H4Z"), &slit("M15 21V15H21 M7 7H17 M7 11H14")),
        "slice-knife" => knife(),
        "slice-select" => small(knife()) + &badge_pointer(),
        "forward-warp" => small(body("smudge")) + &path("M15 8H22 M19 5L22 8L19 11"),
        "reconstruct" => small(brush()) + &path("M15 13C23 13 24 3 18 3H14 M16 1L14 3L16 5"),
        "reconstruct-all" => small(body("reconstruct")) + &rect(16.0, 15.0, 6.0, 7.0),
        "smooth" => path("M2 8C7 0 8 16 13 8S19 4 22 8 M2 16C7 8 8 24 13 16S19 12 22 16"),
        "twirl-cw" => spiral(true),
        "twirl-ccw" => spiral(false),
        "pucker" => path("M2 2L9 9 M5 9H9V5 M22 2L15 9 M15 5V9H19 M2 22L9 15 M5 15H9V19 M22 22L15 15 M15 19V15H19"),
        "bloat" => path("M9 9L2 2 M2 6V2H6 M15 9L22 2 M18 2H22V6 M9 15L2 22 M2 18V22H6 M15 15L22 22 M18 22H22V18"),
        "push-left" => path("M22 12H3 M8 7L3 12L8 17 M15 4C9 7 20 17 14 21 M20 4C14 7 25 17 19 21"),
        "freeze" => lock(false),
        "thaw" => lock(true),
        "freeze-all" => small(lock(false)) + &rect(16.0, 15.0, 6.0, 7.0),
        "thaw-all" => small(lock(true)) + &rect(16.0, 15.0, 6.0, 7.0),
        "freeze-lasso" => small(lasso()) + &format!("<g transform=\"translate(12 9) scale(.55)\">{}</g>", lock(false)),
        "invert-freeze" => circle(12.0, 12.0, 9.0) + &fill("M12 3A9 9 0 0 0 12 21Z") + &path("M12 3V21"),
        "linear-mask" => body("blend") + &path("M1 12H23"),
        "radial-mask" => wire("<ellipse cx=\"12\" cy=\"12\" rx=\"5.5\" ry=\"4\"/>") + &dash(wire("<ellipse cx=\"12\" cy=\"12\" rx=\"10\" ry=\"8\"/>")),
        "subject-mask" => bust() + &dash(path("M3 9V2H21V9 M3 16V22 M21 16V22")),
        "sky-mask" => path("M5 16C-1 16 0 8 6 9C7 1 18 2 18 9C25 8 25 16 19 16Z M3 21H21"),
        "background-mask" => path("M3 15V3H21V15 M3 13L6 9L8 11 M16 10L18 7L21 12") + &bust(),
        "object-mask" => dash(rect(2.0, 2.0, 20.0, 20.0)) + &cutout("object-faces", fill("M7 9L12 6L17 9V16L12 19L7 16Z"), &slit("M7 9L12 12L17 9 M12 12V19")),
        "people-mask" => {
            circle(8.0, 6.0, 3.0) + &path("M2 21V18C2 12 14 12 14 18V21Z") + &neutral(circle(18.0, 7.0, 2.5) + &path("M16 14C20 12 22 15 22 18V21H17"))
        }
        "luminance-range" => circle(12.0, 12.0, 5.0) + &path("M12 2V4 M12 20V22 M2 12H4 M20 12H22 M5 5L6.5 6.5 M17.5 17.5L19 19 M5 19L6.5 17.5 M17.5 6.5L19 5"),
        "depth-range" => path("M3 7L12 2L21 7L12 12Z M3 12L12 17L21 12 M3 17L12 22L21 17"),
        "landscape-mask" => path("M2 20L8 8L12 14L16 9L22 20Z") + &circle(17.0, 4.5, 2.0),
        "prompt-mask" => dash(rect(2.0, 3.0, 20.0, 18.0)) + &path("M6 7H18 M6 12H16 M6 17H12"),
        "mask" => dash(circle(12.0, 12.0, 9.0)) + &circle(12.0, 12.0, 5.0),
        "quick-mask" => rect(3.0, 3.0, 18.0, 18.0) + &circle(12.0, 12.0, 5.5) + &fill("M12 6.5A5.5 5.5 0 0 0 12 17.5Z"),
        "develop-edit" => {
            path("M3 5H8 M12 5H21 M3 12H14 M18 12H21 M3 19H5 M9 19H21") + &circle(10.0, 5.0, 2.0) + &circle(16.0, 12.0, 2.0) + &circle(7.0, 19.0, 2.0)
        }
        "presets" => circle(9.0, 12.0, 7.0) + &circle(15.0, 12.0, 7.0),
        "versions" => rect(7.0, 3.0, 14.0, 14.0) + &path("M3 7V21H17"),
        "targeted-adjust" => circle(8.0, 12.0, 5.0) + &circle(8.0, 12.0, 1.0) + &path("M19 3V21 M16 6L19 3L22 6 M16 18L19 21L22 18"),
        "info" => circle(12.0, 12.0, 9.0) + &path("M12 11V17 M10 17H14") + &circle(12.0, 7.0, 0.5),
        "keywords" => cutout("tag-hole", fill("M3 3H12L22 13L13 22L3 12Z"), "<circle cx=\"7\" cy=\"7\" r=\"1.4\" fill=\"black\" stroke=\"none\"/>"),
        "activity" => circle(12.0, 12.0, 8.0) + &path("M12 7V12L16 15 M2 2V8H8"),
        "more" => circle(4.0, 12.0, 1.8) + &circle(12.0, 12.0, 1.8) + &circle(20.0, 12.0, 1.8),
        "edit-toolbar" => circle(4.0, 12.0, 1.0) + &circle(10.0, 12.0, 1.0) + &path("M13 18L19 12L22 15L16 21H13Z"),
        "guided-upright" => path("M6 2L3 22 M18 2L21 22 M3 7H21 M2 17H22 M9 3V21 M15 3V21"),
        "scissors" => circle(5.0, 18.0, 3.0) + &circle(19.0, 18.0, 3.0) + &path("M7 16L21 2 M17 16L3 2"),
        _ => panic!("missing original artwork: {name}"),
    }
}

fn palette(family: &str) -> (&'static str, &'static str) {
    match family {
        "blue" => ("#61B5FF", "#3474B5"),
        "yellow" => ("#EEC466", "#A77932"),
        "grey" => ("#CCD3DF", "#66717F"),
        "measure" => ("#99B8D6", "#526D8C"),
        "liquify" => ("#EF91BD", "#A24779"),
        "green" => ("#64D6A0", "#2A8861"),
        "cyan" => ("#55D6D0", "#287E83"),
        "purple" => ("#B293FF", "#7257B8"),
        "orange" => ("#FF9C55", "#B95635"),
        _ => panic!("unknown family: {family}"),
    }
}

fn svg(name: &str, family: &str, mono: bool) -> String {
    let (top, bottom) = if mono { ("currentColor", "currentColor") } else { palette(family) };
    let (metal_top, metal_bottom) = if mono { ("currentColor", "currentColor") } else { palette("grey") };
    let edge = if mono { "currentColor" } else { "#1b1c20" };
    let highlight = if mono { "currentColor" } else { "#F7F9FC" };
    let drawing = body(name)
        .replace("@a@", "url(#a)")
        .replace("@n@", "url(#n)")
        .replace("@wire@", "url(#aw)")
        .replace("@metal_wire@", "url(#nw)")
        .replace("@edge@", edge)
        .replace("@shine@", "url(#shine)")
        .replace("@top@", top)
        .replace("@bottom@", bottom)
        .replace("@highlight@", highlight);
    // userSpaceOnUse also shades perfectly horizontal/vertical marks (zero-area boxes).
    let defs = format!(
        "<defs>\n<linearGradient id=\"a\" x1=\"0\" y1=\"0\" x2=\".25\" y2=\"1\"><stop stop-color=\"{top}\"/><stop offset=\"1\" stop-color=\"{bottom}\"/></linearGradient>\n<linearGradient id=\"n\" x1=\"0\" y1=\"0\" x2=\".25\" y2=\"1\"><stop stop-color=\"{metal_top}\"/><stop offset=\"1\" stop-color=\"{metal_bottom}\"/></linearGradient>\n<linearGradient id=\"aw\" gradientUnits=\"userSpaceOnUse\" x1=\"0\" y1=\"0\" x2=\"6\" y2=\"24\"><stop stop-color=\"{top}\"/><stop offset=\"1\" stop-color=\"{bottom}\"/></linearGradient>\n<linearGradient id=\"nw\" gradientUnits=\"userSpaceOnUse\" x1=\"0\" y1=\"0\" x2=\"6\" y2=\"24\"><stop stop-color=\"{metal_top}\"/><stop offset=\"1\" stop-color=\"{metal_bottom}\"/></linearGradient>\n<linearGradient id=\"shine\" gradientUnits=\"userSpaceOnUse\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"20\"><stop stop-color=\"{highlight}\" stop-opacity=\".28\"/><stop offset=\"1\" stop-color=\"{highlight}\" stop-opacity=\"0\"/></linearGradient>\n</defs>\n"
    );
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\">\n<!-- Original Local Image tool artwork; GPL-3.0-or-later. Glossy Duotone, {family} family. -->\n{defs}<g transform=\"translate(1.5 1.5) scale(.875)\" fill=\"none\" stroke=\"{edge}\" stroke-width=\"1.43\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n{drawing}</g>\n</svg>\n"
    )
}

pub fn run(root: &Path) -> Result<()> {
    // This immutable snapshot also supplies the established family assignment, never geometry.
    let before: std::collections::BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(root.join("docs/design/tool-icons-before.json"))?)?;
    if before.len() != TOOLS.len() {
        bail!("baseline/artwork coverage differs");
    }
    std::fs::create_dir_all(root.join("assets/icons/tools"))?;
    let mut embedded = String::from(
        "//! Original Local Image monochrome tools; generated by `cargo xtask redraw-tool-icons`.\n\npub static TOOL_ICONS: &[(&str, &[u8])] = &[\n",
    );
    let mut labels = String::from("\npub static TOOL_LABELS: &[(&str, &str)] = &[\n");
    for &(name, label) in TOOLS {
        let old = before.get(name).with_context(|| format!("no baseline for {name}"))?;
        let family = old.split("Glossy Duotone, ").nth(1).and_then(|s| s.split(" family").next()).context("baseline lacks family")?;
        for (dir, mono) in [("color", false), ("tools", true)] {
            std::fs::write(root.join(format!("assets/icons/{dir}/{name}.svg")), svg(name, family, mono))?;
        }
        embedded.push_str(&format!("    (\"{name}\", include_bytes!(\"../../../assets/icons/tools/{name}.svg\")),\n"));
        labels.push_str(&format!("    (\"{name}\", \"{label}\"),\n"));
    }
    embedded.push_str("];\n");
    labels.push_str("];\n");
    // Labels are used only in Compositing's test-only contact-sheet renderer.
    embedded.push_str("\n#[cfg(test)]\n#[allow(dead_code)]\n");
    embedded.push_str(labels.trim_start());
    std::fs::write(root.join("crates/pc-ui-egui/src/tool_icon_data.rs"), embedded)?;
    println!("{} original tools redrawn in both styles", TOOLS.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_tool_artwork_is_fresh_and_covers_the_baseline() {
        let root = crate::root();
        let before: std::collections::BTreeMap<String, String> =
            serde_json::from_str(&std::fs::read_to_string(root.join("docs/design/tool-icons-before.json")).unwrap()).unwrap();
        let mut names = std::collections::HashSet::new();
        for &(name, _) in TOOLS {
            assert!(names.insert(name), "duplicate {name}");
            let old = before.get(name).unwrap();
            let family = old.split("Glossy Duotone, ").nth(1).unwrap().split(" family").next().unwrap();
            for (dir, mono) in [("color", false), ("tools", true)] {
                let file = root.join(format!("assets/icons/{dir}/{name}.svg"));
                assert_eq!(std::fs::read_to_string(&file).unwrap(), svg(name, family, mono), "stale {}: run cargo xtask redraw-tool-icons", file.display());
            }
        }
        assert_eq!(names.len(), before.len());
    }
}
