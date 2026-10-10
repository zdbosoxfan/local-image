//! Face inference and headshot decoding run on workers; only the session thread persists data.
use crate::{LightcraftApp, i18n::tr, state::Dialog};
use lightcraft_catalog::{MediaKind, PhotoId};
use lightcraft_engine::smart_sort::{
    self, FaceTagger, PeopleEngine,
    faces_store::StoredFace,
    people::{self, PeopleData},
};
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, channel},
};

enum Event {
    Model(Arc<dyn FaceTagger>),
    Row(PhotoId, String, Result<Vec<StoredFace>, String>),
    Seeds(PeopleData, Vec<(String, Vec<StoredFace>)>, usize),
    Done(Result<(), String>),
}
pub struct PeopleTask {
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<Mutex<(usize, usize)>>,
    rx: Receiver<Event>,
    model: String,
    seed: bool,
}
impl Drop for PeopleTask {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
pub fn start(app: &mut LightcraftApp, ids: &[PhotoId], headshots: Option<String>) -> Result<(), String> {
    if app.people_task.is_some() {
        return Err(tr("Face analysis is already running").to_string());
    }
    app.session.require_faces().map_err(|e| e.to_string())?;
    let model = app.session.smart.people.ready()?;
    let injected = app.session.smart.people.tagger.clone();
    let dir = app.session.quick_seg_dir.clone();
    let mut keys = std::collections::BTreeSet::new();
    let todo: Vec<_> = ids
        .iter()
        .copied()
        .filter(|id| {
            app.session.catalog.photo(*id).is_some_and(|p| {
                let key = lightcraft_engine::media::content_key(p);
                p.kind != MediaKind::Video && app.session.smart.people.store.get(&model, &key).is_none() && keys.insert(key)
            })
        })
        .collect();
    let sizes: std::collections::BTreeMap<_, _> =
        todo.iter().filter_map(|id| app.session.catalog.photo(*id).map(|p| (*id, (p.width, p.height)))).collect();
    let inputs = if headshots.is_none() { smart_sort::prepare_inputs(&mut app.session, &todo) } else { Vec::new() };
    let missing: Vec<_> = todo.into_iter().filter(|id| !inputs.iter().any(|j| j.id == *id)).collect();
    let seed = headshots.is_some();
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(Mutex::new((0, inputs.len() + missing.len())));
    let (c, p) = (cancel.clone(), progress.clone());
    let (tx, rx) = channel();
    let work = move || {
        let result = (|| {
            let tagger = injected.map(Ok).unwrap_or_else(|| PeopleEngine::default().tagger(dir.as_deref()))?;
            let _ = tx.send(Event::Model(tagger.clone()));
            if let Some(dir) = headshots {
                // A detached, in-memory session reuses the engine's headshot importer without
                // touching the live catalog or files outside the library.
                let mut worker = lightcraft_engine::Session::new();
                worker.smart.prefs.faces_enabled = true;
                worker.smart.people.tagger = Some(tagger);
                if c.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let result = worker.execute("smartSort.peopleSeedHeadshots", &json!({"dir":dir})).map_err(|e| e.to_string())?;
                if !c.load(Ordering::Relaxed) {
                    let data = worker.smart.people.data.clone();
                    let records = data
                        .seed_sources
                        .keys()
                        .filter_map(|key| worker.smart.people.store.get(worker.smart.people.model_id(), key).map(|f| (key.clone(), f.to_vec())))
                        .collect();
                    let skipped = result["skipped"].as_array().map_or(0, Vec::len);
                    let _ = tx.send(Event::Seeds(data, records, skipped));
                }
            } else {
                for id in missing {
                    let _ = tx.send(Event::Row(id, String::new(), Err(tr("Could not prepare photo").to_string())));
                }
                use rayon::prelude::*;
                let threads = std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).max(1));
                let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().map_err(|e| e.to_string())?;
                pool.install(|| {
                    inputs.into_par_iter().for_each(|input| {
                        if c.load(Ordering::Relaxed) {
                            return;
                        }
                        let (id, key) = (input.id, input.key.clone());
                        let value = people::detect_input(input, sizes.get(&id).copied().unwrap_or((1, 1)), tagger.as_ref());
                        let _ = tx.send(Event::Row(id, key, value));
                        if let Ok(mut p) = p.lock() {
                            p.0 += 1;
                        }
                    })
                });
            }
            Ok(())
        })();
        let _ = tx.send(Event::Done(result));
    };
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::Builder::new().name("smart-sort-faces".into()).spawn(work).map_err(|e| e.to_string())?;
    #[cfg(target_arch = "wasm32")]
    work();
    app.people_task = Some(PeopleTask { cancel, progress, rx, model, seed });
    Ok(())
}
pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(task) = app.people_task.take() else { return };
    // Opt-out or clearing data invalidates pending inference rather than recreating the cache.
    if !app.session.smart.prefs.faces_enabled || app.session.smart.people.data.model != task.model {
        return;
    }
    let mut done = false;
    let mut changed = false;
    loop {
        match task.rx.try_recv() {
            Ok(Event::Model(model)) => app.session.smart.people.tagger = Some(model),
            Ok(Event::Row(id, key, row)) => match row.and_then(|faces| app.session.smart.people.store.insert(&task.model, key, faces)) {
                Ok(()) => changed = true,
                Err(e) => {
                    app.ui.people.error = e.clone();
                    if let Some(Dialog::SmartSort { state }) = &mut app.ui.dialog {
                        state.failed.push((id, e));
                    }
                }
            },
            Ok(Event::Seeds(seeds, records, skipped)) => {
                let result = (|| {
                    for (key, faces) in records {
                        app.session.smart.people.store.insert(&task.model, key, faces)?;
                    }
                    let mut data = app.session.smart.people.data.clone();
                    for person in seeds.people {
                        if data.people.iter().any(|p| person.seed_faces.iter().any(|k| p.seed_faces.contains(k))) {
                            continue;
                        }
                        // A unique, sure unnamed prototype is the same gallery identity. Seed
                        // it in place, keeping IDs already selected in folders and filters.
                        let mut ranked: Vec<_> =
                            data.people.iter().map(|p| (p.id, p.centroid.iter().zip(&person.centroid).map(|(a, b)| a * b).sum::<f32>())).collect();
                        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
                        let existing = ranked
                            .first()
                            .filter(|(_, score)| *score >= people::SURE_COSINE && ranked.get(1).is_none_or(|(_, other)| score - other >= 0.05))
                            .and_then(|(id, _)| data.person(*id).ok())
                            .filter(|p| p.name.is_empty() && !p.ignored)
                            .map(|p| p.id);
                        if let Some(id) = existing {
                            let target = data.person_mut(id)?;
                            target.name = person.name;
                            target.seed_faces.extend(person.seed_faces);
                        } else {
                            let id = data.add(person.name.clone());
                            *data.person_mut(id)? = people::Person { id, ..person };
                        }
                    }
                    data.seed_sources.extend(seeds.seed_sources);
                    people::update_centroids(&mut data, &app.session.smart.people);
                    app.session.commit_people("Smart Sort: seed headshots", data).map_err(|e| e.to_string())
                })();
                if let Err(e) = result {
                    app.ui.people.error = e;
                }
                app.ui.people.names.clear();
                app.ui.people.notice = crate::i18n::tr_format!("Skipped {n} headshots without exactly one face", n = skipped);
                changed = true;
            }
            Ok(Event::Done(result)) => {
                if let Err(e) = result {
                    app.ui.people.error = e;
                }
                done = true;
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                done = true;
                break;
            }
        }
    }
    if changed || done {
        let result = (|| {
            app.session.smart.people.store.save().map_err(|e| e.to_string())?;
            let mut data = app.session.smart.people.data.clone();
            people::refresh(&app.session.catalog, &app.session.smart.people, &mut data, false)?;
            app.session.commit_people("Smart Sort: analyse faces", data).map_err(|e| e.to_string())
        })();
        if let Err(e) = result {
            app.ui.people.error = e;
        }
        app.ui.people.refresh = true;
    }
    if done {
        if !task.seed
            && app.smart_sort.is_none()
            && !task.cancel.load(Ordering::Relaxed)
            && let Some(Dialog::SmartSort { state }) = &mut app.ui.dialog
        {
            state.step = 1;
            state.dirty = true;
        }
    } else {
        app.people_task = Some(task);
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

/// Headshot covers are read-only and decoded off the session thread, even before gallery matches.
#[derive(Default)]
pub struct SeedImages {
    images: std::collections::BTreeMap<String, Option<egui::TextureHandle>>,
    pending: Vec<(String, Receiver<Result<lightcraft_raster::Rgba8, String>>)>,
}
impl SeedImages {
    pub fn request(&mut self, ctx: &egui::Context, source: &str, rect: [f32; 4]) -> Option<egui::TextureId> {
        for i in (0..self.pending.len()).rev() {
            match self.pending[i].1.try_recv() {
                Ok(result) => {
                    let (key, _) = self.pending.swap_remove(i);
                    if let Ok(image) = result {
                        let pixels = image.data.iter().flatten().copied().collect::<Vec<_>>();
                        let image = egui::ColorImage::from_rgba_unmultiplied([image.width, image.height], &pixels);
                        self.images.insert(key, Some(ctx.load_texture("headshot face", image, egui::TextureOptions::LINEAR)));
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending.swap_remove(i);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let key = format!("{source}:{rect:?}");
        if !self.images.contains_key(&key) {
            self.images.insert(key.clone(), None);
            let (tx, rx) = channel();
            let path = source.to_string();
            let context = ctx.clone();
            let work = move || {
                let result = (|| {
                    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                    let thumb = lightcraft_codecs::decode_thumbnail(&bytes, 512).map_err(|e| e.to_string())?;
                    let image = thumb.image.into_oriented(lightcraft_geom::Orientation::from_exif(thumb.orientation));
                    let [x, y, w, h] = rect;
                    let side = (w * image.width as f32).max(h * image.height as f32) * 1.7;
                    let side = side.clamp(1., image.width.min(image.height) as f32);
                    let cx = ((x + w / 2.) * image.width as f32).clamp(side / 2., image.width as f32 - side / 2.);
                    let cy = ((y + h / 2.) * image.height as f32).clamp(side / 2., image.height as f32 - side / 2.);
                    let mut crop = lightcraft_raster::Rgba8::new(128, 128);
                    for py in 0..128 {
                        for px in 0..128 {
                            let sx = (cx - side / 2. + (px as f32 + 0.5) * side / 128.).clamp(0., image.width as f32 - 1.) as usize;
                            let sy = (cy - side / 2. + (py as f32 + 0.5) * side / 128.).clamp(0., image.height as f32 - 1.) as usize;
                            crop.set(px, py, image.get(sx, sy));
                        }
                    }
                    Ok(crop)
                })();
                let _ = tx.send(result);
                context.request_repaint();
            };
            #[cfg(not(target_arch = "wasm32"))]
            let _ = std::thread::Builder::new().name("headshot-cover".into()).spawn(work);
            #[cfg(target_arch = "wasm32")]
            work();
            self.pending.push((key.clone(), rx));
        }
        self.images.get(&key).and_then(|t| t.as_ref().map(egui::TextureHandle::id))
    }
}
