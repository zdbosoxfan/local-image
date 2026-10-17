//! The toolbar: Photoshop-order tool sections with flyout slots, and the colour chips.
use super::*;

// Toolbar

/// Tool groups separated by hairlines, Photoshop order.
/// Photoshop toolbar: divider-separated sections of slots; each slot is a flyout group whose button
/// shows the group's last-used tool.
pub(super) const TOOL_SECTIONS: &[&[&[Tool]]] = &[
    &[&[Tool::Move]],
    &[
        &[Tool::RectMarquee, Tool::EllipseMarquee],
        &[Tool::Lasso, Tool::PolygonLasso, Tool::MagneticLasso],
        &[Tool::ObjectSelection, Tool::QuickSelection, Tool::MagicWand],
        &[Tool::Crop, Tool::PerspectiveCrop, Tool::Slice, Tool::SliceSelect],
        &[Tool::Eyedropper, Tool::Ruler, Tool::Note, Tool::Count],
    ],
    &[
        &[Tool::SpotHealing, Tool::Healing, Tool::Patch, Tool::ContentAwareMove, Tool::RedEye],
        &[Tool::Brush, Tool::Pencil, Tool::MixerBrush],
        &[Tool::CloneStamp],
        &[Tool::HistoryBrush],
        &[Tool::Eraser, Tool::BackgroundEraser, Tool::MagicEraser],
        &[Tool::Gradient, Tool::PaintBucket],
        &[Tool::Blur, Tool::Sharpen, Tool::Smudge],
        &[Tool::Dodge, Tool::Burn, Tool::Sponge],
    ],
    &[
        &[Tool::Pen],
        &[Tool::Type, Tool::VerticalType],
        &[Tool::PathSelection, Tool::DirectSelection],
        &[Tool::Rectangle, Tool::EllipseShape, Tool::Triangle, Tool::Polygon, Tool::Line, Tool::CustomShape],
    ],
    // local-image: the AI tools, in their own section.
    &[&[Tool::AiRemove], &[Tool::AiCutout]],
    &[&[Tool::Hand, Tool::RotateView], &[Tool::Zoom]],
];

/// The tool a slot shows: the current tool if it belongs to the slot, else the last one used.
pub(super) fn slot_tool(ui: &egui::Ui, current: Tool, slot: &[Tool], key: egui::Id) -> Tool {
    if slot.contains(&current) {
        ui.data_mut(|d| d.insert_temp(key, current));
        return current;
    }
    ui.data(|d| d.get_temp::<Tool>(key)).filter(|t| slot.contains(t)).unwrap_or(slot[0])
}

/// The toolbar's sections for the active tool set: each slot keeps the tools the set lists
/// (with its index in [`TOOL_SECTIONS`], which keys the slot's memory), slots and sections left
/// empty are dropped. The active tool is always shown, whatever the set says: its shortcut works
/// for a tool the set leaves out, and the toolbar then shows it in its usual place.
pub fn visible_sections(app: &PhotocraftApp) -> Vec<Vec<(usize, Vec<Tool>)>> {
    let set = app.session.prefs().toolbar.active();
    let mut ranks: std::collections::HashMap<Tool, usize> =
        set.tools.iter().enumerate().filter_map(|(i, name)| Tool::from_name(name).map(|tool| (tool, i))).collect();
    if set.persona == photocraft_engine::toolsets::Persona::Pixel && set.id == photocraft_engine::toolsets::DEFAULT_ID {
        for (i, id) in photocraft_engine::shared_tools::SHARED_ADDITIONS.iter().enumerate() {
            if let Some(tool) = Tool::from_name(id) {
                ranks.entry(tool).or_insert(set.tools.len() + i);
            }
        }
    }
    let rank = |tool: &Tool| ranks.get(tool).copied().unwrap_or(usize::MAX);
    let current = app.ui.tool;
    if set.persona == photocraft_engine::toolsets::Persona::Vector {
        let mut slots = crate::toolsets_ui::persona::vector_slots();
        for (_, tools) in &mut slots {
            tools.retain(|t| rank(t) != usize::MAX || *t == current);
            tools.sort_by_key(rank);
        }
        slots.retain(|(_, tools)| !tools.is_empty());
        slots.sort_by_key(|(slot, tools)| (tools.iter().map(rank).min().unwrap_or(usize::MAX), *slot));
        return vec![slots];
    }
    let mut index = 0usize;
    let mut ordered = Vec::new();
    for (section_index, section) in TOOL_SECTIONS.iter().enumerate() {
        for slot in *section {
            let mut kept: Vec<Tool> = slot.iter().copied().filter(|t| rank(t) != usize::MAX || *t == current).collect();
            kept.sort_by_key(rank);
            if !kept.is_empty() {
                let first = kept.iter().map(rank).min().unwrap_or(usize::MAX);
                ordered.push((first, section_index, index, kept));
            }
            index += 1;
        }
    }
    // Extra geometric flyouts belong to All Tools and explicitly saved custom sets.
    // A shortcut-selected extra remains visible in a smaller set.
    let shared: std::collections::HashSet<_> = photocraft_engine::shared_tools::SHARED_ADDITIONS.iter().filter_map(|id| Tool::from_name(id)).collect();
    for (slot, tools) in crate::toolsets_ui::persona::vector_slots() {
        let kept: Vec<_> = tools.into_iter().filter(|t| shared.contains(t) && (rank(t) != usize::MAX || *t == current)).collect();
        if !kept.is_empty() {
            ordered.push((kept.iter().map(rank).min().unwrap_or(usize::MAX), TOOL_SECTIONS.len(), 1000 + slot, kept));
        }
    }
    // Order applies to slots and to the tools in each flyout. Stable canonical ids keep the
    // last-used tool attached to its group when a set is reordered or switched.
    ordered.sort_by_key(|(rank, _, index, _)| (*rank, *index));
    let mut out: Vec<Vec<(usize, Vec<Tool>)>> = Vec::new();
    let mut previous = None;
    for (_, section, index, kept) in ordered {
        if previous == Some(section)
            && let Some(group) = out.last_mut()
        {
            group.push((index, kept));
        } else {
            out.push(vec![(index, kept)]);
        }
        previous = Some(section);
    }
    out
}

pub fn toolbar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    crate::toolsets_ui::persona::frame(app, ui.ctx());
    let t = Tokens::get(ui.ctx());
    let (w1, bx, m) = if t.pro { (40.0, 30.0, 5i8) } else { (50.0, 36.0, 7i8) };
    // Photoshop switches to a double-column toolbar only when one column doesn't fit.
    let sections = visible_sections(app);
    let slots: usize = sections.iter().map(|g| g.len()).sum();
    let double = toolbar_needs_double(slots + 2, sections.len(), bx, t.pro, ui.available_rect_before_wrap().height());
    let w = if double { w1 + bx + 2.0 } else { w1 };
    egui::Panel::left("toolbar").resizable(false).exact_size(w).frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(m, 8))).show(
        ui,
        |ui| {
            if t.pro {
                let r = ui.max_rect();
                ui.painter().line_segment([r.right_top() + vec2(m as f32, -8.0), r.right_bottom() + vec2(m as f32, 8.0)], Stroke::new(1.0, t.separator));
                // collapse chevrons like Photoshop's toolbar header
                let (cr, _) = ui.allocate_exact_size(vec2(bx, 14.0), Sense::hover());
                icons::paint(ui, cr, "chevrons-right", 11.0, t.text_faint);
                ui.add_space(4.0);
            }
            // Tool set switcher (Window › Tool Set).
            crate::toolsets_ui::switcher(app, ui, if t.pro { bx } else { bx * 0.8 });
            ui.add_space(4.0);
            // Subtle violet wash at the bottom of the toolbar.
            let full = ui.max_rect();
            if !t.bevel && !t.pro && t.dark() {
                let mut mesh = egui::Mesh::default();
                let r = Rect::from_min_max(pos2(full.left() - 7.0, full.bottom() - 260.0), pos2(full.right() + 7.0, full.bottom() + 8.0));
                let top = Color32::TRANSPARENT;
                let bottom = Color32::from_rgba_unmultiplied(90, 70, 190, 34);
                mesh.colored_vertex(r.left_top(), top);
                mesh.colored_vertex(r.right_top(), top);
                mesh.colored_vertex(r.right_bottom(), bottom);
                mesh.colored_vertex(r.left_bottom(), bottom);
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                ui.painter().add(mesh);
            }
            ui.spacing_mut().item_spacing = vec2(2.0, 3.0);
            let flyout_id = egui::Id::new("tool-flyout");
            let held_id = flyout_id.with("held");
            if ui.input(|i| i.pointer.any_pressed()) {
                ui.data_mut(|d| d.remove::<egui::Id>(held_id));
            }
            if let Some((key, _)) = ui.data(|d| d.get_temp::<(egui::Id, Rect)>(flyout_id))
                && !sections.iter().flatten().any(|(index, slot)| slot.len() > 1 && egui::Id::new(("tool-slot", *index)) == key)
            {
                ui.data_mut(|d| d.remove::<(egui::Id, Rect)>(flyout_id));
            }
            // Theme changes appearance, never the availability of footer actions.
            // Reserve their measured geometry before scrolling the tool slots.
            let spacing = ui.spacing().item_spacing.y;
            let row_height = ui.spacing().interact_size.y;
            let chips = if app.session.prefs().toolbar.persona == photocraft_engine::toolsets::Persona::Vector {
                46.0 + 2.0 * (spacing + row_height.max(20.0))
            } else {
                38.0 + spacing + row_height.max(18.0)
            };
            // Space after the scroll area and each footer block, including the final button.
            // Horizontal chip rows reserve at least interact_size.y, even for smaller icons.
            let footer = 3.0 * bx + chips + 5.0 * spacing + if t.pro { 8.0 } else { 14.0 } + 8.0;
            let slot_height = (ui.available_height() - footer).max(bx);
            ui.spacing_mut().scroll.floating = true;
            ui.spacing_mut().scroll.floating_allocated_width = 0.0;
            egui::ScrollArea::vertical().id_salt("toolbar-slots").max_height(slot_height).auto_shrink([false, true]).show(ui, |ui| {
                for (si, section) in sections.iter().enumerate() {
                    // Photoshop 2026 draws one uninterrupted column (no group dividers).
                    if si > 0 && !t.pro {
                        ui.add_space(4.0);
                        let (r, _) = ui.allocate_exact_size(vec2(if double { bx * 2.0 + 2.0 } else { bx }, 1.0), Sense::hover());
                        ui.painter().line_segment([r.left_center() + vec2(8.0, 0.0), r.right_center() - vec2(8.0, 0.0)], Stroke::new(1.0, t.separator));
                        ui.add_space(4.0);
                    }
                    let rows: Vec<&[(usize, Vec<Tool>)]> = if double { section.chunks(2).collect() } else { section.chunks(1).collect() };
                    for row in rows {
                        ui.horizontal(|ui| {
                            for (slot_index, slot) in row.iter() {
                                let key = egui::Id::new(("tool-slot", *slot_index));
                                let slot: &[Tool] = slot;
                                let tool = slot_tool(ui, app.ui.tool, slot, key);
                                let sel = slot.contains(&app.ui.tool);
                                let resp = icons::tool_button(ui, icons::tool_icon_name_for(tool, app.session.prefs().toolbar.persona), bx, sel, "");
                                let flyout_open = ui.data(|d| d.get_temp::<(egui::Id, Rect)>(flyout_id)).is_some();
                                crate::tool_tips::attach(&app.session, ui, &resp, tool, slot, flyout_open);
                                if slot.len() > 1 {
                                    let r = resp.rect;
                                    let tri =
                                        vec![r.right_bottom() + vec2(-2.0, -2.0), r.right_bottom() + vec2(-6.0, -2.0), r.right_bottom() + vec2(-2.0, -6.0)];
                                    ui.painter().add(egui::Shape::convex_polygon(tri, t.text_faint, Stroke::NONE));
                                }
                                if resp.clicked() && ui.data(|d| d.get_temp::<egui::Id>(held_id)) != Some(key) {
                                    crate::toolsets_ui::persona::select_tool(app, ui.ctx(), tool);
                                }
                                // Right-click or long-press opens the flyout (Photoshop).
                                let held_for =
                                    resp.is_pointer_button_down_on().then(|| ui.input(|i| i.pointer.press_start_time().map(|t0| i.time - t0))).flatten();
                                if slot.len() > 1
                                    && let Some(seconds) = held_for
                                    && seconds < 0.35
                                {
                                    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(0.35 - seconds));
                                }
                                let long_press = held_for.is_some_and(|seconds| seconds >= 0.35);
                                if slot.len() > 1 && (resp.secondary_clicked() || long_press) {
                                    ui.data_mut(|d| {
                                        d.insert_temp(flyout_id, (key, resp.rect));
                                        if long_press {
                                            d.insert_temp(held_id, key);
                                        }
                                    });
                                }
                                if slot.len() > 1 && long_press {
                                    ui.ctx().request_repaint();
                                }
                                if let Some((open_key, anchor)) = ui.data(|d| d.get_temp::<(egui::Id, Rect)>(flyout_id))
                                    && open_key == key
                                {
                                    let area = egui::Area::new(key.with("flyout"))
                                        .order(egui::Order::Foreground)
                                        .fixed_pos(anchor.right_top() + vec2(6.0, 0.0))
                                        .show(ui.ctx(), |ui| {
                                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                                let label_w = slot
                                                    .iter()
                                                    .map(|it| {
                                                        ui.painter()
                                                            .layout_no_wrap(tl!(it.label()).to_string(), egui::FontId::proportional(12.5), t.text)
                                                            .size()
                                                            .x
                                                    })
                                                    .fold(0.0f32, f32::max);
                                                let fw = (label_w + 42.0 + 40.0).max(180.0);
                                                for &item in slot.iter() {
                                                    let (r, ir) = ui.allocate_exact_size(vec2(fw, 26.0), Sense::click());
                                                    if ir.hovered() {
                                                        ui.painter().rect_filled(r, 3.0, t.hover);
                                                    }
                                                    if item == tool {
                                                        ui.painter().rect_filled(
                                                            Rect::from_center_size(pos2(r.left() + 8.0, r.center().y), vec2(4.0, 4.0)),
                                                            0.0,
                                                            t.text,
                                                        );
                                                    }
                                                    icons::paint_tool(
                                                        ui,
                                                        Rect::from_center_size(pos2(r.left() + 26.0, r.center().y), vec2(18.0, 18.0)),
                                                        icons::tool_icon_name_for(item, app.session.prefs().toolbar.persona),
                                                        20.0,
                                                        t.icon,
                                                    );
                                                    ui.painter().text(
                                                        pos2(r.left() + 42.0, r.center().y),
                                                        Align2::LEFT_CENTER,
                                                        tl!(item.label()),
                                                        egui::FontId::proportional(12.5),
                                                        t.text,
                                                    );
                                                    crate::tool_tips::attach(&app.session, ui, &ir, item, slot, true);
                                                    if let Some(key) = crate::tool_tips::shortcut(&app.session, item) {
                                                        ui.painter().text(
                                                            pos2(r.right() - 8.0, r.center().y),
                                                            Align2::RIGHT_CENTER,
                                                            key,
                                                            egui::FontId::proportional(12.0),
                                                            t.text_dim,
                                                        );
                                                    }
                                                    if ir.clicked() {
                                                        crate::toolsets_ui::persona::select_tool(app, ui.ctx(), item);
                                                        ui.data_mut(|d| d.remove::<(egui::Id, Rect)>(flyout_id));
                                                    }
                                                }
                                            });
                                        });
                                    let held_release = resp.clicked() && ui.data(|d| d.get_temp::<egui::Id>(held_id)) == Some(key);
                                    let clicked_outside = ui.input(|i| i.pointer.any_click()) && !held_release && !area.response.hovered() && !resp.hovered();
                                    if clicked_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                        ui.data_mut(|d| d.remove::<(egui::Id, Rect)>(flyout_id));
                                    }
                                }
                            }
                        });
                    }
                }
            });
            let ctx = ui.ctx().clone();
            #[cfg(test)]
            {
                let clip = ui.clip_rect().intersect(ui.max_rect());
                ctx.data_mut(|data| data.insert_temp(egui::Id::new("theme-toolbar-clip"), clip));
            }
            if icons::tool_button(ui, "edit-toolbar", bx, false, tl!("Edit Toolbar…")).clicked() {
                let _ = crate::menus::invoke(app, &ctx, "edit.toolbar", json!({}));
            }
            ui.add_space(if t.pro { 8.0 } else { 14.0 });
            if app.session.prefs().toolbar.persona == photocraft_engine::toolsets::Persona::Vector {
                let ids = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
                if let Err(e) = crate::fill_stroke_proxy::widget(ui, &mut app.ui.fill_stroke_proxy, &mut app.session, &ids, &mut app.clip_compound_state) {
                    app.ui.status = e.to_string();
                    app.ui.status_error = true;
                }
            } else {
                color_chips(app, ui);
            }
            ui.add_space(8.0);
            let quick_mask = app.session.active().is_some_and(|s| s.doc.quick_mask.is_some());
            if icons::tool_button(
                ui,
                "quick-mask",
                bx,
                quick_mask,
                if quick_mask { tl!("Edit in Standard Mode  (Q)") } else { tl!("Edit in Quick Mask Mode  (Q)") },
            )
            .clicked()
            {
                let _ = crate::menus::invoke(app, &ctx, "select.editInQuickMaskMode", json!({}));
            }
            let sm = icons::button(ui, "app-window", bx, false, tl!("Change Screen Mode  (F)"));
            if sm.clicked() {
                let _ = crate::menus::invoke(app, &ctx, "view.screenMode.cycle", json!({}));
            }
            // Right-click (or long-press) lists the modes, like Photoshop's flyout.
            egui::Popup::context_menu(&sm).show(|ui| {
                ui.set_min_width(220.0);
                for (id, label) in [
                    ("view.screenMode.standard", tl!("Standard Screen Mode")),
                    ("view.screenMode.fullScreenWithMenuBar", tl!("Full Screen Mode With Menu Bar")),
                    ("view.screenMode.fullScreen", tl!("Full Screen Mode")),
                ] {
                    let on = crate::view_cmds::checked(app, id).unwrap_or(false);
                    if ui.add(egui::Button::selectable(on, label)).clicked() {
                        let _ = crate::menus::invoke(app, &ctx, id, json!({}));
                        ui.close();
                    }
                }
            });
        },
    );
}

/// Does the toolbar need two columns? Height of one column (header, tool slots, Edit Toolbar,
/// colour chips, Quick Mask and Screen Mode) against the height the toolbar gets.
pub fn toolbar_needs_double(slots: usize, sections: usize, bx: f32, pro: bool, avail_h: f32) -> bool {
    let pitch = bx + 3.0;
    let needed = if pro {
        // margins + header + slots + "…" + gap + chips (38 + swap row) + gap + 2 buttons
        16.0 + 21.0 + (slots + 1) as f32 * pitch + 8.0 + 59.0 + 8.0 + 2.0 * pitch
    } else {
        16.0 + (slots + 1) as f32 * pitch + sections.saturating_sub(1) as f32 * 12.0 + 14.0 + 59.0 + 8.0 + 2.0 * pitch
    };
    needed > avail_h
}

pub(super) fn c32(c: [f32; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, (c[3] * 255.0) as u8)
}

pub(super) fn color_chips(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 38.0), Sense::hover());
    let bg = Rect::from_min_size(rect.min + vec2(13.0, 13.0), vec2(21.0, 21.0));
    let fg = Rect::from_min_size(rect.min + vec2(2.0, 2.0), vec2(21.0, 21.0));
    let p = ui.painter();
    p.rect_filled(bg, 5.0, c32(app.session.tools.background));
    p.rect_stroke(bg, 5.0, Stroke::new(1.0, t.field_border), StrokeKind::Outside);
    p.rect_filled(fg, 5.0, c32(app.session.tools.foreground));
    p.rect_stroke(fg, 5.0, Stroke::new(1.5, t.chrome), StrokeKind::Outside);
    p.rect_stroke(fg, 5.0, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
    // Photoshop: clicking a chip opens the Color Picker for that colour.
    let bg_resp = ui.interact(bg, ui.id().with("bgchip"), Sense::click());
    let fg_resp = ui.interact(fg, ui.id().with("fgchip"), Sense::click());
    if fg_resp.on_hover_text(tl!("Set foreground color")).clicked() {
        crate::color_picker_ui::open(app, "foreground");
    } else if bg_resp.on_hover_text(tl!("Set background color")).clicked() {
        crate::color_picker_ui::open(app, "background");
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if icons::button(ui, "arrow-left-right", 18.0, false, tl!("Swap colours (X)")).clicked() {
            let _ = app.run("tools.swapColors", json!({}));
        }
        if icons::button(ui, "contrast", 18.0, false, tl!("Default colours (D)")).clicked() {
            let _ = app.run("tools.defaultColors", json!({}));
        }
    });
}
