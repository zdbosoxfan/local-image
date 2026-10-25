//! The fixes Set Up Models performs: the person's choices for a model (stored with it, applied by
//! the next scan), moving a file ComfyUI lists from the wrong folder (with undo), and the verified
//! download behind a `Download` fix. The UI runs them on worker threads and re-checks afterwards.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::issue::Fix;
use super::state::{SetupState, Status};
use crate::browser::{self, CatalogFile, Item};
use crate::comfy::ObjectInfo;

/// A choice the person made for one model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Hide the model from setup (`false`: show it again).
    Ignore(bool),
    /// Run it through this official template (`None`: let matching decide).
    Template(Option<String>),
    /// Load `installed` where its template declares `declared`.
    UseInstead { declared: String, installed: String },
    /// Run a precision this GPU is not listed for; verification decides.
    TryAnyway(bool),
}

/// Stores `choice` for the model `key` of `host`.
pub fn choose(state_path: &Path, host: &str, key: &str, choice: Choice) -> Result<()> {
    SetupState::update(state_path, |s| {
        let m = s.servers.entry(host.to_owned()).or_default().models.entry(key.to_owned()).or_default();
        match choice {
            Choice::Ignore(ignored) => {
                m.user.ignored = ignored;
                if ignored {
                    m.status = Status::Ignored;
                } else if m.status == Status::Ignored {
                    m.status = Status::New;
                }
            }
            Choice::Template(name) => m.user.template_choice = name,
            Choice::UseInstead { declared, installed } => {
                m.user.substitutions.insert(declared, installed);
            }
            Choice::TryAnyway(on) => m.user.try_anyway = on,
        }
    })?;
    Ok(())
}

/// The model folders a misplaced file is looked for in.
const FOLDERS: &[&str] =
    &["diffusion_models", "unet", "checkpoints", "text_encoders", "clip", "vae", "loras", "model_patches", "clip_vision", "upscale_models"];

/// A file moved between model folders (kept to undo it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// `file` as ComfyUI lists it (`name` or `sub/name`): plain relative components only.
fn relative(file: &str) -> Result<PathBuf> {
    let parts: Vec<&str> = file.split(['/', '\\']).collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains(':')) {
        bail!("{file} is not a plain file name inside the model folder");
    }
    Ok(parts.iter().collect())
}

/// Renames `from` to `to`; a move across disks is left to the person.
fn rename(from: &Path, to: &Path) -> Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            bail!("Move it yourself: {} -> {}", from.display(), to.display())
        }
        Err(e) => Err(e).with_context(|| format!("Could not move {} to {}", from.display(), to.display())),
    }
}

/// Moves `file` from the one model folder it is in to `to_folder`, inside `model_dir` only and on
/// the same disk. Refuses when the file is in no folder or in several, when it is a link, when the
/// target exists, or when a path would leave the model folder.
pub fn move_file(model_dir: &Path, file: &str, to_folder: &str) -> Result<Moved> {
    if !FOLDERS.contains(&to_folder) {
        bail!("{to_folder} is not a ComfyUI model folder");
    }
    let rel = relative(file)?;
    let root = model_dir.canonicalize().context("The model folder is unavailable")?;
    let found: Vec<PathBuf> = FOLDERS.iter().filter(|f| **f != to_folder).map(|f| root.join(f).join(&rel)).filter(|p| p.is_file()).collect();
    let from = match found.as_slice() {
        [one] => one.clone(),
        [] => bail!("{file} is not in the model folder {}", root.display()),
        _ => bail!("{file} is in more than one model folder; move it yourself"),
    };
    if std::fs::symlink_metadata(&from)?.file_type().is_symlink() || !from.canonicalize()?.starts_with(&root) {
        bail!("{} is a link or leaves the model folder; move it yourself", from.display());
    }
    let to = root.join(to_folder).join(&rel);
    if std::fs::symlink_metadata(&to).is_ok() {
        bail!("{} already exists", to.display());
    }
    let parent = to.parent().context("no target folder")?;
    let existing = parent.ancestors().find(|p| p.exists()).context("no existing target ancestor")?;
    if !existing.canonicalize()?.starts_with(&root) {
        bail!("{} leaves the model folder", parent.display());
    }
    std::fs::create_dir_all(parent)?;
    if !parent.canonicalize()?.starts_with(&root) {
        bail!("{} leaves the model folder", parent.display());
    }
    rename(&from, &to)?;
    Ok(Moved { from, to })
}

/// Moves the file back (refused when something now occupies its old place).
pub fn undo_move(m: &Moved) -> Result<()> {
    if std::fs::symlink_metadata(&m.from).is_ok() {
        bail!("{} exists again; not moving {} back", m.from.display(), m.to.display());
    }
    rename(&m.to, &m.from)
}

/// The verified download behind a `Download` fix: the runtime data's pinned public copy when there
/// is one, else the template's URL with the size and SHA-256 its publisher lists (Hugging Face), as a
/// Model Browser install. Files ComfyUI already has are reported as present.
pub fn download_plan(fix: &Fix, info: &ObjectInfo, cfg: &browser::Config, net: &dyn browser::Net) -> Result<browser::Plan> {
    let Fix::Download { files, folder, bytes, url } = fix else { bail!("not a download") };
    let name = files.first().context("nothing to download")?;
    let mirror = crate::companions::runtime().mirror(name);
    let file = CatalogFile {
        name: name.clone(),
        folder: folder.clone(),
        url: mirror.map_or_else(|| url.clone(), |m| m.url.clone()),
        bytes: mirror.map(|m| m.bytes).or((*bytes > 0).then_some(*bytes)),
        approx_bytes: None,
        sha256: mirror.map(|m| m.sha256.clone()),
    };
    let item = Item { id: format!("setup:{name}"), title: name.clone(), files: vec![file], ..Default::default() };
    browser::plan(&item, &item.files, None, info, cfg, net)
}

/// A Model Browser template card, checked and planned.
#[derive(Clone, Debug)]
pub struct TemplatePlan {
    /// The template (UI format).
    pub ui: serde_json::Value,
    /// What installing downloads; `None` when a blocker means downloading would not make it run.
    pub plan: Option<browser::Plan>,
    /// What [`super::check::check_template`] found.
    pub issues: Vec<super::issue::Issue>,
    /// The template's prompt enhancer: planned only when `extras` names it.
    pub optional: Vec<CatalogFile>,
}

/// Fetches a template card's workflow and checks it against the connected ComfyUI before any
/// file is planned: a template this ComfyUI can't run plans (and downloads) nothing. The
/// template's prompt enhancer is planned only when `extras` names it.
pub fn plan_template(item: &Item, cfg: &browser::Config, net: &dyn browser::Net, server: &super::Snapshot, extras: &[String]) -> Result<TemplatePlan> {
    let name = item.template.as_deref().context("not a template card")?;
    let (ui, mut files) = browser::fetch_template(cfg, net, name)?;
    let optional: Vec<CatalogFile> = files.iter().filter(|f| browser::hf::is_prompt_enhancer(&f.name)).cloned().collect();
    files.retain(|f| !browser::hf::is_prompt_enhancer(&f.name) || extras.contains(&f.name));
    let entry = crate::template::index::TemplateEntry {
        name: name.to_owned(),
        title: item.title.clone(),
        min_comfyui: item.min_comfyui.as_deref().and_then(crate::template::index::Version::parse),
        ..Default::default()
    };
    let issues = super::check::check_template(&entry, &ui, server, crate::companions::runtime());
    if issues.iter().any(|i| i.severity == super::issue::Severity::Blocker) {
        return Ok(TemplatePlan { ui, plan: None, issues, optional });
    }
    let fam = item.family.as_deref().and_then(|f| crate::family::registry().family(f));
    let plan = browser::plan(item, &files, fam, &server.info, cfg, net)?;
    Ok(TemplatePlan { ui, plan: Some(plan), issues, optional })
}

/// The main model file a template card installs (`unet:<name>` or `ckpt:<name>` and the name).
pub fn template_main(ui: &serde_json::Value, name: &str) -> Option<(String, String)> {
    let main = crate::template::index::declared_files(name, ui).into_iter().find(|d| d.is_main())?;
    let prefix = if main.folder == "checkpoints" { "ckpt" } else { "unet" };
    Some((format!("{prefix}:{}", main.name), main.name))
}

/// After a template card's files are installed: runs the template's main file through that
/// template (the person's choice, so matching never picks another) and scans, which publishes the
/// model. Returns the model's key.
pub fn adopt_template(opts: &super::Options, ui: &serde_json::Value, name: &str) -> Result<Option<String>> {
    let Some((key, _)) = template_main(ui, name) else { return Ok(None) };
    choose(&opts.state_path, &opts.host, &key, Choice::Template(Some(name.to_owned())))?;
    super::scan(opts);
    Ok(Some(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn dir() -> Dir {
        let d = std::env::temp_dir().join(format!("li-setup-fixes-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    #[test]
    fn choices_are_stored_with_the_model() {
        let d = dir();
        let p = d.0.join("ai-models.json");
        let host = "127.0.0.1:8189";
        choose(&p, host, "unet:a", Choice::Template(Some("image_flux3_klein_t2i".into()))).unwrap();
        choose(&p, host, "unet:a", Choice::UseInstead { declared: "x_int8.safetensors".into(), installed: "x_bf16.safetensors".into() }).unwrap();
        choose(&p, host, "unet:a", Choice::TryAnyway(true)).unwrap();
        choose(&p, host, "unet:a", Choice::Ignore(true)).unwrap();
        let m = SetupState::load(&p).servers[host].models["unet:a"].clone();
        assert_eq!(m.user.template_choice.as_deref(), Some("image_flux3_klein_t2i"));
        assert_eq!(m.user.substitutions["x_int8.safetensors"], "x_bf16.safetensors");
        assert!(m.user.try_anyway && m.user.ignored);
        assert_eq!(m.status, Status::Ignored);
        choose(&p, host, "unet:a", Choice::Ignore(false)).unwrap();
        let m = SetupState::load(&p).servers[host].models["unet:a"].clone();
        assert_eq!((m.user.ignored, m.status), (false, Status::New));
    }

    #[test]
    fn a_misplaced_file_moves_to_its_folder_and_back() {
        let d = dir();
        std::fs::create_dir_all(d.0.join("diffusion_models")).unwrap();
        std::fs::write(d.0.join("diffusion_models/flux2-vae.safetensors"), b"vae").unwrap();
        let moved = move_file(&d.0, "flux2-vae.safetensors", "vae").unwrap();
        assert!(d.0.join("vae/flux2-vae.safetensors").is_file() && !d.0.join("diffusion_models/flux2-vae.safetensors").exists());
        undo_move(&moved).unwrap();
        assert!(d.0.join("diffusion_models/flux2-vae.safetensors").is_file() && !d.0.join("vae/flux2-vae.safetensors").exists());
        // Never over an existing file, never outside the model folder, never an unknown folder.
        std::fs::create_dir_all(d.0.join("vae")).unwrap();
        std::fs::write(d.0.join("vae/flux2-vae.safetensors"), b"other").unwrap();
        assert!(move_file(&d.0, "flux2-vae.safetensors", "vae").is_err());
        assert!(move_file(&d.0, "../flux2-vae.safetensors", "text_encoders").is_err());
        assert!(move_file(&d.0, "flux2-vae.safetensors", "../outside").is_err());
        assert!(move_file(&d.0, "missing.safetensors", "vae").is_err());
        assert_eq!(std::fs::read(d.0.join("vae/flux2-vae.safetensors")).unwrap(), b"other");
    }

    #[test]
    fn a_download_uses_the_pinned_public_copy_when_there_is_one() {
        struct NoNet;
        impl browser::Net for NoNet {
            fn get(&self, url: &str) -> Result<Vec<u8>> {
                bail!("offline: {url}")
            }
            fn head_size(&self, _: &str) -> Result<Option<u64>> {
                Ok(None)
            }
        }
        let fix = Fix::Download {
            files: vec!["flux2-vae.safetensors".into()],
            folder: "vae".into(),
            bytes: 0,
            url: "https://huggingface.co/black-forest-labs/gated/resolve/main/flux2-vae.safetensors".into(),
        };
        let cfg = browser::Config { hf: "http://hf.invalid".into(), ..Default::default() };
        let plan = download_plan(&fix, &ObjectInfo::default(), &cfg, &NoNet).unwrap();
        let m = crate::companions::runtime().mirror("flux2-vae.safetensors").unwrap();
        assert_eq!(plan.files.len(), 1);
        assert_eq!((plan.files[0].url.as_str(), plan.files[0].bytes, plan.files[0].sha256.as_str()), (m.url.as_str(), m.bytes, m.sha256.as_str()));
        assert_eq!(plan.files[0].folder, "vae");
    }
    #[cfg(unix)]
    #[test]
    fn a_destination_symlink_cannot_create_directories_outside_the_model_folder() {
        let models = dir();
        let outside = dir();
        std::fs::create_dir_all(models.0.join("diffusion_models/nested")).unwrap();
        let source = models.0.join("diffusion_models/nested/model.safetensors");
        std::fs::write(&source, b"model").unwrap();
        std::os::unix::fs::symlink(&outside.0, models.0.join("vae")).unwrap();
        assert!(move_file(&models.0, "nested/model.safetensors", "vae").is_err());
        assert!(!outside.0.join("nested").exists(), "a refused move must leave the outside directory untouched");
        assert_eq!(std::fs::read(source).unwrap(), b"model");
    }
}
