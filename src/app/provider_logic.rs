/// Pure formatting and business logic, free of any UI dependencies.
/// Extracted for testability (GPUI proc macros crash during test compilation).
use crate::models::{ConnectionStatus, ProviderKind, ProviderStatus, QuotaInfo};

pub fn format_amount(value: f64) -> String {
    if (value.fract() - 0.0).abs() < f64::EPSILON {
        format!("{:.0}", value)
    } else {
        format!("{:.1}", value)
    }
}

pub fn format_quota_usage(quota: &QuotaInfo) -> String {
    format!(
        "{} / {} used",
        format_amount(quota.used),
        format_amount(quota.limit)
    )
}

pub fn provider_empty_message(provider: &ProviderStatus) -> String {
    if let Some(error) = &provider.error_message {
        if error.contains("Missing environment variable") {
            return format!(
                "Connect {} credentials before quota tracking can start.",
                provider.kind.display_name()
            );
        }

        if error.contains("session cookie expired") {
            return "Session expired. Sign in again to refresh usage.".to_string();
        }

        return error.clone();
    }

    match provider.connection {
        ConnectionStatus::Error => {
            format!(
                "{} usage could not be refreshed right now.",
                provider.kind.display_name()
            )
        }
        ConnectionStatus::Disconnected => {
            format!(
                "Connect {} to start tracking quota.",
                provider.kind.display_name()
            )
        }
        ConnectionStatus::Connected => "No usage details available yet.".to_string(),
    }
}

pub fn provider_account_label(provider: &ProviderStatus, compact: bool) -> String {
    if let Some(email) = &provider.account_email {
        return email.clone();
    }

    if compact {
        match provider.kind {
            ProviderKind::Claude => "Anthropic".to_string(),
            ProviderKind::Gemini => "Google".to_string(),
            ProviderKind::Copilot => "GitHub".to_string(),
            ProviderKind::Codex => "OpenAI".to_string(),
            ProviderKind::Kimi => "Moonshot".to_string(),
            ProviderKind::Amp => "Amp CLI".to_string(),
        }
    } else {
        provider.kind.account_hint().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ConnectionStatus, ProviderKind, ProviderStatus, QuotaInfo};

    fn make_provider(kind: ProviderKind, connection: ConnectionStatus) -> ProviderStatus {
        ProviderStatus {
            kind,
            enabled: true,
            connection,
            quotas: vec![],
            account_email: None,
            is_paid: false,
            last_updated_at: None,
            error_message: None,
            last_refreshed_instant: None,
        }
    }

    // ── format_amount ────────────────────────────────────────

    #[test]
    fn format_amount_integer() {
        assert_eq!(format_amount(100.0), "100");
        assert_eq!(format_amount(0.0), "0");
    }

    #[test]
    fn format_amount_decimal() {
        assert_eq!(format_amount(3.5), "3.5");
        assert_eq!(format_amount(99.9), "99.9");
    }

    // ── format_quota_usage ───────────────────────────────────

    #[test]
    fn format_quota_usage_integers() {
        let q = QuotaInfo::new("Daily", 50.0, 100.0);
        assert_eq!(format_quota_usage(&q), "50 / 100 used");
    }

    #[test]
    fn format_quota_usage_decimals() {
        let q = QuotaInfo::new("Session", 3.5, 10.0);
        assert_eq!(format_quota_usage(&q), "3.5 / 10 used");
    }

    // ── provider_account_label ───────────────────────────────

    #[test]
    fn account_label_with_email() {
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.account_email = Some("user@example.com".into());
        assert_eq!(provider_account_label(&p, true), "user@example.com");
        assert_eq!(provider_account_label(&p, false), "user@example.com");
    }

    #[test]
    fn account_label_compact_without_email() {
        let p = make_provider(ProviderKind::Copilot, ConnectionStatus::Connected);
        assert_eq!(provider_account_label(&p, true), "GitHub");
    }

    #[test]
    fn account_label_verbose_without_email() {
        let p = make_provider(ProviderKind::Copilot, ConnectionStatus::Connected);
        assert_eq!(provider_account_label(&p, false), "GitHub account");
    }

    #[test]
    fn account_label_all_providers_compact() {
        let cases = [
            (ProviderKind::Claude, "Anthropic"),
            (ProviderKind::Gemini, "Google"),
            (ProviderKind::Copilot, "GitHub"),
            (ProviderKind::Codex, "OpenAI"),
            (ProviderKind::Kimi, "Moonshot"),
            (ProviderKind::Amp, "Amp CLI"),
        ];
        for (kind, expected) in cases {
            let p = make_provider(kind, ConnectionStatus::Connected);
            assert_eq!(provider_account_label(&p, true), expected);
        }
    }

    // ── provider_empty_message ───────────────────────────────

    #[test]
    fn empty_message_connected() {
        let p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        assert_eq!(
            provider_empty_message(&p),
            "No usage details available yet."
        );
    }

    #[test]
    fn empty_message_disconnected() {
        let p = make_provider(ProviderKind::Gemini, ConnectionStatus::Disconnected);
        assert_eq!(
            provider_empty_message(&p),
            "Connect Gemini to start tracking quota."
        );
    }

    #[test]
    fn empty_message_error() {
        let p = make_provider(ProviderKind::Copilot, ConnectionStatus::Error);
        assert_eq!(
            provider_empty_message(&p),
            "Copilot usage could not be refreshed right now."
        );
    }

    #[test]
    fn empty_message_missing_env_var() {
        let mut p = make_provider(ProviderKind::Copilot, ConnectionStatus::Error);
        p.error_message = Some("Missing environment variable GITHUB_TOKEN".into());
        assert_eq!(
            provider_empty_message(&p),
            "Connect Copilot credentials before quota tracking can start."
        );
    }

    #[test]
    fn empty_message_session_expired() {
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Error);
        p.error_message = Some("session cookie expired".into());
        assert_eq!(
            provider_empty_message(&p),
            "Session expired. Sign in again to refresh usage."
        );
    }

    #[test]
    fn empty_message_generic_error() {
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Error);
        p.error_message = Some("network timeout".into());
        assert_eq!(provider_empty_message(&p), "network timeout");
    }
}
