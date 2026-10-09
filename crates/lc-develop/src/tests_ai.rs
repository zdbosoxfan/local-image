//! local-image: AI spots and the AI Denoise reference in the settings — older settings load and
//! serialize unchanged, new ones round-trip, and copies to other photos leave them behind.

use serde_json::json;

use crate::{AiKey, AiPatch, DenoiseRef, DevelopSettings, SettingsGroup, Spot, SpotMode, extract_groups};

fn old_json() -> serde_json::Value {
    let mut v = DevelopSettings::default().to_json();
    v["spots"] = json!([{"mode": "heal", "points": [{"x": 0.2, "y": 0.3}], "size": 0.02, "feather": 50.0, "opacity": 100.0, "source_offset": {"x": 0.05, "y": 0.0}}]);
    v["enhance"] = json!({"denoise": 0.0, "raw_details": false, "super_resolution": false});
    v
}

#[test]
fn older_settings_load_and_serialize_unchanged() {
    let v = old_json();
    let s = DevelopSettings::from_json(&v).unwrap();
    assert_eq!(s.spots[0].mode, SpotMode::Heal);
    assert!(s.spots[0].polygon.is_empty() && s.spots[0].mask.is_none() && s.spots[0].patch.is_none());
    assert!(s.enhance.ai.is_none());
    // the same JSON back (so the same hash, sidecars and thumbnails)
    assert_eq!(s.to_json(), v);
    assert_eq!(DevelopSettings::from_json(&s.to_json()).unwrap().hash64(), s.hash64());
}

fn ai() -> DevelopSettings {
    let mut s = DevelopSettings::from_json(&old_json()).unwrap();
    s.spots.push(Spot {
        mode: SpotMode::Ai,
        points: vec![lightcraft_geom::Point::new(0.5, 0.5)],
        polygon: vec![],
        mask: Some(3),
        patch: Some(AiPatch {
            key: "ab".repeat(16),
            source: "cd".repeat(16),
            rect: [0.4, 0.4, 0.6, 0.6],
            engine: "klein".into(),
            seed: 42,
            geometry: "g".into(),
        }),
        ..Default::default()
    });
    s.enhance.denoise = 60.0;
    s.enhance.ai = Some(DenoiseRef { key: AiKey(0xabc), source: AiKey(u128::MAX) });
    s
}

#[test]
fn ai_spots_and_denoise_round_trip() {
    let s = ai();
    let v = s.to_json();
    assert_eq!(v["spots"][1]["mode"], "ai");
    assert_eq!(v["spots"][1]["patch"]["engine"], "klein");
    assert_eq!(v["enhance"]["ai"]["key"], format!("{:032x}", 0xabc));
    assert_eq!(DevelopSettings::from_json(&v).unwrap(), s);
    let mut bad = v.clone();
    bad["enhance"]["ai"]["key"] = json!("xyz");
    assert!(DevelopSettings::from_json(&bad).is_err());
    assert_eq!(AiKey::parse(&AiKey(7).to_string()), Some(AiKey(7)));
}

#[test]
fn copies_leave_photo_bound_ai_results_behind() {
    let s = ai();
    let c = extract_groups(&s, &[SettingsGroup::Spots, SettingsGroup::Detail]);
    let spots = c["spots"].as_array().unwrap();
    assert_eq!(spots.len(), 1, "the heal spot is copied, the AI spot is not");
    assert_eq!(spots[0]["mode"], "heal");
    assert!(c["enhance"].get("ai").is_none());
    assert_eq!(c["enhance"]["denoise"], 60.0);
    // pasting onto a photo with its own Denoise result keeps that result
    let mut other = DevelopSettings::default();
    other.enhance.ai = Some(DenoiseRef { key: AiKey(1), source: AiKey(2) });
    let pasted = crate::apply_partial(&other, &c, 1.0);
    assert_eq!(pasted.enhance.ai, other.enhance.ai);
    assert_eq!(pasted.enhance.denoise, 60.0);
}
