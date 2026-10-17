//! Engine tests: the language registry, catalog parsing and lookups of both catalogs, plural
//! rules, locale tags, the compile-checked format strings, and the parity goldens. Coverage
//! tests that need a shell's own sources or windows live in that shell's crate.

use crate::catalog::{Catalog, parse_entries, placeholders};
use crate::*;

const JA: fn() -> Lang = || Lang::from_code("ja").expect("ja registered");
const ZH: fn() -> Lang = || Lang::from_code("zh-hant").expect("zh-hant registered");
const CS: fn() -> Lang = || Lang::from_code("cs").expect("cs registered");
const ID: fn() -> Lang = || Lang::from_code("id").expect("id registered");

#[test]
fn indonesian_tags_resolve() {
    for tag in ["id", "id-ID", "id_ID", "id_ID.UTF-8"] {
        assert_eq!(Lang::from_tag(tag), Some(ID()), "{tag}");
    }
    assert_eq!(ID().name(), "Bahasa Indonesia");
}

#[test]
fn tags_map_to_languages() {
    assert_eq!(Lang::from_tag("ja_JP.UTF-8"), Some(JA()));
    assert_eq!(Lang::from_tag("ja-JP"), Some(JA()));
    assert_eq!(Lang::from_tag("en_US.UTF-8"), Some(Lang::En));
    assert_eq!(Lang::from_tag("C"), Some(Lang::En));
    assert_eq!(Lang::from_tag("POSIX"), Some(Lang::En));
    assert_eq!(Lang::from_tag("cs_CZ.UTF-8"), Some(CS()));
    assert_eq!(Lang::from_tag("cs-CZ"), Some(CS()));
    assert_eq!(Lang::from_tag("fr_FR"), Lang::from_code("fr"));
    assert_eq!(Lang::from_tag("de_DE"), Lang::from_code("de"));
    assert_eq!(Lang::from_tag("de-AT"), Lang::from_code("de"));
    assert_eq!(Lang::from_tag("it_IT.UTF-8"), Lang::from_code("it"));
    assert_eq!(Lang::from_tag("it-CH"), Lang::from_code("it"));
    // Traditional Chinese: by region, by script, and with a region after the script.
    assert_eq!(Lang::from_tag("zh_TW.UTF-8"), Some(ZH()));
    assert_eq!(Lang::from_tag("zh-TW"), Some(ZH()));
    assert_eq!(Lang::from_tag("zh-HK"), Some(ZH()));
    assert_eq!(Lang::from_tag("zh_MO"), Some(ZH()));
    assert_eq!(Lang::from_tag("zh-Hant"), Some(ZH()));
    assert_eq!(Lang::from_tag("zh-Hant-TW"), Some(ZH()));
    assert_eq!(Lang::from_tag("zh-Hant-HK"), Some(ZH()));
    // Simplified Chinese locales never pick up the Traditional catalog: they resolve to `zh-hans`.
    for tag in ["zh-CN", "zh_CN.UTF-8", "zh_SG", "zh-Hans", "zh-Hans-CN", "zh"] {
        assert_ne!(Lang::from_tag(tag), Some(ZH()), "{tag}");
        assert_eq!(Lang::from_tag(tag), Lang::from_code("zh-hans"), "{tag}");
    }
    assert_eq!(Lang::from_tag(""), None);
    assert_eq!(Lang::from_tag("_"), None);
}

#[test]
fn candidates_walk_from_specific_to_general() {
    assert_eq!(candidates("pt_BR.UTF-8"), ["pt-br", "pt"]);
    assert_eq!(candidates("zh_TW"), ["zh-tw", "zh-hant", "zh"]);
    assert_eq!(candidates("zh-CN"), ["zh-cn", "zh-hans", "zh"]);
    assert_eq!(candidates("zh-Hant-HK"), ["zh-hant-hk", "zh-hant", "zh"]);
}

#[test]
fn preferences_resolve_with_fallback() {
    assert_eq!(Lang::from_pref("ja"), JA());
    assert_eq!(Lang::from_pref("JA"), JA());
    assert_eq!(Lang::from_pref("zh-hant"), ZH());
    assert_eq!(Lang::from_pref("ZH-Hant"), ZH());
    assert_eq!(Lang::from_pref("en"), Lang::En);
    // `auto` and unknown codes follow the system (English under test).
    assert_eq!(Lang::from_pref("auto"), Lang::En);
    assert_eq!(Lang::from_pref("xx-unknown"), Lang::En);
}

#[test]
fn simplified_chinese_covers_dynamic_shortcuts_and_layer_counts() {
    let zh = Lang::from_code("zh-hans").expect("zh-hans registered");
    assert!(zh.complete_menus(), "Simplified Chinese must participate in the coverage gates");
    assert_eq!(Lang::from_pref("ZH-Hans"), zh);
    assert_eq!(tr(zh, "Pixel Layer"), "像素图层");
    assert_eq!(tr(zh, "System Info"), "系统信息");
    for key in ["⌥", "Alt"] {
        assert_eq!(fmt(tr(zh, "Add a mask  (from the selection; {key} inverts)"), &[("key", key)]), format!("添加蒙版  （基于选区；{key} 反相）"));
    }
    for n in [0, 1, 3] {
        assert_eq!(trn(zh, n, "{n} layer", "{n} layers"), format!("{n} 个图层"));
    }
    assert_eq!(tr(zh, "no such label"), "no such label");
}

/// Keep the Korean tool/menu vocabulary aligned with the Photoshop equivalents.
/// Sources and the product-specific vocabulary policy are recorded in `ko.tsv`.
#[test]
fn korean_uses_photoshop_terminology() {
    let ko = Lang::from_code("ko").expect("ko registered");
    for (source, expected) in [
        ("Shape", "모양"),
        ("Stroke", "획"),
        ("Smudge Tool", "손가락 도구"),
        ("Eyedropper Tool", "스포이드 도구"),
        ("Rectangular Marquee Tool", "사각형 선택 윤곽 도구"),
        ("Elliptical Marquee Tool", "원형 선택 윤곽 도구"),
        ("Zoom Tool", "돋보기 도구"),
        ("Horizontal Type Tool", "수평 문자 도구"),
        ("Puppet Warp", "퍼펫 뒤틀기"),
        ("Liquify…", "픽셀 유동화…"),
        ("Gaussian Blur…", "가우시안 흐림 효과…"),
        ("Gaussian Blur", "가우시안 흐림 효과"),
        ("Adaptive Wide Angle…", "응용 광각…"),
        ("Render", "렌더"),
        ("Sharpen", "선명 효과"),
        ("Vibrance", "활기"),
        ("Layer Comps", "레이어 구성 요소"),
        ("Vivid Light", "선명한 라이트"),
        ("Hard Mix", "하드 혼합"),
    ] {
        assert_eq!(tr(ko, source), expected, "{source}");
    }
    assert_eq!(tr_id(ko, "filter.sharpen.sharpen", "Sharpen"), "선명하게");
}

#[test]
fn spanish_resolves_and_pluralises() {
    let es = Lang::from_code("es").expect("es registered");
    for tag in ["es", "es_ES.UTF-8", "es-MX", "es-419"] {
        assert_eq!(Lang::from_tag(tag), Some(es), "{tag}");
    }
    assert_eq!(tr(es, "Layer"), "Capa");
    assert_eq!(trn(es, 1, "{n} item", "{n} items"), "1 elemento");
    assert_eq!(trn(es, 3, "{n} item", "{n} items"), "3 elementos");
}

#[test]
fn brazilian_portuguese_resolves_and_pluralises() {
    let pt = Lang::from_code("pt-br").expect("pt-br registered");
    for tag in ["pt", "pt-BR", "pt_BR.UTF-8", "pt-PT", "pt_PT.UTF-8"] {
        assert_eq!(Lang::from_tag(tag), Some(pt), "{tag}");
    }
    assert_eq!(candidates("pt_PT"), ["pt-pt", "pt-br", "pt"]);
    assert_eq!(tr(pt, "Layer"), "Camada");
    assert_eq!(trn(pt, 0, "{n} item", "{n} items"), "0 item");
    assert_eq!(trn(pt, 1, "{n} item", "{n} items"), "1 item");
    assert_eq!(trn(pt, 2, "{n} item", "{n} items"), "2 itens");
}

#[test]
fn lookups_fall_back_to_english() {
    assert_eq!(tr(JA(), "no such label"), "no such label");
    assert_eq!(tr(Lang::En, "Layer"), "Layer");
    assert_eq!(tr(JA(), "Layer"), "レイヤー");
    assert_eq!(tr(ZH(), "Layer"), "圖層");
    assert_eq!(tr(ZH(), "no such label"), "no such label");
    assert_eq!(tr_id(ZH(), "no.such.id", "Layer"), "圖層");
    assert_eq!(tr_id(JA(), "no.such.id", "Layer"), "レイヤー");
    assert_eq!(tr_ctx(JA(), "no such context", "Layer"), "レイヤー");
}

#[test]
fn russian_plural_rules() {
    let ru = || Lang::from_code("ru").expect("ru registered");
    assert_eq!(trn(ru(), 1, "{n} item", "{n} items"), "1 элемент");
    assert_eq!(trn(ru(), 2, "{n} item", "{n} items"), "2 элемента");
    assert_eq!(trn(ru(), 5, "{n} item", "{n} items"), "5 элементов");
    assert_eq!(trn(ru(), 11, "{n} item", "{n} items"), "11 элементов");
    assert_eq!(trn(ru(), 21, "{n} item", "{n} items"), "21 элемент");
    assert_eq!(trn(ru(), 22, "{n} item", "{n} items"), "22 элемента");
    assert_eq!(trn(ru(), 101, "{n} item", "{n} items"), "101 элемент");
    assert_eq!(trn(ru(), 111, "{n} item", "{n} items"), "111 элементов");
}

#[test]
fn catalog_kinds_are_parsed_and_looked_up() {
    let c = Catalog::parse("# c\n\tHello\tこんにちは\n@id\tfile.save\t保存する\nmenu\tWindows\tウィンドウ群\n@plural\t{n} file|{n} files\t{n} 個\n\n");
    assert_eq!(c.plain("Hello"), Some("こんにちは"));
    assert_eq!(c.id("file.save"), Some("保存する"));
    assert_eq!(c.contextual("menu", "Windows"), Some("ウィンドウ群"));
    assert_eq!(c.contextual("other", "Windows"), None);
    assert_eq!(c.plural("{n} file", "{n} files", 0), Some("{n} 個"));
    assert_eq!(c.plural("{n} file", "{n} files", 5), Some("{n} 個"), "an index past the forms clamps");
}

#[test]
fn malformed_lines_are_reported_not_fatal() {
    let (entries, errors) = parse_entries("\tok\tはい\nno tabs here\n\tonly\n\ta\tb\tc\textra\n\t\tempty source\n");
    assert_eq!(entries.len(), 1);
    assert_eq!(errors.len(), 4, "{errors:?}");
    assert_eq!(parse_entries("\ta\\tb\tx\\ny\\\\z\n").0[0], (String::new(), "a\tb".into(), "x\ny\\z".into()));
}

#[test]
fn plurals_and_placeholders() {
    assert_eq!(trn(Lang::En, 1, "{n} item", "{n} items"), "1 item");
    assert_eq!(trn(Lang::En, 0, "{n} item", "{n} items"), "0 items");
    assert_eq!(trn(Lang::En, 7, "{n} item", "{n} items"), "7 items");
    assert_eq!(trn(JA(), 1, "{n} item", "{n} items"), "1 件");
    assert_eq!(trn(JA(), 7, "{n} item", "{n} items"), "7 件");
    assert_eq!(trn(ZH(), 1, "{n} item", "{n} items"), "1 個項目");
    assert_eq!(trn(ZH(), 7, "{n} item", "{n} items"), "7 個項目");
    assert_eq!(trn(CS(), 1, "{n} item", "{n} items"), "1 položka");
    assert_eq!(trn(CS(), 3, "{n} item", "{n} items"), "3 položky");
    assert_eq!(trn(CS(), 5, "{n} item", "{n} items"), "5 položek");
    assert_eq!(trn(CS(), 0, "{n} item", "{n} items"), "0 položek");
    assert_eq!(fmt("{b} before {a}", &[("a", "x"), ("b", "y"), ("c", "z")]), "y before x");
    assert_eq!(fmt("{missing}", &[]), "{missing}");
    assert_eq!(placeholders("a {x} b {y} {"), ["x", "y"]);
}

#[test]
fn french_resolves_and_pluralises() {
    let fr = Lang::from_code("fr").expect("fr registered");
    for tag in ["fr", "fr_FR.UTF-8", "fr-CA", "fr_BE", "fr-CH"] {
        assert_eq!(Lang::from_tag(tag), Some(fr), "{tag}");
    }
    assert_eq!(tr(fr, "Layer"), "Calque");
    assert_eq!(tr_id(fr, "select.all", "All"), "Tout sélectionner", "an id override wins over the plain label");
    assert_eq!(tr(fr, "All"), "Tout");
    let forms: Vec<usize> = [0, 1, 2, 5, 100, u64::MAX].into_iter().map(plural_fr).collect();
    assert_eq!(forms, [0, 0, 1, 1, 1, 1]);
    assert_eq!(trn(fr, 0, "{n} item", "{n} items"), "0 élément");
    assert_eq!(trn(fr, 1, "{n} item", "{n} items"), "1 élément");
    assert_eq!(trn(fr, 3, "{n} item", "{n} items"), "3 éléments");
}

#[test]
fn czech_plural_rule() {
    let forms: Vec<usize> = [0, 1, 2, 3, 4, 5, 11, 12, 21, 22, 100, u64::MAX].into_iter().map(plural_cs).collect();
    assert_eq!(forms, [2, 0, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2]);
    assert_eq!(tr(CS(), "Layer"), "Vrstva");
    assert_eq!(tr_id(CS(), "select.all", "All"), "Vybrat vše", "an id override wins over the plain label");
    assert_eq!(tr(CS(), "All"), "Vše");
}

/// Every bundled editor catalog is well-formed and consistent with its sources. (Its `@id`
/// entries are checked against the live command list in photocraft-ui-egui.)
#[test]
fn bundled_catalogs_are_consistent() {
    for l in Lang::ALL {
        assert!(l.code() == l.code().to_ascii_lowercase() && !l.name().is_empty(), "{}", l.code());
        let (entries, errors) = parse_entries(l.catalog_source(Domain::Editor));
        assert!(errors.is_empty(), "{}: {errors:?}", l.code());
        let mut seen = std::collections::HashSet::new();
        for (ctx, src, tr) in &entries {
            assert!(seen.insert((ctx.clone(), src.clone())), "{}: duplicate {ctx:?} {src:?}", l.code());
            if ctx == "@plural" {
                let one_other: Vec<&str> = src.split('|').collect();
                assert_eq!(one_other.len(), 2, "{}: plural source must be `one|other`: {src:?}", l.code());
                let forms = (0..=1000).map(|n| l.plural_form(n)).max().unwrap_or(0) + 1;
                assert_eq!(tr.split('|').count(), forms, "{}: {forms} plural forms expected in {src:?}", l.code());
                for form in tr.split('|') {
                    let mut want = placeholders(one_other[1]);
                    let mut got = placeholders(form);
                    want.sort_unstable();
                    got.sort_unstable();
                    assert_eq!(want, got, "{}: placeholders differ in {src:?}", l.code());
                }
                continue;
            }
            let mut want = placeholders(src);
            let mut got = placeholders(tr);
            want.sort_unstable();
            got.sort_unstable();
            assert_eq!(want, got, "{}: placeholders differ in {src:?}", l.code());
            if ctx.is_empty() {
                assert_eq!(src.ends_with('…'), tr.ends_with('…'), "{}: ellipsis mismatch: {src:?}", l.code());
            }
        }
    }
}

#[test]
fn mixer_brush_ui_strings_have_translations_in_every_registered_language() {
    const STRINGS: &[&str] = &["Mixer Brush", "Mixer Brush Tool", "Wet", "Load", "Mix", "Flow", "Sample All Layers"];
    for lang in Lang::ALL {
        for source in STRINGS {
            let translated = tr(lang, source);
            if lang == Lang::En {
                assert_eq!(translated, *source, "English source string {source}");
            } else {
                assert_ne!(translated, *source, "{} is missing {source:?}", lang.code());
            }
        }
    }
}

// Library & Develop catalog

/// The plain (source, translation) pairs of a Library catalog.
fn library_entries(language: Lang) -> Vec<(&'static str, &'static str)> {
    catalog(language, Domain::Library).plain_entries().collect()
}

#[test]
fn every_language_has_a_catalog_covering_the_source() {
    assert!(catalog(Lang::En, Domain::Library).is_empty(), "the source language needs no catalog");
    assert!(catalog(Lang::En, Domain::Editor).is_empty(), "the source language needs no catalog");
    for language in Lang::ALL {
        assert!(!language.name().is_empty(), "{language:?} has a name");
        assert!(!language.script().is_empty(), "{language:?} has a script");
        assert_eq!(Lang::from_code(language.code()), Some(language), "{language:?} parses its own code");
        if language == Lang::En {
            continue;
        }
        let messages = library_entries(language);
        assert!(!messages.is_empty(), "{language:?}: {} has no messages", language.code());
        for (key, value) in messages {
            assert!(!value.is_empty(), "{language:?}: empty translation for {key:?}");
        }
    }
}

/// Where two Library catalogs translate the same text they agree on its placeholders. Catalogs
/// may cover different sets of messages (a message a language lacks shows in English): the gaps
/// are reported, not failed, so a translation can grow at its own pace.
#[test]
fn catalogs_agree_on_placeholders_and_report_gaps() {
    let fields = |text: &str| {
        let mut out: Vec<String> = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            let Some(end) = rest[start..].find('}') else { break };
            out.push(rest[start + 1..start + end].split(':').next().unwrap_or("").to_string());
            rest = &rest[start + end + 1..];
        }
        out.sort();
        out
    };
    let translated: Vec<Lang> = Lang::ALL.into_iter().filter(|language| *language != Lang::En).collect();
    let all: std::collections::BTreeSet<&str> = translated
        .iter()
        .flat_map(|language| library_entries(*language).into_iter().map(|(key, _)| key))
        .chain(crate::pending_library::PENDING_LIBRARY_UX_TRANSLATIONS.iter().copied())
        .chain(crate::pending_library::PENDING_LRC_UX_TRANSLATIONS.iter().copied())
        .collect();
    for language in &translated {
        let catalog = catalog(*language, Domain::Library);
        for (key, value) in catalog.plain_entries() {
            assert_eq!(fields(key), fields(value), "{language:?}: {key:?} -> {value:?}");
        }
        let missing: Vec<_> = all.iter().filter(|key| catalog.plain(key).is_none()).collect();
        if !missing.is_empty() {
            eprintln!("{} lacks {} message(s) another language translates (shown in English): {missing:?}", language.code(), missing.len());
        }
    }
}

/// Locale tags are matched leniently, so a system locale reaches a shipped language.
#[test]
fn locale_tags_normalize() {
    assert_eq!(Lang::from_tag("en"), Some(Lang::En));
    assert_eq!(Lang::from_tag("en-US"), Some(Lang::En));
    assert_eq!(Lang::from_tag("zh-hans"), Some(Lang::ZhHans));
    assert_eq!(Lang::from_tag("zh"), Some(Lang::ZhHans));
    assert_eq!(Lang::from_tag("zh_CN"), Some(Lang::ZhHans));
    assert_eq!(Lang::from_tag("zh-hant"), Some(Lang::ZhHant));
    assert_eq!(Lang::from_tag("zh-Hant"), Some(Lang::ZhHant));
    assert_eq!(Lang::from_tag("zh-TW"), Some(Lang::ZhHant));
    assert_eq!(Lang::from_tag("zh_TW.UTF-8"), Some(Lang::ZhHant));
    assert_eq!(Lang::from_tag("zh-HK"), Some(Lang::ZhHant));
    assert_eq!(Lang::from_tag("zh-Hant-TW"), Some(Lang::ZhHant));
    assert_eq!(Lang::from_tag("zh-CN"), Some(Lang::ZhHans));
    assert_eq!(Lang::from_tag("ja_JP.UTF-8"), Some(Lang::Ja));
    assert_eq!(Lang::from_tag("pt-br"), Some(Lang::PtBr));
    assert_eq!(Lang::from_tag("pt-BR"), Some(Lang::PtBr));
    assert_eq!(Lang::from_tag("pt_BR.UTF-8"), Some(Lang::PtBr));
    assert_eq!(Lang::from_tag("pt"), Some(Lang::PtBr));
    assert_eq!(Lang::from_tag("de_DE.UTF-8"), Some(Lang::De));
    assert_eq!(Lang::from_tag("ko-KR"), Some(Lang::Ko));
    assert_eq!(Lang::from_tag("nl"), None);
}

#[test]
fn switching_language_translates_and_restores() {
    set_current(Lang::ZhHans);
    assert_eq!(library::t("Exposure"), "曝光");
    assert_eq!(library::t("Settings"), "设置");
    // Untranslated and data-like text passes through untouched.
    assert_eq!(library::t("my-photo.jpg"), "my-photo.jpg");
    assert_eq!(library::t("develop.set"), "develop.set");
    assert_eq!(library::t("A string no catalog has"), "A string no catalog has");
    set_current(Lang::Ja);
    assert_eq!(library::t("Exposure"), "露出");
    set_current(Lang::PtBr);
    assert_eq!(library::t("Exposure"), "Exposição");
    assert_eq!(library::t("Settings"), "Configurações");
    set_current(Lang::En);
    assert_eq!(library::t("Exposure"), "Exposure");
}

/// Format strings carry their values in every language, and a language without a translation
/// falls back to the English format rather than dropping the values.
#[test]
fn formats_render_in_every_language() {
    // Every language's format string is checked by `format!` against the call site's values.
    set_current(Lang::En);
    assert_eq!(tr_format!("Imported {} photo{}", 12, "s"), "Imported 12 photos");
    assert_eq!(tr_format!("Exported {ok} of {total} photo{}", "s", ok = 4, total = 12), "Exported 4 of 12 photos");
    set_current(Lang::ZhHans);
    assert_eq!(tr_format!("Imported {} photo{}", 12, "s"), "已导入 12 张照片");
    assert_eq!(tr_format!("Exported {ok} of {total} photo{}", "s", ok = 4, total = 12), "已导出 12 张中的 4 张照片");
    set_current(Lang::Ja);
    assert_eq!(tr_format!("Imported {} photo{}", 12, "s"), "12枚を読み込みました");
    assert_eq!(tr_format!("Exported {ok} of {total} photo{}", "s", ok = 4, total = 12), "12枚中4枚を書き出しました");
    // Format specs survive translation: precision, sign, and values a language reorders.
    for language in Lang::ALL {
        set_current(language);
        let text = tr_format!("{count} smart preview{} · {:.1} MB", "s", 12.345, count = 3);
        assert!(text.contains("12.3") && !text.contains("12.34"), "{language:?}: {text}");
        let merging = tr_format!("Merging… {stage} {:.0}%", 42.4, stage = "Aligning");
        // French puts a (narrow) space before the percent sign.
        assert!(merging.replace([' ', '\u{a0}', '\u{202f}'], "").contains("42%"), "{language:?}: {merging}");
        let delta = tr_format!("{label} {d:+} on every selected photo", label = "Exposure", d = 5);
        assert!(delta.contains("+5"), "{language:?}: {delta}");
        let reading = tr_format!("Reading photos… {} of {}", 3, 10);
        let (three, ten) = (reading.find('3'), reading.find("10"));
        assert!(three.is_some() && ten.is_some(), "{language:?}: {reading}");
    }
    set_current(Lang::En);
    assert_eq!(tr_format!("Reading photos… {} of {}", 3, 10), "Reading photos… 3 of 10");
}

#[test]
fn catalogs_contain_core_workflows() {
    for language in Lang::ALL.into_iter().filter(|language| *language != Lang::En) {
        for key in ["Import Photos…", "Export…", "Exposure", "White Balance", "Settings", "Language"] {
            let value = library::tr(language, key);
            assert!(!value.is_empty() && value != key, "{language:?}: {key}");
        }
    }
}

/// A stock name is translated, a user's name never is.
#[test]
fn builtin_labels_translate_only_stock_names() {
    with_language(Lang::Ja, || {
        assert_eq!(library::builtin_label("Exposure", true), "露出");
        assert_eq!(library::builtin_label("Exposure", false), "Exposure");
    });
}

// parity: one engine, every string as before

/// FNV-1a over a catalog's (source, translation) pairs in source order.
fn digest<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> (usize, u64) {
    let mut pairs: Vec<_> = pairs.into_iter().collect();
    pairs.sort_unstable();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for (source, translation) in &pairs {
        for byte in source.bytes().chain([0]).chain(translation.bytes()).chain([0]) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    (pairs.len(), hash)
}

/// Golden: the Library catalogs and format strings, converted from
/// `lc-ui-egui/locales/*.json` to TSV, hold exactly the JSON's messages (count and FNV-1a of the
/// sorted pairs, computed from the JSON files before they were deleted). Regenerate (or delete
/// this test) when the next translation pass edits `catalogs/library`.
#[test]
fn j29_library_catalogs_match_the_json_they_replaced() {
    const GOLDEN: &[(&str, usize, u64)] = &[
        ("cs", 1588, 0x164bdfce376302ac),
        ("cs-formats", 141, 0xfb56968e3228aa7f),
        ("de", 1588, 0x140191e69d30adc6),
        ("de-formats", 141, 0x930f5b4c22202dba),
        ("es", 1588, 0xef865755126c2c5c),
        ("es-formats", 141, 0x99c19f2c12994f8f),
        ("fr", 1588, 0x43b2a963728cbdc4),
        ("fr-formats", 141, 0x85a9f25ec1e5c04a),
        ("id", 1588, 0x4be022def7fc8da0),
        ("id-formats", 141, 0x6d1ccc37a77cdd76),
        ("it", 1588, 0x3ac92ecfb741717a),
        ("it-formats", 141, 0xe97ee9a4df104e89),
        ("ja", 1588, 0x64342cece04a34bf),
        ("ja-formats", 141, 0x8fa915f293f885fa),
        ("ko", 1588, 0x339ebaababae11b3),
        ("ko-formats", 141, 0x0c5fc82084c87a70),
        ("pt-br", 1588, 0xb4a89cb51a767215),
        ("pt-br-formats", 141, 0x5d36ca4f42d1146a),
        ("ru", 1588, 0x8bba3d7ff7a2804e),
        ("ru-formats", 141, 0xf8bbef8dfd55d501),
        ("zh-hans", 1588, 0x59205cff35e629f7),
        ("zh-hans-formats", 141, 0xe30832510c840b64),
        ("zh-hant", 1588, 0x0ba54315590834e9),
        ("zh-hant-formats", 141, 0xd4ab8a057a8114cc),
    ];
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("catalogs/library");
    let mut checked = 0;
    for language in Lang::ALL.into_iter().filter(|language| *language != Lang::En) {
        let code = language.code();
        let (count, hash) = GOLDEN.iter().find(|(name, ..)| *name == code).map(|(_, n, h)| (*n, *h)).expect("golden for every language");
        assert_eq!(digest(library_entries(language)), (count, hash), "{code}: the Library catalog changed");
        let name = format!("{code}-formats");
        let golden = GOLDEN.iter().find(|(n, ..)| *n == name).map(|(_, n, h)| (*n, *h)).expect("golden for every format catalog");
        let text = std::fs::read_to_string(dir.join(format!("{name}.tsv"))).expect("format catalog");
        let (entries, errors) = parse_entries(&text);
        assert!(errors.is_empty(), "{name}: {errors:?}");
        assert_eq!(digest(entries.iter().map(|(_, s, t)| (s.as_str(), t.as_str()))), golden, "{name}: the format strings changed");
        checked += 2;
    }
    assert_eq!(checked, GOLDEN.len(), "every converted catalog is checked");
}

/// The two catalogs stay separate, so a string both halves show keeps each half's wording
/// (Japanese "Exposure" reads 露出 in Develop and 露光量 in Compositing, as before the merge), and
/// a string only one half translates is not borrowed by the other (no new translations).
#[test]
fn j29_each_half_keeps_its_own_catalog() {
    assert_eq!(library::tr(Lang::Ja, "Exposure"), "露出");
    assert_eq!(tr(Lang::Ja, "Exposure"), "露光量");
    let editor_only = catalog(Lang::Ja, Domain::Editor)
        .plain_entries()
        .map(|(source, _)| source)
        .find(|source| catalog(Lang::Ja, Domain::Library).plain(source).is_none())
        .expect("an editor-only string");
    assert_eq!(library::tr(Lang::Ja, editor_only), editor_only, "the Library falls back to English as before");
    let library_only = library_entries(Lang::Ja).into_iter().map(|(source, _)| source).find(|source| !has(Lang::Ja, source)).expect("a Library-only string");
    assert_eq!(tr(Lang::Ja, library_only), library_only, "the editor falls back to English as before");
    // English is the source in both halves.
    for source in ["Exposure", "Layer", "Import Photos…"] {
        assert_eq!(library::tr(Lang::En, source), source);
        assert_eq!(tr(Lang::En, source), source);
    }
}

/// The one language registry keeps both old lookups' answers for every code and the tags
/// each engine resolved, and the settings file still stores the code.
#[test]
fn j29_one_registry_resolves_both_engines_codes() {
    for language in Lang::ALL {
        assert_eq!(Lang::from_code(language.code()), Some(language));
        assert_eq!(Lang::from_code(&language.code().to_ascii_uppercase()), Some(language), "codes ignore ASCII case");
        assert_eq!(Lang::from_pref(language.code()), language);
        let saved = serde_json::to_string(&language).expect("serialize");
        assert_eq!(saved, format!("\"{}\"", language.code()));
        assert_eq!(serde_json::from_str::<Lang>(&saved).expect("deserialize"), language);
    }
    // Lenient codes in saved settings, as the Library accepted them.
    assert_eq!(serde_json::from_str::<Lang>("\"zh-TW\"").expect("tag"), Lang::ZhHant);
    assert!(serde_json::from_str::<Lang>("\"xx\"").is_err());
    // The registry rows are indexed by variant: `ALL`, the rows and the variants agree.
    for (index, language) in Lang::ALL.into_iter().enumerate() {
        assert_eq!(language as usize, index);
    }
    let codes: Vec<&str> = Lang::ALL.into_iter().map(Lang::code).collect();
    assert_eq!(codes, ["en", "ja", "zh-hans", "zh-hant", "es", "ru", "cs", "fr", "id", "ko", "de", "pt-br", "it"]);
    let scripts: Vec<&str> = Lang::ALL.into_iter().map(Lang::script).collect();
    assert_eq!(scripts, ["Latn", "Jpan", "Hans", "Hant", "Latn", "Cyrl", "Latn", "Latn", "Latn", "Kore", "Latn", "Latn", "Latn"]);
}
