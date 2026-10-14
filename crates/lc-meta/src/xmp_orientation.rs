//! Byte-preserving orientation normalization, including legacy editor XMP fragments.
use std::borrow::Cow;

use quick_xml::events::Event;

const TIFF_NS: &str = "http://ns.adobe.com/tiff/1.0/";

fn orientation(raw: &str, scopes: &[Vec<(String, String)>]) -> bool {
    let (prefix, local) = raw.split_once(':').unwrap_or(("", raw));
    if local != "Orientation" {
        return false;
    }
    match scopes.iter().rev().flat_map(|s| s.iter().rev()).find(|(p, _)| p == prefix) {
        Some((_, uri)) => uri == TIFF_NS,
        // Earlier versions stored fragments without namespace declarations.
        None => prefix == "tiff",
    }
}

fn needs_one(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) && value != "1"
}

fn edits(packet: &str) -> Option<Vec<(usize, usize)>> {
    let mut reader = quick_xml::Reader::from_str(packet);
    let mut scopes = Vec::new();
    let mut elements: Vec<(bool, usize, String, bool)> = Vec::new();
    let mut out = Vec::new();
    let mut nodes = 0usize;
    loop {
        let before = usize::try_from(reader.buffer_position()).ok()?;
        let event = reader.read_event().ok()?;
        let after = usize::try_from(reader.buffer_position()).ok()?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                nodes += 1;
                if nodes > 200_000 || scopes.len() >= 64 {
                    return None;
                }
                let empty = matches!(event, Event::Empty(_));
                let mut scope = Vec::new();
                let mut attrs = Vec::new();
                let mut names = std::collections::HashSet::new();
                for attr in e.attributes().with_checks(false) {
                    let attr = attr.ok()?;
                    let key = std::str::from_utf8(attr.key.as_ref()).ok()?.to_string();
                    if !names.insert(key.clone()) {
                        return None;
                    }
                    let value = attr.unescape_value().ok()?.into_owned();
                    if key == "xmlns" {
                        scope.push((String::new(), value.clone()));
                    } else if let Some(prefix) = key.strip_prefix("xmlns:") {
                        scope.push((prefix.to_string(), value.clone()));
                    }
                    attrs.push((key, value));
                }
                scopes.push(scope);
                let name = std::str::from_utf8(e.name().as_ref()).ok()?.to_string();
                // Locate quoted values in the original opening tag, preserving every other byte.
                let bytes = packet.get(before..after)?.as_bytes();
                let mut at = 1 + name.len();
                for (key, value) in attrs {
                    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                        at += 1;
                    }
                    at = at.checked_add(key.len())?;
                    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                        at += 1;
                    }
                    if bytes.get(at) != Some(&b'=') {
                        return None;
                    }
                    at += 1;
                    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                        at += 1;
                    }
                    let quote = *bytes.get(at).filter(|b| matches!(**b, b'\'' | b'"'))?;
                    at += 1;
                    let start = at;
                    at += bytes.get(at..)?.iter().position(|b| *b == quote)?;
                    if key.contains(':') && orientation(&key, &scopes) && needs_one(&value) {
                        out.push((before + start, before + at));
                    }
                    at += 1;
                }
                if let Some(parent) = elements.last_mut() {
                    parent.3 = true;
                }
                let target = orientation(&name, &scopes);
                if empty {
                    scopes.pop();
                } else {
                    elements.push((target, after, String::new(), false));
                }
            }
            Event::Text(text) => {
                if let Some((true, _, value, _)) = elements.last_mut() {
                    value.push_str(&quick_xml::escape::unescape(&text.decode().ok()?).ok()?);
                }
            }
            Event::CData(text) => {
                if let Some((true, _, value, _)) = elements.last_mut() {
                    value.push_str(&text.decode().ok()?);
                }
            }
            Event::GeneralRef(reference) => {
                if let Some((true, _, value, _)) = elements.last_mut() {
                    let name = reference.decode().ok()?;
                    value.push_str(&quick_xml::escape::unescape(&format!("&{name};")).ok()?);
                }
            }
            Event::End(_) => {
                let (target, start, value, structured) = elements.pop()?;
                scopes.pop()?;
                if target && !structured && needs_one(&value) {
                    out.push((start, before));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Some(out)
}

/// Set real TIFF orientation attributes/elements to 1, keeping all unrelated bytes unchanged.
/// Namespace aliases work; foreign namespaces, comments and ordinary text are never rewritten.
/// Undeclared `tiff:` prefixes and standalone attributes remain supported for old documents.
/// Unreadable XML is returned unchanged. Borrowed when no value needs rewriting.
pub fn upright_xmp(packet: &str) -> Cow<'_, str> {
    let bare = packet.trim_start().starts_with("tiff:Orientation") && !packet.contains('<');
    let wrapped;
    let (source, offset) = if bare {
        wrapped = format!("<legacy {packet}/>");
        (wrapped.as_str(), 8usize)
    } else {
        (packet, 0)
    };
    let Some(mut edits) = edits(source) else { return Cow::Borrowed(packet) };
    if edits.is_empty() {
        return Cow::Borrowed(packet);
    }
    edits.sort_unstable();
    let mut out = String::with_capacity(packet.len());
    let mut at = 0;
    for (start, end) in edits {
        let Some(start) = start.checked_sub(offset) else { return Cow::Borrowed(packet) };
        let Some(end) = end.checked_sub(offset) else { return Cow::Borrowed(packet) };
        if start < at {
            return Cow::Borrowed(packet);
        }
        let Some(prefix) = packet.get(at..start) else { return Cow::Borrowed(packet) };
        out.push_str(prefix);
        out.push('1');
        at = end;
    }
    let Some(tail) = packet.get(at..) else { return Cow::Borrowed(packet) };
    out.push_str(tail);
    Cow::Owned(out)
}

#[cfg(test)]
mod j03_tests {
    use super::*;

    #[test]
    fn j03_xmp_orientation_uses_namespaces_and_preserves_unrelated_bytes() {
        let xmp = r#"<r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:o="http://ns.adobe.com/tiff/1.0/" xmlns:tiff="urn:foreign"><r:Description o:Orientation='6' tiff:Orientation="8"><o:Orientation><![CDATA[8]]></o:Orientation><tiff:Orientation>7</tiff:Orientation><!-- tiff:Orientation="6" --><other>tiff:Orientation="6"</other></r:Description></r:RDF>"#;
        let want = xmp.replace("o:Orientation='6'", "o:Orientation='1'").replace("<![CDATA[8]]>", "1");
        assert_eq!(upright_xmp(xmp), want);
        assert!(matches!(upright_xmp(&want), Cow::Borrowed(_)));
        for (old, new) in [
            (r#"<rdf:Description tiff:Orientation="6"/>"#, r#"<rdf:Description tiff:Orientation="1"/>"#),
            ("tiff:Orientation = '8'", "tiff:Orientation = '1'"),
            ("<tiff:Orientation>06</tiff:Orientation>", "<tiff:Orientation>1</tiff:Orientation>"),
            ("<tiff:Orientation>&#54;</tiff:Orientation>", "<tiff:Orientation>1</tiff:Orientation>"),
        ] {
            assert_eq!(upright_xmp(old), new);
        }
        assert!(matches!(upright_xmp("<broken tiff:Orientation='8'"), Cow::Borrowed(_)));
        assert!(matches!(upright_xmp("<!-- tiff:Orientation='8' -->"), Cow::Borrowed(_)));
    }
}
