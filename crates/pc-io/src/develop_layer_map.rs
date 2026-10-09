//! local-image: **Develop layers in PSD** (a smart object whose `develop` link renders its source
//! through the Library's develop engine; see `photocraft_engine::develop_layer_cmds`).
//!
//! A Develop layer is written as a smart object with its source embedded, like any other smart
//! object, plus:
//!
//! * **Our record** (authoritative): `{"command": "developLayer", "settings", "photo"}` under the
//!   private `localImage` key of the placed-layer descriptor (`soLD`), the same key the Camera Raw
//!   Filter uses for its own record. Photoshop ignores it; our importer reads it first.
//! * **For Photoshop**, depending on the source:
//!   * a **raw** source becomes a Camera Raw smart object: the `crs:` settings
//!     ([`lightcraft_engine::crs::from_settings`], the mapping the Library uses for XMP sidecars)
//!     go with the embedded file as its open parameters ([`open_descriptor`]). Each settings
//!     version gets its own embedded file, as Photoshop keeps settings per file.
//!   * any other source (JPEG, TIFF, PNG…) would open in Photoshop undeveloped, so the develop
//!     is written as a **Camera Raw Filter** smart filter at the bottom of the stack (the subset
//!     Photoshop understands), marked with our record ([`MARKER`]); the importer turns that filter
//!     back into the layer's link ([`resolve`]).
//!
//! On import ([`resolve`]): our record first, then a marked Camera Raw Filter, then Camera Raw
//! settings stored with the embedded file (`crs:` XMP, or Camera Raw descriptor keys).

use photocraft_doc::{DevelopLink, Document, LayerContent, LayerId, Metadata, SmartFilter, SmartObject, SmartSource};
use photocraft_psd::descriptor::{Descriptor, UnicodeString, Value, VersionedDescriptor};
use serde_json::{Value as J, json};

use crate::smart_map::LOCAL_IMAGE_KEY;

/// `command` of our record.
pub const RECORD_COMMAND: &str = "developLayer";
/// Params key of a Camera Raw Filter that stands for a Develop layer (`{"settings", "photo"}`).
/// Bookkeeping: develop-settings parsing ignores `__` keys.
pub const MARKER: &str = "__developLayer";
/// Key of the `crs:` XMP packet in the open descriptor.
const XMP_KEY: &str = "XMPMetadataAsUTF8";

/// Our record for `settings` / `photo`.
pub fn record_json(settings: J, photo: Option<u64>) -> J {
    json!({"command": RECORD_COMMAND, "settings": settings, "photo": photo})
}

/// Is `r` our Develop layer record?
pub fn is_record(r: &J) -> bool {
    r.get("command").and_then(J::as_str) == Some(RECORD_COMMAND)
}

fn link_from_record(r: &J) -> Option<DevelopLink> {
    is_record(r).then_some(())?;
    Some(DevelopLink { settings: r.get("settings")?.clone(), photo: r.get("photo").and_then(J::as_u64) })
}

/// The Develop link recorded in a placed-layer descriptor.
pub fn link_from_sold(d: &Descriptor) -> Option<DevelopLink> {
    let Some(Value::Text(t)) = d.get(LOCAL_IMAGE_KEY) else { return None };
    link_from_record(&serde_json::from_str(&t.to_string_lossy()).ok()?)
}

/// `SoLd` block data with our record set to `link` (or removed when `None`). Unchanged when the
/// data isn't a `soLD` structure.
pub fn sold_with_record(data: Vec<u8>, link: Option<&DevelopLink>) -> Vec<u8> {
    let Some(mut d) = crate::smart_map::sold_descriptor(&data) else { return data };
    let had = d.get(LOCAL_IMAGE_KEY).is_some();
    d.items.retain(|(k, _)| !k.is(LOCAL_IMAGE_KEY));
    match link {
        Some(l) => {
            let text = record_json(l.settings.clone(), l.photo).to_string();
            d.items.push((photocraft_psd::descriptor::Id::new(LOCAL_IMAGE_KEY), Value::Text(UnicodeString::new_nul(&text))));
        }
        None if !had => return data,
        None => {}
    }
    let mut out = b"soLD".to_vec();
    out.extend_from_slice(&4u32.to_be_bytes());
    out.extend(VersionedDescriptor::new(d).to_bytes());
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    out
}

/// The develop settings of `link` as our typed settings (missing fields take their defaults).
fn typed(link: &DevelopLink) -> Option<lightcraft_develop::DevelopSettings> {
    crate::develop_filter::parse_settings(&link.settings).ok()
}

/// The Camera Raw settings Photoshop should open a raw source with: the `crs:` XMP packet of
/// the settings under `XMPMetadataAsUTF8`.
///
/// **Unverified against a Photoshop-made sample**: the PSD specification documents the open
/// descriptor of an embedded file but not what Photoshop keeps there for Camera Raw smart
/// objects, and we have no Photoshop-written raw smart object to compare with. Our record in the
/// placed-layer descriptor stays authoritative; this block is only for other applications, and
/// the one place to correct once a sample is available.
pub fn open_descriptor(link: &DevelopLink) -> Option<Descriptor> {
    let s = typed(link)?;
    let packet = lightcraft_engine::crs::packet(&lightcraft_engine::crs::from_settings(&s, true));
    Some(Descriptor::new("null").with(XMP_KEY, Value::Text(UnicodeString::new_nul(&packet))))
}

/// Develop settings (JSON) from the open descriptor of an embedded file: a `crs:` (or our
/// `lc:settings`) XMP packet, or Camera Raw descriptor keys (directly or under `As  `, as an
/// open action records them). `raw`: the file is raw (absolute white balance).
pub fn settings_from_open(open: &Descriptor, raw: bool) -> Option<J> {
    use lightcraft_engine::sidecar::{DevelopPatch, parse_sidecar};
    if let Some(Value::Text(t)) = open.get(XMP_KEY)
        && let Ok(sc) = parse_sidecar(&t.to_string_lossy(), raw)
    {
        let s = match sc.develop {
            Some(DevelopPatch::Full(s)) => Some(*s),
            Some(DevelopPatch::Partial(v)) => Some(lightcraft_develop::apply_partial(&lightcraft_develop::DevelopSettings::default(), &v, 1.0)),
            None => None,
        };
        if let Some(s) = s {
            return serde_json::to_value(s).ok();
        }
    }
    let d = match open.get("As  ") {
        Some(Value::Descriptor(d)) => d,
        _ => open,
    };
    crate::smart_map::camera_raw_descriptor_settings(d).and_then(|s| serde_json::to_value(s).ok())
}

/// Is the Develop layer's source a raw file? (`bytes` of the source.)
pub fn is_raw_source(bytes: &[u8]) -> bool {
    crate::raw::is_raw(bytes)
}

/// The Camera Raw Filter that stands for a Develop layer with a non-raw source in PSD (applied
/// first, under the layer's own smart filters).
pub fn stand_in_filter(link: &DevelopLink) -> SmartFilter {
    let mut params = match &link.settings {
        J::Object(m) => J::Object(m.clone()),
        _ => json!({}),
    };
    if let Some(m) = params.as_object_mut() {
        m.insert(MARKER.into(), json!({"settings": link.settings, "photo": link.photo}));
    }
    SmartFilter { command: crate::develop_filter::COMMAND.into(), params, blend: photocraft_color::BlendMode::Normal, opacity: 1.0, visible: true }
}

/// The Develop link a smart object imported from PSD carries, if any (see the module docs);
/// a stand-in Camera Raw Filter is taken out of the stack.
fn resolve_one(sm: &mut SmartObject, meta: &Metadata) -> Option<DevelopLink> {
    let stand_in = sm.smart_filters.iter().position(|f| f.command == crate::develop_filter::COMMAND && f.params.get(MARKER).is_some());
    let from_filter = stand_in.map(|i| sm.smart_filters.remove(i)).and_then(|f| {
        let m = f.params.get(MARKER)?;
        Some(DevelopLink { settings: m.get("settings")?.clone(), photo: m.get("photo").and_then(J::as_u64) })
    });
    if let Some(link) = sm.psd_raw.as_deref().and_then(|d| crate::smart_map::sold_descriptor(d)).as_ref().and_then(link_from_sold) {
        return Some(link);
    }
    if from_filter.is_some() {
        return from_filter;
    }
    let SmartSource::Linked { path } = &sm.source else { return None };
    let open = crate::linked::find_open_descriptor(meta, path)?;
    let raw = crate::linked::find_linked_file(meta, path).is_some_and(|f| is_raw_source(&f.bytes));
    Some(DevelopLink { settings: settings_from_open(&open, raw)?, photo: None })
}

/// Turns the smart objects of an imported PSD that are Develop layers back into Develop layers.
pub(crate) fn resolve(doc: &mut Document) {
    let ids: Vec<LayerId> =
        doc.walk().into_iter().filter(|(_, _, l)| matches!(&l.content, LayerContent::Smart(sm) if sm.develop.is_none())).map(|(_, _, l)| l.id).collect();
    if ids.is_empty() {
        return;
    }
    let meta = doc.metadata.clone();
    for id in ids {
        if let Some(LayerContent::Smart(sm)) = doc.layer_mut(id).map(|l| &mut l.content)
            && let Some(link) = resolve_one(sm, &meta)
        {
            sm.develop = Some(link);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link() -> DevelopLink {
        let mut s = lightcraft_develop::DevelopSettings::default();
        s.light.exposure = 0.5;
        s.color.vibrance = 20.0;
        DevelopLink { settings: serde_json::to_value(&s).unwrap(), photo: Some(42) }
    }

    #[test]
    fn records_round_trip_through_placed_layer_data() {
        let d = Descriptor::new("null").with("Idnt", Value::Text(UnicodeString::new_nul("u")));
        let mut data = b"soLD".to_vec();
        data.extend_from_slice(&4u32.to_be_bytes());
        data.extend(VersionedDescriptor::new(d).to_bytes());
        let with = sold_with_record(data.clone(), Some(&link()));
        let back = crate::smart_map::sold_descriptor(&with).unwrap();
        assert_eq!(link_from_sold(&back), Some(link()));
        assert_eq!(back.get("Idnt"), Some(&Value::Text(UnicodeString::new_nul("u"))));
        // removed again; untouched when there is nothing to remove
        let without = sold_with_record(with, None);
        assert_eq!(link_from_sold(&crate::smart_map::sold_descriptor(&without).unwrap()), None);
        assert_eq!(sold_with_record(data.clone(), None), data);
    }

    #[test]
    fn open_descriptor_carries_crs_settings() {
        let open = open_descriptor(&link()).unwrap();
        let Some(Value::Text(t)) = open.get(XMP_KEY) else { panic!("no packet") };
        assert!(t.to_string_lossy().contains("crs:Exposure2012=\"0.5\""));
        let s: lightcraft_develop::DevelopSettings = serde_json::from_value(settings_from_open(&open, true).unwrap()).unwrap();
        assert_eq!((s.light.exposure, s.color.vibrance), (0.5, 20.0));
        assert!(settings_from_open(&Descriptor::new("null"), true).is_none());
    }

    #[test]
    fn camera_raw_descriptor_keys_are_read_too() {
        let acr = Descriptor::new("Adobe Camera Raw")
            .with("CrVe", Value::Text(UnicodeString::new_nul("18.4")))
            .with("PrVN", Value::Integer(6))
            .with("PrVe", Value::Integer(251920384))
            .with("Ex12", Value::Double(1.25))
            .with("Cr12", Value::Integer(-20));
        let open = Descriptor::new("null").with("As  ", Value::Descriptor(acr));
        let s: lightcraft_develop::DevelopSettings = serde_json::from_value(settings_from_open(&open, true).unwrap()).unwrap();
        assert!((s.light.exposure - 1.25).abs() < 1e-6);
        assert_eq!(s.light.contrast, -20.0);
    }

    #[test]
    fn stand_in_filters_become_the_link_again() {
        let mut sm = SmartObject::new(SmartSource::Linked { path: "x".into() }, photocraft_geom::Affine::IDENTITY, None);
        let other = SmartFilter {
            command: "filter.gaussianBlur".into(),
            params: json!({"radius": 2}),
            blend: photocraft_color::BlendMode::Normal,
            opacity: 1.0,
            visible: true,
        };
        sm.smart_filters = vec![stand_in_filter(&link()), other.clone()];
        assert_eq!(resolve_one(&mut sm, &Metadata::default()), Some(link()));
        assert_eq!(sm.smart_filters, vec![other]);
    }
}
