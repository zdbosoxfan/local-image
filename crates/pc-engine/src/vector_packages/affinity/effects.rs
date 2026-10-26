//! Native effect validation at the detached Affinity scene boundary.
use super::{Result, error};
use photocraft_doc::{Color, Effects, effects::*};

pub(super) fn validate(effects: &Effects) -> Result<u64> {
    if effects.items.len() > 64 || effects.psd_raw.is_some() || effects.reference.is_some() {
        return Err(error("Affinity effect count or unsupported raw data"));
    }
    let mut bytes = effects.items.len() as u64 * std::mem::size_of::<Effect>() as u64;
    for effect in &effects.items {
        match effect {
            Effect::DropShadow(shadow) | Effect::InnerShadow(shadow) => {
                unit(shadow.common.opacity)?;
                color(shadow.color)?;
                unit(shadow.spread)?;
                unit(shadow.noise)?;
                length(shadow.size)?;
                length(shadow.distance)?;
                length(shadow.size + shadow.distance)?;
                angle(shadow.angle)?;
                contour(&shadow.contour)?;
                if shadow.use_global_light {
                    return Err(error("Affinity shadow must retain its own light direction"));
                }
            }
            Effect::OuterGlow(glow) => {
                unit(glow.common.opacity)?;
                unit(glow.spread)?;
                unit(glow.range)?;
                unit(glow.jitter)?;
                unit(glow.noise)?;
                length(glow.size)?;
                contour(&glow.contour)?;
                bytes = bytes.saturating_add(paint(&glow.paint)?);
            }
            Effect::Stroke(stroke) => {
                unit(stroke.common.opacity)?;
                length(stroke.size)?;
                bytes = bytes.saturating_add(paint(&stroke.paint)?);
            }
            Effect::ColorOverlay { common: settings, color: value } => {
                unit(settings.opacity)?;
                color(*value)?;
            }
            Effect::GradientOverlay { common: settings, gradient: value, .. } => {
                unit(settings.opacity)?;
                bytes = bytes.saturating_add(gradient(value)?);
            }
            Effect::BevelEmboss(bevel) => {
                length(bevel.size)?;
                length(bevel.soften)?;
                length(bevel.size + bevel.soften)?;
                angle(bevel.angle)?;
                if !bevel.depth.is_finite()
                    || !(0.0..=10.).contains(&bevel.depth)
                    || !bevel.altitude.is_finite()
                    || !(0.0..=90.).contains(&bevel.altitude)
                    || bevel.use_global_light
                    || bevel.contour.is_some()
                    || bevel.texture.is_some()
                {
                    return Err(error("invalid Affinity bevel"));
                }
                contour(&bevel.gloss_contour)?;
                unit(bevel.highlight.opacity)?;
                unit(bevel.shadow.opacity)?;
                color(bevel.highlight_color)?;
                color(bevel.shadow_color)?;
            }
            Effect::InnerGlow(_) | Effect::PatternOverlay { .. } | Effect::Satin(_) => {
                return Err(error("unsupported Affinity native effect"));
            }
        }
    }
    Ok(bytes)
}

fn unit(value: f32) -> Result<()> {
    if value.is_finite() && (0.0..=1.0).contains(&value) { Ok(()) } else { Err(error("invalid Affinity effect fraction")) }
}
fn length(value: f32) -> Result<()> {
    if value.is_finite() && (0.0..=photocraft_compose::effects::MAX_REACH).contains(&value) {
        Ok(())
    } else {
        Err(error("Affinity effect exceeds renderer reach"))
    }
}
fn angle(value: f32) -> Result<()> {
    if value.is_finite() && value.abs() <= 360. { Ok(()) } else { Err(error("invalid Affinity effect angle")) }
}
fn color(value: Color) -> Result<()> {
    if value.mode != photocraft_doc::ColorMode::Rgb {
        return Err(error("Affinity effect color must be RGB"));
    }
    for channel in value.c.into_iter().chain([value.alpha]) {
        unit(channel)?;
    }
    Ok(())
}
fn contour(value: &Contour) -> Result<()> {
    if matches!(value, Contour::Linear) { Ok(()) } else { Err(error("unsupported Affinity effect contour")) }
}
fn paint(value: &FxPaint) -> Result<u64> {
    match value {
        FxPaint::Color(value) => {
            color(*value)?;
            Ok(0)
        }
        FxPaint::Gradient(value) => gradient(value),
        FxPaint::Pattern { .. } => Err(error("unsupported Affinity effect pattern")),
    }
}
fn gradient(value: &Gradient) -> Result<u64> {
    if value.stops.is_empty()
        || value.stops.len() > 1024
        || value.opacity_stops.len() > 1024
        || !value.scale.is_finite()
        || !(0.001..=5000.).contains(&value.scale)
        || !value.offset.0.is_finite()
        || !value.offset.1.is_finite()
        || value.offset.0.abs() > 1_000_000.
        || value.offset.1.abs() > 1_000_000.
    {
        return Err(error("invalid Affinity effect gradient"));
    }
    angle(value.angle)?;
    for &(position, value) in &value.stops {
        unit(position)?;
        color(value)?;
    }
    for &(position, opacity) in &value.opacity_stops {
        unit(position)?;
        unit(opacity)?;
    }
    Ok((value.stops.len() * std::mem::size_of::<(f32, Color)>() + value.opacity_stops.len() * std::mem::size_of::<(f32, f32)>()) as u64)
}
