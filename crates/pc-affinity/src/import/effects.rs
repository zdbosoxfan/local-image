//! Affinity effects remain native layer styles; no baked overlay copies.
use super::{paint, scene::State};
use crate::{Error, model};
use photocraft_doc::{Affine, BlendMode, Layer, effects::*};

impl State<'_, '_> {
    pub(super) fn effects(&mut self, source: &[model::Effect], layer: &mut Layer, m: Affine, name: &str) -> Result<(), Error> {
        let scale = m.determinant().abs().sqrt();
        let length = |value: f64| (value * scale) as f32;
        let mut items = Vec::new();
        for effect in source {
            use model::Effect as E;
            let reach = match effect {
                E::DropShadow { radius, distance, .. } | E::InnerShadow { radius, distance, .. } => radius + distance,
                E::OuterGlow { radius, .. } => *radius,
                E::Outline { width, .. } => *width,
                E::Bevel { size, .. } => *size,
                E::ColorOverlay { .. } | E::GradientOverlay { .. } => 0.,
            } * scale;
            if !reach.is_finite() || reach > super::MAX_EFFECT_REACH {
                self.warn("effect-limit", name, "An effect exceeds the native 512 px reach after DPI scaling and was left out; other artwork is retained.");
                continue;
            }
            let cmyk = match effect {
                model::Effect::DropShadow { color, .. }
                | model::Effect::InnerShadow { color, .. }
                | model::Effect::OuterGlow { color, .. }
                | model::Effect::Outline { color, .. }
                | model::Effect::ColorOverlay { color, .. } => matches!(color, crate::paint::Color::Cmyk { .. }),
                model::Effect::Bevel { highlight, shadow, .. } => {
                    matches!(highlight, crate::paint::Color::Cmyk { .. }) || matches!(shadow, crate::paint::Color::Cmyk { .. })
                }
                model::Effect::GradientOverlay { .. } => false,
            };
            if cmyk {
                self.warn("colour-profile", name, "CMYK effect colours were converted without the source profile.");
            }

            let item = match effect {
                E::DropShadow { color, opacity, radius, distance, angle } | E::InnerShadow { color, opacity, radius, distance, angle } => {
                    let shadow = Shadow {
                        common: FxCommon::new(BlendMode::Normal, *opacity as f32),
                        color: paint::color(*color),
                        angle: (180. - angle.to_degrees()).rem_euclid(360.) as f32,
                        use_global_light: false,
                        distance: length(*distance),
                        spread: 0.,
                        size: length(*radius),
                        contour: Contour::Linear,
                        anti_alias: true,
                        noise: 0.,
                        knocks_out: true,
                    };
                    if matches!(effect, E::DropShadow { .. }) { Effect::DropShadow(shadow) } else { Effect::InnerShadow(shadow) }
                }
                E::OuterGlow { color, opacity, radius } => Effect::OuterGlow(Glow {
                    common: FxCommon::new(BlendMode::Normal, *opacity as f32),
                    paint: FxPaint::Color(paint::color(*color)),
                    technique: GlowTechnique::Softer,
                    spread: 0.,
                    size: length(*radius),
                    contour: Contour::Linear,
                    anti_alias: true,
                    range: 0.5,
                    jitter: 0.,
                    noise: 0.,
                    source: GlowSource::Edge,
                }),
                E::Outline { color, opacity, width, align } => Effect::Stroke(StrokeFx {
                    common: FxCommon::new(BlendMode::Normal, *opacity as f32),
                    size: length(*width),
                    position: match align {
                        1 => StrokePosition::Inside,
                        2 => StrokePosition::Center,
                        _ => StrokePosition::Outside,
                    },
                    paint: FxPaint::Color(paint::color(*color)),
                }),
                E::ColorOverlay { color, opacity, blend } => {
                    Effect::ColorOverlay { common: FxCommon::new(super::scene::blend(*blend, self, name), *opacity as f32), color: paint::color(*color) }
                }
                E::GradientOverlay { gradient, opacity, blend } => {
                    let b = super::scene::bounds(layer);
                    let bounds = (f64::from(b.x0), f64::from(b.y0), f64::from(b.x1), f64::from(b.y1));
                    let (gradient, _) = paint::gradient(self, gradient, m, bounds, name)?;
                    Effect::GradientOverlay { common: FxCommon::new(super::scene::blend(*blend, self, name), *opacity as f32), gradient, dither: false }
                }
                E::Bevel { size, depth, angle, highlight, highlight_opacity, shadow, shadow_opacity, inner } => {
                    self.warn("bevel-approximation", name, "Bevel imported as an editable native approximation; compare it with the original.");
                    Effect::BevelEmboss(Bevel {
                        enabled: true,
                        style: if *inner { BevelStyle::InnerBevel } else { BevelStyle::OuterBevel },
                        technique: BevelTechnique::Smooth,
                        depth: (*depth / 5.).clamp(0., 10.) as f32,
                        up: true,
                        size: length(*size),
                        soften: 0.,
                        angle: angle.to_degrees().rem_euclid(360.) as f32,
                        altitude: 30.,
                        use_global_light: false,
                        gloss_contour: Contour::Linear,
                        highlight: FxCommon::new(BlendMode::Screen, *highlight_opacity as f32),
                        highlight_color: paint::color(*highlight),
                        shadow: FxCommon::new(BlendMode::Multiply, *shadow_opacity as f32),
                        shadow_color: paint::color(*shadow),
                        contour: None,
                        texture: None,
                    })
                }
            };
            items.push(item);
        }
        if !items.is_empty() {
            if m.m[1].abs() > 1e-6 || m.m[2].abs() > 1e-6 || (m.m[0].abs() - m.m[3].abs()).abs() > 1e-6 {
                self.warn("effect-transform", name, "Effect lengths use the geometric mean of a nonuniform placement scale.");
            }
            if layer.effects.items.len().saturating_add(items.len()) > 64 {
                return Err(Error::Limit("combined native effects"));
            }
            layer.effects.enabled = true;
            layer.effects.items.extend(items);
        }
        Ok(())
    }
}
