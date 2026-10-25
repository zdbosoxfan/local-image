//! Installing a card: the confirmation (the licence, then the exact files, and the precision of
//! a card that offers several) and the download, which for a template ends by setting its model
//! up to run through that template.

#[cfg(test)]
use std::cell::RefCell;
#[cfg(test)]
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{Color32, RichText};
use li_ai::browser::{self, HttpNet, Item, Plan};
use li_ai::comfy::JobControl;
use li_ai::settings::AiSettings;
use serde_json::Value;

use super::{Slot, config, first_line, human, request_repaint, with};
use crate::theme::Tokens;
use crate::widgets;

#[derive(Default)]
pub(super) struct InstallRun {
    pub(super) done: u64,
    pub(super) total: u64,
    pub(super) file: String,
    pub(super) finished: bool,
    pub(super) error: Option<String>,
    pub(super) ctl: JobControl,
    pub(super) note: Option<String>,
    /// The installed template model, for "Set up and test".
    pub(super) setup: Option<String>,
}

pub(super) struct Confirm {
    /// The card as it installs (one precision of its model).
    pub(super) item: Item,
    pub(super) plan: Slot<Result<(Plan, Option<Value>), String>>,
    pub(super) accept: bool,
    /// The card with every file, when it offers several main files.
    pub(super) all: Item,
    pub(super) variant: Option<String>,
}

impl Confirm {
    /// Installing `item`: its best precision for this GPU when it offers several.
    pub(super) fn new(item: Item) -> Self {
        let blackwell = crate::ai_ui::status().gpu.is_some_and(|g| li_ai::companions::runtime().gpu_generation(&g.name) == Some("blackwell"));
        let variant = browser::hf::default_variant(&item, blackwell);
        let shown = variant.as_deref().map_or_else(|| item.clone(), |v| browser::hf::with_variant(&item, v));
        Self { plan: start_plan(shown.clone()), item: shown, accept: false, all: item, variant }
    }

    /// Switches to another of the card's main files and plans it again.
    fn choose(&mut self, main: &str) {
        self.variant = Some(main.to_owned());
        self.item = browser::hf::with_variant(&self.all, main);
        self.plan = start_plan(self.item.clone());
    }
}

#[cfg(test)]
thread_local! { static TEST_PLAN: RefCell<Option<Plan>> = const { RefCell::new(None) }; }

#[cfg(test)]
pub(crate) struct TestPlanGuard;
#[cfg(test)]
impl Drop for TestPlanGuard {
    fn drop(&mut self) {
        TEST_PLAN.with(|plan| *plan.borrow_mut() = None);
    }
}
#[cfg(test)]
pub(crate) fn test_install_plan(plan: Plan) -> TestPlanGuard {
    TEST_PLAN.with(|next| *next.borrow_mut() = Some(plan));
    with(|s| s.installed = Some((std::time::Instant::now(), Arc::new(HashSet::new()))));
    TestPlanGuard
}

/// Resolves what installing `item` downloads (and, for a template, its workflow).
fn start_plan(item: Item) -> Slot<Result<(Plan, Option<Value>), String>> {
    #[cfg(test)]
    if let Some(plan) = TEST_PLAN.with(|next| next.borrow_mut().take()) {
        return Arc::new(Mutex::new(Some(Ok((plan, None)))));
    }
    let slot = Arc::new(Mutex::new(None));
    let out = slot.clone();
    let lang = li_i18n::current();
    let _ = std::thread::Builder::new().name("model-plan".into()).spawn(move || {
        li_i18n::set_current(lang);
        let r = (|| -> anyhow::Result<(Plan, Option<Value>)> {
            let cfg = config();
            let net = HttpNet::new(cfg.clone());
            let info = if item.family.as_deref() == Some("restoration") {
                li_ai::comfy::ObjectInfo::default()
            } else {
                li_ai::service().client.object_info().map_err(|e| {
                    anyhow::anyhow!(
                        "{}",
                        li_i18n::fmt(tl!("Start the AI engine first: Local Image checks what ComfyUI already has. ({error})"), &[("error", &format!("{e:#}"))])
                    )
                })?
            };
            let reg = li_ai::family::registry();
            if item.template.is_some() {
                let stats = li_ai::service().client.system_stats()?;
                let server = li_ai::modelsetup::Snapshot::from_parts(li_ai::service().client.host(), &stats, info);
                let checked = li_ai::modelsetup::fixes::plan_template(&item, &cfg, &net, &server)?;
                let Some(mut plan) = checked.plan else {
                    let why: Vec<String> =
                        checked.issues.iter().filter(|i| i.severity == li_ai::modelsetup::issue::Severity::Blocker).map(crate::model_setup::sentence).collect();
                    anyhow::bail!("{}", why.join("\n"));
                };
                if plan.license.is_empty() {
                    plan.license =
                        item.family.as_deref().and_then(|f| reg.family(f)).map(|f| f.license.clone()).unwrap_or_else(|| tl!("See each model's page").into());
                }
                return Ok((plan, Some(checked.ui)));
            }
            let fam = item.family.as_deref().and_then(|f| reg.family(f));
            Ok((browser::plan(&item, &item.files, fam, &info, &cfg, &net)?, None))
        })()
        .map_err(|e| format!("{e:#}"));
        if let Ok(mut o) = out.lock() {
            *o = Some(r);
        }
        request_repaint();
    });
    slot
}

fn start_install(item: Item, plan: Plan, template: Option<Value>) -> Arc<Mutex<InstallRun>> {
    let run = Arc::new(Mutex::new(InstallRun { total: plan.total(), ..Default::default() }));
    // A template whose files the server already has needs no download, on any server.
    let nothing_to_download = template.is_some() && plan.files.is_empty();
    if AiSettings::load().is_remote() && !nothing_to_download {
        if let Ok(mut state) = run.lock() {
            state.finished = true;
            state.error = Some(tl!("Install generative models on the remote ComfyUI server, then check the connection again.").into());
        }
        return run;
    }
    let r = run.clone();
    let lang = li_i18n::current();
    let _ = std::thread::Builder::new().name("model-install".into()).spawn(move || {
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Background);
        li_i18n::set_current(lang);
        let cfg = config();
        let dir = AiSettings::load().model_dir();
        let ctl = r.lock().map(|x| x.ctl.clone()).unwrap_or_default();
        let progress = |done: u64, total: u64, file: &str| {
            if let Ok(mut x) = r.lock() {
                x.done = done;
                x.total = total;
                x.file = file.to_owned();
            }
            request_repaint();
        };
        let installed = if nothing_to_download { Ok(()) } else { browser::install(&plan, &dir, &cfg, &ctl, &progress) };
        let result = installed.and_then(|()| {
            // A template for a family without a builder runs through that template: its main file
            // is set to it, and model setup prepares the recipe.
            let (Some(ui), None, Some(name)) = (&template, &item.family, &item.template) else { return Ok(None) };
            let host = li_ai::service().client.host().to_owned();
            let model_dir = li_ai::modelsetup::is_local(&host).then(|| dir.clone());
            li_ai::modelsetup::fixes::adopt_template(&li_ai::modelsetup::Options::for_host(&host, model_dir), ui, name)
        });
        if let Ok(mut x) = r.lock() {
            x.finished = true;
            match result {
                Ok(setup) => {
                    x.note = setup.is_some().then(|| li_i18n::fmt(tl!("Set up “{name}” to run through its official template"), &[("name", &item.title)]));
                    x.setup = setup;
                }
                Err(e) => x.error = Some(first_line(&format!("{e:#}"))),
            }
        }
        crate::generate_ui::reload_customs();
        crate::ai_ui::refresh();
        request_repaint();
    });
    run
}

/// The install confirmation: licence first, then the exact files.
pub(super) fn confirm_window(ctx: &egui::Context, t: &Tokens) {
    let Some((item, plan, mut accept, all, variant)) =
        with(|s| s.confirm.as_ref().map(|c| (c.item.clone(), c.plan.lock().ok().and_then(|p| p.clone()), c.accept, c.all.clone(), c.variant.clone())))
    else {
        return;
    };
    let mut chosen = variant.clone();
    let mut close = false;
    let mut go: Option<(Plan, Option<Value>)> = None;
    use widgets::{ButtonRole, DialogButton};
    let buttons = |ui: &mut egui::Ui, list: &[DialogButton]| -> Option<ButtonRole> {
        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| widgets::dialog_buttons(ui, list)).inner
    };
    // Like every dialog: no dimming behind it.
    let modal = egui::Modal::new(egui::Id::new("browser-confirm")).backdrop_color(Color32::TRANSPARENT).show(ctx, |ui| {
        ui.set_width(500.0);
        ui.label(RichText::new(li_i18n::fmt(tl!("Install {name}"), &[("name", &item.title)])).size(15.0).strong().color(t.text));
        ui.add_space(6.0);
        let choices = browser::hf::variants(&all);
        if let Some(current) = chosen.as_mut().filter(|_| !choices.is_empty()) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tl!("Version")).color(t.text_dim))
                    .on_hover_text(tl!("This repository has several files of the model (precisions or models); one is installed"));
                let opts: Vec<(String, &str)> = choices.iter().map(|f| (f.name.clone(), f.name.as_str())).collect();
                widgets::dropdown(ui, "browser-confirm-variant", current, &opts, ui.available_width().min(360.0));
            });
            ui.add_space(4.0);
        }
        match &plan {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(tl!("Checking the files’ sizes and checksums…")).color(t.text_dim));
                });
                ui.ctx().request_repaint_after(Duration::from_millis(200));
                if buttons(ui, &[DialogButton::new(ButtonRole::Cancel, tl!("Cancel"), 80.0)]).is_some() {
                    close = true;
                }
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(t.danger));
                let page = item.license_url.clone().or(Some(item.page_url.clone())).filter(|u| !u.is_empty());
                let mut list = vec![DialogButton::new(ButtonRole::Cancel, tl!("Close"), 80.0)];
                if page.is_some() {
                    list.insert(0, DialogButton::new(ButtonRole::Alternate, tl!("Open Its Page"), 0.0));
                }
                match buttons(ui, &list) {
                    Some(ButtonRole::Alternate) => {
                        if let Some(url) = page {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                        }
                    }
                    Some(_) => close = true,
                    None => {}
                }
            }
            Some(Ok((p, template))) => {
                // Licence.
                egui::Frame::new().fill(t.field).corner_radius(t.radius_sm).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(tl!("Licence")).size(11.0).color(t.text_faint));
                    ui.label(RichText::new(if p.license.is_empty() { tl!("Not stated: check the model's page before use") } else { &p.license }).color(t.text));
                    if item.commercial == Some(false) {
                        ui.label(RichText::new(tl!("The author doesn't allow commercial use.")).color(t.warning));
                    }
                    if p.gated {
                        ui.label(
                            RichText::new(tl!(
                                "The publisher asks you to accept its licence on its page first; add a Hugging Face token in Local AI settings."
                            ))
                            .color(t.warning),
                        );
                    }
                    if let Some(url) = &p.license_url
                        && ui.link(tl!("Read the licence")).clicked()
                    {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                });
                ui.add_space(8.0);
                if p.files.is_empty() {
                    ui.label(RichText::new(tl!("Everything it needs is already installed.")).color(t.text_dim));
                } else {
                    let size = human(p.total());
                    let what = if p.files.len() == 1 {
                        li_i18n::fmt(tl!("1 file to download ({size}), verified by SHA-256:"), &[("size", &size)])
                    } else {
                        li_i18n::fmt(tl!("{n} files to download ({size}), verified by SHA-256:"), &[("n", &p.files.len().to_string()), ("size", &size)])
                    };
                    ui.label(RichText::new(what).color(t.text_dim));
                    egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                        for f in &p.files {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&f.name).color(t.text));
                                ui.label(RichText::new(format!("→ {}", f.folder)).size(11.0).color(t.text_faint));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(human(f.bytes)).color(t.text_dim))
                                });
                            });
                        }
                    });
                }
                if !p.present.is_empty() {
                    ui.label(
                        RichText::new(li_i18n::fmt(tl!("Already installed: {names}"), &[("names", &p.present.join(", "))])).size(11.0).color(t.text_faint),
                    );
                }
                let dir = AiSettings::load().model_dir();
                if let Some(free) = li_ai::download::free_space(&dir)
                    && free < p.total() + (256 << 20)
                {
                    ui.label(
                        RichText::new(li_i18n::fmt(
                            tl!("Not enough space in {folder} ({free} free)."),
                            &[("folder", &dir.display().to_string()), ("free", &human(free))],
                        ))
                        .color(t.danger),
                    );
                }
                if template.is_some() && item.family.is_none() {
                    ui.label(
                        RichText::new(tl!(
                            "Local Image runs this model through its official template. After the install, Set Up Models checks it and can test it."
                        ))
                        .size(11.5)
                        .color(t.text_dim),
                    );
                }
                ui.add_space(8.0);
                if !p.files.is_empty() {
                    widgets::checkbox(ui, &mut accept, tl!("I have read and accept the licence"));
                }
                let label = if p.files.is_empty() { tl!("Done") } else { tl!("Install") };
                match buttons(
                    ui,
                    &[
                        DialogButton::new(ButtonRole::Default, label, 80.0).enabled(accept || p.files.is_empty()),
                        DialogButton::new(ButtonRole::Cancel, tl!("Cancel"), 80.0),
                    ],
                ) {
                    Some(ButtonRole::Default) => go = Some((p.clone(), template.clone())),
                    Some(_) => close = true,
                    None => {}
                }
            }
        }
    });
    #[cfg(test)]
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("browser-confirm-rect"), modal.response.rect));
    close |= modal.is_top_modal && !modal.any_popup_open && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    with(|s| {
        if let Some(c) = s.confirm.as_mut() {
            c.accept = accept;
            if let Some(v) = chosen.filter(|v| Some(v) != variant.as_ref()) {
                c.choose(&v);
            }
        }
        if let Some((p, template)) = go {
            if !p.files.is_empty() || template.is_some() {
                s.installs.insert(item.id.clone(), start_install(item.clone(), p, template));
            }
            s.confirm = None;
        } else if close {
            s.confirm = None;
        }
    });
}
