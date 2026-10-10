use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use photocraft_plugins::manifest::{MAX_MANIFEST_BYTES, MAX_OVERLAP, MAX_PARAMS, valid_id};
use photocraft_plugins::{Area, Error, Kind, Manifest};
use serde_json::{Value, json};

const FULL: &str = r#"{"id":"org.example.tone","name":"Tone","version":"1.2","kind":"filter",
    "params":{"amount":{"type":"number","min":0,"max":100,"default":50},
              "mode":{"type":"choice","options":["soft","hard"],"default":"hard"},
              "mono":{"type":"bool"},"seed":{"type":"int","min":0,"max":9}},
    "overlap":2,"area":"canvas"}"#;

fn parse_ok(s: &str) -> std::result::Result<Manifest, Error> {
    Manifest::parse(s.as_bytes())
}

fn manifest_json(params: serde_json::Map<String, Value>) -> Value {
    let mut root = serde_json::Map::new();
    root.insert("id".into(), json!("a"));
    root.insert("name".into(), json!("x"));
    root.insert("kind".into(), json!("filter"));
    root.insert("params".into(), Value::Object(params));
    Value::Object(root)
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let path = std::env::temp_dir().join(format!("pc-plugin-manifest-test-{}-{}-{}", tag, std::process::id(), nanos));
        std::fs::create_dir_all(&path)?;
        Ok(TempDir(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn valid_id_accepts_allowed_characters_and_lengths() {
    assert!(valid_id("a"));
    assert!(valid_id("A0._-"));
    assert!(valid_id("."));
    assert!(valid_id(&"x".repeat(64)));

    assert!(!valid_id(""));
    assert!(!valid_id(&"x".repeat(65)));
    assert!(!valid_id("a b"));
    assert!(!valid_id("a/b"));
    assert!(!valid_id("é"));
    assert!(!valid_id("a,b"));
}

#[test]
fn parse_fills_defaults_for_minimal_manifest() -> Result<(), Box<dyn std::error::Error>> {
    let m = parse_ok(r#"{"id":"x","name":"X","kind":"filter"}"#)?;
    assert_eq!(m.kind, Kind::Filter);
    assert_eq!(m.version, "");
    assert_eq!(m.description, "");
    assert_eq!(m.author, "");
    assert!(m.params.is_empty());
    assert_eq!(m.overlap, 0);
    assert_eq!(m.area, Area::Content);
    Ok(())
}

#[test]
fn parse_rejects_non_utf8_empty_and_too_large() {
    assert!(Manifest::parse(b"").is_err());
    assert!(Manifest::parse(&[0xff, 0xfe]).is_err());
    assert!(Manifest::parse(&vec![b' '; MAX_MANIFEST_BYTES + 1]).is_err());
}

#[test]
fn parse_rejects_bad_id_name_or_kind() {
    let bad = [
        r#"{"name":"x","kind":"filter"}"#,
        r#"{"id":"","name":"x","kind":"filter"}"#,
        r#"{"id":"a b","name":"x","kind":"filter"}"#,
        r#"{"id":"a","name":"","kind":"filter"}"#,
        r#"{"id":"a","name":"x","kind":"panel"}"#,
        r#"{"id":"a","name":"x"}"#,
    ];
    for s in bad {
        assert!(Manifest::parse(s.as_bytes()).is_err(), "{s}");
    }
}

#[test]
fn parse_rejects_invalid_id_and_name_length_and_controls() {
    let long_id = format!(r#"{{"id":"{}","name":"x","kind":"filter"}}"#, "a".repeat(65));
    assert!(Manifest::parse(long_id.as_bytes()).is_err());

    let long_name = format!(r#"{{"id":"a","name":"{}","kind":"filter"}}"#, "x".repeat(65));
    assert!(Manifest::parse(long_name.as_bytes()).is_err());

    let control_name = r#"{"id":"a","name":"x\u0000y","kind":"filter"}"#;
    assert!(Manifest::parse(control_name.as_bytes()).is_err());
}

#[test]
fn parse_enforces_version_description_author_length_limits() {
    let make = |v: &str, d: &str, a: &str| format!(r#"{{"id":"a","name":"x","kind":"filter","version":"{v}","description":"{d}","author":"{a}"}}"#);

    assert!(Manifest::parse(make("", "", "").as_bytes()).is_ok());
    assert!(Manifest::parse(make(&"v".repeat(32), "", "").as_bytes()).is_ok());
    assert!(Manifest::parse(make(&"v".repeat(33), "", "").as_bytes()).is_err());

    assert!(Manifest::parse(make("", &"d".repeat(1024), "").as_bytes()).is_ok());
    assert!(Manifest::parse(make("", &"d".repeat(1025), "").as_bytes()).is_err());

    assert!(Manifest::parse(make("", "", &"a".repeat(128)).as_bytes()).is_ok());
    assert!(Manifest::parse(make("", "", &"a".repeat(129)).as_bytes()).is_err());
}

#[test]
fn parse_overlap_boundary() {
    let make = |overlap: u32| format!(r#"{{"id":"a","name":"x","kind":"filter","overlap":{overlap}}}"#);

    assert!(Manifest::parse(make(MAX_OVERLAP).as_bytes()).is_ok());
    assert!(Manifest::parse(make(MAX_OVERLAP + 1).as_bytes()).is_err());
}

#[test]
fn parse_rejects_invalid_number_params() {
    for bad in [
        r#"{"id":"a","name":"x","kind":"filter","params":{"r":{"type":"number","min":5,"max":1,"default":1}}}"#,
        r#"{"id":"a","name":"x","kind":"filter","params":{"r":{"type":"number","min":1e999,"max":0,"default":0}}}"#,
        r#"{"id":"a","name":"x","kind":"filter","params":{"r":{"type":"number","min":0,"max":1,"default":NaN}}}"#,
        r#"{"id":"a","name":"x","kind":"filter","params":{"r":{"type":"number","min":"0","max":1,"default":0}}}"#,
    ] {
        assert!(Manifest::parse(bad.as_bytes()).is_err(), "{bad}");
    }
}

#[test]
fn parse_rejects_invalid_int_param_range() {
    let bad = r#"{"id":"a","name":"x","kind":"filter","params":{"n":{"type":"int","min":5,"max":1,"default":0}}}"#;
    assert!(Manifest::parse(bad.as_bytes()).is_err());
}

#[test]
fn parse_rejects_invalid_choice_params() {
    for bad in [
        r#"{"id":"a","name":"x","kind":"filter","params":{"c":{"type":"choice","options":[]}}}"#,
        r#"{"id":"a","name":"x","kind":"filter","params":{"c":{"type":"choice","options":["a|b"]}}}"#,
        r#"{"id":"a","name":"x","kind":"filter","params":{"c":{"type":"choice","options":["a\"b"]}}}"#,
        r#"{"id":"a","name":"x","kind":"filter","params":{"c":{"type":"choice","options":["a"],"default":"b"}}}"#,
    ] {
        assert!(Manifest::parse(bad.as_bytes()).is_err(), "{bad}");
    }
}

#[test]
fn parse_choice_options_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let mut params = serde_json::Map::new();
    let mut options = Vec::new();
    for i in 0..64 {
        options.push(json!(format!("o{i}")));
    }
    params.insert("c".to_string(), json!({"type":"choice","options": options}));
    let good = serde_json::to_string(&manifest_json(params))?;
    let m = parse_ok(&good)?;
    assert_eq!(m.params.len(), 1);

    let mut params = serde_json::Map::new();
    let mut options = Vec::new();
    for i in 0..65 {
        options.push(json!(format!("o{i}")));
    }
    params.insert("c".to_string(), json!({"type":"choice","options": options}));
    let bad = serde_json::to_string(&manifest_json(params))?;
    assert!(Manifest::parse(bad.as_bytes()).is_err());
    Ok(())
}

#[test]
fn parse_enforces_max_params() -> Result<(), Box<dyn std::error::Error>> {
    let mut params = serde_json::Map::new();
    for i in 0..MAX_PARAMS {
        params.insert(format!("p{i}"), json!({"type":"bool"}));
    }
    let good = serde_json::to_string(&manifest_json(params))?;
    let m = parse_ok(&good)?;
    assert_eq!(m.params.len(), MAX_PARAMS);

    let mut params = serde_json::Map::new();
    for i in 0..=MAX_PARAMS {
        params.insert(format!("p{i}"), json!({"type":"bool"}));
    }
    let bad = serde_json::to_string(&manifest_json(params))?;
    assert!(Manifest::parse(bad.as_bytes()).is_err());
    Ok(())
}

#[test]
fn parse_rejects_invalid_param_names() -> Result<(), Box<dyn std::error::Error>> {
    for bad in [
        r#"{"id":"a","name":"x","kind":"filter","params":{"_x":{"type":"bool"}}}"#,
        r#"{"id":"a","name":"x","kind":"filter","params":{"__x":{"type":"bool"}}}"#,
        r#"{"id":"a","name":"x","kind":"filter","params":{"id":{"type":"bool"}}}"#,
        r#"{"id":"a","name":"x","kind":"filter","params":{"layer":{"type":"bool"}}}"#,
        r#"{"id":"a","name":"x","kind":"filter","params":{"a-b":{"type":"bool"}}}"#,
        r#"{"id":"a","name":"x","kind":"filter","params":{"":{"type":"bool"}}}"#,
    ] {
        assert!(Manifest::parse(bad.as_bytes()).is_err(), "{bad}");
    }

    let mut params = serde_json::Map::new();
    params.insert("k".repeat(65), json!({"type":"bool"}));
    let bad = serde_json::to_string(&manifest_json(params))?;
    assert!(Manifest::parse(bad.as_bytes()).is_err());
    Ok(())
}

#[test]
fn parse_keeps_param_order_and_notation_formats_all_specs() -> Result<(), Box<dyn std::error::Error>> {
    let m = parse_ok(FULL)?;
    assert_eq!(m.id, "org.example.tone");
    assert_eq!(m.params.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(), vec!["amount", "mode", "mono", "seed"]);
    assert_eq!(m.overlap, 2);
    assert_eq!(m.area, Area::Canvas);
    assert_eq!(m.params_notation(), r#"{"amount":0..100=50,"mode":"hard|soft","mono":bool=false,"seed":int=0}"#);
    Ok(())
}

#[test]
fn params_notation_uses_first_choice_when_no_default_and_empty_params() -> Result<(), Box<dyn std::error::Error>> {
    let m = parse_ok(r#"{"id":"a","name":"x","kind":"filter","params":{"c":{"type":"choice","options":["beta","alpha"]}}}"#)?;
    assert_eq!(m.params_notation(), r#"{"c":"beta|alpha"}"#);

    let empty = parse_ok(r#"{"id":"a","name":"x","kind":"filter"}"#)?;
    assert_eq!(empty.params_notation(), "{}");
    Ok(())
}

#[test]
fn resolve_params_fills_defaults_and_ignores_unknown_and_null() -> Result<(), Box<dyn std::error::Error>> {
    let m = parse_ok(FULL)?;
    let p = m.resolve_params(&json!({"junk":1,"amount":null,"mode":null,"seed":3,"extra":true}))?;
    assert_eq!(Value::Object(p), json!({"amount":50.0,"mode":"hard","mono":false,"seed":3}));
    Ok(())
}

#[test]
fn resolve_params_clamps_numbers_and_rounds_ints() -> Result<(), Box<dyn std::error::Error>> {
    let m = parse_ok(FULL)?;
    let p = m.resolve_params(&json!({"amount": -10, "seed": 3.6}))?;
    assert_eq!(Value::Object(p), json!({"amount":0.0,"mode":"hard","mono":false,"seed":4}));

    let p = m.resolve_params(&json!({"amount": 150, "seed": -100}))?;
    assert_eq!(Value::Object(p), json!({"amount":100.0,"mode":"hard","mono":false,"seed":0}));
    Ok(())
}

#[test]
fn resolve_params_rejects_wrong_types_and_non_objects() -> Result<(), Box<dyn std::error::Error>> {
    let m = parse_ok(FULL)?;
    assert!(m.resolve_params(&json!({"amount":"lots"})).is_err());
    assert!(m.resolve_params(&json!({"mode":"medium"})).is_err());
    assert!(m.resolve_params(&json!({"mono":1})).is_err());
    assert!(m.resolve_params(&json!({"seed":"many"})).is_err());
    assert!(m.resolve_params(&json!([1, 2])).is_err());
    assert!(m.resolve_params(&json!("x")).is_err());
    assert!(m.resolve_params(&Value::Null).is_ok());
    Ok(())
}

#[test]
fn resolve_params_choice_uses_default_then_first_option() -> Result<(), Box<dyn std::error::Error>> {
    let with_default = parse_ok(r#"{"id":"a","name":"x","kind":"filter","params":{"c":{"type":"choice","options":["a","b"],"default":"b"}}}"#)?;
    assert_eq!(Value::Object(with_default.resolve_params(&Value::Null)?), json!({"c":"b"}));

    let no_default = parse_ok(r#"{"id":"a","name":"x","kind":"filter","params":{"c":{"type":"choice","options":["a","b"]}}}"#)?;
    assert_eq!(Value::Object(no_default.resolve_params(&Value::Null)?), json!({"c":"a"}));
    Ok(())
}

#[test]
fn resolve_params_int_uses_default_min_max_when_omitted() -> Result<(), Box<dyn std::error::Error>> {
    let m = parse_ok(r#"{"id":"a","name":"x","kind":"filter","params":{"n":{"type":"int"}}}"#)?;
    assert_eq!(Value::Object(m.resolve_params(&Value::Null)?), json!({"n":0}));
    assert_eq!(Value::Object(m.resolve_params(&json!({"n": 2_000_000}))?), json!({"n":1_000_000}));
    assert_eq!(Value::Object(m.resolve_params(&json!({"n": -2_000_000}))?), json!({"n":-1_000_000}));
    Ok(())
}

#[test]
fn manifest_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let m = parse_ok(FULL)?;
    let bytes = serde_json::to_vec(&m)?;
    let de: Manifest = serde_json::from_slice(&bytes)?;
    assert_eq!(m, de);
    Ok(())
}

#[test]
fn manifest_write_read_roundtrip_to_temp_dir() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new("write-read")?;
    let file = dir.path().join("manifest.json");
    let m = parse_ok(FULL)?;
    let json = serde_json::to_vec_pretty(&m)?;
    std::fs::write(&file, json)?;
    let bytes = std::fs::read(&file)?;
    let m2 = Manifest::parse(&bytes)?;
    assert_eq!(m, m2);
    Ok(())
}

#[test]
fn params_notation_is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    let m = parse_ok(FULL)?;
    assert_eq!(m.params_notation(), m.params_notation());
    assert_eq!(m.resolve_params(&Value::Null)?, m.resolve_params(&Value::Null)?);
    Ok(())
}
