//! Library labels that wrap user or catalog data in translated text: Activity steps, source
//! headings and dates. The lookups themselves are `li_i18n`'s (the Library & Develop catalog).

use li_i18n::{library, tr_format};

pub use li_i18n::dates::{date_group_label, display_time};

/// An Activity (history) step for display. The step's stored label stays English; generated steps
/// are translated around the names they carry, and a user preset's name is kept verbatim.
pub fn history_label(source: &str, presets: &[lightcraft_develop::Preset]) -> String {
    if let Some(name) = source.strip_prefix("Preset: ") {
        // A user preset may share a stock preset's name: then the step could be either, so it
        // keeps the name as written.
        let builtin = presets.iter().any(|p| p.builtin && p.name == name) && !presets.iter().any(|p| !p.builtin && p.name == name);
        return tr_format!("Preset: {name}", name = library::builtin_label(name, builtin));
    }
    if let Some(name) = source.strip_prefix("Reset ") {
        return tr_format!("Reset {name}", name = library::t(name));
    }
    if let Some(name) = source.strip_prefix("Quick Develop: ") {
        return tr_format!("Quick Develop: {name}", name = library::t(name));
    }
    library::t(source).to_string()
}

/// A library source's heading (All Photos, Recently Deleted…) in the UI language; an album's
/// name is the user's and stays verbatim.
pub fn source_label(source: lightcraft_engine::LibrarySource, catalog: &lightcraft_catalog::Catalog) -> String {
    let label = source.label(catalog);
    if matches!(source, lightcraft_engine::LibrarySource::Album(id) if catalog.album(id).is_some()) { label } else { library::t(&label).to_string() }
}
