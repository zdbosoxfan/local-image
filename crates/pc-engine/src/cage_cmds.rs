//! local-image: Edit › Transform › Cage (GIMP's and Krita's cage transform): the pixels inside a
//! cage polygon follow its vertices to their targets through Green coordinates (Lipman et al.
//! 2008) or mean value coordinates (see `photocraft_geom::cage` and `photocraft_algo::cage`).
//! A pixel layer is edited in place (a linked mask follows); a smart object records the cage as
//! a smart filter, re-applied whenever it renders, as Puppet Warp does.

use photocraft_algo::cage::{cage_warp_gray, cage_warp_surface};
use photocraft_algo::transform::Interp;
use photocraft_doc::LayerContent;
use photocraft_geom::cage::{CageCoords, CageMap};
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

/// The command (and smart filter) id.
pub const CAGE: &str = "edit.transform.cage";

fn bad(msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: CAGE.into(), msg: msg.into() }
}

fn enabled(s: &Session) -> std::result::Result<(), String> {
    let d = s.active().ok_or("no document open")?;
    let id = d.active_layer.ok_or("no active layer")?;
    match &d.doc.layer(id).ok_or("no active layer")?.content {
        LayerContent::Raster(_) => Ok(()),
        LayerContent::Smart(sm) if sm.cache.is_some() => Ok(()),
        other => Err(format!("needs a pixel layer or smart object (active layer is a {} layer)", other.kind_name())),
    }
}

fn points(p: &Value, key: &str) -> Result<Vec<[f64; 2]>> {
    let a = p.get(key).and_then(Value::as_array).ok_or_else(|| bad(format!("needs \"{key}\": [[x, y], …]")))?;
    a.iter()
        .map(|q| {
            let x = q.get(0).and_then(Value::as_f64);
            let y = q.get(1).and_then(Value::as_f64);
            x.zip(y).map(|(x, y)| [x, y]).ok_or_else(|| bad(format!("\"{key}\" points must be [x, y]")))
        })
        .collect()
}

/// The cage map and interpolation from command params.
pub fn params(p: &Value) -> Result<(CageMap, Interp)> {
    let cage = points(p, "cage")?;
    let target = points(p, "target")?;
    let coords = match p.get("coordinates").and_then(Value::as_str) {
        None => CageCoords::Green,
        Some(c) => CageCoords::parse(c).ok_or_else(|| bad("coordinates: green|meanValue"))?,
    };
    let map = CageMap::new(&cage, &target, coords).map_err(|e| bad(e.to_string()))?;
    let interp = Interp::parse(p.get("interpolation").and_then(Value::as_str).unwrap_or("bicubic"));
    Ok((map, interp))
}

/// Smart-filter hook: the cage applied to a rendered smart object.
pub fn apply_to_surface(p: &Value, surf: &Surface) -> Option<Surface> {
    let (map, interp) = params(p).ok()?;
    Some(cage_warp_surface(surf, &map, interp))
}

fn cage(s: &mut Session, p: &Value) -> Result<Value> {
    let (map, interp) = params(p)?;
    if map.is_identity() {
        let id = p.get("layer").and_then(Value::as_u64).or_else(|| s.active().and_then(|d| d.active_layer).map(|l| l.0));
        return Ok(json!({"layer": id, "changed": false}));
    }
    crate::distort_cmds::run_on_layer(s, CAGE, "Cage", p, true, &|surf, _, _| cage_warp_surface(surf, &map, interp), Some(&|m| cage_warp_gray(m, &map, interp)))
}

/// Cage transform command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: CAGE,
        label: "Cage",
        menu: &["Edit", "Transform"],
        shortcut: None,
        params: r##"{"cage":[[x,y]…] (≥ 3 points, a polygon that doesn't cross itself),"target":[[x,y]…] (where each cage point goes),"coordinates":"green|meanValue"="green","interpolation":"bicubic|bilinear|nearest"="bicubic","layer":id?=active} → {"layer","changed"} (pixels inside the cage follow it; a smart object keeps it as a smart filter)"##,
        enabled,
        run: cage,
        journal: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_doc::LayerId;
    use photocraft_geom::Rect;

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 100, "height": 100})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        s.edit("paint", |doc, active| {
            let l = doc.layer_mut(active.unwrap()).unwrap();
            l.surface_mut().unwrap().fill_rect(Rect::new(40, 40, 50, 50), &[1.0, 0.0, 0.0, 1.0]);
            Ok(())
        })
        .unwrap();
        s
    }

    fn cage_params(dx: f64) -> Value {
        let c = [[30.0, 30.0], [60.0, 30.0], [60.0, 60.0], [30.0, 60.0]];
        let t: Vec<[f64; 2]> = c.iter().map(|p| [p[0] + dx, p[1]]).collect();
        json!({"cage": c, "target": t, "interpolation": "bilinear"})
    }

    fn alpha(s: &Session, x: i32, y: i32) -> f32 {
        let d = s.active().unwrap();
        d.doc.layer(d.active_layer.unwrap()).unwrap().surface().unwrap().pixel(x, y)[3]
    }

    #[test]
    fn cage_moves_pixels_and_undoes() {
        let mut s = session();
        let r = s.execute(CAGE, cage_params(20.0)).unwrap();
        assert_eq!(r["changed"], json!(true));
        assert!(alpha(&s, 65, 45) > 0.95 && alpha(&s, 45, 45) < 0.05);
        s.execute("edit.undo", json!({})).unwrap();
        assert!(alpha(&s, 45, 45) > 0.95);
        // Identity: nothing to do.
        assert_eq!(s.execute(CAGE, cage_params(0.0)).unwrap()["changed"], json!(false));
    }

    #[test]
    fn bad_cages_are_bad_params() {
        let mut s = session();
        for p in [
            json!({"cage": [[0, 0], [10, 0]], "target": [[0, 0], [10, 0]]}),
            json!({"cage": [[0, 0], [10, 0], [10, 10]], "target": [[0, 0], [10, 0]]}),
            json!({"cage": [[0, 0], [10, 10], [10, 0], [0, 12]], "target": [[0, 0], [10, 10], [10, 0], [0, 12]]}),
            json!({"cage": [[0, 0], [10, 0], [10, 10]], "target": [[0, 0], [10, 0], [10, 10]], "coordinates": "x"}),
            json!({"target": []}),
        ] {
            assert!(matches!(s.execute(CAGE, p.clone()), Err(EngineError::BadParams { .. })), "{p}");
        }
    }

    #[test]
    fn smart_objects_keep_the_cage_as_a_smart_filter() {
        let mut s = session();
        let id = s.active().unwrap().active_layer.unwrap();
        s.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
        let sid = s.active().unwrap().active_layer.unwrap_or(id);
        s.execute(CAGE, cage_params(20.0)).unwrap();
        let d = s.active().unwrap();
        let l = d.doc.layer(sid).or_else(|| d.doc.layer(LayerId(sid.0))).unwrap();
        let LayerContent::Smart(sm) = &l.content else { panic!("not a smart object") };
        assert_eq!(sm.smart_filters.last().map(|f| f.command.as_str()), Some(CAGE));
        let c = sm.cache.as_ref().unwrap();
        assert!(c.pixel(65, 45)[3] > 0.9 && c.pixel(45, 45)[3] < 0.1, "{:?} {:?}", c.pixel(65, 45), c.pixel(45, 45));
    }
}
