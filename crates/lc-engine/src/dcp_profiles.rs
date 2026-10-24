//! Read-only camera/lens profiles from the user's own Adobe install or chosen folder.
//! Original files stay whole on disk. No Adobe profiles are redistributed.
use lightcraft_raw::{RawImage, camera_matrices::normalized, dcp::Dcp};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, OnceLock, PoisonError, RwLock},
};

#[derive(Clone, Debug)]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub group: String,
    pub model: String,
    pub path: PathBuf,
}
#[derive(Default)]
struct Registry {
    profiles: BTreeMap<String, (ProfileInfo, Arc<Dcp>)>,
    lenses: BTreeMap<String, (ProfileInfo, Arc<lightcraft_raw::lcp::LcpProfile>)>,
    skipped: usize,
    key: u64,
}
fn registry() -> &'static RwLock<Registry> {
    static R: OnceLock<RwLock<Registry>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(read_roots(&standard_roots())))
}
pub fn cache_key() -> u64 {
    registry().read().unwrap_or_else(PoisonError::into_inner).key
}
pub fn configure(folder: &str) {
    let mut roots = standard_roots();
    if !folder.trim().is_empty() {
        roots.push(PathBuf::from(folder));
    }
    *registry().write().unwrap_or_else(PoisonError::into_inner) = read_roots(&roots);
}
/// Platform locations documented by Adobe/RawPedia. Linux can choose a copied installation folder.
pub fn standard_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/Library/Application Support/Adobe/CameraRaw"));
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Library/Application Support/Adobe/CameraRaw"));
        }
        roots.push(PathBuf::from("/Applications/Adobe DNG Converter.app/Contents/Resources/CameraRaw"));
        roots.push(PathBuf::from("/Applications/Adobe DNG Converter.app/Contents/Resources"));
    }
    if cfg!(target_os = "windows") {
        for name in ["PROGRAMDATA", "ALLUSERSPROFILE", "APPDATA"] {
            if let Some(dir) = std::env::var_os(name) {
                roots.push(PathBuf::from(dir).join("Adobe/CameraRaw"));
            }
        }
        for name in ["PROGRAMFILES", "PROGRAMFILES(X86)"] {
            if let Some(dir) = std::env::var_os(name) {
                roots.push(PathBuf::from(dir).join("Common Files/Adobe/Camera Raw"));
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}
fn hash_bytes(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100000001b3))
}
pub fn selection_key(id: &str) -> u64 {
    match id {
        "" => 0,
        "matrix" => 1,
        _ => hash_bytes(id.as_bytes()).max(2),
    }
}
fn matches(model: &str, make: &str, camera: &str) -> bool {
    let n = normalized(model);
    !n.is_empty() && (n == normalized(camera) || n == normalized(&format!("{make}{camera}")))
}
/// Every compatible input profile, preserving alternate Adobe Standard/Camera Matching looks.
pub fn available(make: &str, model: &str) -> Vec<ProfileInfo> {
    let reg = registry().read().unwrap_or_else(PoisonError::into_inner);
    let mut out: Vec<_> = reg.profiles.values().filter(|(i, _)| matches(&i.model, make, model)).map(|(i, _)| i.clone()).collect();
    if let Some(p) = Dcp::bundled(make, model) {
        out.push(ProfileInfo {
            id: "bundled".into(),
            name: if p.name.is_empty() { "Bundled Camera Profile".into() } else { p.name },
            group: "Camera Profiles".into(),
            model: p.model,
            path: PathBuf::new(),
        });
    }
    out.sort_by(|a, b| (&a.group, &a.name, &a.id).cmp(&(&b.group, &b.name, &b.id)));
    out
}
pub fn name(id: &str) -> String {
    match id {
        "" => "Automatic".into(),
        "matrix" => "Matrix Only".into(),
        "bundled" => "Bundled Camera Profile".into(),
        _ => registry()
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .profiles
            .get(id)
            .map(|(i, _)| i.name.clone())
            .unwrap_or_else(|| "Missing Camera Profile".into()),
    }
}
/// Bound traversal, do not follow symlinks (cycles or paths outside the chosen folder).
fn profile_paths(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut pending: Vec<_> = roots.iter().cloned().map(|p| (p, 0)).collect();
    let mut paths = Vec::new();
    let mut visited = 0;
    while let Some((dir, depth)) = pending.pop() {
        if depth > 12 || visited >= 65536 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().take(65536 - visited).collect();
        entries.sort_by_key(|e| e.path());
        for e in entries {
            visited += 1;
            if visited > 65536 {
                break;
            }
            let Ok(kind) = e.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push((e.path(), depth + 1));
            } else if kind.is_file() && e.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("dcp") || ext.eq_ignore_ascii_case("lcp")) {
                paths.push(e.path());
            }
        }
    }
    // The stack visits the explicitly configured root (last) first. Preserve
    // that priority through the file/byte limits: global sorting can otherwise
    // fill the budget with Adobe's system profiles before reaching that folder.
    let mut seen = std::collections::BTreeSet::new();
    paths.retain(|path| seen.insert(path.clone()));
    paths.truncate(4096);
    paths
}
fn read_roots(roots: &[PathBuf]) -> Registry {
    let mut reg = Registry::default();
    let mut loaded = 0u64;
    for path in profile_paths(roots) {
        if !path.metadata().is_ok_and(|m| m.len() <= 32 * 1024 * 1024) {
            reg.skipped += 1;
            continue;
        }
        let size = path.metadata().map(|m| m.len()).unwrap_or_default();
        if loaded + size > 256 * 1024 * 1024 {
            reg.skipped += 1;
            continue;
        }
        loaded += size;
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lcp")) {
            match lightcraft_raw::lcp::LcpProfile::parse(&bytes) {
                Ok(p) => {
                    let id = format!("lcp:{:016x}", hash_bytes(&bytes));
                    let name = if p.name.is_empty() { p.lens_name.clone() } else { p.name.clone() };
                    let info = ProfileInfo { id: id.clone(), name, group: "Lens Profiles".into(), model: p.lens_name.clone(), path };
                    reg.key = reg.key.wrapping_add(hash_bytes(&bytes));
                    reg.lenses.insert(id, (info, Arc::new(p)));
                }
                Err(_) => reg.skipped += 1,
            }
            continue;
        }
        let Ok(p) = Dcp::parse(&bytes) else {
            reg.skipped += 1;
            continue;
        };
        if p.model.is_empty() {
            continue;
        }
        let name = if p.name.is_empty() { path.file_stem().unwrap_or_default().to_string_lossy().into_owned() } else { p.name.clone() };
        let group = if name.starts_with("Adobe ") {
            "Adobe"
        } else if name.starts_with("Camera ") {
            "Camera Matching"
        } else {
            "Custom Camera Profiles"
        };
        // Content identity survives moving folders and sharing settings across machines.
        let id = format!("dcp:{:016x}", hash_bytes(&bytes));
        let info = ProfileInfo { id: id.clone(), name, group: group.into(), model: p.model.clone(), path };
        reg.key = reg.key.wrapping_add(hash_bytes(&bytes));
        reg.profiles.insert(id, (info, Arc::new(p)));
    }
    reg
}
pub fn apply(raw: &mut RawImage) {
    apply_selected(raw, 0);
}
pub fn apply_selected(raw: &mut RawImage, key: u64) {
    if key == 1 {
        raw.color.profile = Default::default();
        return;
    }
    let reg = registry().read().unwrap_or_else(PoisonError::into_inner);
    apply_from(raw, key, &reg);
}
fn apply_from(raw: &mut RawImage, key: u64, reg: &Registry) {
    let (make, model) = (raw.metadata.make.as_deref().unwrap_or_default(), raw.metadata.model.as_deref().unwrap_or_default());
    let picked = if key == 0 {
        // Automatic preserves the existing choice; an embedded DNG profile takes precedence.
        if !raw.color.profile.is_empty() {
            return;
        }
        reg.profiles.values().filter(|(i, _)| matches(&i.model, make, model)).min_by_key(|(i, _)| (&i.group, &i.name, &i.id)).map(|(_, p)| p.clone())
    } else {
        reg.profiles.iter().find(|(id, (i, _))| selection_key(id) == key && matches(&i.model, make, model)).map(|(_, (_, p))| p.clone())
    };
    // A picked profile that is gone renders with the bundled colour model, but the Camera look
    // must not take the bundled tone curve for the user's pick.
    let lost = key != 0 && key != selection_key("bundled") && picked.is_none();
    let profile = picked.or_else(|| Dcp::bundled(make, model).map(Arc::new));
    if let Some(d) = profile {
        raw.color.color_matrix = d.color.color_matrix;
        raw.color.illuminant = d.color.illuminant;
        raw.color.forward_matrix = d.color.forward_matrix;
        raw.color.illuminant_3 = d.color.illuminant_3;
        raw.color.illuminant_white_xy = d.color.illuminant_white_xy;
        raw.color.color_matrix_3 = d.color.color_matrix_3;
        raw.color.forward_matrix_3 = d.color.forward_matrix_3;
        // CameraCalibration belongs to the input file, not a profile.
        raw.color.baseline_exposure += d.color.baseline_exposure;
        raw.color.profile_calibration_signature = d.color.profile_calibration_signature.clone();
        raw.color.profile = d.color.profile.clone();
        if lost {
            raw.color.profile.tone_curve = None;
        }
    }
}

/// Counts for the import UI, including malformed/unsupported profiles.
pub fn counts() -> (usize, usize, usize) {
    let r = registry().read().unwrap_or_else(PoisonError::into_inner);
    (r.profiles.len(), r.lenses.len(), r.skipped)
}
fn lens_matches(profile: &lightcraft_raw::lcp::LcpProfile, camera: &str, lens: &str, raw: bool) -> bool {
    profile.is_raw == raw
        && !lens.is_empty()
        && normalized(&profile.lens_name) == normalized(lens)
        && (profile.camera_model.is_empty()
            || matches(&profile.camera_model, &profile.camera_make, camera)
            || normalized(&format!("{}{}", profile.camera_make, profile.camera_model)) == normalized(camera))
}
pub fn available_lenses(camera: &str, lens: &str, raw: bool) -> Vec<ProfileInfo> {
    registry()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .lenses
        .values()
        .filter(|(_, p)| lens_matches(p, camera, lens, raw))
        .map(|(i, _)| i.clone())
        .collect()
}
/// Resolve imported lens models using the shot's metadata. No LCP file is modified or copied.
pub fn lens_for_photo(p: &lightcraft_catalog::Photo, s: &lightcraft_develop::DevelopSettings) -> Option<lightcraft_pipeline::lcp::LcpCorrection> {
    if !s.optics.lens_profile || s.lens_profile == "none" || !s.section_enabled("optics") {
        return None;
    }
    let r = registry().read().unwrap_or_else(PoisonError::into_inner);
    let entry = if s.lens_profile.is_empty() {
        r.lenses.values().find(|(_, l)| lens_matches(l, &p.meta.camera, &p.meta.lens, p.develops_raw()))
    } else {
        r.lenses.get(&s.lens_profile).filter(|(_, l)| lens_matches(l, &p.meta.camera, &p.meta.lens, p.develops_raw()))
    }?;
    let c = entry.1.correction(f64::from(p.meta.focal_mm?), None, p.meta.aperture.map(f64::from))?;
    let model = |m: lightcraft_raw::lcp::LcpModel| lightcraft_pipeline::lcp::LcpModel {
        focal: m.focal,
        center: m.center,
        radial: m.radial,
        tangential: m.tangential,
        scale: m.scale,
    };
    Some(lightcraft_pipeline::lcp::LcpCorrection {
        distortion: c.distortion.map(model),
        chromatic: c.chromatic.map(|m| m.map(model)),
        vignette: c.vignette.map(model),
        exif_orientation: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colorprofiles_explicit_folder_survives_a_full_system_profile_scan() {
        let dir = std::env::temp_dir().join(format!("lc-profile-priority-{}", std::process::id()));
        let standard = dir.join("a-system");
        let chosen = dir.join("z-chosen");
        std::fs::create_dir_all(&standard).unwrap();
        std::fs::create_dir_all(&chosen).unwrap();
        for i in 0..4096 {
            std::fs::write(standard.join(format!("{i:04}.dcp")), b"invalid").unwrap();
        }
        std::fs::write(chosen.join("custom.dcp"), include_bytes!("../../lc-raw/data/dcp/NIKON_D7500.dcp")).unwrap();
        let registry = read_roots(&[standard, chosen.clone()]);
        assert_eq!(registry.profiles.len(), 1, "the chosen folder must not be displaced by the bounded system scan");
        assert!(registry.profiles.values().all(|(info, _)| info.path.starts_with(&chosen)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn colorprofiles_recursive_read_only_registry_preserves_alternates() {
        let dir = std::env::temp_dir().join(format!("lc-colorprofiles-{}", std::process::id()));
        let nested = dir.join("CameraProfiles/Camera/Nikon");
        std::fs::create_dir_all(&nested).unwrap();
        let bytes = include_bytes!("../../lc-raw/data/dcp/NIKON_D7500.dcp");
        let file = nested.join("camera.DCP");
        std::fs::write(&file, bytes).unwrap();
        let mut alternate = bytes.to_vec();
        let old_name = Dcp::parse(bytes).unwrap().name;
        let at = alternate.windows(old_name.len()).rposition(|v| v == old_name.as_bytes()).unwrap();
        alternate[at] = if alternate[at] == b'A' { b'B' } else { b'A' };
        assert_eq!(Dcp::parse(&alternate).unwrap().model, Dcp::parse(bytes).unwrap().model);
        assert_ne!(Dcp::parse(&alternate).unwrap().name, old_name);
        std::fs::write(nested.join("alternate.dcp"), &alternate).unwrap();
        std::fs::write(dir.join("broken.dcp"), b"IIRC").unwrap();
        let oversized = dir.join("oversized.dcp");
        std::fs::File::create(&oversized).unwrap().set_len(32 * 1024 * 1024 + 1).unwrap();
        let first = read_roots(std::slice::from_ref(&dir));
        assert_eq!(first.profiles.len(), 2);
        let names: std::collections::BTreeSet<_> = first.profiles.values().map(|(i, _)| &i.name).collect();
        assert_eq!(names.len(), 2);
        assert_eq!(first.profiles.values().next().unwrap().1.model, "NIKON D7500");
        assert_ne!(first.key, 0);
        assert_eq!(read_roots(std::slice::from_ref(&dir)).key, first.key);
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
        assert!(matches("NIKON D7500", "Nikon", "D7500"));
        assert!(!matches("NIKON D7500", "Nikon", "D500"));
        assert_eq!(selection_key(""), 0);
        assert_eq!(selection_key("matrix"), 1);
        assert_ne!(selection_key("a"), selection_key("b"));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn colorprofiles_selection_reaches_color_model_and_cache_keys() {
        let d = Dcp::parse(include_bytes!("../../lc-raw/data/dcp/NIKON_D7500.dcp")).unwrap();
        let mut reg = Registry::default();
        for (id, name, scale) in [("dcp:a", "Adobe Standard", 1.0), ("dcp:b", "Camera Portrait", 1.2)] {
            let mut profile = d.clone();
            profile.name = name.into();
            profile.color.forward_matrix = profile.color.forward_matrix.map(|m| m.map(|m| li_color::Mat3(m.0.map(|r| r.map(|v| v * scale)))));
            reg.profiles.insert(
                id.into(),
                (
                    ProfileInfo { id: id.into(), name: name.into(), model: d.model.clone(), group: "Test".into(), path: PathBuf::new() },
                    Arc::new(profile),
                ),
            );
        }
        let mut raw = lightcraft_raw::decode(&crate::tests_xmp::synthetic_dng_with(None, Default::default())).unwrap();
        raw.metadata.make = Some("Nikon".into());
        raw.metadata.model = Some("D7500".into());
        let mut a = raw.clone();
        let mut b = raw.clone();
        apply_from(&mut a, selection_key("dcp:a"), &reg);
        apply_from(&mut b, selection_key("dcp:b"), &reg);
        assert_ne!(a.color.forward_matrix, b.color.forward_matrix);
        let mut settings = lightcraft_develop::DevelopSettings { camera_profile: "dcp:a".into(), ..Default::default() };
        let ka = crate::files::RawOptions::of(&settings).key();
        settings.camera_profile = "dcp:b".into();
        assert_ne!(ka, crate::files::RawOptions::of(&settings).key());
        settings.set_section_enabled("raw", false);
        assert_eq!(crate::files::RawOptions::of(&settings).camera_profile, selection_key("dcp:b"));
        apply_selected(&mut b, 1);
        assert!(b.color.profile.is_empty());
        raw.metadata.model = Some("Other Camera".into());
        let old = raw.color.clone();
        apply_from(&mut raw, selection_key("dcp:a"), &reg);
        assert_eq!(old, raw.color);
    }
    #[test]
    fn a_missing_pick_renders_the_bundled_colour_without_its_tone_curve() {
        let bundled = Dcp::bundled("Nikon", "D7500").expect("a bundled D7500 profile");
        assert!(bundled.color.profile.tone_curve.is_some());
        let mut raw = lightcraft_raw::decode(&crate::tests_xmp::synthetic_dng_with(None, Default::default())).unwrap();
        raw.metadata.make = Some("Nikon".into());
        raw.metadata.model = Some("D7500".into());
        let decode = |id: &str| {
            let mut r = raw.clone();
            apply_from(&mut r, selection_key(id), &Registry::default());
            r.color
        };
        let (auto, chosen, lost) = (decode(""), decode("bundled"), decode("dcp:removed"));
        assert_eq!(auto.profile, bundled.color.profile);
        assert_eq!(chosen.profile, bundled.color.profile);
        assert_eq!(lost.forward_matrix, bundled.color.forward_matrix);
        assert!(lost.profile.tone_curve.is_none());
    }
}
