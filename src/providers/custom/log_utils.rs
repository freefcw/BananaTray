/// 截断长文本用于日志输出，避免日志爆炸。
///
/// 使用 char_indices 确保截断在字符边界上，避免多字节 UTF-8 切割 panic。
pub(super) fn truncate_for_log(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let safe_end = s
            .char_indices()
            .take_while(|(i, _)| *i <= max_len)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        format!("{}...(truncated, total {} bytes)", &s[..safe_end], s.len())
    }
}

/// 脱敏 auth 头信息：只显示 value 的前几个字符。
pub(super) fn mask_auth_header(header: &str) -> String {
    const VISIBLE_LEN: usize = 8;
    if let Some((name, value)) = header.split_once(':') {
        let value = value.trim();
        let masked = if value.len() > VISIBLE_LEN {
            let safe_end = value
                .char_indices()
                .take_while(|(i, _)| *i <= VISIBLE_LEN)
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            format!("{}...", &value[..safe_end])
        } else {
            value.to_string()
        };
        format!("{}: {}", name.trim(), masked)
    } else {
        header.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_string_unchanged() {
        assert_eq!(truncate_for_log("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length_unchanged() {
        assert_eq!(truncate_for_log("12345", 5), "12345");
    }

    #[test]
    fn test_truncate_long_ascii() {
        let result = truncate_for_log("abcdefghij", 5);
        assert!(result.starts_with("abcde"));
        assert!(result.contains("truncated"));
        assert!(result.contains("10 bytes"));
    }

    #[test]
    fn test_truncate_multibyte_no_panic() {
        let s = "你好世界";
        let result = truncate_for_log(s, 5);
        assert!(result.starts_with("你"));
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_truncate_empty_string() {
        assert_eq!(truncate_for_log("", 10), "");
    }

    #[test]
    fn test_mask_short_value_unchanged() {
        assert_eq!(mask_auth_header("X-Key: abc"), "X-Key: abc");
    }

    #[test]
    fn test_mask_long_value_truncated() {
        let result = mask_auth_header("Authorization: Bearer sk-very-long-token-123");
        assert_eq!(result, "Authorization: Bearer s...");
    }

    #[test]
    fn test_mask_no_colon() {
        assert_eq!(mask_auth_header("no-colon-header"), "no-colon-header");
    }

    #[test]
    fn test_mask_multibyte_no_panic() {
        let result = mask_auth_header("Cookie: 这是一个很长的中文值用于测试");
        assert!(result.starts_with("Cookie:"));
        assert!(result.contains("..."));
    }
}
