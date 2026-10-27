//! One model's page: what it is (Identify), what it needs (Check, with fixes), its test renders
//! (Verify) and how to use it (Use).

use li_ai::modelsetup::issue::Severity;

use super::*;

/// Where "Check online" points: the newer ComfyUI releases a missing template ships with.
const COMFY_RELEASES: &str = "https://github.com/Comfy-Org/ComfyUI/releases";

pub(super) fn show(ui: &mut egui::Ui, s: &mut State, t: &Tokens, out: &mut Vec<Action>) {
    let (Some(scan), Some(key)) = (s.scan.clone(), s.selected.clone()) else { return };
    let Some(m) = scan.server.models.get(&key) else {
        ui.label(RichText::new(tl!("Choose a model on the left.")).color(t.text_dim));
        return;
    };
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (icon, text, tone) = chip(m.status);
            widgets::status_chip(ui, icon, text, tone);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(&m.label).font(crate::theme::semibold(15.0)).color(t.text)).truncate()).on_hover_text(&m.label);
            });
        });
    });
    let mut line = vec![m.file.clone()];
    if m.fingerprint.bytes > 0 {
        line.push(li_ai::download::human_bytes(m.fingerprint.bytes));
    }
    if !m.folder.is_empty() {
        line.push(m.folder.clone());
    }
    ui.label(RichText::new(line.join(" · ")).size(11.5).color(t.text_faint));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        for step in Step::ALL {
            if widgets::pill_tab_translated(ui, step.label(), s.step == step).clicked() {
                s.step = step;
            }
        }
    });
    ui.add_space(4.0);
    widgets::hairline(ui);
    ui.add_space(6.0);
    match s.step {
        Step::Identify => identify(ui, s, &scan, &key, m, t),
        Step::Check => check(ui, s, &scan, &key, m, t, out),
        Step::Verify => verify(ui, s, &scan, &key, m, t, out),
        Step::Use => use_it(ui, s, &key, m, t, out),
    }
}

fn field(ui: &mut egui::Ui, t: &Tokens, name: &str, value: &str) {
    ui.label(RichText::new(name).color(t.text_faint));
    ui.add(egui::Label::new(RichText::new(value).color(t.text)).wrap());
    ui.end_row();
}

fn template_title(scan: &Scan, name: &str) -> String {
    scan.templates
        .iter()
        .find(|x| x.name == name)
        .map(|x| x.title.clone())
        .filter(|x| !x.is_empty())
        .unwrap_or_else(|| modelsetup::display_words(name.trim_start_matches("image_")))
}

fn capability_key(c: &str) -> &'static str {
    match c {
        "create" => "Create",
        "edit" => "Edit",
        "references" => "References",
        "style" => "Style references",
        "refine" => "Refine",
        "inpaint" => "Fill",
        "upscale" => "Upscale",
        "prompt_enhancer" => "Prompt enhancer",
        _ => "Other",
    }
}

fn capability_name(c: &str) -> &'static str {
    li_i18n::t(capability_key(c))
}

fn identify(ui: &mut egui::Ui, s: &mut State, scan: &Scan, key: &str, m: &ModelState, t: &Tokens) {
    let id = m.identity.as_ref();
    egui::Grid::new("model-setup-identify").num_columns(2).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
        let kind = match id.map(|i| i.kind) {
            Some(li_ai::arch::FileKind::Checkpoint) => tl!("Checkpoint"),
            Some(li_ai::arch::FileKind::DiffusionModel) => tl!("Diffusion model"),
            Some(li_ai::arch::FileKind::Lora) => tl!("LoRA"),
            Some(li_ai::arch::FileKind::TextEncoder) => tl!("Text encoder"),
            Some(li_ai::arch::FileKind::Vae) => tl!("VAE"),
            _ => tl!("Unknown"),
        };
        field(ui, t, tl!("Kind"), kind);
        let arch = id.and_then(|i| i.arch.as_deref());
        let mut what = arch
            .map(|a| li_ai::family::registry().family(a).map(|f| f.label.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| a.to_owned()))
            .unwrap_or_else(|| tl!("Not recognised").into());
        if let Some(g) = id.and_then(|i| i.vetoed.as_deref()) {
            what = li_i18n::fmt(tl!("Looks like {generation}"), &[("generation", g)]);
        }
        ui.label(RichText::new(tl!("Architecture")).color(t.text_faint));
        let response = ui.add(egui::Label::new(RichText::new(what).color(t.text)).wrap());
        if let Some(evidence) = id.map(|i| i.evidence.as_str()).filter(|e| !e.is_empty()) {
            response.on_hover_text(li_i18n::fmt(tl!("How Local Image knows: {evidence}"), &[("evidence", evidence)]));
        }
        ui.end_row();
        let quant = id.and_then(|i| i.quant.class.as_ref()).map(|q| q.label()).unwrap_or_else(|| tl!("Unknown").into());
        let precision = if m.fingerprint.bytes > 0 { format!("{quant} ({})", li_ai::download::human_bytes(m.fingerprint.bytes)) } else { quant };
        field(ui, t, tl!("Precision"), &precision);
        let engine = match m.engine.as_str() {
            "template" => tl!("Official ComfyUI templates"),
            "builtin" => tl!("Built-in support"),
            "builder" => tl!("Local Image's own recipe"),
            "universal" => tl!("Refine only (any image model)"),
            _ => tl!("Nothing yet"),
        };
        field(ui, t, tl!("Runs through"), engine);
        for (cap, name) in &m.templates {
            field(ui, t, capability_name(cap), &template_title(scan, name));
        }
        if let Some(choice) = &m.user.template_choice {
            field(ui, t, tl!("Your choice"), &template_title(scan, choice));
        }
    });
    if let Some(src) = m.recipe.as_ref().and_then(|r| r.sources.iter().find(|x| x.tier == "stem" && x.declared_main != m.file)) {
        ui.label(
            RichText::new(li_i18n::fmt(tl!("Same model as the template's {file} (different precision)."), &[("file", &src.declared_main)])).color(t.text_dim),
        );
    }
    if matches!(m.engine.as_str(), "template" | "unsupported") && !scan.templates.is_empty() {
        ui.add_space(6.0);
        if ui
            .add_enabled_ui(!s.busy(), |ui| widgets::secondary_button_translated(ui, tl!("Change template…"), 0.0))
            .inner
            .on_disabled_hover_text(tl!("Wait for the current task to finish."))
            .on_hover_text(tl!("Run this file through another official template"))
            .clicked()
        {
            s.choose_template = Some(ChooseTemplate { key: key.to_owned(), ..Default::default() });
        }
    }
}

fn severity_status(sev: Severity) -> (&'static str, &'static str, Tone) {
    match sev {
        Severity::Blocker => ("x", tl!("Needs attention"), Tone::Danger),
        Severity::Warning => ("triangle-alert", tl!("Warning"), Tone::Warning),
        Severity::Info => ("info", tl!("Information"), Tone::Neutral),
    }
}

/// One finding with its fixes.
fn issue_row(ui: &mut egui::Ui, s: &mut State, scan: &Scan, key: &str, i: &Issue, t: &Tokens, out: &mut Vec<Action>) {
    let (icon, status, tone) = severity_status(i.severity);
    ui.horizontal_top(|ui| {
        widgets::status_icon(ui, icon, status, tone);
        ui.vertical(|ui| {
            ui.add(egui::Label::new(RichText::new(sentence(i)).color(if i.severity == Severity::Info { t.text_dim } else { t.text })).wrap()).widget_info(
                || {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Label,
                        true,
                        li_i18n::fmt(tl!("{status}: {issue}"), &[("status", status), ("issue", &sentence(i))]),
                    )
                },
            );
            let mut fixes: Vec<Fix> = i.fixes.clone();
            if i.code == "ReverifyNeeded" && !fixes.iter().any(|f| matches!(f, Fix::Verify { .. })) {
                fixes.push(Fix::Verify { model: key.to_owned() });
            }
            if matches!(i.code.as_str(), "NoTemplate" | "UnsupportedGeneration" | "TemplateUnusable") && !scan.templates.is_empty() {
                fixes.insert(0, Fix::ChooseTemplate { candidates: Vec::new() });
            }
            if !fixes.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for (n, f) in fixes.iter().enumerate() {
                        ui.push_id((key, &i.code, n), |ui| fix_button(ui, s, scan, key, f, out));
                    }
                });
            }
        });
    });
    ui.add_space(4.0);
}

fn fix_button(ui: &mut egui::Ui, s: &mut State, scan: &Scan, key: &str, f: &Fix, out: &mut Vec<Action>) {
    let busy = s.busy();
    let local = scan.snapshot.as_ref().is_some_and(|x| !x.remote);
    let size = |b: u64| li_ai::download::human_bytes(b);
    let (label, tip, enabled): (String, Option<&str>, bool) = match f {
        Fix::Download { bytes, .. } => (
            if *bytes > 0 { li_i18n::fmt(tl!("Download {size}"), &[("size", &size(*bytes))]) } else { tl!("Download…").into() },
            Some(tl!("Checks the file's size and SHA-256 before it is used")),
            local && !busy,
        ),
        Fix::UseInstead { installed, .. } => (li_i18n::fmt(tl!("Use {file} instead"), &[("file", installed)]), None, !busy),
        Fix::Move { to_folder, .. } => (li_i18n::fmt(tl!("Move to {folder}/"), &[("folder", to_folder)]), None, local && !busy),
        Fix::ChooseTemplate { .. } => (tl!("Choose template…").into(), None, !busy),
        Fix::OpenUrl { label, .. } => (li_i18n::t(label).to_owned(), None, true),
        Fix::CopyCommand { .. } => (tl!("Copy commands").into(), Some(tl!("Copies the commands; Local Image never runs them")), true),
        Fix::UpdateProfiles => (tl!("Update Model Profiles…").into(), Some(tl!("Opens the Model Browser, where profiles update")), true),
        Fix::CheckOnline => (tl!("Check online…").into(), Some(tl!("Opens ComfyUI's releases on github.com")), true),
        Fix::Recheck => (tl!("Re-check").into(), None, !busy),
        Fix::Verify { .. } => {
            (if s.model(key).is_some_and(|m| m.status == Status::NeedsReverify) { tl!("Verify again") } else { tl!("Verify…") }.into(), None, !busy)
        }
        Fix::TryAnyway => (tl!("Try anyway").into(), Some(tl!("Verification decides whether it runs on this GPU")), !busy),
        Fix::Ignore => (tl!("Ignore").into(), None, !busy),
    };
    let r = ui.add_enabled_ui(enabled, |ui| widgets::secondary_button_translated(ui, &label, 0.0)).inner;
    let reason = if busy {
        tl!("Wait for the current task to finish.")
    } else if scan.snapshot.is_none() {
        tl!("Start the AI engine first.")
    } else {
        tl!("The model folder is on another computer; install or move the file there.")
    };
    let r = r.on_disabled_hover_text(reason);
    let r = match tip {
        Some(tip) => r.on_hover_text(tip),
        None => r,
    };
    if !r.clicked() {
        return;
    }
    let key = key.to_owned();
    match f {
        Fix::Download { .. } => out.push(Action::PlanDownload(f.clone())),
        Fix::UseInstead { declared, installed } => {
            out.push(Action::Choose(key, Choice::UseInstead { declared: declared.clone(), installed: installed.clone() }))
        }
        Fix::Move { from, to_folder } => {
            let kind =
                s.model(&key).and_then(|m| m.bindings.iter().find(|b| b.used.as_deref() == Some(from.as_str()))).map(|b| b.role.clone()).unwrap_or_default();
            s.confirm_move = Some(MoveAsk { file: from.clone(), to_folder: to_folder.clone(), kind: modelsetup::issue::role_label(&kind, false) });
        }
        Fix::ChooseTemplate { .. } => s.choose_template = Some(ChooseTemplate { key, ..Default::default() }),
        Fix::UpdateProfiles => out.push(Action::OpenModelBrowser),
        Fix::OpenUrl { url, .. } => out.push(Action::OpenUrl(url.clone())),
        Fix::CopyCommand { command, .. } => out.push(Action::Copy(command.clone())),
        Fix::CheckOnline => out.push(Action::OpenUrl(COMFY_RELEASES.into())),
        Fix::Recheck => out.push(Action::Recheck),
        Fix::Verify { .. } => s.step = Step::Verify,
        Fix::TryAnyway => out.push(Action::Choose(key, Choice::TryAnyway(true))),
        Fix::Ignore => out.push(Action::Choose(key, Choice::Ignore(true))),
    }
}

fn check(ui: &mut egui::Ui, s: &mut State, scan: &Scan, key: &str, m: &ModelState, t: &Tokens, out: &mut Vec<Action>) {
    let mut issues: Vec<&Issue> = scan.issues.iter().filter(|i| i.model.is_none()).chain(m.issues.iter()).collect();
    issues.sort_by_key(|i| i.severity);
    let problems = issues.iter().filter(|i| i.severity != Severity::Info).count();
    if problems == 0 {
        ui.horizontal(|ui| {
            widgets::status_icon(ui, "check", tl!("All checks passed"), Tone::Good);
            ui.label(RichText::new(tl!("Nothing is in the way.")).color(t.text));
        });
        ui.add_space(4.0);
    }
    for i in &issues {
        issue_row(ui, s, scan, key, i, t, out);
    }
    let mut passed: Vec<String> = Vec::new();
    if let Some(sn) = &scan.snapshot {
        passed.push(li_i18n::fmt(tl!("ComfyUI {version}"), &[("version", &sn.version_label())]));
    }
    for b in m.bindings.iter().filter(|b| b.role != "main" && !b.optional) {
        let Some(used) = &b.used else { continue };
        let role = modelsetup::issue::role_label(&b.role, false);
        let mut letters = role.chars();
        let role = letters.next().map(|first| first.to_uppercase().collect::<String>() + letters.as_str()).unwrap_or_default();
        let role = li_i18n::t(&role);
        let mut line = li_i18n::fmt(tl!("{role}: {file}"), &[("role", role), ("file", used)]);
        if *used != b.declared {
            line.push_str(&li_i18n::fmt(tl!(" (the template names {file})"), &[("file", &b.declared)]));
        }
        passed.push(line);
    }
    let ok: Vec<&str> = m.capabilities.iter().filter(|c| c.ok).map(|c| capability_name(&c.name)).collect();
    if !ok.is_empty() {
        passed.push(li_i18n::fmt(tl!("Builds: {list}"), &[("list", &ok.join(" · "))]));
    }
    if passed.is_empty() {
        return;
    }
    let title = if problems == 0 { tl!("All checks passed") } else { tl!("What passed") };
    egui::CollapsingHeader::new(RichText::new(title).color(t.text_dim)).id_salt(("model-setup-passed", key)).default_open(problems == 0).show(ui, |ui| {
        for p in passed {
            ui.horizontal(|ui| {
                widgets::status_icon(ui, "check", tl!("Check passed"), Tone::Good);
                ui.add(egui::Label::new(RichText::new(p).color(t.text_dim)).wrap());
            });
        }
    });
}

/// Count only capabilities that submit a render, including optional installed graphs.
fn renders(m: &ModelState) -> usize {
    let base = m.capabilities.iter().filter(|c| c.ok && matches!(c.name.as_str(), "create" | "edit" | "refine" | "inpaint")).count();
    let references = usize::from(m.recipe.is_some() && m.capabilities.iter().any(|c| c.ok && matches!(c.name.as_str(), "references" | "style")));
    let enhancer = usize::from(m.capabilities.iter().any(|c| c.ok && c.name == "prompt_enhancer"));
    (base + references + enhancer).max(1)
}

fn result_text(m: &ModelState, cap: &str, r: &CapabilityResult) -> String {
    if r.via.is_some() {
        return tl!("uses Refine").into();
    }
    let mut parts = vec![li_i18n::fmt(tl!("{seconds} s"), &[("seconds", &format!("{:.1}", r.seconds))])];
    if r.cold {
        parts.push(tl!("first run, includes loading").into());
    }
    if cap == "refine"
        && let Some(d) = r.difference
    {
        parts.push(li_i18n::fmt(tl!("changed {difference} of 255 on average"), &[("difference", &format!("{d:.1}"))]));
    }
    let mut text = parts.join(" · ");
    let explain = |code: &str| m.issues.iter().find(|i| i.code == code && i.args.get("capability").is_none_or(|c| c == capability_key(cap))).map(sentence);
    if let Some(code) = &r.failure {
        text.push('\n');
        text.push_str(&explain(code).or_else(|| r.message.clone()).unwrap_or_else(|| code.clone()));
    }
    for w in &r.warnings {
        if let Some(x) = explain(w) {
            text.push('\n');
            text.push_str(&x);
        }
    }
    text
}

fn verify(ui: &mut egui::Ui, s: &mut State, scan: &Scan, key: &str, m: &ModelState, t: &Tokens, out: &mut Vec<Action>) {
    let task = s.task.clone();
    let running = task.as_ref().and_then(|x| x.lock().ok().map(|x| (x.verifying.contains(&key.to_owned()), x.current.clone(), x.steps.clone())));
    if let Some((true, current, steps)) = running {
        let here: Vec<&(String, String, CapabilityResult)> = steps.iter().filter(|(k, _, _)| k == key).collect();
        let label = if current.as_deref() == Some(key) { tl!("Testing this model…") } else { tl!("Waiting for the model before it…") };
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new(label).color(t.text_dim));
            if widgets::secondary_button_translated(ui, tl!("Cancel"), 0.0).clicked() {
                out.push(Action::Cancel);
            }
        });
        egui::Grid::new("model-setup-live-results").num_columns(3).show(ui, |ui| {
            for (_, cap, r) in here {
                result_row(ui, m, cap, r, t);
            }
        });
        return;
    }
    let connected = scan.snapshot.is_some();
    let can = m.status.usable() || m.status == Status::Failed;
    let reason = if !connected {
        Some(tl!("Start the AI engine first."))
    } else if !can {
        Some(tl!("Fix what Check found first."))
    } else if s.busy() {
        Some(tl!("Wait for the current task to finish."))
    } else {
        None
    };
    let label = if m.verified.is_some() {
        li_i18n::trn(li_i18n::current(), renders(m) as u64, "Verify again · {n} short test render", "Verify again · {n} short test renders")
    } else {
        li_i18n::trn(li_i18n::current(), renders(m) as u64, "Verify · {n} short test render", "Verify · {n} short test renders")
    };
    let r = ui.add_enabled_ui(reason.is_none(), |ui| widgets::primary_button_translated(ui, &label, 0.0)).inner;
    let r = match reason {
        Some(why) => r.on_disabled_hover_text(why),
        None => r.on_hover_text(tl!("Each test renders at most one megapixel; ComfyUI frees the GPU afterwards")),
    };
    if r.clicked() {
        out.push(Action::Verify(vec![key.to_owned()]));
    }
    ui.add_space(8.0);
    let Some(v) = &m.verified else {
        ui.label(RichText::new(tl!("Not tested on this computer yet.")).color(t.text_dim));
        return;
    };
    egui::Grid::new("model-setup-results").num_columns(3).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
        for cap in ["create", "edit", "references", "style", "refine", "inpaint", "upscale", "prompt_enhancer"] {
            if let Some(r) = v.capabilities.get(cap) {
                result_row(ui, m, cap, r, t);
            }
        }
    });
    ui.add_space(4.0);
    let peak = v.capabilities.values().filter_map(|c| c.peak_vram_gb).reduce(f64::max);
    if let Some(peak) = peak {
        let total = scan.snapshot.as_ref().and_then(|x| x.gpu.as_ref()).map(|g| g.vram_total as f64 / 1e9).unwrap_or(0.0);
        ui.label(
            RichText::new(li_i18n::fmt(
                tl!("GPU memory in use during the test: up to {used} GB of {total} GB"),
                &[("used", &format!("{peak:.1}")), ("total", &format!("{total:.1}"))],
            ))
            .color(t.text_dim),
        );
    }
    ui.label(
        RichText::new(li_i18n::fmt(
            tl!("Checked on ComfyUI {version} · {date}"),
            &[("version", &v.comfyui_version), ("date", &li_i18n::dates::display_time(&v.at))],
        ))
        .size(11.5)
        .color(t.text_faint),
    )
    .on_hover_text(li_i18n::fmt(
        tl!("Checked on ComfyUI {version} · {date} · seed {seed}"),
        &[("version", &v.comfyui_version), ("date", &v.at.replacen('T', " ", 1)), ("seed", &v.seed.to_string())],
    ));
}

fn result_row(ui: &mut egui::Ui, m: &ModelState, cap: &str, r: &CapabilityResult, t: &Tokens) {
    let (icon, tone) = if r.pass { ("check", Tone::Good) } else { ("x", Tone::Danger) };
    let text = result_text(m, cap, r);
    let outcome = if r.pass {
        li_i18n::fmt(tl!("{capability} passed: {result}"), &[("capability", capability_name(cap)), ("result", &text)])
    } else {
        li_i18n::fmt(tl!("{capability} failed: {reason}"), &[("capability", capability_name(cap)), ("reason", &text)])
    };
    let icon_label = li_i18n::fmt(if r.pass { tl!("{capability} passed") } else { tl!("{capability} failed") }, &[("capability", capability_name(cap))]);
    widgets::status_icon(ui, icon, &icon_label, tone);
    ui.label(RichText::new(capability_name(cap)).color(t.text));
    ui.add(egui::Label::new(RichText::new(text).color(t.text_dim)).wrap()).widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &outcome));
    ui.end_row();
}

fn use_it(ui: &mut egui::Ui, s: &mut State, key: &str, m: &ModelState, t: &Tokens, out: &mut Vec<Action>) {
    if !m.status.usable() {
        ui.label(RichText::new(li_i18n::fmt(tl!("{model} can't be used yet."), &[("model", &m.label)])).color(t.text));
        if let Some(i) = m.issues.iter().filter(|i| i.severity != Severity::Info).min_by_key(|i| i.severity) {
            ui.add(egui::Label::new(RichText::new(sentence(i)).color(t.text_dim)).wrap());
        }
        if widgets::secondary_button_translated(ui, tl!("Go to Check"), 0.0).clicked() {
            s.step = Step::Check;
        }
        return;
    }
    let ready = if m.status == Status::Ready { tl!("{model} is ready (not tested yet).") } else { tl!("{model} is ready.") };
    ui.label(RichText::new(li_i18n::fmt(ready, &[("model", &m.label)])).font(crate::theme::semibold(13.0)).color(t.text));
    let mut caps: Vec<String> = Vec::new();
    for c in m.capabilities.iter().filter(|c| c.ok) {
        caps.push(match c.name.as_str() {
            "edit" | "references" | "style" if c.references > 0 => li_i18n::fmt(
                &li_i18n::trn(li_i18n::current(), c.references as u64, "{capability} (up to {n} reference)", "{capability} (up to {n} references)"),
                &[("capability", capability_name(&c.name))],
            ),
            name => capability_name(name).to_owned(),
        });
    }
    let catalog_key = if m.catalog_id.is_empty() { key } else { m.catalog_id.as_str() };
    if li_ai::catalog::ModelId::from_key(catalog_key).and_then(|id| id.try_info()).is_some_and(|i| i.lora) {
        caps.push(tl!("Styles (LoRA)").into());
    }
    if !caps.is_empty() {
        ui.label(RichText::new(caps.join(" · ")).color(t.text_dim));
    }
    if let Some(i) = m.issues.iter().find(|i| matches!(i.code.as_str(), "LicenceNotice" | "LicenceUnknown")) {
        ui.add(egui::Label::new(RichText::new(sentence(i)).size(11.5).color(t.text_faint)).wrap());
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if widgets::primary_button_translated(ui, tl!("Use in Generate"), 0.0).clicked() {
            out.push(Action::UseInGenerate(catalog_key.to_owned()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{FLUX3, fixture};
    use super::*;

    #[test]
    fn template_titles_preserve_model_acronyms() {
        let scan = fixture();
        assert_eq!(template_title(&scan, "image_flux3_lora_sft_vae"), "Flux3 LoRA SFT VAE");
        assert_eq!(template_title(&scan, "image_flux3_klein_distilled_t2i"), "FLUX 3 Image Klein Distilled: Text to Image");
    }

    #[test]
    fn each_failed_tool_gets_its_own_failure_sentence() {
        let mut m = fixture().server.models[FLUX3].clone();
        m.issues = ["Create", "Fill"]
            .iter()
            .map(|cap| Issue::new("VerifyFailed:noise", Some(FLUX3), &[("capability", (*cap).into()), ("vae", "vae.safetensors".into())]))
            .collect();
        let result = CapabilityResult { failure: Some("VerifyFailed:noise".into()), ..Default::default() };
        assert!(result_text(&m, "create", &result).contains("The Create test image is noise"));
        assert!(result_text(&m, "inpaint", &result).contains("The Fill test image is noise"));
    }
}
