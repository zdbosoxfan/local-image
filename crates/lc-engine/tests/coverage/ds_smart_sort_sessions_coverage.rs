use lightcraft_engine::catalog::{Catalog, Op, Photo, PhotoId, Source};
use lightcraft_engine::smart_sort::sessions::{NO_TIME, SessionSettings, Sessions, SortSession, session_of, split_sessions};
use std::collections::BTreeMap;

// Helper: build a catalog with photos having optional capture times.
fn make_catalog(times: &[Option<&str>]) -> (Catalog, Vec<PhotoId>) {
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

fn make_settings(enabled: bool, gap_minutes: u32) -> SessionSettings {
    SessionSettings { enabled, gap_minutes, ..Default::default() }
}

#[test]
fn empty_input_returns_empty_sessions_and_no_time() {
    let c = Catalog::new();
    let empty: Vec<PhotoId> = Vec::new();
    let s = split_sessions(&c, &empty, &SessionSettings::default());
    assert!(s.sessions.is_empty());
    assert!(s.no_time.is_empty());
}

#[test]
fn single_photo_creates_single_session_with_same_start_and_end() {
    let (c, ids) = make_catalog(&[Some("2026-04-01T10:00:00")]);
    let s = split_sessions(&c, &ids, &make_settings(true, 20));
    assert_eq!(s.sessions.len(), 1);
    assert_eq!(s.sessions[0].photos, vec![ids[0]]);
    assert_eq!(s.sessions[0].start, "2026-04-01T10:00:00");
    assert_eq!(s.sessions[0].end, "2026-04-01T10:00:00");
    assert_eq!(s.sessions[0].index, 1);
    assert!(s.no_time.is_empty());
}

#[test]
fn gap_larger_than_threshold_splits_into_multiple_sessions() {
    let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:10:00"), Some("2026-04-01T10:35:00"), Some("2026-04-01T10:45:00")];
    let (c, ids) = make_catalog(&times);
    let s = split_sessions(&c, &ids, &make_settings(true, 20));
    assert_eq!(s.sessions.len(), 2);
    assert_eq!(s.sessions[0].photos, vec![ids[0], ids[1]]);
    assert_eq!(s.sessions[1].photos, vec![ids[2], ids[3]]);
    assert_eq!(s.sessions[0].index, 1);
    assert_eq!(s.sessions[1].index, 2);
    assert!(s.no_time.is_empty());
}

#[test]
fn gap_equal_to_threshold_does_not_split() {
    let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:20:00"), Some("2026-04-01T10:40:00")];
    let (c, ids) = make_catalog(&times);
    let s = split_sessions(&c, &ids, &make_settings(true, 20));
    assert_eq!(s.sessions.len(), 1);
    assert_eq!(s.sessions[0].photos, ids);
    assert!(s.no_time.is_empty());
}

#[test]
fn gap_smaller_than_threshold_does_not_split() {
    let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:10:00"), Some("2026-04-01T10:15:00")];
    let (c, ids) = make_catalog(&times);
    let s = split_sessions(&c, &ids, &make_settings(true, 20));
    assert_eq!(s.sessions.len(), 1);
    assert_eq!(s.sessions[0].photos, ids);
    assert!(s.no_time.is_empty());
}

#[test]
fn disabled_settings_ignore_large_gap() {
    let times = [
        Some("2026-01-01T00:00:00"),
        Some("2026-01-02T00:00:00"), // 24h gap
    ];
    let (c, ids) = make_catalog(&times);
    let s = split_sessions(&c, &ids, &make_settings(false, 20));
    assert_eq!(s.sessions.len(), 1);
    assert_eq!(s.sessions[0].photos, ids);
    assert!(s.no_time.is_empty());
}

#[test]
fn no_time_photos_are_collected_in_input_order() {
    let times = [Some("2026-04-01T10:00:00"), None, Some("not-a-time"), Some("2026-04-01T11:00:00")];
    let (c, ids) = make_catalog(&times);
    let s = split_sessions(&c, &ids, &make_settings(true, 20));
    // timed photos: 10:00 and 11:00 -> gap 60 min > 20, so two sessions
    assert_eq!(s.sessions.len(), 2);
    assert_eq!(s.sessions[0].photos, vec![ids[0]]);
    assert_eq!(s.sessions[1].photos, vec![ids[3]]);
    assert_eq!(s.no_time, vec![ids[1], ids[2]]);
}

#[test]
fn unknown_ids_are_ignored() {
    let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:10:00")];
    let (c, ids) = make_catalog(&times);
    let mixed = vec![ids[0], PhotoId(99), ids[1]];
    let s = split_sessions(&c, &mixed, &make_settings(true, 20));
    assert_eq!(s.sessions.len(), 1);
    assert_eq!(s.sessions[0].photos, vec![ids[0], ids[1]]);
    assert!(s.no_time.is_empty());
}

#[test]
fn equal_times_preserve_input_order() {
    let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:00:00"), Some("2026-04-01T10:00:00")];
    let (c, ids) = make_catalog(&times);
    // reorder ids: [2,0,1]
    let reordered = vec![ids[2], ids[0], ids[1]];
    let s = split_sessions(&c, &reordered, &make_settings(true, 20));
    assert_eq!(s.sessions.len(), 1);
    assert_eq!(s.sessions[0].photos, reordered);
    assert!(s.no_time.is_empty());
}

#[test]
fn custom_name_by_start_time_is_used() {
    let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:30:00"), Some("2026-04-01T10:40:00")];
    let (c, ids) = make_catalog(&times);
    let mut st = make_settings(true, 20);
    st.names.insert("2026-04-01T10:00:00".to_string(), "Morning".to_string());
    let s = split_sessions(&c, &ids, &st);
    assert_eq!(s.sessions.len(), 2);
    assert_eq!(s.sessions[0].name, "Morning");
    assert_eq!(s.sessions[1].name, "Session 2");
}

#[test]
fn gap_clamp_low_uses_minimum_1_minute() {
    let times = [
        Some("2026-04-01T10:00:00"),
        Some("2026-04-01T10:02:00"), // 2 min gap > clamped 1 min
    ];
    let (c, ids) = make_catalog(&times);
    let s = split_sessions(&c, &ids, &make_settings(true, 0)); // 0 clamped to 1
    assert_eq!(s.sessions.len(), 2);
}

#[test]
fn gap_clamp_high_uses_maximum_1440_minutes() {
    let times = [
        Some("2026-01-01T00:00:00"),
        Some("2026-01-02T01:00:00"), // 1500 min gap > 1440
    ];
    let (c, ids) = make_catalog(&times);
    let s = split_sessions(&c, &ids, &make_settings(true, 2000)); // 2000 clamped to 1440
    assert_eq!(s.sessions.len(), 2);
}

#[test]
fn session_settings_round_trips_through_json() {
    let mut names = BTreeMap::new();
    names.insert("time1".to_string(), "Custom".to_string());
    let settings = SessionSettings { enabled: true, gap_minutes: 45, export_folders: true, names };

    let json = serde_json::to_string(&settings).unwrap();
    let back: SessionSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(settings, back);
}

#[test]
fn sessions_round_trip_through_json() {
    let s = Sessions {
        sessions: vec![SortSession {
            index: 1,
            name: "Session 1".to_string(),
            start: "2026-04-01T10:00:00".to_string(),
            end: "2026-04-01T10:10:00".to_string(),
            photos: vec![PhotoId(1), PhotoId(2)],
        }],
        no_time: vec![PhotoId(3)],
    };

    let json = serde_json::to_string(&s).unwrap();
    let back: Sessions = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}

#[test]
fn session_of_returns_correct_session() {
    let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:05:00"), Some("2026-04-01T11:00:00")];
    let (c, ids) = make_catalog(&times);
    let s = split_sessions(&c, &ids, &make_settings(true, 20));
    assert_eq!(s.sessions.len(), 2);

    let found = session_of(&s, ids[0]).unwrap();
    assert_eq!(found.index, 1);
    assert_eq!(found.photos, vec![ids[0], ids[1]]);

    let found = session_of(&s, ids[2]).unwrap();
    assert_eq!(found.index, 2);
    assert_eq!(found.photos, vec![ids[2]]);

    assert!(session_of(&s, PhotoId(999)).is_none());
}

#[test]
fn key_of_returns_start_for_timed_and_no_time_for_untimed() {
    let times = [Some("2026-04-01T10:00:00"), None, Some("2026-04-01T11:00:00")];
    let (c, ids) = make_catalog(&times);
    let s = split_sessions(&c, &ids, &make_settings(true, 20));

    assert_eq!(s.key_of(ids[0]), "2026-04-01T10:00:00");
    assert_eq!(s.key_of(ids[1]), NO_TIME);
    assert_eq!(s.key_of(ids[2]), "2026-04-01T11:00:00");
}

#[test]
fn name_of_returns_session_name_or_no_time_default() {
    let times = [Some("2026-04-01T10:00:00"), None, Some("2026-04-01T11:00:00")];
    let (c, ids) = make_catalog(&times);
    let s = split_sessions(&c, &ids, &make_settings(true, 20));

    assert_eq!(s.name_of(ids[0]), "Session 1");
    assert_eq!(s.name_of(ids[1]), "No time");
    assert_eq!(s.name_of(ids[2]), "Session 2");
}

#[test]
fn split_sessions_is_deterministic() {
    let times = [Some("2026-04-01T10:00:00"), Some("2026-04-01T10:10:00"), Some("2026-04-01T10:35:00"), Some("2026-04-01T10:45:00"), None];
    let (c, ids) = make_catalog(&times);
    let st = make_settings(true, 20);
    let s1 = split_sessions(&c, &ids, &st);
    let s2 = split_sessions(&c, &ids, &st);
    assert_eq!(s1, s2);
}

#[test]
fn malformed_capture_times_do_not_panic() {
    let weird_times = [
        Some(""),
        Some("garbage"),
        Some("2026-13-99T99:99:99"),
        Some("2026-04-01"),                    // missing time
        Some("2026-04-01T10:00:00Z"),          // timezone
        Some("2026-04-01T10:00:00.123456789"), // fractional seconds
        Some("+1234567890"),                   // just number
    ];
    let (c, ids) = make_catalog(&weird_times);
    let s = split_sessions(&c, &ids, &make_settings(true, 20));

    // All input photos should appear either in a session or in no_time.
    let total_photos: usize = s.sessions.iter().map(|sess| sess.photos.len()).sum();
    assert_eq!(total_photos + s.no_time.len(), ids.len());
}
