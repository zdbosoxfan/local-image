//! local-image: Perspective Crop (Photoshop's tool, C group): four corners drawn on the image
//! become the edges of the new canvas, so a document, sign or façade photographed at an angle
//! comes out square. Every layer is resampled through the same homography (the Background stays a
//! Background); masks, channels and the selection follow.

use photocraft_algo::transform::{Homography, Interp, quad_free_size};
use photocraft_doc::{LayerContent, Size};
use photocraft_geom::Rect;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

const CMD: &str = "image.perspectiveCrop";

fn bad(msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: CMD.into(), msg: msg.into() }
}

/// `quad`: four `[x, y]` corners, clockwise from top-left.
fn quad(p: &Value) -> Result<[[f64; 2]; 4]> {
    let arr = p.get("quad").and_then(Value::as_array).ok_or_else(|| bad("missing `quad` (four [x, y] corners, clockwise from top-left)"))?;
    if arr.len() != 4 {
        return Err(bad("`quad` needs exactly four corners"));
    }
    let mut q = [[0.0; 2]; 4];
    for (i, c) in arr.iter().enumerate() {
        let (Some(x), Some(y)) = (c.get(0).and_then(Value::as_f64), c.get(1).and_then(Value::as_f64)) else {
            return Err(bad("each corner is [x, y]"));
        };
        if !(x.is_finite() && y.is_finite() && x.abs() < 1e6 && y.abs() < 1e6) {
            return Err(bad("corners must be finite"));
        }
        q[i] = [x, y];
    }
    // Convex and clockwise (on screen): every turn has the same sign.
    let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
    let turns: Vec<f64> = (0..4).map(|i| cross(q[i], q[(i + 1) % 4], q[(i + 2) % 4])).collect();
    if !(turns.iter().all(|t| *t > 1e-6) || turns.iter().all(|t| *t < -1e-6)) {
        return Err(bad("the corners must form a convex shape"));
    }
    Ok(q)
}

/// The output size when none is given: the rectangle the quad shows in perspective to a camera
/// aimed at the centre of a `doc`-sized image ([`quad_free_size`]).
pub fn output_size(q: &[[f64; 2]; 4], doc: Size) -> (u32, u32) {
    let (w, h) = quad_free_size(q, [f64::from(doc.width) / 2.0, f64::from(doc.height) / 2.0]);
    ((w.round() as u32).max(1), (h.round() as u32).max(1))
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let q = quad(p)?;
    let (aw, ah) = output_size(&q, s.active().ok_or(EngineError::NoDocument)?.doc.size);
    let int = |k: &str| p.get(k).and_then(Value::as_u64).filter(|v| (1..=300_000).contains(v)).map(|v| v as u32);
    let (w, h) = (int("width").unwrap_or(aw), int("height").unwrap_or(ah));
    let interp = Interp::parse(p.get("interpolation").and_then(Value::as_str).unwrap_or("bicubic"));
    // Output rectangle → quad, inverted: the image's quad onto the new canvas.
    let to_quad = Homography::rect_to_quad([0.0, 0.0, f64::from(w), f64::from(h)], q).ok_or_else(|| bad("the corners don't make a usable shape"))?;
    let hm = to_quad.inverse().ok_or_else(|| bad("the corners don't make a usable shape"))?;
    let bg = s.tools.background;
    s.edit("Perspective Crop", |doc, _| {
        doc.size = Size::new(w, h);
        let canvas = Rect::new(0, 0, w as i32, h as i32);
        let fmt = doc.pixel_format();
        for l in doc.layers.iter_mut() {
            let background = l.name == "Background" && l.locks.position && matches!(l.content, LayerContent::Raster(_));
            if background && let Some(surf) = l.surface_mut() {
                let src = surf.content_bounds();
                let warped = photocraft_algo::transform::warp_surface_clipped(surf, src, &hm, interp, canvas);
                let mut base = photocraft_raster::Surface::new(fmt);
                let fill = photocraft_raster::from_rgba(&fmt, bg);
                base.write_region(canvas, &fill.repeat(canvas.width() as usize * canvas.height() as usize));
                crate::transform_cmds::composite_over(&mut base, &warped)?;
                base.prune();
                *surf = base;
                if let Some(m) = l.mask.as_mut() {
                    m.surface = crate::transform_cmds::warp_gray(&m.surface, &hm, interp);
                }
            } else {
                let locks = l.locks;
                l.locks.position = false;
                l.locks.all = false;
                crate::transform_cmds::transform_layer(None, photocraft_doc::Locks::default(), l, &hm, None, interp)?;
                l.locks = locks;
            }
        }
        for ch in doc.channels.iter_mut().chain(doc.quick_mask.as_mut()) {
            ch.surface = crate::transform_cmds::warp_gray(&ch.surface, &hm, interp);
        }
        doc.selection = None;
        crate::canvas_geom::refresh(doc, crate::canvas_geom::Refresh::All)?;
        doc.guides = Default::default();
        crate::image_cmds::crop_to(doc, canvas)?;
        Ok(())
    })?;
    Ok(json!({ "width": w, "height": h }))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: CMD,
        label: "Perspective Crop",
        menu: &[],
        shortcut: None,
        params: r##"{"quad":[[x,y]×4] (clockwise from top-left),"width":px?,"height":px?,"interpolation":"bicubic|bilinear|nearest"="bicubic"} → {width,height}"##,
        enabled: |s| s.active().map(|_| ()).ok_or_else(|| "no document open".into()),
        run,
        journal: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tilted_square_comes_out_square() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 200, "height": 160, "fill": "white"})).unwrap();
        // A red quadrilateral drawn in perspective.
        let q = [[40.0, 30.0], [160.0, 50.0], [150.0, 140.0], [50.0, 120.0]];
        s.execute("select.polygon", json!({"points": q})).ok();
        let r = s.execute("image.perspectiveCrop", json!({"quad": q})).unwrap();
        let (w, h) = (r["width"].as_u64().unwrap(), r["height"].as_u64().unwrap());
        assert_eq!((w, h), {
            let (a, b) = output_size(&q, Size::new(200, 160));
            (a as u64, b as u64)
        });
        let d = &s.active().unwrap().doc;
        assert_eq!((d.size.width as u64, d.size.height as u64), (w, h));
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc.size.width, 200);
        // Bad shapes are refused.
        assert!(s.execute("image.perspectiveCrop", json!({"quad": [[0, 0], [10, 10], [0, 10], [10, 0]]})).is_err());
        assert!(s.execute("image.perspectiveCrop", json!({"quad": [[0, 0], [10, 0]]})).is_err());
    }

    /// With no size given the canvas is the rectangle the quad shows in perspective, larger than
    /// this skewed quad's own edges (661 x 375 as their means).
    #[test]
    fn a_free_crop_takes_the_perspective_size() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 800, "height": 500, "fill": "white"})).unwrap();
        let q = [[100.0, 50.0], [700.0, 80.0], [760.0, 450.0], [40.0, 420.0]];
        let r = s.execute("image.perspectiveCrop", json!({"quad": q})).unwrap();
        let (w, h) = (r["width"].as_u64().unwrap(), r["height"].as_u64().unwrap());
        assert!(w.abs_diff(732) <= 1 && h.abs_diff(588) <= 1, "{w} x {h}, want 732 x 588 within 1 px");
    }

    /// #7557: a quad low on a strongly converging photo (its horizon crosses the canvas, with the
    /// canvas centre beyond it) is cropped, not left blank in the background colour.
    #[test]
    fn a_quad_below_a_horizon_across_the_canvas_keeps_its_content() {
        use photocraft_color::{Color, ColorMode, SampleType};
        let doc = photocraft_doc::Document::with_background("red", Size::new(100, 100), ColorMode::Rgb, SampleType::U8, Color::rgb(1.0, 0.0, 0.0));
        let mut s = Session::new();
        s.add_document(doc, None);
        s.execute(CMD, json!({"quad": [[40, 70], [60, 70], [90, 95], [10, 95]], "width": 50, "height": 39})).unwrap();
        let px = s.active().unwrap().doc.layers[0].surface().unwrap().rgba(25, 20);
        assert_eq!(px, [1.0, 0.0, 0.0, 1.0]);
    }
}
