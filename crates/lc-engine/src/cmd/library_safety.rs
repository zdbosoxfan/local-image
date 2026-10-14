use super::{CommandSpec, always, bad, cmd};
use serde_json::json;
fn on_disk(s: &crate::Session) -> std::result::Result<(), String> {
    if s.library.as_ref().is_some_and(|l| l.on_disk) { Ok(()) } else { Err("Open a library on disk first".into()) }
}
pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "library.backupPreferences",
            "Catalog Backup Preferences",
            [],
            None,
            "{frequency?: never|everyQuit|daily|weekly, folder?, keep?: 1..100}",
            always,
            |s, p| {
                if p.as_object().is_some_and(|v| !v.is_empty()) {
                    let mut value = serde_json::to_value(&s.backup_preferences).map_err(|e| bad("library.backupPreferences", e.to_string()))?;
                    for key in ["frequency", "folder", "keep"] {
                        if let Some(v) = p.get(key) {
                            value[key] = v.clone();
                        }
                    }
                    let prefs: crate::library_safety::BackupPreferences =
                        serde_json::from_value(value).map_err(|e| bad("library.backupPreferences", e.to_string()))?;
                    if !(1..=100).contains(&prefs.keep) {
                        return Err(bad("library.backupPreferences", "keep must be 1..100"));
                    }
                    if !prefs.folder.is_empty() && !std::path::Path::new(&prefs.folder).is_absolute() {
                        return Err(bad("library.backupPreferences", "folder must be absolute"));
                    }
                    let previous = std::mem::replace(&mut s.backup_preferences, prefs);
                    if let Err(e) = s.save_prefs() {
                        s.backup_preferences = previous;
                        return Err(e);
                    }
                }
                serde_json::to_value(&s.backup_preferences).map_err(|e| bad("library.backupPreferences", e.to_string()))
            }
        ),
        cmd!(
            "library.verifyOptimize",
            "Verify / Optimize Library",
            ["Library"],
            None,
            "{} — verify persisted bytes without repair, then compact the journal",
            on_disk,
            |s, _| { s.verify_optimize_library().map(|photos| json!({"verified":photos,"optimized":true})) }
        ),
        cmd!("library.backupNow", "Back Up Catalog Now", ["Library"], None, "{} — snapshot, verify and publish a backup", on_disk, |s, _| {
            s.end_interaction()?;
            s.compact_library()?;
            s.backup_library_after_snapshot(true).map(|path| json!({"path":path.map(|p| p.display().to_string())}))
        }),
    ]
}
