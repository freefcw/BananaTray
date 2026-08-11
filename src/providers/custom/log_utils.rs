/// 认证 header 日志只保留名称，值一律完全隐藏。
pub(super) fn mask_auth_header(header: &str) -> String {
    header
        .split_once(':')
        .map(|(name, _)| format!("{}: <redacted>", name.trim()))
        .unwrap_or_else(|| "<invalid header>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_header_value_is_fully_redacted() {
        assert_eq!(mask_auth_header("X-Key: abc"), "X-Key: <redacted>");
        assert_eq!(
            mask_auth_header("Authorization: Bearer sk-very-long-token-123"),
            "Authorization: <redacted>"
        );
    }

    #[test]
    fn test_mask_no_colon_does_not_echo_input() {
        assert_eq!(mask_auth_header("secret-without-colon"), "<invalid header>");
    }

    #[test]
    fn test_mask_multibyte_value_is_fully_redacted() {
        assert_eq!(
            mask_auth_header("Cookie: 这是一个很长的中文值用于测试"),
            "Cookie: <redacted>"
        );
    }
}
