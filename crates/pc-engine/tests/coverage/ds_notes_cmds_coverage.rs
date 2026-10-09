use photocraft_engine::{Session, notes_cmds};
use serde_json::json;

/// Creates a session with a 100x80 RGB document, as used by `notes_cmds` internal tests.
fn session_with_doc() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 100, "height": 80})).expect("file.new should succeed");
    s
}

/// Asserts that a string is a PDF-style date (`D:YYYYMMDDhhmmssZ`).
fn assert_pdf_date(date: &str) {
    assert!(date.starts_with("D:"), "date should start with 'D:', got {date:?}");
    assert!(date.ends_with('Z'), "date should end with 'Z', got {date:?}");
    assert_eq!(date.len(), 17, "date length should be 17, got {date:?} ({})", date.len());
    let digits = &date[2..16];
    assert!(digits.chars().all(|c| c.is_ascii_digit()), "date middle should be 14 digits, got {date:?}");
}

/// Asserts that a JSON array of three numbers approximately equals the given RGB values.
fn assert_color_eq(actual: &serde_json::Value, expected: [f64; 3]) {
    let arr = actual.as_array().expect("color should be an array");
    assert_eq!(arr.len(), 3, "color array should have 3 elements");
    for (i, &v) in expected.iter().enumerate() {
        let got = arr[i].as_f64().expect("color component should be a number");
        assert!((got - v).abs() < 1e-5, "color component {i} expected {v}, got {got}");
    }
}

// ---------------------------------------------------------------------------
// Command registry / specs
// ---------------------------------------------------------------------------

#[test]
fn specs_contain_all_note_commands() {
    let specs = notes_cmds::specs();
    let ids: Vec<&str> = specs.iter().map(|s| s.id).collect();
    assert_eq!(ids.len(), 5, "expected 5 note commands, got {ids:?}");
    for expected in ["notes.add", "notes.set", "notes.delete", "notes.list", "file.import.notes"] {
        assert!(ids.contains(&expected), "missing command {expected}");
    }
    // All note commands should be journaled except notes.list.
    for spec in &specs {
        if spec.id == "notes.list" {
            assert!(!spec.journal, "notes.list should not be journaled");
        } else {
            assert!(spec.journal, "command {} should be journaled", spec.id);
        }
    }
}

// ---------------------------------------------------------------------------
// notes.add
// ---------------------------------------------------------------------------

#[test]
fn add_note_returns_index_and_note_json() {
    let mut s = session_with_doc();
    let r = s.execute("notes.add", json!({"x": 10.0, "y": 12.0})).unwrap();
    assert_eq!(r["index"], 0);
    let note = &r["note"];
    assert_eq!(note["author"], "");
    assert_eq!(note["text"], "");
    assert_color_eq(&note["color"], [1.0, 1.0, 0.51]); // default pale yellow RGB
    assert_eq!(note["position"], json!([10.0, 12.0]));
    assert_eq!(note["open"], true);
    assert_pdf_date(note["modified"].as_str().unwrap());
}

#[test]
fn add_note_defaults() {
    let mut s = session_with_doc();
    s.execute("notes.add", json!({"x": 20.0, "y": 30.0})).unwrap();
    let d = s.active().unwrap().doc.clone();
    assert_eq!(d.notes.len(), 1);
    let n = &d.notes[0];
    assert_eq!(n.author, "");
    assert_eq!(n.text, "");
    assert_eq!(n.position, [20.0, 30.0]);
    assert_eq!(n.popup, [40.0, 50.0, 280.0, 190.0]); // x+20, y+20, x+260, y+160
    assert!(n.open);
    assert_pdf_date(&n.modified);
}

#[test]
fn add_note_custom_hex_color() {
    let mut s = session_with_doc();
    let r = s.execute("notes.add", json!({"x": 1.0, "y": 2.0, "color": "#ff0000"})).unwrap();
    assert_color_eq(&r["note"]["color"], [1.0, 0.0, 0.0]);
}

#[test]
fn add_note_custom_array_color() {
    let mut s = session_with_doc();
    let r = s.execute("notes.add", json!({"x": 1.0, "y": 2.0, "color": [0.1, 0.2, 0.3]})).unwrap();
    assert_color_eq(&r["note"]["color"], [0.1, 0.2, 0.3]);
}

#[test]
fn add_note_custom_fields() {
    let mut s = session_with_doc();
    let r = s
        .execute(
            "notes.add",
            json!({
                "x": 5.0, "y": 6.0,
                "text": "fix soon",
                "author": "tester",
                "open": false
            }),
        )
        .unwrap();
    let note = &r["note"];
    assert_eq!(note["text"], "fix soon");
    assert_eq!(note["author"], "tester");
    assert_eq!(note["open"], false);
}

#[test]
fn add_note_requires_x_and_y() {
    let mut s = session_with_doc();
    assert!(s.execute("notes.add", json!({"x": 1.0})).is_err());
    assert!(s.execute("notes.add", json!({"y": 2.0})).is_err());
    assert!(s.execute("notes.add", json!({})).is_err());
}

#[test]
fn add_note_rejects_non_finite_coordinates() {
    let mut s = session_with_doc();
    // NaN and Infinity are serialized to null by serde_json, so they become missing fields
    // and are rejected by the required x/y check.
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(s.execute("notes.add", json!({"x": bad, "y": 1.0})).is_err());
        assert!(s.execute("notes.add", json!({"x": 1.0, "y": bad})).is_err());
    }
}

// ---------------------------------------------------------------------------
// notes.set
// ---------------------------------------------------------------------------

#[test]
fn set_note_updates_all_fields() {
    let mut s = session_with_doc();
    s.execute("notes.add", json!({"x": 10.0, "y": 20.0})).unwrap();
    let r = s
        .execute(
            "notes.set",
            json!({
                "index": 0,
                "text": "updated",
                "author": "new author",
                "color": "#00ff00",
                "open": false,
                "x": 100.0,
                "y": 200.0
            }),
        )
        .unwrap();
    assert_eq!(r["changed"], true);
    let d = s.active().unwrap().doc.clone();
    let n = &d.notes[0];
    assert_eq!(n.text, "updated");
    assert_eq!(n.author, "new author");
    assert_eq!(n.position, [100.0, 200.0]);
    assert_eq!(n.popup, [120.0, 220.0, 360.0, 360.0]);
    assert!(!n.open);
    assert_color_eq(&r["note"]["color"], [0.0, 1.0, 0.0]);
}

#[test]
fn set_note_move_updates_popup_but_not_modified() {
    let mut s = session_with_doc();
    s.execute("notes.add", json!({"x": 10.0, "y": 20.0})).unwrap();
    let before_modified = s.active().unwrap().doc.notes[0].modified.clone();
    let r = s.execute("notes.set", json!({"index": 0, "x": 50.0, "y": 60.0})).unwrap();
    assert_eq!(r["changed"], true);
    let d = s.active().unwrap().doc.clone();
    let n = &d.notes[0];
    assert_eq!(n.position, [50.0, 60.0]);
    assert_eq!(n.popup, [70.0, 80.0, 310.0, 220.0]);
    assert_eq!(n.modified, before_modified, "moving should not update modified date");
}

#[test]
fn set_note_no_change_returns_changed_false() {
    let mut s = session_with_doc();
    s.execute("notes.add", json!({"x": 10.0, "y": 20.0, "text": "original"})).unwrap();
    let revision_before = s.active().unwrap().revision;
    let r = s.execute("notes.set", json!({"index": 0, "text": "original"})).unwrap();
    assert_eq!(r["changed"], false);
    assert_eq!(s.active().unwrap().revision, revision_before, "no-op edit should not bump revision");
}

#[test]
fn set_note_invalid_index_errors() {
    let mut s = session_with_doc();
    s.execute("notes.add", json!({"x": 1.0, "y": 1.0})).unwrap();
    assert!(s.execute("notes.set", json!({"index": 5, "text": "x"})).is_err());
    assert!(s.execute("notes.set", json!({})).is_err());
}

// Note: NaN/Infinity cannot be represented in JSON; they become null. The library treats
// null coordinates as "not provided" and ignores them. This is arguably fine because the
// param schema says x,y are numbers, and JSON null is not a number. The test for
// rejecting non-finite coordinates is therefore not applicable to notes.set.
// If this is considered a bug, it would need a direct internal test without JSON.

#[test]
fn set_note_partial_position_updates_only_given_coordinate() {
    let mut s = session_with_doc();
    s.execute("notes.add", json!({"x": 10.0, "y": 20.0})).unwrap();
    // Only x changes
    s.execute("notes.set", json!({"index": 0, "x": 100.0})).unwrap();
    let d = s.active().unwrap().doc.clone();
    assert_eq!(d.notes[0].position, [100.0, 20.0]);
    assert_eq!(d.notes[0].popup, [120.0, 40.0, 360.0, 180.0]);
    // Only y changes
    s.execute("notes.set", json!({"index": 0, "y": 200.0})).unwrap();
    let d = s.active().unwrap().doc.clone();
    assert_eq!(d.notes[0].position, [100.0, 200.0]);
    assert_eq!(d.notes[0].popup, [120.0, 220.0, 360.0, 360.0]);
}

// ---------------------------------------------------------------------------
// notes.delete / notes.list
// ---------------------------------------------------------------------------

#[test]
fn delete_note_by_index() {
    let mut s = session_with_doc();
    s.execute("notes.add", json!({"x": 1.0, "y": 1.0})).unwrap();
    s.execute("notes.add", json!({"x": 2.0, "y": 2.0})).unwrap();
    let r = s.execute("notes.delete", json!({"index": 0})).unwrap();
    assert_eq!(r["deleted"], 1);
    assert_eq!(r["remaining"], 1);
    let list = s.execute("notes.list", json!({})).unwrap();
    let notes = list["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["position"], json!([2.0, 2.0]));
}

#[test]
fn delete_all_notes() {
    let mut s = session_with_doc();
    s.execute("notes.add", json!({"x": 1.0, "y": 1.0})).unwrap();
    s.execute("notes.add", json!({"x": 2.0, "y": 2.0})).unwrap();
    let r = s.execute("notes.delete", json!({"all": true})).unwrap();
    assert_eq!(r["deleted"], 2);
    assert_eq!(r["remaining"], 0);
    assert!(s.active().unwrap().doc.notes.is_empty());
}

#[test]
fn delete_all_notes_when_no_notes_is_disabled() {
    let mut s = session_with_doc();
    // The command is disabled when there are no notes (has_notes precondition).
    assert!(!s.is_enabled("notes.delete"));
    let result = s.execute("notes.delete", json!({"all": true}));
    assert!(result.is_err(), "expected command to be disabled when no notes exist");
}

#[test]
fn delete_note_invalid_index_errors() {
    let mut s = session_with_doc();
    s.execute("notes.add", json!({"x": 1.0, "y": 1.0})).unwrap();
    assert!(s.execute("notes.delete", json!({"index": 5})).is_err());
    assert!(s.execute("notes.delete", json!({})).is_err());
}

#[test]
fn list_notes_empty() {
    let mut s = session_with_doc();
    let r = s.execute("notes.list", json!({})).unwrap();
    assert_eq!(r["notes"], json!([]));
}

#[test]
fn list_notes_ordering() {
    let mut s = session_with_doc();
    for i in 0..3 {
        s.execute("notes.add", json!({"x": i as f64, "y": i as f64})).unwrap();
    }
    let r = s.execute("notes.list", json!({})).unwrap();
    let notes = r["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 3);
    for (i, note) in notes.iter().enumerate() {
        assert_eq!(note["index"], i);
        assert_eq!(note["position"], json!([i as f64, i as f64]));
    }
}

// ---------------------------------------------------------------------------
// History (undo/redo)
// ---------------------------------------------------------------------------

#[test]
fn undo_redo_single_note() {
    let mut s = session_with_doc();
    s.execute("notes.add", json!({"x": 10.0, "y": 20.0})).unwrap();
    assert_eq!(s.active().unwrap().doc.notes.len(), 1);
    assert!(s.undo());
    assert!(s.active().unwrap().doc.notes.is_empty());
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc.notes.len(), 1);
}

// ---------------------------------------------------------------------------
// Enabled predicates
// ---------------------------------------------------------------------------

#[test]
fn enabled_predicates_reflect_document_and_notes() {
    let mut s = Session::new();
    // No document
    assert!(!s.is_enabled("notes.add"));
    assert!(!s.is_enabled("notes.set"));
    assert!(!s.is_enabled("notes.delete"));
    assert!(!s.is_enabled("notes.list"));
    assert!(!s.is_enabled("file.import.notes"));

    // Document, but no notes
    s.execute("file.new", json!({"width": 100, "height": 80})).unwrap();
    assert!(s.is_enabled("notes.add"));
    assert!(s.is_enabled("notes.list"));
    assert!(!s.is_enabled("notes.set"));
    assert!(!s.is_enabled("notes.delete"));
    assert!(s.is_enabled("file.import.notes")); // only requires a document

    // After adding a note
    s.execute("notes.add", json!({"x": 1.0, "y": 1.0})).unwrap();
    assert!(s.is_enabled("notes.set"));
    assert!(s.is_enabled("notes.delete"));
}

// ---------------------------------------------------------------------------
// import_notes_from (public helper) error path
// ---------------------------------------------------------------------------

#[test]
fn import_notes_from_invalid_bytes_errors() {
    let mut s = session_with_doc();
    let invalid = [0u8, 1, 2, 3, 4]; // not a valid PSD/pcraft
    let result = notes_cmds::import_notes_from(&mut s, "bad.psd", &invalid);
    assert!(result.is_err(), "expected error for invalid import bytes");
}

// ---------------------------------------------------------------------------
// file.import.notes error path (nonexistent file)
// ---------------------------------------------------------------------------

#[test]
fn import_notes_nonexistent_path_errors() {
    let mut s = session_with_doc();
    let result = s.execute("file.import.notes", json!({"path": "/nonexistent/path/to/file.psd"}));
    assert!(result.is_err(), "expected error for missing file");
}
