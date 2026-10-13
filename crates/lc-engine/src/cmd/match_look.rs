//! Match Look preparation/analysis/solve transactions. Workers never mutate the session;
//! generation + source + settings snapshots reject stale results. Saved looks contain no IDs.
use super::{CommandSpec, always, bad, cmd, has_selection};
use crate::{
    Result, Session,
    media::{DecodedSource, SourceLevel, SourceRef},
};
use lightcraft_catalog::{Op, PhotoId, Store};
use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::{
    SourceInfo,
    match_look::{self, Look, Options},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

const C: &str = "develop.matchLook";
const FILE: &str = "match-looks.json";
const VERSION: u32 = 2;
const ANALYSIS_VERSION: u32 = 1;
pub const IMPORT_MAX_BYTES: usize = 2 * 1024 * 1024;
pub const LOOK_NAME_MAX_CHARS: usize = 128;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    pub id: PhotoId,
    pub key: String,
    pub settings: Arc<DevelopSettings>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub version: u32,
    pub identities: Vec<Identity>,
    pub looks: Vec<Look>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    pub references: Vec<PhotoId>,
    pub reference_weights: std::collections::BTreeMap<PhotoId, f64>,
    #[serde(skip)]
    pub undone: std::collections::BTreeMap<PhotoId, Applied>,
    #[serde(skip)]
    pub analysis: Option<Analysis>,
    pub options: Options,
    pub selected: Option<String>,
    #[serde(skip)]
    pub applied: std::collections::BTreeMap<PhotoId, Applied>,
}
/// Catalog-local amount history, stored separately on close. Global saved looks contain only
/// statistics; this is never serialized into prefs or a saved look or allowed to overwrite later edits.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Applied {
    pub key: String,
    pub before: Arc<DevelopSettings>,
    pub full: DevelopSettings,
    pub last: DevelopSettings,
    pub options: Options,
    pub similarity: f64,
    #[serde(default = "full_reach")]
    pub fit: f64,
    #[serde(default = "full_reach")]
    pub reach: f64,
    #[serde(default = "full_reach")]
    pub wb_ratio: f64,
    #[serde(default)]
    pub preview_only: bool,
    pub weights: Vec<f64>,
}
fn full_reach() -> f64 {
    1.0
}
#[derive(Serialize, Deserialize)]
pub struct AppliedFile {
    pub version: u32,
    pub records: std::collections::BTreeMap<PhotoId, Applied>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SavedLook {
    pub id: String,
    pub name: String,
    pub created: String,
    pub reference_count: usize,
    pub looks: Vec<Look>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LookFile {
    pub version: u32,
    pub looks: Vec<SavedLook>,
}
impl Default for LookFile {
    fn default() -> Self {
        Self { version: VERSION, looks: Vec::new() }
    }
}
#[derive(Default)]
pub struct SavedLooks {
    pub file: LookFile,
    store: Option<Box<dyn Store>>,
    pub error: Option<String>,
}
impl SavedLooks {
    pub fn loaded(&self) -> bool {
        self.store.is_some() || self.error.is_some()
    }

    /// Inject the host's global Presets store (or MemStore for tests/browser). Never silently
    /// replace corrupt/unknown-version data. Atomic writes happen before changing memory.
    pub fn open(&mut self, mut store: Box<dyn Store>) -> std::result::Result<(), String> {
        let result = (|| {
            let loaded = store
                .read(FILE)
                .map_err(|e| e.to_string())?
                .map(|b| serde_json::from_slice::<LookFile>(&b).map_err(|e| e.to_string()))
                .transpose()?;
            let mut file = loaded.unwrap_or_default();
            validate_file(&file)?;
            file.version = VERSION;
            Ok::<_, String>(file)
        })();
        match result {
            Ok(file) => {
                self.file = file;
                self.store = Some(store);
                self.error = None;
                Ok(())
            }
            Err(error) => {
                self.error = Some(error.clone());
                Err(error)
            }
        }
    }

    fn replace(&mut self, file: LookFile) -> Result<()> {
        validate_file(&file).map_err(|e| bad(C, e))?;
        if let Some(e) = &self.error {
            return Err(bad(C, e.clone()));
        }
        if let Some(store) = &mut self.store {
            let bytes = serde_json::to_vec_pretty(&file).map_err(|e| bad(C, e.to_string()))?;
            store.write_atomic(FILE, &bytes).map_err(|e| bad(C, e.to_string()))?;
        }
        self.file = file;
        Ok(())
    }
}
fn validate_file(f: &LookFile) -> std::result::Result<(), String> {
    if f.version != VERSION && f.version != 1 {
        return Err("Unsupported saved-look format version".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for l in &f.looks {
        if l.id.is_empty()
            || l.name.trim().is_empty()
            || l.name.chars().count() > LOOK_NAME_MAX_CHARS
            || l.name.len() > LOOK_NAME_MAX_CHARS * 4
            || l.id.len() > 128
            || l.created.len() > 64
            || !ids.insert(&l.id)
            || l.looks.is_empty()
            || l.looks.len() > 10
            || l.reference_count != l.looks.len()
            || l.looks.iter().any(|l| !l.valid() || l.support.iter().all(|w| *w == 0.0))
        {
            return Err("Invalid saved look".into());
        }
    }
    Ok(())
}
pub fn reconcile_selection(s: &mut Session) -> bool {
    if s.match_look.selected.as_ref().is_some_and(|id| s.saved_looks.error.is_some() || !s.saved_looks.file.looks.iter().any(|l| &l.id == id)) {
        s.match_look.selected = None;
        return true;
    }
    false
}
/// Bound file bytes before parsing; the global look collection deliberately has no count/size cap.
pub fn read_import(path: &std::path::Path) -> std::result::Result<LookFile, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > IMPORT_MAX_BYTES as u64 {
        return Err("Look file exceeds 2 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(IMPORT_MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    parse_import(&bytes)
}
pub fn parse_import(bytes: &[u8]) -> std::result::Result<LookFile, String> {
    if bytes.len() > IMPORT_MAX_BYTES {
        return Err("Look file exceeds 2 MiB".into());
    }
    let file: LookFile = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    validate_file(&file)?;
    Ok(file)
}
pub fn identity(s: &Session, id: PhotoId) -> Option<Identity> {
    let p = s.catalog.photo(id)?;
    p.in_library().then(|| Identity { id, key: crate::media::content_key(p), settings: p.develop.clone() })
}
fn unchanged(s: &Session, i: &Identity) -> bool {
    identity(s, i.id).as_ref() == Some(i)
}
pub fn current_ready(s: &Session) -> bool {
    s.match_look.analysis.as_ref().is_some_and(|a| {
        a.version == ANALYSIS_VERSION
            && a.looks.len() == s.match_look.references.len()
            && !a.looks.is_empty()
            && a.looks.iter().all(Look::valid)
            && a.identities.iter().map(|i| i.id).eq(s.match_look.references.iter().copied())
            && a.identities.iter().all(|i| unchanged(s, i))
    })
}
pub fn selected_looks(s: &Session) -> Option<Vec<Look>> {
    if s.saved_looks.error.is_none()
        && let Some(id) = &s.match_look.selected
        && let Some(look) = s.saved_looks.file.looks.iter().find(|l| l.id == *id)
    {
        return Some(look.looks.clone());
    }
    current_ready(s).then(|| {
        s.match_look
            .analysis
            .as_ref()
            .map(|a| {
                a.looks
                    .iter()
                    .cloned()
                    .zip(&s.match_look.references)
                    .map(|(mut look, id)| {
                        look.importance = if s.match_look.reference_weights.get(id) == Some(&2.0) { 2.0 } else { 1.0 };
                        look
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}
pub fn ready(s: &Session) -> std::result::Result<(), String> {
    has_selection(s)?;
    selected_looks(s).map(|_| ()).ok_or_else(|| "Analyze references or choose a saved look first".into())
}
#[derive(Default)]
pub struct Control {
    pub cancel: AtomicBool,
    pub completed: AtomicUsize,
    pub total: usize,
}
#[derive(Clone)]
struct Input {
    identity: Identity,
    source: SourceRef,
    verification: SourceRef,
    info: SourceInfo,
    bytes: u64,
    faces: Vec<[f32; 4]>,
    preview_only: bool,
}
fn input(s: &mut Session, id: PhotoId) -> Result<Input> {
    let photo = s.catalog.photo(id).cloned().ok_or_else(|| bad(C, "Unknown photo"))?;
    if photo.kind == lightcraft_catalog::MediaKind::Video || !photo.in_library() {
        return Err(bad(C, format!("Choose a still photo in the catalog: {}", photo.file_name)));
    }
    Ok(Input {
        preview_only: photo.preview_only.is_some(),
        bytes: u64::from(photo.width.min(512))
            .saturating_mul(u64::from(photo.height.min(512)))
            .saturating_mul(32)
            .saturating_add(16 << 20)
            .saturating_add(if photo.develops_raw() {
                u64::from(photo.width).saturating_mul(u64::from(photo.height)).saturating_mul(6).saturating_add(photo.file_size)
            } else {
                0
            }),
        faces: s
            .smart
            .people
            .store
            .get(s.smart.people.model_id(), &crate::media::content_key(&photo))
            .unwrap_or(&[])
            .iter()
            .map(|f| f.rect)
            .collect(),
        identity: identity(s, id).ok_or_else(|| bad(C, "Missing photo"))?,
        info: s.source_info(id),
        source: s.media.source_ref(&photo, SourceLevel::Thumb),
        verification: s.media.source_ref(&photo, SourceLevel::Preview),
    })
}
/// Audit near matches at the user-visible preview level. Proxy fitting is an approximation;
/// its local-contrast frequency and conditional palette support must not worsen a good photo.
fn near_match_errors(
    src: &Arc<lightcraft_raster::Rgb32f>,
    info: &SourceInfo,
    before: &DevelopSettings,
    after: &DevelopSettings,
    refs: &[Look],
    weights: &[f64],
    opts: Options,
    similarity: f64,
) -> (f64, f64) {
    let edge = 1024.min(src.width.max(src.height));
    let req = lightcraft_pipeline::RenderRequest::fit(edge, edge);
    let a = match_look::measure(&lightcraft_pipeline::render(src, info, before, &req).image);
    let b = match_look::measure(&lightcraft_pipeline::render(src, info, after, &req).image);
    let (goal, _) = match_look::reachable_goal(&a, &match_look::blend(refs, weights), opts, similarity);
    (match_look::distance(&a, &goal, opts), match_look::distance(&b, &goal, opts))
}
pub struct AnalyzeWork {
    generation: u64,
    inputs: Vec<Input>,
}
pub struct Analyzed {
    generation: u64,
    analysis: Analysis,
    decoded: Vec<(PhotoId, DecodedSource)>,
}
pub fn prepare_analysis(s: &mut Session) -> Result<AnalyzeWork> {
    let ids = s.match_look.references.clone();
    if ids.is_empty() || ids.len() > 10 {
        return Err(bad(C, "Choose 1–10 reference photos"));
    }
    let inputs = ids.into_iter().map(|id| input(s, id)).collect::<Result<Vec<_>>>()?;
    Ok(AnalyzeWork { generation: s.library_generation, inputs })
}
impl AnalyzeWork {
    pub fn len(&self) -> usize {
        self.inputs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }
    pub fn run(self, c: &Control) -> std::result::Result<Analyzed, String> {
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
        let stop = || c.cancel.load(Ordering::Relaxed);
        let mut decoded = Vec::new();
        let mut a = Analysis { version: ANALYSIS_VERSION, identities: Vec::new(), looks: Vec::new() };
        for input in self.inputs {
            let _permit = lightcraft_sysmem::heavy_jobs().admit(input.bytes, &stop).ok_or("Cancelled")?;
            let src = lightcraft_sysmem::in_background_pool(|| input.source.load_source())?;
            if !matches!(input.source, SourceRef::Loaded(_)) {
                decoded.push((input.identity.id, src.clone()));
            }
            if stop() {
                return Err("Cancelled".into());
            }
            a.looks.push(lightcraft_sysmem::in_background_pool(|| {
                match_look::analyze_with_faces(&src.image, &src.info_or(input.info), &input.identity.settings, &input.faces)
            }));
            a.identities.push(input.identity);
            c.completed.fetch_add(1, Ordering::Relaxed);
        }
        if stop() {
            return Err("Cancelled".into());
        }
        Ok(Analyzed { generation: self.generation, analysis: a, decoded })
    }
}
pub fn apply_analysis(s: &mut Session, result: Analyzed, update: Option<&str>) -> Result<Value> {
    if s.library_generation != result.generation
        || result.analysis.identities.iter().any(|i| !unchanged(s, i))
        || !result.analysis.identities.iter().map(|i| i.id).eq(s.match_look.references.iter().copied())
    {
        return Ok(json!({"discarded":true}));
    }
    for (id, src) in result.decoded {
        s.media.insert_source(id, SourceLevel::Thumb, src);
    }
    if let Some(id) = update {
        let mut file = s.saved_looks.file.clone();
        let look = file.looks.iter_mut().find(|l| l.id == id).ok_or_else(|| bad(C, "Look was deleted"))?;
        look.looks = result.analysis.looks.clone();
        for (look, id) in look.looks.iter_mut().zip(&s.match_look.references) {
            look.importance = if s.match_look.reference_weights.get(id) == Some(&2.0) { 2.0 } else { 1.0 };
        }
        look.reference_count = look.looks.len();
        s.saved_looks.replace(file)?;
    }
    let preview_only = result.analysis.identities.iter().filter(|i| s.catalog.photo(i.id).is_some_and(|p| p.preview_only.is_some())).count();
    let n = result.analysis.looks.len();
    s.match_look.analysis = Some(result.analysis);
    if update.is_none() {
        s.match_look.selected = None;
    }
    s.save_match_look_analysis()?;
    s.save_prefs()?;
    Ok(json!({"analyzed":n,"previewOnly":preview_only}))
}
pub struct SolveWork {
    generation: u64,
    inputs: Vec<Input>,
    looks: Arc<[Look]>,
    opts: Options,
    failed: Vec<Value>,
}
pub struct Matched {
    pub decoded: Vec<(PhotoId, DecodedSource)>,
    generation: u64,
    pub rows: Vec<(Identity, DevelopSettings)>,
    pub applications: std::collections::BTreeMap<PhotoId, Applied>,
    pub failed: Vec<Value>,
    pub preview: Option<lightcraft_raster::Rgba8>,
    pub preview_before: Option<lightcraft_raster::Rgba8>,
    pub elapsed_ms: f64,
    pub load_ms: f64,
    pub solve_ms: f64,
    pub renders: usize,
    pub workers: usize,
    pub per_photo: Vec<Value>,
}
pub fn prepare(s: &mut Session, p: &Value) -> Result<SolveWork> {
    ready(s).map_err(|e| bad(C, e))?;
    let opts: Options = serde_json::from_value(p.clone()).map_err(|e| bad(C, e.to_string()))?;
    if !opts.strength.is_finite() || !(0.0..=125.0).contains(&opts.strength) {
        return Err(bad(C, "Strength must be 0–125"));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut failed = Vec::new();
    let inputs = s
        .targets(p)
        .into_iter()
        .filter(|id| seen.insert(*id))
        .filter_map(|id| match input(s, id) {
            Ok(input) => Some(input),
            Err(error) => {
                failed.push(json!([id.0, error.to_string()]));
                None
            }
        })
        .collect::<Vec<_>>();
    if inputs.is_empty() {
        return Err(bad(C, "No still photos in the selection"));
    }
    Ok(SolveWork { generation: s.library_generation, inputs, failed, looks: selected_looks(s).ok_or_else(|| bad(C, "Analyze first"))?.into(), opts })
}
impl SolveWork {
    pub fn len(&self) -> usize {
        self.inputs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }
    pub fn run(self, c: &Control, preview: bool) -> Matched {
        let bytes = self.inputs.iter().map(|i| i.bytes).max().unwrap_or(1);
        let jobs = lightcraft_sysmem::heavy_jobs();
        // OS coordinators may overlap their bounded CPU solves with heavy raw decodes.
        // The shared permit still limits the decode slots and memory independently.
        let workers = if preview || cfg!(target_arch = "wasm32") {
            1
        } else {
            self.inputs.len().min(lightcraft_sysmem::profile().threads).min((jobs.headroom() / bytes).max(1) as usize).max(1)
        };
        if workers == 1 {
            return self.run_sequential(c, preview);
        }
        let start = web_time::Instant::now();
        let next = AtomicUsize::new(0);
        let results = std::sync::Mutex::new(Vec::new());
        let mut workers_started = 0;
        std::thread::scope(|scope| {
            for worker in 0..workers {
                let work = &self;
                let next = &next;
                let results = &results;
                let spawned = std::thread::Builder::new().name(format!("match-look-{worker}")).spawn_scoped(scope, move || {
                    lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
                    loop {
                        if c.cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(input) = work.inputs.get(index) else {
                            break;
                        };
                        let result = SolveWork {
                            generation: work.generation,
                            inputs: vec![input.clone()],
                            looks: work.looks.clone(),
                            opts: work.opts,
                            failed: Vec::new(),
                        }
                        .run_sequential(c, false);
                        results.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((index, result));
                    }
                });
                match spawned {
                    Ok(_) => workers_started += 1,
                    Err(error) => log::warn!("Could not start Match Look worker: {error}; continuing with available workers"),
                }
            }
        });
        if workers_started == 0 {
            return self.run_sequential(c, preview);
        }
        let mut results = results.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
        results.sort_by_key(|(index, _)| *index);
        let mut out = Matched {
            decoded: Vec::new(),
            generation: self.generation,
            rows: Vec::new(),
            applications: Default::default(),
            failed: self.failed,
            preview: None,
            preview_before: None,
            elapsed_ms: 0.0,
            load_ms: 0.0,
            solve_ms: 0.0,
            renders: 0,
            workers: workers_started,
            per_photo: Vec::new(),
        };
        for (_, result) in results {
            out.decoded.extend(result.decoded);
            out.rows.extend(result.rows);
            out.applications.extend(result.applications);
            out.failed.extend(result.failed);
            out.per_photo.extend(result.per_photo);
            out.load_ms += result.load_ms;
            out.solve_ms += result.solve_ms;
            out.renders += result.renders;
        }
        out.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        out
    }
    #[cfg(test)]
    pub(crate) fn run_serial(self, c: &Control) -> Matched {
        self.run_sequential(c, false)
    }
    fn run_sequential(self, c: &Control, preview: bool) -> Matched {
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
        let start = web_time::Instant::now();
        let stop = || c.cancel.load(Ordering::Relaxed);
        let mut out = Matched {
            decoded: Vec::new(),
            generation: self.generation,
            rows: Vec::new(),
            applications: Default::default(),
            failed: self.failed,
            preview: None,
            preview_before: None,
            elapsed_ms: 0.0,
            load_ms: 0.0,
            solve_ms: 0.0,
            renders: 0,
            workers: 1,
            per_photo: Vec::new(),
        };
        for i in self.inputs {
            if stop() {
                break;
            }
            let Some(mut permit) = lightcraft_sysmem::heavy_jobs().admit(i.bytes, &stop) else { break };
            let load_start = web_time::Instant::now();
            match lightcraft_sysmem::in_background_pool(|| i.source.load_source()) {
                Ok(src) => {
                    if !matches!(i.source, SourceRef::Loaded(_)) {
                        out.decoded.push((i.identity.id, src.clone()));
                    }
                    // Sensor/file buffers are gone. The bounded Thumb/probe working set is
                    // small: release the heavy decode slot while keeping its memory reserved,
                    // so the next raw can decode during this photo's CPU-only solve.
                    permit.finish_heavy((src.image.data.len() as u64).saturating_mul(12).saturating_add(16 << 20));
                    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;
                    out.load_ms += load_ms;
                    let id = i.identity.id;
                    let solve_start = web_time::Instant::now();
                    let info = src.info_or(i.info);
                    if let Some(mut solved) = lightcraft_sysmem::in_background_pool(|| {
                        if preview {
                            match_look::solve_with_faces(&src.image, &info, &i.identity.settings, &self.looks, self.opts, &i.faces, &stop)
                        } else {
                            match_look::solve_batch_with_faces(&src.image, &info, &i.identity.settings, &self.looks, self.opts, &i.faces, &stop)
                        }
                    }) {
                        let already_close = solved.before_error < 0.05;
                        if already_close && solved.settings != *i.identity.settings {
                            // A separate heavy permit bounds the rare larger preview decode.
                            let verify_bytes = i.bytes.saturating_add(2560 * 2560 * 32);
                            if let Some(_verify_permit) = lightcraft_sysmem::heavy_jobs().admit(verify_bytes, &stop) {
                                if let Ok(verified) = lightcraft_sysmem::in_background_pool(|| i.verification.load_source()) {
                                    let verified_info = verified.info_or(info.clone());
                                    let (before, after) = lightcraft_sysmem::in_background_pool(|| {
                                        lightcraft_raster::sequential(|| {
                                            near_match_errors(
                                                &verified.image,
                                                &verified_info,
                                                &i.identity.settings,
                                                &solved.settings,
                                                &self.looks,
                                                &solved.weights,
                                                self.opts,
                                                solved.similarity,
                                            )
                                        })
                                    });
                                    out.renders += 2;
                                    if after >= before {
                                        solved.settings = (*i.identity.settings).clone();
                                        solved.full_settings = solved.settings.clone();
                                        solved.after_error = solved.before_error;
                                        solved.fit = 0.0;
                                    }
                                } else {
                                    // If a close result cannot be audited, retain the existing look.
                                    solved.settings = (*i.identity.settings).clone();
                                    solved.full_settings = solved.settings.clone();
                                    solved.after_error = solved.before_error;
                                    solved.fit = 0.0;
                                }
                            }
                        }
                        out.renders += solved.renders;
                        let similarity = solved.similarity;
                        let wb_ratio = if self.opts.new_layer {
                            solved.settings.masks.last().map_or(1.0, |m| (m.adjust.temp * 0.6 / 100.0).exp())
                        } else {
                            lightcraft_pipeline::auto::current_wb(&info, &solved.settings).0
                                / lightcraft_pipeline::auto::current_wb(&info, &i.identity.settings).0
                        };
                        out.per_photo.push(json!({"id":id,"fit":solved.fit,"reach":solved.reach,"similarity":similarity,"wbRatio":wb_ratio,"previewOnly":i.preview_only,"alreadyClose":already_close}));
                        out.applications.insert(
                            i.identity.id,
                            Applied {
                                key: i.identity.key.clone(),
                                before: i.identity.settings.clone(),
                                full: solved.full_settings,
                                last: solved.settings.clone(),
                                options: self.opts,
                                similarity,
                                fit: solved.fit,
                                reach: solved.reach,
                                wb_ratio,
                                preview_only: i.preview_only,
                                weights: solved.weights,
                            },
                        );
                        if preview {
                            out.preview_before = Some(
                                lightcraft_pipeline::render(
                                    &src.image,
                                    &info,
                                    &i.identity.settings,
                                    &lightcraft_pipeline::RenderRequest::fit(400, 400),
                                )
                                .image,
                            );
                            out.preview = Some(
                                lightcraft_pipeline::render(&src.image, &info, &solved.settings, &lightcraft_pipeline::RenderRequest::fit(400, 400))
                                    .image,
                            );
                        }
                        out.rows.push((i.identity, solved.settings));
                    }
                    let solve_ms = solve_start.elapsed().as_secs_f64() * 1000.0;
                    out.solve_ms += solve_ms;
                    if let Some(row) = out.per_photo.last_mut().filter(|row| row["id"] == json!(id)) {
                        row["loadMs"] = json!(load_ms);
                        row["solveMs"] = json!(solve_ms);
                    }
                }
                Err(e) => out.failed.push(json!([i.identity.id.0, e])),
            }
            c.completed.fetch_add(1, Ordering::Relaxed);
        }
        out.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        out
    }
}
/// Install source pixels together with their decoder facts only for the originating library.
pub fn retain_decoded(s: &mut Session, result: &mut Matched) {
    if s.library_generation == result.generation {
        for (id, src) in result.decoded.drain(..) {
            if result.rows.iter().any(|(captured, _)| captured.id == id && identity(s, id).is_some_and(|now| now.key == captured.key)) {
                s.media.insert_source(id, SourceLevel::Thumb, src);
            }
        }
    }
}
pub fn apply(s: &mut Session, mut result: Matched) -> Result<Value> {
    // Like Sync, finish the batch in one transaction; protect only targets changed since capture.
    if s.library_generation != result.generation {
        return Ok(json!({"changed":0,"skipped":[],"discarded":true}));
    }
    retain_decoded(s, &mut result);
    let mut skipped = Vec::new();
    let mut ops = Vec::new();
    let mut applications = result.applications;
    let mut installed = Vec::new();
    for (identity, settings) in result.rows {
        if !unchanged(s, &identity) {
            skipped.push(identity.id);
            continue;
        }
        if identity.settings.as_ref() != &settings {
            ops.extend(s.develop_op(identity.id, settings, "Match Look"));
            installed.push(identity.id);
        }
    }
    let per_photo = result.per_photo.into_iter().filter(|row| !skipped.iter().any(|id| row["id"] == json!(id))).collect::<Vec<_>>();
    let weak = per_photo
        .iter()
        .filter(|r| r["reach"].as_f64().unwrap_or(1.0) < 0.6 || (r["alreadyClose"] != true && r["fit"].as_f64().unwrap_or(1.0) < 0.5))
        .count();
    let preview_only = per_photo.iter().filter(|r| r["previewOnly"] == true).count();
    let n = ops.len();
    if n > 0 {
        s.commit("Match Look", Op::Batch { ops })?;
        for id in installed {
            if let Some(record) = applications.remove(&id) {
                s.match_look.applied.insert(id, record);
            }
        }
    }
    Ok(
        json!({"changed":n,"weak":weak,"previewOnly":preview_only,"skipped":skipped,"failed":result.failed,"elapsedMs":result.elapsed_ms,"loadMs":result.load_ms,"solveMs":result.solve_ms,"renders":result.renders,"workers":result.workers,"perPhoto":per_photo}),
    )
}
fn references(s: &mut Session, p: &Value) -> Result<Value> {
    let mut ids = s.match_look.references.clone();
    if let Some(id) = p["weightId"].as_u64() {
        let id = PhotoId(id);
        if !ids.contains(&id) {
            return Err(bad(C, "Unknown reference"));
        }
        let weight = if s.match_look.reference_weights.get(&id) == Some(&2.0) { 1.0 } else { 2.0 };
        s.match_look.reference_weights.insert(id, weight);
        s.save_prefs()?;
        return status(s, &Value::Null);
    }
    if let Some(remove) = p["remove"].as_u64() {
        ids.retain(|i| i.0 != remove);
    } else {
        let incoming = s.targets(p);
        if incoming.is_empty() {
            return Err(bad(C, "Select reference photos first"));
        }
        if p["replace"] == true {
            ids.clear();
        }
        for id in incoming {
            input(s, id)?;
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        if ids.len() > 10 {
            return Err(bad(C, "At most 10 references; remove a reference first"));
        }
    }
    s.match_look.reference_weights.retain(|id, _| ids.contains(id));
    s.match_look.references = ids;
    s.save_prefs()?;
    status(s, &Value::Null)
}
fn status(s: &mut Session, _: &Value) -> Result<Value> {
    Ok(
        json!({"references":s.match_look.references,"analyzed":current_ready(s),"ready":selected_looks(s).is_some(),"selected":s.match_look.selected,"looks":s.saved_looks.file.looks}),
    )
}
fn saved(s: &mut Session, p: &Value) -> Result<Value> {
    reconcile_selection(s);
    let action = p["action"].as_str().unwrap_or("save");
    if action == "importFile" {
        let path = p["path"].as_str().ok_or_else(|| bad(C, "Choose a look file"))?;
        let imported = read_import(std::path::Path::new(path)).map_err(|e| bad(C, e))?;
        return saved(s, &json!({"action":"import","file":imported}));
    }
    if action == "exportFile" {
        let path = p["path"].as_str().ok_or_else(|| bad(C, "Choose a destination"))?;
        s.check_write_target(path).map_err(|e| bad(C, e))?;
        let id = p["id"].as_str().map(str::to_string).or_else(|| s.match_look.selected.clone());
        let look = if let Some(id) = id {
            s.saved_looks.file.looks.iter().find(|l| l.id == id).cloned().ok_or_else(|| bad(C, "Unknown saved look"))?
        } else {
            let looks = selected_looks(s).ok_or_else(|| bad(C, "Analyze references before exporting"))?;
            SavedLook {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Analyzed references".into(),
                created: (s.clock)(),
                reference_count: looks.len(),
                looks,
            }
        };
        let bytes = serde_json::to_vec_pretty(&LookFile { version: VERSION, looks: vec![look] }).map_err(|e| bad(C, e.to_string()))?;
        parse_import(&bytes).map_err(|e| bad(C, e))?;
        let path = std::path::Path::new(path);
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
        let name = path.file_name().and_then(|v| v.to_str()).ok_or_else(|| bad(C, "Invalid destination"))?;
        let mut store = lightcraft_catalog::FsStore::open(parent).map_err(|e| bad(C, e.to_string()))?;
        store.write_atomic(name, &bytes).map_err(|e| bad(C, e.to_string()))?;
        return Ok(json!({"exported":1,"path":path}));
    }
    if action == "select" {
        let id = p["id"].as_str();
        if id.is_some_and(|id| !s.saved_looks.file.looks.iter().any(|l| l.id == id)) {
            return Err(bad(C, "Unknown saved look"));
        }
        s.match_look.selected = id.map(str::to_string);
        s.save_prefs()?;
        return status(s, p);
    }
    if action == "export" {
        return Ok(json!(s.saved_looks.file));
    }
    let mut file = s.saved_looks.file.clone();
    if action == "save" {
        if !current_ready(s) {
            return Err(bad(C, "Analyze current references before saving"));
        }
        let name = p["name"].as_str().unwrap_or("").trim();
        if name.is_empty() {
            return Err(bad(C, "Enter a look name"));
        }
        let mut looks = s.match_look.analysis.as_ref().map(|a| a.looks.clone()).unwrap_or_default();
        for (look, id) in looks.iter_mut().zip(&s.match_look.references) {
            look.importance = if s.match_look.reference_weights.get(id) == Some(&2.0) { 2.0 } else { 1.0 };
        }
        let id = uuid::Uuid::new_v4().to_string();
        file.looks.push(SavedLook { id: id.clone(), name: name.into(), created: (s.clock)(), reference_count: looks.len(), looks });
        s.saved_looks.replace(file)?;
        s.match_look.selected = Some(id.clone());
        s.save_prefs()?;
        return Ok(json!({"id":id}));
    } else if action == "import" {
        let bytes = serde_json::to_vec(&p["file"]).map_err(|e| bad(C, e.to_string()))?;
        let imported = parse_import(&bytes).map_err(|e| bad(C, e))?;
        for mut l in imported.looks {
            if file.looks.iter().any(|existing| existing.looks == l.looks) {
                continue;
            }
            if file.looks.iter().any(|existing| existing.name == l.name) {
                l.name = format!("{} (imported)", l.name.chars().take(LOOK_NAME_MAX_CHARS - 11).collect::<String>());
            }
            l.id = uuid::Uuid::new_v4().to_string();
            file.looks.push(l);
        }
    } else {
        let id = p["id"].as_str().ok_or_else(|| bad(C, "Missing look id"))?;
        let index = file.looks.iter().position(|l| l.id == id).ok_or_else(|| bad(C, "Unknown saved look"))?;
        match action {
            "rename" => {
                let name = p["name"].as_str().unwrap_or("").trim();
                if name.is_empty() {
                    return Err(bad(C, "Enter a look name"));
                }
                file.looks[index].name = name.into();
            }
            "delete" => {
                file.looks.remove(index);
            }
            "duplicate" => {
                let mut l = file.looks[index].clone();
                l.id = uuid::Uuid::new_v4().to_string();
                l.name = format!("{} copy", l.name.chars().take(LOOK_NAME_MAX_CHARS - 5).collect::<String>());
                l.created = (s.clock)();
                file.looks.push(l);
            }
            _ => return Err(bad(C, "Unknown saved-look action")),
        }
    }
    s.saved_looks.replace(file)?;
    if s.match_look.selected.as_ref().is_some_and(|id| !s.saved_looks.file.looks.iter().any(|l| &l.id == id)) {
        s.match_look.selected = None;
    }
    s.save_prefs()?;
    status(s, p)
}
pub fn reconcile_applied(s: &mut Session) {
    if s.interaction.is_some() {
        return;
    }
    let undone = s
        .match_look
        .applied
        .iter()
        .filter(|(id, r)| r.last != *r.before && s.develop_of(**id).as_deref() == Some(r.before.as_ref()))
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    for id in undone {
        if let Some(record) = s.match_look.applied.remove(&id) {
            s.match_look.undone.insert(id, record);
        }
    }
}
pub fn restore_applied(s: &mut Session) {
    let ids = s
        .match_look
        .undone
        .iter()
        .filter(|(id, r)| s.develop_of(**id).as_deref() == Some(&r.last) && identity(s, **id).is_some_and(|i| i.key == r.key))
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    for id in ids {
        if let Some(r) = s.match_look.undone.remove(&id) {
            s.match_look.applied.insert(id, r);
        }
    }
}
pub fn export_name(s: &Session, id: Option<&str>) -> String {
    let id = id.or(s.match_look.selected.as_deref());
    let name = id.and_then(|id| s.saved_looks.file.looks.iter().find(|l| l.id == id)).map_or("Analyzed references", |look| look.name.as_str());
    let name = name.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect::<String>();
    let name = name.trim_matches([' ', '.']);
    format!("{}.json", if name.is_empty() { "Match Look" } else { name })
}
fn amount(s: &mut Session, p: &Value) -> Result<Value> {
    reconcile_applied(s);
    let id = s.active().ok_or_else(|| bad(C, "Select an applied photo"))?;
    let mut record = s.match_look.applied.get(&id).cloned().ok_or_else(|| bad(C, "No Match Look amount for this photo"))?;
    if s.develop_of(id).as_deref() != Some(&record.last) || identity(s, id).is_none_or(|i| i.key != record.key) {
        return Err(bad(C, "Photo edited since Match Look; undo those edits before changing its amount"));
    }
    if let Some(group) = p["resetGroup"].as_str() {
        record.full =
            match_look::reset_group(&record.before, &record.full, record.options, group).ok_or_else(|| bad(C, "Unknown Match Look group"))?;
        match group {
            "normalize" => record.options.normalize = false,
            "light" => record.options.light_contrast = false,
            "color" => record.options.color_adjust = false,
            "tone" => record.options.color_tone = false,
            _ => {}
        }
    } else {
        let strength =
            p["strength"].as_f64().filter(|x| x.is_finite() && (0.0..=125.0).contains(x)).ok_or_else(|| bad(C, "Strength must be 0–125"))?;
        record.options.strength = strength;
    }
    if p["interaction"] == "begin" {
        s.begin_interaction("Match Look amount")?;
    }
    let d = match_look::amount_settings(&record.before, &record.full, record.options);
    if d != record.last {
        s.set_develop(id, d.clone(), "Match Look amount")?;
    }
    if p["interaction"] == "end" {
        s.end_interaction()?;
    }
    record.last = d;
    s.match_look.applied.insert(id, record);
    Ok(json!({"changed":1}))
}
pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "develop.matchLookAmount",
            "Match Look amount",
            [],
            None,
            "{strength?:0..125, resetGroup?:normalize|light|color|tone, interaction?:begin|update|end}",
            has_selection,
            amount
        ),
        cmd!(
            "develop.matchLookReferences",
            "Add to Match Look References",
            ["Photo"],
            None,
            "{ids?, remove?, replace?} — add up to 10 edited reference photos",
            always,
            references
        ),
        cmd!(query "develop.matchLookStatus","Match Look Status",[],None,"{}",always,status),
        cmd!(
            "develop.analyzeLook",
            "Analyze Match Look",
            [],
            None,
            "{update?: savedLookId} — UI runs as a cancellable background job",
            always,
            |s, p| {
                let result = prepare_analysis(s)?.run(&Control::default()).map_err(|e| bad(C, e))?;
                apply_analysis(s, result, p["update"].as_str())
            }
        ),
        cmd!(
            "develop.matchLook",
            "Match Look",
            [],
            None,
            "{ids?, normalize?, lightContrast?, colorAdjust?, colorTone?, strength?:0..125, newLayer?, consistentBatch?} — solve editable controls, one undo",
            ready,
            |s, p| {
                let result = prepare(s, p)?.run(&Control::default(), false);
                apply(s, result)
            }
        ),
        cmd!(
            "develop.savedLook",
            "Saved Looks",
            [],
            None,
            "{action: save|select|rename|delete|duplicate|export|import|exportFile|importFile, id?, name?, file?}",
            always,
            saved
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_catalog::{MemStore, Store};
    fn setup() -> Session {
        let mut s = Session::with_demo();
        let id = s.active().unwrap();
        s.execute("develop.set", &json!({"control":"light.exposure","value":0.5})).unwrap();
        s.execute("develop.matchLookReferences", &json!({"ids":[id.0]})).unwrap();
        s.execute("develop.analyzeLook", &json!({})).unwrap();
        s
    }
    #[test]
    fn match_look_already_close_stays_close() {
        let source = Arc::new(lightcraft_raster::Rgb32f::from_fn(512, 384, |x, y| [0.06 + x as f32 / 900.0, 0.08 + y as f32 / 900.0, 0.2]));
        let info = SourceInfo::default();
        let before = DevelopSettings::default();
        let mut desired = before.clone();
        desired.light.exposure = 0.03;
        let references = [match_look::analyze(&source, &info, &desired)];
        let mut overshoot = before.clone();
        overshoot.light.exposure = 0.6;
        overshoot.grading.midtones.sat = 15.0;
        let (a, b) = near_match_errors(&source, &info, &before, &overshoot, &references, &[1.0], Options::default(), 1.0);
        assert!(a < 0.05);
        assert!(b > a, "regression fixture must expose overshoot");
        let solved = match_look::solve(&source, &info, &before, &references, Options::default(), &|| false).unwrap();
        let (a, b) = near_match_errors(&source, &info, &before, &solved.settings, &references, &[1.0], Options::default(), solved.similarity);
        let chosen = if b >= a { before.clone() } else { solved.settings };
        let (_, final_error) = near_match_errors(&source, &info, &before, &chosen, &references, &[1.0], Options::default(), solved.similarity);
        assert!(final_error <= a + 1e-12);
    }
    #[test]
    fn match_look_revision3_retains_decode_and_restores_redo_record() {
        let mut s = setup();
        let ids = s.catalog.photos().filter(|p| !s.match_look.references.contains(&p.id)).take(8).map(|p| p.id).collect::<Vec<_>>();
        let count = Arc::new(AtomicUsize::new(0));
        let mut work = prepare(&mut s, &json!({"ids":ids,"colorTone":false})).unwrap();
        for input in &mut work.inputs {
            let count = count.clone();
            input.source = SourceRef::File {
                path: "decode-count".into(),
                max_edge: 64,
                fallback: None,
                loader: Some(Arc::new(move |_, _| {
                    assert!(std::thread::current().name().is_some_and(|name| name.starts_with("li-background-")));
                    #[cfg(target_os = "linux")]
                    {
                        let stat = std::fs::read_to_string("/proc/thread-self/stat").unwrap();
                        let fields = stat.rsplit_once(')').unwrap().1.split_whitespace().collect::<Vec<_>>();
                        assert!(fields[16].parse::<i64>().unwrap() >= 10);
                    }
                    count.fetch_add(1, Ordering::Relaxed);
                    Ok((
                        lightcraft_raster::Rgb32f::from_fn(64, 48, |x, y| [0.05 + x as f32 / 200.0, 0.04 + y as f32 / 200.0, 0.1]),
                        SourceInfo::default(),
                    ))
                })),
            };
        }
        let result = work.run(&Control::default(), false);
        assert_eq!(result.decoded.len(), 8);
        apply(&mut s, result).unwrap();
        assert_eq!(count.load(Ordering::Relaxed), 8);
        let second = prepare(&mut s, &json!({"ids":ids,"colorTone":false})).unwrap();
        assert!(second.inputs.iter().all(|i| matches!(i.source, SourceRef::Loaded(_))));
        let result = second.run(&Control::default(), false);
        assert!(result.decoded.is_empty());
        apply(&mut s, result).unwrap();
        assert_eq!(count.load(Ordering::Relaxed), 8);
        let id = ids[0];
        let record = s.match_look.applied[&id].clone();
        // An amount operation has its own undo. Undo the complete initial batch afterward.
        while !s.undo.is_empty() && s.develop_of(id).as_deref() != Some(record.before.as_ref()) {
            s.execute("edit.undo", &json!({})).unwrap();
        }
        assert!(!s.match_look.applied.contains_key(&id));
        while !s.redo.is_empty() && s.develop_of(id).as_deref() != Some(&record.last) {
            s.execute("edit.redo", &json!({})).unwrap();
        }
        assert_eq!(s.match_look.applied[&id].last, record.last);
    }
    #[test]
    fn match_look_revision3_reference_pin_survives_prefs_and_saved_look() {
        let mut s = setup();
        let id = s.match_look.references[0];
        s.execute("develop.matchLookReferences", &json!({"weightId":id})).unwrap();
        assert_eq!(s.match_look.reference_weights[&id], 2.0);
        assert_eq!(selected_looks(&s).unwrap()[0].importance, 2.0);
        let state: State = serde_json::from_value(serde_json::to_value(&s.match_look).unwrap()).unwrap();
        assert_eq!(state.reference_weights[&id], 2.0);
        s.execute("develop.savedLook", &json!({"name":"Pinned"})).unwrap();
        assert_eq!(s.saved_looks.file.looks[0].looks[0].importance, 2.0);
    }
    #[test]
    fn match_look_reference_cap_stale_state_and_self_undo() {
        let mut s = setup();
        let active = s.active().unwrap();
        let undo = s.undo.len();
        s.execute("develop.matchLook", &json!({"ids":[active.0]})).unwrap();
        assert_eq!(s.undo.len(), undo);
        let ids = s.catalog.photos().take(11).map(|p| p.id.0).collect::<Vec<_>>();
        assert_eq!(ids.len(), 11);
        assert!(s.execute("develop.matchLookReferences", &json!({"ids":ids,"replace":true})).is_err());
        assert_eq!(s.match_look.references, [active]);
        s.execute("develop.set", &json!({"control":"color.saturation","value":12.0})).unwrap();
        assert!(!current_ready(&s));
        assert!(s.execute("develop.matchLook", &json!({})).is_err());
        s.execute("develop.analyzeLook", &json!({})).unwrap();
        assert!(current_ready(&s));
        s.execute("develop.matchLookReferences", &json!({"ids":ids[..10],"replace":true})).unwrap();
        assert!(!current_ready(&s));
        s.execute("develop.analyzeLook", &json!({})).unwrap();
        assert_eq!(s.match_look.analysis.as_ref().unwrap().looks.len(), 10);
    }
    #[test]
    fn match_look_batch_is_one_undo_with_editable_layer_controls() {
        let mut s = setup();
        let reference = s.active().unwrap();
        let ids = s.catalog.photos().filter(|p| p.id != reference).take(3).map(|p| p.id).collect::<Vec<_>>();
        let before = ids.iter().map(|id| s.develop_of(*id).unwrap()).collect::<Vec<_>>();
        let undo = s.undo.len();
        let value = s.execute("develop.matchLook", &json!({"ids":ids,"newLayer":true,"colorTone":false})).unwrap();
        assert_eq!(value["changed"], 3);
        assert_eq!(s.undo.len(), undo + 1);
        for (id, b) in ids.iter().zip(&before) {
            let a = s.develop_of(*id).unwrap();
            assert_eq!(a.masks.len(), b.masks.len() + 1);
            assert_eq!(a.light, b.light);
        }
        s.execute("edit.undo", &json!({})).unwrap();
        for (id, b) in ids.iter().zip(before) {
            assert_eq!(s.develop_of(*id).unwrap(), b);
        }
    }
    #[test]
    fn match_look_cancel_and_stale_worker_cannot_commit() {
        let mut s = setup();
        let result = prepare_analysis(&mut s).unwrap().run(&Control::default()).unwrap();
        s.execute("develop.set", &json!({"control":"light.contrast","value":28.0})).unwrap();
        assert_eq!(apply_analysis(&mut s, result, None).unwrap()["discarded"], true);
        s.execute("develop.analyzeLook", &json!({})).unwrap();
        let ids = s.catalog.photos().take(3).map(|p| p.id.0).collect::<Vec<_>>();
        let control = Control { cancel: AtomicBool::new(true), ..Default::default() };
        let revision = s.catalog.revision;
        let result = prepare(&mut s, &json!({"ids":ids})).unwrap().run(&control, false);
        assert!(result.rows.is_empty());
        assert_eq!(s.catalog.revision, revision);
        let result = prepare(&mut s, &json!({"ids":ids})).unwrap().run(&Control::default(), false);
        s.execute("develop.set", &json!({"ids":[ids[0]],"control":"light.contrast","value":-12.0})).unwrap();
        let revision = s.catalog.revision;
        let value = apply(&mut s, result).unwrap();
        assert_eq!(value["skipped"], json!([ids[0]]));
        assert!(value["changed"].as_u64().unwrap() > 0);
        assert!(s.catalog.revision > revision);
        assert_eq!(s.develop_of(PhotoId(ids[0])).unwrap().light.contrast, -12.0);
        let result = prepare_analysis(&mut s).unwrap().run(&Control::default()).unwrap();
        s.library_generation += 1;
        assert_eq!(apply_analysis(&mut s, result, None).unwrap()["discarded"], true);
    }
    #[test]
    fn match_look_saved_reload_same_sliders_survives_removed_references() {
        let store = MemStore::new();
        let mut s = setup();
        s.saved_looks.open(Box::new(store.clone())).unwrap();
        let target = s.catalog.photos().find(|p| p.id != s.active().unwrap()).unwrap().id;
        let p = json!({"ids":[target.0],"colorTone":false});
        let live = prepare(&mut s, &p).unwrap().run(&Control::default(), false).rows[0].1.clone();
        let id = s.execute("develop.savedLook", &json!({"name":"Exteriors"})).unwrap()["id"].as_str().unwrap().to_string();
        let reference = s.match_look.references[0];
        s.execute("photo.delete", &json!({"ids":[reference.0]})).unwrap();
        let mut reloaded = Session::with_demo();
        reloaded.saved_looks.open(Box::new(store)).unwrap();
        reloaded.execute("develop.savedLook", &json!({"action":"select","id":id})).unwrap();
        let saved = prepare(&mut reloaded, &p).unwrap().run(&Control::default(), false).rows[0].1.clone();
        assert_eq!(live, saved);
        assert!(reloaded.match_look.references.is_empty());
        let bytes = serde_json::to_string(&reloaded.saved_looks.file).unwrap();
        assert!(!bytes.contains("identities") && !bytes.contains("settings") && !bytes.contains("photoId"));
        assert_eq!(reloaded.saved_looks.file.looks[0].reference_count, 1);
        assert!(!reloaded.saved_looks.file.looks[0].created.is_empty());
    }
    #[test]
    fn match_look_saved_rename_delete_duplicate_update_and_import_export() {
        let mut s = setup();
        let id = s.execute("develop.savedLook", &json!({"name":"Interiors"})).unwrap()["id"].clone();
        s.execute("develop.savedLook", &json!({"action":"rename","id":id,"name":"Interior architecture"})).unwrap();
        s.execute("develop.savedLook", &json!({"action":"duplicate","id":id})).unwrap();
        assert_eq!(s.saved_looks.file.looks.len(), 2);
        let before = s.saved_looks.file.looks[0].looks.clone();
        s.execute("develop.set", &json!({"control":"light.contrast","value":-22.0})).unwrap();
        s.execute("develop.analyzeLook", &json!({"update":id})).unwrap();
        assert_ne!(s.saved_looks.file.looks[0].looks, before);
        let file = s.execute("develop.savedLook", &json!({"action":"export"})).unwrap();
        let mut other = Session::new();
        other.execute("develop.savedLook", &json!({"action":"import","file":file})).unwrap();
        assert_eq!(other.saved_looks.file.looks[0].looks, s.saved_looks.file.looks[0].looks);
        assert_ne!(other.saved_looks.file.looks[0].id, s.saved_looks.file.looks[0].id);
        s.execute("develop.savedLook", &json!({"action":"delete","id":id})).unwrap();
        assert_eq!(s.saved_looks.file.looks.len(), 1);
        assert_eq!(s.match_look.selected, None);
        assert!(s.execute("develop.savedLook", &json!({"action":"import","file":{"version":99,"looks":[]}})).is_err());
    }
    #[test]
    fn match_look_catalog_analysis_persists_and_old_prefs_load() {
        let catalog = MemStore::new();
        let files = MemStore::new();
        let open = |s: &mut Session| {
            s.open_library_in(
                crate::library::LibraryStores {
                    dir: "matchlook-test".into(),
                    catalog: Box::new(catalog.clone()),
                    files: Box::new(files.clone()),
                    on_disk: false,
                },
                true,
            )
            .unwrap();
        };
        let mut s = Session::new();
        open(&mut s);
        crate::demo::load(&mut s);
        s.execute("develop.matchLookReferences", &json!({})).unwrap();
        s.execute("develop.analyzeLook", &json!({})).unwrap();
        s.close_library().unwrap();
        let mut reloaded = Session::new();
        open(&mut reloaded);
        assert!(current_ready(&reloaded));
        let mut file_store = files.clone();
        let mut prefs: Value = serde_json::from_slice(&file_store.read("prefs.json").unwrap().unwrap()).unwrap();
        prefs.as_object_mut().unwrap().remove("matchLook");
        file_store.write_atomic("prefs.json", &serde_json::to_vec(&prefs).unwrap()).unwrap();
        reloaded.close_library().unwrap();
        // Closing saves preferences, so remove the new field after close to model an old catalog.
        file_store.write_atomic("prefs.json", &serde_json::to_vec(&prefs).unwrap()).unwrap();
        let mut old = Session::new();
        open(&mut old);
        assert!(old.match_look.references.is_empty());
        assert!(!current_ready(&old)); // isolated analysis cannot activate without the reference tray
    }
    #[test]
    fn match_look_no_onnx_model_is_loaded() {
        let mut s = setup();
        assert!(s.smart.tagger.is_none());
        assert!(s.smart.people.tagger.is_none());
        let target = s.catalog.photos().find(|p| p.id != s.active().unwrap()).unwrap().id;
        s.execute("develop.matchLook", &json!({"ids":[target.0]})).unwrap();
        assert!(s.smart.tagger.is_none());
        assert!(s.smart.people.tagger.is_none());
        assert_eq!(lightcraft_sysmem::heavy_jobs().in_flight(), (0, 0));
    }
    #[test]
    fn match_look_legacy_embedded_analysis_migrates_without_pref_loss() {
        let catalog = MemStore::new();
        let files = MemStore::new();
        let open = |s: &mut Session| {
            s.open_library_in(
                crate::library::LibraryStores {
                    dir: "look-legacy-prefs".into(),
                    catalog: Box::new(catalog.clone()),
                    files: Box::new(files.clone()),
                    on_disk: false,
                },
                true,
            )
            .unwrap();
        };
        let mut s = Session::new();
        open(&mut s);
        crate::demo::load(&mut s);
        s.execute("develop.matchLookReferences", &json!({})).unwrap();
        s.execute("develop.analyzeLook", &json!({})).unwrap();
        let analysis = s.match_look.analysis.clone().unwrap();
        s.close_library().unwrap();
        let mut store = files.clone();
        let mut prefs: Value = serde_json::from_slice(&store.read("prefs.json").unwrap().unwrap()).unwrap();
        prefs["matchLook"]["analysis"] = serde_json::to_value(&analysis).unwrap();
        store.write_atomic("prefs.json", &serde_json::to_vec(&prefs).unwrap()).unwrap();
        let migrated_files = MemStore::new();
        migrated_files.set("prefs.json", serde_json::to_vec(&prefs).unwrap());
        let reopen = |s: &mut Session| {
            s.open_library_in(
                crate::library::LibraryStores {
                    dir: "look-legacy-prefs".into(),
                    catalog: Box::new(catalog.clone()),
                    files: Box::new(migrated_files.clone()),
                    on_disk: false,
                },
                true,
            )
            .unwrap();
        };
        let mut old = Session::new();
        reopen(&mut old);
        assert!(current_ready(&old));
        assert!(migrated_files.get("match-look-analysis.json").is_some());
        old.save_prefs().unwrap();
        old.close_library().unwrap();
        let mut newer = Session::new();
        reopen(&mut newer);
        assert!(current_ready(&newer));
    }
    #[test]
    fn match_look_photo_amount_persists_on_catalog_close_and_reload() {
        let catalog = MemStore::new();
        let files = MemStore::new();
        let open = |s: &mut Session| {
            s.open_library_in(
                crate::library::LibraryStores {
                    dir: "look-amount-reload".into(),
                    catalog: Box::new(catalog.clone()),
                    files: Box::new(files.clone()),
                    on_disk: false,
                },
                true,
            )
            .unwrap();
        };
        let mut s = Session::new();
        open(&mut s);
        crate::demo::load(&mut s);
        s.execute("develop.matchLookReferences", &json!({})).unwrap();
        s.execute("develop.analyzeLook", &json!({})).unwrap();
        let target = s.catalog.photos().find(|p| p.id != s.active().unwrap()).unwrap().id;
        s.execute("library.select", &json!({"ids":[target.0]})).unwrap();
        s.execute("develop.matchLook", &json!({})).unwrap();
        let record = s.match_look.applied[&target].clone();
        let expected = match_look::amount_settings(&record.before, &record.full, Options { strength: 50.0, ..record.options });
        s.close_library().unwrap();
        let mut reloaded = Session::new();
        open(&mut reloaded);
        assert!(reloaded.match_look.applied.contains_key(&target));
        reloaded.execute("library.select", &json!({"ids":[target.0]})).unwrap();
        reloaded.execute("develop.matchLookAmount", &json!({"strength":50.0})).unwrap();
        let got = reloaded.develop_of(target).unwrap();
        for spec in lightcraft_develop::controls::CONTROLS {
            assert!(
                (lightcraft_develop::controls::get(&got, spec.id).unwrap() - lightcraft_develop::controls::get(&expected, spec.id).unwrap()).abs()
                    < 1e-9,
                "{}",
                spec.id
            );
        }
    }
    #[test]
    fn match_look_bad_analysis_and_options_cannot_discard_other_preferences() {
        let catalog = MemStore::new();
        let files = MemStore::new();
        let global = MemStore::new();
        let open = |s: &mut Session| {
            s.open_library_in(
                crate::library::LibraryStores {
                    dir: "look-isolation-test".into(),
                    catalog: Box::new(catalog.clone()),
                    files: Box::new(files.clone()),
                    on_disk: false,
                },
                true,
            )
            .unwrap();
        };
        let mut s = Session::new();
        open(&mut s);
        crate::demo::load(&mut s);
        s.last_export = Some(json!({"proof":"retained"}));
        s.execute("develop.matchLookReferences", &json!({})).unwrap();
        s.execute("develop.analyzeLook", &json!({})).unwrap();
        s.saved_looks.open(Box::new(global.clone())).unwrap();
        let selected = s.execute("develop.savedLook", &json!({"name":"Retained selected look"})).unwrap()["id"].as_str().unwrap().to_string();
        s.close_library().unwrap();
        let mut store = files.clone();
        let mut prefs: Value = serde_json::from_slice(&store.read("prefs.json").unwrap().unwrap()).unwrap();
        assert!(prefs["matchLook"].get("analysis").is_none());
        let references = s.match_look.references.clone();
        prefs["matchLook"]["options"]["strength"] = Value::Null;
        store.write_atomic("prefs.json", &serde_json::to_vec(&prefs).unwrap()).unwrap();
        store.write_atomic("match-look-analysis.json", b"{broken look").unwrap();
        let mut reopened = Session::new();
        reopened.saved_looks.open(Box::new(global)).unwrap();
        open(&mut reopened);
        assert_eq!(reopened.last_export, Some(json!({"proof":"retained"})));
        assert!(reopened.match_look.analysis.is_none());
        assert_eq!(reopened.match_look.options, Options::default());
        assert_eq!(reopened.match_look.references, references);
        assert_eq!(reopened.match_look.selected.as_deref(), Some(selected.as_str()));
    }
    #[test]
    fn match_look_import_bounds_unlimited_global_looks_and_dangling_selection() {
        let mut s = setup();
        s.execute("develop.savedLook", &json!({"name":"Exteriors"})).unwrap();
        let original = s.saved_looks.file.clone();
        let mut many = original.clone();
        for _ in 0..1050 {
            let mut look = original.looks[0].clone();
            look.id = uuid::Uuid::new_v4().to_string();
            many.looks.push(look);
        }
        s.saved_looks.replace(many).unwrap();
        assert_eq!(s.saved_looks.file.looks.len(), 1051);
        assert!(parse_import(&vec![b' '; IMPORT_MAX_BYTES + 1]).is_err());
        for invalid in ["name", "refs", "numeric", "clusters"] {
            let mut f = original.clone();
            let l = &mut f.looks[0];
            match invalid {
                "name" => l.name = "x".repeat(LOOK_NAME_MAX_CHARS + 1),
                "refs" => {
                    l.looks = vec![l.looks[0].clone(); 11];
                    l.reference_count = 11;
                }
                "numeric" => l.looks[0].features[2] = 1000.0,
                _ => l.looks[0].clusters = vec![Default::default(); 4],
            }
            assert!(parse_import(&serde_json::to_vec(&f).unwrap()).is_err(), "{invalid}");
        }
        s.match_look.selected = Some("deleted-in-another-catalog".into());
        assert!(reconcile_selection(&mut s));
        assert!(s.match_look.selected.is_none());
        assert!(selected_looks(&s).is_some());
        let mut other = Session::new();
        other.execute("develop.savedLook", &json!({"action":"import","file":serde_json::to_value(&original).unwrap()})).unwrap();
        assert_eq!(original.looks[0].looks, other.saved_looks.file.looks[0].looks);
    }
    #[test]
    fn match_look_per_photo_amount_normalize_reset_and_manual_edit_guard() {
        let mut s = setup();
        let target = s.catalog.photos().find(|p| p.id != s.active().unwrap()).unwrap().id;
        s.execute("library.select", &json!({"ids":[target.0]})).unwrap();
        s.execute("develop.matchLook", &json!({"strength":100.0})).unwrap();
        let applied = s.develop_of(target).unwrap();
        s.execute("develop.matchLookAmount", &json!({"strength":0.0})).unwrap();
        let zero = s.develop_of(target).unwrap();
        assert_eq!(zero.light.exposure, applied.light.exposure);
        assert_eq!(zero.wb, applied.wb);
        s.execute("develop.matchLookAmount", &json!({"strength":125.0})).unwrap();
        s.execute("develop.matchLookAmount", &json!({"resetGroup":"light"})).unwrap();
        assert_eq!(s.develop_of(target).unwrap().light.contrast, s.match_look.applied[&target].before.light.contrast);
        s.execute("develop.set", &json!({"control":"light.contrast","value":42.0})).unwrap();
        assert!(s.execute("develop.matchLookAmount", &json!({"strength":40.0})).is_err());
        assert_eq!(s.develop_of(target).unwrap().light.contrast, 42.0);
    }
    #[test]
    fn match_look_corrupt_global_file_is_preserved() {
        let mut store = MemStore::new();
        store.write_atomic(FILE, b"{broken").unwrap();
        let original = store.read(FILE).unwrap();
        let mut s = setup();
        assert!(s.saved_looks.open(Box::new(store.clone())).is_err());
        assert_eq!(store.read(FILE).unwrap(), original);
    }
    #[test]
    fn match_look_revision2_amount_drag_undo_and_import_dedupe() {
        let mut s = setup();
        let id = s.catalog.photos().find(|p| p.id != s.active().unwrap()).unwrap().id;
        s.execute("library.select", &json!({"ids":[id]})).unwrap();
        s.execute("develop.matchLook", &json!({"colorTone":false})).unwrap();
        let undo = s.undo.len();
        let history = s.catalog.photo(id).unwrap().history.len();
        for (index, value) in [90, 80, 70, 60, 50, 40].into_iter().enumerate() {
            let interaction = if index == 0 {
                "begin"
            } else if index == 5 {
                "end"
            } else {
                "update"
            };
            s.execute("develop.matchLookAmount", &json!({"strength":value,"interaction":interaction})).unwrap();
        }
        assert_eq!(s.undo.len(), undo + 1);
        assert_eq!(s.catalog.photo(id).unwrap().history.len(), history + 1);
        s.execute("develop.matchLookAmount", &json!({"strength":30})).unwrap();
        assert_eq!(s.undo.len(), undo + 2);
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(!s.match_look.applied.contains_key(&id));
        s.execute("develop.savedLook", &json!({"name":"Facade / evening"})).unwrap();
        assert_eq!(export_name(&s, None), "Facade _ evening.json");
        let file = s.saved_looks.file.clone();
        s.execute("develop.savedLook", &json!({"action":"import","file":file})).unwrap();
        assert_eq!(s.saved_looks.file.looks.len(), 1);
        let mut changed = file;
        changed.looks[0].looks[0].features[2] += 0.01;
        s.execute("develop.savedLook", &json!({"action":"import","file":changed})).unwrap();
        assert_eq!(s.saved_looks.file.looks[1].name, "Facade / evening (imported)");
    }
    #[test]
    fn match_look_revision2_mixed_selection_parallel_order_and_cancel() {
        use std::time::{Duration, Instant};
        let mut s = setup();
        let ids = s.catalog.photos().filter(|p| p.id != s.active().unwrap()).take(20).map(|p| p.id).collect::<Vec<_>>();
        let mut video = s.catalog.photo(ids[0]).unwrap().as_ref().clone();
        video.id = s.catalog.alloc_photo_id();
        let video_id = video.id;
        video.kind = lightcraft_catalog::MediaKind::Video;
        s.apply_silent(Op::AddPhoto { photo: Box::new(video) }).unwrap();
        let work = prepare(&mut s, &json!({"ids":[ids[1], ids[2], video_id],"colorTone":false})).unwrap();
        assert_eq!(work.failed.len(), 1);
        assert!(work.inputs.iter().all(|i| i.bytes <= 24 << 20));
        let result = work.run(&Control::default(), false);
        assert_eq!(apply(&mut s, result).unwrap()["changed"], 2);
        let mut work = prepare(&mut s, &json!({"ids":ids[1..17],"colorTone":false})).unwrap();
        let threads = Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
        let source = Arc::new(lightcraft_raster::Rgb32f::from_fn(32, 24, |x, y| [0.03 + x as f32 * 0.01, 0.04 + y as f32 * 0.01, 0.07]));
        for input in &mut work.inputs {
            let source = source.clone();
            let threads = threads.clone();
            input.source = SourceRef::File {
                path: "slow-fixture".into(),
                max_edge: 32,
                fallback: None,
                loader: Some(Arc::new(move |_, _| {
                    threads.lock().unwrap().insert(std::thread::current().id());
                    std::thread::sleep(Duration::from_millis(30));
                    Ok(((*source).clone(), SourceInfo::default()))
                })),
            };
        }
        let expected = work.inputs.iter().map(|i| i.identity.id).collect::<Vec<_>>();
        let serial = prepare(&mut s, &json!({"ids":expected,"colorTone":false})).unwrap();
        let serial = SolveWork { inputs: work.inputs.clone(), ..serial }.run_serial(&Control::default());
        let result = work.run(&Control::default(), false);
        assert_eq!(serial.rows, result.rows);
        assert_eq!(result.rows.iter().map(|r| r.0.id).collect::<Vec<_>>(), expected);
        assert_eq!(result.per_photo.len(), expected.len());
        if lightcraft_sysmem::profile().threads > 1 && lightcraft_sysmem::heavy_jobs().headroom() >= 48 << 20 {
            assert!(result.workers > 1);
        }
        if result.workers > 1 {
            assert!(threads.lock().unwrap().len() > 1);
            assert!(result.elapsed_ms < result.load_ms + result.solve_ms);
        }
        assert_eq!(apply(&mut s, result).unwrap()["changed"], expected.len());
        let mut work = prepare(&mut s, &json!({"ids":ids[1..],"colorTone":false})).unwrap();
        for input in &mut work.inputs {
            let source = source.clone();
            input.source = SourceRef::File {
                path: "cancel-fixture".into(),
                max_edge: 32,
                fallback: None,
                loader: Some(Arc::new(move |_, _| {
                    std::thread::sleep(Duration::from_millis(30));
                    Ok(((*source).clone(), SourceInfo::default()))
                })),
            };
        }
        let control = Control::default();
        let start = Instant::now();
        let result = std::thread::scope(|scope| {
            let job = scope.spawn(|| work.run(&control, false));
            while control.completed.load(Ordering::Relaxed) < 3 {
                assert!(start.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(2));
            }
            control.cancel.store(true, Ordering::Relaxed);
            job.join().unwrap()
        });
        assert!(result.rows.len() < expected.len());
        assert!(start.elapsed() < Duration::from_millis(expected.len() as u64 * 30));
    }
    #[derive(Clone)]
    struct AmountWriteFailure(MemStore);
    impl Store for AmountWriteFailure {
        fn read(&mut self, n: &str) -> std::io::Result<Option<Vec<u8>>> {
            self.0.read(n)
        }
        fn write_atomic(&mut self, n: &str, b: &[u8]) -> std::io::Result<()> {
            if n == "match-look-applied.json" { Err(std::io::Error::other("amount disk full")) } else { self.0.write_atomic(n, b) }
        }
        fn append(&mut self, n: &str, b: &[u8]) -> std::io::Result<()> {
            self.0.append(n, b)
        }
        fn truncate(&mut self, n: &str, l: u64) -> std::io::Result<()> {
            self.0.truncate(n, l)
        }
        fn describe(&self) -> String {
            "amount-write-failure".into()
        }
    }
    #[test]
    fn match_look_revision2_optional_amount_write_preserves_catalog() {
        let catalog = MemStore::new();
        let files = AmountWriteFailure(MemStore::new());
        let open = |s: &mut Session, seed| {
            s.open_library_in(
                crate::library::LibraryStores {
                    dir: "amount-failure".into(),
                    catalog: Box::new(catalog.clone()),
                    files: Box::new(files.clone()),
                    on_disk: false,
                },
                seed,
            )
            .unwrap();
        };
        let mut s = Session::new();
        open(&mut s, true);
        let id = s.active().unwrap();
        s.execute("develop.matchLookReferences", &json!({})).unwrap();
        s.execute("develop.analyzeLook", &json!({})).unwrap();
        s.execute("develop.matchLook", &json!({"ids":[s.catalog.photos().find(|p| p.id!=id).unwrap().id]})).unwrap();
        s.set_develop(
            id,
            DevelopSettings { light: lightcraft_develop::Light { exposure: 1.25, ..Default::default() }, ..Default::default() },
            "Last edit",
        )
        .unwrap();
        s.close_library().unwrap();
        assert!(s.library.as_ref().unwrap().settings_warnings.iter().any(|w| w.contains("amount disk full")));
        let mut reopened = Session::new();
        open(&mut reopened, false);
        assert_eq!(reopened.develop_of(id).unwrap().light.exposure, 1.25);
    }
}

#[cfg(test)]
mod gpu_acceptance {
    #[test]
    #[ignore = "physical Vulkan GPU acceptance; no unavailable/software adapter skips"]
    fn match_look_gpu_solved_editable_controls_match_cpu_render() {
        use lightcraft_develop::DevelopSettings;
        use lightcraft_pipeline::{RenderRequest, SourceInfo, match_look};
        use std::sync::Arc;
        let _gpu = crate::tests_gpu::gpu_state();
        assert!(crate::gpu::available(), "physical GPU required");
        let adapter = crate::gpu::adapter_name().unwrap();
        assert!(adapter.contains("RTX 5090") && adapter.contains("Vulkan"), "{adapter}");
        println!("MATCH_LOOK_GPU adapter={adapter}");
        let src = Arc::new(lightcraft_raster::Rgb32f::from_fn(128, 96, |x, y| {
            let v = 0.03 + x as f32 / 256.0;
            [v, v * (0.8 + y as f32 / 480.0), v * 0.75]
        }));
        let info = SourceInfo::default();
        let mut edited = DevelopSettings::default();
        edited.wb.mode = lightcraft_develop::WbMode::Custom;
        edited.wb.temp = 8500.0;
        edited.wb.tint = 12.0;
        edited.light.exposure = 0.65;
        edited.light.contrast = 22.0;
        edited.color.saturation = -17.0;
        edited.grading.shadows.hue = 205.0;
        edited.grading.shadows.sat = 12.0;
        let look = match_look::analyze(&src, &info, &edited);
        for new_layer in [false, true] {
            let solved = match_look::solve(
                &src,
                &info,
                &DevelopSettings::default(),
                std::slice::from_ref(&look),
                match_look::Options { new_layer, ..Default::default() },
                &|| false,
            )
            .unwrap();
            let settings: DevelopSettings = serde_json::from_value(solved.settings.to_json()).unwrap();
            assert_eq!(settings, solved.settings);
            let req = RenderRequest::fit(128, 128);
            let cpu = lightcraft_pipeline::render(&src, &info, &settings, &req).image;
            let gpu = crate::gpu::render(&src, &info, &settings, &req, None).expect("GPU render must run").image;
            let errors = cpu.data.iter().zip(&gpu.data).flat_map(|(a, b)| (0..3).map(move |i| a[i].abs_diff(b[i]) as f64)).collect::<Vec<_>>();
            let mean = errors.iter().sum::<f64>() / errors.len() as f64;
            let max = errors.iter().copied().fold(0.0, f64::max);
            println!("MATCH_LOOK_GPU new_layer={new_layer} mean_lsb={mean} max_lsb={max}");
            assert!(mean < 0.5 && max <= 3.0, "layer={new_layer}: mean={mean}, max={max}");
            if new_layer {
                assert!(settings.masks.last().unwrap().tools.wb.is_none());
                let mut later = settings.clone();
                later.wb.mode = lightcraft_develop::WbMode::Custom;
                later.wb.temp = 4500.0;
                let c = lightcraft_pipeline::render(&src, &info, &later, &req).image;
                assert_ne!(c, cpu, "Global WB must still affect a Match Look layer");
                let g = crate::gpu::render(&src, &info, &later, &req, None).expect("hardware GPU later-WB render").image;
                let errors = c.data.iter().zip(&g.data).flat_map(|(a, b)| (0..3).map(move |i| a[i].abs_diff(b[i]) as f64)).collect::<Vec<_>>();
                let mean = errors.iter().sum::<f64>() / errors.len() as f64;
                let max = errors.iter().copied().fold(0.0, f64::max);
                println!("MATCH_LOOK_GPU later_global_wb mean_lsb={mean} max_lsb={max}");
                assert!(mean < 0.5 && max <= 3.0);
            }
        }
    }

    #[test]
    #[ignore = "physical Vulkan GPU: monochrome treatment, per-photo amount and layer treatment"]
    fn match_look_gpu_revision2_monochrome_amount_and_layer() {
        use lightcraft_develop::{DevelopSettings, Treatment};
        use lightcraft_pipeline::{
            RenderRequest, SourceInfo,
            match_look::{self, Options},
        };
        use std::sync::Arc;
        let _gpu = crate::tests_gpu::gpu_state();
        assert!(crate::gpu::available());
        let adapter = crate::gpu::adapter_name().unwrap();
        assert!(adapter.contains("RTX 5090") && adapter.contains("Vulkan"), "{adapter}");
        let source = Arc::new(lightcraft_raster::Rgb32f::from_fn(128, 96, |x, y| [0.03 + x as f32 / 260.0, 0.02 + y as f32 / 230.0, 0.08]));
        let info = SourceInfo::default();
        let base = DevelopSettings::default();
        let reference = DevelopSettings { treatment: Treatment::Bw, ..Default::default() };
        let look = match_look::analyze(&source, &info, &reference);
        for new_layer in [false, true] {
            let opts = Options { new_layer, strength: 50.0, ..Default::default() };
            let solved = match_look::solve(&source, &info, &base, std::slice::from_ref(&look), opts, &|| false).unwrap();
            assert!(solved.monochrome);
            let settings = match_look::amount_settings(&base, &solved.full_settings, opts);
            let req = RenderRequest::fit(128, 128);
            let cpu = lightcraft_pipeline::render(&source, &info, &settings, &req).image;
            let gpu = crate::gpu::render(&source, &info, &settings, &req, None).expect("native GPU monochrome render").image;
            let errors = cpu.data.iter().zip(&gpu.data).flat_map(|(a, b)| (0..3).map(move |i| f64::from(a[i].abs_diff(b[i])))).collect::<Vec<_>>();
            let mean = errors.iter().sum::<f64>() / errors.len() as f64;
            let max = errors.iter().copied().fold(0.0, f64::max);
            println!("MATCH_LOOK_GPU_MONO adapter={adapter} layer={new_layer} mean_lsb={mean:.4} max_lsb={max}");
            assert!(mean < 0.5 && max <= 3.0, "{mean} {max}");
        }
    }
}
