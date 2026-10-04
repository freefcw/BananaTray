use crate::providers::common::secret::mask_secret_preview;
use regex::Regex;
use std::sync::OnceLock;

const DETAIL_LIMIT: usize = 160;

struct Span {
    start: usize,
    end: usize,
    replacement: String,
}

/// 入库前的 `raw_detail` 脱敏。先占住的区间不再被后面的规则改写，全部遮完再截断。
pub fn scrub_history_detail(raw: &str) -> String {
    let mut spans = Vec::new();
    claim_whole(&mut spans, raw, email_re(), |_| {
        "<redacted-email>".to_string()
    });
    claim_group(raw, &mut spans, userinfo_re(), 3);
    claim_group(raw, &mut spans, bearer_re(), 1);
    claim_group(raw, &mut spans, query_re(), 2);
    claim_group(raw, &mut spans, header_re(), 2);
    claim_whole(&mut spans, raw, secret_prefix_re(), mask_preview);
    claim_whole(&mut spans, raw, hex_re(), mask_preview);
    claim_whole(&mut spans, raw, base64_re(), mask_preview);

    spans.sort_by_key(|span| span.start);
    let mut out = String::new();
    let mut cursor = 0;
    for span in spans {
        if span.start < cursor {
            continue;
        }
        out.push_str(&raw[cursor..span.start]);
        out.push_str(&span.replacement);
        cursor = span.end;
    }
    out.push_str(&raw[cursor..]);
    out.chars().take(DETAIL_LIMIT).collect()
}

fn mask_preview(value: &str) -> String {
    mask_secret_preview(value, "••••", |_| "••••".to_string())
}

fn overlaps(spans: &[Span], start: usize, end: usize) -> bool {
    spans
        .iter()
        .any(|span| start < span.end && end > span.start)
}

fn claim_whole(spans: &mut Vec<Span>, raw: &str, re: &Regex, replace: impl Fn(&str) -> String) {
    for found in re.find_iter(raw) {
        if overlaps(spans, found.start(), found.end()) {
            continue;
        }
        spans.push(Span {
            start: found.start(),
            end: found.end(),
            replacement: replace(found.as_str()),
        });
    }
}

fn claim_group(raw: &str, spans: &mut Vec<Span>, re: &Regex, group: usize) {
    for found in re.captures_iter(raw) {
        let Some(value) = found.get(group) else {
            continue;
        };
        if value.as_str().is_empty() || overlaps(spans, value.start(), value.end()) {
            continue;
        }
        spans.push(Span {
            start: value.start(),
            end: value.end(),
            replacement: mask_preview(value.as_str()),
        });
    }
}

fn email_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").expect("email regex")
    })
}

fn userinfo_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)([a-z][a-z0-9+.-]*://)([^/\s:@]*):([^/\s@]+)@").expect("userinfo regex")
    })
}

fn bearer_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)bearer\s+([A-Za-z0-9\-._~+/]+=*)").expect("bearer regex"))
}

fn query_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)([?&](?:token|api_key|apikey|access_token|refresh_token|key|cookie)=)([^&#\s]+)",
        )
        .expect("query regex")
    })
}

fn header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(authorization|cookie|x-api-key|api-key)\s*[:=]\s*([^\r\n]+)")
            .expect("header regex")
    })
}

fn secret_prefix_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"sk-[A-Za-z0-9_\-]{8,}").expect("secret prefix regex"))
}

fn hex_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[0-9a-fA-F]{32,}\b").expect("hex regex"))
}

fn base64_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[A-Za-z0-9+/]{32,}={0,2}\b").expect("base64 regex"))
}

#[cfg(test)]
mod tests {
    use super::scrub_history_detail;

    #[test]
    fn cookie_header_masks_the_value_through_end_of_line() {
        let scrubbed = scrub_history_detail("Cookie: a=b; session=second-secret");
        assert!(!scrubbed.contains("second-secret"));
        assert!(scrubbed.to_lowercase().contains("cookie"));
    }

    #[test]
    fn bearer_span_is_not_rewritten_by_the_header_rule() {
        let scrubbed = scrub_history_detail("Authorization: Bearer sk-abcdefghijklmnopqrstuvwxyz");
        assert!(!scrubbed.contains("sk-abcdefghijklmnopqrstuvwxyz"));
        assert!(!scrubbed.contains("abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn email_is_replaced_without_showing_the_local_part() {
        let scrubbed = scrub_history_detail("user@example.com failed");
        assert_eq!(scrubbed, "<redacted-email> failed");
    }

    #[test]
    fn truncation_happens_after_masking() {
        let secret = format!("token={}", "k".repeat(200));
        let scrubbed = scrub_history_detail(&secret);
        assert!(scrubbed.chars().count() <= 160);
        assert!(!scrubbed.contains(&"k".repeat(20)));
    }
}
