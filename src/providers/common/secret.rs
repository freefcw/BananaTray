/// 用统一的 Unicode-safe 规则脱敏 secret 预览：
/// - 仅展示前后各 4 个字符
/// - 中间使用调用方指定的 mask
/// - 短字符串由调用方决定如何显示
pub(crate) fn mask_secret_preview<F>(secret: &str, middle_mask: &str, short_mask: F) -> String
where
    F: FnOnce(usize) -> String,
{
    const VISIBLE_CHARS: usize = 4;

    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= VISIBLE_CHARS * 2 {
        return short_mask(chars.len());
    }

    let prefix: String = chars[..VISIBLE_CHARS].iter().copied().collect();
    let suffix: String = chars[chars.len() - VISIBLE_CHARS..]
        .iter()
        .copied()
        .collect();
    format!("{}{}{}", prefix, middle_mask, suffix)
}

#[cfg(test)]
mod tests {
    use super::mask_secret_preview;

    #[test]
    fn preview_uses_callback_for_short_values() {
        let result = mask_secret_preview("测试", "…", |len| format!("<{}>", len));
        assert_eq!(result, "<2>");
    }

    #[test]
    fn preview_is_unicode_safe_for_long_values() {
        let result = mask_secret_preview("测试令牌abcdWXYZ", "••••", |_| {
            "••••••••".to_string()
        });
        assert_eq!(result, "测试令牌••••WXYZ");
    }
}
