//! The one translation engine for Local Image: Library, Develop and Compositing look their text
//! up here. Strings in code stay English and are the lookup keys; a per-language catalog
//! (`catalogs/*/<code>.tsv`, format documented in `catalogs/editor/ja.tsv`) maps them to display
//! text at render time. Command ids, menu paths used for logic, the control channel, the CLI and
//! MCP never see translated text, so agents and scripts are unaffected. A string without a
//! translation is shown in English.
//!
//! # Catalogs
//! Each language has two catalogs, one per half of the app ([`Domain`]): `catalogs/editor`
//! (Compositing and the shared shell: menus, Preferences, dialogs) and `catalogs/library`
//! (Library and Develop). They are kept apart so every string reads exactly as it did before the
//! two engines were merged; folding them into one catalog per language (one translation per
//! shared string) is translation work for the next translation pass.
//!
//! # Adding a language
//! 1. Add a variant to [`Lang`] and one row to `LANGUAGES` (code, native name, script, plural
//!    rule, catalogs).
//! 2. Add `catalogs/editor/xx.tsv` (copy the header of `ja.tsv`; translate from the *meaning* of
//!    the English text, clean-room), `catalogs/library/xx.tsv` and `catalogs/library/xx-formats.tsv`.
//!
//! That is all: the Preferences dropdown, the system-locale match, the font fallback and the
//! catalog tests (parse, placeholders, plural forms) pick it up from the registry; `build.rs`
//! picks up the format strings by file name.
//!
//! # Looking strings up
//! - [`tl!`] / [`t`]: a plain string in the current language. [`tr`]: in a given language.
//! - [`tr_ctx`]: when one English word needs different translations.
//! - [`tr_id`]: a command-id keyed string with the English label as fallback (menu items), so a
//!   translation survives rewording of the English text and can differ per command.
//! - [`trn`]: plural-aware (`{n}` is filled in). [`fmt`]: fill `{name}` placeholders after [`tr`];
//!   translators may reorder placeholders freely.
//! - [`library::t`] / [`library::tr`]: the same lookups in the Library & Develop catalog;
//!   [`library::builtin_label`] translates stock names and never a user's.
//! - [`tr_format!`]: a Library & Develop format string whose translations are checked by
//!   `format!` at compile time (a wrong placeholder in any language is a build error).

pub mod catalog;
mod system;

use std::cell::Cell;
use std::sync::OnceLock;

use catalog::Catalog;
pub use system::{system_lang, with_system_tags};

/// Translate a string literal into the current UI language: `tl!("Image Size…")`.
#[macro_export]
macro_rules! tl {
    ($s:expr) => {
        $crate::t($s)
    };
}

/// Which half of the app a catalog belongs to (see the crate docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    /// Compositing and the shared shell (menus, Preferences, dialogs): [`t`], [`tr`], [`tr_ctx`],
    /// [`tr_id`], [`trn`].
    Editor,
    /// Library and Develop: [`library::t`], [`library::tr`], [`tr_format!`].
    Library,
}

/// A UI language. The settings file stores its BCP 47 code (`"zh-hans"`), never the variant
/// name, so a language can be renamed in code without invalidating anyone's settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lang {
    /// The language the UI is written in: the source text and every lookup's fallback.
    #[default]
    En,
    Ja,
    ZhHans,
    /// Traditional Chinese in the vocabulary used in Taiwan; `zh-TW`, `zh-HK`, `zh-MO` and
    /// `zh-Hant-*` locales all resolve here (see `candidates`).
    ZhHant,
    Es,
    Ru,
    Cs,
    Fr,
    Id,
    Ko,
    De,
    /// Brazilian Portuguese; `pt`, `pt-BR` and `pt-PT` locales all resolve here.
    PtBr,
    It,
}

/// One registered language. Rows are in [`Lang`] order.
struct LangInfo {
    /// BCP 47 code, lowercase (`ja`, `zh-hans`, `pt-br`). Also the `interface.language` value.
    code: &'static str,
    /// The language's name in itself (an endonym, so a user who cannot read the current UI
    /// language can still find their own), shown in the Language menus and Preferences.
    name: &'static str,
    /// The ISO 15924 script its text needs (`"Latn"`, `"Jpan"`, `"Hans"`…): picks the CJK font
    /// fallback.
    script: &'static str,
    /// Plural form index for a count (English: 0 = one, 1 = other; Japanese and Chinese: always 0;
    /// Czech: 0 = one, 1 = few (2–4), 2 = other; French: 0 = one (0 and 1), 1 = other). A
    /// catalog's `@plural` entries list one form per index.
    plural: fn(u64) -> usize,
    /// Must the editor catalog cover every menu string? (checked by the tests)
    complete_menus: bool,
    /// [`Domain::Editor`] catalog file contents (empty for the built-in English).
    editor: &'static str,
    /// [`Domain::Library`] catalog file contents (empty for the built-in English).
    library: &'static str,
}

fn plural_one_other(n: u64) -> usize {
    usize::from(n != 1)
}

fn plural_none(_: u64) -> usize {
    0
}

fn plural_russian(n: u64) -> usize {
    match (n % 10, n % 100) {
        (1, 11..=19) => 2,
        (1, _) => 0,
        (2..=4, 11..=19) => 2,
        (2..=4, _) => 1,
        _ => 2,
    }
}

/// Czech: 1 → one, 2–4 → few, everything else (0, 5+) → other.
fn plural_cs(n: u64) -> usize {
    match n {
        1 => 0,
        2..=4 => 1,
        _ => 2,
    }
}

/// French: 0 and 1 take the singular, everything else the plural.
fn plural_fr(n: u64) -> usize {
    usize::from(n > 1)
}

/// Portuguese: 0 and 1 take the singular, everything else the plural.
fn plural_pt(n: u64) -> usize {
    usize::from(n > 1)
}

macro_rules! row {
    ($code:literal, $name:literal, $script:literal, $plural:ident) => {
        LangInfo {
            code: $code,
            name: $name,
            script: $script,
            plural: $plural,
            complete_menus: true,
            editor: include_str!(concat!("../catalogs/editor/", $code, ".tsv")),
            library: include_str!(concat!("../catalogs/library/", $code, ".tsv")),
        }
    };
}

const LANG_COUNT: usize = 13;

/// The registry, in [`Lang`] order. English first: it is the fallback and the source language.
static LANGUAGES: [LangInfo; LANG_COUNT] = [
    LangInfo { code: "en", name: "English", script: "Latn", plural: plural_one_other, complete_menus: false, editor: "", library: "" },
    row!("ja", "日本語", "Jpan", plural_none),
    row!("zh-hans", "简体中文", "Hans", plural_none),
    row!("zh-hant", "繁體中文", "Hant", plural_none),
    row!("es", "Español", "Latn", plural_one_other),
    row!("ru", "Русский", "Cyrl", plural_russian),
    row!("cs", "Čeština", "Latn", plural_cs),
    row!("fr", "Français", "Latn", plural_fr),
    row!("id", "Bahasa Indonesia", "Latn", plural_none),
    row!("ko", "한국어", "Kore", plural_none),
    row!("de", "Deutsch", "Latn", plural_one_other),
    row!("pt-br", "Português (Brasil)", "Latn", plural_pt),
    row!("it", "Italiano", "Latn", plural_one_other),
];

/// Parsed catalogs, built on first use per language and domain.
static CATALOGS: [[OnceLock<Catalog>; LANG_COUNT]; 2] = [const { [const { OnceLock::new() }; LANG_COUNT] }; 2];

impl Lang {
    /// Every language, in menu order (English first).
    pub const ALL: [Lang; LANG_COUNT] =
        [Lang::En, Lang::Ja, Lang::ZhHans, Lang::ZhHant, Lang::Es, Lang::Ru, Lang::Cs, Lang::Fr, Lang::Id, Lang::Ko, Lang::De, Lang::PtBr, Lang::It];

    const fn info(self) -> &'static LangInfo {
        &LANGUAGES[self as usize]
    }

    /// The BCP 47 code (`"zh-hans"`): the `interface.language` value.
    pub const fn code(self) -> &'static str {
        self.info().code
    }

    /// The language's own name, as shown in the Language menus and Preferences.
    pub const fn name(self) -> &'static str {
        self.info().name
    }

    /// The ISO 15924 script this language is written in (`"Latn"`, `"Jpan"`, `"Hans"`…): the
    /// craft-fonts faces for it are the ones this language needs.
    pub const fn script(self) -> &'static str {
        self.info().script
    }

    /// Does this language's editor catalog claim to cover every menu string and `tl!` literal?
    pub const fn complete_menus(self) -> bool {
        self.info().complete_menus
    }

    /// The plural form index for a count (see `catalogs/editor/ja.tsv` for `@plural`).
    pub fn plural_form(self, n: u64) -> usize {
        (self.info().plural)(n)
    }

    /// The raw catalog file of `domain` (empty for English).
    pub const fn catalog_source(self, domain: Domain) -> &'static str {
        match domain {
            Domain::Editor => self.info().editor,
            Domain::Library => self.info().library,
        }
    }

    /// A language by its exact code (ASCII case ignored).
    pub fn from_code(code: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code().eq_ignore_ascii_case(code))
    }

    /// The registered language for a locale tag such as `ja_JP.UTF-8`, `ja-JP`, `zh-TW`; `None`
    /// if the language isn't supported. `C`/`POSIX` mean English.
    pub fn from_tag(tag: &str) -> Option<Lang> {
        let cands = candidates(tag);
        if matches!(cands.first().map(String::as_str), Some("c" | "posix")) {
            return Some(Lang::En);
        }
        cands.iter().find_map(|c| Lang::from_code(c))
    }

    /// Resolve the `interface.language` preference: a language code, or `auto` (and anything
    /// unknown, e.g. a code from a newer version) to follow the system locale.
    pub fn from_pref(pref: &str) -> Lang {
        if pref.eq_ignore_ascii_case("auto") {
            return system_lang();
        }
        Lang::from_code(pref).or_else(|| Lang::from_tag(pref)).unwrap_or_else(system_lang)
    }
}

impl serde::Serialize for Lang {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

impl<'de> serde::Deserialize<'de> for Lang {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Lang::from_code(&code).or_else(|| Lang::from_tag(&code)).ok_or_else(|| serde::de::Error::custom(format!("unknown language {code:?}")))
    }
}

/// The parsed catalog of `lang` for `domain` (built on first use). A malformed line is logged
/// and skipped: a broken translation degrades to English text, it never takes the app down.
pub fn catalog(lang: Lang, domain: Domain) -> &'static Catalog {
    CATALOGS[domain as usize][lang as usize].get_or_init(|| {
        let (entries, errors) = catalog::parse_entries(lang.catalog_source(domain));
        if !errors.is_empty() {
            log::error!("{} {domain:?} catalog: {} malformed line(s) skipped: {errors:?}", lang.code(), errors.len());
        }
        Catalog::from_entries(entries)
    })
}

/// Candidate language codes for a locale tag, most specific first: `zh_TW.UTF-8` →
/// `zh-tw`, `zh-hant`, `zh`.
fn candidates(tag: &str) -> Vec<String> {
    let base = tag.split(['.', '@']).next().unwrap_or("").replace('_', "-").to_ascii_lowercase();
    let parts: Vec<&str> = base.split('-').filter(|p| !p.is_empty()).collect();
    let Some(&primary) = parts.first() else { return Vec::new() };
    let mut out = Vec::new();
    for n in (1..=parts.len()).rev() {
        out.push(parts[..n].join("-"));
    }
    if primary == "zh" && !parts.iter().any(|p| matches!(*p, "hans" | "hant")) {
        // Chinese by region when no script is given.
        let script = if parts.iter().any(|p| matches!(*p, "tw" | "hk" | "mo")) { "zh-hant" } else { "zh-hans" };
        out.insert(out.len() - 1, script.to_string());
    }
    if primary == "pt" && !out.iter().any(|c| c == "pt-br") {
        // The only Portuguese catalog is Brazilian; other regions use it rather than English.
        out.insert(out.len() - 1, "pt-br".to_string());
    }
    out
}

/// The UI language named by `LOCAL_IMAGE_LANGUAGE` (`zh-hans`, `ja_JP.UTF-8`…), else English:
/// the Library's starting language for headless runs and snapshots.
pub fn env_language() -> Lang {
    std::env::var("LOCAL_IMAGE_LANGUAGE").ok().and_then(|value| Lang::from_tag(&value)).unwrap_or(Lang::En)
}

thread_local! {
    // Independent app/test threads must not change each other's drawing language.
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::En) };
}

/// Set the UI language for drawing (the shells call this from the preference), so widgets can
/// translate without every call site carrying a language around.
pub fn set_current(lang: Lang) {
    CURRENT.set(lang);
}

/// The language the UI is drawn in.
#[inline]
pub fn current() -> Lang {
    CURRENT.get()
}

/// Temporarily draw in another language, restoring the previous one even on unwinding.
/// Preferences previews use this without changing the saved setting or other dialogs.
pub fn with_language<R>(lang: Lang, draw: impl FnOnce() -> R) -> R {
    let _restore = language_scope(lang);
    draw()
}

/// Keep the language active until this guard is dropped on the drawing thread.
#[must_use]
pub fn language_scope(lang: Lang) -> impl Drop {
    struct Restore {
        previous: Lang,
        _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            set_current(self.previous);
        }
    }
    let restore = Restore { previous: current(), _thread: std::marker::PhantomData };
    set_current(lang);
    restore
}

/// Does `lang` have an editor-catalog entry for this plain string? (English never does: it is
/// the source.)
pub fn has(lang: Lang, s: &str) -> bool {
    catalog(lang, Domain::Editor).plain(s).is_some()
}

/// Translate an English UI string into the current language ([`tr`] with [`current`]).
#[inline]
pub fn t(s: &str) -> &str {
    tr(current(), s)
}

/// Translate an English UI string; unknown strings come back unchanged.
#[inline]
pub fn tr(lang: Lang, s: &str) -> &str {
    // English is the source: no catalog to consult (the default, per-frame path).
    if lang == Lang::En {
        return s;
    }
    catalog(lang, Domain::Editor).plain(s).unwrap_or(s)
}

/// Like [`tr`], for an English string that needs a disambiguating `context`.
pub fn tr_ctx<'a>(lang: Lang, context: &str, s: &'a str) -> &'a str {
    catalog(lang, Domain::Editor).contextual(context, s).unwrap_or_else(|| tr(lang, s))
}

/// A string keyed by its command id, falling back to the translation of the English `label`.
pub fn tr_id<'a>(lang: Lang, id: &str, label: &'a str) -> &'a str {
    catalog(lang, Domain::Editor).id(id).unwrap_or_else(|| tr(lang, label))
}

/// Fill `{name}` placeholders. Unknown placeholders are left as written.
pub fn fmt(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (k, v) in args {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// A plural-aware message: `one`/`other` are the English forms (with `{n}` where the count goes).
pub fn trn(lang: Lang, n: u64, one: &str, other: &str) -> String {
    let text = catalog(lang, Domain::Editor).plural(one, other, lang.plural_form(n)).unwrap_or(if n == 1 { one } else { other });
    fmt(text, &[("n", &n.to_string())])
}

/// Library & Develop lookups: the [`Domain::Library`] catalog. Never call these on editable user
/// text, file names or command identifiers.
pub mod library {
    use super::{Domain, Lang, catalog, current};

    /// Translate a built-in display label into the current language, preserving unknown labels
    /// verbatim.
    #[inline]
    pub fn t(s: &str) -> &str {
        tr(current(), s)
    }

    /// Translate a built-in display label into `lang`; unknown labels come back unchanged.
    #[inline]
    pub fn tr(lang: Lang, s: &str) -> &str {
        if lang == Lang::En {
            return s;
        }
        catalog(lang, Domain::Library).plain(s).unwrap_or(s)
    }

    /// A stock preset's or profile's name in the UI language; an imported or user-edited name
    /// stays exactly as its owner wrote it.
    pub fn builtin_label(s: &str, builtin: bool) -> &str {
        if builtin { t(s) } else { s }
    }
}

// `tr_format!` (one arm per message, one `format!` per language) is generated by `build.rs` from
// `catalogs/library/*-formats.tsv`.
include!(concat!(env!("OUT_DIR"), "/tr-formats.rs"));

#[cfg(test)]
mod pending_library;
#[cfg(test)]
mod tests;
