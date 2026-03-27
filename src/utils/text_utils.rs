//! Shared text utilities.

use regex::Regex;
use std::sync::LazyLock;

static ANSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\][^\x07]*\x07").unwrap());

/// Strip ANSI escape sequences (CSI and OSC) from text.
pub fn strip_ansi(text: &str) -> String {
    ANSI_RE.replace_all(text, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
