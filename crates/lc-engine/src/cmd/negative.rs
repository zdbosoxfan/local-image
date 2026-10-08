//! Film negative conversion commands (`develop.negative.*`): the film base picker and the automatic
//! estimates of darktable's negadoctor (see `lightcraft_pipeline::negative`).

use lightcraft_develop::Negative;
use lightcraft_geom::Rect;
use lightcraft_pipeline::negative::{self as neg, AreaStats};
use serde_json::Value;

use super::develop::{active, edit};
use super::{CommandSpec, bad, cmd, f64_or, f64_req, has_active};
use crate::{Result, Session, media::SourceLevel};

/// `{x, y, w?, h?}` (normalized oriented-image coordinates of the uncropped photo) as a rectangle.
fn area_param(p: &Value, c: &str) -> Result<Rect> {
    let (x, y) = (f64_req(p, "x", c)?, f64_req(p, "y", c)?);
    let (w, h) = (f64_or(p, "w", 0.0), f64_or(p, "h", 0.0));
    if ![x, y, w, h].iter().all(|v| v.is_finite()) {
        return Err(bad(c, "x, y, w, h must be finite numbers"));
    }
    Ok(Rect::from_xywh(x, y, w, h))
}

/// The area when the parameters name one (`x` given).
fn opt_area(p: &Value, c: &str) -> Result<Option<Rect>> {
    if p.get("x").is_some() { area_param(p, c).map(Some) } else { Ok(None) }
}

/// Statistics of the conversion's input on the active photo: over `area` when given, else over
/// the photo as cropped.
fn stats(s: &mut Session, area: Option<Rect>, c: &str) -> Result<AreaStats> {
    let id = active(s, c)?;
    let src = s.source_now(id, SourceLevel::Thumb).map_err(|e| bad(c, e))?;
    let info = s.source_info(id);
    let d = s.develop_of(id).unwrap_or_default();
    let st = match area {
        Some(a) => neg::sample_area(&src, &info, &d, a),
        None => neg::sample_image(&src, &info, &d),
    };
    st.ok_or_else(|| bad(c, "the area is outside the photo"))
}

/// Apply `f` to the active photo's negative settings (switching the conversion on) and return them.
fn set_negative(s: &mut Session, c: &str, label: &str, f: impl FnOnce(&Negative) -> Negative) -> Result<Value> {
    let mut out = None;
    edit(s, c, label, |d| {
        d.negative = f(&d.negative);
        d.negative.enabled = true;
        out = Some(d.negative);
        Ok(())
    })?;
    Ok(serde_json::to_value(out).unwrap_or_default())
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "develop.negative.pickDmin",
            "Pick Film Base",
            [],
            None,
            "{x, y, w?, h?} — an area of the unexposed film rim (normalized coords of the uncropped image); sets the film base colour (D-min) to its average and turns the negative conversion on → the negative settings",
            has_active,
            |s, p| {
                let c = "develop.negative.pickDmin";
                let st = stats(s, Some(area_param(p, c)?), c)?;
                set_negative(s, c, "Film Base", |n| Negative { dmin: neg::auto_dmin(n.film, &st), ..*n })
            }
        ),
        cmd!(
            "develop.negative.autoDmax",
            "Auto Film Range",
            [],
            None,
            "{x?, y?, w?, h?} — sets D-max and the scan exposure bias from the densest and thinnest parts of the negative (the area, else the photo as cropped) → the negative settings",
            has_active,
            |s, p| {
                let c = "develop.negative.autoDmax";
                let st = stats(s, opt_area(p, c)?, c)?;
                set_negative(s, c, "Auto D-max", |n| neg::auto_range(n, &st))
            }
        ),
        cmd!(
            "develop.negative.autoExposure",
            "Auto Print Exposure",
            [],
            None,
            "{x?, y?, w?, h?} — sets the paper black and print exposure so the print spans black to white (the area, else the photo as cropped) → the negative settings",
            has_active,
            |s, p| {
                let c = "develop.negative.autoExposure";
                let st = stats(s, opt_area(p, c)?, c)?;
                set_negative(s, c, "Auto Print Exposure", |n| neg::auto_print(n, &st))
            }
        ),
        cmd!(
            "develop.negative.auto",
            "Auto Negative",
            [],
            None,
            "{x?, y?, w?, h?} — D-max, scan exposure bias, paper black and print exposure from the negative (the film base is kept: pick it first) → the negative settings",
            has_active,
            |s, p| {
                let c = "develop.negative.auto";
                let st = stats(s, opt_area(p, c)?, c)?;
                set_negative(s, c, "Auto Negative", |n| neg::auto_print(&neg::auto_range(n, &st), &st))
            }
        ),
    ]
}
