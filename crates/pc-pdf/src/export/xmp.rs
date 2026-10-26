//! File Info metadata for the PDF writer. The post-pass accepts only krilla's own plain-xref
//! output; it edits the uncompressed metadata stream and shifts subsequent object offsets.

use super::{ExportError, Result};

/// The File Info fields delivered with an editor PDF. Other image metadata is excluded.
#[derive(Clone, Debug, Default)]
pub struct FileInfo {
    pub author: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub copyright_marked: Option<bool>,
    pub copyright: String,
    pub copyright_url: String,
}

fn xml_character(character: &char) -> bool {
    matches!(character, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

pub(super) fn metadata(title: &str, info: &FileInfo, budget: usize) -> Result<krilla::metadata::Metadata> {
    let size = [title, &info.author, &info.description, &info.copyright, &info.copyright_url]
        .into_iter().chain(info.keywords.iter().map(String::as_str))
        .try_fold(0usize, |sum, value| sum.checked_add(value.len()))
        .ok_or(ExportError::Budget("metadata bytes"))?;
    if size > budget {
        return Err(ExportError::Budget("metadata bytes"));
    }
    let clean = |value: &str| value.chars().filter(xml_character).collect::<String>();
    let mut metadata = krilla::metadata::Metadata::new().title(clean(title)).creator("Local Image".into());
    if !info.author.is_empty() {
        metadata = metadata.authors(vec![clean(&info.author)]);
    }
    Ok(metadata.description(clean(&info.description)).keywords(info.keywords.iter().map(|keyword| clean(keyword)).collect()))
}

fn xml_text(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars().filter(xml_character) {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\r' => output.push_str("&#xD;"),
            _ => output.push(character),
        }
    }
    output
}

fn properties(info: &FileInfo) -> String {
    let mut output = String::new();
    if !info.keywords.is_empty() {
        output.push_str("<dc:subject xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><rdf:Bag>");
        for keyword in &info.keywords {
            output.push_str(&format!("<rdf:li>{}</rdf:li>", xml_text(keyword)));
        }
        output.push_str("</rdf:Bag></dc:subject>");
    }
    let notice = info.copyright.trim();
    if !notice.is_empty() {
        output.push_str(&format!("<dc:rights xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:rights>", xml_text(notice)));
    }
    let url = info.copyright_url.trim();
    if !url.is_empty() {
        output.push_str(&format!("<xmpRights:WebStatement xmlns:xmpRights=\"http://ns.adobe.com/xap/1.0/rights/\">{}</xmpRights:WebStatement>", xml_text(url)));
    }
    if let Some(marked) = info.copyright_marked {
        output.push_str(&format!("<xmpRights:Marked xmlns:xmpRights=\"http://ns.adobe.com/xap/1.0/rights/\">{}</xmpRights:Marked>", if marked { "True" } else { "False" }));
    }
    output
}

fn failed() -> ExportError {
    ExportError::Write("the PDF writer's metadata or cross-reference layout is unsupported".into())
}

fn find(bytes: &[u8], needle: &[u8]) -> Result<usize> {
    bytes.windows(needle.len()).position(|window| window == needle).ok_or_else(failed)
}

fn number(value: &[u8]) -> Result<usize> {
    std::str::from_utf8(value).map_err(|_| failed())?.trim().parse().map_err(|_| failed())
}

/// Add rights and the structured keyword bag without changing any page or stream contents.
pub(super) fn apply(pdf: Vec<u8>, info: &FileInfo, budget: usize) -> Result<Vec<u8>> {
    let properties = properties(info);
    if properties.is_empty() {
        return Ok(pdf);
    }
    let startxref = pdf.windows(b"startxref\n".len()).rposition(|window| window == b"startxref\n").ok_or_else(failed)?;
    let pointer_start = startxref + b"startxref\n".len();
    let pointer_end = pointer_start + find(&pdf[pointer_start..], b"\n")?;
    let xref_start = number(&pdf[pointer_start..pointer_end])?;
    let xref = pdf.get(xref_start..startxref).ok_or_else(failed)?;
    if !xref.starts_with(b"xref\n0 ") {
        return Err(failed());
    }
    let entries_start = find(&xref[5..], b"\n")? + 6;
    let count = number(&xref[7..entries_start - 1])?;
    let entries_end = count.checked_mul(20).and_then(|length| entries_start.checked_add(length)).ok_or_else(failed)?;
    let entries = xref.get(entries_start..entries_end).ok_or_else(failed)?;
    let trailer = xref.get(entries_end..).ok_or_else(failed)?;
    if !trailer.starts_with(b"trailer\n") {
        return Err(failed());
    }
    let root_start = find(trailer, b"/Root ")? + 6;
    let root_end = root_start + find(&trailer[root_start..], b" ")?;
    let root_id = number(&trailer[root_start..root_end])?;
    let offset = |id: usize| -> Result<usize> {
        let start = id.checked_mul(20).ok_or_else(failed)?;
        let entry = entries.get(start..start.checked_add(20).ok_or_else(failed)?).ok_or_else(failed)?;
        if entry.get(17) != Some(&b'n') {
            return Err(failed());
        }
        number(&entry[..10])
    };
    let root_offset = offset(root_id)?;
    let root = pdf.get(root_offset..xref_start).ok_or_else(failed)?;
    let root_end = find(root, b"\nendobj")?;
    let root = &root[..root_end];
    let metadata_id_start = find(root, b"/Metadata ")? + 10;
    let metadata_id_end = metadata_id_start + find(&root[metadata_id_start..], b" ")?;
    let metadata_offset = offset(number(&root[metadata_id_start..metadata_id_end])?)?;
    let metadata = pdf.get(metadata_offset..xref_start).ok_or_else(failed)?;
    let stream_start = find(metadata, b"\nstream\n")? + 8;
    let dictionary = &metadata[..stream_start];
    find(dictionary, b"/Type/Metadata")?;
    find(dictionary, b"/Subtype/XML")?;
    let length_start = find(dictionary, b"/Length ")? + 8;
    let length_end = length_start + dictionary[length_start..].iter().position(|byte| !byte.is_ascii_digit()).ok_or_else(failed)?;
    let old_length = number(&dictionary[length_start..length_end])?;
    let stream_end = stream_start.checked_add(old_length).ok_or_else(failed)?;
    let stream = metadata.get(stream_start..stream_end).ok_or_else(failed)?;
    if !metadata.get(stream_end..).is_some_and(|suffix| suffix.starts_with(b"\nendstream")) {
        return Err(failed());
    }
    let insertion = metadata_offset + stream_start + find(stream, b"</rdf:Description>")?;
    let length_start = metadata_offset + length_start;
    let length_end = metadata_offset + length_end;
    let new_length = old_length.checked_add(properties.len()).ok_or(ExportError::Budget("output bytes"))?.to_string();
    let growth = properties.len().checked_add(new_length.len()).and_then(|size| size.checked_sub(length_end - length_start)).ok_or_else(failed)?;
    let new_xref = xref_start.checked_add(growth).ok_or(ExportError::Budget("output bytes"))?;
    let new_pointer = new_xref.to_string();
    let output_size = pdf.len().checked_add(growth).and_then(|size| size.checked_add(new_pointer.len())).and_then(|size| size.checked_sub(pointer_end - pointer_start)).ok_or(ExportError::Budget("output bytes"))?;
    if output_size > budget {
        return Err(ExportError::Budget("output bytes"));
    }
    let mut output = Vec::new();
    output.try_reserve_exact(output_size).map_err(|error| ExportError::Write(format!("not enough memory for PDF metadata: {error}")))?;
    output.extend_from_slice(&pdf[..length_start]);
    output.extend_from_slice(new_length.as_bytes());
    output.extend_from_slice(&pdf[length_end..insertion]);
    output.extend_from_slice(properties.as_bytes());
    output.extend_from_slice(&pdf[insertion..xref_start]);
    output.extend_from_slice(&xref[..entries_start]);
    for entry in entries.as_chunks::<20>().0 {
        let original = number(&entry[..10])?;
        let shifted = if entry[17] == b'n' && original > metadata_offset { original.checked_add(growth).ok_or_else(failed)? } else { original };
        if shifted > 9_999_999_999usize {
            return Err(failed());
        }
        output.extend_from_slice(format!("{shifted:010}").as_bytes());
        output.extend_from_slice(&entry[10..]);
    }
    output.extend_from_slice(trailer);
    output.extend_from_slice(&pdf[startxref..pointer_start]);
    output.extend_from_slice(new_pointer.as_bytes());
    output.extend_from_slice(&pdf[pointer_end..]);
    Ok(output)
}

#[cfg(test)]
#[path = "xmp_tests.rs"]
mod tests;
