//! Native UI-language preferences, queried once without subprocesses or registry parsing.

use std::cell::RefCell;

use crate::Lang;

#[cfg(not(target_arch = "wasm32"))]
const MAX_SYSTEM_TAGS: usize = 64;
#[cfg(not(target_arch = "wasm32"))]
const MAX_SYSTEM_TAG_BYTES: usize = 128;

thread_local! {
    // A scoped stand-in for the OS's list on this thread ([`with_system_tags`]); `None` asks the OS.
    static OVERRIDE: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// Resolve Auto against the registry. Cache the OS's tags, rather than a language, so matching
/// remains separate from platform detection. Missing or unsupported system preferences always
/// resolve to English.
pub fn system_lang() -> Lang {
    if let Some(lang) = OVERRIDE.with_borrow(|tags| tags.as_deref().map(resolve)) {
        return lang;
    }
    // This crate's own tests never depend on the developer's locale.
    if cfg!(test) {
        return Lang::En;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        static SYSTEM: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        resolve(SYSTEM.get_or_init(detect_system_tags))
    }
    #[cfg(target_arch = "wasm32")]
    {
        Lang::En
    }
}

/// Resolve Auto against `tags` instead of the OS's list while `run` runs on this thread (tests
/// and reproducible previews). Nested calls restore the outer list, also when `run` unwinds.
pub fn with_system_tags<R>(tags: &[&str], run: impl FnOnce() -> R) -> R {
    struct Restore(Option<Vec<String>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OVERRIDE.set(self.0.take());
        }
    }
    let _restore = Restore(OVERRIDE.replace(Some(tags.iter().map(|tag| (*tag).to_owned()).collect())));
    run()
}

fn resolve(tags: &[String]) -> Lang {
    tags.iter().find_map(|tag| Lang::from_tag(tag)).unwrap_or(Lang::En)
}

#[cfg(not(target_arch = "wasm32"))]
fn bounded_tags(tags: impl IntoIterator<Item = String>) -> Vec<String> {
    tags.into_iter()
        .take(MAX_SYSTEM_TAGS)
        .filter(|tag| {
            !tag.trim().is_empty()
                && tag.len() <= MAX_SYSTEM_TAG_BYTES
                && tag.trim().bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'@'))
        })
        .map(|tag| tag.trim().to_owned())
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_system_tags() -> Vec<String> {
    if let Ok(tag) = std::env::var("PHOTOCRAFT_LOCALE")
        && !tag.trim().is_empty()
    {
        return bounded_tags([tag]);
    }
    // sys-locale exposes safe wrappers for GetUserPreferredUILanguages (Windows) and
    // CFLocaleCopyPreferredLanguages (macOS); Unix uses its standard locale environment.
    // A failure in platform interop must also leave the app usable in English.
    std::panic::catch_unwind(|| bounded_tags(sys_locale::get_locales())).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_preferences_are_negotiated_in_order() {
        for (tags, expected) in [
            (vec!["ja-JP", "en-US"], "ja"),
            (vec!["fr-CA", "en-US"], "fr"),
            (vec!["sv-SE", "ko-KR", "fr-FR"], "ko"),
            (vec!["zh-Hant-HK", "zh-CN"], "zh-hant"),
            (vec!["zh-Hans-CN", "zh-TW"], "zh-hans"),
            (vec!["en-US", "ru-RU"], "en"),
            (vec!["sv-SE", "ar-SA"], "en"),
            (vec!["de-AT", "en-US"], "de"),
            (vec!["pt-PT"], "pt-br"),
            (vec!["it-IT", "en-US"], "it"),
            (vec![], "en"),
        ] {
            with_system_tags(&tags, || assert_eq!(system_lang().code(), expected));
        }
    }

    #[test]
    fn test_system_preferences_restore_and_remain_thread_local() {
        with_system_tags(&["ko-KR"], || {
            with_system_tags(&["fr-FR"], || assert_eq!(system_lang().code(), "fr"));
            assert_eq!(system_lang().code(), "ko");
            std::thread::spawn(|| assert_eq!(system_lang(), Lang::En)).join().expect("locale test thread");
            assert!(std::panic::catch_unwind(|| with_system_tags(&["ja-JP"], || panic!("test unwind"))).is_err());
            assert_eq!(system_lang().code(), "ko");
        });
        assert_eq!(system_lang(), Lang::En);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_locale_results_are_bounded_and_trimmed() {
        assert_eq!(bounded_tags(["  fr-CA  ".into(), "".into(), " ".into(), "fr-\0".into(), "x".repeat(129)]), ["fr-CA"]);
        assert_eq!(bounded_tags(std::iter::repeat_n("en-US".into(), 100)).len(), MAX_SYSTEM_TAGS);
        let detected = detect_system_tags();
        assert!(detected.len() <= MAX_SYSTEM_TAGS);
        assert!(detected.iter().all(|tag| !tag.is_empty() && tag.len() <= MAX_SYSTEM_TAG_BYTES));
    }
}
