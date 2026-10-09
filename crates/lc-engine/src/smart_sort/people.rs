//! Opt-in face analysis and durable people identities. Suggestions are separate from human
//! confirmations: only sure assignments or confirmed faces enter folders. Centroids and all
//! rejections stay in AI/people.json, never catalog metadata or command results.

use super::faces_store::{DIM, FacesStore, StoredFace};
use crate::{EngineError, Result, Session};
use li_seg::faces::{FaceImage, FaceModels, FaceNode, NodeKey, RgbPixels};
use lightcraft_catalog::{Catalog, Op, PhotoId};
use lightcraft_raster::Rgba8;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub const FACE_MODEL: &str = "yunet-sface";
/// Conservative folder cutoff above the .45 review threshold; coordinator can tune this.
pub const SURE_COSINE: f32 = 0.60;

pub trait FaceTagger: Send + Sync {
    fn model_id(&self) -> &str;
    /// Detect and embed on the full, oriented, uncropped 1024px analysis frame.
    fn faces(&self, image: &Rgba8) -> std::result::Result<Vec<StoredFace>, String>;
}
pub struct RealFaces(pub FaceModels);
impl FaceTagger for RealFaces {
    fn model_id(&self) -> &str {
        FACE_MODEL
    }
    fn faces(&self, image: &Rgba8) -> std::result::Result<Vec<StoredFace>, String> {
        let rgb: Vec<_> = image.data.iter().flat_map(|p| p[..3].iter().copied()).collect();
        self.0
            .faces(FaceImage { pixels: RgbPixels::U8(&rgb), width: image.width, height: image.height })
            .map(|faces| {
                faces
                    .into_iter()
                    .map(|f| StoredFace {
                        rect: [
                            f.rect[0] / image.width as f32,
                            f.rect[1] / image.height as f32,
                            f.rect[2] / image.width as f32,
                            f.rect[3] / image.height as f32,
                        ],
                        score: f.score,
                        embedding: f.embedding.to_vec(),
                    })
                    .collect()
            })
            .map_err(|_| "face inference failed".into())
    }
}

/// Paint a white 8x8 marker followed by an 8x8 identity colour. Additional markers every
/// 16px allow multi-face folder tests; identical colours always have identical embeddings.
#[doc(hidden)]
#[derive(Default)]
pub struct MockFaces;
impl FaceTagger for MockFaces {
    fn model_id(&self) -> &str {
        "mock-faces"
    }
    fn faces(&self, image: &Rgba8) -> std::result::Result<Vec<StoredFace>, String> {
        if image.width == 0 || image.height == 0 || image.width.checked_mul(image.height) != Some(image.data.len()) {
            return Err("invalid face input".into());
        }
        let mut faces = Vec::new();
        if image.height < 8 {
            return Ok(faces);
        }
        for x in (0..image.width.saturating_sub(15)).step_by(16) {
            if !(0..8).all(|y| (x..x + 8).all(|c| image.data[y * image.width + c][..3] == [255, 255, 255])) {
                continue;
            }
            let mut v = vec![0.; DIM];
            for y in 0..8 {
                for c in x + 8..x + 16 {
                    for (axis, val) in v.iter_mut().take(3).enumerate() {
                        *val += f32::from(image.data[y * image.width + c][axis]);
                    }
                }
            }
            if v[..3].iter().all(|n| *n == 0.) {
                v[3] = 1.;
            }
            let embedding = super::store::normalized(v, DIM)?;
            faces.push(StoredFace {
                rect: [x as f32 / image.width as f32, 0., 8. / image.width as f32, 8. / image.height as f32],
                score: 0.99,
                embedding,
            });
        }
        Ok(faces)
    }
}

/// Tuple representation is compatible with the original spec's [content-key,face-index].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FaceKey(pub String, pub usize);

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Person {
    pub id: u64,
    pub name: String,
    pub pinned: bool,
    pub ignored: bool,
    pub seed_faces: Vec<FaceKey>,
    #[serde(alias = "faces")]
    pub confirmed_faces: Vec<FaceKey>,
    #[serde(alias = "rejected")]
    pub rejected_faces: Vec<FaceKey>,
    /// Unnamed cluster's prototype, not user confirmation.
    pub cluster_faces: Vec<FaceKey>,
    pub centroid: Vec<f32>,
    pub folder_enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PeopleData {
    pub version: u32,
    pub model: String,
    pub people: Vec<Person>,
    pub next_id: u64,
    pub min_photos: u8,
    pub name_suggestions: Vec<NameSuggestion>,
    /// A split's negative evidence prevents automatically recombining the same identities.
    pub different: Vec<[u64; 2]>,
    /// Keep saved folder selections working after a merge.
    pub merged_ids: BTreeMap<u64, u64>,
    /// Read-only headshot locations let the dialog crop seed bubbles before gallery matches.
    pub seed_sources: BTreeMap<String, String>,
}
impl Default for PeopleData {
    fn default() -> Self {
        Self {
            version: 1,
            model: String::new(),
            people: Vec::new(),
            next_id: 1,
            min_photos: 3,
            name_suggestions: Vec::new(),
            different: Vec::new(),
            merged_ids: BTreeMap::new(),
            seed_sources: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NameSuggestion {
    pub name: String,
    pub title: String,
}
impl PeopleData {
    pub fn resolve_id(&self, id: u64) -> u64 {
        self.merged_ids.get(&id).copied().unwrap_or(id)
    }
    pub fn person(&self, id: u64) -> std::result::Result<&Person, String> {
        self.people.iter().find(|p| p.id == id).ok_or("unknown person".into())
    }
    pub fn person_mut(&mut self, id: u64) -> std::result::Result<&mut Person, String> {
        self.people.iter_mut().find(|p| p.id == id).ok_or("unknown person".into())
    }
    pub fn add(&mut self, name: String) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.people.push(Person { id, name, ..Default::default() });
        id
    }
    fn validate(&self) -> std::result::Result<(), String> {
        if self.version != 1 || !(1..=10).contains(&self.min_photos) || self.next_id == u64::MAX {
            return Err("invalid people data version or settings".into());
        }
        let mut ids = BTreeSet::new();
        for p in &self.people {
            if p.id == 0 || p.id >= self.next_id || !ids.insert(p.id) {
                return Err("invalid people identities".into());
            }
            if !p.centroid.is_empty() {
                super::store::normalized(p.centroid.clone(), DIM)?;
            }
            if p.seed_faces.iter().chain(&p.confirmed_faces).chain(&p.cluster_faces).chain(&p.rejected_faces).any(|f| f.0.is_empty() || f.1 > 254) {
                return Err("invalid face reference".into());
            }
        }
        if self.merged_ids.iter().any(|(old, target)| *old == 0 || *old >= self.next_id || ids.contains(old) || !ids.contains(target)) {
            return Err("invalid merged person identities".into());
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct PeopleEngine {
    pub tagger: Option<Arc<dyn FaceTagger>>,
    pub store: FacesStore,
    pub data: PeopleData,
    dir: Option<PathBuf>,
    loaded: bool,
}
impl PeopleEngine {
    pub fn new(library: Option<&Path>) -> Self {
        Self { dir: library.map(|d| d.join("AI")), store: FacesStore::new(library), ..Default::default() }
    }
    pub fn ensure(&mut self) -> std::result::Result<(), String> {
        if self.loaded {
            return Ok(());
        }
        if let Some(dir) = &self.dir {
            match std::fs::read(dir.join("people.json")) {
                Ok(bytes) => match serde_json::from_slice::<PeopleData>(&bytes).ok().filter(|p| p.validate().is_ok()) {
                    Some(data) => self.data = data,
                    None => log::warn!("Smart Sort: ignoring damaged people data"),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("could not read people data".into()),
            }
        }
        self.loaded = true;
        Ok(())
    }
    pub fn model_id(&self) -> &str {
        self.tagger.as_ref().map_or(if self.data.model.is_empty() { FACE_MODEL } else { &self.data.model }, |t| t.model_id())
    }
    pub fn ready(&mut self) -> std::result::Result<String, String> {
        self.ensure()?;
        let model = self.model_id().to_owned();
        if !self.data.model.is_empty() && self.data.model != model {
            return Err("clear face data before changing the face model".into());
        }
        self.store.ensure(&model)?;
        for person in &mut self.data.people {
            if person.centroid.is_empty() {
                let keys: Vec<_> = person.seed_faces.iter().chain(&person.confirmed_faces).cloned().collect();
                person.centroid = centroid(if keys.is_empty() { &person.cluster_faces } else { &keys }, &self.store, &model);
            }
        }
        self.data.model = model.clone();
        Ok(model)
    }
    pub fn tagger(&mut self, dir: Option<&Path>) -> std::result::Result<Arc<dyn FaceTagger>, String> {
        if self.tagger.is_none() {
            let dir = dir.ok_or("face model folder is not configured")?;
            if li_seg::installed_bytes(dir, li_seg::Group::Faces.official()).is_none() {
                return Err("Smart Sort needs both verified face models".into());
            }
            self.tagger = Some(Arc::new(RealFaces(FaceModels::load(dir).map_err(|_| "could not load face models")?)));
        }
        self.tagger.clone().ok_or("face models unavailable".into())
    }
    pub fn save_data(&self, data: &PeopleData) -> std::result::Result<(), String> {
        data.validate()?;
        if let Some(dir) = &self.dir {
            std::fs::create_dir_all(dir).map_err(|_| "could not create face data directory")?;
            let bytes = serde_json::to_vec_pretty(data).map_err(|_| "could not encode people data")?;
            lightcraft_catalog::safe_file::write_atomic(&dir.join("people.json"), &bytes).map_err(|_| "could not save people data")?;
        }
        Ok(())
    }
    pub fn save(&mut self) -> std::result::Result<(), String> {
        self.store.save().map_err(|_| "could not save face cache")?;
        self.save_data(&self.data)
    }
    pub fn clear(&mut self) -> std::result::Result<(), String> {
        self.store.clear().map_err(|_| "could not delete face cache")?;
        if let Some(dir) = &self.dir {
            match std::fs::remove_file(dir.join("people.json")) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("could not delete people data".into()),
            }
        }
        self.data = PeopleData::default();
        self.loaded = true;
        self.tagger = None;
        Ok(())
    }
}

fn vector(v: &[f32]) -> std::result::Result<[f32; DIM], String> {
    v.try_into().map_err(|_| "invalid face embedding dimension".into())
}
fn similarity(a: &[f32], b: &[f32]) -> f32 {
    vector(a).and_then(|a| vector(b).and_then(|b| li_seg::faces::cosine(&a, &b).map_err(|_| "invalid face vector".into()))).unwrap_or(-1.)
}
fn lookup<'a>(store: &'a FacesStore, model: &str, key: &FaceKey) -> Option<&'a StoredFace> {
    store.get(model, &key.0)?.get(key.1)
}
fn centroid(keys: &[FaceKey], store: &FacesStore, model: &str) -> Vec<f32> {
    let mut sum = vec![0.; DIM];
    let mut seen = BTreeSet::new();
    for key in keys.iter().filter(|k| seen.insert(*k)) {
        if let Some(face) = lookup(store, model, key) {
            for (a, b) in sum.iter_mut().zip(&face.embedding) {
                *a += b;
            }
        }
    }
    super::store::normalized(sum, DIM).unwrap_or_default()
}
fn refresh_centroids(data: &mut PeopleData, store: &FacesStore, model: &str) {
    for person in &mut data.people {
        let keys: Vec<_> = person.seed_faces.iter().chain(&person.confirmed_faces).cloned().collect();
        person.centroid = centroid(if keys.is_empty() { &person.cluster_faces } else { &keys }, store, model);
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceMatch {
    pub photo: PhotoId,
    pub face: usize,
    pub rect: [f32; 4],
    pub similarity: f32,
    pub confirmed: bool,
    pub sure: bool,
}
/// Rank against all identities, including rejected best matches (never force second place).
pub fn matches(cat: &Catalog, engine: &PeopleEngine, data: &PeopleData, id: u64) -> std::result::Result<Vec<FaceMatch>, String> {
    let person = data.person(id)?;
    let model = engine.model_id();
    let mut out = Vec::new();
    for photo in cat.photos().filter(|p| p.in_library()) {
        let key = crate::media::content_key(photo);
        for (i, face) in engine.store.get(model, &key).unwrap_or(&[]).iter().enumerate() {
            let key = FaceKey(key.clone(), i);
            if person.rejected_faces.contains(&key) {
                continue;
            }
            let confirmed = person.confirmed_faces.contains(&key) || person.seed_faces.contains(&key);
            // Explicit confirmation owns the face even if another centroid is closer.
            if !confirmed && data.people.iter().any(|p| p.id != id && (p.confirmed_faces.contains(&key) || p.seed_faces.contains(&key))) {
                continue;
            }
            let score = similarity(&face.embedding, &person.centroid);
            let other = data.people.iter().filter(|p| p.id != id).map(|p| similarity(&face.embedding, &p.centroid)).fold(-1., f32::max);
            if confirmed || (score >= li_seg::faces::ASSIGN_COSINE && score - other >= li_seg::faces::ASSIGN_MARGIN) {
                out.push(FaceMatch {
                    photo: photo.id,
                    face: i,
                    rect: face.rect,
                    similarity: score,
                    confirmed,
                    sure: confirmed || score >= SURE_COSINE,
                });
            }
        }
    }
    out.sort_by(|a, b| b.similarity.total_cmp(&a.similarity).then(a.photo.cmp(&b.photo)).then(a.face.cmp(&b.face)));
    Ok(out)
}

fn nodes(cat: &Catalog, engine: &PeopleEngine, data: &PeopleData) -> Vec<FaceNode> {
    let mut dates = BTreeMap::new();
    for p in cat.photos().filter(|p| p.in_library()) {
        let date = p.captured.as_deref().and_then(lightcraft_meta::DateTime::parse_iso).map(|d| d.unix_seconds()).unwrap_or(i64::MAX);
        dates.entry(crate::media::content_key(p)).and_modify(|d: &mut i64| *d = (*d).min(date)).or_insert(date);
    }
    let mut out = Vec::new();
    for (key, date) in dates {
        for (i, face) in engine.store.get(engine.model_id(), &key).unwrap_or(&[]).iter().enumerate() {
            let fk = FaceKey(key.clone(), i);
            if data.people.iter().any(|p| p.seed_faces.contains(&fk) || p.confirmed_faces.contains(&fk) || p.cluster_faces.contains(&fk)) {
                continue;
            }
            // Existing identities claim sure/review candidates; a rejected best pair stays
            // available for a new identity rather than being forced onto the runner-up.
            let mut ranked: Vec<_> = data.people.iter().map(|p| (p, similarity(&face.embedding, &p.centroid))).collect();
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.id.cmp(&b.0.id)));
            if ranked.first().is_some_and(|(p, s)| {
                *s >= li_seg::faces::ASSIGN_COSINE
                    && ranked.get(1).is_none_or(|(_, r)| s - r >= li_seg::faces::ASSIGN_MARGIN)
                    && !p.rejected_faces.contains(&fk)
            }) {
                continue;
            }
            if let Ok(embedding) = vector(&face.embedding) {
                out.push(FaceNode { key: NodeKey { photo_date: date, key: key.clone(), face_index: i }, embedding });
            }
        }
    }
    out
}

/// XMP names are seeds only for imported/manual regions, never our own speculative regions.
pub fn refresh(cat: &Catalog, engine: &PeopleEngine, data: &mut PeopleData, cluster: bool) -> std::result::Result<(), String> {
    let model = engine.model_id();
    for photo in cat.photos().filter(|p| p.in_library()) {
        let key = crate::media::content_key(photo);
        for region in photo.meta.regions.iter().filter(|r| !r.auto && r.kind == lightcraft_meta::RegionKind::Face) {
            let Some(name) = region.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) else {
                continue;
            };
            for (i, face) in engine.store.get(model, &key).unwrap_or(&[]).iter().enumerate() {
                let rect = face.bounds();
                let intersection = rect.intersect(&region.rect).area();
                let union = rect.area() + region.rect.area() - intersection;
                if union <= 0. || intersection / union < 0.5 {
                    continue;
                }
                let fk = FaceKey(key.clone(), i);
                if data.people.iter().any(|p| p.confirmed_faces.contains(&fk) || p.seed_faces.contains(&fk)) {
                    continue;
                }
                let id = data.people.iter().find(|p| p.name.eq_ignore_ascii_case(name)).map(|p| p.id).unwrap_or_else(|| data.add(name.into()));
                let p = data.person_mut(id)?;
                if !p.rejected_faces.contains(&fk) {
                    p.confirmed_faces.push(fk);
                }
            }
        }
    }
    refresh_centroids(data, &engine.store, model);
    if cluster {
        let mut unassigned = nodes(cat, engine, data);
        let groups = li_seg::faces::chinese_whispers(&unassigned).map_err(|_| "could not cluster faces")?;
        let mut grouped = BTreeSet::new();
        for group in groups {
            let id = data.add(String::new());
            grouped.extend(group.members.iter().map(|k| FaceKey(k.key.clone(), k.face_index)));
            data.person_mut(id)?.cluster_faces = group.members.into_iter().map(|k| FaceKey(k.key, k.face_index)).collect();
        }
        // Owner items 8/11 extend the original multi-face suggestions: Show everyone and a
        // minimum of one must also expose people seen in only one photo, with stable IDs.
        unassigned.sort_by(|a, b| a.key.cmp(&b.key));
        for node in unassigned {
            let key = FaceKey(node.key.key, node.key.face_index);
            if !grouped.contains(&key) {
                let id = data.add(String::new());
                data.person_mut(id)?.cluster_faces.push(key);
            }
        }
        refresh_centroids(data, &engine.store, model);
    }
    Ok(())
}

/// Project accepted membership into the catalog, retaining imported regions exactly. This
/// permits normal People search and rules to work for unnamed identities as well as names.
pub fn region_ops(cat: &Catalog, engine: &PeopleEngine, data: &PeopleData) -> std::result::Result<Vec<Op>, String> {
    let mut accepted: BTreeMap<PhotoId, Vec<(u64, String, FaceMatch)>> = BTreeMap::new();
    for person in &data.people {
        if person.ignored {
            continue;
        }
        for row in matches(cat, engine, data, person.id)?.into_iter().filter(|r| r.sure) {
            accepted.entry(row.photo).or_default().push((person.id, person.name.clone(), row));
        }
    }
    let mut ops = Vec::new();
    for photo in cat.photos().filter(|p| p.in_library()) {
        let mut meta = photo.meta.clone();
        meta.regions.retain(|r| !r.auto);
        meta.person_ids.clear();
        for (id, name, row) in accepted.get(&photo.id).into_iter().flatten() {
            if !meta.person_ids.contains(id) {
                meta.person_ids.push(*id);
            }
            if !name.is_empty() {
                let face = StoredFace { rect: row.rect, score: 1., embedding: Vec::new() };
                let rect = face.bounds();
                if !meta.regions.iter().any(|r| r.kind == lightcraft_meta::RegionKind::Face && r.rect == rect && r.name.as_deref() == Some(name)) {
                    meta.regions.push(lightcraft_meta::Region {
                        kind: lightcraft_meta::RegionKind::Face,
                        name: Some(name.clone()),
                        rect,
                        description: None,
                        auto: true,
                    });
                }
            }
        }
        meta.person_ids.sort_unstable();
        if meta != photo.meta {
            ops.push(Op::SetMeta { id: photo.id, meta: Box::new(meta) });
        }
    }
    Ok(ops)
}

impl Session {
    pub fn require_faces(&self) -> Result<()> {
        if self.smart.prefs.faces_enabled { Ok(()) } else { Err(EngineError::Other("face recognition is disabled for this library".into())) }
    }
    /// Persist identity changes and catalog regions together in the same history entry.
    pub fn commit_people(&mut self, label: &str, data: PeopleData) -> Result<()> {
        let before = self.smart.people.data.clone();
        let ops = region_ops(&self.catalog, &self.smart.people, &data).map_err(EngineError::Other)?;
        if data == before && ops.is_empty() {
            return Ok(());
        }
        self.smart.people.save_data(&data).map_err(EngineError::Other)?;
        if let Err(e) = self.commit(label, Op::Batch { ops }) {
            self.smart.people.save_data(&before).map_err(EngineError::Other)?;
            return Err(e);
        }
        self.smart.people.data = data;
        if let Some(entry) = self.undo.last_mut() {
            entry.people = Some(before);
        }
        Ok(())
    }
    pub(crate) fn apply_people_history(
        &mut self,
        op: &Op,
        folder: Option<&crate::FolderMove>,
        people: Option<&PeopleData>,
    ) -> Result<(Op, Option<PeopleData>)> {
        let before = people.map(|_| self.smart.people.data.clone());
        let after = people.map(|data| {
            let mut data = data.clone();
            // Undo may remove an identity already referenced by a saved folder. Never reuse
            // its ID for a different person when the user starts a new branch of history.
            data.next_id = data.next_id.max(self.smart.people.data.next_id);
            data
        });
        if let Some(data) = &after {
            self.smart.people.save_data(data).map_err(EngineError::Other)?;
        }
        match self.apply_with_files(op, folder) {
            Ok(inverse) => {
                if let Some(data) = after {
                    self.smart.people.data = data;
                }
                Ok((inverse, before))
            }
            Err(e) => {
                if let Some(before) = before {
                    self.smart.people.save_data(&before).map_err(EngineError::Other)?;
                }
                Err(e)
            }
        }
    }
}

/// Cached thumbnails may have a crop and user rotation. Map detections back to the full
/// EXIF-upright frame before storing them or comparing imported MWG regions.
#[derive(Clone, Copy)]
struct FaceMapping {
    orientation: lightcraft_geom::Orientation,
    crop: lightcraft_geom::CropGeometry,
    width: f64,
    height: f64,
}
impl FaceMapping {
    fn rect(self, rect: [f32; 4]) -> [f32; 4] {
        let [x, y, w, h] = rect.map(f64::from);
        let bounds = lightcraft_meta::Rect::from_xywh(x, y, w, h);
        let transform = self.crop.output_to_source(self.width, self.height, 1., 1.);
        let points = bounds.corners().map(|p| transform.apply(p));
        let x0 = points.iter().map(|p| p.x / self.width).fold(f64::INFINITY, f64::min);
        let y0 = points.iter().map(|p| p.y / self.height).fold(f64::INFINITY, f64::min);
        let x1 = points.iter().map(|p| p.x / self.width).fold(f64::NEG_INFINITY, f64::max);
        let y1 = points.iter().map(|p| p.y / self.height).fold(f64::NEG_INFINITY, f64::max);
        let r = self.orientation.inverse().map_norm_rect(lightcraft_meta::Rect::new(x0, y0, x1, y1)).intersect(&lightcraft_meta::Rect::UNIT);
        [r.x0 as f32, r.y0 as f32, r.width() as f32, r.height() as f32]
    }
}
fn face_input(input: super::InputJob, size: (u32, u32)) -> std::result::Result<(Rgba8, FaceMapping), String> {
    let mut mapping = FaceMapping { orientation: lightcraft_geom::Orientation::Normal, crop: Default::default(), width: 1., height: 1. };
    if let Some((path, loader)) = input.embedded
        && let Some(image) = loader(&path, 1024)
    {
        return Ok((image, mapping));
    }
    mapping.orientation = input.render.settings.orientation;
    let (w, h) = (f64::from(size.0.max(1)), f64::from(size.1.max(1)));
    (mapping.width, mapping.height) = if mapping.orientation.swaps_axes() { (h, w) } else { (w, h) };
    match input.render.run().rendered {
        Ok(r) => Ok((r.image, mapping)),
        Err(error) => match input.fallback {
            Some(job) => {
                mapping.crop = job.settings.crop.geometry;
                job.run().rendered.map(|r| (r.image, mapping)).map_err(|_| error)
            }
            None => Err(error),
        },
    }
}

/// Run a prepared job on a UI worker and return faces in the full upright catalog frame.
/// `size` is the photo's uncropped, EXIF-upright width/height, captured on the session thread.
pub fn detect_input(input: super::InputJob, size: (u32, u32), tagger: &dyn FaceTagger) -> std::result::Result<Vec<StoredFace>, String> {
    let (image, mapping) = face_input(input, size)?;
    let mut faces = tagger.faces(&image)?;
    for face in &mut faces {
        face.rect = mapping.rect(face.rect);
    }
    faces.retain(|f| f.rect[2] > 0. && f.rect[3] > 0.);
    Ok(faces)
}

/// Face-only batches are also usable by UI workers without loading CLIP. Completed records are
/// saved before progress/cancellation and virtual copies reuse one content-key record.
pub fn analyze(s: &mut Session, ids: &[PhotoId], cancel: &AtomicBool, progress: &dyn Fn(usize, usize)) -> Result<super::Analysis> {
    use rayon::prelude::*;
    s.require_faces()?;
    let tagger = s.smart.people.tagger(s.quick_seg_dir.as_deref()).map_err(EngineError::Other)?;
    let model = s.smart.people.ready().map_err(EngineError::Other)?;
    let mut result = super::Analysis::default();
    let mut keys = BTreeSet::new();
    let mut todo = Vec::new();
    for id in ids {
        let Some(p) = s.catalog.photo(*id) else {
            result.failed.push((*id, "unknown photo".into()));
            continue;
        };
        if p.kind == lightcraft_catalog::MediaKind::Video {
            result.videos += 1;
            result.skipped += 1;
            continue;
        }
        let key = crate::media::content_key(p);
        if s.smart.people.store.get(&model, &key).is_some() || !keys.insert(key) {
            result.skipped += 1;
        } else {
            todo.push(*id);
        }
    }
    let threads = std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).max(1));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .map_err(|_| EngineError::Other("could not start face analysis workers".into()))?;
    for batch in todo.chunks(32) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let inputs = super::prepare_inputs(s, batch);
        let prepared: BTreeSet<_> = inputs.iter().map(|j| j.id).collect();
        for id in batch {
            if !prepared.contains(id) {
                result.failed.push((*id, "could not prepare photo".into()));
            }
        }
        let sizes: BTreeMap<_, _> = batch.iter().filter_map(|id| s.catalog.photo(*id).map(|p| (*id, (p.width, p.height)))).collect();
        let rows: Vec<_> = pool.install(|| {
            inputs
                .into_par_iter()
                .map(|input| {
                    let (id, key) = (input.id, input.key.clone());
                    let faces = if cancel.load(Ordering::Relaxed) {
                        None
                    } else {
                        Some(detect_input(input, sizes.get(&id).copied().unwrap_or((1, 1)), tagger.as_ref()))
                    };
                    (id, key, faces)
                })
                .collect()
        });
        for (id, key, faces) in rows {
            if let Some(faces) = faces {
                match faces.and_then(|f| s.smart.people.store.insert(&model, key, f)) {
                    Ok(()) => result.analysed += 1,
                    Err(_) => result.failed.push((id, "could not analyse faces".into())),
                }
            }
        }
        s.smart.people.store.save().map_err(|_| EngineError::Other("could not save face cache".into()))?;
        progress(result.analysed + result.skipped + result.failed.len(), ids.len());
    }
    s.smart.people.store.save().map_err(|_| EngineError::Other("could not save face cache".into()))?;
    let mut data = s.smart.people.data.clone();
    refresh(&s.catalog, &s.smart.people, &mut data, false).map_err(EngineError::Other)?;
    s.commit_people("Smart Sort: analyse faces", data)?;
    result.cancelled = cancel.load(Ordering::Relaxed);
    Ok(result)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cover {
    pub photo: Option<PhotoId>,
    pub source: Option<String>,
    pub key: FaceKey,
    pub rect: [f32; 4],
    pub score: f32,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bubble {
    pub id: u64,
    pub name: String,
    pub pinned: bool,
    pub ignored: bool,
    pub folder_enabled: bool,
    pub count: usize,
    pub folder_count: usize,
    pub pending: usize,
    pub cover: Option<Cover>,
}
pub fn bubbles(cat: &Catalog, engine: &PeopleEngine, folder: Option<&BTreeSet<PhotoId>>) -> std::result::Result<Vec<Bubble>, String> {
    let mut out = Vec::new();
    for p in &engine.data.people {
        let rows = matches(cat, engine, &engine.data, p.id)?;
        let total: BTreeSet<_> = rows.iter().map(|r| r.photo).collect();
        let pending: BTreeSet<_> = rows.iter().filter(|r| !r.confirmed).map(|r| r.photo).collect();
        let mut covers = Vec::new();
        for row in &rows {
            if let Some(photo) = cat.photo(row.photo) {
                let key = FaceKey(crate::media::content_key(photo), row.face);
                if let Some(face) = lookup(&engine.store, engine.model_id(), &key) {
                    covers.push(Cover { photo: Some(row.photo), source: None, key, rect: face.rect, score: face.score });
                }
            }
        }
        for key in &p.seed_faces {
            if let Some(face) = lookup(&engine.store, engine.model_id(), key) {
                covers.push(Cover {
                    photo: None,
                    source: engine.data.seed_sources.get(&key.0).cloned(),
                    key: key.clone(),
                    rect: face.rect,
                    score: face.score,
                });
            }
        }
        covers.sort_by(|a, b| (b.rect[2] * b.rect[3]).total_cmp(&(a.rect[2] * a.rect[3])).then(b.score.total_cmp(&a.score)).then(a.key.cmp(&b.key)));
        out.push(Bubble {
            id: p.id,
            name: p.name.clone(),
            pinned: p.pinned,
            ignored: p.ignored,
            folder_enabled: p.folder_enabled,
            count: total.len(),
            folder_count: folder.map_or(total.len(), |ids| total.intersection(ids).count()),
            pending: pending.len(),
            cover: covers.into_iter().next(),
        });
    }
    out.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.count.cmp(&a.count)).then(a.id.cmp(&b.id)));
    Ok(out)
}

pub fn same_people(data: &PeopleData, id: u64) -> std::result::Result<Vec<(u64, f32)>, String> {
    let person = data.person(id)?;
    let mut out: Vec<_> = data
        .people
        .iter()
        .filter(|p| p.id != id && !p.ignored && !data.different.iter().any(|pair| pair.contains(&id) && pair.contains(&p.id)))
        .map(|p| (p.id, similarity(&person.centroid, &p.centroid)))
        .filter(|(_, s)| *s >= SURE_COSINE)
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(out)
}

pub fn update_centroids(data: &mut PeopleData, engine: &PeopleEngine) {
    refresh_centroids(data, &engine.store, engine.model_id());
}
pub fn face_at<'a>(engine: &'a PeopleEngine, key: &FaceKey) -> Option<&'a StoredFace> {
    lookup(&engine.store, engine.model_id(), key)
}

#[cfg(test)]
mod tests;
