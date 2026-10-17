//! Start-up compute policy: detect
//! the machine once, build rayon's one global compute pool from it before anything else uses rayon,
//! and run tract on that pool. Preferences › Performance › Processing holds optional overrides
//! (thread count, background priority, memory share); they apply at the next launch because the
//! pool can't be resized once built. The profile is logged once.

use std::path::Path;

use photocraft_engine::prefs::{BackgroundPriority, Performance, SystemResources, lenient};

/// The overrides the `performance.system` preference asks for (0 = Automatic).
pub fn overrides_of(c: &SystemResources) -> lightcraft_sysmem::Overrides {
    lightcraft_sysmem::Overrides {
        threads: (c.threads > 0).then_some(c.threads as usize),
        background_priority: match c.background_priority {
            BackgroundPriority::Low => lightcraft_sysmem::BackgroundPriority::Low,
            BackgroundPriority::Normal => lightcraft_sysmem::BackgroundPriority::Normal,
        },
        memory_percent: (c.memory_percent > 0).then_some(c.memory_percent.min(90) as u8),
    }
}

/// The saved overrides (Automatic when the file is missing or unreadable; a bad value falls back to
/// its default alone).
pub fn saved_overrides(prefs_file: Option<&Path>) -> lightcraft_sysmem::Overrides {
    let saved = prefs_file.and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    let performance: Performance =
        saved.and_then(|v| v.get("performance").cloned()).and_then(|p| lenient::<Performance>(p).ok()).map(|(p, _)| p).unwrap_or_default();
    overrides_of(&performance.system)
}

/// Build the compute pool and point tract at it. Call before anything uses rayon.
pub fn init(prefs_file: Option<&Path>) {
    let full_speed = prefs_file
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.pointer("/performance/system/fullSpeedOnBattery").and_then(serde_json::Value::as_bool))
        .unwrap_or(false);
    lightcraft_sysmem::power::set_full_speed(full_speed);
    lightcraft_sysmem::init(saved_overrides(prefs_file));
    lightcraft_catalog::local::set_compute_threads(lightcraft_sysmem::profile().threads);
    li_seg::init_executor();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_by_default_and_overrides_when_set() {
        assert_eq!(overrides_of(&SystemResources::default()), lightcraft_sysmem::Overrides::default());
        let c = SystemResources { threads: 3, background_priority: BackgroundPriority::Normal, memory_percent: 120, ..Default::default() };
        let o = overrides_of(&c);
        assert_eq!((o.threads, o.background_priority, o.memory_percent), (Some(3), lightcraft_sysmem::BackgroundPriority::Normal, Some(90)));
    }

    #[test]
    fn saved_preferences_are_read_leniently() {
        let dir = std::env::temp_dir().join(format!("li-compute-policy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("preferences.json");
        std::fs::write(&file, r#"{"performance":{"historyStates":"bad","system":{"threads":2,"backgroundPriority":"normal"}}}"#).unwrap();
        let o = saved_overrides(Some(&file));
        assert_eq!((o.threads, o.background_priority, o.memory_percent), (Some(2), lightcraft_sysmem::BackgroundPriority::Normal, None));
        std::fs::write(&file, "not json").unwrap();
        assert_eq!(saved_overrides(Some(&file)), lightcraft_sysmem::Overrides::default());
        assert_eq!(saved_overrides(None), lightcraft_sysmem::Overrides::default());
        let _ = std::fs::remove_dir_all(dir);
    }
}
