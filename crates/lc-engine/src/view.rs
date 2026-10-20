//! Library view state: what the grid shows and what is selected.

use lightcraft_catalog::{AlbumId, Catalog, Filter, PhotoId};
use serde::{Deserialize, Serialize};

/// The "My Photos" source in the left panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
pub enum LibrarySource {
    #[default]
    All,
    /// Imported in the last import session (we treat "recent" as the latest import date).
    RecentlyAdded,
    Album(AlbumId),
    RecentlyDeleted,
    /// Photos with picks.
    Picks,
    /// A folder on disk ([`crate::Session::browse`]): its files, added to the library or not.
    Folder,
    /// Photos whose original file can't be found (`library.missing`).
    Missing,
}

/// The folder a [`LibrarySource::Folder`] view shows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Browse {
    pub path: String,
    pub subfolders: bool,
}

/// How far back Recently Added reaches from the latest import.
pub const RECENT_DAYS: i64 = 30;

impl LibrarySource {
    pub fn to_filter(&self, f: &Filter, cat: &Catalog) -> Filter {
        let mut f = f.clone();
        match self {
            LibrarySource::All => {}
            // everything imported in the 30 days up to the latest import (by import, newest first)
            LibrarySource::RecentlyAdded => {
                let latest = cat.photos().filter(|p| p.in_library()).map(|p| p.imported.clone()).max().unwrap_or_default();
                let from = lightcraft_catalog::stacks::iso_seconds(&latest)
                    .map(|s| lightcraft_catalog::dates::civil(s - RECENT_DAYS * 86_400))
                    .unwrap_or_else(|| latest.get(..10).unwrap_or("").to_string());
                f.imported_from = Some(from);
            }
            LibrarySource::Album(a) => f.album = Some(*a),
            LibrarySource::RecentlyDeleted => f.deleted = true,
            LibrarySource::Picks => f.flag = Some(lightcraft_catalog::Flag::Pick),
            // the folder itself is filled in by the session (it holds the path)
            LibrarySource::Folder | LibrarySource::Missing => {}
        }
        f
    }

    pub fn label(&self, cat: &Catalog) -> String {
        match self {
            LibrarySource::All => "All Photos".into(),
            LibrarySource::RecentlyAdded => "Recently Added".into(),
            LibrarySource::Album(a) => cat.album(*a).map(|a| a.name.clone()).unwrap_or_else(|| "Album".into()),
            LibrarySource::RecentlyDeleted => "Recently Deleted".into(),
            LibrarySource::Picks => "Picks".into(),
            LibrarySource::Folder => "Folder".into(),
            LibrarySource::Missing => "Missing Photos".into(),
        }
    }
}

/// Selected photos (ordered) and the active (most-selected) one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub ids: Vec<PhotoId>,
    pub active: Option<PhotoId>,
}

impl Selection {
    pub fn single(id: PhotoId) -> Selection {
        Selection { ids: vec![id], active: Some(id) }
    }
    pub fn contains(&self, id: PhotoId) -> bool {
        self.ids.contains(&id)
    }
    pub fn toggle(&mut self, id: PhotoId) {
        if let Some(i) = self.ids.iter().position(|x| *x == id) {
            self.ids.remove(i);
            if self.active == Some(id) {
                self.active = self.ids.last().copied();
            }
        } else {
            self.ids.push(id);
            self.active = Some(id);
        }
    }
    /// Shift-click: select the range between the active photo and `id` in `order`.
    pub fn extend_to(&mut self, id: PhotoId, order: &[PhotoId]) {
        let anchor = self.active.unwrap_or(id);
        let (Some(a), Some(b)) = (order.iter().position(|x| *x == anchor), order.iter().position(|x| *x == id)) else {
            *self = Selection::single(id);
            return;
        };
        let (lo, hi) = (a.min(b), a.max(b));
        for x in &order[lo..=hi] {
            if !self.ids.contains(x) {
                self.ids.push(*x);
            }
        }
        self.active = Some(id);
    }
}

/// One active filter, shown as a removable chip above the grid.
#[derive(Clone, Debug, PartialEq)]
pub struct FilterChip {
    pub label: String,
    /// `library.filter` patch that clears just this filter.
    pub clear: serde_json::Value,
}

/// The chips for every constraint in the filter bar / search / sidebar (not the source itself).
pub fn filter_chips(f: &Filter, cat: &Catalog) -> Vec<FilterChip> {
    use lightcraft_catalog::{MediaKind, RatingOp};
    use serde_json::{Value::Null, json};
    let mut v = Vec::new();
    let mut add = |label: String, clear: serde_json::Value| v.push(FilterChip { label, clear });
    if !f.text.trim().is_empty() {
        add(format!("Search: “{}”", f.text.trim()), json!({"text": ""}));
    }
    if f.rating > 0 {
        let op = match f.rating_op {
            RatingOp::AtLeast => "≥",
            RatingOp::Exactly => "=",
            RatingOp::AtMost => "≤",
        };
        add(format!("Rating {op} {}★", f.rating), json!({"rating": 0}));
    }
    if let Some(fl) = f.flag {
        add(format!("Flag: {}", format!("{fl:?}").to_lowercase()), json!({"flag": Null}));
    }
    let mut labels = f.labels.clone();
    labels.extend(f.label.filter(|l| !labels.contains(l)));
    if !labels.is_empty() {
        let names: Vec<String> = labels.iter().map(|l| cat.label_name(*l)).collect();
        add(format!("Label: {}", names.join(" or ")), json!({"label": Null, "labels": []}));
    }
    if let Some(k) = f.kind {
        let n = match k {
            MediaKind::Image => "Photos",
            MediaKind::Raw => "Raw",
            MediaKind::Video => "Videos",
        };
        add(format!("Type: {n}"), json!({"kind": Null}));
    }
    if let Some(m) = &f.merged {
        let n = match m.as_str() {
            "hdr" => "HDR",
            "panorama" => "Panoramas",
            "hdrPanorama" => "HDR panoramas",
            _ => "Merged",
        };
        add(format!("Type: {n}"), json!({"merged": Null}));
    }
    if let Some(e) = f.edited {
        add(if e { "Edited".into() } else { "Unedited".into() }, json!({"edited": Null}));
    }
    for (name, key, val) in
        [("Camera", "camera", &f.camera), ("Lens", "lens", &f.lens), ("Keyword", "keyword", &f.keyword), ("Person", "person", &f.person)]
    {
        if let Some(x) = val {
            add(format!("{name}: {x}"), json!({key: Null}));
        }
    }
    if !f.person_ids.is_empty() {
        let ids = f.person_ids.iter().map(u64::to_string).collect::<Vec<_>>().join(if f.people_everyone { " and " } else { " or " });
        add(format!("People: {ids}"), json!({"personIds": [], "peopleEveryone": false}));
    }
    if let Some(d) = &f.date {
        add(format!("Date: {}", date_label(d)), json!({"date": Null}));
    }
    if let Some(d) = &f.imported {
        add(format!("Imported: {}", date_label(d)), json!({"imported": Null}));
    }
    match (&f.date_from, &f.date_to) {
        (Some(a), Some(b)) => add(format!("Captured {a} – {b}"), json!({"dateFrom": Null, "dateTo": Null})),
        (Some(a), None) => add(format!("Captured from {a}"), json!({"dateFrom": Null})),
        (None, Some(b)) => add(format!("Captured until {b}"), json!({"dateTo": Null})),
        _ => {}
    }
    if let Some(rs) = f.rule_set.as_ref().filter(|r| !r.rules.is_empty()) {
        add(format!("Rules: {}", rs.describe()), json!({"ruleSet": Null}));
    }
    if !f.only.is_empty() {
        add(format!("Only {} photos", f.only.len()), json!({"only": []}));
    }
    v
}

/// `2026-01-16` → "January 16, 2026"; `2026-01` → "January 2026"; anything else as is.
fn date_label(d: &str) -> String {
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let parts: Vec<&str> = d.split('-').collect();
    let month = parts.get(1).and_then(|m| m.parse::<usize>().ok()).filter(|m| (1..=12).contains(m)).map(|m| MONTHS[m - 1]);
    match (parts.as_slice(), month) {
        ([y, _, day], Some(m)) => format!("{m} {}, {y}", day.trim_start_matches('0')),
        ([y, _], Some(m)) => format!("{m} {y}"),
        _ => d.to_string(),
    }
}

/// Apply a soft-proof patch atomically. Profiles are validated before any flags or
/// view state change; the parsed bytes then travel with jobs instead of being reread.
pub fn update_soft_proof(current: &lightcraft_pipeline::Proof, patch: &serde_json::Value) -> Result<lightcraft_pipeline::Proof, String> {
    use lightcraft_pipeline::{OutputSpace, ProofIntent, ProofProfile};
    let mut proof = current.clone();
    if let Some(value) = patch.get("space") {
        proof.space = value
            .as_str()
            .and_then(OutputSpace::parse)
            .ok_or_else(|| "view.softProof: unknown space (srgb|displayP3|adobeRgb|proPhoto|rec2020)".to_string())?;
        proof.profile = None;
    }
    if let Some(value) = patch.get("profilePath") {
        if value.is_null() {
            proof.profile = None;
        } else {
            use std::io::Read;
            let path = value.as_str().filter(|p| !p.trim().is_empty()).ok_or("view.softProof: profilePath must be an ICC file path")?;
            const LIMIT: u64 = 16 * 1024 * 1024;
            let file = std::fs::File::open(path).map_err(|e| format!("Cannot open proof profile: {e}"))?;
            if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err("Proof profile must be a regular file".into());
            }
            let mut bytes = Vec::new();
            file.take(LIMIT + 1).read_to_end(&mut bytes).map_err(|e| format!("Cannot read proof profile: {e}"))?;
            if bytes.len() as u64 > LIMIT {
                return Err("Proof profile exceeds 16 MiB".into());
            }
            proof.profile = Some(ProofProfile::try_from(std::sync::Arc::new(bytes)).map_err(|e| format!("Invalid proof profile: {e}"))?);
        }
    }
    if let Some(value) = patch.get("intent") {
        proof.intent = match value.as_str() {
            Some("perceptual") => ProofIntent::Perceptual,
            Some("relativeColorimetric" | "relative") => ProofIntent::RelativeColorimetric,
            Some("saturation") => ProofIntent::Saturation,
            Some("absoluteColorimetric" | "absolute") => ProofIntent::AbsoluteColorimetric,
            _ => return Err("view.softProof: unknown rendering intent".into()),
        };
    }
    for (key, flag) in [
        ("destWarning", &mut proof.dest_warning),
        ("displayWarning", &mut proof.display_warning),
        ("bpc", &mut proof.bpc),
        ("simulatePaper", &mut proof.simulate_paper),
    ] {
        if let Some(value) = patch.get(key) {
            *flag = value.as_bool().ok_or_else(|| format!("view.softProof: {key} must be a boolean"))?;
        }
    }
    Ok(proof)
}

#[cfg(test)]
mod proof_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn proof_thumbnail_adapter_does_not_reuse_or_poison_normal_cache() {
        use lightcraft_pipeline::{Proof, ProofProfile};
        let mut session = crate::Session::with_demo();
        let id = PhotoId(1);
        let job = session.thumb_job(id, 64).unwrap();
        let plain_key = job.key;
        let plain = job.run();
        session.accept(&plain);
        let proof = Proof {
            profile: Some(
                ProofProfile::try_from(std::sync::Arc::new(include_bytes!("../../pc-cms/profiles/photocraft-coated-cmyk.icc").to_vec())).unwrap(),
            ),
            simulate_paper: true,
            ..Default::default()
        };
        let proof_job = session.thumb_job(id, 64).unwrap().with_proof(Some(proof));
        assert_ne!(proof_job.key, plain_key);
        let proofed = proof_job.run();
        assert_ne!(plain.rendered.as_ref().unwrap().image, proofed.rendered.as_ref().unwrap().image);
        let next = session.thumb_job(id, 64).unwrap().run();
        assert_eq!(plain.rendered.unwrap().image, next.rendered.unwrap().image);
    }

    #[test]
    fn proof_patch_is_atomic_and_legacy_settings_load() {
        let old: lightcraft_pipeline::Proof = serde_json::from_value(json!({"space":"adobeRgb", "destWarning":true})).unwrap();
        assert!(old.bpc);
        assert!(old.profile.is_none());
        let patched = update_soft_proof(&old, &json!({"intent":"perceptual", "simulatePaper":true, "bpc":false})).unwrap();
        assert_ne!(old.key(), patched.key());
        assert!(update_soft_proof(&old, &json!({"destWarning":false,"intent":"unknown"})).is_err());
        assert!(old.dest_warning);
        assert!(update_soft_proof(&old, &json!({"profilePath":"", "space":"srgb"})).is_err());
        assert_eq!(old.space, lightcraft_pipeline::OutputSpace::AdobeRgb);
    }
}
