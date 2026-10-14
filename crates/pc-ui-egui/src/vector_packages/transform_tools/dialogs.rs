use super::*;
use li_i18n::t;
fn number(text: &str) -> Result<f64, String> {
    let v = text.trim().parse::<f64>().map_err(|_| "Enter a number".to_string())?;
    engine::finite(v, 1e7).map_err(|e| e.to_string())?;
    Ok(v)
}
fn field(ui: &mut Ui, label: &str, value: &mut String) -> bool {
    ui.horizontal(|ui| {
        let label = ui.label(t(label));
        ui.add(egui::TextEdit::singleline(value).desired_width(110.)).labelled_by(label.id).changed()
    })
    .inner
}
fn options(ui: &mut Ui, o: &mut Options) -> bool {
    let mut changed = ui.checkbox(&mut o.scale_strokes_effects, t("Scale Strokes & Effects")).changed();
    changed |= ui.checkbox(&mut o.scale_corners, t("Scale Corners")).changed();
    changed |= ui.checkbox(&mut o.objects, t("Transform Objects")).changed();
    changed |= ui.checkbox(&mut o.patterns, t("Transform Patterns")).changed();
    changed
}
impl Dialog {
    fn request(&self, options: Options, copy: bool) -> Result<Commit, String> {
        let affine = |m| Operation::Affine { matrix: m };
        let operation = match self.kind {
            Kind::Rotate => affine(engine::rotate(self.origin, number(&self.fields[0])?).map_err(|e| e.to_string())?),
            Kind::Reflect => affine(engine::reflect(self.origin, number(&self.fields[0])?).map_err(|e| e.to_string())?),
            Kind::Scale => {
                let x = number(&self.fields[0])? / 100.;
                let y = if self.proportional { x } else { number(&self.fields[1])? / 100. };
                affine(engine::scale(self.origin, x, y).map_err(|e| e.to_string())?)
            }
            Kind::Shear => affine(engine::shear(self.origin, number(&self.fields[0])?, self.vertical).map_err(|e| e.to_string())?),
            Kind::Move => {
                if self.move_mode == MoveMode::Distance {
                    affine(engine::move_by(number(&self.fields[0])?, number(&self.fields[1])?).map_err(|e| e.to_string())?)
                } else {
                    let mut d = [number(&self.fields[0])?, number(&self.fields[1])?];
                    if self.move_mode == MoveMode::Position {
                        d[0] -= self.origin.x;
                        d[1] -= self.origin.y;
                    }
                    affine(engine::validate_matrix([1., 0., 0., 1., d[0], d[1]]).map_err(|e| e.to_string())?.m)
                }
            }
            Kind::Each => Operation::Each(Each {
                scale: [number(&self.fields[0])? / 100., number(&self.fields[1])? / 100.],
                movement: [number(&self.fields[2])?, number(&self.fields[3])?],
                rotation: number(&self.fields[4])?,
                reference: self.reference,
                random: self.random,
                seed: if self.random { self.seed.trim().parse().map_err(|_| "Seed must be an unsigned integer".to_string())? } else { 0 },
                reflect: [self.vertical, self.proportional],
            }),
            _ => return Err("this tool uses canvas handles".into()),
        };
        Ok(Commit { capture: self.capture.clone(), operation, options, copy, conversion: ConversionChoice::Ask })
    }
}
impl State {
    /// Show dialogs after canvas input; preview remains detached until OK/Copy.
    pub fn dialogs(&mut self, ctx: &egui::Context, s: &mut Session) {
        if let Some(mut dialog) = self.dialog.take() {
            let mut action = 0;
            let mut changed = self.preview.is_none();
            let mut open = true;
            egui::Window::new(t(dialog.kind.label())).id(egui::Id::new("vector-transform-dialog")).open(&mut open).collapsible(false).resizable(false).show(
                ctx,
                |ui| {
                    match dialog.kind {
                        Kind::Rotate | Kind::Reflect | Kind::Shear => {
                            changed |= field(ui, if dialog.kind == Kind::Reflect { "Axis angle" } else { "Angle" }, &mut dialog.fields[0]);
                            if dialog.kind == Kind::Shear {
                                changed |= ui.checkbox(&mut dialog.vertical, t("Vertical axis")).changed();
                            }
                        }
                        Kind::Scale => {
                            changed |= ui.checkbox(&mut dialog.proportional, t("Proportional")).changed();
                            changed |= field(ui, "Horizontal %", &mut dialog.fields[0]);
                            if !dialog.proportional {
                                changed |= field(ui, "Vertical %", &mut dialog.fields[1]);
                            }
                        }
                        Kind::Move => {
                            ui.horizontal(|ui| {
                                changed |= ui.selectable_value(&mut dialog.move_mode, MoveMode::Relative, t("Horizontal / Vertical")).changed();
                                if ui.selectable_value(&mut dialog.move_mode, MoveMode::Position, t("Position")).clicked() {
                                    if let Some(st) = s.active()
                                        && let Ok([x, y, _, _]) = engine::bounds(&st.doc, &dialog.capture.layers)
                                    {
                                        dialog.origin = Point::new(x, y);
                                        dialog.fields[0] = x.to_string();
                                        dialog.fields[1] = y.to_string();
                                    }
                                    changed = true;
                                }
                                changed |= ui.selectable_value(&mut dialog.move_mode, MoveMode::Distance, t("Distance / Angle")).changed();
                            });
                            let labels = match dialog.move_mode {
                                MoveMode::Relative => ["Horizontal", "Vertical"],
                                MoveMode::Position => ["X position", "Y position"],
                                MoveMode::Distance => ["Distance", "Angle"],
                            };
                            changed |= field(ui, labels[0], &mut dialog.fields[0]);
                            changed |= field(ui, labels[1], &mut dialog.fields[1]);
                        }
                        Kind::Each => {
                            for (i, label) in ["Horizontal %", "Vertical %", "Horizontal move", "Vertical move", "Angle"].iter().enumerate() {
                                changed |= field(ui, label, &mut dialog.fields[i]);
                            }
                            changed |= ui.checkbox(&mut dialog.vertical, t("Reflect X")).changed();
                            changed |= ui.checkbox(&mut dialog.proportional, t("Reflect Y")).changed();
                            ui.label(t("Reference point"));
                            egui::Grid::new("transform-each-reference").show(ui, |ui| {
                                for i in 0..9 {
                                    changed |= ui.selectable_value(&mut dialog.reference, i, (i + 1).to_string()).changed();
                                    if i % 3 == 2 {
                                        ui.end_row();
                                    }
                                }
                            });
                            changed |= ui.checkbox(&mut dialog.random, t("Random")).changed();
                            changed |= field(ui, "Seed", &mut dialog.seed);
                        }
                        _ => {}
                    }
                    ui.separator();
                    changed |= options(ui, &mut self.preferences.options);
                    changed |= ui.checkbox(&mut dialog.preview, t("Preview")).changed();
                    if let Some(error) = &self.error {
                        ui.colored_label(Color32::RED, t(error));
                    }
                    ui.horizontal(|ui| {
                        if ui.button(t("Cancel")).clicked() {
                            action = 1;
                        }
                        if ui.button(t("Copy")).clicked() {
                            action = 2;
                        }
                        if ui.button(t("OK")).clicked() {
                            action = 3;
                        }
                    });
                },
            );
            if !open || action == 1 {
                self.cancel();
                return;
            }
            if action >= 2 {
                let result = dialog.request(self.preferences.options.clone(), action == 2).and_then(|c| self.publish(s, &c));
                match result {
                    Ok(()) => {
                        self.error = None;
                        return;
                    }
                    Err(error) => {
                        self.error = Some(error);
                        self.preview = None;
                    }
                }
            } else if changed {
                self.generation = self.generation.wrapping_add(1) | (1 << 59);
                if dialog.preview {
                    match dialog.request(self.preferences.options.clone(), false).and_then(|c| engine::prepare(s, &c).map_err(|e| e.to_string())) {
                        Ok(p) => {
                            self.preview = Some(p.document);
                            self.error = None;
                        }
                        Err(error) => {
                            self.preview = None;
                            self.error = Some(error);
                        }
                    }
                } else {
                    self.preview = None;
                    self.error = None;
                }
            }
            self.dialog = Some(dialog);
        }
        if let Some(request) = self.pending.clone() {
            let mut action = 0;
            egui::Window::new(t("Transform text and symbols")).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(t("Convert text and symbols to curves to apply this transform."));
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::RED, t(error));
                }
                if ui.button(t("Convert to Curves")).clicked() {
                    action = 1;
                }
                if ui.button(t("Cancel")).clicked() {
                    action = 3;
                }
            });
            match action {
                1 => {
                    let mut c = request;
                    c.conversion = ConversionChoice::Curves;
                    if let Err(e) = self.publish(s, &c) {
                        self.error = Some(e);
                    }
                }
                2 => {
                    let mut c = request;
                    c.conversion = ConversionChoice::Envelope;
                    self.pending = None;
                    self.envelope_request = Some(EnvelopeRequest { transform: c });
                    self.preview = None;
                }
                3 => self.cancel(),
                _ => {}
            }
        }
    }
    pub fn preview_envelope(&mut self, s: &Session, router: &dyn engine::EnvelopeRouter) -> Result<(), String> {
        let request = self.envelope_request.as_ref().ok_or("no envelope request")?;
        request.transform.capture.check(s).map_err(|e| e.to_string())?;
        self.preview = Some(router.preview(s, request).map_err(|e| e.to_string())?);
        self.generation = self.generation.wrapping_add(1) | (1 << 59);
        Ok(())
    }
    pub fn commit_envelope(&mut self, s: &mut Session, router: &dyn engine::EnvelopeRouter) -> Result<(), String> {
        let request = self.envelope_request.as_ref().ok_or("no envelope request")?;
        request.transform.capture.check(s).map_err(|e| e.to_string())?;
        let ids = router.commit(s, request).map_err(|e| e.to_string())?;
        self.cancel();
        self.targets = ids;
        Ok(())
    }
}
