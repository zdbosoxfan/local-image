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
    failed: bool,
}
impl Drop for PeopleTask {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn missing_inputs(todo: Vec<PhotoId>, prepared: impl IntoIterator<Item = PhotoId>) -> Vec<PhotoId> {
    let prepared: std::collections::BTreeSet<_> = prepared.into_iter().collect();
    todo.into_iter().filter(|id| !prepared.contains(id)).collect()
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
    let missing = missing_inputs(todo, inputs.iter().map(|j| j.id));
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
                    let total = data.seed_sources.len() + skipped;
                    if let Ok(mut progress) = p.lock() {
                        *progress = (total, total);
                    }
                    let _ = tx.send(Event::Seeds(data, records, skipped));
                }
            } else {
                for id in missing {
                    let _ = tx.send(Event::Row(id, String::new(), Err(tr("Could not prepare photo").to_string())));
                    if let Ok(mut progress) = p.lock() {
                        progress.0 += 1;
                    }
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
    app.people_task = Some(PeopleTask { cancel, progress, rx, model, seed, failed: false });
    Ok(())
}
pub fn poll(app: &mut LightcraftApp, ctx: &egui::Context) {
    let Some(mut task) = app.people_task.take() else { return };
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
                            let id = data.add(person.name.clone())?;
                            *data.person_mut(id)? = people::Person { id, ..person };
                        }
                    }
                    data.seed_sources.extend(seeds.seed_sources);
                    people::update_centroids(&mut data, &app.session.smart.people);
                    app.session.commit_people("Smart Sort: seed headshots", data).map_err(|e| e.to_string())
                })();
                if let Err(e) = result {
                    app.ui.people.error = e;
                    task.failed = true;
                }
                app.ui.people.names.clear();
                app.ui.people.notice = crate::i18n::tr_format!("Skipped {n} headshots without exactly one face", n = skipped);
                changed = true;
            }
            Ok(Event::Done(result)) => {
                if let Err(e) = result {
                    app.ui.people.error = e;
                    task.failed = true;
                }
                done = true;
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                app.ui.people.error = tr("Face analysis worker stopped unexpectedly").to_string();
                task.failed = true;
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
            task.failed = true;
        }
        app.ui.people.refresh = true;
    }
    if done {
        if !task.seed
            && !task.failed
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
    images: std::collections::BTreeMap<String, (u64, Option<egui::TextureHandle>)>,
    clock: u64,
    pending: Vec<(String, Receiver<Result<lightcraft_raster::Rgba8, String>>)>,
}
impl SeedImages {
    const CAPACITY: usize = 256;
    const WORKERS: usize = 4;

    fn room(&mut self) {
        if self.images.len() < Self::CAPACITY {
            return;
        }
        let oldest = self
            .images
            .iter()
            .filter(|(key, _)| !self.pending.iter().any(|(p, _)| p == *key))
            .min_by_key(|(_, (stamp, _))| *stamp)
            .map(|(key, _)| key.clone());
        if let Some(key) = oldest {
            self.images.remove(&key);
        }
    }
    pub fn request(&mut self, ctx: &egui::Context, source: &str, rect: [f32; 4]) -> Option<egui::TextureId> {
        for i in (0..self.pending.len()).rev() {
            match self.pending[i].1.try_recv() {
                Ok(result) => {
                    let (key, _) = self.pending.swap_remove(i);
                    if let Ok(image) = result {
                        let pixels = image.data.iter().flatten().copied().collect::<Vec<_>>();
                        let image = egui::ColorImage::from_rgba_unmultiplied([image.width, image.height], &pixels);
                        if let Some((_, slot)) = self.images.get_mut(&key) {
                            *slot = Some(ctx.load_texture("headshot face", image, egui::TextureOptions::LINEAR));
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending.swap_remove(i);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.pending.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        self.clock = self.clock.wrapping_add(1);
        let key = format!("{source}:{rect:?}");
        if !self.images.contains_key(&key) {
            if self.pending.len() >= Self::WORKERS {
                return None;
            }
            self.room();
            self.images.insert(key.clone(), (self.clock, None));
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
        self.images.get_mut(&key).and_then(|(stamp, texture)| {
            *stamp = self.clock;
            texture.as_ref().map(egui::TextureHandle::id)
        })
    }
}

#[cfg(test)]
mod ds2_tests {
    use super::*;
    #[test]
    fn ds2_missing_input_partition_includes_only_unprepared_ids() {
        assert_eq!(missing_inputs(vec![PhotoId(17), PhotoId(5), PhotoId(999)], [PhotoId(17), PhotoId(999), PhotoId(999)]), vec![PhotoId(5)]);
        let missing = missing_inputs((0..50_000).map(PhotoId).collect(), (0..50_000).step_by(2).map(PhotoId));
        assert_eq!(missing.len(), 25_000);
        assert!(missing.iter().all(|id| id.0 % 2 == 1));
    }
    #[test]
    fn ds2_headshot_progress_counts_imported_and_skipped_images() {
        let dir = std::env::temp_dir().join(format!("ds2-headshot-progress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut image = lightcraft_raster::Rgba8::filled(16, 16, [255, 0, 0, 255]);
        for y in 0..8 {
            for x in 0..8 {
                image.set(x, y, [255; 4]);
            }
        }
        for (name, image) in [("Jane", image), ("Skip", lightcraft_raster::Rgba8::filled(16, 16, [0, 0, 0, 255]))] {
            let png = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&image), &Default::default()).unwrap();
            std::fs::write(dir.join(format!("{name}.png")), png).unwrap();
        }
        let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), Default::default());
        app.session.smart.prefs.faces_enabled = true;
        app.session.smart.people.tagger = Some(Arc::new(lightcraft_engine::smart_sort::people::MockFaces));
        start(&mut app, &[], Some(dir.to_string_lossy().into_owned())).unwrap();
        let progress = app.people_task.as_ref().unwrap().progress.clone();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while *progress.lock().unwrap() != (2, 2) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(*progress.lock().unwrap(), (2, 2));
        poll(&mut app, &egui::Context::default());
        assert_eq!(app.session.smart.people.data.people.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn ds2_disconnected_face_worker_reports_failure_and_does_not_advance() {
        let mut app = LightcraftApp::new(lightcraft_engine::Session::new(), Default::default());
        app.session.smart.prefs.faces_enabled = true;
        app.session.smart.people.tagger = Some(Arc::new(lightcraft_engine::smart_sort::people::MockFaces));
        app.session.smart.people.ready().unwrap();
        app.ui.dialog = Some(Dialog::SmartSort { state: Box::default() });
        let (tx, rx) = channel();
        drop(tx);
        app.people_task = Some(PeopleTask {
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new((0, 1))),
            rx,
            model: "mock-faces".into(),
            seed: false,
            failed: false,
        });
        poll(&mut app, &egui::Context::default());
        assert!(app.people_task.is_none());
        assert!(app.ui.people.error.contains("stopped unexpectedly"));
        assert!(matches!(app.ui.dialog,Some(Dialog::SmartSort {state}) if state.step==0));
    }
    #[test]
    fn ds2_headshot_cache_bounds_textures_and_concurrent_decodes() {
        let ctx = egui::Context::default();
        let mut cache = SeedImages::default();
        let mut senders = vec![];
        for n in 0..SeedImages::WORKERS {
            let (tx, rx) = channel();
            senders.push(tx);
            let key = format!("pending-{n}");
            cache.images.insert(key.clone(), (0, None));
            cache.pending.push((key, rx));
        }
        assert!(cache.request(&ctx, "/tmp/ds2-never-read", [0., 0., 1., 1.]).is_none());
        assert_eq!(cache.pending.len(), SeedImages::WORKERS);
        assert_eq!(cache.images.len(), SeedImages::WORKERS);
        drop(senders);
        cache.pending.clear();
        cache.images.clear();
        let baseline = ctx.tex_manager().read().allocated().count();
        for n in 0..300 {
            cache.room();
            let tex =
                ctx.load_texture(format!("face-{n}"), egui::ColorImage::new([128, 128], vec![egui::Color32::WHITE; 128 * 128]), Default::default());
            cache.images.insert(format!("face-{n}"), (n, Some(tex)));
        }
        assert_eq!(cache.images.len(), SeedImages::CAPACITY);
        assert!(!cache.images.contains_key("face-0"));
        assert!(cache.images.contains_key("face-299"));
        assert_eq!(ctx.tex_manager().read().allocated().count(), SeedImages::CAPACITY + baseline);
    }
}
