use log::warn;

/// 将相对路径（以 / 开头）拼接到 base_url 上，绝对 URL 直接返回。
/// 同时支持 `${ENV_VAR}` 展开。
pub(super) fn resolve_url(base_url: &Option<String>, url: &str) -> String {
    let expanded = expand_env_vars(url);
    match base_url {
        Some(base) if expanded.starts_with('/') => {
            let base = expand_env_vars(base.trim_end_matches('/'));
            format!("{}{}", base, expanded)
        }
        _ => expanded,
    }
}

/// 展开路径中的 ~ 为用户 home 目录。
pub(super) fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

/// 展开字符串中的 `${ENV_VAR}` 引用。
pub(super) fn expand_env_vars(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next();
            let var_name: String = chars.by_ref().take_while(|&ch| ch != '}').collect();
            match std::env::var(&var_name) {
                Ok(val) => result.push_str(&val),
                Err(_) => {
                    warn!(
                        target: "providers::custom",
                        "Environment variable '{}' is not set, expanding to empty string",
                        var_name
                    );
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/test/path");
        assert!(!expanded.starts_with('~'));
        assert!(expanded.ends_with("test/path"));
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        assert_eq!(expand_tilde("/absolute/path"), "/absolute/path");
    }

    #[test]
    fn test_expand_env_vars() {
        std::env::set_var("TEST_FETCHER_VAR", "hello");
        assert_eq!(
            expand_env_vars("prefix-${TEST_FETCHER_VAR}-suffix"),
            "prefix-hello-suffix"
        );
        std::env::remove_var("TEST_FETCHER_VAR");
    }

    #[test]
    fn test_expand_env_vars_no_vars() {
        assert_eq!(expand_env_vars("plain text"), "plain text");
    }

    #[test]
    fn test_expand_env_vars_missing() {
        assert_eq!(expand_env_vars("${NONEXISTENT_VAR_12345}"), "");
    }

    #[test]
    fn test_expand_env_vars_multiple_vars() {
        std::env::set_var("TEST_EV_A", "hello");
        std::env::set_var("TEST_EV_B", "world");
        assert_eq!(expand_env_vars("${TEST_EV_A}-${TEST_EV_B}"), "hello-world");
        std::env::remove_var("TEST_EV_A");
        std::env::remove_var("TEST_EV_B");
    }

    #[test]
    fn test_expand_env_vars_dollar_without_brace() {
        assert_eq!(expand_env_vars("$plain"), "$plain");
    }

    #[test]
    fn test_expand_env_vars_empty_var_name() {
        assert_eq!(expand_env_vars("before${}after"), "beforeafter");
    }

    #[test]
    fn test_resolve_url_relative_path() {
        let base = Some("https://example.com".to_string());
        assert_eq!(
            resolve_url(&base, "/api/user/self"),
            "https://example.com/api/user/self"
        );
    }

    #[test]
    fn test_resolve_url_absolute_url_unchanged() {
        let base = Some("https://example.com".to_string());
        assert_eq!(
            resolve_url(&base, "https://other.com/api"),
            "https://other.com/api"
        );
    }

    #[test]
    fn test_resolve_url_no_base() {
        assert_eq!(
            resolve_url(&None, "https://example.com/api"),
            "https://example.com/api"
        );
    }

    #[test]
    fn test_resolve_url_base_trailing_slash() {
        let base = Some("https://example.com/".to_string());
        assert_eq!(
            resolve_url(&base, "/api/user/self"),
            "https://example.com/api/user/self"
        );
    }
}
