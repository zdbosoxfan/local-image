//! Smart Sort export path name tokens. Pure engine logic: patterns are expanded from
//! [`TokenValues`] and sanitized per path segment; callers build the final export plan.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TokenValues {
    pub event: String,
    pub folder: String,
    pub person: Option<String>,
    pub session: Option<String>,
    pub captured: Option<String>,
    pub camera: Option<String>,
    pub original: Option<String>,
    pub seq: Option<u32>,
}

pub const DEFAULT_FOLDER_PATTERN: &str = "{event}/{folder}";

const BASE_TOKENS: &[&str] = &["event", "folder", "person", "session", "date", "camera"];

pub fn validate(pattern: &str, allow_file_tokens: bool) -> Result<(), String> {
    if allow_file_tokens && (pattern.contains('/') || pattern.contains('\\')) {
        return Err("file pattern cannot contain '/'".into());
    }
    if pattern.starts_with('/') || pattern.starts_with('\\') {
        return Err("folder pattern must be relative".into());
    }
    for token in placeholders(pattern)? {
        validate_token(&token, allow_file_tokens)?;
    }
    Ok(())
}

fn validate_token(token: &str, allow_file_tokens: bool) -> Result<(), String> {
    if !allow_file_tokens {
        if token == "seq" || token.starts_with("seq:") || token == "original" {
            return Err(format!("token not allowed in folder pattern: {{{token}}}"));
        }
        if !BASE_TOKENS.contains(&token) {
            return Err(format!("unknown token {{{token}}}"));
        }
        return Ok(());
    }

    if token == "seq" || token == "original" {
        return Ok(());
    }

    if let Some(width) = token.strip_prefix("seq:") {
        let width: usize = width.parse().map_err(|_| format!("invalid sequence padding {{{token}}}"))?;
        if !(1..=9).contains(&width) {
            return Err(format!("sequence padding must be 1..=9 in {{{token}}}"));
        }
        return Ok(());
    }

    if !BASE_TOKENS.contains(&token) {
        return Err(format!("unknown token {{{token}}}"));
    }
    Ok(())
}

pub fn expand_folder(pattern: &str, v: &TokenValues) -> Result<PathBuf, String> {
    validate(pattern, false)?;
    let mut path = PathBuf::new();
    for segment in pattern.split('/') {
        let expanded = replace_tokens(segment, v, false)?;
        let cleaned = sanitize_component(&expanded);
        if !cleaned.is_empty() {
            path.push(cleaned);
        }
    }
    Ok(path)
}

pub fn expand_file(pattern: &str, v: &TokenValues) -> Result<String, String> {
    validate(pattern, true)?;
    let expanded = replace_tokens(pattern, v, true)?;
    Ok(sanitize_component(&expanded))
}

pub fn dedupe_paths(paths: Vec<(String, PathBuf)>) -> Vec<(String, PathBuf)> {
    let mut used = std::collections::BTreeSet::new();
    paths
        .into_iter()
        .map(|(key, path)| {
            let lower = path.to_string_lossy().to_lowercase();
            if used.insert(lower) {
                return (key, path);
            }
            let mut n = 2;
            loop {
                let candidate = add_suffix(&path, n);
                let candidate_lower = candidate.to_string_lossy().to_lowercase();
                if used.insert(candidate_lower) {
                    return (key, candidate);
                }
                n += 1;
            }
        })
        .collect()
}

fn placeholders(pattern: &str) -> Result<Vec<String>, String> {
    let bytes = pattern.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b'}' {
                    j += 1;
                }
                if j == bytes.len() {
                    return Err("unclosed token placeholder".into());
                }
                let token = std::str::from_utf8(&bytes[i + 1..j]).map_err(|_| "invalid token placeholder".to_string())?;
                if token.is_empty() {
                    return Err("empty token placeholder".into());
                }
                out.push(token.to_string());
                i = j + 1;
            }
            b'}' => return Err("unmatched '}'".into()),
            _ => i += 1,
        }
    }
    Ok(out)
}

fn replace_tokens(pattern: &str, v: &TokenValues, allow_file: bool) -> Result<String, String> {
    let bytes = pattern.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let start = i;
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j == bytes.len() {
                return Err("unclosed token placeholder".into());
            }
            let token = std::str::from_utf8(&bytes[start + 1..j]).map_err(|_| "invalid token placeholder".to_string())?;
            out.push_str(&token_value(token, v, allow_file)?);
            i = j + 1;
        } else if bytes[i] == b'}' {
            return Err("unmatched '}'".into());
        } else {
            let next = pattern[i..].find('{').map(|p| i + p).unwrap_or(bytes.len());
            out.push_str(&pattern[i..next]);
            i = next;
        }
    }
    Ok(out)
}

fn token_value(token: &str, v: &TokenValues, allow_file: bool) -> Result<String, String> {
    match token {
        "event" => Ok(v.event.clone()),
        "folder" => Ok(v.folder.clone()),
        "person" => Ok(v.person.clone().unwrap_or_default()),
        "session" => Ok(v.session.clone().unwrap_or_default()),
        "date" => Ok(capture_date(v.captured.as_deref())),
        "camera" => Ok(v.camera.as_deref().filter(|s| !s.is_empty()).unwrap_or("Unknown camera").to_string()),
        "original" if allow_file => Ok(v.original.clone().unwrap_or_default()),
        "seq" if allow_file => seq_value(v.seq, None),
        _ if allow_file && token.starts_with("seq:") => {
            let width = token.strip_prefix("seq:").unwrap();
            let width: usize = width.parse().map_err(|_| format!("invalid sequence padding {{{token}}}"))?;
            if !(1..=9).contains(&width) {
                return Err(format!("sequence padding must be 1..=9 in {{{token}}}"));
            }
            seq_value(v.seq, Some(width))
        }
        "original" => Err(format!("token not allowed in folder pattern: {token}")),
        "seq" => Err(format!("token not allowed in folder pattern: {token}")),
        _ if token.starts_with("seq:") => Err(format!("token not allowed in folder pattern: {token}")),
        _ => Err(format!("unknown token {{{token}}}")),
    }
}

fn capture_date(captured: Option<&str>) -> String {
    match captured {
        Some(s) if s.len() >= 10 && s.as_bytes().get(4) == Some(&b'-') && s.as_bytes().get(7) == Some(&b'-') => s[..10].to_string(),
        _ => "No date".into(),
    }
}

fn seq_value(seq: Option<u32>, width: Option<usize>) -> Result<String, String> {
    match seq {
        Some(n) => match width {
            Some(w) => Ok(format!("{:0width$}", n, width = w)),
            None => Ok(n.to_string()),
        },
        None => Err("sequence token requires a sequence number".into()),
    }
}

fn sanitize_component(s: &str) -> String {
    let cleaned: String = s.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    cleaned.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string()
}

fn add_suffix(path: &Path, suffix: usize) -> PathBuf {
    let mut p = path.to_path_buf();
    let suffix = format!(" ({suffix})");
    if let Some(name) = p.file_name() {
        let mut new_name = name.to_os_string();
        new_name.push(suffix.as_str());
        p.set_file_name(new_name);
    } else {
        p.push(suffix);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn each_token_expands() {
        let v = TokenValues {
            event: "Event".into(),
            folder: "Folder".into(),
            person: Some("Person".into()),
            session: Some("Session".into()),
            captured: Some("2026-10-05T12:34:56".into()),
            camera: Some("Sony A7".into()),
            original: Some("IMG_0001".into()),
            seq: Some(7),
        };
        assert_eq!(
            norm(&expand_folder("{event}/{folder}/{person}/{session}/{date}/{camera}", &v).unwrap()),
            "Event/Folder/Person/Session/2026-10-05/Sony A7"
        );
        assert_eq!(
            expand_file("{event}_{folder}_{person}_{session}_{date}_{camera}_{seq}_{original}", &v).unwrap(),
            "Event_Folder_Person_Session_2026-10-05_Sony A7_7_IMG_0001"
        );
    }

    #[test]
    fn date_and_camera_defaults() {
        let v = TokenValues { event: "E".into(), folder: "F".into(), ..Default::default() };
        assert_eq!(norm(&expand_folder("{date}/{camera}", &v).unwrap()), "No date/Unknown camera");
    }

    #[test]
    fn empty_values_drop_folder_segments() {
        let v = TokenValues { event: "E".into(), folder: "F".into(), ..Default::default() };
        assert_eq!(norm(&expand_folder("{event}/{person}/{session}/{folder}", &v).unwrap()), "E/F");
    }

    #[test]
    fn sequence_padding() {
        let v = TokenValues { event: "E".into(), folder: "F".into(), seq: Some(7), ..Default::default() };
        assert_eq!(expand_file("{seq}", &v).unwrap(), "7");
        assert_eq!(expand_file("{seq:1}", &v).unwrap(), "7");
        assert_eq!(expand_file("{seq:4}", &v).unwrap(), "0007");
        assert!(validate("{seq:0}", true).is_err());
        assert!(validate("{seq:10}", true).is_err());
    }

    #[test]
    fn unsafe_chars_are_sanitized_per_segment() {
        let v = TokenValues { event: "Event: side?".into(), folder: "F\\lder*".into(), ..Default::default() };
        let path = expand_folder("{event}/{folder}", &v).unwrap();
        assert_eq!(norm(&path), "Event_ side_/F_lder_");
    }

    #[test]
    fn unknown_token_is_an_error() {
        let err = validate("{event}/{wat}", false).unwrap_err();
        assert!(err.contains("unknown token {wat}"));
        let err = validate("{event}_{wat}", true).unwrap_err();
        assert!(err.contains("unknown token {wat}"));
    }

    #[test]
    fn dotdot_cannot_escape() {
        let v = TokenValues { event: "..".into(), folder: "F".into(), ..Default::default() };
        assert_eq!(norm(&expand_folder("{event}/{folder}", &v).unwrap()), "F");
        assert_eq!(norm(&expand_folder("..", &v).unwrap()), "");
    }

    #[test]
    fn file_pattern_rejects_slash() {
        assert!(validate("{event}/{folder}", true).is_err());
    }

    #[test]
    fn dedupe_adds_suffix_case_insensitively() {
        let paths = vec![
            ("one".to_string(), PathBuf::from("one/two")),
            ("two".to_string(), PathBuf::from("one/two")),
            ("three".to_string(), PathBuf::from("ONE/two")),
            ("four".to_string(), PathBuf::from("one/three")),
        ];
        let out = dedupe_paths(paths);
        assert_eq!(out[0].0, "one");
        assert_eq!(norm(&out[0].1), "one/two");
        assert_eq!(out[1].0, "two");
        assert_eq!(norm(&out[1].1), "one/two (2)");
        assert_eq!(out[2].0, "three");
        assert_eq!(norm(&out[2].1), "ONE/two (3)");
        assert_eq!(out[3].0, "four");
        assert_eq!(norm(&out[3].1), "one/three");
    }
}
