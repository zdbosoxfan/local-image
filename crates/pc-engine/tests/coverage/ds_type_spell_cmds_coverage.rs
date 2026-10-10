use photocraft_engine::Session;
use photocraft_engine::doc::{LayerContent, LayerId};
use photocraft_engine::type_spell_cmds::specs;
use serde_json::json;

fn session_with_text(text: &str) -> (Session, u64) {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 320, "height": 100, "background": "white"})).unwrap();
    let r = s.execute("type.create", json!({"x": 10, "y": 50, "text": text, "size": 12})).unwrap();
    let layer = r["layer"].as_u64().unwrap();
    (s, layer)
}

fn session_with_two_layers(t1: &str, t2: &str) -> (Session, u64, u64) {
    let (mut s, id1) = session_with_text(t1);
    let r = s.execute("type.create", json!({"x": 10, "y": 50, "text": t2, "size": 12})).unwrap();
    let id2 = r["layer"].as_u64().unwrap();
    (s, id1, id2)
}

fn text_of(s: &Session, layer: u64) -> String {
    let st = s.active().expect("active document");
    let layer_ref = st.doc.layer(LayerId(layer)).expect("layer exists");
    match &layer_ref.content {
        LayerContent::Text(t) => t.text.clone(),
        _ => panic!("layer {} is not a text layer", layer),
    }
}

#[test]
fn specs_exposes_the_two_public_commands() {
    let specs = specs();
    let ids: Vec<_> = specs.iter().map(|s| s.id).collect();
    assert_eq!(ids, vec!["edit.checkSpelling", "type.insertText"]);
}

#[test]
fn list_empty_text_returns_zero_misspellings() {
    let (mut s, _) = session_with_text("");
    let r = s.execute("edit.checkSpelling", json!({})).unwrap();
    assert_eq!(r["count"], 0);
    assert_eq!(r["misspellings"].as_array().unwrap().len(), 0);
}

#[test]
fn list_flags_misspellings_with_char_offsets_and_suggestions() {
    let (mut s, id) = session_with_text("Teh quick brwon fox, teh end. Photocraft");
    let r = s.execute("edit.checkSpelling", json!({})).unwrap();
    let miss = r["misspellings"].as_array().unwrap();
    let words: Vec<&str> = miss.iter().map(|m| m["word"].as_str().unwrap()).collect();
    assert_eq!(words, vec!["Teh", "brwon", "teh", "Photocraft"]);
    assert_eq!(miss[0]["start"], 0);
    assert_eq!(miss[0]["end"], 3);
    assert_eq!(miss[0]["layer"], id);
    assert!(miss[0]["suggestions"].as_array().unwrap().iter().any(|v| v.as_str() == Some("The")));
}

#[test]
fn list_respects_layer_scoping_and_ignore_words() {
    let (mut s, id1, id2) = session_with_two_layers("teh one", "brwon two");
    s.select_layer(LayerId(id1)).unwrap();

    let all = s.execute("edit.checkSpelling", json!({})).unwrap();
    assert_eq!(all["count"], 2);

    let active = s.execute("edit.checkSpelling", json!({"allLayers": false})).unwrap();
    assert_eq!(active["count"], 1);
    assert_eq!(active["misspellings"][0]["layer"], id1);
    assert_eq!(active["misspellings"][0]["word"], "teh");

    let layer2 = s.execute("edit.checkSpelling", json!({"layer": id2})).unwrap();
    assert_eq!(layer2["count"], 1);
    assert_eq!(layer2["misspellings"][0]["word"], "brwon");

    let ignored = s.execute("edit.checkSpelling", json!({"ignore": ["brwon"]})).unwrap();
    assert_eq!(ignored["count"], 1);
    assert_eq!(ignored["misspellings"][0]["word"], "teh");
}

#[test]
fn change_one_occurrence_verifies_word_and_rejects_stale_word() {
    let (mut s, id) = session_with_text("Teh teh");
    let r = s.execute("edit.checkSpelling", json!({"action": "change", "layer": id, "start": 0, "end": 3, "word": "Teh", "replace": "The"})).unwrap();
    assert_eq!(r["changed"], 1);
    assert_eq!(text_of(&s, id), "The teh");

    let err = s.execute("edit.checkSpelling", json!({"action": "change", "layer": id, "start": 0, "end": 3, "word": "Teh", "replace": "X"}));
    assert!(err.is_err());
}

#[test]
fn change_all_replaces_case_sensitive_whole_words() {
    let (mut s, id) = session_with_text("Teh teh teh");
    let r = s.execute("edit.checkSpelling", json!({"action": "changeAll", "word": "teh", "replace": "the"})).unwrap();
    assert_eq!(r["changed"], 2);
    assert_eq!(text_of(&s, id), "Teh the the");
}

#[test]
fn change_all_multiple_layers_and_undo_redo() {
    let (mut s, id1, id2) = session_with_two_layers("teh one", "teh two");
    let r = s.execute("edit.checkSpelling", json!({"action": "changeAll", "word": "teh", "replace": "the"})).unwrap();
    assert_eq!(r["changed"], 2);
    assert_eq!(text_of(&s, id1), "the one");
    assert_eq!(text_of(&s, id2), "the two");

    assert!(s.undo());
    assert_eq!(text_of(&s, id1), "teh one");
    assert_eq!(text_of(&s, id2), "teh two");

    assert!(s.redo());
    assert_eq!(text_of(&s, id1), "the one");
    assert_eq!(text_of(&s, id2), "the two");
}

#[test]
fn add_remove_dictionary_words_are_case_insensitive_and_sorted() {
    let (mut s, _) = session_with_text("irrelevant");

    let r = s.execute("edit.checkSpelling", json!({"action": "addToDictionary", "word": "Photocraft"})).unwrap();
    assert_eq!(r["added"], true);
    assert_eq!(r["userDictionary"].as_array().unwrap().len(), 1);

    let r = s.execute("edit.checkSpelling", json!({"action": "addToDictionary", "word": "photocraft"})).unwrap();
    assert_eq!(r["added"], false);
    assert_eq!(r["userDictionary"].as_array().unwrap().len(), 1);

    s.execute("edit.checkSpelling", json!({"action": "addToDictionary", "word": "alpha"})).unwrap();
    let r = s.execute("edit.checkSpelling", json!({"action": "addToDictionary", "word": "Beta"})).unwrap();
    let dict: Vec<&str> = r["userDictionary"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(dict, vec!["alpha", "Beta", "Photocraft"]);

    let r = s.execute("edit.checkSpelling", json!({"action": "removeFromDictionary", "word": "PHOTOCRAFT"})).unwrap();
    assert_eq!(r["removed"], true);
    let dict: Vec<&str> = r["userDictionary"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(dict, vec!["alpha", "Beta"]);

    let r = s.execute("edit.checkSpelling", json!({"action": "removeFromDictionary", "word": "PHOTOCRAFT"})).unwrap();
    assert_eq!(r["removed"], false);

    assert!(s.execute("edit.checkSpelling", json!({"action": "addToDictionary", "word": "two words"})).is_err());
    assert!(s.execute("edit.checkSpelling", json!({"action": "addToDictionary", "word": "  "})).is_err());
}

#[test]
fn suggest_reports_correct_and_honors_dictionary_and_ignore() {
    let (mut s, _) = session_with_text("the");

    let r = s.execute("edit.checkSpelling", json!({"action": "suggest", "word": "the"})).unwrap();
    assert_eq!(r["correct"], true);

    let r = s.execute("edit.checkSpelling", json!({"action": "suggest", "word": "teh"})).unwrap();
    assert_eq!(r["correct"], false);
    assert!(r["suggestions"].as_array().unwrap().iter().any(|v| v.as_str() == Some("the")));

    let r = s.execute("edit.checkSpelling", json!({"action": "suggest", "word": "photocraft"})).unwrap();
    assert_eq!(r["correct"], false);

    let r = s.execute("edit.checkSpelling", json!({"action": "suggest", "word": "photocraft", "ignore": ["photocraft"]})).unwrap();
    assert_eq!(r["correct"], true);

    s.execute("edit.checkSpelling", json!({"action": "addToDictionary", "word": "photocraft"})).unwrap();
    let r = s.execute("edit.checkSpelling", json!({"action": "suggest", "word": "photocraft"})).unwrap();
    assert_eq!(r["correct"], true);
}

#[test]
fn insert_glyph_multibyte_and_undo_redo() {
    let (mut s, id) = session_with_text("ab");
    let r = s.execute("type.insertText", json!({"layer": id, "text": "→", "at": 1})).unwrap();
    assert_eq!(text_of(&s, id), "a→b");
    assert_eq!(r["caret"], 2);

    assert!(s.undo());
    assert_eq!(text_of(&s, id), "ab");

    assert!(s.redo());
    assert_eq!(text_of(&s, id), "a→b");
}

#[test]
fn insert_glyph_over_range_replaces_selection() {
    let (mut s, id) = session_with_text("ab");
    let r = s.execute("type.insertText", json!({"layer": id, "text": "€", "range": [0, 2]})).unwrap();
    assert_eq!(text_of(&s, id), "€");
    assert_eq!(r["caret"], 1);
}

#[test]
fn insert_text_default_at_end_and_rejects_empty() {
    let (mut s, id) = session_with_text("ab");
    let r = s.execute("type.insertText", json!({"layer": id, "text": "!"})).unwrap();
    assert_eq!(text_of(&s, id), "ab!");
    assert_eq!(r["caret"], 3);

    assert!(s.execute("type.insertText", json!({"layer": id, "text": ""})).is_err());
    assert!(s.execute("type.insertText", json!({"layer": id})).is_err());
}

#[test]
fn insert_text_errors_on_non_type_layer_or_missing_document() {
    let mut s = Session::new();
    assert!(s.execute("type.insertText", json!({"text": "x"})).is_err());

    s.execute("file.new", json!({"width": 10, "height": 10})).unwrap();
    assert!(!s.is_enabled("type.insertText"));
    assert!(s.execute("type.insertText", json!({"text": "x"})).is_err());

    let r = s.execute("type.create", json!({"x": 1, "y": 1, "text": "t", "size": 12})).unwrap();
    let _type_id = r["layer"].as_u64().unwrap();
    assert!(s.is_enabled("type.insertText"));
    assert!(s.execute("type.insertText", json!({"layer": 999, "text": "x"})).is_err());
}

#[test]
fn list_errors_with_no_document() {
    let mut s = Session::new();
    assert!(s.execute("edit.checkSpelling", json!({})).is_err());
}

#[test]
fn change_missing_required_params_returns_error() {
    let (mut s, id) = session_with_text("teh");
    assert!(s.execute("edit.checkSpelling", json!({"action": "change", "layer": id, "start": 0, "end": 3})).is_err());
    assert!(s.execute("edit.checkSpelling", json!({"action": "change", "layer": id, "start": 0, "replace": "x"})).is_err());
    assert!(s.execute("edit.checkSpelling", json!({"action": "change", "layer": id, "end": 3, "replace": "x"})).is_err());
    assert!(s.execute("edit.checkSpelling", json!({"action": "change", "start": 0, "end": 3, "replace": "x"})).is_err());
}

#[test]
fn change_all_rejects_missing_or_empty_word() {
    let (mut s, _) = session_with_text("teh");
    assert!(s.execute("edit.checkSpelling", json!({"action": "changeAll", "replace": "the"})).is_err());
    assert!(s.execute("edit.checkSpelling", json!({"action": "changeAll", "word": "", "replace": "the"})).is_err());
}

#[test]
fn list_and_suggest_respect_suggestion_limit_zero() {
    let (mut s, _) = session_with_text("teh");

    let r = s.execute("edit.checkSpelling", json!({"suggestions": 0})).unwrap();
    assert_eq!(r["count"], 1);
    assert_eq!(r["misspellings"][0]["suggestions"].as_array().unwrap().len(), 0);

    let r = s.execute("edit.checkSpelling", json!({"action": "suggest", "word": "teh", "suggestions": 0})).unwrap();
    assert_eq!(r["suggestions"].as_array().unwrap().len(), 0);
}

#[test]
fn spell_results_are_deterministic() {
    let (mut s, _) = session_with_text("teh brwon teh");

    let list1 = s.execute("edit.checkSpelling", json!({})).unwrap();
    let list2 = s.execute("edit.checkSpelling", json!({})).unwrap();
    assert_eq!(list1, list2);

    let sugg1 = s.execute("edit.checkSpelling", json!({"action": "suggest", "word": "teh"})).unwrap();
    let sugg2 = s.execute("edit.checkSpelling", json!({"action": "suggest", "word": "teh"})).unwrap();
    assert_eq!(sugg1, sugg2);
}

#[test]
fn change_all_no_matches_does_not_touch_revision() {
    let (mut s, id) = session_with_text("hello");
    let rev_before = s.active().unwrap().revision;

    let r = s.execute("edit.checkSpelling", json!({"action": "changeAll", "word": "zzzz", "replace": "x"})).unwrap();
    assert_eq!(r["changed"], 0);
    assert_eq!(text_of(&s, id), "hello");
    assert_eq!(s.active().unwrap().revision, rev_before);
}

#[test]
fn insert_text_out_of_bounds_caret_clamps_to_end() {
    let (mut s, id) = session_with_text("ab");
    let r = s.execute("type.insertText", json!({"layer": id, "text": "Z", "at": 999})).unwrap();
    assert_eq!(text_of(&s, id), "abZ");
    assert_eq!(r["caret"], 3);
}

#[test]
fn insert_text_range_start_greater_than_end_swaps() {
    let (mut s, id) = session_with_text("abcd");
    let r = s.execute("type.insertText", json!({"layer": id, "text": "X", "range": [3, 1]})).unwrap();
    assert_eq!(text_of(&s, id), "aXd");
    assert_eq!(r["caret"], 2);
}
