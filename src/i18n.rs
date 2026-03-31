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
    let is_known = SUPPORTED_LANGUAGES
        .iter()
        .any(|&(code, _)| code != "system" && code == language);

    if is_known {
        match language {
            "zh-CN" => "zh-CN",
            "en" => "en",
            _ => "en",
        }
    } else {
        "en"
    }
}

/// Apply a language setting to the runtime i18n system.
///
/// This calls `rust_i18n::set_locale()` and should only be called from the
/// main thread (see module-level doc).
pub fn apply_locale(language: &str) {
    let locale = resolve_locale(language);
    rust_i18n::set_locale(locale);
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
        apply_locale("en");
        assert_eq!(rust_i18n::locale().to_string(), "en");
    }

    #[test]
    fn apply_locale_sets_zh_cn() {
        apply_locale("zh-CN");
        assert_eq!(rust_i18n::locale().to_string(), "zh-CN");
        // 恢复到 en，避免影响其他测试
        apply_locale("en");
    }

    #[test]
    fn apply_locale_invalid_falls_back_to_en() {
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

    // ── i18n key coverage ────────────────────────────────────

    #[test]
    fn all_hint_keys_exist_in_locales() {
        // 代码中使用的所有 hint key（仅包含面向用户的提示）
        let required_keys = [
            "hint.set_env_var",
            "hint.login_cli",
            "hint.relogin_cli",
            "hint.refresh_cli",
            "hint.login_app",
            "hint.cli_exit_failed",
            "hint.api_http_error",
            "hint.api_error",
            "hint.no_oauth_creds",
            "hint.both_unavailable",
            "hint.trust_folder",
            "hint.cannot_parse_quota",
            "hint.token_still_invalid",
        ];

        // 测试每个 locale
        for locale in ["en", "zh-CN"] {
            rust_i18n::set_locale(locale);
            for key in &required_keys {
                let result = rust_i18n::t!(*key);
                // 如果 key 不存在，rust_i18n 会返回 key 本身
                assert_ne!(
                    result, *key,
                    "Missing i18n key '{}' in locale '{}'",
                    key, locale
                );
            }
        }

        // 恢复到 en
        rust_i18n::set_locale("en");
    }
}
