use lightcraft_engine::smart_sort::store::{Store, normalized};
use std::path::{Path, PathBuf};

fn temp_library_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("lightcraft-smart-sort-store-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn write_cache_file(dir: &Path, model: &str, content: &[u8]) -> PathBuf {
    let ai = dir.join("AI");
    std::fs::create_dir_all(&ai).unwrap();
    let file = ai.join(format!("embeddings-{model}.bin"));
    std::fs::write(&file, content).unwrap();
    file
}

fn assert_slice_approx_eq(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len(), "length mismatch");
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 1e-6, "expected {e}, got {a}");
    }
}

fn header_line(model: &str, dim: usize) -> String {
    format!("LIEMB1 {{\"model\":\"{model}\",\"dim\":{dim}}}\n")
}

#[test]
fn normalized_accepts_and_normalizes() {
    let v = normalized(vec![3.0, 4.0], 2).unwrap();
    assert_slice_approx_eq(&v, &[0.6, 0.8]);

    let u = normalized(vec![0.0, -2.0, 0.0], 3).unwrap();
    assert_slice_approx_eq(&u, &[0.0, -1.0, 0.0]);
}

#[test]
fn normalized_rejects_dimension_mismatch() {
    assert!(normalized(vec![1.0], 2).is_err());
    assert!(normalized(vec![1.0, 2.0], 1).is_err());
}

#[test]
fn normalized_rejects_non_finite_or_zero_norm() {
    assert!(normalized(vec![0.0, 0.0], 2).is_err());
    assert!(normalized(vec![f32::NAN, 1.0], 2).is_err());
    assert!(normalized(vec![f32::INFINITY, 0.0], 2).is_err());
    assert!(normalized(vec![f32::NEG_INFINITY, 0.0], 2).is_err());
    assert!(normalized(vec![0.0], 1).is_err());
}

#[test]
fn ensure_rejects_invalid_model_id_or_dimension() {
    let mut s = Store::new(None);
    assert!(s.ensure("", 1).is_err());
    assert!(s.ensure("bad model", 1).is_err());
    assert!(s.ensure(&"a".repeat(129), 1).is_err());
    assert!(s.ensure("model", 0).is_err());
    assert!(s.ensure("model", 16385).is_err());
}

#[test]
fn ensure_allows_boundary_model_and_dimension() {
    let mut s = Store::new(None);
    let model = "a".repeat(128);
    assert!(s.ensure(&model, 1).is_ok());
    assert!(s.ensure("m", 16384).is_ok());
}

#[test]
fn ensure_dimension_change_is_rejected() {
    let mut s = Store::new(None);
    assert!(s.ensure("m", 4).is_ok());
    assert!(s.ensure("m", 8).is_err());
}

#[test]
fn insert_requires_loaded_model() {
    let mut s = Store::new(None);
    assert!(s.insert("m", "k".into(), vec![1.0]).is_err());
}

#[test]
fn insert_rejects_empty_key() {
    let mut s = Store::new(None);
    s.ensure("m", 1).unwrap();
    assert!(s.insert("m", String::new(), vec![1.0]).is_err());
}

#[test]
fn insert_key_length_boundaries() {
    let mut s = Store::new(None);
    s.ensure("m", 1).unwrap();

    let max_key = "a".repeat(u16::MAX as usize);
    assert!(s.insert("m", max_key.clone(), vec![1.0]).is_ok());
    assert_eq!(s.len("m"), 1);
    assert_slice_approx_eq(s.get("m", &max_key).unwrap(), &[1.0]);

    let too_long = "a".repeat(u16::MAX as usize + 1);
    assert!(s.insert("m", too_long, vec![1.0]).is_err());
}

#[test]
fn insert_normalizes_values() {
    let mut s = Store::new(None);
    s.ensure("m", 2).unwrap();
    s.insert("m", "k".into(), vec![3.0, 4.0]).unwrap();

    let got = s.get("m", "k").unwrap();
    assert_slice_approx_eq(got, &[0.6, 0.8]);
    assert_eq!(s.len("m"), 1);
}

#[test]
fn sharpness_filters_non_finite_values() {
    let mut s = Store::new(None);
    s.set_sharpness("a".into(), f32::NAN);
    s.set_sharpness("b".into(), f32::INFINITY);
    s.set_sharpness("c".into(), -1.0);

    assert_eq!(s.sharpness("a"), None);
    assert_eq!(s.sharpness("b"), None);
    assert_eq!(s.sharpness("c"), Some(-1.0));
}

#[test]
fn truncated_cache_file_is_discarded() {
    let dir = temp_library_dir("truncated");
    let mut content = header_line("m", 2).into_bytes();
    content.push(7); // first byte of a u16 key length, second byte missing
    let _ = write_cache_file(&dir, "m", &content);

    let mut s = Store::new(Some(dir.as_path()));
    assert!(s.ensure("m", 2).is_ok());
    assert_eq!(s.len("m"), 0);
    assert!(s.get("m", "k").is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cache_with_non_finite_float_is_discarded() {
    let dir = temp_library_dir("nonfinite");
    let mut content = header_line("m", 2).into_bytes();
    content.extend_from_slice(&1u16.to_le_bytes());
    content.push(b'a');
    content.extend_from_slice(&0.0f32.to_le_bytes());
    content.extend_from_slice(&f32::NAN.to_le_bytes());
    let _ = write_cache_file(&dir, "m", &content);

    let mut s = Store::new(Some(dir.as_path()));
    assert!(s.ensure("m", 2).is_ok());
    assert_eq!(s.len("m"), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_appends_records_without_losing_existing_ones() {
    let dir = temp_library_dir("append");
    let mut s = Store::new(Some(dir.as_path()));
    s.ensure("m", 2).unwrap();
    s.insert("m", "first".into(), vec![1.0, 0.0]).unwrap();
    s.save().unwrap();
    drop(s);

    let mut s2 = Store::new(Some(dir.as_path()));
    s2.ensure("m", 2).unwrap();
    s2.insert("m", "second".into(), vec![0.0, 1.0]).unwrap();
    s2.save().unwrap();
    drop(s2);

    let mut s3 = Store::new(Some(dir.as_path()));
    s3.ensure("m", 2).unwrap();
    assert_eq!(s3.len("m"), 2);
    assert_slice_approx_eq(s3.get("m", "first").unwrap(), &[1.0, 0.0]);
    assert_slice_approx_eq(s3.get("m", "second").unwrap(), &[0.0, 1.0]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn insert_same_key_keeps_last_value_and_saves_once() {
    let dir = temp_library_dir("same-key");
    let mut s = Store::new(Some(dir.as_path()));
    s.ensure("m", 2).unwrap();
    s.insert("m", "k".into(), vec![1.0, 0.0]).unwrap();
    s.insert("m", "k".into(), vec![0.0, 1.0]).unwrap();
    assert_slice_approx_eq(s.get("m", "k").unwrap(), &[0.0, 1.0]);
    s.save().unwrap();
    drop(s);

    let mut s2 = Store::new(Some(dir.as_path()));
    s2.ensure("m", 2).unwrap();
    assert_eq!(s2.len("m"), 1);
    assert_slice_approx_eq(s2.get("m", "k").unwrap(), &[0.0, 1.0]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_without_pending_or_dirty_does_not_create_dir() {
    let dir = temp_library_dir("noop");
    let mut s = Store::new(Some(dir.as_path()));
    s.ensure("m", 2).unwrap();
    s.save().unwrap();
    drop(s);

    assert!(!dir.join("AI").exists());

    let _ = std::fs::remove_dir_all(&dir);
}
