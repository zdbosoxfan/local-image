//! local-image: the Red Eye tool (Photoshop's, in the Healing group; Pet Eye as in Lightroom):
//! a click on an eye, or a drag around it, finds the pupil and darkens it.
//!
//! One algorithm with Develop: the pupil is found and fixed by [`lightcraft_pipeline::redeye`]
//! through the editor's Develop bridge ([`photocraft_io::develop_filter::red_eye_surface`]):
//! red-dominant pixels (pet eye: the brightest glow, any colour) weighted on a
//! resolution-independent grid inside the ellipse, their centroid and area giving the pupil,
//! then the red neutralized and darkened in linear light (pet eye: a dark pupil, optionally
//! with a catchlight). Pupil Size scales the found pupil; Darken Amount sets how dark it gets.

use photocraft_doc::LayerContent;
use photocraft_io::develop_filter::{self, EyeFix};
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

const CMD: &str = "paint.redEye";

fn enabled(s: &Session) -> std::result::Result<(), String> {
    let l = crate::active_layer_of(s)?;
    if matches!(l.content, LayerContent::Raster(_)) { Ok(()) } else { Err(format!("the layer is a {} layer, not a pixel layer", l.content.kind_name())) }
}

fn red_eye(s: &mut Session, p: &Value) -> Result<Value> {
    let bad = |m: &str| EngineError::BadParams { cmd: CMD.into(), msg: m.into() };
    let f = |k: &str, d: f64| p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(d);
    let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
    let d = s.active().ok_or(EngineError::NoDocument)?;
    let canvas = d.doc.bounds();
    // A drag gives the ellipse to search; a click searches a circle around the point.
    let (center, radii) = match p.get("rect").and_then(Value::as_array) {
        Some(a) if a.len() >= 4 => {
            let v: Vec<f64> = a.iter().filter_map(Value::as_f64).filter(|v| v.is_finite()).collect();
            if v.len() < 4 {
                return Err(bad("`rect` is [x, y, w, h]"));
            }
            let (x, y, w, h) = (v[0].min(v[0] + v[2]), v[1].min(v[1] + v[3]), v[2].abs(), v[3].abs());
            ([x + w / 2.0, y + h / 2.0], [(w / 2.0).max(3.0), (h / 2.0).max(3.0)])
        }
        _ => {
            let x = p.get("x").and_then(Value::as_f64).filter(|v| v.is_finite()).ok_or_else(|| bad("needs x, y or rect"))?;
            let r = (canvas.width().min(canvas.height()) as f64 / 12.0).clamp(12.0, 200.0);
            ([x.floor() + 0.5, f("y", 0.0).floor() + 0.5], [r, r])
        }
    };
    if !canvas.contains(center[0].floor() as i32, center[1].floor() as i32) {
        return Err(bad("the point is outside the canvas"));
    }
    let eye = EyeFix {
        center,
        radii,
        pupil_size: f("pupilSize", 50.0).clamp(1.0, 100.0),
        darken: f("darken", 50.0).clamp(1.0, 100.0),
        pet: flag("pet"),
        catchlight: flag("catchlight"),
    };
    let id = d.active_layer.ok_or_else(|| EngineError::Other("no active layer".into()))?;
    let surf = d.doc.layer(id).and_then(|l| l.surface()).ok_or_else(|| EngineError::Other("the active layer has no pixels".into()))?;
    // RGB / Gray documents through their profile; other modes through their RGB view (sRGB).
    let profile = crate::color_cmds::composite_profile(&d.doc);
    let Some(fixed) = develop_filter::red_eye_surface(surf, canvas, &eye, &profile).map_err(EngineError::Other)? else {
        return Ok(json!({ "changed": false }));
    };
    s.edit(if eye.pet { "Pet Eye" } else { "Red Eye" }, |doc, _| {
        *doc.layer_mut(id).and_then(|l| l.surface_mut()).ok_or(EngineError::NoLayer(id))? = fixed;
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
        params: r##"{"x":px,"y":px | "rect":[x,y,w,h],"pupilSize":1..100=50,"darken":1..100=50,"pet":bool=false,"catchlight":bool=false} → {changed}"##,
        enabled,
        run: red_eye,
        journal: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_geom::Rect;

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

    #[test]
    fn pet_eye_command_darkens_a_glow_in_one_step() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 80, "height": 80, "depth": 16})).unwrap();
        s.edit("paint", |doc, _| {
            let bg = doc.layers[0].surface_mut().unwrap();
            let fmt = bg.format();
            let mut data = Vec::new();
            for y in 0..80 {
                for x in 0..80 {
                    let d = ((x as f32 - 40.0).powi(2) + (y as f32 - 40.0).powi(2)).sqrt();
                    let c = if d < 7.0 {
                        [0.62, 0.8, 0.35, 1.0]
                    } else if d < 14.0 {
                        [0.22, 0.17, 0.1, 1.0]
                    } else {
                        [0.85, 0.65, 0.55, 1.0]
                    };
                    data.extend(photocraft_raster::from_rgba(&fmt, c));
                }
            }
            bg.write_region(Rect::new(0, 0, 80, 80), &data);
            Ok(())
        })
        .unwrap();
        let at = |s: &Session| s.active().unwrap().doc.layers[0].surface().unwrap().rgba(40, 40);
        let before = at(&s);
        let skin = s.active().unwrap().doc.layers[0].surface().unwrap().rgba(3, 3);
        // Not red: the Red Eye mode finds nothing; Pet Eye (with a catchlight) does.
        assert_eq!(s.execute("paint.redEye", json!({"x": 41, "y": 39})).unwrap()["changed"], false);
        let r = s.execute("paint.redEye", json!({"rect": [28, 28, 24, 24], "pet": true, "catchlight": true, "darken": 80})).unwrap();
        assert_eq!(r["changed"], true);
        let after = at(&s);
        assert!(after[1] < 0.25 && after[1] < before[1] * 0.4, "{before:?} → {after:?}");
        assert_eq!(s.active().unwrap().doc.layers[0].surface().unwrap().rgba(3, 3), skin);
        s.undo();
        assert_eq!(at(&s), before);
        assert!(s.execute("paint.redEye", json!({"rect": [1, 2, 3]})).is_err(), "a short rect and no click point");
    }
}
