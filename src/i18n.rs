//! Internationalization (i18n) module.
//!
//! Provides locale detection, normalization, and the `apply_locale()` entry
//! point that bridges the `language` field in `AppSettings` to the runtime
//! `rust_i18n::set_locale()` call.
//!
//! **Thread-safety note**: `apply_locale()` must only be called from the main
//! (UI) thread — at startup and when the user changes the language setting.
//! Background refresh threads read the current locale via `t!()` which is safe
//! for concurrent reads, but concurrent writes would be a data race.

/// Supported languages: `(locale_code, display_name_i18n_key)`.
///
/// The first entry (`"system"`) is a virtual code meaning "follow OS locale".
pub const SUPPORTED_LANGUAGES: &[(&str, &str)] = &[
    ("system", "lang.system"),
    ("en", "lang.en"),
    ("zh-CN", "lang.zh_CN"),
];

/// Normalize a raw system locale string (e.g. `"zh_Hans-CN"`, `"en-US"`)
/// into one of the supported locale codes.
///
/// Rules:
/// - Any string starting with `"zh"` → `"zh-CN"`
/// - Everything else → `"en"`
pub fn normalize_locale(raw: &str) -> &'static str {
    let lower = raw.to_lowercase().replace('_', "-");
    if lower.starts_with("zh") {
        "zh-CN"
    } else {
        "en"
    }
}

/// Resolve a language setting to a concrete locale code.
///
/// - `"system"` → detect via `sys_locale` then normalize
/// - known code (e.g. `"en"`, `"zh-CN"`) → use directly
/// - anything else → fallback to `"en"`
pub fn resolve_locale(language: &str) -> &'static str {
    if language == "system" {
        let sys = sys_locale::get_locale().unwrap_or_else(|| "en".to_string());
        return normalize_locale(&sys);
    }

    // Check if language matches a known concrete code (skip "system")
    SUPPORTED_LANGUAGES
        .iter()
        .find(|&&(code, _)| code != "system" && code == language)
        .map(|&(code, _)| code)
        .unwrap_or("en")
}

/// Apply a language setting to the runtime i18n system.
///
/// This calls `rust_i18n::set_locale()` and should only be called from the
/// main thread (see module-level doc).
pub fn apply_locale(language: &str) {
    let locale = resolve_locale(language);
    rust_i18n::set_locale(locale);
}

#[cfg(test)]
pub(crate) type TestLocaleGuard = std::sync::MutexGuard<'static, ()>;

#[cfg(test)]
pub(crate) fn test_locale_guard(locale: &str) -> TestLocaleGuard {
    use std::sync::{Mutex, OnceLock};

    static LOCALE_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

    let guard = LOCALE_TEST_MUTEX
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("locale test mutex poisoned");
    rust_i18n::set_locale(locale);
    guard
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
    use serde_yml::Value;
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::{Path, PathBuf};

    const EN_LOCALE: &str = include_str!("../locales/en.yml");
    const ZH_CN_LOCALE: &str = include_str!("../locales/zh-CN.yml");

    // ── normalize_locale ─────────────────────────────────────

    #[test]
    fn normalize_zh_variants() {
        assert_eq!(normalize_locale("zh-CN"), "zh-CN");
        assert_eq!(normalize_locale("zh_CN"), "zh-CN");
        assert_eq!(normalize_locale("zh-Hans"), "zh-CN");
        assert_eq!(normalize_locale("zh_Hans-CN"), "zh-CN");
        assert_eq!(normalize_locale("zh-TW"), "zh-CN"); // 目前统一到 zh-CN
        assert_eq!(normalize_locale("ZH-CN"), "zh-CN"); // 大写
    }

    #[test]
    fn normalize_en_variants() {
        assert_eq!(normalize_locale("en"), "en");
        assert_eq!(normalize_locale("en-US"), "en");
        assert_eq!(normalize_locale("en-GB"), "en");
        assert_eq!(normalize_locale("EN-US"), "en");
    }

    #[test]
    fn normalize_unknown_falls_back_to_en() {
        assert_eq!(normalize_locale("ja-JP"), "en");
        assert_eq!(normalize_locale("fr-FR"), "en");
        assert_eq!(normalize_locale("ko"), "en");
        assert_eq!(normalize_locale(""), "en");
    }

    // ── resolve_locale ───────────────────────────────────────

    #[test]
    fn resolve_known_codes() {
        assert_eq!(resolve_locale("en"), "en");
        assert_eq!(resolve_locale("zh-CN"), "zh-CN");
    }

    #[test]
    fn resolve_unknown_falls_back() {
        assert_eq!(resolve_locale("ja-JP"), "en");
        assert_eq!(resolve_locale("invalid"), "en");
        assert_eq!(resolve_locale(""), "en");
    }

    #[test]
    fn resolve_system_returns_valid_code() {
        // "system" should always resolve to a known locale code
        let result = resolve_locale("system");
        assert!(
            result == "en" || result == "zh-CN",
            "system locale resolved to unexpected code: {}",
            result
        );
    }

    // ── apply_locale ─────────────────────────────────────────

    #[test]
    fn apply_locale_sets_en() {
        let _locale_guard = test_locale_guard("en");
        apply_locale("en");
        assert_eq!(rust_i18n::locale().to_string(), "en");
    }

    #[test]
    fn apply_locale_sets_zh_cn() {
        let _locale_guard = test_locale_guard("en");
        apply_locale("zh-CN");
        assert_eq!(rust_i18n::locale().to_string(), "zh-CN");
        // 恢复到 en，避免影响其他测试
        apply_locale("en");
    }

    #[test]
    fn apply_locale_invalid_falls_back_to_en() {
        let _locale_guard = test_locale_guard("en");
        apply_locale("invalid-locale");
        assert_eq!(rust_i18n::locale().to_string(), "en");
    }

    // ── SUPPORTED_LANGUAGES 一致性 ────────────────────────────

    #[test]
    fn supported_languages_first_is_system() {
        assert_eq!(SUPPORTED_LANGUAGES[0].0, "system");
    }

    #[test]
    fn supported_languages_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for &(code, _) in SUPPORTED_LANGUAGES {
            assert!(
                seen.insert(code),
                "duplicate locale code in SUPPORTED_LANGUAGES: {}",
                code
            );
        }
    }

    #[test]
    fn supported_languages_all_concrete_codes_resolve_to_self() {
        for &(code, _) in SUPPORTED_LANGUAGES {
            if code == "system" {
                continue;
            }
            assert_eq!(
                resolve_locale(code),
                code,
                "concrete code '{}' should resolve to itself",
                code
            );
        }
    }

    // ── i18n resource coverage ───────────────────────────────

    fn locale_keys(contents: &str) -> BTreeSet<String> {
        let map: BTreeMap<String, Value> =
            serde_yml::from_str(contents).expect("locale file should be valid YAML");
        map.into_keys().filter(|key| key != "_version").collect()
    }

    fn locale_sets() -> [(&'static str, BTreeSet<String>); 2] {
        [
            ("en", locale_keys(EN_LOCALE)),
            ("zh-CN", locale_keys(ZH_CN_LOCALE)),
        ]
    }

    fn collect_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(dir)
            .unwrap_or_else(|err| panic!("failed to read {}: {}", dir.display(), err));

        for entry in entries {
            let path = entry.expect("failed to read src entry").path();
            if path.is_dir() {
                collect_rs_files(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    fn code_referenced_i18n_keys() -> BTreeSet<String> {
        let t_literal_re = Regex::new(r#"(?:rust_i18n::)?\bt!\(\s*"([A-Za-z0-9_.-]+)""#).unwrap();
        let dynamic_key_field_re = Regex::new(
            r#"(?:placeholder_i18n_key|help_tip_i18n_key|title_i18n_key|description_i18n_key|source_i18n_key):\s*(?:Some\()?\"([A-Za-z0-9_.-]+)\""#,
        )
        .unwrap();

        let mut files = Vec::new();
        collect_rs_files(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );

        let mut keys = BTreeSet::new();
        for path in files {
            let contents = fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("failed to read {}: {}", path.display(), err));

            for captures in t_literal_re.captures_iter(&contents) {
                keys.insert(captures[1].to_string());
            }
            for captures in dynamic_key_field_re.captures_iter(&contents) {
                keys.insert(captures[1].to_string());
            }
        }

        for &(_, display_name_key) in SUPPORTED_LANGUAGES {
            keys.insert(display_name_key.to_string());
        }

        for key in [
            "newapi.save_success_title",
            "newapi.save_success_body",
            "newapi.save_partial_title",
            "newapi.save_partial_body",
            "newapi.edit_success_title",
            "newapi.edit_success_body",
        ] {
            keys.insert(key.to_string());
        }

        keys
    }

    #[test]
    fn locale_files_have_same_keys() {
        let [(_, en_keys), (_, zh_cn_keys)] = locale_sets();
        assert_eq!(en_keys, zh_cn_keys);
    }

    #[test]
    fn all_code_referenced_keys_exist_in_locales() {
        let _locale_guard = test_locale_guard("en");

        let required_keys = code_referenced_i18n_keys();
        for (locale, keys) in locale_sets() {
            for key in &required_keys {
                assert!(
                    keys.contains(key),
                    "Missing i18n key '{}' in locale '{}'",
                    key,
                    locale
                );
            }
        }
    }

    #[test]
    fn all_locale_keys_resolve_at_runtime() {
        let _locale_guard = test_locale_guard("en");

        for (locale, keys) in locale_sets() {
            rust_i18n::set_locale(locale);
            for key in keys {
                let result = rust_i18n::t!(key.as_str());
                assert_ne!(
                    result, key,
                    "i18n key '{}' is present in locale file but missing from rust-i18n runtime for '{}'",
                    key, locale
                );
            }
        }

        rust_i18n::set_locale("en");
    }
}
