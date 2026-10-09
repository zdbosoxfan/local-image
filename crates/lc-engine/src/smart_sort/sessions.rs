//! Sessions by capture time for Smart Sort step 1.
//!
//! Photos with a parseable capture time are ordered by capture time and split into sessions
//! when the gap to the previous photo exceeds the configured `gap_minutes`. Photos without a
//! parseable capture time are collected in `no_time` in input order.

use std::collections::BTreeMap;

use lightcraft_catalog::stacks::iso_seconds;
use lightcraft_catalog::{Catalog, PhotoId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionSettings {
    pub enabled: bool,
    pub gap_minutes: u32,
    pub names: BTreeMap<String, String>,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self { enabled: false, gap_minutes: 20, names: BTreeMap::new() }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SortSession {
    pub index: usize,
    pub name: String,
    pub start: String,
    pub end: String,
    pub photos: Vec<PhotoId>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Sessions {
    pub sessions: Vec<SortSession>,
    pub no_time: Vec<PhotoId>,
}

/// Group `ids` into capture-time sessions. Unknown ids are ignored; parseable capture times are
/// ordered by `iso_seconds` (stable for equal times, preserving input order). `no_time` holds
/// photos with no or unparseable capture time.
pub fn split_sessions(cat: &Catalog, ids: &[PhotoId], settings: &SessionSettings) -> Sessions {
    let mut no_time = Vec::new();
    let mut timed = Vec::new();

    for (idx, id) in ids.iter().enumerate() {
        let Some(photo) = cat.photo(*id) else {
            continue;
        };
        let Some(captured) = photo.captured.as_deref() else {
            no_time.push(*id);
            continue;
        };
        let Some(secs) = iso_seconds(captured) else {
            no_time.push(*id);
            continue;
        };
        timed.push((secs, captured.to_string(), *id, idx));
    }

    // Stable sort keeps the original input order for equal times (via idx as tie-breaker).
    timed.sort_by_key(|&(secs, _, _, idx)| (secs, idx));

    let effective_gap_secs = (settings.gap_minutes.clamp(1, 1440) as i64) * 60;
    let mut sessions = Vec::new();
    let mut current: Option<(String, String, Vec<PhotoId>)> = None; // (start, end, photos)
    let mut last_secs: Option<i64> = None;
    let mut session_index = 1usize;

    for (secs, captured, id, _) in timed {
        let captured_owned = captured;
        if let Some(last) = last_secs
            && settings.enabled
            && (secs - last) > effective_gap_secs
        {
            if let Some((start, end, photos)) = current.take() {
                sessions.push(SortSession {
                    index: session_index,
                    name: settings.names.get(&start).cloned().unwrap_or_else(|| format!("Session {session_index}")),
                    start,
                    end,
                    photos,
                });
            }
            session_index += 1;
            current = Some((captured_owned.clone(), captured_owned, vec![id]));
            last_secs = Some(secs);
            continue;
        }

        match &mut current {
            None => {
                current = Some((captured_owned.clone(), captured_owned, vec![id]));
            }
            Some((_, end, photos)) => {
                *end = captured_owned;
                photos.push(id);
            }
        }
        last_secs = Some(secs);
    }

    if let Some((start, end, photos)) = current {
        sessions.push(SortSession {
            index: session_index,
            name: settings.names.get(&start).cloned().unwrap_or_else(|| format!("Session {session_index}")),
            start,
            end,
            photos,
        });
    }

    Sessions { sessions, no_time }
}

/// The session containing `id`, if any.
pub fn session_of(sessions: &Sessions, id: PhotoId) -> Option<&SortSession> {
    sessions.sessions.iter().find(|s| s.photos.contains(&id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_catalog::{Catalog, Op, Photo, PhotoId, Source};

    fn cat(times: &[Option<&str>]) -> (Catalog, Vec<PhotoId>) {
        let mut c = Catalog::new();
        let mut ids = Vec::new();
        for (i, t) in times.iter().enumerate() {
            let id = c.alloc_photo_id();
            let mut p = Photo::new(id, Source::Demo { scene: 1 }, &format!("{i}.jpg"), "JPEG", 60, 40, "2026-01-01T00:00:00");
            p.captured = t.map(|s| s.to_string());
            c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
            ids.push(id);
        }
        (c, ids)
    }

    fn settings(enabled: bool, gap_minutes: u32) -> SessionSettings {
        SessionSettings { enabled, gap_minutes, ..SessionSettings::default() }
    }

    #[test]
    fn gap_25_minutes_splits() {
        let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:10:00"), Some("2026-04-01T10:35:00"), Some("2026-04-01T10:45:00")];
        let (c, ids) = cat(&times);
        let s = split_sessions(&c, &ids, &settings(true, 20));
        assert_eq!(s.sessions.len(), 2);
        assert_eq!(s.sessions[0].photos, vec![ids[0], ids[1]]);
        assert_eq!(s.sessions[1].photos, vec![ids[2], ids[3]]);
        assert_eq!(s.sessions[0].start, "2026-04-01T10:00:00");
        assert_eq!(s.sessions[0].end, "2026-04-01T10:10:00");
        assert_eq!(s.sessions[1].start, "2026-04-01T10:35:00");
        assert_eq!(s.sessions[1].end, "2026-04-01T10:45:00");
        assert!(s.no_time.is_empty());
    }

    #[test]
    fn gap_10_minutes_does_not_split() {
        let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:10:00"), Some("2026-04-01T10:20:00")];
        let (c, ids) = cat(&times);
        let s = split_sessions(&c, &ids, &settings(true, 20));
        assert_eq!(s.sessions.len(), 1);
        assert_eq!(s.sessions[0].photos, ids);
        assert!(s.no_time.is_empty());
    }

    #[test]
    fn exactly_20_minutes_does_not_split() {
        let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:20:00"), Some("2026-04-01T10:40:00")];
        let (c, ids) = cat(&times);
        let s = split_sessions(&c, &ids, &settings(true, 20));
        assert_eq!(s.sessions.len(), 1);
        assert_eq!(s.sessions[0].photos, ids);
        assert!(s.no_time.is_empty());
    }

    #[test]
    fn no_time_photos_are_separate() {
        let times = [Some("2026-04-01T10:00:00"), None, Some("not-a-time"), Some("2026-04-01T11:00:00")];
        let (c, ids) = cat(&times);
        let s = split_sessions(&c, &ids, &settings(true, 20));
        // gaps: 60 min between t0 and t3 -> two sessions
        assert_eq!(s.sessions.len(), 2);
        assert_eq!(s.sessions[0].photos, vec![ids[0]]);
        assert_eq!(s.sessions[1].photos, vec![ids[3]]);
        assert_eq!(s.no_time, vec![ids[1], ids[2]]);
    }

    #[test]
    fn custom_name_by_start_time() {
        let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:30:00"), Some("2026-04-01T10:40:00")];
        let (c, ids) = cat(&times);
        let mut st = settings(true, 20);
        st.names.insert("2026-04-01T10:00:00".to_string(), "Morning".to_string());
        let s = split_sessions(&c, &ids, &st);
        assert_eq!(s.sessions.len(), 2);
        assert_eq!(s.sessions[0].name, "Morning");
        assert_eq!(s.sessions[1].name, "Session 2");
    }

    #[test]
    fn equal_times_preserve_input_order() {
        let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:00:00"), Some("2026-04-01T10:00:00")];
        let (c, ids) = cat(&times);
        // pass ids in a different order
        let reordered = vec![ids[2], ids[0], ids[1]];
        let s = split_sessions(&c, &reordered, &settings(true, 20));
        assert_eq!(s.sessions.len(), 1);
        assert_eq!(s.sessions[0].photos, reordered);
        assert!(s.no_time.is_empty());
    }

    #[test]
    fn unknown_ids_are_ignored() {
        let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:10:00")];
        let (c, ids) = cat(&times);
        let mixed = vec![ids[0], PhotoId(99), ids[1]];
        let s = split_sessions(&c, &mixed, &settings(true, 20));
        assert_eq!(s.sessions.len(), 1);
        assert_eq!(s.sessions[0].photos, vec![ids[0], ids[1]]);
        assert!(s.no_time.is_empty());
    }

    #[test]
    fn gap_clamp_low() {
        let times = [
            Some("2026-04-01T10:00:00"),
            Some("2026-04-01T10:02:00"), // 2 min gap > clamped 1 min
        ];
        let (c, ids) = cat(&times);
        let s = split_sessions(&c, &ids, &settings(true, 0)); // clamped to 1 min
        assert_eq!(s.sessions.len(), 2);
    }

    #[test]
    fn gap_clamp_high() {
        let times = [
            Some("2026-01-01T00:00:00"),
            Some("2026-01-02T01:00:00"), // 25 h = 1500 min, > clamped 1440 min
        ];
        let (c, ids) = cat(&times);
        let s = split_sessions(&c, &ids, &settings(true, 2000)); // clamped to 1440 min
        assert_eq!(s.sessions.len(), 2);
    }
}
