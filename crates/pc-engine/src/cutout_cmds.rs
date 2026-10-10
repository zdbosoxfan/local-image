//! Remove Background for a pixel layer: Select Subject with edge refinement, then a mask,
//! applied transparency, a background plate or a new cutout layer, in one history step.

use photocraft_algo::matting::{self, RefineParams};
use photocraft_algo::segment::subject;
use photocraft_doc::{Color, Document, Fill, Layer, LayerContent, LayerId, LayerMask};
use photocraft_raster::Surface;

use crate::jobs::JobCtx;
use serde_json::{Value, json};

use crate::commands::{CommandSpec, layer_param};
use crate::smartselect_cmds::with_doc_sampler;
use crate::{EngineError, Result, Session};

const CMD: &str = "layer.removeBackground";

/// The output of either subject-removal engine. Missing parameters preserve the historical mask.
#[derive(Clone, Debug, PartialEq)]
pub enum BackgroundOutput {
    Mask,
    Transparent,
    White,
    Color([f32; 4]),
    Blur(f32),
    NewLayer,
}

impl BackgroundOutput {
    pub fn from_params(p: &Value, cmd: &str) -> Result<Self> {
        let bad = |msg: &str| EngineError::BadParams { cmd: cmd.into(), msg: msg.into() };
        match p.get("output") {
            None => Ok(Self::Mask),
            Some(Value::String(v)) => match v.as_str() {
                "mask" => Ok(Self::Mask),
                "transparent" => Ok(Self::Transparent),
                "white" => Ok(Self::White),
                "newLayer" => Ok(Self::NewLayer),
                "color" => {
                    let valid = match p.get("color") {
                        Some(Value::Array(a)) => (3..=4).contains(&a.len()) && a.iter().all(|v| v.as_f64().is_some_and(|v| (0.0..=1.0).contains(&v))),
                        Some(Value::String(s)) => {
                            let hex = s.strip_prefix('#').unwrap_or(s);
                            matches!(hex.len(), 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
                        }
                        _ => false,
                    };
                    if !valid {
                        return Err(bad("color must be #rrggbb[aa] or 3–4 numbers in 0..1"));
                    }
                    Ok(Self::Color(crate::commands::color_param(p, "color", [1.0; 4])))
                }
                "blur" => {
                    let amount = match p.get("amount") {
                        None => 12.0,
                        Some(v) => v.as_f64().filter(|v| (0.0..=1000.0).contains(v)).ok_or_else(|| bad("amount must be in 0..1000 pixels"))? as f32,
                    };
                    Ok(Self::Blur(amount))
                }
                _ => Err(bad("output must be mask|transparent|white|color|blur|newLayer")),
            },
            _ => Err(bad("output must be a string")),
        }
    }
}

/// Apply a matte and its output within the caller's single document edit. Both CPU and Qwen
/// commands share this path, including cancellation and sibling placement inside groups.
pub fn apply_output(doc: &mut Document, id: LayerId, mask: Surface, output: &BackgroundOutput, ctx: &JobCtx) -> Result<LayerId> {
    let bounds = doc.bounds();
    let original = doc.layer(id).ok_or(EngineError::NoLayer(id))?.clone();
    let matte = LayerMask { surface: mask, ..LayerMask::reveal_all() };
    let plate = match output {
        BackgroundOutput::White | BackgroundOutput::Color(_) => {
            let c = match output {
                BackgroundOutput::Color(c) => *c,
                _ => [1.0; 4],
            };
            Some(Layer::new(doc.next_layer_name("Background Color"), LayerContent::Fill(Fill::Solid(Color::rgba(c[0], c[1], c[2], c[3])))))
        }
        BackgroundOutput::Blur(amount) => {
            let mut plate = original.duplicate();
            plate.name = doc.next_layer_name("Blurred Background");
            // Re-running removal after a mask output must still reveal the photo's background.
            plate.mask = None;
            let src = original.surface().ok_or_else(|| EngineError::Other("Remove Background needs a pixel layer".into()))?;
            if *amount > 0.0 {
                let bounds = doc.bounds();
                let extent = bounds.union(&src.content_bounds());
                let params = photocraft_algo::FilterParams::GaussianBlur { radius: *amount };
                let blurred = ctx
                    .stage(0.98, 1.0, "Blurring background", |ctl| photocraft_algo::apply_in_with(src, &params, extent, bounds, None, extent, ctl))
                    .ok_or(EngineError::Cancelled)?;
                plate.content = LayerContent::Raster(blurred);
            }
            Some(plate)
        }
        _ => None,
    };
    ctx.check()?;
    let result = if *output == BackgroundOutput::NewLayer {
        let mut cutout = original.duplicate();
        cutout.name = doc.next_layer_name("Cutout");
        cutout.visible = true;
        cutout.locks.transparency = false;
        cutout.locks.position = false;
        cutout.mask = Some(matte);
        let new_id = doc.insert_above(Some(id), cutout);
        doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?.visible = false;
        new_id
    } else {
        crate::extra_cmds::background_to_layer_for_mask(doc, id);
        let layer = doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?;
        if *output == BackgroundOutput::Transparent {
            let src = layer.surface().ok_or_else(|| EngineError::Other("Remove Background needs a pixel layer".into()))?;
            let mut fmt = src.format();
            fmt.alpha = true;
            let pixels = if src.format().alpha { src.clone() } else { src.convert(fmt) };
            let area = src.content_bounds().union(&bounds);
            let mut raw = pixels.read_region(area);
            let n = fmt.channels();
            let w = area.width() as usize;
            for (i, px) in raw.chunks_exact_mut(n).enumerate() {
                if i % 4096 == 0 {
                    ctx.check()?;
                }
                px[n - 1] *= matte.value(area.x0 + (i % w) as i32, area.y0 + (i / w) as i32);
                if px[n - 1] == 0.0 {
                    px.fill(0.0);
                }
            }
            // A non-alpha source reads opaque even in unallocated tiles. The applied matte
            // hides everything outside its coverage, so the resulting default must be empty.
            let mut applied = Surface::new(fmt);
            applied.write_region(area, &raw);
            applied.prune();
            layer.content = LayerContent::Raster(applied);
            layer.mask = None;
        } else {
            layer.mask = Some(matte);
        }
        id
    };
    if let Some(plate) = plate {
        let plate_id = doc.insert_above(Some(id), plate);
        doc.shift(plate_id, -1);
    }
    doc.selection = None;
    Ok(result)
}

/// Edge refinement after Select Subject: a narrow smart radius and a little smoothing, so soft
/// edges and hair get partial coverage instead of a hard cut.
const REFINE: RefineParams = RefineParams { radius: 2.0, smart_radius: true, smooth: 10.0, feather: 0.5, contrast: 10.0, shift_edge: 0.0 };

/// Remove Background needs an unlocked pixel layer.
fn check(doc: &Document, l: &Layer) -> std::result::Result<(), String> {
    if !matches!(l.content, LayerContent::Raster(_)) {
        return Err(format!("the layer is a {} layer, not a pixel layer", l.content.kind_name()));
    }
    let locks = doc.effective_locks(l.id);
    if locks.pixels || locks.all {
        return Err(format!("Could not complete your request because the layer \"{}\" is locked", l.name));
    }
    Ok(())
}

fn enabled(s: &Session) -> std::result::Result<(), String> {
    let d = s.active().ok_or("no document open")?;
    check(&d.doc, crate::active_layer_of(s)?)
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let id = layer_param(s, p)?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    check(doc, doc.layer(id).ok_or(EngineError::NoLayer(id))?).map_err(EngineError::Other)?;
    let output = BackgroundOutput::from_params(p, CMD)?;
    let sample_all = p.get("sampleAllLayers").and_then(Value::as_bool).unwrap_or(false);
    let refine = p.get("refine").and_then(Value::as_bool).unwrap_or(true);
    let require_learned = p.get("engine").and_then(Value::as_str) == Some("learned");
    let learned = crate::seg::wanted(p).map_err(|msg| EngineError::BadParams { cmd: CMD.into(), msg })?;
    crate::jobs::edit_job(
        s,
        "Remove Background",
        move |doc, active, ctx| {
            ctx.progress(0.1, "Finding subject");
            // local-image: the learned model first (when installed), the classical heuristic otherwise.
            let learned = learned.then(|| crate::seg::subject(doc, Some(id), sample_all)).flatten();
            if require_learned && learned.is_none() {
                return Err(EngineError::Other("no subject found".into()));
            }
            let refine_params = if learned.is_some() { crate::smartselect_cmds::learned_refine(doc.bounds()) } else { REFINE };
            let found = learned.or_else(|| with_doc_sampler(doc, Some(id), sample_all, |smp, d| subject::select_subject(smp, d.bounds())));
            let mut region = found.ok_or_else(|| EngineError::Other("no subject found".into()))?;
            if ctx.cancelled() {
                return Err(EngineError::Cancelled);
            }
            if refine {
                ctx.progress(0.6, "Refining edge");
                let refined = with_doc_sampler(doc, Some(id), sample_all, |smp, d| {
                    matting::refine_mask(smp, &matting::region_reader(&region), region.bbox, d.bounds(), &refine_params)
                });
                region = refined.unwrap_or(region);
                if ctx.cancelled() {
                    return Err(EngineError::Cancelled);
                }
            }
            let result = apply_output(doc, id, matting::region_surface(&region), &output, ctx)?;
            *active = Some(result);
            let b = region.bbox;
            Ok((result, [b.x0, b.y0, b.width() as i32, b.height() as i32]))
        },
        move |(result, bounds)| json!({ "layer": result.0, "bounds": bounds }),
    )
}

/// Remove Background command spec (no menu path: a Properties Quick Action in Photoshop).
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: CMD,
        label: "Remove Background",
        menu: &[],
        shortcut: None,
        params: r##"{"layer":id?,"sampleAllLayers":bool=false,"refine":bool=true,"engine":"auto|classic|learned"="auto","output":"mask|transparent|white|color|blur|newLayer"="mask","color":"#rrggbb[aa]"|[r,g,b,a?],"amount":0..1000=12} → {layer,bounds} (the Background becomes a normal layer; one undo step)"##,
        enabled,
        run,
        journal: true,
    }]
}

#[cfg(test)]
#[path = "cutout_output_tests.rs"]
mod output_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_algo::segment::Rng;
    use photocraft_doc::LayerId;
    use photocraft_geom::Rect;

    fn session_with(w: u32, h: u32, depth: u32, px: impl Fn(u32, u32) -> [f32; 3]) -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": w, "height": h, "depth": depth})).unwrap();
        s.edit("paint", |doc, _| {
            let bg = doc.layers[0].surface_mut().unwrap();
            let n = bg.channels();
            let mut data = Vec::with_capacity((w * h) as usize * n);
            for y in 0..h {
                for x in 0..w {
                    let c = px(x, y);
                    data.extend_from_slice(&c[..n.min(3)]);
                    if n == 4 {
                        data.push(1.0);
                    }
                }
            }
            bg.write_region(Rect::new(0, 0, w as i32, h as i32), &data);
            Ok(())
        })
        .unwrap();
        s
    }

    /// Textured disc (centre (100, 80), radius 45) on a noisy background.
    fn disc(depth: u32) -> (Session, impl Fn(i32, i32) -> bool) {
        let mut rng = Rng::new(2);
        let nz: Vec<f32> = (0..200 * 160).map(|_| rng.normal()).collect();
        let inside = |x: i32, y: i32| {
            let (dx, dy) = (x as f32 + 0.5 - 100.0, y as f32 + 0.5 - 80.0);
            dx * dx + dy * dy <= 45.0 * 45.0
        };
        let s = session_with(200, 160, depth, move |x, y| {
            let n = 0.05 * nz[(y * 200 + x) as usize];
            if inside(x as i32, y as i32) {
                let t = ((x + y) as f32 * 0.35).sin() * 0.5 + 0.5;
                [0.85 + 0.05 * t + n, 0.2 + 0.3 * t + n, 0.15 + n]
            } else {
                [0.25 + n, 0.45 + n, 0.7 + n]
            }
            .map(|v| v.clamp(0.0, 1.0))
        });
        (s, inside)
    }

    fn active_id(s: &Session) -> LayerId {
        s.active().unwrap().active_layer.unwrap()
    }

    fn layer(s: &Session, id: LayerId) -> &Layer {
        s.active().unwrap().doc.layer(id).unwrap()
    }

    fn mask_cov(s: &Session, x: i32, y: i32) -> f32 {
        layer(s, active_id(s)).mask.as_ref().map_or(0.0, |m| m.value(x, y))
    }

    fn mask_iou(s: &Session, w: i32, h: i32, truth: impl Fn(i32, i32) -> bool) -> f32 {
        let (mut i, mut u) = (0, 0);
        for y in 0..h {
            for x in 0..w {
                let (a, b) = (mask_cov(s, x, y) >= 0.5, truth(x, y));
                i += (a && b) as i32;
                u += (a || b) as i32;
            }
        }
        i as f32 / u.max(1) as f32
    }

    #[test]
    fn background_output_cpu_command_dispatches_all_modes() {
        for output in ["mask", "transparent", "white", "color", "blur", "newLayer"] {
            let (mut s, _) = disc(8);
            let before = s.active().unwrap().doc.clone();
            let history = s.active().unwrap().history.past_len();
            let r = s.execute(CMD, json!({"engine":"classic", "output":output, "color":"#336699", "amount":2})).unwrap();
            let st = s.active().unwrap();
            let target = st.doc.layer(LayerId(r["layer"].as_u64().unwrap())).unwrap();
            assert_eq!(target.mask.is_some(), output != "transparent");
            assert_eq!(st.doc.layers.len(), if matches!(output, "mask" | "transparent") { 1 } else { 2 });
            assert_eq!(st.history.past_len(), history + 1);
            s.undo();
            assert_eq!(s.active().unwrap().doc, before);
        }
    }

    #[test]
    fn remove_background_masks_the_subject_at_every_depth_and_undoes() {
        for depth in [8, 16, 32] {
            let (mut s, inside) = disc(depth);
            assert!(s.is_enabled(CMD));
            let id = active_id(&s);
            let pixels = layer(&s, id).surface().unwrap().read_region(Rect::new(0, 0, 200, 160));
            let steps = s.active().unwrap().history.past_len();
            let r = s.execute(CMD, json!({})).unwrap();
            assert_eq!(r["layer"], id.0);
            assert_eq!(s.active().unwrap().history.past_len(), steps + 1, "depth {depth}: one history step");
            assert!(layer(&s, id).mask.is_some(), "depth {depth}: expected a layer mask");
            let v = mask_iou(&s, 200, 160, &inside);
            assert!(v >= 0.75, "depth {depth}: IoU {v}");
            assert!(mask_cov(&s, 0, 0) < 0.2, "depth {depth}: background corner still revealed");
            assert!(mask_cov(&s, 100, 80) >= 0.8, "depth {depth}: disc centre hidden");
            // Non-destructive: the pixels are untouched, only masked.
            assert_eq!(layer(&s, id).surface().unwrap().read_region(Rect::new(0, 0, 200, 160)), pixels, "depth {depth}");
            let corner = photocraft_compose::render(&s.active().unwrap().doc, Rect::new(0, 0, 1, 1)).px[0][3];
            assert!(corner < 0.2, "depth {depth}: composite corner alpha {corner}");
            assert!(s.active().unwrap().doc.selection.is_none());
            s.execute("edit.undo", json!({})).unwrap();
            assert!(layer(&s, id).mask.is_none(), "depth {depth}: mask remained after undo");
            s.execute("edit.redo", json!({})).unwrap();
            assert!(layer(&s, id).mask.is_some(), "depth {depth}: mask missing after redo");
        }
    }

    #[test]
    fn remove_background_turns_the_background_into_a_layer() {
        let (mut s, _) = disc(8);
        let id = active_id(&s);
        assert_eq!(layer(&s, id).name, "Background");
        s.execute(CMD, json!({})).unwrap();
        let l = layer(&s, id);
        assert_eq!(l.name, "Layer 0");
        assert!(!l.locks.transparency && !l.locks.position);
        assert!(l.mask.is_some());
    }

    #[test]
    fn remove_background_without_a_subject_leaves_the_document() {
        let mut s = session_with(32, 32, 8, |_, _| [0.5, 0.5, 0.5]);
        let before = s.active().unwrap().revision;
        let id = active_id(&s);
        assert!(s.execute(CMD, json!({})).is_err());
        assert_eq!(s.active().unwrap().revision, before);
        assert!(layer(&s, id).mask.is_none());
    }

    #[test]
    fn remove_background_disabled_states_and_bad_params() {
        assert!(!Session::new().is_enabled(CMD));
        assert!(Session::new().execute(CMD, json!({})).is_err());

        let (mut s, _) = disc(8);
        s.execute("type.create", json!({"text": "Hi", "size": 24, "x": 10, "y": 20})).unwrap();
        assert!(!s.is_enabled(CMD));
        assert!(s.execute(CMD, json!({})).is_err());

        let (mut s, _) = disc(8);
        s.execute("shape.create", json!({"kind": "rect", "rect": [10, 10, 40, 20], "fill": "#ff0000"})).unwrap();
        assert!(!s.is_enabled(CMD));
        assert!(s.execute(CMD, json!({})).is_err());

        let (mut s, _) = disc(8);
        s.execute("layer.setProps", json!({"locks": {"pixels": true}})).unwrap();
        assert!(!s.is_enabled(CMD));
        assert!(s.execute(CMD, json!({})).is_err());

        let (mut s, _) = disc(8);
        let before = s.active().unwrap().revision;
        assert!(s.execute(CMD, json!({"layer": 999_999})).is_err());
        // Wrong types fall back to the defaults (the active layer, layer-only sampling, refined).
        assert!(s.execute(CMD, json!({"layer": "x", "sampleAllLayers": 3, "refine": "no"})).is_ok());
        assert!(s.active().unwrap().revision > before);
    }

    #[test]
    fn remove_background_looks_at_the_layer_alone_by_default() {
        let (mut s, _) = disc(8);
        let bottom = active_id(&s);
        s.execute("layer.new.layer", json!({"name": "Empty"})).unwrap();
        let top = active_id(&s);
        let before = s.active().unwrap().revision;
        // The empty layer has no subject of its own, though the photo shows through it.
        assert!(s.execute(CMD, json!({})).is_err());
        assert_eq!(s.active().unwrap().revision, before);
        assert!(layer(&s, top).mask.is_none() && layer(&s, bottom).mask.is_none());
        // Sampling all layers finds the subject in the composite.
        s.execute(CMD, json!({"sampleAllLayers": true})).unwrap();
        assert!(layer(&s, top).mask.is_some() && layer(&s, bottom).mask.is_none());
    }

    #[test]
    fn remove_background_twice_replaces_the_mask() {
        let (mut s, _) = disc(8);
        s.execute(CMD, json!({})).unwrap();
        s.execute(CMD, json!({"refine": false})).unwrap();
        let masks = s.active().unwrap().doc.walk().into_iter().filter(|(_, _, l)| l.mask.is_some()).count();
        assert_eq!(masks, 1);
    }
}
