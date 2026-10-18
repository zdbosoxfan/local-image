//! The Layers panel: rows, locks, thumbnails, effects sub-rows and drag-and-drop reordering.
use super::*;

// Layers

/// The simple themes' single lock button. On the Background it converts the layer to a normal
/// one, as clicking the Background's lock does in Photoshop. Elsewhere it clears every lock when
/// any is set, and locks all otherwise. Toggling only "lock all" would leave the Background's
/// transparency lock (still showing as locked), and a second click would lock its pixels,
/// shutting out the Eraser and every other paint tool (#76).
/// The active layer's state decides, and every target follows it (`targets`: `{"layer": id}` or
/// `{"layers": [ids]}`).
pub(super) fn simple_lock_toggle(background: bool, l: &Layer, targets: &Value) -> (String, Value) {
    if background {
        return ("layer.new.layerFromBackground".into(), json!({}));
    }
    let k = &l.locks;
    let locked = k.transparency || k.pixels || k.position || k.artboard || k.all;
    let locks = if locked { json!({"transparency": false, "pixels": false, "position": false, "artboard": false, "all": false}) } else { json!({"all": true}) };
    let mut p = targets.clone();
    p["locks"] = locks;
    ("layer.setProps".into(), p)
}

/// The `layer.setProps` edit of the Opacity or Fill (`key`) field at `pct` percent.
fn pct_action(targets: &Value, key: &str, pct: f32) -> (String, Value) {
    let mut p = targets.clone();
    p[key] = json!(pct / 100.0);
    ("layer.setProps".into(), p)
}

pub(super) fn blend_options(groups: bool) -> Vec<(BlendMode, &'static str)> {
    std::iter::once(BlendMode::PassThrough).filter(|_| groups).chain(BlendMode::LAYER_MODES).map(|m| (m, m.label())).collect()
}

pub(super) fn layers(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    // A new active layer opens its parent groups and is scrolled into view (#152).
    let reveal = crate::layer_reveal::track(app, ui.ctx());
    let Some(st) = app.session.active() else {
        empty(ui, tl!("No document"));
        return;
    };
    let doc = st.doc.clone();
    let active = st.active_layer;
    let active_layer = active.and_then(|id| doc.layer(id));
    let selection = st.selected_layers();
    let isolated = st.isolated_layers.clone();
    let mut actions: Vec<(String, Value)> = Vec::new();

    let t = Tokens::get(ui.ctx());
    if t.pro {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let (r, _) = ui.allocate_exact_size(vec2(64.0, 22.0), Sense::hover());
            ui.painter().rect_filled(r, t.radius_sm, t.field);
            ui.painter().rect_stroke(r, t.radius_sm, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
            icons::paint(ui, Rect::from_center_size(r.left_center() + vec2(11.0, 0.0), vec2(14.0, 14.0)), "search", 11.0, t.text_faint);
            ui.painter().text(r.left_center() + vec2(22.0, 0.0), Align2::LEFT_CENTER, tl!("Kind"), egui::FontId::proportional(11.5), t.text_dim);
            ui.add_space(4.0);
            for (kind, icon, tip) in [
                ("pixel", "image", tl!("Filter for pixel layers")),
                ("adjustment", "contrast", tl!("Filter for adjustment layers")),
                ("type", "type", tl!("Filter for type layers")),
                ("shape", "square", tl!("Filter for shape layers")),
                ("symbol", "copy", tl!("Filter for symbols")),
                ("smart", "app-window", tl!("Filter for smart objects")),
            ] {
                let on = app.ui.layer_filter.iter().any(|k| k == kind);
                if icons::button(ui, icon, 22.0, on, tip).clicked() {
                    if on {
                        app.ui.layer_filter.retain(|k| k != kind);
                    } else {
                        app.ui.layer_filter.push(kind.to_string());
                    }
                }
            }
        });
        ui.add_space(4.0);
    }
    if let Some(l) = active_layer {
        // With several layers selected, blend mode, Opacity, Fill and the locks change every one
        // of them, as in Photoshop; the controls show the active layer's values. A selected
        // Background stays out of a several-layer change: Opacity, Fill or a blend mode would
        // turn it into Layer 0.
        let ids: Vec<LayerId> = if selection.len() > 1 && selection.contains(&l.id) {
            selection.iter().copied().filter(|id| *id == l.id || !doc.layer(*id).is_some_and(|x| crate::doc_props_ui::is_background(&doc, x))).collect()
        } else {
            vec![l.id]
        };
        let targets = if ids.len() > 1 { json!({"layers": ids.iter().map(|id| id.0).collect::<Vec<_>>()}) } else { json!({"layer": l.id.0}) };
        // Photoshop greys blend mode, Opacity and Fill for the Background layer.
        let bg = crate::doc_props_ui::is_background(&doc, l);
        // Pass Through is a group's mode: offered only when every target is a group.
        let all_groups = ids.iter().all(|id| doc.layer(*id).is_some_and(Layer::is_group));
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!bg, |ui| {
                let mut m = l.blend;
                // Leave room for the Opacity label and field: a translated label ("Непрозрачность:")
                // can be much wider than the English one, and must not slide under the dropdown.
                let opacity_label = if t.pro { tl!("Opacity:") } else { tl!("Opacity") };
                let right = (body_text_width(ui, opacity_label) + 66.0 + 2.0 * ui.spacing().item_spacing.x + 16.0).max(150.0);
                let w = ui.available_width() - right;
                if widgets::dropdown(ui, "blend", &mut m, &blend_options(all_groups), w.max(100.0)) {
                    let mut p = targets.clone();
                    p["blend"] = json!(m.label());
                    actions.push(("layer.setProps".into(), p));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut o = l.opacity * 100.0;
                    if widgets::value_field(ui, &mut o, 0.0..=100.0, "%", 66.0).changed() {
                        actions.push(pct_action(&targets, "opacity", o));
                    }
                    label(ui, opacity_label);
                });
            });
        });
        // Docks zero the item spacing: keep the Opacity and Fill fields apart (#155).
        ui.add_space((theme::ROW_GAP - ui.spacing().item_spacing.y).max(0.0));
        ui.horizontal(|ui| {
            let lock_label = if t.pro { tl!("Lock:") } else { tl!("Lock") };
            let fill_label = if t.pro { tl!("Fill:") } else { tl!("Fill") };
            // In a narrow panel with long translated labels, drop the "Lock:" text (the icons keep
            // their tooltips) rather than let the Fill label run over the lock icons.
            let gap = ui.spacing().item_spacing.x;
            let icons_w = if t.pro { 5.0 * 20.0 } else { 22.0 + gap };
            let fits = body_text_width(ui, lock_label) + icons_w + body_text_width(ui, fill_label) + 66.0 + 2.0 * gap <= ui.available_width();
            if fits {
                label(ui, lock_label);
            }
            if t.pro {
                ui.spacing_mut().item_spacing.x = 0.0;
                let lk = l.locks;
                for (key, icon, tip, on) in [
                    ("transparency", "grid-3x3", tl!("Lock transparent pixels"), lk.transparency),
                    ("pixels", "brush", tl!("Lock image pixels"), lk.pixels),
                    ("position", "move", tl!("Lock position"), lk.position),
                    ("artboard", "scan", tl!("Prevent auto-nesting in and out of Artboards and Frames"), lk.artboard),
                    ("all", "lock", tl!("Lock all"), lk.all),
                ] {
                    if icons::button(ui, icon, 20.0, on, tip).clicked() {
                        let mut p = targets.clone();
                        p["locks"] = json!({ key: !on });
                        actions.push(("layer.setProps".into(), p));
                    }
                }
            }
            if !t.pro {
                let locked = l.locks.transparency || l.locks.position || l.locks.all;
                if icons::button(ui, if locked { "lock" } else { "lock-open" }, 22.0, locked, tl!("Lock layer")).clicked() {
                    actions.push(simple_lock_toggle(bg, l, &targets));
                }
            }
            ui.add_enabled_ui(!bg, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut f = l.fill_opacity * 100.0;
                    if widgets::value_field(ui, &mut f, 0.0..=100.0, "%", 66.0).changed() {
                        actions.push(pct_action(&targets, "fill", f));
                    }
                    label(ui, fill_label);
                })
            });
        });
        ui.add_space(2.0);
    }

    // Top of the stack first, groups above their contents, closed groups' contents hidden (#126).
    let rows = crate::layer_tree_ui::display_rows(&doc, !app.ui.layer_filter.is_empty());
    let ctx = ui.ctx().clone();
    let footer = 38.0;
    let fill = ui.available_height() > footer + 60.0;
    let rows_h = if fill { ui.available_height() - footer } else { f32::INFINITY };
    if app.session.prefs().toolbar.persona == photocraft_engine::toolsets::Persona::Vector {
        crate::clip_compound::layers_begin(&mut app.clip_compound_state);
    }
    egui::ScrollArea::vertical()
        .id_salt("layer-rows")
        .max_height(rows_h)
        .min_scrolled_height(if fill { rows_h } else { 0.0 })
        .auto_shrink([false, !fill])
        .show(ui, |ui| {
            let filter = app.ui.layer_filter.clone();
            let fx_collapsed = app.session.active().map(|d| d.fx_collapsed.clone()).unwrap_or_default();
            crate::layer_row_ui::begin(ui.ctx());
            for &(depth, l) in &rows {
                // Select › Isolate Layers.
                if !photocraft_engine::select_extra_cmds::isolation_shows(&doc, &isolated, l.id) {
                    continue;
                }
                if !filter.is_empty() {
                    let kind = match &l.content {
                        LayerContent::Raster(_) => "pixel",
                        LayerContent::Adjustment(_) | LayerContent::Fill(_) => "adjustment",
                        LayerContent::Text(_) => "type",
                        LayerContent::Shape(_) => "shape",
                        LayerContent::Smart(_) => "smart",
                        LayerContent::Group(_) => "",
                        LayerContent::Symbol(_) => "symbol",
                    };
                    if !filter.iter().any(|k| k == kind) {
                        continue;
                    }
                }
                let row = RowSel { selected: selection.contains(&l.id), primary: active == Some(l.id), multi: selection.len() > 1 };
                let top = ui.cursor().top();
                layer_row(app, &ctx, ui, &doc, l, depth, row, &mut actions);
                if reveal == Some(l.id) {
                    crate::layer_reveal::scroll_to_row(ui, top);
                }
                if !l.effects.items.is_empty() && fx_collapsed.iter().all(|id| *id != l.id) {
                    effect_rows(app, ui, l, depth);
                }
                crate::smart_ui::filter_rows(app, ui, l, depth, &mut actions);
            }
            // A rename whose row is gone (deleted, filtered out, inside a closed group) ends,
            // committed: nothing else could commit or cancel it.
            if let Some(layer) = crate::layer_row_ui::renaming(ui.ctx())
                && !crate::layer_row_ui::recorded(ui.ctx()).iter().any(|r| r.layer == layer)
                && let Some(done) = crate::layer_row_ui::end_rename(ui.ctx(), true)
            {
                actions.push(done);
            }
        });
    if app.session.prefs().toolbar.persona == photocraft_engine::toolsets::Persona::Vector {
        crate::clip_compound::layers_end(ui, &mut app.session, &mut app.clip_compound_state);
    }
    // A layer being dragged can also be dropped on the footer's Delete, New Layer and Group
    // buttons (#736); read the drag before it ends.
    let footer_drag = ctx.data(|d| d.get_temp::<u64>(egui::Id::new("layer-drag"))).map(|id| {
        let in_selection = selection.iter().any(|s| s.0 == id);
        (id, in_selection)
    });
    // End any layer drag after every row has had a chance to accept the drop.
    if ctx.input(|i| i.pointer.any_released()) {
        ctx.data_mut(|d| d.remove::<u64>(egui::Id::new("layer-drag")));
    }
    ui.add_space(4.0);
    widgets::hairline(ui);
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let trash = icons::button(ui, "trash", 26.0, false, tl!("Delete layer"));
            if trash.clicked() {
                actions.push(("layer.delete".into(), json!({})));
            }
            actions.extend(footer_drop(ui, &trash, footer_drag, "layer.delete"));
            let new_layer = icons::button(ui, "square-plus", 26.0, false, &crate::shortcuts::tip_label(app, "Create a new layer", "layer.new.layer"));
            if new_layer.clicked() {
                actions.push(("layer.new.layer".into(), json!({})));
            }
            actions.extend(footer_drop(ui, &new_layer, footer_drag, "layer.duplicate"));
            let group = icons::button(ui, "folder", 26.0, false, tl!("Create a new group"));
            if group.clicked() {
                actions.push(("layer.new.group".into(), json!({})));
            }
            actions.extend(footer_drop(ui, &group, footer_drag, "layer.groupLayers"));
            let adj = icons::button(ui, "contrast", 26.0, false, tl!("Create new fill or adjustment layer"));
            egui::Popup::menu(&adj).show(|ui| {
                ui.set_min_width(190.0);
                for c in photocraft_engine::command_specs().iter().filter(|c| c.id.starts_with("layer.newAdjustmentLayer.")) {
                    if ui.button(tl!(c.label).trim_end_matches('…')).clicked() {
                        actions.push((c.id.into(), json!({})));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(tl!("Solid Color…")).clicked() {
                    actions.push(("layer.newFillLayer.solidColor".into(), json!({})));
                    ui.close();
                }
                if ui.button(tl!("Gradient…")).clicked() {
                    actions.push(("layer.newFillLayer.gradient".into(), json!({})));
                    ui.close();
                }
            });
            if icons::button(
                ui,
                "square-dot",
                26.0,
                false,
                &li_i18n::fmt(tl!("Add a mask  (from the selection; {key} inverts)"), &[("key", &crate::shortcuts::pretty("Alt"))]),
            )
            .clicked()
            {
                let alt = ui.input(|i| i.modifiers.alt);
                actions.push((crate::layer_menu_ui::add_mask_command(doc.selection.is_some(), alt).into(), json!({})));
            }
            let fx = fx_button(ui, 26.0, tl!("Add a layer style"));
            egui::Popup::menu(&fx).show(|ui| {
                ui.set_min_width(180.0);
                if ui.button(tl!("Blending Options…")).clicked() {
                    crate::layer_style::open(app, None);
                    ui.close();
                }
                ui.separator();
                for &(kind, label) in crate::layer_style::KINDS {
                    if ui.button(format!("{}…", tl!(label))).clicked() {
                        crate::layer_style::open(app, Some(kind));
                        ui.close();
                    }
                }
            });
            // Photoshop's footer starts with Link Layers (enabled with two or more layers selected).
            let can_link = app.session.is_enabled("layer.linkLayers");
            if ui.add_enabled_ui(can_link, |ui| icons::button(ui, "link", 26.0, false, tl!("Link layers"))).inner.clicked() {
                actions.push(("layer.linkLayers".into(), json!({})));
            }
        });
    });
    for (id, p) in actions {
        if id == "layer.select" && app.ui.text_edit.is_some() {
            crate::type_tool::commit(app);
        }
        if id == "ui.maskTarget" {
            app.ui.mask_target = p.as_bool().unwrap_or(false);
            app.ui.vector_mask_target = false;
            continue;
        }
        if id == "ui.vectorMaskTarget" {
            app.ui.vector_mask_target = p.as_bool().unwrap_or(false);
            app.ui.mask_target = false;
            continue;
        }
        if p.is_null() {
            // Context-menu items behave like their menu-bar twins (dialogs included).
            let ctx = ui.ctx().clone();
            if let Err(e) = crate::menus::invoke(app, &ctx, &id, json!({})) {
                app.ui.status = e;
                app.ui.status_error = true;
            }
            continue;
        }
        let _ = app.run(&id, p);
    }
}

/// A layer row dragged onto a Layers panel footer button, as in Photoshop: onto Delete deletes it,
/// onto New Layer duplicates it, onto New Group groups it. A row that is part of the selection
/// carries the whole selection; any other row goes alone. Highlights the button while over it and
/// returns the command on release.
pub(super) fn footer_drop(ui: &egui::Ui, button: &egui::Response, drag: Option<(u64, bool)>, command: &str) -> Option<(String, Value)> {
    // Named for screen readers (and tests) after the command a drop runs.
    button.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, footer_label(command)));
    let (layer, in_selection) = drag?;
    let p = ui.ctx().input(|i| i.pointer.interact_pos())?;
    if !button.rect.contains(p) {
        return None;
    }
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_stroke(button.rect, t.radius_sm, Stroke::new(2.0, t.accent), StrokeKind::Inside);
    if !ui.ctx().input(|i| i.pointer.any_released()) {
        return None;
    }
    Some((command.into(), if in_selection { json!({}) } else { json!({"layer": layer}) }))
}

pub(super) fn footer_label(command: &str) -> &'static str {
    match command {
        "layer.delete" => tl!("Delete layer"),
        "layer.duplicate" => tl!("Create a new layer"),
        _ => tl!("Create a new group"),
    }
}

/// Photoshop's italic "fx" footer button (no icon-font equivalent).
pub(super) fn fx_button(ui: &mut egui::Ui, size: f32, tip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, t.radius_sm, t.hover);
    }
    let mut job = egui::text::LayoutJob::default();
    job.append("fx", 0.0, egui::TextFormat { font_id: egui::FontId::proportional(15.0), color: t.icon, italics: true, ..Default::default() });
    let g = ui.painter().layout_job(job);
    ui.painter().galley(r.center() - g.size() / 2.0, g, t.icon);
    resp.on_hover_text(tip)
}

/// How a Layers panel row is selected: in the (multi-)selection, the primary/active layer, and
/// whether several layers are selected.
#[derive(Clone, Copy)]
pub(super) struct RowSel {
    pub(super) selected: bool,
    pub(super) primary: bool,
    pub(super) multi: bool,
}

/// `layer.select` mode for a click with these modifiers (Photoshop: ⌘-click toggles, ⇧-click
/// selects a range, a plain click selects one layer).
pub(super) fn select_mode(m: egui::Modifiers) -> &'static str {
    if m.command {
        "toggle"
    } else if m.shift {
        "range"
    } else {
        "replace"
    }
}

/// A drag down the Layers panel's eye column (Photoshop): the first eye toggles and every row
/// swept over gets the same visibility, once per row, all in one history step.
#[derive(Clone)]
pub(super) struct EyeSweep {
    pub(super) visible: bool,
    /// The `coalesce` key shared by the drag's `layer.setProps` calls.
    pub(super) key: u64,
    pub(super) swept: Vec<u64>,
}

/// Starts an eye-column sweep from `eye`, or applies a running one to the row `row` of `l`.
pub(super) fn eye_sweep(ctx: &egui::Context, l: &Layer, row: Rect, eye: &egui::Response, actions: &mut Vec<(String, Value)>) {
    let id = egui::Id::new("layer-eye-sweep");
    let set =
        |visible: bool, key: u64| ("layer.setProps".to_string(), json!({"layer": l.id.0, "visible": visible, "coalesce": format!("layer-eye-sweep:{key}")}));
    if eye.drag_started() {
        let sweep = EyeSweep { visible: !l.visible, key: ctx.cumulative_pass_nr(), swept: vec![l.id.0] };
        actions.push(set(sweep.visible, sweep.key));
        ctx.data_mut(|d| d.insert_temp(id, sweep));
        return;
    }
    let Some(mut sweep) = ctx.data(|d| d.get_temp::<EyeSweep>(id)) else { return };
    let (down, pos, delta) = ctx.input(|i| (i.pointer.primary_down(), i.pointer.interact_pos(), i.pointer.delta()));
    if !down {
        ctx.data_mut(|d| d.remove::<EyeSweep>(id));
        return;
    }
    // Everything the pointer passed since the last frame, so a fast drag skips no row.
    let Some(p) = pos else { return };
    let (y0, y1) = ((p.y - delta.y).min(p.y), (p.y - delta.y).max(p.y));
    if y1 < row.top() || y0 >= row.bottom() || sweep.swept.contains(&l.id.0) {
        return;
    }
    if l.visible != sweep.visible {
        actions.push(set(sweep.visible, sweep.key));
    }
    sweep.swept.push(l.id.0);
    ctx.data_mut(|d| d.insert_temp(id, sweep));
}

#[allow(clippy::too_many_arguments)]
pub(super) fn layer_row(
    app: &mut PhotocraftApp,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    doc: &photocraft_doc::Document,
    l: &Layer,
    depth: usize,
    row: RowSel,
    actions: &mut Vec<(String, Value)>,
) {
    let selected = row.selected;
    let t = Tokens::get(ctx);
    // Photoshop's default (medium) thumbnails: 32 pt rows.
    let row_h = if t.pro { 32.0 } else { 46.0 };
    let (rect, mut resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click_and_drag());
    layer_drag_and_drop(app, ctx, ui, l, rect, &resp, actions);
    if resp.drag_started() {
        crate::layer_transfer::begin_from_panel(app, ctx, l.id);
    }
    // Rows are painted: name them for screen readers and UI tests.
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &l.name));
    // A row scrolled out of view only keeps its place (#125): a layout's hundreds of rows would
    // otherwise lay out names, icons and thumbnails every frame. Group rows stay whole: their
    // disclosure triangle is a widget (accessibility, scroll-to).
    if !ui.is_rect_visible(rect) && !l.is_group() && !resp.context_menu_opened() && crate::layer_row_ui::renaming(ctx) != Some(l.id.0) {
        return;
    }
    let painter = ui.painter_at(rect.expand(1.0));
    if t.pro {
        if selected {
            painter.rect_filled(rect, 0.0, t.row_selected);
        } else if resp.hovered() {
            painter.rect_filled(rect, 0.0, t.hover.gamma_multiply(0.5));
        }
        // eye column divider + row divider, as in Photoshop
        painter.line_segment([pos2(rect.left() + 30.0, rect.top()), pos2(rect.left() + 30.0, rect.bottom())], Stroke::new(1.0, t.separator));
        painter.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, t.separator));
    } else if selected {
        painter.rect_filled(rect, t.radius, t.hover);
        painter.rect_stroke(rect, t.radius, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
    } else if resp.hovered() {
        painter.rect_filled(rect, t.radius, t.hover.gamma_multiply(0.5));
    }
    let mut x = rect.left() + 6.0;
    let eye = Rect::from_min_size(pos2(x, rect.center().y - 11.0), vec2(22.0, 22.0));
    // The eye takes drags too (so a drag starting on it never reorders the row): dragging down
    // the eyes gives every row swept over the visibility the first eye toggled to.
    let eye_resp = ui.interact(eye, ui.id().with(("eye", l.id.0)), Sense::click_and_drag());
    // A hidden layer's eye box is left empty (still clickable).
    if l.visible {
        icons::paint(ui, eye, "eye", 15.0, t.icon);
    }
    let vector = app.session.prefs().toolbar.persona == photocraft_engine::toolsets::Persona::Vector;
    let eye_consumed = if vector {
        let mut bridge = crate::isolation::bridge(ctx);
        let result = crate::view_modes::layer_eye(ui, &eye_resp, &mut bridge.views, doc, l.id);
        crate::isolation::store_session(&mut app.session, ctx, bridge);
        match result {
            Ok(consumed) => consumed,
            Err(e) => {
                app.ui.status = e.to_string();
                app.ui.status_error = true;
                true
            }
        }
    } else {
        false
    };
    if !eye_consumed && !(vector && ui.input(|i| i.modifiers.command || i.modifiers.ctrl)) {
        eye_sweep(ctx, l, rect, &eye_resp, actions);
    }
    if !eye_consumed && eye_resp.clicked() {
        // ⌥-click shows only this layer; ⌥-click it again to restore the others.
        if ui.input(|i| i.modifiers.alt) {
            actions.push(("layer.showOnly".into(), json!({"layer": l.id.0})));
        } else {
            actions.push(("layer.setProps".into(), json!({"layer": l.id.0, "visible": !l.visible})));
        }
    }
    // Everything but the thumbnails, the indentation and the name, so a narrow panel squeezes
    // the indentation first, then the thumbnails (a layer with two masks has three).
    let others = 6.0
        + 28.0
        + if l.is_group() { crate::layer_tree_ui::TRIANGLE_W } else { 0.0 }
        + if l.clipped { 12.0 } else { 0.0 }
        + crate::layer_row_ui::reserved_width(l);
    let ts = crate::mask_thumbs_ui::thumb_size(l, if t.pro { 24.0 } else { 34.0 }, rect.width() - others);
    let fixed = others + ts + 6.0 + crate::mask_thumbs_ui::width(l, ts);
    x += 28.0 + crate::layer_row_ui::indent(depth, rect.width(), fixed);
    let toggled = crate::layer_tree_ui::disclosure(ui, rect, &mut x, l, actions);
    if l.clipped {
        painter.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, "↳", egui::FontId::proportional(13.0), t.text_faint);
        x += 12.0;
    }
    let thumb = Rect::from_min_size(pos2(x, rect.center().y - ts / 2.0), vec2(ts, ts));
    let mut isolation_consumed = false;
    if vector {
        let thumb_response = ui.interact(thumb, ui.id().with(("vector-isolation-thumb", l.id.0)), Sense::click());
        let mut bridge = crate::isolation::bridge(ctx);
        let result = crate::isolation::thumbnail(ui, &thumb_response, &mut app.session, &mut bridge.isolation, l.id);
        crate::isolation::store_session(&mut app.session, ctx, bridge);
        match result {
            Ok(consumed) => isolation_consumed = consumed,
            Err(e) => {
                app.ui.status = e.to_string();
                app.ui.status_error = true;
                isolation_consumed = true;
            }
        }
        resp = resp.union(thumb_response);
        if isolation_consumed {
            crate::vector_integration::cancel(app);
        }
    }
    draw_layer_thumb(app, ctx, ui, doc, l, thumb, row.primary);
    x += ts + 6.0;
    // Link chains and pixel / vector mask thumbnails (#153).
    let masks = crate::mask_thumbs_ui::paint(app, ctx, ui, &painter, doc, l, &mut x, rect.center().y, ts, actions);
    let mask_rect = masks.thumb(crate::mask_thumbs_ui::MaskKind::Pixel);
    let vector_rect = masks.thumb(crate::mask_thumbs_ui::MaskKind::Vector);
    // Photoshop frames the targeted thumbnail (pixels, mask or vector mask) of the active layer
    // with corner brackets.
    if row.primary {
        let target = if app.ui.vector_mask_target && vector_rect.is_some() {
            vector_rect
        } else if app.ui.mask_target {
            mask_rect.or(Some(thumb))
        } else {
            Some(thumb)
        };
        if let Some(r) = target {
            crate::mask_thumbs_ui::paint_brackets(&painter, r, t.text);
        }
    }
    // Right-hand indicators first; the name gets what is left and ends in "…" (#144).
    let fx_open = app.session.active().is_none_or(|d| !d.fx_collapsed.contains(&l.id));
    let (name_right, indicators, fx_toggled) = crate::layer_row_ui::indicators(ui, &painter, rect, x, l, fx_open, actions);
    let name_color = if l.visible { t.text } else { t.text_faint };
    let font = if selected && !t.pro { theme::medium(13.0) } else { egui::FontId::proportional(if t.pro { 12.0 } else { 13.0 }) };
    // Photoshop before 2026 set the Background layer's name in italics; 2026 sets it upright.
    let italic = !t.pro && l.name == "Background" && l.locks.transparency;
    // Photoshop rows show only the name; the kind sub-label is a Studio-theme addition.
    let is_pixel = t.pro || matches!(l.content, LayerContent::Raster(_));
    let name_rect = crate::layer_row_ui::truncated(&painter, &l.name, font, name_color, italic, name_right - x).map(|galley| {
        let text_pos = pos2(x, rect.center().y - galley.size().y / 2.0 - if is_pixel { 0.0 } else { 7.0 });
        let r = Rect::from_min_size(text_pos, galley.size());
        painter.galley(text_pos, galley, name_color);
        r
    });
    if !is_pixel {
        let sub = match &l.content {
            LayerContent::Adjustment(a) => tl!(a.label()).to_string(),
            LayerContent::Group(g) => li_i18n::trn(li_i18n::current(), g.children.len() as u64, "Group · {n} layer", "Group · {n} layers"),
            other => tl!(other.kind_name()).to_string(),
        };
        crate::layer_row_ui::label(&painter, x, rect.center().y + 8.0, name_right, &sub, egui::FontId::proportional(11.0), t.text_faint);
    }
    crate::layer_row_ui::record(ctx, crate::layer_row_ui::RowRects { layer: l.id.0, row: rect, name: name_rect, indicators });
    // ⌘-click a layer, mask or vector-mask thumbnail loads its transparency / mask / path as a
    // selection (⇧ add, ⌥ subtract, ⇧⌥ intersect) instead of changing the layer selection.
    let thumb_load = resp.clicked().then(|| (ui.input(|i| i.modifiers), resp.interact_pointer_pos())).and_then(|(m, pos)| {
        let p = pos.filter(|_| m.command)?;
        if let Some(kind) = masks.hit(p) {
            return Some(crate::mask_thumbs_ui::load_params(l, kind, m));
        }
        thumb.expand(2.0).contains(p).then(|| json!({"channel": "transparency", "layer": l.id.0, "operation": crate::channels_panel::load_operation(m)}))
    });
    // ⇧-click a mask thumbnail: disable / enable that mask; ⌥-click a layer mask: view it.
    let mask_toggle = resp.clicked().then(|| (ui.input(|i| i.modifiers), resp.interact_pointer_pos())).and_then(|(m, pos)| {
        let kind = masks.hit(pos?)?;
        crate::mask_thumbs_ui::click_command(l, kind, m)
    });
    if isolation_consumed || eye_consumed {
    } else if let Some(p) = thumb_load {
        actions.push(("select.loadSelection".into(), p));
    } else if let Some(cmd) = mask_toggle {
        actions.push(cmd);
    } else if resp.clicked() && !eye_resp.clicked() && !toggled && !fx_toggled && !masks.clicked {
        let mode = select_mode(ui.input(|i| i.modifiers));
        actions.push(("layer.select".into(), json!({"layer": l.id.0, "mode": mode})));
        // Clicking a thumbnail picks what painting targets; adjustment/fill layers target their mask.
        let pos = resp.interact_pointer_pos();
        let on_mask = mask_rect.zip(pos).is_some_and(|(r, p)| r.expand(2.0).contains(p));
        let on_thumb = pos.is_some_and(|p| thumb.expand(2.0).contains(p));
        let on_vector = pos.and_then(|p| masks.hit(p)) == Some(crate::mask_thumbs_ui::MaskKind::Vector);
        // Clicking the layer thumbnail leaves mask view (#196).
        let viewing = app.session.active().and_then(photocraft_engine::mask_view_cmds::current).is_some_and(|v| v.layer == l.id);
        if on_thumb && viewing {
            actions.push((photocraft_engine::mask_view_cmds::ID.into(), json!({"layer": l.id.0, "mode": "off"})));
        }
        let content_less = matches!(l.content, LayerContent::Adjustment(_) | LayerContent::Fill(_));
        if on_vector {
            // The vector mask thumbnail targets the vector mask: the path tools edit it and the
            // Paths panel selects the layer's path.
            app.ui.selected_path = Some("layer".into());
            actions.push(("ui.vectorMaskTarget".into(), json!(true)));
        } else if on_mask || (content_less && l.mask.is_some()) {
            actions.push(("ui.maskTarget".into(), json!(true)));
        } else if on_thumb || !row.primary {
            actions.push(("ui.maskTarget".into(), json!(false)));
        }
    }
    // Double-click: the name renames in place (over the row's full height, not just the glyphs:
    // #651); the Background, which can't be renamed while it's locked, becomes a normal layer; an
    // adjustment or fill thumbnail opens its settings and a Smart Object thumbnail its contents,
    // and a type thumbnail edits its text; anywhere else on the row opens Layer Style (#350, #537).
    // The first click already made this the active layer.
    if !isolation_consumed && resp.double_clicked() {
        let pos = resp.interact_pointer_pos();
        let on = |r: Rect| pos.is_some_and(|p| r.expand(2.0).contains(p));
        if crate::doc_props_ui::is_background(doc, l) {
            actions.push(("layer.new.layerFromBackground".into(), json!({})));
        } else if name_rect.is_some_and(|n| on(Rect::from_x_y_ranges(n.x_range(), rect.y_range()))) {
            if let Some(done) = crate::layer_row_ui::start_rename(ctx, l.id.0, &l.name) {
                // One rename at a time: starting this one commits any other (#314).
                actions.push(done);
            }
        } else if matches!(l.content, LayerContent::Shape(_)) && on(thumb) {
            // local-image: a shape thumbnail opens its fill colour in Properties, as in Photoshop.
            crate::vector_ui::open_fill_picker(ctx, l.id);
            crate::dock::reveal(app, crate::dock::Group::Properties);
            app.ui.dock_tabs.properties = 0;
        } else if pos.and_then(|p| masks.hit(p)).is_none() {
            let id = match &l.content {
                LayerContent::Adjustment(_) | LayerContent::Fill(_) if on(thumb) => "layer.layerContentOptions",
                // local-image: a Develop layer goes back to Develop.
                LayerContent::Smart(_) if on(thumb) && crate::develop_layer::is_develop_layer(l) => crate::develop_layer::DEVELOP_ID,
                LayerContent::Smart(_) if on(thumb) => "layer.smartObjects.editContents",
                LayerContent::Text(_) if on(thumb) => "type.editText",
                _ => "layer.layerStyle.blendingOptions",
            };
            if crate::menus::is_enabled(app, id) {
                // Null params: run like the menu item, dialog included.
                actions.push((id.into(), Value::Null));
            }
        }
    }
    let edit_rect = Rect::from_min_max(pos2(x - 3.0, rect.center().y - 11.0), pos2(name_right.max(x + 40.0), rect.center().y + 11.0));
    if let Some(done) = crate::layer_row_ui::rename_field(ui, l.id.0, edit_rect) {
        actions.push(done);
    }
    // Right-click context menu.
    resp.context_menu(|ui| {
        // Right-clicking inside a multi-selection keeps it and acts on every selected layer.
        let on_set = row.multi && selected;
        if crate::layer_menu_ui::show(app, ui, l, on_set, actions)
            && let Some(done) = crate::layer_row_ui::start_rename(ui.ctx(), l.id.0, &l.name)
        {
            actions.push(done);
        }
    });
}

pub(super) fn draw_layer_thumb(
    app: &mut PhotocraftApp,
    ctx: &egui::Context,
    ui: &egui::Ui,
    doc: &photocraft_doc::Document,
    l: &Layer,
    rect: Rect,
    selected: bool,
) {
    let t = Tokens::get(ctx);
    let p = ui.painter();
    match &l.content {
        LayerContent::Adjustment(_) | LayerContent::Group(_) => {
            p.rect_filled(rect, 6.0, t.field);
            let icon = if l.is_group() { "folder" } else { "contrast" };
            icons::paint(ui, rect, icon, 18.0, t.icon);
        }
        LayerContent::Text(_) => {
            // Photoshop: type layers show a "T" tile rather than a pixel thumbnail.
            p.rect_filled(rect, if t.pro { 0.0 } else { 6.0 }, Color32::from_gray(if t.pro { 222 } else { 236 }));
            icons::paint(ui, rect, "type", 20.0, Color32::from_gray(40));
        }
        // Gradient fills show the gradient itself (Photoshop); solid ones their colour.
        LayerContent::Fill(f) if crate::gradient_ui::paint_thumbnail(ui, l.id, f, rect) => {}
        LayerContent::Fill(f) => {
            let c = match f {
                photocraft_doc::Fill::Solid(c) => c.to_rgba8(),
                _ => [128, 128, 128, 255],
            };
            p.rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]));
        }
        _ => {
            // Keep row layout and outside thumbnail decorations when the image is clipped.
            if ui.is_rect_visible(rect) {
                widgets::checker(p, rect, 5.0);
                let tex = app.layer_thumb(ctx, doc, l);
                p.image(tex, rect, Rect::from_min_max(egui::Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
            }
        }
    }
    let stroke = if t.pro {
        Stroke::new(1.0, Color32::from_gray(20))
    } else if selected {
        Stroke::new(1.5, t.text)
    } else {
        Stroke::new(1.0, t.field_border)
    };
    p.rect_stroke(rect, CornerRadius::same(if t.pro { 0 } else { 5 }), stroke, StrokeKind::Outside);
    crate::smart_ui::thumb_badge(ui, l, rect);
}

/// Drag a layer row to reorder: drop on the upper/lower half to place above/below, or on the middle
/// of a group to move into it. One `layer.moveTo` command (one undo step).
pub(super) fn layer_drag_and_drop(
    app: &mut PhotocraftApp,
    ctx: &egui::Context,
    ui: &egui::Ui,
    l: &Layer,
    rect: Rect,
    resp: &egui::Response,
    actions: &mut Vec<(String, Value)>,
) {
    let vector = app.session.prefs().toolbar.persona == photocraft_engine::toolsets::Persona::Vector;
    if vector && resp.drag_started() {
        crate::clip_compound::layer_row(ui, resp, l.id, &app.session, &mut app.clip_compound_state);
    }
    let t = Tokens::get(ctx);
    let key = egui::Id::new("layer-drag");
    if resp.drag_started() {
        ctx.data_mut(|d| d.insert_temp(key, l.id.0));
    }
    let Some(dragged) = ctx.data(|d| d.get_temp::<u64>(key)) else { return };
    let pointer = ctx.input(|i| i.pointer.interact_pos());
    let released = ctx.input(|i| i.pointer.any_released());
    if dragged == l.id.0 {
        // Ghost label following the pointer.
        if let Some(p) = pointer {
            crate::layer_transfer::ghost(ctx, p, &l.name);
        }
        return;
    }
    let Some(p) = pointer else { return };
    if !rect.contains(p) {
        return;
    }
    let f = (p.y - rect.top()) / rect.height();
    let native_inside = vector
        && (0.3..0.7).contains(&f)
        && matches!(l.content, LayerContent::Shape(_) | LayerContent::Text(_) | LayerContent::Group(_) | LayerContent::Symbol(_));
    if native_inside {
        crate::clip_compound::layer_row(ui, resp, l.id, &app.session, &mut app.clip_compound_state);
    }
    let position = if (l.is_group() || native_inside) && (0.3..0.7).contains(&f) {
        "into"
    } else if f < 0.5 {
        "above"
    } else {
        "below"
    };
    let painter = ui.painter();
    match position {
        "into" => {
            painter.rect_stroke(rect.shrink(1.0), t.radius_sm, Stroke::new(2.0, t.accent), StrokeKind::Inside);
        }
        "above" => {
            painter.line_segment([rect.left_top(), rect.right_top()], Stroke::new(2.0, t.accent));
        }
        _ => {
            painter.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(2.0, t.accent));
        }
    }
    if released && !native_inside {
        actions.push(("layer.moveTo".into(), json!({"layer": dragged, "target": l.id.0, "position": position})));
    }
}

/// Photoshop shows a layer's effects as indented sub-rows ("Effects", then each effect).
pub(super) fn effect_rows(app: &mut PhotocraftApp, ui: &mut egui::Ui, l: &Layer, depth: usize) {
    let t = Tokens::get(ui.ctx());
    let indent = 30.0 + depth as f32 * 14.0 + 34.0;
    let mut rows: Vec<(String, bool, Option<&'static str>)> = vec![("Effects".into(), l.effects.enabled, None)];
    for e in &l.effects.items {
        let kind = crate::layer_style::KINDS.iter().find(|k| k.1 == e.label()).map(|k| k.0);
        rows.push((e.label().to_string(), e.enabled(), kind));
    }
    for (i, (name, on, kind)) in rows.into_iter().enumerate() {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
        if !ui.is_rect_visible(rect) {
            continue;
        }
        if resp.hovered() {
            ui.painter().rect_filled(rect, 0.0, t.hover.gamma_multiply(0.35));
        }
        if t.pro {
            ui.painter().line_segment([pos2(rect.left() + 30.0, rect.top()), pos2(rect.left() + 30.0, rect.bottom())], Stroke::new(1.0, t.separator));
        }
        let eye = Rect::from_min_size(pos2(rect.left() + 6.0, rect.center().y - 9.0), vec2(18.0, 18.0));
        if on {
            icons::paint(ui, eye, "eye", 12.0, t.icon);
        }
        let x = rect.left() + indent + if i == 0 { 0.0 } else { 16.0 };
        if i == 0 {
            icons::paint(ui, Rect::from_center_size(pos2(x - 12.0, rect.center().y), vec2(14.0, 14.0)), "sparkles", 11.0, t.text_dim);
        }
        let right = rect.right() - crate::layer_row_ui::RIGHT_PAD;
        crate::layer_row_ui::label(
            ui.painter(),
            x,
            rect.center().y,
            right,
            &name,
            egui::FontId::proportional(11.5),
            if on { t.text_dim } else { t.text_faint },
        );
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name.clone()));
        if resp.double_clicked() {
            crate::layer_style::open(app, kind);
        }
    }
}
