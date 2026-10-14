//! Compile the translated format strings into the `tr_format!` macro.
//!
//! `catalogs/library/<code>-formats.tsv` maps an English format string to its translation for one
//! language. Every English message becomes one `tr_format!` arm that picks the active language and
//! formats that language's translation with `format!` — so Rust checks every catalog's
//! placeholders against the call site's arguments (a wrong or forgotten placeholder is a compile
//! error, and `{:.1}`, `{:+}`… keep their meaning). The English source is the fallback arm. A new
//! language's file is picked up by name; its code must be a [`Lang`] variant's (`zh-hans` →
//! `ZhHans`).
//!
//! Every catalog must carry the same messages: a `tr_format!` call whose message is in no catalog
//! does not compile, so a message missing from one language would otherwise go unnoticed until it
//! shows English.
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::PathBuf,
};

// The run-time catalog parser: one format, one reader.
#[allow(dead_code)]
#[path = "src/catalog.rs"]
mod catalog;

const DIR: &str = "catalogs/library";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/catalog.rs");
    // The directory itself, so a catalog added or removed reruns this script.
    println!("cargo:rerun-if-changed={DIR}");
    let out = PathBuf::from(env::var("OUT_DIR")?);
    fs::write(out.join("tr-formats.rs"), generate()?)?;
    Ok(())
}

/// `ja` → `Ja`, `zh-hans` → `ZhHans`: the `Lang` variant for a code. The code must be a
/// lowercase BCP-47-like tag (`^[a-z]+(-[a-z]+)*$`), so the variant is always a valid identifier.
fn variant_for(code: &str) -> Result<String, String> {
    let valid = !code.is_empty() && code.split('-').all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_lowercase()));
    if !valid {
        return Err(format!("{DIR}/{code}-formats.tsv: the language code must match ^[a-z]+(-[a-z]+)*$"));
    }
    Ok(code.split('-').map(capitalize).collect())
}

fn capitalize(part: &str) -> String {
    let mut chars = part.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn generate() -> Result<String, Box<dyn std::error::Error>> {
    let mut catalogs: Vec<(String, String, BTreeMap<String, String>)> = Vec::new();
    for entry in fs::read_dir(DIR)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
        // `en` is the source language: it has no catalog, its formats are the source themselves.
        let Some(code) = name.strip_suffix("-formats.tsv").filter(|code| *code != "en") else { continue };
        println!("cargo:rerun-if-changed={DIR}/{name}");
        let variant = variant_for(code)?;
        let (entries, errors) = catalog::parse_entries(&fs::read_to_string(&path)?);
        if !errors.is_empty() {
            return Err(format!("{DIR}/{name}: {errors:?}").into());
        }
        let mut messages = BTreeMap::new();
        for (context, english, translated) in entries {
            if !context.is_empty() {
                return Err(format!("{DIR}/{name}: format strings take no context ({context:?} for {english:?})").into());
            }
            if messages.insert(english.clone(), translated).is_some() {
                return Err(format!("{DIR}/{name}: {english:?} is listed twice").into());
            }
        }
        catalogs.push((code.to_string(), variant, messages));
    }
    catalogs.sort_by(|a, b| a.0.cmp(&b.0));

    // Every catalog translates the same messages.
    let all: BTreeSet<&String> = catalogs.iter().flat_map(|(_, _, messages)| messages.keys()).collect();
    let mut problems = Vec::new();
    for (code, _, messages) in &catalogs {
        for key in &all {
            if !messages.contains_key(*key) {
                problems.push(format!("{DIR}/{code}-formats.tsv is missing {key:?}"));
            }
        }
    }
    if !problems.is_empty() {
        return Err(problems.join("\n").into());
    }

    let mut source = String::from(concat!(
        "/// Translate a Library & Develop format string, checked by `format!` in every language.\n",
        "/// `build.rs` writes one arm per message from `catalogs/library/*-formats.tsv`.\n",
        "#[macro_export]\n",
        "macro_rules! tr_format {\n",
    ));
    for english in all {
        let message = literal(english);
        source.push_str(&format!("    ({message} $(, $($args:tt)*)?) => {{\n        match $crate::current() {{\n"));
        for (_, variant, messages) in &catalogs {
            if let Some(translated) = messages.get(english) {
                let translated = literal(translated);
                source.push_str(&format!("            $crate::Lang::{variant} => format!({translated} $(, $($args)*)?),\n"));
            }
        }
        source.push_str(&format!("            _ => format!({message} $(, $($args)*)?),\n        }}\n    }};\n"));
    }
    source.push_str("}\n");
    Ok(source)
}

/// `text` as a Rust string literal written the way call sites write it: only backslash, `"` and control
/// characters are escaped. `macro_rules!` matches a literal by its written form, so the arm for
/// `"Merging… {stage}"` must spell the `…` the same way the call does.
fn literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            ch if ch.is_control() => out.push_str(&format!("\\u{{{:x}}}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}
