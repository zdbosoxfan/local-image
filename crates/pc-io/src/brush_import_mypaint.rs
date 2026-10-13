//! MyPaint JSON v3 per mypaint/lib/brush.py and libmypaint/brushsettings.json.
use super::brush_import as common;
use photocraft_paint::{BrushSettings, Control, Dynamic};
use serde_json::Value;
pub(crate) fn read(bytes: &[u8], group: &str) -> Result<photocraft_paint::BrushPreset, String> {
    if bytes.len() > 1 << 20 {
        return Err("MyPaint preset exceeds 1 MiB".into());
    }
    let json: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if json.get("version").and_then(Value::as_u64) != Some(3) {
        return Err("only MyPaint JSON version 3 is supported".into());
    }
    let settings = json.get("settings").and_then(Value::as_object).ok_or("missing MyPaint settings")?;
    let base = |k: &str, d: f32| settings.get(k).and_then(|s| s.get("base_value")).and_then(Value::as_f64).filter(|v| v.is_finite()).map_or(d, |v| v as f32);
    let pressure = |k: &str| settings.get(k).and_then(|s| s.pointer("/inputs/pressure")).and_then(Value::as_array).is_some_and(|a| !a.is_empty());
    let mut b = BrushSettings {
        size: (2.0 * base("radius_logarithmic", 2.3).clamp(-3.0, 8.0).exp()).clamp(0.5, photocraft_paint::MAX_BRUSH_SIZE),
        hardness: base("hardness", 0.8).clamp(0.0, 1.0),
        opacity: base("opaque", 1.0).clamp(0.0, 1.0),
        flow: base("opaque_multiply", 1.0).clamp(0.0, 1.0),
        pressure_size: pressure("radius_logarithmic"),
        pressure_opacity: pressure("opaque") || pressure("opaque_multiply"),
        erase: base("eraser", 0.0) > 0.5,
        roundness: 1.0 / base("elliptical_dab_ratio", 1.0).clamp(1.0, 100.0),
        angle: base("elliptical_dab_angle", 0.0),
        ..Default::default()
    };
    // libmypaint dabs_per_actual_radius/basic_radius counts dabs per radius (diameter = 2r).
    b.spacing = (0.5 / base("dabs_per_actual_radius", 0.0).max(base("dabs_per_basic_radius", 2.0)).max(0.05)).clamp(0.01, 10.0);
    if pressure("opaque_multiply") {
        b.transfer.enabled = true;
        b.transfer.flow = Dynamic::controlled(Control::PenPressure);
    }
    b.import_notes.push("MyPaint brush dynamics and dab model mapped approximately; non-pressure sensors and smudge settings are not reproduced".into());
    let name = json.get("name").and_then(Value::as_str).unwrap_or(group);
    Ok(common::preset(name, group, b))
}
