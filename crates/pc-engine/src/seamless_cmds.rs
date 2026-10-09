//! local-image: Edit › Paste Special › Paste Seamless: pastes the clipboard as a new layer and
//! blends it into what shows under it, GIMP's Seamless Clone idea: the pasted pixels keep their
//! detail while their colour and light follow the surroundings along the paste's edge (mean
//! value coordinates, Farbman et al. 2009; see `photocraft_algo::seamless`). One history step.

use photocraft_doc::{Document, LayerId};
use photocraft_paint::retouch::{Region, alpha_index};
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

fn has_clip(s: &Session) -> std::result::Result<(), String> {
    s.active().ok_or("no document open")?;
    s.clipboard.as_ref().map(|_| ()).ok_or_else(|| "the clipboard is empty".into())
}

/// Blends layer `id`'s pixels into what shows under them (within the canvas). Returns whether
/// anything was blended.
pub(crate) fn blend_layer_into_below(doc: &mut Document, id: LayerId) -> Result<bool> {
    let canvas = doc.bounds();
    let l = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    let surf = l.surface().ok_or_else(|| EngineError::Other("the layer has no pixels".into()))?;
    let fmt = surf.format();
    let area = surf.content_bounds().intersect(&canvas);
    if area.is_empty() {
        return Ok(false);
    }
    let src = Region::read(surf, area);
    let under = crate::retouch_cmds::composite_under(doc, id, area, fmt);
    let n = fmt.channels();
    let a = alpha_index(&fmt);
    let colour = a.unwrap_or(n);
    let (w, h) = (area.width() as usize, area.height() as usize);
    let mask: Vec<bool> = (0..w * h).map(|i| a.is_none_or(|a| src.data[i * n + a] >= 0.5)).collect();
    if !mask.iter().any(|m| *m) {
        return Ok(false);
    }
    // Where nothing shows underneath, there is nothing to match.
    let mut diff = vec![0.0f32; w * h * colour];
    for i in 0..w * h {
        let k = a.map_or(1.0, |a| under.data[i * n + a].clamp(0.0, 1.0));
        for c in 0..colour {
            diff[i * colour + c] = (under.data[i * n + c] - src.data[i * n + c]) * k;
        }
    }
    let mem = photocraft_algo::seamless::mvc_membrane(w, h, &mask, &diff, colour);
    let float = fmt.sample == photocraft_color::SampleType::F32;
    let mut out = src.data.clone();
    for i in (0..w * h).filter(|i| mask[*i]) {
        for c in 0..colour {
            let v = out[i * n + c] + mem[i * colour + c];
            out[i * n + c] = if float { v.max(0.0) } else { v.clamp(0.0, 1.0) };
        }
    }
    let l = doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?;
    let surf = crate::pixels_mut(l)?;
    surf.write_region(area, &out);
    Ok(true)
}

fn paste_seamless(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.active().ok_or(EngineError::NoDocument)?;
    let key = format!("pasteSeamless#{}", d.revision);
    let mut params = json!({"coalesce": key});
    if let Some(c) = p.get("center") {
        params["center"] = c.clone();
    } else if let Some(sel) = &d.doc.selection {
        // Into the selected area, as Paste Into places it.
        let b = sel.content_bounds();
        if !b.is_empty() {
            params["center"] = json!([(b.x0 + b.x1) as f64 / 2.0, (b.y0 + b.y1) as f64 / 2.0]);
        }
    }
    let r = s.execute("edit.paste", params)?;
    let id = r.get("layer").and_then(Value::as_u64).map(LayerId).ok_or_else(|| EngineError::Other("nothing was pasted".into()))?;
    s.coalesce_request = Some(key);
    let blended = s.edit("Paste Seamless", |doc, _| blend_layer_into_below(doc, id));
    s.coalesce_request = None;
    let blended = blended?;
    let mut out = r;
    out["blended"] = json!(blended);
    Ok(out)
}

/// Seamless paste command specs.
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "edit.pasteSpecial.pasteSeamless",
        label: "Paste Seamless",
        menu: &["Edit", "Paste Special"],
        shortcut: None,
        params: r##"{"center":[x,y]? (default: the selection's centre, else where Paste puts it)} → {"layer","offset","blended"} (a new layer whose colours blend into what shows under it)"##,
        enabled: has_clip,
        run: paste_seamless,
        journal: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_geom::Rect;

    /// A 120×100 document whose background is a horizontal ramp, with a 30×30 patch of the same
    /// ramp shifted by a constant colour on the clipboard.
    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 120, "height": 100})).unwrap();
        s.edit("ramp", |doc, active| {
            let l = doc.layer_mut(active.unwrap()).unwrap();
            let surf = crate::pixels_mut(l)?;
            for x in 0..120 {
                let v = 0.2 + 0.5 * x as f32 / 120.0;
                surf.fill_rect(Rect::new(x, 0, x + 1, 100), &[v, 0.4, 0.3, 1.0]);
            }
            Ok(())
        })
        .unwrap();
        s.execute("select.rect", json!({"x": 40, "y": 30, "width": 30, "height": 30, "antiAlias": false})).unwrap();
        s.execute("edit.copy", json!({})).unwrap();
        // Brighten the copy: a constant offset the blend must remove.
        let clip = s.clipboard.as_mut().unwrap();
        let b = clip.bounds;
        let px = clip.surface.read_region(b);
        let brighter: Vec<f32> = px.chunks_exact(4).flat_map(|p| [(p[0] + 0.3).min(1.0), (p[1] - 0.2).max(0.0), p[2], p[3]]).collect();
        clip.surface.write_region(b, &brighter);
        s.execute("select.deselect", json!({})).unwrap();
        s
    }

    #[test]
    fn a_constant_offset_paste_blends_into_its_surroundings_in_one_step() {
        let mut s = session();
        let steps = s.active().unwrap().history.past_len();
        let r = s.execute("edit.pasteSpecial.pasteSeamless", json!({"center": [55, 45]})).unwrap();
        assert_eq!(r["blended"], json!(true));
        let id = LayerId(r["layer"].as_u64().unwrap());
        let st = s.active().unwrap();
        assert_eq!(st.history.past_len(), steps + 1, "one history step");
        let surf = st.doc.layer(id).unwrap().surface().unwrap();
        // Inside the patch the pixels match the ramp underneath again (8-bit rounding).
        for (x, y) in [(42, 32), (55, 45), (68, 58), (60, 35)] {
            let p = surf.pixel(x, y);
            let want = 0.2 + 0.5 * x as f32 / 120.0;
            assert!((p[0] - want).abs() < 3.0 / 255.0 && (p[1] - 0.4).abs() < 3.0 / 255.0, "({x},{y}) {p:?} vs {want}");
        }
        s.execute("edit.undo", json!({})).unwrap();
        assert!(s.active().unwrap().doc.layer(id).is_none(), "undo removes the paste");
    }

    #[test]
    fn it_needs_a_clipboard() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 20, "height": 20})).unwrap();
        assert!(!s.is_enabled("edit.pasteSpecial.pasteSeamless"));
        assert!(s.execute("edit.pasteSpecial.pasteSeamless", json!({})).is_err());
    }
}
