//! Audit every serde enum in the document, geometry, colour and native-manifest sources.
//! Tag scopes are JSON objects: a named field starts a new scope, whereas internally tagged
//! newtypes and #[serde(flatten)] merge into their parent (the repeat/envelope failure case).
//! This small lexer reads source, never serializes to Value (which would erase duplicate keys).
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
struct Member {
    name: String,
    attrs: Vec<String>,
    body: Vec<String>,
}
#[derive(Clone, Debug)]
struct Model {
    file: PathBuf,
    name: String,
    attrs: Vec<String>,
    members: Vec<Member>,
    is_enum: bool,
}
fn tokens(s: &str) -> Vec<String> {
    let mut chars = s.chars().peekable();
    let mut out = Vec::new();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c == '/' && chars.peek() == Some(&'/') {
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
            continue;
        }
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut depth = 1;
            while let Some(c) = chars.next() {
                if c == '/' && chars.peek() == Some(&'*') {
                    chars.next();
                    depth += 1;
                }
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            continue;
        }
        if c == '\'' {
            if chars.clone().nth(1) == Some('\'') {
                chars.next();
                chars.next();
                out.push("char_literal".into());
                continue;
            }
            if chars.peek() == Some(&'\\') && chars.clone().nth(2) == Some('\'') {
                chars.next();
                chars.next();
                chars.next();
                out.push("char_literal".into());
                continue;
            }
        }
        if c == '"' {
            let mut token = String::from("\"");
            while let Some(c) = chars.next() {
                token.push(c);
                if c == '\\' {
                    token.push(chars.next().unwrap());
                } else if c == '"' {
                    break;
                }
            }
            out.push(token);
            continue;
        }
        if c.is_alphanumeric() || c == '_' {
            let mut token = c.to_string();
            while chars.peek().is_some_and(|c| c.is_alphanumeric() || *c == '_') {
                token.push(chars.next().unwrap());
            }
            out.push(token);
        } else {
            out.push(c.to_string());
        }
    }
    out
}
fn closing(t: &[String], at: usize) -> usize {
    let end = match t[at].as_str() {
        "{" => "}",
        "[" => "]",
        "(" => ")",
        "<" => ">",
        _ => panic!("not an opening delimiter"),
    };
    let mut n = 0;
    for i in at..t.len() {
        if t[i] == t[at] {
            n += 1;
        }
        if t[i] == end {
            n -= 1;
            if n == 0 {
                return i;
            }
        }
    }
    panic!("unbalanced source delimiter");
}
fn attributes(t: &[String], at: &mut usize) -> Vec<String> {
    let mut out = Vec::new();
    while t.get(*at).is_some_and(|s| s == "#") && t.get(*at + 1).is_some_and(|s| s == "[") {
        let end = closing(t, *at + 1);
        out.push(t[*at + 2..end].join(" "));
        *at = end + 1;
    }
    out
}
fn members(t: &[String]) -> Vec<Member> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < t.len() {
        let attrs = attributes(t, &mut at);
        if t.get(at).is_some_and(|s| s == "pub") {
            at += 1;
            if t.get(at).is_some_and(|s| s == "(") {
                at = closing(t, at) + 1;
            }
        }
        let Some(name) = t.get(at).cloned() else { break };
        at += 1;
        let start = at;
        while at < t.len() && t[at] != "," {
            if ["{", "[", "(", "<"].contains(&t[at].as_str()) {
                at = closing(t, at) + 1;
            } else {
                at += 1;
            }
        }
        out.push(Member { name, attrs, body: t[start..at].to_vec() });
        at += 1;
    }
    out
}
fn parse(file: PathBuf, s: &str) -> Vec<Model> {
    // Test-only enums aren't part of the storage schema.
    let s = s.split("#[cfg(test)]").next().unwrap();
    let t = tokens(s);
    let mut out = Vec::new();
    let mut attrs = Vec::new();
    let mut at = 0;
    while at < t.len() {
        if t[at] == "#" && t.get(at + 1).is_some_and(|s| s == "[") {
            attrs.extend(attributes(&t, &mut at));
            continue;
        }
        if ["enum", "struct"].contains(&t[at].as_str()) {
            let is_enum = t[at] == "enum";
            let name = t[at + 1].clone();
            at += 2;
            while at < t.len() && !["{", "(", ";"].contains(&t[at].as_str()) {
                at += 1;
            }
            if at == t.len() {
                break;
            }
            if t[at] == ";" {
                attrs.clear();
                at += 1;
                continue;
            }
            let end = closing(&t, at);
            if attrs.iter().any(|a| a.contains("Serialize") || a.contains("Deserialize")) {
                out.push(Model { file: file.clone(), name, attrs: attrs.clone(), members: members(&t[at + 1..end]), is_enum });
            }
            attrs.clear();
            at = end + 1;
            continue;
        }
        if ["fn", "impl", "use", "mod", "const", "type", "static"].contains(&t[at].as_str()) {
            attrs.clear();
        }
        at += 1;
    }
    out
}
fn values(attrs: &[String], key: &str) -> Vec<String> {
    attrs
        .iter()
        .filter(|a| a.starts_with("serde "))
        .flat_map(|a| {
            let t = tokens(a);
            t.windows(3).filter(|w| w[0] == key && w[1] == "=" && w[2].starts_with('"')).map(|w| w[2].trim_matches('"').to_string()).collect::<Vec<_>>()
        })
        .collect()
}
fn value(attrs: &[String], key: &str) -> Option<String> {
    values(attrs, key).into_iter().next()
}
fn renamed(name: &str, attrs: &[String], rule: Option<&str>) -> String {
    if let Some(s) = value(attrs, "rename") {
        return s;
    }
    match rule {
        None | Some("PascalCase") => name.into(),
        Some("lowercase") => name.to_lowercase(),
        Some("UPPERCASE") => name.to_uppercase(),
        Some("camelCase") => {
            let mut c = name.chars();
            c.next().unwrap().to_lowercase().chain(c).collect()
        }
        Some(rule @ ("snake_case" | "SCREAMING_SNAKE_CASE" | "kebab-case" | "SCREAMING-KEBAB-CASE")) => {
            let mut s = String::new();
            for (i, c) in name.chars().enumerate() {
                if c.is_uppercase() && i != 0 {
                    s.push('_');
                }
                s.extend(c.to_lowercase());
            }
            if rule.contains("SCREAMING") {
                s = s.to_uppercase();
            }
            if rule.contains("kebab") || rule.contains("KEBAB") {
                s = s.replace('_', "-");
            }
            s
        }
        Some(other) => panic!("unhandled serde rename rule: {other}"),
    }
}
fn collect(dir: &Path, out: &mut Vec<Model>) {
    let mut paths: Vec<_> = std::fs::read_dir(dir).unwrap().map(|p| p.unwrap().path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|s| s == "rs") {
            out.extend(parse(path.clone(), &std::fs::read_to_string(path).unwrap()));
        }
    }
}
fn models() -> Vec<Model> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut out = Vec::new();
    for krate in ["pc-doc", "pc-color", "pc-geom", "pc-format"] {
        collect(&crates.join(krate).join("src"), &mut out);
    }
    assert!(out.iter().filter(|m| m.is_enum).count() > 40, "schema audit must discover the model, not a fixed allowlist");
    out
}
fn discriminant_errors(models: &[Model]) -> Vec<String> {
    let mut errors = Vec::new();
    for m in models.iter().filter(|m| m.is_enum && value(&m.attrs, "untagged").is_none()) {
        let mut seen = BTreeMap::new();
        let rule = value(&m.attrs, "rename_all");
        for member in &m.members {
            let mut tags = vec![renamed(&member.name, &member.attrs, rule.as_deref())];
            tags.extend(values(&member.attrs, "alias"));
            for tag in tags {
                if let Some(previous) = seen.insert(tag.clone(), &member.name) {
                    errors.push(format!("{}::{}: {previous} and {} both use {tag}", m.file.display(), m.name, member.name));
                }
            }
        }
    }
    errors
}
fn resolve<'a>(models: &'a [Model], parent: &Model, body: &[String]) -> Option<&'a Model> {
    // For newtypes this picks the inner type (also handles Box<T>/Arc<T>).
    let candidates: Vec<_> = models.iter().filter(|m| body.contains(&m.name)).collect();
    candidates.iter().copied().find(|m| m.file == parent.file).or_else(|| candidates.first().copied())
}
fn keys(m: &Model, models: &[Model], visiting: &mut BTreeSet<String>) -> BTreeSet<String> {
    let identity = format!("{}::{}", m.file.display(), m.name);
    if !visiting.insert(identity.clone()) {
        return BTreeSet::new();
    }
    let mut out = BTreeSet::new();
    if let Some(tag) = value(&m.attrs, "tag") {
        out.insert(tag);
    }
    if !m.is_enum {
        for f in &m.members {
            if f.attrs.iter().any(|a| a.contains("flatten")) {
                if let Some(inner) = resolve(models, m, &f.body) {
                    out.extend(keys(inner, models, visiting));
                }
            } else {
                out.insert(renamed(&f.name, &f.attrs, None));
            }
        }
    }
    visiting.remove(&identity);
    out
}
fn nesting_errors(models: &[Model]) -> Vec<String> {
    let mut out = Vec::new();
    for m in models.iter().filter(|m| m.is_enum) {
        let Some(tag) = value(&m.attrs, "tag") else { continue };
        if value(&m.attrs, "content").is_some() {
            continue;
        }
        for v in &m.members {
            let merged = if v.body.first().is_some_and(|s| s == "(") {
                resolve(models, m, &v.body).map(|inner| keys(inner, models, &mut BTreeSet::new())).unwrap_or_default()
            } else if v.body.first().is_some_and(|s| s == "{") {
                let fields = Model {
                    file: m.file.clone(),
                    name: format!("{}::{}", m.name, v.name),
                    attrs: vec![],
                    members: members(&v.body[1..v.body.len() - 1]),
                    is_enum: false,
                };
                keys(&fields, models, &mut BTreeSet::new())
            } else {
                BTreeSet::new()
            };
            if merged.contains(&tag) {
                out.push(format!("{}::{}::{} merges duplicate tag key {tag}", m.file.display(), m.name, v.name));
            }
        }
    }
    out
}
#[test]
fn every_document_serde_enum_has_unique_discriminants() {
    assert_eq!(discriminant_errors(&models()), Vec::<String>::new());
}
#[test]
fn unrelated_nested_tag_scopes_and_non_live_newtypes_are_unique() {
    let errors: Vec<_> =
        nesting_errors(&models()).into_iter().filter(|e| !e.contains("::LiveGroup::Repeat ") && !e.contains("::LiveGroup::Envelope ")).collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}
#[test]
fn all_nested_tag_keys_are_unique_in_each_json_object() {
    let errors = nesting_errors(&models());
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}
#[test]
fn schema_auditor_detects_duplicate_renames_aliases_and_flattened_tags() {
    let source = r#"
        #[derive(Serialize, Deserialize)] #[serde(tag="kind", rename_all="snake_case")]
        enum Outer { One(Inner), Two { #[serde(flatten)] inner: Inner }, Named { inner: Inner } }
        #[derive(Serialize, Deserialize)] #[serde(tag="kind")]
        enum Inner { A, B }
        #[derive(Serialize, Deserialize)]
        enum Renames { #[serde(rename="same")] A, #[serde(rename="same")] B }
        #[derive(Serialize, Deserialize)]
        enum Aliases { #[serde(alias="B")] A, B }
    "#;
    let parsed = parse(PathBuf::from("sentinel.rs"), source);
    assert_eq!(discriminant_errors(&parsed).len(), 2);
    let errors = nesting_errors(&parsed);
    assert_eq!(errors.len(), 2);
    assert!(!errors.iter().any(|e| e.contains("::Named ")));
}
