//! Generate settings widgets: model choice and grouping, sampling and size rows, chips, readiness
//! notes and the model picker.
use super::*;

pub(super) fn model_of(s: &GenerateState) -> ModelId {
    ModelId::from_key(&s.model).filter(|m| m.try_info().is_some()).unwrap_or(ModelId::Klein4B)
}

/// Keeps sampling inside what the connected ComfyUI accepts (its sampler's reported limits); with no
/// engine connected only "at least one step" applies.
pub(super) fn fit_to_backend(s: &mut GenerateState, limits: Option<li_ai::comfy::SamplerLimits>) {
    let info = model_of(s).info();
    s.steps = s.steps.max(1);
    if !s.guidance.is_finite() {
        s.guidance = info.guidance.default;
    }
    if !s.denoise.is_finite() {
        s.denoise = if s.mode == Mode::Create { info.resolved.sampling.denoise } else { s.mode.default_strength(info.id) };
    }
    if let Some(l) = limits {
        s.steps = s.steps.clamp(l.steps.0.max(1.0) as u32, l.steps.1.max(l.steps.0).max(1.0) as u32);
        s.guidance = s.guidance.clamp(l.cfg.0, l.cfg.1.max(l.cfg.0));
        s.denoise = s.denoise.clamp(l.denoise.0, l.denoise.1.max(l.denoise.0));
    }
}

/// "Recommended: 4–8 (default 6)" or "Recommended: 8" for a model's suggested range.
pub(super) fn range_hint(min: f32, max: f32, default: f32, decimals: usize) -> String {
    let f = |v: f32| format!("{v:.decimals$}");
    if min == max {
        li_i18n::fmt(tl!("Recommended: {value}"), &[("value", &f(default))])
    } else {
        li_i18n::fmt(tl!("Recommended: {min}–{max} (default {default})"), &[("min", &f(min)), ("max", &f(max)), ("default", &f(default))])
    }
}

// The shared number widget reports numeric changes, while provenance also needs text
// entry of the same number (including a value equal to the model's recommendation).
pub(super) fn number_edited(response: &egui::Response) -> bool {
    response.changed()
        || (response.has_focus() && response.ctx.input(|i| i.events.iter().any(|event| matches!(event, egui::Event::Text(_) | egui::Event::Paste(_)))))
}

/// Number fields use the Properties value field; model recommendations never become slider ranges.
pub(super) fn number_row(ui: &mut egui::Ui, label: &str, v: &mut u32, lo: u32, hi: u32, hint: Option<String>) -> bool {
    *v = (*v).clamp(lo, hi.max(lo));
    number_edited(&value_row(ui, label, v, lo..=hi.max(lo), hint, 0))
}

pub(super) fn value_row<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut T,
    range: std::ops::RangeInclusive<T>,
    hint: Option<String>,
    decimals: usize,
) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let response = ui
        .horizontal(|ui| {
            let label = ui.label(RichText::new(label).color(t.text_dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::value_field_with_precision(ui, v, range, "", 92.0, decimals).labelled_by(label.id)
            })
            .inner
        })
        .inner;
    if let Some(h) = hint {
        ui.label(RichText::new(h).font(crate::theme::medium(11.0)).color(t.text_faint));
    }
    response
}

pub(super) fn guidance_row(ui: &mut egui::Ui, v: &mut f32, lo: f32, hi: f32, hint: Option<String>) -> bool {
    guidance_row_labeled(ui, tl!("Guidance"), v, lo, hi, hint)
}

pub(super) fn denoise_row(ui: &mut egui::Ui, label: &str, v: &mut f32, lo: f32, hi: f32) -> bool {
    number_edited(
        &value_row(ui, label, v, lo..=hi.max(lo), None, 2).on_hover_text(tl!("ComfyUI denoise: 1 regenerates the image, lower values keep more of it")),
    )
}

pub(super) fn guidance_row_labeled(ui: &mut egui::Ui, label: &str, v: &mut f32, lo: f32, hi: f32, hint: Option<String>) -> bool {
    number_edited(&value_row(ui, label, v, lo..=hi.max(lo), hint, 1))
}

pub(super) fn sampling_ui(ui: &mut egui::Ui, s: &mut GenerateState, limits: Option<li_ai::comfy::SamplerLimits>, denoise: bool) {
    fit_to_backend(s, limits);
    let info = model_of(s).info();
    let (slo, shi) = limits.map_or((1.0, u32::MAX as f32), |l| l.steps);
    let steps_changed = number_row(
        ui,
        tl!("Steps"),
        &mut s.steps,
        slo.max(1.0) as u32,
        shi.max(1.0) as u32,
        Some(range_hint(info.steps.min, info.steps.max, info.steps.default, 0)),
    );
    let (clo, chi) = limits.map_or((f32::MIN, f32::MAX), |l| l.cfg);
    let guidance_changed = guidance_row(ui, &mut s.guidance, clo, chi, Some(range_hint(info.guidance.min, info.guidance.max, info.guidance.default, 1)));
    if steps_changed {
        s.refine_defaults[0] = false;
    }
    if guidance_changed {
        s.refine_defaults[1] = false;
    }
    if info.id == ModelId::ZImageTurbo && s.mode == Mode::Create {
        if !s.variety.is_finite() {
            s.variety = 0.0;
        }
        s.variety = s.variety.clamp(0.0, 100.0);
        ui.horizontal(|ui| {
            label(ui, tl!("Variety"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::value_field(ui, &mut s.variety, 0.0..=100.0, "%", 74.0);
                if widgets::secondary_button(ui, tl!("Off"), 0.0).clicked() {
                    s.variety = 0.0;
                }
            });
        });
        let r = widgets::slider(ui, &mut s.variety, 0.0..=100.0, None);
        r.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), s.variety as f64, tl!("Variety")));
        r.on_hover_text(tl!("Higher Variety can change compositions and loosen prompt details. Set to 0 to turn it off."));
    }
    if denoise {
        let (lo, hi) = limits.map_or((f32::MIN, f32::MAX), |l| l.denoise);
        if denoise_row(ui, tl!("Denoise"), &mut s.denoise, lo, hi) {
            s.refine_defaults[2] = false;
        }
    }
}

/// The Generate size component used by both the panel and Draft / Refinement.
/// Presets and edited dimensions use the selected model's pixel grid; merely opening an image
/// never changes its dimensions.
pub(super) fn size_controls(ui: &mut egui::Ui, s: &mut GenerateState) {
    let t = Tokens::get(ui.ctx());
    let sizes = model_of(s).info().resolved.sizes;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(crate::theme::ROW_GAP, crate::theme::ROW_GAP);
        for (aspect, _) in ASPECTS {
            if widgets::pill_tab(ui, aspect, s.aspect == aspect).clicked() {
                s.aspect = aspect.into();
                s.aspect_locked = true;
                (s.width, s.height) = size_for(aspect, s, sizes.native);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = crate::theme::ROW_GAP;
        let (mut w, mut h) = (s.width, s.height);
        let lo = if s.mode == Mode::Refine { 1 } else { sizes.min };
        let hi = sizes.max.max(w).max(h);
        let fw = ((ui.available_width() - 70.0) / 2.0).clamp(56.0, 112.0);
        ui.label(RichText::new("W").font(crate::theme::medium(11.0)).color(t.text_dim));
        let width = widgets::value_field_with_precision(ui, &mut w, lo..=hi, "px", fw, 0).on_hover_text(tl!("Width"));
        width.widget_info(|| egui::WidgetInfo { label: Some(tl!("Width").into()), ..egui::WidgetInfo::drag_value(ui.is_enabled(), w as f64) });
        let lock = crate::icons::button(ui, if s.aspect_locked { "link" } else { "unlink" }, 22.0, s.aspect_locked, tl!("Link width and height"));
        lock.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), s.aspect_locked, tl!("Link width and height")));
        if lock.clicked() {
            s.aspect_locked = !s.aspect_locked;
        }
        ui.label(RichText::new("H").font(crate::theme::medium(11.0)).color(t.text_dim));
        let height = widgets::value_field_with_precision(ui, &mut h, lo..=hi, "px", fw, 0).on_hover_text(tl!("Height"));
        height.widget_info(|| egui::WidgetInfo { label: Some(tl!("Height").into()), ..egui::WidgetInfo::drag_value(ui.is_enabled(), h as f64) });
        if width.changed() || height.changed() {
            if s.aspect_locked {
                let ratio = ASPECTS.iter().find(|(k, _)| *k == s.aspect).map_or(s.width as f32 / s.height.max(1) as f32, |(_, r)| *r);
                if width.changed() {
                    h = (w as f32 / ratio).round().max(1.0) as u32;
                } else {
                    w = (h as f32 * ratio).round().max(1.0) as u32;
                }
            } else {
                s.aspect = "custom".into();
            }
            (s.width, s.height) = (w, h);
        }
        if ((width.changed() || height.changed()) && !(width.has_focus() || height.has_focus())) || width.lost_focus() || height.lost_focus() {
            (s.width, s.height) = li_ai::imaging::snap_size(s.width, s.height, sizes.multiple.max(8));
        }
    });
    ui.label(
        RichText::new(li_i18n::fmt(tl!("Grid: {multiple} px"), &[("multiple", &sizes.multiple.max(8).to_string())]))
            .font(crate::theme::medium(11.0))
            .color(t.text_faint),
    );
}

pub(super) fn variant_picker(ui: &mut egui::Ui, model: ModelId, variant: &mut String) {
    let presets: Vec<_> = catalog::presets_for(model).collect();
    if presets.len() > 1 {
        ui.horizontal(|ui| {
            label(ui, tl!("Precision"));
            let opts: Vec<_> = presets.iter().map(|p| (p.variant.clone(), p.label.as_str())).collect();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::dropdown(ui, "gen-variant", variant, &opts, ui.available_width().min(240.0));
            });
        });
    }
}

/// Apply a newly verified recipe's recommendations once, preserving values the user edited
/// while the checker ran. This matters when fallback changes full-reference denoise to img2img.
pub(super) fn inherit_refine_defaults(id: &'static str, s: &mut GenerateState) {
    let model = model_of(s);
    let Some(defaults) = li_ai::refine::model_defaults(model) else { return };
    let changed = rt(|r| {
        let value = (model, s.variant.clone(), defaults.clone());
        if r.learned_refine.get(id) == Some(&value) {
            return false;
        }
        r.learned_refine.insert(id, value);
        true
    });
    if !changed {
        return;
    }
    if s.refine_defaults[0] {
        s.steps = defaults.steps;
    }
    if s.refine_defaults[1] {
        s.guidance = defaults.cfg;
    }
    if s.refine_defaults[2] {
        s.denoise = defaults.denoise;
    }
}

pub(super) fn set_model(s: &mut GenerateState, m: ModelId) {
    let info = m.info();
    s.model = m.key().into();
    s.variant = catalog::default_variant(m).into();
    s.steps = info.steps.default as u32;
    s.refine_defaults = [true; 3];
    s.variety = 0.0;
    s.guidance = info.guidance.default;
    if s.mode == Mode::Refine
        && let Some(d) = li_ai::refine::model_defaults(m)
    {
        s.steps = d.steps;
        s.guidance = d.cfg;
    }
    s.denoise = if s.mode == Mode::Create { info.resolved.sampling.denoise } else { s.mode.default_strength(m) };
    s.sampler.clear();
    s.scheduler.clear();
    if !info.transparent {
        s.transparent = false;
    }
    // LoRAs that don't fit the new family are dropped.
    let fam = info.resolved.clone();
    let fits: Vec<String> = li_ai::inventory::loras_for(&fam).into_iter().map(|(l, _)| l.name).collect();
    s.loras.retain(|l| fits.contains(&l.name));
    if s.mode == Mode::Create && !info.text_to_image {
        s.mode = Mode::Refine;
        s.denoise = s.mode.default_strength(m);
    }
}

/// Whether a model can run `mode` (Upscale: the official upscalers, or any image model once
/// "Choose another model…" was picked this session).
pub(super) fn fits_mode(info: &catalog::ModelInfo, mode: Mode) -> bool {
    fits_mode_with(info, mode, upscale_any())
}

/// [`fits_mode`]; `any_upscaler`: Upscale lists every image model (diffusion refine-upscale)
/// instead of the official upscalers (SeedVR2).
pub(super) fn fits_mode_with(info: &catalog::ModelInfo, mode: Mode, any_upscaler: bool) -> bool {
    if mode == Mode::Upscale && !any_upscaler {
        return is_upscaler(info);
    }
    if info.tool || info.resolved.kind != li_ai::family::FamilyKind::Image {
        return false;
    }
    match mode {
        Mode::Create => info.text_to_image,
        Mode::Fill => info.inpaint,
        Mode::Refine => !info.upscale && (info.text_to_image || info.edit || info.family == "unknown"),
        Mode::Edit | Mode::Upscale => true,
    }
}

/// A supported upscaler: runs through the shared Enhance service.
pub(super) fn is_upscaler(info: &catalog::ModelInfo) -> bool {
    info.upscale
}

/// Upscale with a supported restoration model goes to `ai.enhance`, not a diffusion
/// refine-upscale.
pub(super) fn runs_as_enhance(info: &catalog::ModelInfo, mode: Mode) -> bool {
    mode == Mode::Upscale && is_upscaler(info)
}

/// "Choose another model…" in Upscale mode (this session only).
pub(super) fn upscale_any() -> bool {
    rt(|r| r.upscale_any)
}

pub(super) fn restore_upscale_choice(s: &GenerateState) {
    if s.mode == Mode::Upscale
        && !upscale_any()
        && (s.model.starts_with("custom:")
            || s.model.starts_with("cloud:")
            || ModelId::from_key(&s.model).and_then(|m| m.try_info()).is_some_and(|info| !is_upscaler(info)))
    {
        rt(|r| r.upscale_any = true);
    }
}

pub(super) fn size_for(aspect: &str, s: &GenerateState, native: u32) -> (u32, u32) {
    let Some((_, r)) = ASPECTS.iter().find(|(k, _)| *k == aspect) else { return (s.width, s.height) };
    // About the model's native pixel count, snapped to its pixel grid.
    let area = native as f32 * native as f32;
    let h = (area / r).sqrt();
    li_ai::imaging::snap_size((h * r) as u32, h as u32, model_of(s).info().resolved.sizes.multiple.max(8))
}

pub(super) fn thumb_texture(ctx: &egui::Context, key: &str, path: &std::path::Path) -> Option<TextureHandle> {
    rt(|r| r.gallery_thumbs.get(ctx, key, path))
}

pub(super) fn image_texture(ctx: &egui::Context, key: &str, img: &RgbaImage) -> TextureHandle {
    let t = image::imageops::thumbnail(img, 160.min(img.width()).max(1), (160 * img.height() / img.width().max(1)).clamp(1, 160));
    let ci = egui::ColorImage::from_rgba_unmultiplied([t.width() as usize, t.height() as usize], t.as_raw());
    ctx.load_texture(format!("ref-{key}"), ci, egui::TextureOptions::LINEAR)
}

pub(super) fn label(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(s).color(t.text_dim));
}

/// A row of chip buttons; returns the clicked index.
pub(super) fn chips(ui: &mut egui::Ui, items: &[&str], selected: usize) -> Option<usize> {
    chips_with(ui, items, selected, &[], &[])
}

/// Chips with tooltips; `disabled[i]` greys a chip out (with its reason as tooltip).
pub(super) fn chips_with(ui: &mut egui::Ui, items: &[&str], selected: usize, tips: &[&str], disabled: &[Option<&str>]) -> Option<usize> {
    let t = Tokens::get(ui.ctx());
    let mut hit = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (i, s) in items.iter().enumerate() {
            let on = i == selected;
            let off = disabled.get(i).copied().flatten();
            let ink = if on {
                t.accent_text
            } else if off.is_some() {
                t.text_faint
            } else {
                t.text_dim
            };
            let galley = ui.painter().layout_no_wrap((*s).to_owned(), crate::theme::medium(12.0), ink);
            let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 16.0, 22.0), Sense::click());
            let fill = if on {
                t.accent
            } else if resp.hovered() && off.is_none() {
                t.hover
            } else {
                t.field
            };
            ui.painter().rect_filled(r, 11.0, fill);
            ui.painter().galley(r.center() - galley.size() / 2.0, galley, t.text);
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, off.is_none(), *s));
            let resp = match (off, tips.get(i)) {
                (Some(why), _) => resp.on_hover_text(why),
                (None, Some(tip)) => resp.on_hover_text(*tip),
                _ => resp,
            };
            if resp.clicked() && off.is_none() {
                hit = Some(i);
            }
        }
    });
    hit
}

/// A small capability tag.
pub(super) fn tag(ui: &mut egui::Ui, text: &str, t: &Tokens) {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(10.5), t.text_dim);
    let (r, _) = ui.allocate_exact_size(vec2(galley.size().x + 10.0, 16.0), Sense::hover());
    ui.painter().rect_filled(r, 8.0, t.field);
    ui.painter().galley(r.center() - galley.size() / 2.0, galley, t.text_dim);
}

pub(super) fn model_variety_tip(model: ModelId) -> Option<&'static str> {
    (model == ModelId::ZImageTurbo)
        .then(|| tl!("Low variety by design: different seeds give similar layouts. Change the prompt (or raise Variety) for different compositions."))
}

/// Shared by Generate and Draft, scoped to the Turbo model rather than the Z-Image family.
pub(super) fn model_variety_note(ui: &mut egui::Ui, model: ModelId) {
    if let Some(tip) = model_variety_tip(model) {
        let t = Tokens::get(ui.ctx());
        ui.horizontal_wrapped(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            crate::icons::paint(ui, r, "info", 12.0, t.text_faint);
            ui.add(egui::Label::new(RichText::new(tip).color(t.text_faint).size(11.0)).wrap());
        });
    }
}

/// Readiness of a model on the running engine: `Ok` ready, `Err(reason)` otherwise.
pub(super) fn readiness(st: &crate::ai_ui::EngineStatus, m: ModelId, variant: &str) -> Result<(), String> {
    st.ready(m, variant)
}

pub(super) fn refine_readiness(st: &crate::ai_ui::EngineStatus, m: ModelId, variant: &str) -> Result<(), String> {
    readiness(st, m, variant)?;
    match li_ai::refine::checker().status_for_variant(&st.host, m, variant) {
        li_ai::refine::Status::Failed(reason) => Err(format!("Refine not supported ({reason})")),
        li_ai::refine::Status::Testing => Err("testing…".into()),
        _ => Ok(()),
    }
}

/// Shared readiness hint and the action that resolves it in either Generate surface.
pub(super) fn readiness_warning(app: &mut PhotocraftApp, ui: &mut egui::Ui, why: &str, st: &crate::ai_ui::EngineStatus, cloud: bool) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(why).font(crate::theme::medium(11.5)).color(t.warning));
        if cloud {
            if widgets::secondary_button(ui, tl!("Add Key…"), 0.0).clicked() {
                crate::ai_ui::open_local_ai(app);
            }
        } else if !st.connected {
            if widgets::secondary_button(ui, tl!("Set up AI…"), 0.0).clicked() {
                crate::ai_ui::open_local_ai(app);
            }
        } else if widgets::secondary_button(ui, tl!("Get models…"), 0.0).clicked() {
            crate::model_browser::open();
        }
    });
}

/// The model picker: models grouped by family group, each with a readiness dot and its capability
/// tags; custom workflows last; Browse Models… and Import Workflow… at the bottom. Returns the
/// chosen key.
pub(super) fn model_groups(models: &[catalog::ModelInfo], mode: Mode) -> Vec<(&str, Vec<&catalog::ModelInfo>)> {
    if mode == Mode::Refine {
        let mut groups = vec![(tl!("Edit models — refine by reference"), Vec::new()), (tl!("Image models — refine by denoise"), Vec::new())];
        for m in models.iter().filter(|m| fits_mode(m, mode)) {
            let group = match catalog::refine_method(m.id) {
                catalog::RefineMethod::Reference => 0,
                catalog::RefineMethod::Img2Img => 1,
            };
            groups[group].1.push(m);
        }
        groups.retain(|(_, models)| !models.is_empty());
        groups
    } else {
        let mut groups: Vec<(&str, Vec<&catalog::ModelInfo>)> = Vec::new();
        for m in models.iter().filter(|m| fits_mode(m, mode)) {
            match groups.iter_mut().find(|(g, _)| *g == m.group) {
                Some((_, v)) => v.push(m),
                None => groups.push((m.group.as_str(), vec![m])),
            }
        }
        groups
    }
}

pub(super) fn model_picker(
    ui: &mut egui::Ui,
    id: &str,
    current: (&str, &str),
    mode: Mode,
    st: &crate::ai_ui::EngineStatus,
    allow_custom: bool,
) -> Option<String> {
    let (current, selected_variant) = current;
    let t = Tokens::get(ui.ctx());
    let custom = current.strip_prefix("custom:");
    let current_label = match custom {
        Some(name) => li_i18n::fmt(tl!("{name} · Custom workflow"), &[("name", name)]),
        None => ModelId::from_key(current)
            .and_then(|m| m.try_info())
            .map(|i| format!("{} · {}", i.label, i.family_label))
            .unwrap_or_else(|| tl!("Choose a model").into()),
    };
    let mut picked = None;
    let width = ui.available_width();
    widgets::dropdown_menu(ui, id, &current_label, width, |ui| {
        ui.set_min_width(width.max(300.0));
        let cat = catalog::catalog();
        for (g, models) in model_groups(&cat.models, mode) {
            ui.add_space(4.0);
            let heading = if mode == Mode::Refine { g.to_owned() } else { g.to_uppercase() };
            ui.label(RichText::new(heading).size(10.5).color(t.text_faint).strong());
            for m in models {
                let variant = if current == m.id.key() { selected_variant } else { catalog::default_variant(m.id) };
                let ok = catalog::presets_for(m.id).any(|p| readiness(st, m.id, &p.variant).is_ok());
                let reason = readiness(st, m.id, variant).err().unwrap_or_default();
                let refine_status =
                    if mode == Mode::Refine { li_ai::refine::checker().status_for_variant(&st.host, m.id, variant) } else { li_ai::refine::Status::Unchecked };
                let failed = matches!(refine_status, li_ai::refine::Status::Failed(_));
                let ok = ok && !failed && refine_status != li_ai::refine::Status::Testing;
                let row = ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
                    let colour = if ok {
                        t.accent
                    } else if m.origin == catalog::Origin::Installed {
                        t.warning
                    } else {
                        t.text_faint
                    };
                    ui.painter().circle_filled(dot.center(), 3.5, colour);
                    let sel = current == m.id.key();
                    let resp = ui.add_enabled(
                        !failed,
                        egui::Button::selectable(sel, RichText::new(&m.label).color(if ok || !st.connected { t.text } else { t.text_dim })),
                    );
                    if mode == Mode::Refine && refine_status != li_ai::refine::Status::Unchecked {
                        if refine_status == li_ai::refine::Status::Testing {
                            if ui.small_button(tl!("Cancel test")).clicked() {
                                li_ai::refine::checker().cancel_variant(&li_ai::ComfyClient::new(&st.host), m.id, variant);
                            }
                        } else if ui.small_button(tl!("Re-test")).clicked() {
                            li_ai::refine::checker().request(li_ai::ComfyClient::new(&st.host), m.id, variant.into(), true);
                        }
                    }
                    ui.label(RichText::new(&m.family_label).size(10.5).color(t.text_faint));
                    for tg in m.tags().iter().take(4) {
                        tag(ui, tg, &t);
                    }
                    resp
                });
                match &refine_status {
                    li_ai::refine::Status::Testing => {
                        ui.label(tl!("testing…"));
                    }
                    li_ai::refine::Status::Failed(reason) => {
                        ui.add(egui::Label::new(RichText::new(format!("Refine not supported ({reason})")).size(10.5).color(t.warning)).truncate())
                            .on_hover_text(reason);
                    }
                    _ => {}
                }
                let mut tip = if ok {
                    format!("{}\n{}", m.best_for, m.tags().join(" · "))
                } else if m.origin == catalog::Origin::Profile {
                    format!("{}\n{}", m.best_for, li_i18n::fmt(tl!("Not installed: {reason}"), &[("reason", &reason)]))
                } else {
                    format!("{}\n{}", m.best_for, reason)
                };
                if let Some(variety) = model_variety_tip(m.id) {
                    tip.push('\n');
                    tip.push_str(variety);
                }
                if row.inner.on_hover_text(tip).clicked() {
                    picked = Some(m.id.key().to_owned());
                }
            }
        }
        if allow_custom && mode != Mode::Refine {
            let clouds: Vec<_> = li_ai::cloud::MODELS.iter().filter(|m| cloud_fits(m, mode)).collect();
            if !clouds.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(tl!("CLOUD · YOUR API KEY")).size(10.5).color(t.text_faint).strong());
                let keys = cloud_keys();
                for m in clouds {
                    let key = format!("cloud:{}", m.key);
                    let has_key = keys.contains(&m.provider);
                    let row = ui.horizontal(|ui| {
                        let (dot, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
                        ui.painter().circle_filled(dot.center(), 3.5, if has_key { t.accent } else { t.text_faint });
                        let resp = ui.add(egui::Button::selectable(current == key, RichText::new(m.label).color(if has_key { t.text } else { t.text_dim })));
                        ui.label(RichText::new(m.provider.label()).size(10.5).color(t.text_faint));
                        for (on, tg) in [(m.create, tl!("Create")), (m.edit, tl!("Edit")), (m.fill, tl!("Fill"))] {
                            if on {
                                tag(ui, tg, &t);
                            }
                        }
                        resp
                    });
                    let tip = if has_key {
                        format!(
                            "{}\n{}",
                            m.best_for,
                            li_i18n::fmt(tl!("Runs on {provider}'s servers: your prompt and images are sent there."), &[("provider", m.provider.label())])
                        )
                    } else {
                        format!(
                            "{}\n{}",
                            m.best_for,
                            li_i18n::fmt(tl!("Add your {provider} API key in Local AI › Cloud to use it."), &[("provider", m.provider.label())])
                        )
                    };
                    if row.inner.on_hover_text(tip).clicked() {
                        picked = Some(key);
                    }
                }
            }
            let cws = customs();
            if !cws.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(tl!("CUSTOM WORKFLOWS")).size(10.5).color(t.text_faint).strong());
                for w in cws {
                    let key = format!("custom:{}", w.name);
                    let row = ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        let resp = ui.add(egui::Button::selectable(current == key, &w.name));
                        for tg in w.tags().iter().take(4) {
                            tag(ui, tg, &t);
                        }
                        resp
                    });
                    if row.inner.clicked() {
                        picked = Some(key);
                    }
                }
            }
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if widgets::secondary_button(ui, tl!("Browse Models…"), 0.0).clicked() {
                picked = Some("browse:".into());
            }
            if widgets::secondary_button(ui, tl!("Browse LoRAs…"), 0.0).on_hover_text(tl!("Find LoRAs (styles) for the chosen model's family")).clicked() {
                picked = Some("browse-loras:".into());
            }
            if allow_custom && mode != Mode::Refine && widgets::secondary_button(ui, tl!("Import Workflow…"), 0.0).clicked() {
                picked = Some("import:".into());
            }
        });
    });
    if mode == Mode::Refine
        && st.connected
        && let Some(m) = ModelId::from_key(current)
    {
        let variant = selected_variant;
        if st.ready(m, variant).is_ok() {
            #[cfg(not(test))]
            li_ai::refine::checker().request(li_ai::ComfyClient::new(&st.host), m, variant.into(), false);
        }
        if li_ai::refine::checker().status_for_variant(&st.host, m, variant) == li_ai::refine::Status::Testing {
            ui.label(tl!("testing…"));
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }
    }
    if mode == Mode::Refine
        && let Some(m) = picked.as_deref().and_then(ModelId::from_key).or_else(|| ModelId::from_key(current))
    {
        let hint = match catalog::refine_method(m) {
            catalog::RefineMethod::Reference => tl!("Refines by reference: the image guides the result"),
            catalog::RefineMethod::Img2Img => tl!("Refines by denoise: lower keeps more of the image"),
        };
        ui.label(RichText::new(hint).size(11.0).color(t.text_faint));
    }
    picked
}

/// The Model Browser on LoRAs for `model`'s family (or on LoRAs in general).
pub(super) fn browse_loras(model: &str) {
    match ModelId::from_key(model).and_then(|m| m.try_info()) {
        Some(info) => crate::model_browser::open_loras(&info.family),
        None => crate::model_browser::open_loras_any(),
    }
}
