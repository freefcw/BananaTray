//! Shared text utilities.

use regex::Regex;
use std::sync::LazyLock;

/// Matches common CSI sequences (colors, cursor movement, etc.) and OSC title sequences.
static ANSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\][^\x07]*\x07").unwrap());

/// Matches all terminal noise including ANSI, plus:
/// - CSI sequences: \x1B[...X (full spec: param bytes 0x30–3F, intermediate 0x20–2F, final 0x40–7E)
/// - OSC sequences: \x1B]...BEL or \x1B]...\x1B\\
/// - Cursor save/restore: \x1B7, \x1B8
/// - Braille spinner characters: U+2800..U+28FF
/// - Backspace: \x08
static TERMINAL_NOISE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:\x1B\[[\x30-\x3F]*[\x20-\x2F]*[\x40-\x7E]|\x1B\].*?(?:\x07|\x1B\\)|\x1B[78]|[\x{2800}-\x{28FF}]|\x08)",
    )
    .unwrap()
});

/// Strip ANSI escape sequences (CSI and OSC) from text.
pub fn strip_ansi(text: &str) -> String {
    ANSI_RE.replace_all(text, "").to_string()
}

/// Strip all terminal noise: ANSI escape sequences, cursor save/restore,
/// braille spinner characters, and backspaces.
///
/// Use this when processing raw PTY output that may contain spinners and
/// cursor manipulation in addition to standard color/style codes.
#[allow(dead_code)]
pub fn strip_terminal_noise(text: &str) -> String {
    TERMINAL_NOISE_RE.replace_all(text, "").to_string()
}

/// Check if byte data contains meaningful text content, i.e. something
/// beyond escape sequences, spinners, whitespace, and control characters.
pub fn has_meaningful_content(data: &[u8]) -> bool {
    if let Ok(text) = std::str::from_utf8(data) {
        let stripped = TERMINAL_NOISE_RE.replace_all(text, "");
        stripped
            .chars()
            .any(|c| !c.is_whitespace() && !c.is_control())
    } else {
        !data.is_empty()
    }
}

/// Normalize text for fuzzy matching: strip terminal noise, lowercase,
/// remove whitespace and control characters.
///
/// Useful for matching prompts/keywords in PTY output regardless of
/// color codes, cursor positioning, or spacing differences.
pub fn normalize_for_matching(text: &str) -> String {
    let stripped = TERMINAL_NOISE_RE.replace_all(text, "");
    stripped
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect()
}

/// 简易 URL 编码（percent-encoding），避免引入额外依赖
///
/// 对非 RFC 3986 unreserved 字符进行 %XX 编码。
/// 用途：拼接 GitHub issue/new URL 的查询参数。
pub fn url_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 2);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- strip_ansi ---

    #[test]
    fn test_strip_ansi_basic() {
        let input = "\x1b[31mhello\x1b[0m world";
        assert_eq!(strip_ansi(input), "hello world");
    }

    #[test]
    fn test_strip_ansi_osc() {
        let input = "\x1b]0;title\x07text";
        assert_eq!(strip_ansi(input), "text");
    }

    #[test]
    fn test_strip_ansi_no_sequences() {
        assert_eq!(strip_ansi("plain text"), "plain text");
    }

    // --- strip_terminal_noise ---

    #[test]
    fn test_strip_terminal_noise_csi_colors() {
        let input = "\x1b[32mModel: auto\x1b[0m (/model to change)";
        assert_eq!(
            strip_terminal_noise(input),
            "Model: auto (/model to change)"
        );
    }

    #[test]
    fn test_strip_terminal_noise_cursor_save_restore() {
        let input = "\x1b7⠙\x1b8";
        assert_eq!(strip_terminal_noise(input), "");
    }

    #[test]
    fn test_strip_terminal_noise_braille_spinners() {
        let input = "⠋ loading ⠙⠹⠸⠼";
        assert_eq!(strip_terminal_noise(input), " loading ");
    }

    #[test]
    fn test_strip_terminal_noise_backspace() {
        let input = "abc\x08\x08xy";
        assert_eq!(strip_terminal_noise(input), "abcxy");
    }

    #[test]
    fn test_strip_terminal_noise_mixed() {
        // Simulates kiro-cli output: cursor save + spinner + cursor restore + ANSI color prompt
        let input = "\x1b7⠋ 0 of 2 mcp servers\x1b8\x1b[32mPlan: KIRO FREE\x1b[0m (/usage for more detail)\r\n";
        let result = strip_terminal_noise(input);
        assert!(result.contains("Plan: KIRO FREE"));
        assert!(result.contains("/usage for more detail"));
        assert!(!result.contains('\x1b'));
        assert!(!result.contains('⠋'));
    }

    // --- has_meaningful_content ---

    #[test]
    fn test_meaningful_real_text() {
        assert!(has_meaningful_content(b"hello world"));
    }

    #[test]
    fn test_meaningful_spinner_only() {
        // cursor save + braille spinner + cursor restore
        assert!(!has_meaningful_content("\x1b7⠙\x1b8".as_bytes()));
    }

    #[test]
    fn test_meaningful_ansi_only() {
        assert!(!has_meaningful_content("\x1b[32m\x1b[0m".as_bytes()));
    }

    #[test]
    fn test_meaningful_whitespace_only() {
        assert!(!has_meaningful_content(b"   \n\t  "));
    }

    #[test]
    fn test_meaningful_text_with_ansi() {
        assert!(has_meaningful_content("\x1b[31mhello\x1b[0m".as_bytes()));
    }

    #[test]
    fn test_meaningful_non_utf8() {
        assert!(has_meaningful_content(&[0xFF, 0xFE]));
    }

    // --- normalize_for_matching ---

    #[test]
    fn test_normalize_plain_text() {
        assert_eq!(
            normalize_for_matching("Do you trust the files?"),
            "doyoutrustthefiles?"
        );
    }

    #[test]
    fn test_normalize_extra_whitespace() {
        assert_eq!(normalize_for_matching("  Ready  to  code  "), "readytocode");
    }

    #[test]
    fn test_normalize_with_ansi_codes() {
        let input = "\x1b[32mModel: auto\x1b[0m (/usage for more detail)";
        let result = normalize_for_matching(input);
        assert!(result.contains("/usageformoredetail"));
    }

    #[test]
    fn test_normalize_with_spinner_and_cursor() {
        let input = "\x1b7⠋ loading...\x1b8\x1b[32mReady\x1b[0m";
        let result = normalize_for_matching(input);
        assert_eq!(result, "loading...ready");
    }

    #[test]
    fn test_normalize_matching_kiro_prompt() {
        // Real kiro-cli output pattern
        let output = "\x1b[32mModel: auto\x1b[0m (/model to change) | Plan: \x1b[1mKIRO FREE\x1b[0m (/usage for more detail)\r\n";
        let key = normalize_for_matching("/usage for more detail");

        let normalized = normalize_for_matching(output);
        assert!(
            normalized.contains(&key),
            "normalized={:?} should contain key={:?}",
            normalized,
            key
        );
    }

    // --- url_encode ---

    #[test]
    fn test_url_encode_plain_text() {
        assert_eq!(url_encode("foo"), "foo");
    }

    #[test]
    fn test_url_encode_special_characters() {
        assert_eq!(url_encode("hello world"), "hello%20world");
        assert_eq!(url_encode("a=b&c"), "a%3Db%26c");
    }

    #[test]
    fn test_url_encode_unicode() {
        let encoded = url_encode("中文");
        assert!(encoded.contains("%E4%B8%AD"));
        assert!(encoded.contains("%E6%96%87"));
    }
}
