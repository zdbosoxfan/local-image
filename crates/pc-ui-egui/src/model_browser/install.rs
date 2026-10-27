//! Installing a card: the confirmation (the licence, then the exact files: the precision of a
//! card that offers several, chosen to fit the GPU, and its optional files, ticked by hand) and the
//! download, which for a template ends by setting its model up to run through that template.

#[cfg(test)]
use std::cell::RefCell;
#[cfg(test)]
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{Color32, RichText};
use li_ai::browser::{self, CatalogFile, HttpNet, Item, Plan};
use li_ai::comfy::JobControl;
use li_ai::settings::AiSettings;
use serde_json::Value;

use super::{Slot, Spawn, config, first_line, human, request_repaint, with};
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

/// What installing a card downloads.
#[derive(Clone)]
pub(super) struct Planned {
    /// The card as it installs: one precision of its model, and the optional files ticked.
    pub(super) item: Item,
    /// The main file installed, when the card offers several.
    pub(super) variant: Option<String>,
    pub(super) plan: Plan,
    /// A template card's workflow.
    pub(super) template: Option<Value>,
    /// Files that install only when ticked (LoRAs, a prompt enhancer).
    pub(super) optional: Vec<CatalogFile>,
}

pub(super) struct Confirm {
    pub(super) plan: Slot<Result<Planned, String>>,
    pub(super) accept: bool,
    /// The card with every file.
    pub(super) all: Item,
    /// The main file picked in the Version list; until then the plan picks the largest that fits
    /// this GPU.
    pub(super) variant: Option<String>,
    /// The optional files ticked.
    pub(super) extras: Vec<String>,
}

impl Confirm {
    pub(super) fn new(item: Item) -> Self {
        Self { plan: start_plan(item.clone(), None, Vec::new(), std::thread::Builder::spawn), accept: false, all: item, variant: None, extras: Vec::new() }
    }

    fn replan(&mut self) {
        self.plan = start_plan(self.all.clone(), self.variant.clone(), self.extras.clone(), std::thread::Builder::spawn);
    }

    /// Switches to another of the card's main files and plans it again.
    fn choose(&mut self, main: &str) {
        self.variant = Some(main.to_owned());
        self.replan();
    }

    /// Ticks or unticks an optional file and plans again, keeping the precision planned.
    fn tick(&mut self, name: &str, on: bool) {
        if self.variant.is_none() {
            self.variant = self.plan.lock().ok().and_then(|p| p.as_ref()?.as_ref().ok()?.variant.clone());
        }
        self.extras.retain(|e| e != name);
        if on {
            self.extras.push(name.to_owned());
        }
        self.replan();
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

/// Resolves what installing `item` downloads: its main file `variant` (by default the largest
/// precision that fits this GPU, by the files' sizes), the optional files `extras` names, and for
/// a template its workflow.
fn start_plan(item: Item, variant: Option<String>, extras: Vec<String>, spawn: Spawn) -> Slot<Result<Planned, String>> {
    let gpu = crate::ai_ui::status().gpu;
    let blackwell = gpu.as_ref().is_some_and(|g| li_ai::companions::runtime().gpu_generation(&g.name) == Some("blackwell"));
    let vram = gpu.map(|g| g.vram_total).filter(|v| *v > 0);
    #[cfg(test)]
    if let Some(plan) = TEST_PLAN.with(|next| next.borrow_mut().take()) {
        let variant = variant.or_else(|| browser::hf::default_variant(&item, blackwell, vram));
        let optional = browser::hf::optional_files(&item, variant.as_deref());
        let item = browser::hf::selection(&item, variant.as_deref(), &extras);
        return Arc::new(Mutex::new(Some(Ok(Planned { item, variant, plan, template: None, optional }))));
    }
    let slot = Arc::new(Mutex::new(None));
    let out = slot.clone();
    let lang = li_i18n::current();
    let work = move || {
        li_i18n::set_current(lang);
        let r = (|| -> anyhow::Result<Planned> {
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
                let checked = li_ai::modelsetup::fixes::plan_template(&item, &cfg, &net, &server, &extras)?;
                let Some(mut plan) = checked.plan else {
                    let why: Vec<String> =
                        checked.issues.iter().filter(|i| i.severity == li_ai::modelsetup::issue::Severity::Blocker).map(crate::model_setup::sentence).collect();
                    anyhow::bail!("{}", why.join("\n"));
                };
                if plan.license.is_empty() {
                    plan.license =
                        item.family.as_deref().and_then(|f| reg.family(f)).map(|f| f.license.clone()).unwrap_or_else(|| tl!("See each model's page").into());
                }
                return Ok(Planned { item, variant: None, plan, template: Some(checked.ui), optional: checked.optional });
            }
            let variant = match variant {
                Some(v) => Some(v),
                None if browser::hf::variants(&item).is_empty() => None,
                None => browser::hf::default_variant(&browser::hf::with_sizes(&item, &cfg, &net), blackwell, vram),
            };
            let optional = browser::hf::optional_files(&item, variant.as_deref());
            let item = browser::hf::selection(&item, variant.as_deref(), &extras);
            let fam = item.family.as_deref().and_then(|f| reg.family(f));
            let plan = browser::plan(&item, &item.files, fam, &info, &cfg, &net)?;
            Ok(Planned { item, variant, plan, template: None, optional })
        })()
        .map_err(|e| format!("{e:#}"));
        if let Ok(mut o) = out.lock() {
            *o = Some(r);
        }
        request_repaint();
    };
    if let Err(error) = spawn(std::thread::Builder::new().name("model-plan".into()), Box::new(work)) {
        if let Ok(mut state) = slot.lock() {
            *state = Some(Err(error.to_string()));
        }
        request_repaint();
    }
    slot
}

fn start_install(item: Item, plan: Plan, template: Option<Value>, settings: &AiSettings, spawn: Spawn) -> Arc<Mutex<InstallRun>> {
    let run = Arc::new(Mutex::new(InstallRun { total: plan.total(), ..Default::default() }));
    // A template whose files the server already has needs no download, on any server.
    let nothing_to_download = template.is_some() && plan.files.is_empty();
    if settings.is_remote() && !nothing_to_download {
        if let Ok(mut state) = run.lock() {
            state.finished = true;
            state.error = Some(tl!("Install generative models on the remote ComfyUI server, then check the connection again.").into());
        }
        return run;
    }
    let r = run.clone();
    let lang = li_i18n::current();
    let work = move || {
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Background);
        li_i18n::set_current(lang);
        let cfg = config();
        let settings = AiSettings::load();
        let dir = settings.model_dir();
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
            let model_dir = (!settings.is_remote() && li_ai::modelsetup::is_local(&host)).then(|| dir.clone());
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
    };
    if let Err(error) = spawn(std::thread::Builder::new().name("model-install".into()), Box::new(work)) {
        if let Ok(mut state) = run.lock() {
            state.finished = true;
            state.error = Some(error.to_string());
        }
        request_repaint();
    }
    run
}

/// The install confirmation: licence first, then the exact files.
pub(super) fn confirm_window(ctx: &egui::Context, t: &Tokens) {
    let Some((plan, mut accept, all, extras)) =
        with(|s| s.confirm.as_ref().map(|c| (c.plan.lock().ok().and_then(|p| p.clone()), c.accept, c.all.clone(), c.extras.clone())))
    else {
        return;
    };
    let item = match &plan {
        Some(Ok(p)) => p.item.clone(),
        _ => all.clone(),
    };
    let shown = with(|s| s.confirm.as_ref().and_then(|c| c.variant.clone())).or_else(|| plan.as_ref().and_then(|p| p.as_ref().ok()?.variant.clone()));
    let mut chosen = shown.clone();
    let mut ticked: Option<(String, bool)> = None;
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
            Some(Ok(Planned { plan: p, template, optional, .. })) => {
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
                if !optional.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(tl!("Optional files, installed only when ticked:")).color(t.text_dim))
                        .on_hover_text(tl!("LoRAs and a template's prompt enhancer (a language model that rewrites prompts) aren't needed to run the model"));
                    for f in optional.iter() {
                        let mut on = extras.contains(&f.name);
                        if widgets::checkbox(ui, &mut on, &f.name).changed() {
                            ticked = Some((f.name.clone(), on));
                        }
                    }
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
            if let Some(v) = chosen.filter(|v| Some(v) != shown.as_ref()) {
                c.choose(&v);
            }
            if let Some((name, on)) = ticked {
                c.tick(&name, on);
            }
        }
        if let Some((p, template)) = go {
            if !p.files.is_empty() || template.is_some() {
                s.installs.insert(item.id.clone(), start_install(item.clone(), p, template, &AiSettings::load(), std::thread::Builder::spawn));
            }
            s.confirm = None;
        } else if close {
            s.confirm = None;
        }
    });
}

#[cfg(test)]
mod spawn_tests {
    use super::super::spawn_tests::fail_spawn;
    use super::*;

    #[test]
    fn plan_thread_failure_completes_with_error() {
        let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(crate::ai_ui::EngineStatus::default()));
        let slot = start_plan(Item::default(), None, Vec::new(), fail_spawn);
        assert!(matches!(&*slot.lock().unwrap(), Some(Err(error)) if error == "worker thread unavailable"), "planning must leave the loading state");
    }

    #[test]
    fn install_thread_failure_finishes_with_error() {
        let plan = Plan::default();
        let run = start_install(Item::default(), plan, None, &AiSettings::default(), fail_spawn);
        let run = run.lock().unwrap();
        assert!(run.finished, "installation must leave the progress state");
        assert_eq!(run.error.as_deref(), Some("worker thread unavailable"));
        assert!(run.note.is_none());
        assert_eq!(run.done, 0);
    }
}
