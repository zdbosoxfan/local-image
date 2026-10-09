//! local-image: Filter › Other › Frequency Separation… (GIMP's Wavelet Decompose, the retoucher's
//! two-layer version): the active pixel layer is split into **Low Frequency** (a Gaussian blur:
//! colour and tone) and **High Frequency** (`(image − low) / 2 + ½` in Linear Light: texture),
//! both above it. Together they reproduce the image exactly, so tone can be smoothed on one layer
//! while pores and hair stay crisp on the other.

use photocraft_color::BlendMode;
use photocraft_doc::{Layer, LayerContent};
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

fn has_pixel_layer(s: &Session) -> std::result::Result<(), String> {
    let st = s.active().ok_or("no document open")?;
    let id = st.active_layer.ok_or("no active layer")?;
    match st.doc.layer(id).map(|l| &l.content) {
        Some(LayerContent::Raster(_)) => Ok(()),
        _ => Err("Frequency Separation needs a pixel layer".into()),
    }
}

/// Splits `data` (`n` channels, alpha at `alpha`) into low and high frequency.
pub fn split(data: &[f32], w: usize, h: usize, n: usize, alpha: Option<usize>, radius: f32) -> (Vec<f32>, Vec<f32>) {
    let mut low = data.to_vec();
    let mut high = data.to_vec();
    for c in (0..n).filter(|c| Some(*c) != alpha) {
        let plane: Vec<f32> = (0..w * h).map(|i| data[i * n + c]).collect();
        let blurred = photocraft_algo::matting::gaussian_blur(&plane, w, h, radius.max(0.5));
        for i in 0..w * h {
            low[i * n + c] = blurred[i];
            high[i * n + c] = ((plane[i] - blurred[i]) * 0.5 + 0.5).clamp(0.0, 1.0);
        }
    }
    (low, high)
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let id = st.active_layer.ok_or_else(|| EngineError::Other("no active layer".into()))?;
    let (w, h) = (st.doc.size.width, st.doc.size.height);
    // About 1 px per 500 px of the shorter side (4 px on a 2000 px portrait), as retouchers start.
    let radius = p.get("radius").and_then(Value::as_f64).map(|r| r as f32).unwrap_or((w.min(h) as f32 / 500.0).max(2.0)).clamp(0.5, 250.0);
    let layer = st.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    let surf = layer.surface().ok_or_else(|| EngineError::Other("Frequency Separation needs a pixel layer".into()))?;
    let fmt = surf.format();
    let n = fmt.channels();
    let alpha = fmt.alpha.then(|| n - 1);
    let rect = photocraft_geom::Rect::new(0, 0, w as i32, h as i32);
    let data = surf.read_region(rect);
    let (low, high) = split(&data, w as usize, h as usize, n, alpha, radius);
    let name = layer.name.clone();
    s.edit("Frequency Separation", |doc, active| {
        let mut lo = Layer::raster(format!("{name} · Low Frequency"), fmt);
        crate::pixels_mut(&mut lo)?.write_region(rect, &low);
        let mut hi = Layer::raster(format!("{name} · High Frequency"), fmt);
        crate::pixels_mut(&mut hi)?.write_region(rect, &high);
        hi.blend = BlendMode::LinearLight;
        let lo_id = doc.insert_above(Some(id), lo);
        let hi_id = doc.insert_above(Some(lo_id), hi);
        *active = Some(hi_id);
        Ok(())
    })?;
    Ok(json!({ "radius": radius }))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "filter.frequencySeparation",
        label: "Frequency Separation…",
        menu: &[],
        shortcut: None,
        params: r##"{"radius":px?} — low/high frequency layers above the active pixel layer (radius defaults to the shorter side / 500, at least 2)"##,
        enabled: has_pixel_layer,
        run,
        journal: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Low + high in Linear Light (`low + 2·high − 1`) gives back the image.
    #[test]
    fn the_two_layers_recombine_exactly() {
        let (w, h, n) = (24usize, 16usize, 4usize);
        let data: Vec<f32> = (0..w * h * n).map(|i| if i % 4 == 3 { 1.0 } else { ((i * 37) % 101) as f32 / 100.0 }).collect();
        let (low, high) = split(&data, w, h, n, Some(3), 3.0);
        let mut worst = 0f32;
        for i in 0..w * h {
            for c in 0..3 {
                let back = low[i * n + c] + 2.0 * high[i * n + c] - 1.0;
                if (0.0..=1.0).contains(&back) {
                    worst = worst.max((back - data[i * n + c]).abs());
                }
            }
            assert_eq!(low[i * n + 3], 1.0);
        }
        assert!(worst < 1e-4, "{worst}");
    }

    #[test]
    fn the_command_adds_two_layers_above() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        s.execute("paint.stroke", json!({"points": [[10.0, 10.0], [50.0, 40.0]], "size": 12})).unwrap();
        let before = s.active().unwrap().doc.walk().len();
        s.execute("filter.frequencySeparation", json!({"radius": 3})).unwrap();
        let st = s.active().unwrap();
        assert_eq!(st.doc.walk().len(), before + 2);
        let top = st.doc.layer(st.active_layer.unwrap()).unwrap();
        assert!(top.name.ends_with("High Frequency") && top.blend == BlendMode::LinearLight);
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc.walk().len(), before);
    }
}
