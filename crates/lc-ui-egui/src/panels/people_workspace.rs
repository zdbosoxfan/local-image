//! Shared People controls for the Library and Smart Sort. Queries stay separate from inference,
//! and the focused confirmation queue uses the engine's durable rejection and undo history.
use crate::{
    LightcraftApp,
    i18n::tr,
    smart_sort_keys::{self, QueueAction},
    theme::Tokens,
    widgets::register,
};
use egui::{Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use lightcraft_catalog::PhotoId;
use lightcraft_engine::smart_sort::FolderDef;
use lightcraft_engine::smart_sort::people::{self, PeopleData};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug)]
struct PeopleQuery {
    revision: u64,
    face_revision: u64,
    model: String,
    before: PeopleData,
    cancel: Arc<AtomicBool>,
    result: Arc<Mutex<Option<Result<PeopleData, String>>>>,
}

/// A test can hold the real query worker pending without relying on clustering
/// taking longer than the UI frames on a particular machine.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct QueryTestGate {
    pub entered: std::sync::mpsc::Sender<std::thread::ThreadId>,
    pub resume: Mutex<std::sync::mpsc::Receiver<()>>,
    pub completed: std::sync::mpsc::Sender<bool>,
}

#[derive(Clone, Debug)]
pub struct PeopleUi {
    pub refresh: bool,
    pub(crate) person_folder_enabled: bool,
    query: Option<PeopleQuery>,
    #[cfg(test)]
    pub(crate) query_test_gate: Option<Arc<QueryTestGate>>,
    pub list: Value,
    pub revision: u64,
    pub scope: Vec<PhotoId>,
    pub names: BTreeMap<u64, String>,
    pub show_everyone: bool,
    pub show_all: bool,
    pub paste: bool,
    pub pasted: String,
    pub finder: Option<PhotoId>,
    pub picked: Option<usize>,
    pub find_name: String,
    pub queue: Option<ConfirmQueue>,
    pub merge: Option<(u64, u64)>,
    pub error: String,
    pub notice: String,
    pub filter_ids: Vec<u64>,
    pub filter_everyone: bool,
    pub clear: bool,
    pub remove_names: bool,
}
impl Default for PeopleUi {
    fn default() -> Self {
        Self {
            refresh: true,
            person_folder_enabled: false,
            query: None,
            #[cfg(test)]
            query_test_gate: None,
            list: Value::Null,
            revision: u64::MAX,
            scope: Vec::new(),
            names: BTreeMap::new(),
            show_everyone: false,
            show_all: false,
            paste: false,
            pasted: String::new(),
            finder: None,
            picked: None,
            find_name: String::new(),
            queue: None,
            merge: None,
            error: String::new(),
            notice: String::new(),
            filter_ids: Vec::new(),
            filter_everyone: false,
            clear: false,
            remove_names: false,
        }
    }
}
#[derive(Clone, Debug)]
pub struct ConfirmQueue {
    pub person: u64,
    pub skipped: BTreeSet<(PhotoId, usize)>,
    pub cursor: usize,
    pub all: bool,
    pub history: Vec<(BTreeSet<(PhotoId, usize)>, usize, bool)>,
    pub rows: Value,
    pub revision: u64,
}
fn button(ui: &mut Ui, id: impl Into<String>, label: &str) -> egui::Response {
    let id = id.into();
    let r = crate::widgets::text_button(ui, &id, tr(label), false);
    register(ui.ctx(), id, r.rect);
    r
}
fn check(ui: &mut Ui, id: impl Into<String>, on: &mut bool, label: &str) -> bool {
    let r = ui.checkbox(on, tr(label));
    register(ui.ctx(), id, r.rect);
    r.changed()
}
fn command(app: &mut LightcraftApp, s: &mut PeopleUi, id: &str, p: Value) -> Option<Value> {
    match app.session.execute(id, &p) {
        Ok(v) => {
            if matches!(id, "edit.undo" | "edit.redo" | "smartSort.mergePeople") {
                s.names.clear();
            }
            if id == "smartSort.personFolder" && p["enabled"] == true {
                // Smart Sort owns its dialog while drawing these controls. Carry the
                // explicit action back without overriding a later unchecked option.
                s.person_folder_enabled = true;
            }
            s.refresh = true;
            Some(v)
        }
        Err(e) => {
            s.error = e.to_string();
            None
        }
    }
}
pub fn face_options(app: &mut LightcraftApp, ui: &mut Ui) {
    let mut enabled = app.session.smart.prefs.faces_enabled;
    let status = app.session.execute("smartSort.status", &json!({})).unwrap_or_default();
    let installed = status["faces"]["installed"] == true;
    let busy = app.smart_sort.is_some() || app.people_task.is_some();
    let r = ui.add_enabled(!busy && installed, egui::Checkbox::new(&mut enabled, tr("Also sort by person (face recognition)")));
    register(ui.ctx(), "smartSort:faces", r.rect);
    if r.changed()
        && let Err(e) = app.session.execute("smartSort.facesEnable", &json!({"enabled":enabled}))
    {
        app.ui.people.error = e.to_string();
    }
    let t = Tokens::get(ui.ctx());
    ui.label(
        egui::RichText::new(tr(
            "Finds faces and groups photos of the same person. Face data is stored only in this library and can be cleared at any time.",
        ))
        .font(t.font(11.5))
        .color(t.text_dim),
    );
    if !installed {
        let download = app.services.model_download_status.as_ref().map(|f| f("yunet")).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(tr("Download face models (39 MB)"));
            if download.running {
                ui.add(egui::ProgressBar::new(download.done as f32 / download.total.max(1) as f32).desired_width(180.0));
                if button(ui, "smartSort:facesDownloadCancel", "Cancel download").clicked()
                    && let Some(f) = &app.services.download_models
                {
                    let _ = f("yunet", true);
                }
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            } else {
                let r = ui.add_enabled(app.services.download_models.is_some(), egui::Button::new(tr("Download")));
                register(ui.ctx(), "smartSort:facesDownload", r.rect);
                if r.clicked()
                    && let Some(f) = &app.services.download_models
                    && let Err(e) = f("yunet", false)
                {
                    app.ui.people.error = e;
                }
            }
        });
        if let Some(e) = download.error {
            ui.label(e);
        }
        ui.horizontal(|ui| {
            ui.hyperlink_to(tr("YuNet · MIT"), "https://huggingface.co/opencv/face_detection_yunet");
            ui.hyperlink_to(tr("SFace · Apache-2.0"), "https://huggingface.co/opencv/face_recognition_sface");
        });
    }
    if let Some(task) = &app.people_task {
        let (done, total) = task.job.status().step.unwrap_or_default();
        ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(crate::i18n::tr_format!(
            "Faces: {done} of {total}",
            done = done,
            total = total
        )));
        if button(ui, "smartSort:facesCancel", "Cancel face analysis").clicked() {
            task.cancel();
        }
    }
}
fn refresh(app: &mut LightcraftApp, s: &mut PeopleUi, scope: &[PhotoId]) {
    let mut clustered = false;
    if let Some(query) = s.query.take() {
        if !app.session.smart.prefs.faces_enabled
            || query.revision != app.session.catalog.revision
            || query.before != app.session.smart.people.data
            || query.face_revision != app.session.smart.people.store.revision()
            || query.model != app.session.smart.people.model_id()
        {
            query.cancel.store(true, Ordering::Relaxed);
        } else {
            let result = query.result.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
            if let Some(result) = result {
                match result.and_then(|data| app.session.commit_people("Smart Sort: suggest people", data).map_err(|e| e.to_string())) {
                    Ok(()) => clustered = true,
                    Err(e) => {
                        s.error = e;
                        s.refresh = false;
                        s.revision = app.session.catalog.revision;
                        s.scope = scope.to_vec();
                        return;
                    }
                }
            } else {
                s.query = Some(query);
                return;
            }
        }
    }
    if !clustered && !s.refresh && s.revision == app.session.catalog.revision && s.scope == scope {
        return;
    }
    if !app.session.smart.prefs.faces_enabled {
        return;
    }
    if let Err(e) = app.session.smart.people.ready() {
        s.error = e;
        return;
    }
    if !clustered && app.session.smart.people.store.entries(app.session.smart.people.model_id()).map(|(_, f)| f.len()).sum::<usize>() >= 256 {
        let query = PeopleQuery {
            revision: app.session.catalog.revision,
            face_revision: app.session.smart.people.store.revision(),
            model: app.session.smart.people.model_id().to_owned(),
            before: app.session.smart.people.data.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            result: Arc::new(Mutex::new(None)),
        };
        let (cat, engine) = (app.session.catalog.clone(), app.session.smart.people.snapshot());
        let (cancel, result, mut data) = (query.cancel.clone(), query.result.clone(), query.before.clone());
        #[cfg(test)]
        let gate = s.query_test_gate.take();
        let work = move || {
            #[cfg(test)]
            if let Some(gate) = &gate {
                let _ = gate.entered.send(std::thread::current().id());
                // Disconnection releases a worker after a test panic. The
                // watchdog also bounds a regression that executes work inline.
                let _ = gate.resume.lock().unwrap_or_else(std::sync::PoisonError::into_inner).recv_timeout(std::time::Duration::from_secs(120));
            }
            let value = people::refresh_cancellable(&cat, &engine, &mut data, true, &|| cancel.load(Ordering::Relaxed)).map(|()| data);
            *result.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(value);
            #[cfg(test)]
            if let Some(gate) = &gate {
                let _ = gate.completed.send(cancel.load(Ordering::Relaxed));
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Err(e) = std::thread::Builder::new().name("lc-people-query".into()).spawn(work) {
            s.error = e.to_string();
            return;
        }
        #[cfg(target_arch = "wasm32")]
        work();
        s.query = Some(query);
        return;
    }
    if let Some(list) = command(
        app,
        s,
        "smartSort.people",
        json!({
            "folderIds":scope,"showEveryone":s.show_everyone,"limit":if s.show_all {usize::MAX} else {60},"cluster":!clustered
        }),
    ) {
        s.list = list;
        s.revision = app.session.catalog.revision;
        s.scope = scope.to_vec();
        s.refresh = false;
    }
}
/// Draw round crops using the Library's asynchronous renderer, clipped by a textured fan.
fn crop(app: &mut LightcraftApp, ui: &mut Ui, photo: PhotoId, bounds: [f32; 4], r: Rect, round: bool) {
    let t = Tokens::get(ui.ctx());
    if round {
        ui.painter().circle_filled(r.center(), r.width() / 2., t.canvas);
    } else {
        ui.painter().rect_filled(r, 5., t.canvas);
    }
    let [x, y, w, h] = bounds.map(f64::from);
    let bounds = lightcraft_meta::Rect::from_xywh(x, y, w, h);
    if let Some(job) = app.session.face_job(photo, bounds, (r.width() * ui.ctx().pixels_per_point()).ceil() as usize)
        && let Some(tex) = app.renderer.variant(job)
    {
        if round {
            paint_round(ui, r, tex.tex.id());
        } else {
            ui.painter().image(tex.tex.id(), r, Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)), egui::Color32::WHITE);
        }
    }
}
fn folder_has(app: &LightcraftApp, folder: &FolderDef, id: u64) -> bool {
    folder.person_ids.iter().any(|p| app.session.smart.people.data.resolve_id(*p) == id)
}
fn folder_select(app: &LightcraftApp, folder: &mut FolderDef, id: u64, on: bool) {
    if on {
        if !folder_has(app, folder, id) {
            folder.person_ids.push(id);
        }
    } else {
        folder.person_ids.retain(|p| app.session.smart.people.data.resolve_id(*p) != id);
    }
}

pub fn show(app: &mut LightcraftApp, ui: &mut Ui, scope: &[PhotoId], mut folder: Option<&mut FolderDef>) {
    let mut s = std::mem::take(&mut app.ui.people);
    refresh(app, &mut s, scope);
    if s.query.is_some() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(tr("Grouping people…"));
            if button(ui, "people:groupCancel", "Cancel").clicked() {
                if let Some(query) = s.query.take() {
                    query.cancel.store(true, Ordering::Relaxed);
                }
                s.refresh = false;
                s.revision = app.session.catalog.revision;
                s.scope = scope.to_vec();
            }
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }

    ui.horizontal(|ui| {
        if button(ui, "smartSort:seedHeadshots", "Add People from Photos…").clicked()
            && let Some(pick) = &mut app.services.pick_folder
            && let Some(dir) = pick()
            && let Err(e) = crate::people_task::start(app, &[], Some(dir))
        {
            s.error = e;
        }
        if button(ui, "smartSort:pasteNames", "Paste names…").clicked() {
            s.paste = true;
        }
        ui.label(tr("Minimum photos"));
        let mut min = s.list["minPhotos"].as_u64().unwrap_or(3) as u8;
        let r = ui.add(egui::DragValue::new(&mut min).range(1..=10));
        register(ui.ctx(), "smartSort:minFaces", r.rect);
        if r.changed() {
            command(app, &mut s, "smartSort.people", json!({"minPhotos":min,"cluster":false}));
        }
        if button(ui, "smartSort:peopleUndo", "Undo").clicked() {
            command(app, &mut s, "edit.undo", json!({}));
        }
    });
    if s.paste {
        ui.label(tr("Paste one name[,title] per line"));
        let r = ui.add(egui::TextEdit::multiline(&mut s.pasted).desired_rows(3).desired_width(450.));
        register(ui.ctx(), "smartSort:namesText", r.rect);
        ui.horizontal(|ui| {
            if button(ui, "smartSort:namesApply", "Add names").clicked() {
                let pasted = s.pasted.clone();
                if command(app, &mut s, "smartSort.peoplePasteNames", json!({"text":pasted})).is_some() {
                    s.paste = false;
                }
            }
            if button(ui, "smartSort:namesCancel", "Cancel").clicked() {
                s.paste = false;
            }
        });
    }
    let rows = s.list["people"].as_array().cloned().unwrap_or_default();
    let t = Tokens::get(ui.ctx());
    if rows.is_empty() {
        ui.label(tr("No people to show. Analyse photos or add headshots."));
    }
    let cols = (ui.available_width() / 184.).floor().max(1.) as usize;
    egui::Grid::new("people-bubbles").spacing([12., 12.]).show(ui, |ui| {
        for (index, row) in rows.iter().enumerate() {
            let Some(id) = row["id"].as_u64() else { continue };
            ui.push_id(id, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(172.);
                    let (r, resp) = ui.allocate_exact_size(vec2(86., 86.), Sense::click_and_drag());
                    register(ui.ctx(), format!("smartSort:bubble:{id}"), r);
                    if let Some(photo) = row["cover"]["photo"].as_u64() {
                        let bounds = serde_json::from_value(row["cover"]["rect"].clone()).unwrap_or([0., 0., 1., 1.]);
                        crop(app, ui, PhotoId(photo), bounds, r, true);
                    } else if let Some(source) = row["cover"]["source"].as_str() {
                        let bounds = serde_json::from_value(row["cover"]["rect"].clone()).unwrap_or([0., 0., 1., 1.]);
                        if let Some(tex) = app.people_images.request(ui.ctx(), source, bounds) {
                            paint_round(ui, r, tex);
                        }
                    } else {
                        ui.painter().circle_filled(r.center(), 43., t.canvas);
                    }
                    if row["pinned"] == true || folder.as_deref().is_some_and(|f| folder_has(app, f, id)) {
                        ui.painter().circle_stroke(r.center(), 44., Stroke::new(2., t.accent));
                    }
                    ui.painter().text(r.right_bottom(), egui::Align2::RIGHT_BOTTOM, row["count"].as_u64().unwrap_or(0), t.semibold(13.), t.text);
                    let pending = row["pending"].as_u64().unwrap_or(0);
                    if pending > 0 {
                        let badge = Rect::from_min_size(r.right_top() + vec2(-12., -2.), vec2(38., 22.));
                        ui.painter().rect_filled(badge, 11., t.field);
                        ui.painter().text(badge.center(), egui::Align2::CENTER_CENTER, format!("? {pending}"), t.semibold(11.), t.text);
                        let review = ui.interact(badge, ui.id().with("pending"), Sense::click());
                        register(ui.ctx(), format!("smartSort:pending:{id}"), badge);
                        if review.clicked() {
                            open_queue(app, &mut s, id);
                        }
                    }
                    resp.dnd_set_drag_payload(id);
                    if let Some(other) = resp.dnd_release_payload::<u64>()
                        && *other != id
                    {
                        s.merge = Some((id, *other));
                    }
                    if resp.clicked() {
                        if let Some(f) = folder.as_deref_mut() {
                            let on = !folder_has(app, f, id);
                            folder_select(app, f, id, on);
                        } else {
                            s.filter_ids = vec![id];
                            apply_filter(app, &mut s);
                            let _ = app.run("view.photoGrid", json!({}));
                        }
                    }
                    let base_name = row["name"].as_str().unwrap_or("");
                    let draft_id = ui.id().with("person-name-draft");
                    let suspended_id = draft_id.with("suspended");
                    let owner = egui::Id::new((app.session.library_identity(), app.session.library_generation, id, base_name));
                    let changed_owner = ui.data_mut(|d| {
                        let previous = d.get_temp::<egui::Id>(draft_id);
                        d.insert_temp(draft_id, owner);
                        if previous.is_some_and(|previous| previous != owner) {
                            d.remove::<bool>(suspended_id);
                            true
                        } else {
                            false
                        }
                    });
                    if changed_owner {
                        s.names.remove(&id);
                    }
                    let name = s.names.entry(id).or_insert_with(|| base_name.into());
                    let name_r = ui.add(egui::TextEdit::singleline(name).hint_text(tr("Add name")).desired_width(165.));
                    register(ui.ctx(), format!("smartSort:bubbleName:{id}"), name_r.rect);
                    let edited = name.clone();
                    // Response focus access reacquires Context memory; snapshot it
                    // before the temporary-data write lock below.
                    let (has_focus, lost_focus) = (name_r.has_focus(), name_r.lost_focus());
                    // egui reports lost_focus again on the frame after a disabled
                    // field surrendered focus. Even a one-frame host modal must
                    // suspend publication until this name is deliberately refocused.
                    let suspended = ui.data_mut(|d| {
                        if app.host_modal {
                            d.insert_temp(suspended_id, true);
                        } else if has_focus {
                            d.remove::<bool>(suspended_id);
                        }
                        d.get_temp::<bool>(suspended_id).unwrap_or(false)
                    });
                    let owns_input = !app.host_modal && !suspended;
                    let save = owns_input && (lost_focus || (has_focus && ui.input(|i| i.key_pressed(egui::Key::Enter))));
                    let mut chose_suggestion = false;
                    if owns_input && (has_focus || lost_focus) && !edited.is_empty() {
                        for suggestion in s.list["nameSuggestions"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default()
                            .iter()
                            .filter(|n| n["name"].as_str().is_some_and(|n| n.to_lowercase().starts_with(&edited.to_lowercase())))
                        {
                            let n = suggestion["name"].as_str().unwrap_or("");
                            if button(ui, format!("smartSort:nameSuggestion:{id}:{n}"), n).clicked() {
                                chose_suggestion = true;
                                s.names.insert(id, n.into());
                                command(app, &mut s, "smartSort.namePerson", json!({"person":id,"name":n}));
                            }
                        }
                    }
                    if save && !chose_suggestion && !edited.trim().is_empty() && edited != row["name"].as_str().unwrap_or("") {
                        command(app, &mut s, "smartSort.namePerson", json!({"person":id,"name":edited}));
                    }
                    ui.label(crate::i18n::tr_format!("{folder} / {total} photos", folder = row["folderCount"], total = row["count"]));
                    let pending = row["pending"].as_u64().unwrap_or(0);
                    if pending > 0 {
                        ui.label(crate::i18n::tr_format!("? {n} matches to review", n = pending));
                    }
                    if button(ui, format!("smartSort:review:{id}"), "Review matches…").clicked() {
                        open_queue(app, &mut s, id);
                    }
                    if let Some(f) = folder.as_deref_mut() {
                        let mut on = folder_has(app, f, id);
                        if check(ui, format!("smartSort:folderBubble:{id}"), &mut on, "In this folder") {
                            folder_select(app, f, id, on);
                        }
                    } else {
                        let mut on = row["folderEnabled"] == true;
                        if check(ui, format!("smartSort:personFolder:{id}"), &mut on, "Create a folder for this person") {
                            command(app, &mut s, "smartSort.personFolder", json!({"person":id,"enabled":on}));
                        }
                    }
                    ui.horizontal(|ui| {
                        if button(ui, format!("smartSort:ignore:{id}"), "Ignore").clicked() {
                            command(app, &mut s, "smartSort.peopleIgnore", json!({"person":id}));
                        }
                        if button(ui, format!("smartSort:pin:{id}"), if row["pinned"] == true { "Unpin" } else { "Pin to top" }).clicked() {
                            command(app, &mut s, "smartSort.peoplePin", json!({"person":id,"enabled":row["pinned"]!=true}));
                        }
                    });
                    let menu = ui.menu_button(tr("Merge with…"), |ui| {
                        for other in &rows {
                            if let Some(other_id) = other["id"].as_u64().filter(|p| *p != id)
                                && button(ui, format!("smartSort:merge:{id}:{other_id}"), &display_name(other)).clicked()
                            {
                                s.merge = Some((id, other_id));
                                ui.close();
                            }
                        }
                    });
                    register(ui.ctx(), format!("smartSort:mergeMenu:{id}"), menu.response.rect);
                    if button(ui, format!("smartSort:split:{id}"), "Not the same person…").clicked() {
                        s.queue = Some(ConfirmQueue {
                            person: id,
                            skipped: BTreeSet::new(),
                            cursor: 0,
                            all: true,
                            history: Vec::new(),
                            rows: Value::Null,
                            revision: u64::MAX,
                        });
                    }
                });
            });
            if (index + 1) % cols == 0 {
                ui.end_row();
            }
        }
    });
    let hidden = s.list["hidden"].as_u64().unwrap_or(0);
    if hidden > 0 && button(ui, "smartSort:showEveryone", &crate::i18n::tr_format!("Show everyone ({n} more)", n = hidden)).clicked() {
        s.show_everyone = true;
        s.refresh = true;
    }
    if s.list["remaining"].as_u64().unwrap_or(0) > 0 && button(ui, "smartSort:showAll", "Show all").clicked() {
        s.show_all = true;
        s.refresh = true;
    }
    let ignored = s.list["ignored"].as_array().cloned().unwrap_or_default();
    if !ignored.is_empty() {
        let ignored_section = ui.collapsing(tr("Ignored"), |ui| {
            for row in ignored {
                let id = row["id"].as_u64().unwrap_or(0);
                if button(ui, format!("smartSort:restore:{id}"), &crate::i18n::tr_format!("Restore {name}", name = display_name(&row))).clicked() {
                    command(app, &mut s, "smartSort.peopleIgnore", json!({"person":id,"enabled":false}));
                }
            }
        });
        register(ui.ctx(), "smartSort:ignored", ignored_section.header_response.rect);
    }
    if !s.error.is_empty() {
        ui.colored_label(t.caution, &s.error);
    }
    if !s.notice.is_empty() {
        ui.label(&s.notice);
    }
    app.ui.people = s;
}
fn display_name(row: &Value) -> String {
    row["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| crate::i18n::tr_format!("Unnamed person {id}", id = row["id"]))
}
fn open_queue(app: &mut LightcraftApp, s: &mut PeopleUi, id: u64) {
    let id = app.session.smart.people.data.resolve_id(id);
    s.queue = Some(ConfirmQueue {
        person: id,
        skipped: BTreeSet::new(),
        cursor: 0,
        all: false,
        history: Vec::new(),
        rows: Value::Null,
        revision: u64::MAX,
    });
    if let Some(r) = command(app, s, "smartSort.findPerson", json!({"person":id,"queue":true}))
        && let Some(other) = r["probablySame"].as_array().and_then(|r| r.first()).and_then(|r| r["person"].as_u64())
    {
        s.merge = Some((id, other));
    }
}
pub fn finder(app: &mut LightcraftApp, photo: PhotoId) {
    app.ui.people.finder = Some(photo);
    app.ui.people.picked = None;
    app.ui.people.find_name.clear();
}
fn apply_filter(app: &mut LightcraftApp, s: &mut PeopleUi) {
    let _ = app.run("library.filter", json!({"person":null,"personIds":s.filter_ids,"peopleEveryone":s.filter_everyone}));
}
fn selected_names(app: &LightcraftApp) -> Vec<String> {
    app.session
        .filter
        .person_ids
        .iter()
        .map(|id| {
            let id = app.session.smart.people.data.resolve_id(*id);
            app.session
                .smart
                .people
                .data
                .person(id)
                .ok()
                .map(|p| p.name.clone())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| crate::i18n::tr_format!("Unnamed person {id}", id = id))
        })
        .collect()
}

/// The Library shows names for identity filters; the engine's JSON representation keeps IDs.
pub fn filter_chips(app: &LightcraftApp) -> Vec<lightcraft_engine::FilterChip> {
    let mut chips = lightcraft_engine::filter_chips(&app.session.filter, &app.session.catalog);
    for chip in &mut chips {
        if chip.clear.get("personIds").is_some() {
            chip.label = selected_names(app).join(if app.session.filter.people_everyone { " + " } else { ", " });
        }
    }
    chips
}

pub fn facet(app: &mut LightcraftApp, ui: &mut Ui) {
    let mut s = std::mem::take(&mut app.ui.people);
    s.filter_ids = app.session.filter.person_ids.clone();
    s.filter_everyone = app.session.filter.people_everyone;
    let names = selected_names(app);
    let label = if names.is_empty() { app.session.filter.person.clone().unwrap_or_else(|| tr("People").to_string()) } else { names.join(", ") };
    let label = if label.chars().count() > 22 { format!("{}…", label.chars().take(21).collect::<String>()) } else { label };
    let r = crate::widgets::dropdown(ui, "filter-person", &label, Tokens::get(ui.ctx()).font(12.5), Tokens::get(ui.ctx()).text_label);
    register(ui.ctx(), "filter:person", r.rect);
    egui::Popup::menu(&r).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        if button(ui, "filter:person:clear", "Any person").clicked() {
            s.filter_ids.clear();
            apply_filter(app, &mut s);
        }
        for person in app.session.smart.people.data.people.clone().iter().filter(|p| !p.ignored) {
            let label = if person.name.is_empty() { crate::i18n::tr_format!("Unnamed person {id}", id = person.id) } else { person.name.clone() };
            let mut on = s.filter_ids.contains(&person.id);
            if check(ui, format!("filter:person:{}", person.id), &mut on, &label) {
                if on {
                    s.filter_ids.push(person.id)
                } else {
                    s.filter_ids.retain(|p| *p != person.id)
                }
                apply_filter(app, &mut s);
            }
        }
        // Imported XMP names remain usable before opting into recognition.
        let imported: Vec<_> = app
            .session
            .catalog
            .people()
            .into_iter()
            .filter(|p| !app.session.smart.people.data.people.iter().any(|known| known.name.eq_ignore_ascii_case(&p.name)))
            .collect();
        for person in imported {
            if button(ui, format!("filter:personName:{}", person.name), &person.name).clicked() {
                s.filter_ids.clear();
                let _ = app.run("library.filter", json!({"person":person.name,"personIds":[]}));
            }
        }
        if check(ui, "filter:peopleEveryone", &mut s.filter_everyone, "Everyone in the photo together") {
            apply_filter(app, &mut s);
        }
        ui.label(tr("Any of these people"));
    });
    app.ui.people = s;
}
fn photo(app: &mut LightcraftApp, ui: &mut Ui, id: PhotoId, size: egui::Vec2) -> Rect {
    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(r, 5., Tokens::get(ui.ctx()).canvas);
    if let Some(p) = app.session.catalog.photo(id) {
        let natural = vec2(p.width.max(1) as f32, p.height.max(1) as f32);
        let scale = (r.width() / natural.x).min(r.height() / natural.y);
        let fit = Rect::from_center_size(r.center(), natural * scale);
        let mut settings = (*p.develop).clone();
        settings.crop = Default::default();
        settings.orientation = lightcraft_geom::Orientation::Normal;
        if let Some(job) = app.session.variant_job(id, &settings, size.x.max(size.y) as usize)
            && let Some(tex) = app.renderer.variant(job)
        {
            ui.painter().image(tex.tex.id(), fit, Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)), egui::Color32::WHITE);
        }
        return fit;
    }
    r
}
pub fn overlay(app: &mut LightcraftApp, ctx: &egui::Context) {
    // Keep the workspace pending while a separate dialog owns input.
    if app.ui.dialog.as_ref().is_some_and(|d| !matches!(d, crate::state::Dialog::SmartSort { .. })) {
        return;
    }
    if app.ui.people.queue.is_none() && app.ui.people.merge.is_none() && app.ui.people.finder.is_none() {
        return;
    }
    let mut s = std::mem::take(&mut app.ui.people);
    let screen = ctx.content_rect();
    let t = Tokens::get(ctx);
    let shown = egui::Window::new(tr("People"))
        .id(egui::Id::new("people-overlay"))
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_pos(screen.min)
        .fixed_size(screen.size() - vec2(16., 16.))
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(16))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(tr(if s.finder.is_some() { "Find This Person…" } else { "Review matches…" }));
                if button(ui, "smartSort:peopleClose", "Done").clicked() {
                    s.queue = None;
                    s.merge = None;
                    s.finder = None;
                }
            });
            ui.separator();
            if let Some((person, other)) = s.merge {
                let name = app
                    .session
                    .smart
                    .people
                    .data
                    .person(other)
                    .ok()
                    .map(|p| p.name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| crate::i18n::tr_format!("Unnamed person {id}", id = other));
                ui.heading(crate::i18n::tr_format!("Merge with {name}?", name = name));
                ui.horizontal(|ui| {
                    if button(ui, "smartSort:mergeSame", "Same").clicked() {
                        if let Some(q) = &mut s.queue {
                            q.history.push((q.skipped.clone(), q.cursor, true));
                        }
                        command(app, &mut s, "smartSort.mergePeople", json!({"ids":[person,other]}));
                        s.merge = None;
                    }
                    if button(ui, "smartSort:mergeDifferent", "Different").clicked() {
                        command(app, &mut s, "smartSort.peopleDifferent", json!({"person":person,"other":other}));
                        s.merge = None;
                    }
                    if button(ui, "smartSort:mergeSkip", "Not sure").clicked() {
                        s.merge = None;
                    }
                });
            } else if let Some(id) = s.finder {
                finder_body(app, ui, &mut s, id);
            } else if let Some(mut q) = s.queue.take() {
                queue_body(app, ui, &mut s, &mut q);
                s.queue = Some(q);
            }
            if !s.error.is_empty() {
                ui.colored_label(t.caution, &s.error);
            }
        });
    if let Some(w) = shown {
        ctx.move_to_top(w.response.layer_id);
        register(ctx, "people:overlay", w.response.rect);
    }
    app.ui.people = s;
}
fn finder_body(app: &mut LightcraftApp, ui: &mut Ui, s: &mut PeopleUi, id: PhotoId) {
    face_options(app, ui);
    if !app.ui.people.error.is_empty() {
        s.error = std::mem::take(&mut app.ui.people.error);
    }
    if !app.session.smart.prefs.faces_enabled {
        return;
    }
    let faces = command(app, s, "smartSort.facesInPhoto", json!({"photo":id})).unwrap_or_default();
    if faces["analysed"] != true {
        if app.people_task.is_none()
            && button(ui, "smartSort:findAnalyze", "Analyse faces in this photo").clicked()
            && let Err(e) = crate::people_task::start(app, &[id], None)
        {
            s.error = e;
        }
        return;
    }
    let r = photo(app, ui, id, vec2((ui.available_width() - 32.).min(850.), (ui.available_height() - 130.).max(160.)));
    for face in faces["faces"].as_array().cloned().unwrap_or_default() {
        let i = face["face"].as_u64().unwrap_or(0) as usize;
        let [x, y, w, h]: [f32; 4] = serde_json::from_value(face["rect"].clone()).unwrap_or([0., 0., 1., 1.]);
        let f = Rect::from_min_size(r.min + vec2(x * r.width(), y * r.height()), vec2(w * r.width(), h * r.height()));
        let resp = ui.interact(f, ui.id().with((id, i)), Sense::click());
        register(ui.ctx(), format!("smartSort:face:{}:{i}", id.0), f);
        ui.painter().rect_stroke(
            f,
            3.,
            Stroke::new(2., if s.picked == Some(i) { Tokens::get(ui.ctx()).accent } else { Tokens::get(ui.ctx()).text }),
            StrokeKind::Inside,
        );
        if resp.clicked() {
            s.picked = Some(i);
        }
    }
    ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut s.find_name).hint_text(tr("Add name")).desired_width(250.));
        register(ui.ctx(), "smartSort:findName", r.rect);
        let r = ui.add_enabled(s.picked.is_some() && !s.find_name.trim().is_empty(), egui::Button::new(tr("Find Photos")));
        register(ui.ctx(), "smartSort:findPhotos", r.rect);
        if r.clicked() {
            let name = s.find_name.clone();
            if let Some(result) = command(app, s, "smartSort.findPerson", json!({"name":name,"faces":[{"photo":id,"face":s.picked}]}))
                && let Some(person) = result["person"].as_u64()
            {
                s.finder = None;
                s.filter_ids = vec![person];
                apply_filter(app, s);
                let _ = app.run("view.photoGrid", json!({}));
                open_queue(app, s, person);
            }
        }
    });
}
fn queue_body(app: &mut LightcraftApp, ui: &mut Ui, s: &mut PeopleUi, q: &mut ConfirmQueue) {
    if q.revision != app.session.catalog.revision || q.rows.is_null() {
        match lightcraft_engine::smart_sort::people::matches(
            &app.session.catalog,
            &app.session.smart.people,
            &app.session.smart.people.data,
            q.person,
        ) {
            Ok(mut matches) => {
                matches.retain(|r| q.all || !r.confirmed);
                matches.sort_by(|a, b| a.similarity.total_cmp(&b.similarity).then(a.photo.cmp(&b.photo)).then(a.face.cmp(&b.face)));
                q.rows = json!({"matches":matches});
                q.revision = app.session.catalog.revision;
            }
            Err(e) => s.error = e,
        }
    }
    let result = &q.rows;
    let rows: Vec<_> = result["matches"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| !q.skipped.contains(&(PhotoId(r["photo"].as_u64().unwrap_or(0)), r["face"].as_u64().unwrap_or(0) as usize)))
        .collect();
    q.cursor = q.cursor.min(rows.len().saturating_sub(1));
    let row = rows.get(q.cursor);
    let name = app.session.smart.people.data.person(q.person).ok().map(|p| p.name.clone()).unwrap_or_default();
    ui.label(name);
    if let Some(row) = row {
        let id = PhotoId(row["photo"].as_u64().unwrap_or(0));
        let bounds = serde_json::from_value(row["rect"].clone()).unwrap_or([0., 0., 1., 1.]);
        ui.horizontal_top(|ui| {
            let h = (ui.available_height() - 130.).clamp(180., 650.);
            let (r, _) = ui.allocate_exact_size(vec2(h.min(340.), h.min(340.)), Sense::hover());
            crop(app, ui, id, bounds, r, false);
            photo(app, ui, id, vec2((ui.available_width() - 16.).max(100.), h));
        });
        register(ui.ctx(), "smartSort:queueCurrent", ui.min_rect());
        ui.label(app.session.catalog.photo(id).map(|p| p.file_name.clone()).unwrap_or_default());
    } else {
        ui.label(tr("All matches reviewed"));
    }
    let mut action = None;
    ui.horizontal(|ui| {
        for (id, label, a) in [
            ("reviewYes", "Yes (Y)", QueueAction::Yes),
            ("reviewNo", "No (N)", QueueAction::No),
            ("reviewSkip", "Not sure (S)", QueueAction::NotSure),
            ("reviewUndo", "Undo (⌘Z)", QueueAction::Undo),
        ] {
            if button(ui, format!("smartSort:{id}"), label).clicked() {
                action = Some(a);
            }
        }
        if button(ui, "smartSort:createPersonFolder", "Create Folder").clicked() {
            command(app, s, "smartSort.personFolder", json!({"person":q.person,"enabled":true}));
        }
        if q.all && row.is_some() && button(ui, "smartSort:splitFace", "Not the same person").clicked() {
            let r = row.cloned().unwrap_or_default();
            q.history.push((q.skipped.clone(), q.cursor, true));
            command(app, s, "smartSort.splitPerson", json!({"person":q.person,"faces":[{"photo":r["photo"],"face":r["face"]}]}));
            s.refresh = true;
        }
    });
    if !ui.ctx().egui_wants_keyboard_input() {
        for e in ui.input(|i| i.events.clone()) {
            if let egui::Event::Key { key, modifiers, pressed: true, .. } = e
                && let Some(a) = smart_sort_keys::queue_action(key, modifiers)
            {
                ui.input_mut(|i| {
                    i.consume_key(modifiers, key);
                });
                action = Some(a);
            }
        }
    }
    if let Some(action) = action {
        match action {
            QueueAction::Undo => {
                if let Some(previous) = q.history.pop() {
                    q.skipped = previous.0;
                    if previous.2 {
                        command(app, s, "edit.undo", json!({}));
                    }
                    q.cursor = previous.1;
                    q.revision = u64::MAX;
                }
            }
            QueueAction::Prev => q.cursor = q.cursor.saturating_sub(1),
            QueueAction::Next => q.cursor = (q.cursor + 1).min(rows.len().saturating_sub(1)),
            QueueAction::Yes | QueueAction::No | QueueAction::NotSure => {
                if let Some(row) = row {
                    if action == QueueAction::NotSure {
                        q.history.push((q.skipped.clone(), q.cursor, false));
                        q.skipped.insert((PhotoId(row["photo"].as_u64().unwrap_or(0)), row["face"].as_u64().unwrap_or(0) as usize));
                    } else {
                        q.history.push((q.skipped.clone(), q.cursor, true));
                        command(
                            app,
                            s,
                            if action == QueueAction::Yes { "smartSort.confirmFace" } else { "smartSort.rejectFace" },
                            json!({"person":q.person,"photo":row["photo"],"face":row["face"]}),
                        );
                    }
                    q.cursor = 0;
                }
            }
        }
    }
}

pub fn review_person(app: &mut LightcraftApp, id: u64) {
    let mut s = std::mem::take(&mut app.ui.people);
    open_queue(app, &mut s, id);
    app.ui.people = s;
}

fn paint_round(ui: &Ui, r: Rect, texture: egui::TextureId) {
    let mut mesh = egui::Mesh::with_texture(texture);
    mesh.vertices.push(egui::epaint::Vertex { pos: r.center(), uv: pos2(0.5, 0.5), color: egui::Color32::WHITE });
    for i in 0..=48 {
        let a = std::f32::consts::TAU * i as f32 / 48.;
        let p = vec2(a.cos(), a.sin());
        mesh.vertices.push(egui::epaint::Vertex {
            pos: r.center() + p * r.width() / 2.,
            uv: pos2(0.5 + p.x / 2., 0.5 + p.y / 2.),
            color: egui::Color32::WHITE,
        });
        if i > 0 {
            mesh.add_triangle(0, i, i + 1);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}
