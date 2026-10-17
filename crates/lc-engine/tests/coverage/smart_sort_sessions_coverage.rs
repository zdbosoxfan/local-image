use lightcraft_engine::catalog::{Catalog, Op, Photo, PhotoId, Source};
use lightcraft_engine::smart_sort::sessions::{NO_TIME, SessionSettings, session_of, split_sessions};

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
