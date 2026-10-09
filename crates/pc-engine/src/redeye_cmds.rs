//! local-image: the Red Eye tool (Photoshop's, in the Healing group): a click on an eye, or a
//! drag around it, finds the red pupil and replaces its red with a dark neutral.
//!
//! The pupil is the 8-connected region of "red" pixels (red clearly above green and blue, as in
//! GEGL's `red-eye-removal` and Pinta) grown from the red pixel nearest the click, kept within a
//! search radius. Pupil Size grows or shrinks it; Darken Amount sets how dark the corrected pupil
//! gets. The edge is feathered by a pixel so the fix doesn't show a seam.

use photocraft_doc::LayerContent;
use photocraft_geom::Rect;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

const CMD: &str = "paint.redEye";

fn enabled(s: &Session) -> std::result::Result<(), String> {
    let l = crate::active_layer_of(s)?;
    if matches!(l.content, LayerContent::Raster(_)) { Ok(()) } else { Err(format!("the layer is a {} layer, not a pixel layer", l.content.kind_name())) }
}

/// How red a straight RGB pixel is (≤ 0: not red).
fn redness(p: [f32; 4]) -> f32 {
    let (r, g, b) = (p[0], p[1], p[2]);
    if r < 0.15 || p[3] < 0.5 {
        return 0.0;
    }
    (r - g.max(b)) / (r + 1e-3) - 0.25
}

/// The pupil mask over `area` (`px` covers it): the red region around `seed`, within `radius`,
/// grown (pupil > 50) or shrunk (< 50) by up to a quarter of its size. `None` when no red is found.
pub fn pupil_mask(px: &[[f32; 4]], area: Rect, seed: (i32, i32), radius: i32, pupil: f32) -> Option<Vec<f32>> {
    let (w, h) = (area.width() as usize, area.height() as usize);
    let inside = |x: i32, y: i32| area.contains(x, y) && (x - seed.0).pow(2) + (y - seed.1).pow(2) <= radius * radius;
    let at = |x: i32, y: i32| (y - area.y0) as usize * w + (x - area.x0) as usize;
    // The red pixel nearest the click (within a quarter of the radius).
    let reach = (radius / 4).max(3);
    let mut start = None;
    let mut best = i32::MAX;
    for y in seed.1 - reach..=seed.1 + reach {
        for x in seed.0 - reach..=seed.0 + reach {
            let d = (x - seed.0).pow(2) + (y - seed.1).pow(2);
            if inside(x, y) && d < best && redness(px[at(x, y)]) > 0.0 {
                best = d;
                start = Some((x, y));
            }
        }
    }
    let start = start?;
    let mut m = vec![0.0f32; w * h];
    let mut stack = vec![start];
    m[at(start.0, start.1)] = 1.0;
    let mut count = 0usize;
    let (mut sx, mut sy) = (0i64, 0i64);
    while let Some((x, y)) = stack.pop() {
        count += 1;
        sx += i64::from(x);
        sy += i64::from(y);
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let (nx, ny) = (x + dx, y + dy);
            if inside(nx, ny) && m[at(nx, ny)] == 0.0 && redness(px[at(nx, ny)]) > 0.0 {
                m[at(nx, ny)] = 1.0;
                stack.push((nx, ny));
            }
        }
    }
    // Fill holes (the catch-light) and fit: everything within the region's equivalent radius
    // (scaled by Pupil Size) of its centre that the region surrounds counts.
    let (cx, cy) = (sx as f32 / count as f32, sy as f32 / count as f32);
    let r_eq = (count as f32 / std::f32::consts::PI).sqrt();
    let r_fit = r_eq * (0.75 + 0.5 * (pupil.clamp(1.0, 100.0) / 100.0)) + 0.5;
    let mut out = vec![0.0f32; w * h];
    for y in area.y0..area.y1 {
        for x in area.x0..area.x1 {
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            let red = m[at(x, y)] > 0.0;
            // Inside the fitted circle: full where red or a hole in the region, soft over one pixel.
            let edge = (r_fit - d + 0.5).clamp(0.0, 1.0);
            let v = if red {
                edge.max(if d < r_fit + 1.0 { 0.5 } else { 0.0 })
            } else if d < r_eq * 0.8 {
                edge
            } else {
                0.0
            };
            out[at(x, y)] = v;
        }
    }
    Some(out)
}

/// Replace the red in `p` with a dark neutral (`darken` 0..1), by `k` (mask coverage).
fn correct(p: &mut [f32; 4], k: f32, darken: f32) {
    let (g, b) = (p[1], p[2]);
    let neutral = (g + b) / 2.0;
    let level = neutral * (1.0 - 0.75 * darken);
    let target = [level, level.min(g), level.min(b)];
    for c in 0..3 {
        p[c] += (target[c] - p[c]) * k;
    }
}

fn red_eye(s: &mut Session, p: &Value) -> Result<Value> {
    let bad = |m: &str| EngineError::BadParams { cmd: CMD.into(), msg: m.into() };
    let f = |k: &str, d: f64| p.get(k).and_then(Value::as_f64).unwrap_or(d);
    let pupil = f("pupilSize", 50.0) as f32;
    let darken = (f("darken", 50.0) as f32).clamp(1.0, 100.0) / 100.0;
    let d = s.active().ok_or(EngineError::NoDocument)?;
    let canvas = d.doc.bounds();
    // A drag gives the box to search; a click searches a radius around the point.
    let (seed, radius) = match p.get("rect").and_then(Value::as_array) {
        Some(a) if a.len() >= 4 => {
            let v: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
            let (x, y, w, h) = (v[0].min(v[0] + v[2]), v[1].min(v[1] + v[3]), v[2].abs(), v[3].abs());
            (((x + w / 2.0) as i32, (y + h / 2.0) as i32), (w.max(h) / 2.0).ceil().max(3.0) as i32)
        }
        _ => {
            let (x, y) = (p.get("x").and_then(Value::as_f64).ok_or_else(|| bad("needs x, y or rect"))?, f("y", 0.0));
            let r = (canvas.width().min(canvas.height()) as f64 / 12.0).clamp(12.0, 200.0);
            ((x.floor() as i32, y.floor() as i32), r as i32)
        }
    };
    let area = Rect::new(seed.0 - radius, seed.1 - radius, seed.0 + radius + 1, seed.1 + radius + 1).intersect(&canvas);
    if area.is_empty() {
        return Err(bad("the point is outside the canvas"));
    }
    let id = d.active_layer.ok_or_else(|| EngineError::Other("no active layer".into()))?;
    let surf = d.doc.layer(id).and_then(|l| l.surface()).ok_or_else(|| EngineError::Other("the active layer has no pixels".into()))?;
    let mut px = vec![[0.0f32; 4]; area.width() as usize * area.height() as usize];
    surf.read_rgba_into(area, &mut px);
    let Some(mask) = pupil_mask(&px, area, seed, radius, pupil) else {
        return Ok(json!({ "changed": false }));
    };
    let fmt = surf.format();
    s.edit("Red Eye", |doc, _| {
        let surf = doc.layer_mut(id).and_then(|l| l.surface_mut()).ok_or(EngineError::NoLayer(id))?;
        let mut data = Vec::with_capacity(px.len() * fmt.channels());
        for (q, k) in px.iter().zip(&mask) {
            let mut v = *q;
            if *k > 0.0 {
                correct(&mut v, *k, darken);
            }
            data.extend(photocraft_raster::from_rgba(&fmt, v));
        }
        surf.write_region(area, &data);
        Ok(())
    })?;
    Ok(json!({ "changed": true }))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: CMD,
        label: "Red Eye",
        menu: &[],
        shortcut: None,
        params: r##"{"x":px,"y":px | "rect":[x,y,w,h],"pupilSize":1..100=50,"darken":1..100=50} → {changed}"##,
        enabled,
        run: red_eye,
        journal: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn red_pupil_turns_dark_and_skin_stays() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 60, "height": 60})).unwrap();
        s.edit("paint", |doc, _| {
            let bg = doc.layers[0].surface_mut().unwrap();
            let fmt = bg.format();
            let mut data = Vec::new();
            for y in 0..60 {
                for x in 0..60 {
                    let d = ((x as f32 - 30.0).powi(2) + (y as f32 - 30.0).powi(2)).sqrt();
                    let c = if d < 6.0 { [0.85, 0.1, 0.12, 1.0] } else { [0.85, 0.65, 0.55, 1.0] };
                    data.extend(photocraft_raster::from_rgba(&fmt, c));
                }
            }
            bg.write_region(Rect::new(0, 0, 60, 60), &data);
            Ok(())
        })
        .unwrap();
        let r = s.execute("paint.redEye", json!({"x": 31, "y": 29})).unwrap();
        assert_eq!(r["changed"], true);
        let surf = s.active().unwrap().doc.layers[0].surface().unwrap();
        let mut px = vec![[0.0f32; 4]; 1];
        surf.read_rgba_into(Rect::new(30, 30, 31, 31), &mut px);
        assert!(px[0][0] < 0.2, "pupil {:?}", px[0]);
        surf.read_rgba_into(Rect::new(5, 5, 6, 6), &mut px);
        assert!(px[0][0] > 0.8, "skin {:?}", px[0]);
        // Nothing red near the click: no change, no history step.
        let r = s.execute("paint.redEye", json!({"x": 5, "y": 5})).unwrap();
        assert_eq!(r["changed"], false);
    }
}
