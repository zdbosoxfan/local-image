//! File Info interchange shared by the Library and editor. The JSON field names remain the
//! editor's public command schema; XML parsing, writing and preservation belong to this backend.

use serde_json::{Value, json};

use crate::{MergeRules, Metadata, XmpError, merge_xmp, parse_xmp, write_xmp};

const FIELDS: [(&str, &str); 8] = [
    ("title", "dc:title"),
    ("author", "dc:creator"),
    ("authorTitle", "photoshop:AuthorsPosition"),
    ("description", "dc:description"),
    ("keywords", "dc:subject"),
    ("copyright", "dc:rights"),
    ("copyrightStatus", "xmpRights:Marked"),
    ("copyrightUrl", "xmpRights:WebStatement"),
];

/// Read File Info using namespace URIs, entities, CDATA and x-default language alternatives.
/// Missing or unreadable packets yield the existing command's empty-field defaults.
pub fn read_file_info(xmp: Option<&str>) -> Value {
    let m = xmp.and_then(|x| parse_xmp(x).ok()).map(|x| x.metadata).unwrap_or_default();
    file_info(&m)
}

fn file_info(m: &Metadata) -> Value {
    json!({
        "title":m.title.as_deref().unwrap_or_default(), "author":m.artist.as_deref().unwrap_or_default(),
        "authorTitle":m.author_title.as_deref().unwrap_or_default(), "description":m.caption.as_deref().unwrap_or_default(),
        "keywords":m.keywords, "copyright":m.copyright.as_deref().unwrap_or_default(),
        "copyrightStatus":match m.copyright_marked { Some(true)=>"copyrighted", Some(false)=>"publicDomain", None=>"unknown" },
        "copyrightUrl":m.copyright_url.as_deref().unwrap_or_default(),
    })
}

fn text(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

fn list(v: &Value, split: bool) -> Vec<String> {
    match v {
        Value::Array(items) => items.iter().filter_map(text).collect(),
        Value::String(s) if split => s.split([';', ',']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect(),
        _ => text(v).into_iter().collect(),
    }
}

/// Apply a partial File Info edit, preserving all unedited properties and their XML framing.
/// A scalar author is one literal name; arrays explicitly replace the creator sequence.
/// Unchanged displayed values keep imported creator lists and localized alternatives intact.
/// Malformed packets fail before mutation instead of discarding opaque metadata.
pub fn try_write_file_info(xmp: Option<&str>, info: &Value) -> Result<String, XmpError> {
    let xmp = xmp.filter(|x| !x.trim().is_empty());
    let mut m = match xmp {
        Some(x) => parse_xmp(x)?.metadata,
        None => Metadata::default(),
    };
    let current = file_info(&m);
    let mut owned = Vec::new();
    for (key, property) in FIELDS {
        let Some(value) = info.get(key) else { continue };
        if xmp.is_some() && current.get(key) == Some(value) {
            continue;
        }
        owned.push(property);
        match key {
            "title" => m.title = text(value),
            "author" => {
                let creators = list(value, false);
                m.set_artist((!creators.is_empty()).then(|| creators.join("; ")));
                if creators.len() > 1 {
                    m.creators = creators;
                }
            }
            "authorTitle" => m.author_title = text(value),
            "description" => m.caption = text(value),
            "keywords" => m.keywords = list(value, true),
            "copyright" => m.copyright = text(value),
            "copyrightStatus" => {
                m.copyright_marked = match value.as_str() {
                    Some("copyrighted") => Some(true),
                    Some("publicDomain") => Some(false),
                    _ => None,
                }
            }
            "copyrightUrl" => m.copyright_url = text(value),
            _ => {}
        }
    }
    if let Some(existing) = xmp {
        if owned.is_empty() {
            return Ok(existing.to_string());
        }
        merge_xmp(existing, &write_xmp(&m, None), MergeRules { owned: &owned, owned_if_present: &[] })
    } else {
        Ok(write_xmp(&m, None))
    }
}

/// Build Save for Web's privacy-limited packet from the same namespace-aware metadata.
/// Never carry camera settings, locations, keywords, ratings or opaque developer properties.
pub fn copyright_xmp(xmp: &str, contact: bool) -> Result<Option<String>, XmpError> {
    let source = parse_xmp(xmp)?.metadata;
    let m = Metadata {
        copyright: source.copyright,
        copyright_marked: source.copyright_marked,
        copyright_url: source.copyright_url,
        artist: if contact { source.artist } else { None },
        creators: if contact { source.creators } else { Vec::new() },
        author_title: if contact { source.author_title } else { None },
        ..Default::default()
    };
    let populated =
        m.copyright.is_some() || m.copyright_marked.is_some() || m.copyright_url.is_some() || m.artist.is_some() || m.author_title.is_some();
    Ok(populated.then(|| write_xmp(&m, None)))
}

#[cfg(test)]
mod j03_tests {
    use super::*;

    #[test]
    fn j03_file_info_partial_edits_preserve_creators_locales_and_unknown_properties() {
        let m = Metadata {
            artist: Some("Doe, Jane; Studio; Bob".into()),
            creators: vec!["Doe, Jane; Studio".into(), "Bob".into()],
            title: Some("Original".into()),
            rating: Some(4),
            keywords: vec!["literal, keyword; one".into()],
            ..Default::default()
        };
        let xmp = write_xmp(&m, Some(r#"{"exposure":1.25}"#));
        let xmp = xmp.replacen("    </rdf:Alt>", r#"<rdf:li xml:lang="fr">Titre</rdf:li></rdf:Alt>"#, 1);
        assert!(xmp.contains(r#"<rdf:li xml:lang="fr">Titre</rdf:li>"#), "fixture includes the translation");
        let unchanged = try_write_file_info(Some(&xmp), &read_file_info(Some(&xmp))).unwrap();
        assert_eq!(unchanged, xmp);
        let edited = try_write_file_info(Some(&xmp), &json!({"description":"New & caption", "authorTitle":"Photographer"})).unwrap();
        let back = parse_xmp(&edited).unwrap();
        assert_eq!(back.metadata.creators, m.creators);
        assert_eq!(back.metadata.keywords, m.keywords);
        assert_eq!(back.metadata.caption.as_deref(), Some("New & caption"));
        assert_eq!(back.metadata.author_title.as_deref(), Some("Photographer"));
        assert_eq!(back.metadata.rating, Some(4));
        assert_eq!(back.lc_settings.as_deref(), Some(r#"{"exposure":1.25}"#));
        assert!(edited.contains(r#"<rdf:li xml:lang="fr">Titre</rdf:li>"#));
        let explicit = try_write_file_info(Some(&edited), &json!({"author":"Doe, Jane; Studio; Bob"})).unwrap();
        assert_eq!(parse_xmp(&explicit).unwrap().metadata.creators, m.creators, "retaining the UI display retains the list");
        let explicit = try_write_file_info(Some(&edited), &json!({"author":"Smith, Sam; Studio"})).unwrap();
        assert_eq!(parse_xmp(&explicit).unwrap().properties["dc:creator"], vec!["Smith, Sam; Studio"]);
        let explicit = try_write_file_info(Some(&edited), &json!({"author":["A; B", "C, D"]})).unwrap();
        assert_eq!(parse_xmp(&explicit).unwrap().metadata.creators, vec!["A; B", "C, D"]);
    }

    #[test]
    fn j03_file_info_clears_fields_without_losing_developer_metadata_and_refuses_bad_xml() {
        let m = Metadata { title: Some("Title".into()), artist: Some("Author".into()), copyright_marked: Some(true), ..Default::default() };
        let old = write_xmp(&m, Some("opaque"));
        let new = try_write_file_info(Some(&old), &json!({"title":"", "author":null, "copyrightStatus":"unknown"})).unwrap();
        let back = parse_xmp(&new).unwrap();
        assert!(back.metadata.title.is_none() && back.metadata.artist.is_none() && back.metadata.copyright_marked.is_none());
        assert_eq!(back.lc_settings.as_deref(), Some("opaque"));
        let bad = "<rdf:RDF><rdf:Description><dc:title>bad</rdf:RDF>";
        assert!(try_write_file_info(Some(bad), &json!({"title":"Replacement"})).is_err());
        assert_eq!(try_write_file_info(Some(bad), &json!({"title":"Replacement"})).unwrap_or_else(|_| bad.to_string()), bad);
    }

    #[test]
    fn j03_copyright_privacy_preserves_creator_lists_and_omits_private_fields() {
        let m = Metadata {
            artist: Some("A; B; C".into()),
            creators: vec!["A; B".into(), "C".into()],
            copyright: Some("© Test".into()),
            copyright_marked: Some(false),
            copyright_url: Some("https://example.test".into()),
            title: Some("Private title".into()),
            rating: Some(5),
            keywords: vec!["private".into()],
            gps: Some(crate::Gps { latitude: 12.0, longitude: 34.0, altitude: None }),
            ..Default::default()
        };
        let old = write_xmp(&m, Some("private settings"));
        let out = copyright_xmp(&old, true).unwrap().unwrap();
        let back = parse_xmp(&out).unwrap();
        assert_eq!(back.metadata.creators, m.creators);
        assert_eq!(back.metadata.copyright_marked, Some(false));
        assert!(back.metadata.title.is_none() && back.metadata.gps.is_none() && back.metadata.keywords.is_empty() && back.lc_settings.is_none());
        let rights = parse_xmp(&copyright_xmp(&old, false).unwrap().unwrap()).unwrap();
        assert!(rights.metadata.artist.is_none());
        assert_eq!(copyright_xmp(&write_xmp(&Metadata::default(), None), false).unwrap(), None);
    }
    #[test]
    fn j03_file_info_foreign_namespace_collisions_survive_editing() {
        let packet = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:dc="urn:foreign" xmlns:d="http://purl.org/dc/elements/1.1/"><rdf:Description dc:title="foreign title" d:title="Actual"/></rdf:RDF>"#;
        assert_eq!(read_file_info(Some(packet))["title"], "Actual");
        let edited = try_write_file_info(Some(packet), &json!({"title":"Edited"})).unwrap();
        assert!(edited.contains("foreign title"));
        assert_eq!(read_file_info(Some(&edited))["title"], "Edited");
        let packet = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns="http://purl.org/dc/elements/1.1/" title="unqualified"><title>Actual</title></rdf:Description></rdf:RDF>"#;
        assert_eq!(read_file_info(Some(packet))["title"], "Actual");
        let edited = try_write_file_info(Some(packet), &json!({"title":"Edited"})).unwrap();
        assert!(edited.contains("unqualified"));
        assert_eq!(read_file_info(Some(&edited))["title"], "Edited");
        let mut old = serde_json::to_value(Metadata::default()).unwrap();
        old.as_object_mut().unwrap().remove("author_title");
        let old: Metadata = serde_json::from_value(old).unwrap();
        assert!(old.author_title.is_none());
        assert_eq!(read_file_info(Some(&try_write_file_info(Some(""), &json!({"title":"New"})).unwrap()))["title"], "New");
    }
}
