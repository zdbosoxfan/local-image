//! Data-only native live-group compatibility codec for the VectorData field.
//! Original GPL-3.0-or-later. Adjacent tagging separates the live group's kind from the
//! nested repeat/envelope kind. A blend or live-paint record without `spec` uses pc-doc's
//! internally tagged form and is read as such; flattened repeat/envelope kinds are refused.
//! This source has no engine/UI dependency and can be registered directly in pc-doc.
use photocraft_doc::vector_design::live::{BlendSpec, EnvelopeSpec, LiveGroup, RepeatSpec};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Serialize)]
#[serde(tag = "kind", content = "spec", rename_all = "snake_case")]
enum WireRef<'a> {
    Blend(&'a BlendSpec),
    Repeat(&'a RepeatSpec),
    Envelope(&'a EnvelopeSpec),
    LivePaint(&'a photocraft_doc::vector_design::live::LivePaintSpec),
}
#[derive(Deserialize)]
#[serde(tag = "kind", content = "spec", rename_all = "snake_case")]
enum Wire {
    Blend(BlendSpec),
    Repeat(RepeatSpec),
    Envelope(EnvelopeSpec),
    LivePaint(photocraft_doc::vector_design::live::LivePaintSpec),
}
impl From<Wire> for LiveGroup {
    fn from(value: Wire) -> Self {
        match value {
            Wire::Blend(s) => Self::Blend(s),
            Wire::Repeat(s) => Self::Repeat(s),
            Wire::Envelope(s) => Self::Envelope(s),
            Wire::LivePaint(s) => Self::LivePaint(s),
        }
    }
}
pub fn serialize<S: Serializer>(value: &Option<LiveGroup>, serializer: S) -> Result<S::Ok, S::Error> {
    let wire = value.as_ref().map(|value| match value {
        LiveGroup::Blend(s) => WireRef::Blend(s),
        LiveGroup::Repeat(s) => WireRef::Repeat(s),
        LiveGroup::Envelope(s) => WireRef::Envelope(s),
        LiveGroup::LivePaint(s) => WireRef::LivePaint(s),
    });
    wire.serialize(serializer)
}
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<LiveGroup>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(None);
    }
    let parse = |value| serde_json::from_value::<Wire>(value).map(LiveGroup::from).map(Some).map_err(serde::de::Error::custom);
    if value.get("spec").is_some() {
        return parse(value);
    }
    let kind = value.get("kind").and_then(serde_json::Value::as_str).ok_or_else(|| serde::de::Error::custom("live group kind required"))?;
    // pc-doc writes blend and live paint internally tagged (no `spec`).
    let outer = match kind {
        "blend" => "blend",
        "live_paint" => "live_paint",
        _ => return Err(serde::de::Error::custom("unsupported live group kind")),
    };
    parse(serde_json::json!({"kind":outer,"spec":value}))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use photocraft_doc::vector_design::live::BlendSpacing;
    #[derive(Serialize, Deserialize)]
    struct Record {
        #[serde(default, with = "super")]
        live: Option<LiveGroup>,
    }
    #[test]
    fn repeat19_native_codec_adjacent_tags_roundtrip_all_live_kinds() {
        for live in [
            LiveGroup::Repeat(RepeatSpec::Radial { count: 5, center: [4., 7.], angle: 0.3 }),
            LiveGroup::Repeat(RepeatSpec::Grid { rows: 2, cols: 3, spacing: [4., 6.] }),
            LiveGroup::Repeat(RepeatSpec::Mirror { origin: [4., 7.], angle: 0.3 }),
            LiveGroup::Blend(BlendSpec { spacing: BlendSpacing::Steps(3), ..Default::default() }),
            LiveGroup::Envelope(EnvelopeSpec::Warp { style: "arc".into(), bend: 0.5, horizontal: 0., vertical: 0. }),
            LiveGroup::Envelope(EnvelopeSpec::Mesh { rows: 2, cols: 2, points: vec![[0., 0.], [1., 0.], [0., 1.], [1., 1.]] }),
        ] {
            let data = serde_json::to_value(Record { live: Some(live.clone()) }).unwrap();
            assert!(data["live"]["spec"].is_object());
            let loaded: Record = serde_json::from_value(data).unwrap();
            assert_eq!(loaded.live, Some(live));
        }
        let loaded: Record = serde_json::from_str("{}").unwrap();
        assert_eq!(loaded.live, None);
        let loaded: Record = serde_json::from_str("{\"live\":null}").unwrap();
        assert_eq!(loaded.live, None);
    }
    #[test]
    fn repeat19_native_codec_reads_pc_doc_tags_and_rejects_flattened_or_bad_parameters() {
        for live in [
            LiveGroup::Repeat(RepeatSpec::Grid { rows: 2, cols: 3, spacing: [4., 6.] }),
            LiveGroup::Repeat(RepeatSpec::Radial { count: 5, center: [4., 7.], angle: 0.3 }),
            LiveGroup::Repeat(RepeatSpec::Mirror { origin: [4., 7.], angle: 0.3 }),
            LiveGroup::Blend(BlendSpec { spacing: BlendSpacing::Steps(3), ..Default::default() }),
            LiveGroup::Envelope(EnvelopeSpec::Warp { style: "arc".into(), bend: 0.5, horizontal: 0., vertical: 0. }),
        ] {
            let doc_wire = serde_json::to_value(&live).unwrap();
            let loaded: Record = serde_json::from_value(serde_json::json!({"live":doc_wire})).unwrap();
            assert_eq!(loaded.live, Some(live));
        }
        for value in [
            serde_json::json!({"kind":"missing"}),
            serde_json::json!({"kind":"grid","rows":2}),
            serde_json::json!({"kind":"grid","rows":2,"cols":3,"spacing":[4.0,6.0]}),
            serde_json::json!({"kind":"repeat","spec":{"kind":"mirror","angle":0,"origin":[null,0]}}),
        ] {
            assert!(serde_json::from_value::<Record>(serde_json::json!({"live":value})).is_err());
        }
    }
}
