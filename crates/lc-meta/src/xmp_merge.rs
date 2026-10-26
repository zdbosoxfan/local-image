//! Merging a freshly written XMP packet into an existing one (a sidecar another application
//! wrote, e.g. with its develop settings and edit history), keeping everything the writer does not
//! own byte for byte.
//!
//! The existing packet is scanned once (namespace-resolved, with byte positions). Only top-level
//! properties of the `rdf:Description` elements directly under `rdf:RDF` are considered, in
//! attribute form (`xmp:Rating="3"`) and element form (`<dc:subject>…</dc:subject>`):
//! - properties the writer owns ([`MergeRules::owned`]) are removed, and written from the fresh
//!   packet when it has them (so a cleared field is cleared in the file too);
//! - properties owned only when stated ([`MergeRules::owned_if_present`]) are replaced when the
//!   fresh packet has them and kept otherwise;
//! - any other property of the fresh packet (e.g. `xmp:CreatorTool`) is not written: the file
//!   keeps describing itself as its creator left it;
//! - everything else — other namespaces (e.g. `crs:`), `xmpMM:History`, unknown properties,
//!   comments, the packet wrapper and padding — is copied unchanged.
//!
//! The written properties go into one new `rdf:Description` (declaring its own namespaces, so
//! the existing prefix bindings never matter) just before `</rdf:RDF>`. A description left empty
//! by the removals is dropped, so repeated saves don't accumulate. The result is checked by
//! parsing it again: every written property must read back as in the fresh packet.

use std::collections::{BTreeSet, HashSet};

use quick_xml::events::Event;

use crate::xmp::{MAX_NODES, MAX_OPAQUE_NAME_BYTES, XmpError, canonical_name, parse_xmp};

/// Which properties [`merge_xmp`] takes from the fresh packet. Names are canonical
/// `prefix:name` (prefixes as [`crate::parse_xmp`] reports them); `prefix:*` is a whole namespace.
#[derive(Clone, Copy, Debug, Default)]
pub struct MergeRules<'a> {
    /// Always replaced: removed from the existing packet, written from the fresh one if stated.
    pub owned: &'a [&'a str],
    /// Replaced only when the fresh packet states them; otherwise the existing value stays.
    pub owned_if_present: &'a [&'a str],
}

fn listed(list: &[&str], name: &str) -> bool {
    list.iter().any(|p| match p.strip_suffix('*') {
        Some(ns) => name.starts_with(ns),
        None => *p == name,
    })
}

#[derive(Debug)]
struct Attr {
    geometry: Option<&'static str>,
    raw: String,
    name: String,
    /// The value as written (still escaped).
    value: String,
}

#[derive(Debug)]
struct Prop {
    geometry: Option<&'static str>,
    name: String,
    start: usize,
    end: usize,
}

#[derive(Debug, Default)]
struct Desc {
    tag_start: usize,
    tag_end: usize,
    /// The element name as written (`rdf:Description`, or another prefix for the RDF namespace).
    raw_name: String,
    empty: bool,
    attrs: Vec<Attr>,
    props: Vec<Prop>,
    /// The end of `</rdf:Description>` (`tag_end` when self-closing).
    end: usize,
}

#[derive(Debug, Default)]
pub(crate) struct Scan {
    descs: Vec<Desc>,
    /// Where the first `</rdf:RDF>` starts.
    rdf_close: Option<usize>,
}

enum Kind {
    Other,
    Rdf,
    Desc(usize),
    Prop { desc: usize, start: usize, name: String, geometry: Option<&'static str> },
}

// Geometry edits require a real namespace binding: a foreign namespace may use
// the literal prefix `tiff` without owning TIFF properties.
fn geometry_name(raw: &str, scopes: &[Vec<(String, String)>], attribute: bool) -> Option<&'static str> {
    let (prefix, local) = raw.split_once(':').unwrap_or(("", raw));
    if attribute && prefix.is_empty() {
        return None;
    }
    let uri = scopes.iter().rev().flat_map(|scope| scope.iter().rev()).find(|(p, _)| p == prefix)?.1.as_str();
    match (uri, local) {
        ("http://ns.adobe.com/tiff/1.0/", "Orientation") => Some("tiff:Orientation"),
        ("http://ns.adobe.com/tiff/1.0/", "ImageWidth") => Some("tiff:ImageWidth"),
        ("http://ns.adobe.com/tiff/1.0/", "ImageLength") => Some("tiff:ImageLength"),
        ("http://ns.adobe.com/exif/1.0/", "PixelXDimension") => Some("exif:PixelXDimension"),
        ("http://ns.adobe.com/exif/1.0/", "PixelYDimension") => Some("exif:PixelYDimension"),
        ("http://ns.adobe.com/xmp/note/", "HasExtendedXMP") => Some("xmpNote:HasExtendedXMP"),
        _ => None,
    }
}

fn in_namespace(raw: &str, scopes: &[Vec<(String, String)>], uri: &str, local: &str) -> bool {
    let (prefix, name) = raw.split_once(':').unwrap_or(("", raw));
    name == local && scopes.iter().rev().flat_map(|scope| scope.iter().rev()).find(|(p, _)| p == prefix).is_some_and(|(_, value)| value == uri)
}

fn xml_value(raw: &str) -> Result<String, XmpError> {
    quick_xml::escape::unescape(raw).map(|value| value.into_owned()).map_err(|e| XmpError::Xml(e.to_string()))
}

fn is_meta_attr(name: &str) -> bool {
    name.starts_with("xmlns")
        || matches!(name, "rdf:about" | "rdf:ID" | "rdf:nodeID" | "xml:lang" | "rdf:parseType" | "rdf:resource" | "rdf:datatype")
}

fn pos(r: &quick_xml::Reader<&[u8]>) -> usize {
    usize::try_from(r.buffer_position()).unwrap_or(usize::MAX)
}

/// Locate the descriptions, their top-level properties and `</rdf:RDF>` in a packet.
pub(crate) fn scan(s: &str) -> Result<Scan, XmpError> {
    let mut reader = quick_xml::Reader::from_reader(s.as_bytes());
    reader.config_mut().check_end_names = true;
    let mut out = Scan::default();
    let mut stack: Vec<Kind> = Vec::new();
    let mut scopes: Vec<Vec<(String, String)>> = vec![vec![("xml".into(), "http://www.w3.org/XML/1998/namespace".into())]];
    let mut count = 0usize;
    let mut opaque_budget = MAX_OPAQUE_NAME_BYTES;
    loop {
        let before = pos(&reader);
        let ev = reader.read_event().map_err(|e| XmpError::Xml(e.to_string()))?;
        let after = pos(&reader);
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                count += 1;
                if count > MAX_NODES || stack.len() > 64 {
                    return Err(XmpError::TooComplex);
                }
                let empty = matches!(ev, Event::Empty(_));
                let mut scope = Vec::new();
                let mut raw_attrs = Vec::new();
                // Repeated names are refused with a set, in linear time (quick-xml's own check is quadratic).
                let mut seen = HashSet::new();
                for a in e.attributes().with_checks(false) {
                    let a = a.map_err(|e| XmpError::Xml(e.to_string()))?;
                    let k = String::from_utf8_lossy(a.key.as_ref()).into_owned();
                    if !seen.insert(k.clone()) {
                        return Err(XmpError::Xml(format!("repeated attribute {k}")));
                    }
                    let v = String::from_utf8_lossy(&a.value).into_owned();
                    if k == "xmlns" {
                        scope.push((String::new(), xml_value(&v)?));
                    } else if let Some(p) = k.strip_prefix("xmlns:") {
                        scope.push((p.to_string(), xml_value(&v)?));
                    }
                    raw_attrs.push((k, v));
                }
                scopes.push(scope);
                let raw_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let name = canonical_name(&raw_name, &scopes, &mut opaque_budget)?;
                let kind = match stack.last() {
                    Some(Kind::Rdf) if in_namespace(&raw_name, &scopes, "http://www.w3.org/1999/02/22-rdf-syntax-ns#", "Description") => {
                        let attrs = raw_attrs
                            .into_iter()
                            .map(|(raw, value)| {
                                Ok(Attr {
                                    geometry: geometry_name(&raw, &scopes, true),
                                    name: if raw.contains(':') { canonical_name(&raw, &scopes, &mut opaque_budget)? } else { raw.clone() },
                                    raw,
                                    value,
                                })
                            })
                            .collect::<Result<_, XmpError>>()?;
                        out.descs.push(Desc { tag_start: before, tag_end: after, raw_name, empty, attrs, props: Vec::new(), end: after });
                        Kind::Desc(out.descs.len() - 1)
                    }
                    Some(Kind::Desc(i)) => Kind::Prop { desc: *i, start: before, geometry: geometry_name(&raw_name, &scopes, false), name },
                    _ if in_namespace(&raw_name, &scopes, "http://www.w3.org/1999/02/22-rdf-syntax-ns#", "RDF")
                        && out.rdf_close.is_none()
                        && !stack.iter().any(|kind| matches!(kind, Kind::Rdf)) =>
                    {
                        Kind::Rdf
                    }
                    _ => Kind::Other,
                };
                if empty {
                    scopes.pop();
                    if let Kind::Prop { desc, start, name, geometry } = kind
                        && let Some(d) = out.descs.get_mut(desc)
                    {
                        d.props.push(Prop { name, start, end: after, geometry });
                    }
                } else {
                    stack.push(kind);
                }
            }
            Event::End(_) => {
                scopes.pop();
                match stack.pop() {
                    Some(Kind::Prop { desc, start, name, geometry }) => {
                        if let Some(d) = out.descs.get_mut(desc) {
                            d.props.push(Prop { name, start, end: after, geometry });
                        }
                    }
                    Some(Kind::Desc(i)) => {
                        if let Some(d) = out.descs.get_mut(i) {
                            d.end = after;
                        }
                    }
                    Some(Kind::Rdf) => out.rdf_close = out.rdf_close.or(Some(before)),
                    Some(Kind::Other) => {}
                    None => return Err(XmpError::Xml("unbalanced end tag".into())),
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(XmpError::Xml("unclosed element".into()));
    }
    Ok(out)
}

/// `start` moved back over the indentation and line break before it (so a removed property
/// takes its line with it).
fn line_start(s: &str, start: usize) -> usize {
    let b = s.as_bytes();
    let mut i = start;
    while i > 0 && matches!(b.get(i - 1), Some(b' ' | b'\t')) {
        i -= 1;
    }
    if i > 0 && b.get(i - 1) == Some(&b'\n') {
        i -= 1;
        if i > 0 && b.get(i - 1) == Some(&b'\r') {
            i -= 1;
        }
    }
    i
}

/// Merge `fresh` (a packet as [`crate::write_xmp_lc`] writes it) into `existing` following
/// `rules` (see the module docs). Fails — and the caller must not overwrite `existing` blindly —
/// when `existing` isn't a well-formed XMP packet.
pub fn merge_xmp(existing: &str, fresh: &str, rules: MergeRules) -> Result<String, XmpError> {
    let (bom, old) = match existing.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", existing),
    };
    let previous = parse_xmp(old)?;
    let want = parse_xmp(fresh)?;
    // Preserve the original creator sequence and its XML framing when its actual
    // list is unchanged. Equal joined display strings alone are deliberately insufficient.
    let keep_creators = previous.values.contains_key("dc:creator")
        && previous.values.get("dc:creator") == want.values.get("dc:creator")
        && previous.properties.get("dc:creator") == want.properties.get("dc:creator");
    let ex = scan(old)?;
    let rdf_close = ex.rdf_close.ok_or_else(|| XmpError::Xml("no rdf:RDF element".into()))?;
    let new = scan(fresh)?;
    let fresh_desc = new.descs.first().ok_or_else(|| XmpError::Xml("the new packet has no rdf:Description".into()))?;
    let fresh_names: BTreeSet<&str> = fresh_desc.props.iter().map(|p| p.name.as_str()).collect();
    let remove = |name: &str| {
        !(keep_creators && name == "dc:creator")
            && (listed(rules.owned, name) || (listed(rules.owned_if_present, name) && fresh_names.contains(name)))
    };
    let write = |name: &str| !(keep_creators && name == "dc:creator") && (listed(rules.owned, name) || listed(rules.owned_if_present, name));

    // edits on `old`: (start, end, replacement), applied back to front
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for d in &ex.descs {
        let gone_attrs: Vec<&Attr> = d.attrs.iter().filter(|a| !is_meta_attr(&a.name) && remove(&a.name)).collect();
        let gone_props: Vec<&Prop> = d.props.iter().filter(|p| remove(&p.name)).collect();
        if gone_attrs.is_empty() && gone_props.is_empty() {
            continue;
        }
        let attrs_left = d.attrs.iter().any(|a| !is_meta_attr(&a.name) && !remove(&a.name));
        if !attrs_left && gone_props.len() == d.props.len() {
            // nothing of its own left: drop the whole description
            edits.push((line_start(old, d.tag_start), d.end, String::new()));
            continue;
        }
        if !gone_attrs.is_empty() {
            let mut tag = format!("<{}", d.raw_name);
            for a in d.attrs.iter().filter(|a| is_meta_attr(&a.name) || !remove(&a.name)) {
                tag.push_str(&format!("\n    {}=\"{}\"", a.raw, a.value.replace('"', "&quot;")));
            }
            tag.push_str(if d.empty { "/>" } else { ">" });
            edits.push((d.tag_start, d.tag_end, tag));
        }
        for p in gone_props {
            edits.push((line_start(old, p.start), p.end, String::new()));
        }
    }
    // the new description: its own namespace declarations, the properties to write
    let mut block = String::from("\n  <rdf:Description rdf:about=\"\"\n    xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"");
    for a in fresh_desc.attrs.iter().filter(|a| a.raw.starts_with("xmlns:") && a.raw != "xmlns:rdf") {
        block.push_str(&format!("\n    {}=\"{}\"", a.raw, a.value));
    }
    block.push('>');
    let mut written = 0;
    for p in fresh_desc.props.iter().filter(|p| write(&p.name)) {
        let text = fresh.get(p.start..p.end).ok_or_else(|| XmpError::Xml("bad property span".into()))?;
        block.push_str("\n   ");
        block.push_str(text);
        written += 1;
    }
    block.push_str("\n  </rdf:Description>");
    if written > 0 {
        let at = old.get(..rdf_close).map_or(rdf_close, |h| h.trim_end().len());
        edits.push((at, at, block));
    }
    edits.sort_by_key(|e| (e.0, e.1));
    // overlapping edits can't happen (properties sit inside their description, which is either
    // dropped whole or edited piecewise), but never splice blindly
    let mut out = String::with_capacity(old.len() + 4096);
    out.push_str(bom);
    let mut at = 0;
    for (s, e, r) in &edits {
        if *s < at {
            return Err(XmpError::Xml("overlapping edits".into()));
        }
        out.push_str(old.get(at..*s).ok_or_else(|| XmpError::Xml("bad span".into()))?);
        out.push_str(r);
        at = *e;
    }
    out.push_str(old.get(at..).ok_or_else(|| XmpError::Xml("bad span".into()))?);

    // read back: everything written must come out as written
    let merged = parse_xmp(&out)?;
    for (k, v) in &want.properties {
        let top = k.split('/').next().unwrap_or(k);
        if write(top) && merged.properties.get(k) != Some(v) {
            return Err(XmpError::Xml(format!("{k} did not survive the merge")));
        }
    }
    Ok(out)
}

/// Normalize existing pixel-geometry properties for upright replacement pixels.
/// Namespace aliases and duplicate properties are handled alike. Only geometry value
/// spans are changed; foreign fields, whitespace, comments and packet padding survive.
/// A packet with no geometry properties is returned unchanged. Unreadable geometry
/// fails instead of publishing contradictory metadata alongside the replacement raster.
pub fn normalize_xmp_geometry(packet: &str, width: u32, height: u32) -> Result<String, XmpError> {
    rewrite_xmp_geometry(packet, width, height, None)
}

/// Whether a packet references Extended XMP, regardless of prefix or XML scalar form.
/// Callers without Extended APP1 chunks must reject this dangling reference.
pub fn xmp_has_extended_guid(packet: &str) -> Result<bool, XmpError> {
    parse_xmp(packet)?;
    Ok(scan(packet)?.descs.iter().any(|desc| {
        desc.attrs.iter().any(|attr| attr.geometry == Some("xmpNote:HasExtendedXMP"))
            || desc.props.iter().any(|prop| prop.geometry == Some("xmpNote:HasExtendedXMP"))
    }))
}

/// Normalize a main JPEG XMP packet and update its Extended XMP link. At least one
/// namespace-resolved link to `old_guid` must exist; foreign GUID-shaped text is kept.
pub fn normalize_xmp_geometry_with_extended_guid(packet: &str, width: u32, height: u32, old_guid: &str, new_guid: &str) -> Result<String, XmpError> {
    rewrite_xmp_geometry(packet, width, height, Some((old_guid, new_guid)))
}

fn rewrite_xmp_geometry(packet: &str, width: u32, height: u32, extended_guid: Option<(&str, &str)>) -> Result<String, XmpError> {
    parse_xmp(packet)?;
    let scan = scan(packet)?;
    let bad = || XmpError::Xml("unsupported XMP geometry property".into());
    let mut links_updated = 0;
    let mut replacement = |name: &str, current: &str| -> Result<Option<String>, XmpError> {
        Ok(match name {
            "tiff:Orientation" => Some("1".into()),
            "tiff:ImageWidth" | "exif:PixelXDimension" => Some(width.to_string()),
            "tiff:ImageLength" | "exif:PixelYDimension" => Some(height.to_string()),
            "xmpNote:HasExtendedXMP" => match extended_guid {
                Some((old, new)) if current.trim().eq_ignore_ascii_case(old) => {
                    links_updated += 1;
                    Some(new.into())
                }
                Some(_) => return Err(XmpError::Xml("Extended XMP link does not match its packet GUID".into())),
                None => None,
            },
            _ => None,
        })
    };
    let mut edits = Vec::new();
    for desc in scan.descs {
        // XML was validated above. Walk quoted attribute values in source order,
        // paired with the scanner's resolved names, so no foreign bytes are rebuilt.
        let tag = &packet[desc.tag_start..desc.tag_end];
        let bytes = tag.as_bytes();
        let mut at = 1 + desc.raw_name.len();
        for attr in desc.attrs {
            while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
            at = at.checked_add(attr.raw.len()).ok_or_else(bad)?;
            while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
            if bytes.get(at) != Some(&b'=') {
                return Err(bad());
            }
            at += 1;
            while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
            let quote = *bytes.get(at).filter(|q| matches!(**q, b'\'' | b'"')).ok_or_else(bad)?;
            at += 1;
            let start = at;
            at += bytes.get(at..).and_then(|rest| rest.iter().position(|b| *b == quote)).ok_or_else(bad)?;
            if let Some(name) = attr.geometry
                && let Some(value) = replacement(name, &xml_value(&attr.value)?)?
            {
                edits.push((desc.tag_start + start, desc.tag_start + at, value));
            }
            at += 1;
        }
        for prop in desc.props {
            let Some(property) = prop.geometry else { continue };
            if property == "xmpNote:HasExtendedXMP" && extended_guid.is_none() {
                continue;
            }
            let source = &packet[prop.start..prop.end];
            let mut reader = quick_xml::Reader::from_reader(source.as_bytes());
            let first = reader.read_event().map_err(|e| XmpError::Xml(e.to_string()))?;
            let opening_end = pos(&reader);
            // Resource-valued/structured forms need their own RDF rewrite. Refuse
            // them instead of adding text alongside a stale resource attribute.
            if let Event::Start(ref e) | Event::Empty(ref e) = first {
                // scan() above already refused repeated attribute names in linear time.
                for attr in e.attributes().with_checks(false) {
                    let attr = attr.map_err(|_| bad())?;
                    let name = attr.key.as_ref();
                    if !name.starts_with(b"xmlns") && name != b"xml:lang" {
                        return Err(bad());
                    }
                }
            }
            match first {
                Event::Empty(e) => {
                    let Some(value) = replacement(property, "")? else { continue };
                    let name = std::str::from_utf8(e.name().as_ref()).map_err(|_| bad())?.to_owned();
                    let opening = source.strip_suffix("/>").ok_or_else(bad)?;
                    edits.push((prop.start, prop.end, format!("{opening}>{value}</{name}>")));
                }
                Event::Start(_) => {
                    let mut semantic_value = String::new();
                    let closing_start = loop {
                        let before = pos(&reader);
                        match reader.read_event().map_err(|e| XmpError::Xml(e.to_string()))? {
                            Event::Text(text) => semantic_value.push_str(&xml_value(&text.decode().map_err(|e| XmpError::Xml(e.to_string()))?)?),
                            Event::CData(text) => semantic_value.push_str(&text.decode().map_err(|e| XmpError::Xml(e.to_string()))?),
                            Event::GeneralRef(reference) => {
                                let name = reference.decode().map_err(|e| XmpError::Xml(e.to_string()))?;
                                semantic_value.push_str(&xml_value(&format!("&{name};"))?);
                            }
                            Event::Comment(_) => {}
                            Event::End(_) => break before,
                            _ => return Err(bad()),
                        }
                    };
                    if let Some(value) = replacement(property, &semantic_value)? {
                        edits.push((prop.start + opening_end, prop.start + closing_start, value));
                    }
                }
                _ => return Err(bad()),
            }
        }
    }
    if extended_guid.is_some() && links_updated == 0 {
        return Err(XmpError::Xml("missing Extended XMP link".into()));
    }
    edits.sort_by_key(|(start, _, _)| *start);
    let mut out = String::with_capacity(packet.len());
    let mut at = 0;
    for (start, end, value) in edits {
        if start < at {
            return Err(bad());
        }
        out.push_str(&packet[at..start]);
        out.push_str(&value);
        at = end;
    }
    out.push_str(&packet[at..]);
    parse_xmp(&out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_normalization_preserves_foreign_bytes_and_resolves_namespaces() {
        let source = concat!(
            "<?xpacket begin='test'?>\n<x:xmpmeta xmlns:x='adobe:ns:meta/'>",
            "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>",
            "<r:Description xmlns:t='http://ns.adobe.com/tiff/1.0/' xmlns:e='http://ns.adobe.com/exif/1.0/' ",
            "xmlns:f='urn:foreign' f:Exact = 'a &amp; b' t:Orientation = '6' t:ImageWidth=\"99\" e:PixelYDimension='88'>",
            "<!-- t:Orientation='8' is a comment -->",
            "<t:ImageLength>88</t:ImageLength><e:PixelXDimension><![CDATA[99]]></e:PixelXDimension>",
            "<t:Orientation>8</t:Orientation><f:History><r:Seq><r:li>untouched</r:li></r:Seq></f:History>",
            "</r:Description>",
            "<r:Description xmlns:tiff='urn:foreign' tiff:Orientation='7'><tiff:ImageWidth>123</tiff:ImageWidth></r:Description>",
            "</r:RDF></x:xmpmeta>   \n<?xpacket end='w'?>"
        );
        let expected = source
            .replace("t:Orientation = '6'", "t:Orientation = '1'")
            .replace("t:ImageWidth=\"99\"", "t:ImageWidth=\"3\"")
            .replace("e:PixelYDimension='88'", "e:PixelYDimension='2'")
            .replace("<t:ImageLength>88", "<t:ImageLength>2")
            .replace("<![CDATA[99]]>", "3")
            .replace("<t:Orientation>8", "<t:Orientation>1");
        let out = normalize_xmp_geometry(source, 3, 2).unwrap();
        assert_eq!(out, expected);
        assert_eq!(normalize_xmp_geometry(&out, 3, 2).unwrap(), out);
        let absent = "<x:xmpmeta xmlns:x='adobe:ns:meta/'>original</x:xmpmeta>";
        assert_eq!(normalize_xmp_geometry(absent, 3, 2).unwrap(), absent);
        let default_namespace = "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><r:Description xmlns='http://ns.adobe.com/tiff/1.0/' ImageWidth='foreign'><ImageWidth>99</ImageWidth></r:Description></r:RDF>";
        assert_eq!(normalize_xmp_geometry(default_namespace, 3, 2).unwrap(), default_namespace.replace(">99<", ">3<"));
    }

    #[test]
    fn geometry_normalization_rejects_malformed_or_structured_geometry() {
        for packet in [
            "<a><b></a>",
            "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><r:Description xmlns:t='http://ns.adobe.com/tiff/1.0/'><t:Orientation r:resource='6'/></r:Description></r:RDF>",
            "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><r:Description xmlns:t='http://ns.adobe.com/tiff/1.0/'><t:Orientation><r:Seq><r:li>6</r:li></r:Seq></t:Orientation></r:Description></r:RDF>",
        ] {
            assert!(normalize_xmp_geometry(packet, 3, 2).is_err());
        }
    }

    #[test]
    fn geometry_and_extended_links_compare_xml_semantic_values() {
        let packet = "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns&#35;'><r:Description xmlns:t='http://ns.adobe.com/tiff/1.0&#x2F;' xmlns:f='urn:foreign' t:Orientation='&#54;' f:Exact='&#54; &amp; untouched'><t:ImageWidth>&#57;&#57;</t:ImageWidth></r:Description></r:RDF>";
        let expected = packet.replace("t:Orientation='&#54;'", "t:Orientation='1'").replace(">&#57;&#57;<", ">3<");
        assert_eq!(normalize_xmp_geometry(packet, 3, 2).unwrap(), expected);
        const OLD: &str = "A4AB635057D3BDE49D8FDAD7B888BC32";
        const NEW: &str = "1C1A85B3C7F85F31F24FF5B1CCADE1C3";
        for scalar in [format!("<![CDATA[{OLD}]]>"), OLD.replacen('A', "&#65;", 1), format!("<![CDATA[A4AB]]>{}", &OLD[4..])] {
            let packet = format!(
                "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><r:Description xmlns:n='http://ns.adobe.com/xmp/note&#47;' xmlns:f='urn:foreign' f:Exact='&#65;'><n:HasExtendedXMP>{scalar}</n:HasExtendedXMP></r:Description></r:RDF>"
            );
            assert!(xmp_has_extended_guid(&packet).unwrap());
            assert_eq!(normalize_xmp_geometry_with_extended_guid(&packet, 3, 2, OLD, NEW).unwrap(), packet.replace(&scalar, NEW));
        }
        let escaped = OLD.replacen('A', "&#x41;", 1);
        let packet = format!(
            "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><r:Description xmlns:n='http://ns.adobe.com/xmp/note&#x2F;' xmlns:f='urn:foreign' n:HasExtendedXMP='{escaped}' f:Exact='{escaped}'/></r:RDF>"
        );
        let expected = packet.replace(&format!("n:HasExtendedXMP='{escaped}'"), &format!("n:HasExtendedXMP='{NEW}'"));
        assert!(xmp_has_extended_guid(&packet).unwrap());
        assert_eq!(normalize_xmp_geometry_with_extended_guid(&packet, 3, 2, OLD, NEW).unwrap(), expected);
        assert!(normalize_xmp_geometry_with_extended_guid(&packet, 3, 2, NEW, OLD).is_err());
    }

    #[test]
    fn foreign_and_nested_rdf_resources_are_not_image_geometry() {
        let packet = "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#' xmlns:f='urn:foreign-props' xmlns:rdf='urn:foreign' xmlns:t='http://ns.adobe.com/tiff/1.0/'><r:Description><f:History r:parseType='Resource'><rdf:RDF><rdf:Description t:Orientation='6'/></rdf:RDF></f:History></r:Description></r:RDF>";
        assert_eq!(normalize_xmp_geometry(packet, 3, 2).unwrap(), packet);
        let nested = packet.replace("xmlns:rdf='urn:foreign'", "xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'");
        assert_eq!(normalize_xmp_geometry(&nested, 3, 2).unwrap(), nested);
    }

    #[test]
    fn unchanged_creator_sequence_keeps_xml_but_same_display_scalar_edit_replaces_it() {
        let creator = "<d:creator>\n <r:Seq><r:li>Ann</r:li><!-- order matters --><r:li>Bob</r:li></r:Seq>\n</d:creator>";
        let original = format!(
            "<r:RDF xmlns:r='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><r:Description xmlns:d='http://purl.org/dc/elements/1.1/' xmlns:f='urn:foreign' f:Exact='keep &amp; me'>{creator}<f:Thing>foreign</f:Thing></r:Description></r:RDF>"
        );
        let rules = MergeRules { owned: &["dc:creator", "xmp:Rating"], ..Default::default() };
        let mut metadata = parse_xmp(&original).unwrap().metadata;
        metadata.rating = Some(5);
        let saved = merge_xmp(&original, &crate::write_xmp(&metadata, None), rules).unwrap();
        assert!(saved.contains(creator));
        assert!(saved.contains("f:Exact='keep &amp; me'"));
        assert_eq!(parse_xmp(&saved).unwrap().properties["dc:creator"], vec!["Ann", "Bob"]);
        metadata.set_artist(Some("Ann; Bob".into()));
        let edited = merge_xmp(&saved, &crate::write_xmp(&metadata, None), rules).unwrap();
        assert_eq!(parse_xmp(&edited).unwrap().properties["dc:creator"], vec!["Ann; Bob"]);
        assert!(!edited.contains(creator));
        metadata.set_artist(None);
        let cleared = merge_xmp(&edited, &crate::write_xmp(&metadata, None), rules).unwrap();
        assert!(!parse_xmp(&cleared).unwrap().properties.contains_key("dc:creator"));
        assert!(cleared.contains("<f:Thing>foreign</f:Thing>"));
    }

    const RULES: MergeRules =
        MergeRules { owned: &["xmp:Rating", "xmp:Label", "dc:subject", "dc:title", "lc:*"], owned_if_present: &["exif:DateTimeOriginal"] };

    /// A sidecar as another raw developer writes it: attribute-form simple properties, develop
    /// settings, a structured history, an unknown namespace.
    const FOREIGN: &str = "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"Some Toolkit 1.0\">
 <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">
  <rdf:Description rdf:about=\"\"
    xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"
    xmlns:xmpMM=\"http://ns.adobe.com/xap/1.0/mm/\"
    xmlns:stEvt=\"http://ns.adobe.com/xap/1.0/sType/ResourceEvent#\"
    xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\"
    xmlns:dc=\"http://purl.org/dc/elements/1.1/\"
    xmlns:foo=\"http://example.com/foo/\"
    xmp:Rating=\"2\"
    xmp:CreatorTool=\"Other App\"
    crs:Exposure2012=\"+0.50\"
    crs:Contrast2012=\"-12\"
    foo:Thing=\"a &amp; b\">
   <xmpMM:History>
    <rdf:Seq>
     <rdf:li stEvt:action=\"saved\" stEvt:when=\"2026-01-02T03:04:05\"/>
    </rdf:Seq>
   </xmpMM:History>
   <crs:ToneCurvePV2012>
    <rdf:Seq>
     <rdf:li>0, 0</rdf:li>
     <rdf:li>255, 255</rdf:li>
    </rdf:Seq>
   </crs:ToneCurvePV2012>
   <dc:subject>
    <rdf:Bag>
     <rdf:li>old</rdf:li>
    </rdf:Bag>
   </dc:subject>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
";

    /// With the packet's whitespace padding and trailer.
    fn foreign() -> String {
        format!("{FOREIGN}{}\n<?xpacket end=\"w\"?>", " ".repeat(100))
    }

    fn fresh(rating: i8, keywords: &[&str]) -> String {
        let m = crate::Metadata {
            software: Some("Local Image".into()),
            rating: Some(rating),
            keywords: keywords.iter().map(|k| k.to_string()).collect(),
            ..Default::default()
        };
        crate::write_xmp_lc(&m, &[("settings", "{\"a\":1}"), ("flag", "pick")])
    }

    #[test]
    fn keeps_foreign_properties_and_replaces_owned_ones() {
        let out = merge_xmp(&foreign(), &fresh(4, &["new", "two"]), RULES).unwrap();
        let d = parse_xmp(&out).unwrap();
        let p = |k: &str| d.properties.get(k).cloned().unwrap_or_default();
        assert_eq!(p("xmp:Rating"), vec!["4"]);
        assert_eq!(p("dc:subject"), vec!["new", "two"]);
        assert_eq!(p("lc:settings"), vec!["{\"a\":1}"]);
        assert_eq!(p("lc:flag"), vec!["pick"]);
        assert_eq!(p("xmp:CreatorTool"), vec!["Other App"], "not ours: kept");
        assert_eq!(p("crs:Exposure2012"), vec!["+0.50"]);
        assert_eq!(p("foo:Thing").len(), 1);
        // untouched parts are byte for byte the same
        for part in [
            "   <xmpMM:History>\n    <rdf:Seq>\n     <rdf:li stEvt:action=\"saved\" stEvt:when=\"2026-01-02T03:04:05\"/>\n    </rdf:Seq>\n   </xmpMM:History>",
            "   <crs:ToneCurvePV2012>\n    <rdf:Seq>\n     <rdf:li>0, 0</rdf:li>",
            "crs:Contrast2012=\"-12\"",
            "foo:Thing=\"a &amp; b\"",
            "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"Some Toolkit 1.0\">",
            &format!("</x:xmpmeta>\n{}\n<?xpacket end=\"w\"?>", " ".repeat(100)),
        ] {
            assert!(out.contains(part), "lost {part:?} in\n{out}");
        }
        assert!(!out.contains("xmp:Rating=\"2\""));
        assert!(!out.contains("<rdf:li>old</rdf:li>"));
        // saving again changes only our part, and doesn't pile up descriptions
        let again = merge_xmp(&out, &fresh(1, &[]), RULES).unwrap();
        let d = parse_xmp(&again).unwrap();
        assert_eq!(d.properties.get("xmp:Rating"), Some(&vec!["1".to_string()]));
        assert!(!d.properties.contains_key("dc:subject"), "a cleared field is cleared");
        assert_eq!(again.matches("<rdf:Description").count(), 2, "{again}");
        assert_eq!(merge_xmp(&again, &fresh(1, &[]), RULES).unwrap(), again, "idempotent");
    }

    #[test]
    fn owned_if_present_and_prefix_independence() {
        // the existing packet binds the namespaces to other prefixes, and has a capture time
        let ex = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><r:RDF xmlns:r=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
            <r:Description r:about=\"\" xmlns:a=\"http://ns.adobe.com/xap/1.0/\" xmlns:e=\"http://ns.adobe.com/exif/1.0/\" \
            a:Rating=\"5\" e:DateTimeOriginal=\"2020-01-01T00:00:00\"/></r:RDF></x:xmpmeta>";
        let out = merge_xmp(ex, &fresh(3, &[]), RULES).unwrap();
        let d = parse_xmp(&out).unwrap();
        assert_eq!(d.properties.get("xmp:Rating"), Some(&vec!["3".to_string()]));
        assert_eq!(d.properties.get("exif:DateTimeOriginal"), Some(&vec!["2020-01-01T00:00:00".to_string()]), "fresh has none: kept");
        let mut m = crate::Metadata { rating: Some(3), ..Default::default() };
        m.capture_time = crate::DateTime::parse_iso("2024-05-06T07:08:09");
        let out = merge_xmp(ex, &crate::write_xmp_lc(&m, &[]), RULES).unwrap();
        let d = parse_xmp(&out).unwrap();
        assert_eq!(d.properties.get("exif:DateTimeOriginal"), Some(&vec!["2024-05-06T07:08:09".to_string()]));
    }

    #[test]
    fn many_attributes_scan_in_linear_time() {
        // Upstream LightCraft 87cd873: quick-xml's duplicate-name check compares each attribute with all
        // earlier ones; 200 000 attributes on one description took over 20 s to scan.
        let n = 200_000;
        let mut s = String::from(
            "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
             <rdf:Description rdf:about=\"\" xmlns:foo=\"http://example.com/foo/\"",
        );
        for i in 0..n {
            s.push_str(&format!(" foo:a{i}=\"{i}\""));
        }
        s.push_str("/></rdf:RDF></x:xmpmeta>");
        let t = std::time::Instant::now();
        let found = scan(&s).unwrap();
        let took = t.elapsed();
        assert_eq!(found.descs[0].attrs.len(), n + 2, "every attribute is read");
        assert!(took < std::time::Duration::from_secs(5), "{n} attributes took {took:?}");
    }

    #[test]
    fn refuses_a_repeated_attribute() {
        // Not well-formed XML: no merge, so the caller keeps a backup. An owned name, a foreign one and one
        // on a nested element (a merge would keep both copies of the last two).
        for (once, twice) in [
            ("xmp:Rating=\"2\"", "xmp:Rating=\"2\" xmp:Rating=\"3\""),
            ("foo:Thing=\"a &amp; b\"", "foo:Thing=\"a &amp; b\" foo:Thing=\"c\""),
            ("stEvt:action=\"saved\"", "stEvt:action=\"saved\" stEvt:action=\"saved\""),
        ] {
            let ex = foreign().replace(once, twice);
            assert!(merge_xmp(&ex, &fresh(1, &[]), RULES).is_err(), "{twice}");
        }
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        for bad in ["", "not xml at all", "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF", "<a><b></a>", "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>"] {
            assert!(merge_xmp(bad, &fresh(1, &[]), RULES).is_err(), "{bad:?}");
        }
    }
    #[test]
    fn preserves_foreign_properties_using_known_prefixes() {
        for property in [r#"lc:Secret="keep-me"/>"#, r#"><lc:Secret>keep-me</lc:Secret></rdf:Description>"#] {
            let desc = format!(r#"<rdf:Description rdf:about="" xmlns:lc="https://example.org/vendor/" {property}"#);
            let old = format!(r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">{desc}</rdf:RDF>"#);
            let out = merge_xmp(&old, &fresh(4, &[]), RULES).unwrap();
            assert!(out.contains(&desc), "lost foreign description: {out}");
            assert_eq!(parse_xmp(&out).unwrap().metadata.rating, Some(4));
            assert_eq!(merge_xmp(&out, &fresh(4, &[]), RULES).unwrap(), out);
        }
    }
    #[test]
    fn namespace_aliases_remain_distinct_during_merge() {
        // The alias URI includes a character reference: scanning and parsing must resolve it identically.
        let foreign = r#"<rdf:Description xmlns:xmp="https://example.org/vendor/" xmp:Rating="2"/>"#;
        let old = format!(
            r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">{foreign}
            <rdf:Description xmlns:a="http://ns.adobe.com/xap/1.0&#47;" a:Rating="3"/>
            </rdf:RDF>"#
        );
        let out = merge_xmp(&old, &fresh(5, &[]), RULES).unwrap();
        assert!(out.contains(foreign), "lost foreign rating: {out}");
        assert!(!out.contains("a:Rating="), "genuine alias must be replaced: {out}");
        assert_eq!(parse_xmp(&out).unwrap().metadata.rating, Some(5));
        assert_eq!(merge_xmp(&out, &fresh(5, &[]), RULES).unwrap(), out);
    }

    #[test]
    fn opaque_namespace_expansion_is_bounded() {
        let uri = format!("https://example.org/{}", "a".repeat(1024));
        for body in [
            format!(r#"<rdf:Description xmlns:lc="{uri}">{}</rdf:Description>"#, "<lc:p/>".repeat(10_000)),
            format!(r#"<rdf:Description xmlns:lc="{uri}" {}/>"#, (0..10_000).map(|i| format!(r#"lc:p{i}="value" "#)).collect::<String>()),
        ] {
            let packet = format!(r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">{body}</rdf:RDF>"#);
            assert!(matches!(parse_xmp(&packet), Err(XmpError::TooComplex)));
            assert!(matches!(scan(&packet), Err(XmpError::TooComplex)));
        }
    }
}
