//! Match Look preparation/analysis/solve transactions. Workers never mutate the session;
//! generation + source + settings snapshots reject stale results. Saved looks contain no IDs.
use super::{CommandSpec, always, bad, cmd, has_selection};
use crate::{
    Result, Session,
    media::{SourceLevel, SourceRef},
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
const VERSION: u32 = 1;
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
    pub weights: Vec<f64>,
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
            let file = loaded.unwrap_or_default();
            validate_file(&file)?;
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
    if f.version != VERSION {
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
        a.version == VERSION
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
    current_ready(s).then(|| s.match_look.analysis.as_ref().map(|a| a.looks.clone()).unwrap_or_default())
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
struct Input {
    identity: Identity,
    source: SourceRef,
    info: SourceInfo,
    bytes: u64,
    faces: Vec<[f32; 4]>,
}
fn input(s: &mut Session, id: PhotoId) -> Result<Input> {
    let photo = s.catalog.photo(id).cloned().ok_or_else(|| bad(C, "Unknown photo"))?;
    if photo.kind == lightcraft_catalog::MediaKind::Video || !photo.in_library() {
        return Err(bad(C, "Choose still photos in the catalog"));
    }
    Ok(Input {
        bytes: u64::from(photo.width).saturating_mul(u64::from(photo.height)).saturating_mul(32).saturating_add(16 << 20),
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
    })
}
pub struct AnalyzeWork {
    generation: u64,
    inputs: Vec<Input>,
}
pub struct Analyzed {
    generation: u64,
    analysis: Analysis,
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
        let mut a = Analysis { version: VERSION, identities: Vec::new(), looks: Vec::new() };
        for input in self.inputs {
            let _permit = lightcraft_sysmem::heavy_jobs().admit(input.bytes, &stop).ok_or("Cancelled")?;
            let src = input.source.load_source()?;
            if stop() {
                return Err("Cancelled".into());
            }
            a.looks.push(match_look::analyze_with_faces(&src.image, &src.info_or(input.info), &input.identity.settings, &input.faces));
            a.identities.push(input.identity);
            c.completed.fetch_add(1, Ordering::Relaxed);
        }
        if stop() {
            return Err("Cancelled".into());
        }
        Ok(Analyzed { generation: self.generation, analysis: a })
    }
}
pub fn apply_analysis(s: &mut Session, result: Analyzed, update: Option<&str>) -> Result<Value> {
    if s.library_generation != result.generation
        || result.analysis.identities.iter().any(|i| !unchanged(s, i))
        || !result.analysis.identities.iter().map(|i| i.id).eq(s.match_look.references.iter().copied())
    {
        return Ok(json!({"discarded":true}));
    }
    if let Some(id) = update {
        let mut file = s.saved_looks.file.clone();
        let look = file.looks.iter_mut().find(|l| l.id == id).ok_or_else(|| bad(C, "Look was deleted"))?;
        look.looks = result.analysis.looks.clone();
        look.reference_count = look.looks.len();
        s.saved_looks.replace(file)?;
    }
    let n = result.analysis.looks.len();
    s.match_look.analysis = Some(result.analysis);
    if update.is_none() {
        s.match_look.selected = None;
    }
    s.save_match_look_analysis()?;
    s.save_prefs()?;
    Ok(json!({"analyzed":n}))
}
pub struct SolveWork {
    generation: u64,
    inputs: Vec<Input>,
    looks: Vec<Look>,
    opts: Options,
}
pub struct Matched {
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
}
pub fn prepare(s: &mut Session, p: &Value) -> Result<SolveWork> {
    ready(s).map_err(|e| bad(C, e))?;
    let opts: Options = serde_json::from_value(p.clone()).map_err(|e| bad(C, e.to_string()))?;
    if !opts.strength.is_finite() || !(0.0..=125.0).contains(&opts.strength) {
        return Err(bad(C, "Strength must be 0–125"));
    }
    let mut seen = std::collections::BTreeSet::new();
    let inputs = s.targets(p).into_iter().filter(|id| seen.insert(*id)).map(|id| input(s, id)).collect::<Result<Vec<_>>>()?;
    Ok(SolveWork { generation: s.library_generation, inputs, looks: selected_looks(s).ok_or_else(|| bad(C, "Analyze first"))?, opts })
}
impl SolveWork {
    pub fn len(&self) -> usize {
        self.inputs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }
    pub fn run(self, c: &Control, preview: bool) -> Matched {
        lightcraft_sysmem::set_current_thread_class(lightcraft_sysmem::ThreadClass::Utility);
        let start = web_time::Instant::now();
        let stop = || c.cancel.load(Ordering::Relaxed);
        let mut out = Matched {
            generation: self.generation,
            rows: Vec::new(),
            applications: Default::default(),
            failed: Vec::new(),
            preview: None,
            preview_before: None,
            elapsed_ms: 0.0,
            load_ms: 0.0,
            solve_ms: 0.0,
            renders: 0,
        };
        for i in self.inputs {
            if stop() {
                break;
            }
            let Some(_permit) = lightcraft_sysmem::heavy_jobs().admit(i.bytes, &stop) else { break };
            let load_start = web_time::Instant::now();
            match i.source.load_source() {
                Ok(src) => {
                    out.load_ms += load_start.elapsed().as_secs_f64() * 1000.0;
                    let solve_start = web_time::Instant::now();
                    let info = src.info_or(i.info);
                    if let Some(solved) =
                        match_look::solve_with_faces(&src.image, &info, &i.identity.settings, &self.looks, self.opts, &i.faces, &stop)
                    {
                        out.renders += solved.renders;
                        let similarity = solved.similarity;
                        out.applications.insert(
                            i.identity.id,
                            Applied {
                                key: i.identity.key.clone(),
                                before: i.identity.settings.clone(),
                                full: solved.full_settings,
                                last: solved.settings.clone(),
                                options: self.opts,
                                similarity,
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
                    out.solve_ms += solve_start.elapsed().as_secs_f64() * 1000.0;
                }
                Err(e) => out.failed.push(json!([i.identity.id.0, e])),
            }
            c.completed.fetch_add(1, Ordering::Relaxed);
        }
        out.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        out
    }
}
pub fn apply(s: &mut Session, result: Matched) -> Result<Value> {
    // Like Sync, finish the batch in one transaction; protect only targets changed since capture.
    if s.library_generation != result.generation {
        return Ok(json!({"changed":0,"skipped":[],"discarded":true}));
    }
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
        json!({"changed":n,"skipped":skipped,"failed":result.failed,"elapsedMs":result.elapsed_ms,"loadMs":result.load_ms,"solveMs":result.solve_ms,"renders":result.renders}),
    )
}
fn references(s: &mut Session, p: &Value) -> Result<Value> {
    let mut ids = s.match_look.references.clone();
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
        let looks = s.match_look.analysis.as_ref().map(|a| a.looks.clone()).unwrap_or_default();
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
fn amount(s: &mut Session, p: &Value) -> Result<Value> {
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
    let d = match_look::amount_settings(&record.before, &record.full, record.options);
    if d != record.last
        && let Some(op) = s.develop_op(id, d.clone(), "Match Look amount")
    {
        s.commit("Match Look amount", op)?;
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
            "{strength?:0..125, resetGroup?:normalize|light|color|tone}",
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
        s.close_library().unwrap();
        let mut store = files.clone();
        let mut prefs: Value = serde_json::from_slice(&store.read("prefs.json").unwrap().unwrap()).unwrap();
        assert!(prefs["matchLook"].get("analysis").is_none());
        prefs["matchLook"]["options"]["strength"] = Value::Null;
        store.write_atomic("prefs.json", &serde_json::to_vec(&prefs).unwrap()).unwrap();
        store.write_atomic("match-look-analysis.json", b"{broken look").unwrap();
        let mut reopened = Session::new();
        open(&mut reopened);
        assert_eq!(reopened.last_export, Some(json!({"proof":"retained"})));
        assert!(reopened.match_look.analysis.is_none());
        assert_eq!(reopened.match_look.options, Options::default());
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
}
