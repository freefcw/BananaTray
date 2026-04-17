use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::providers::ProviderError;
use anyhow::Result;

pub(super) fn parse_usage_response(raw: &str) -> Result<Vec<QuotaInfo>> {
    let mut quotas = Vec::new();

    let (headers, body) = if let Some(idx) = raw.find("\r\n\r\n") {
        (&raw[..idx], raw[idx + 4..].trim())
    } else if let Some(idx) = raw.find("\n\n") {
        (&raw[..idx], raw[idx + 2..].trim())
    } else {
        ("", raw.trim())
    };

    // 注意：401/403 认证错误已在 http_client 层通过 HttpError::HttpStatus 结构化返回，
    // 不再需要在此处做字符串匹配。

    let mut found_headers = false;
    let mut primary_percent: Option<f64> = None;
    let mut secondary_percent: Option<f64> = None;
    let mut credits_balance: Option<f64> = None;

    for line in headers.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("x-codex-primary-used-percent:") {
            primary_percent = line
                .split_once(':')
                .and_then(|(_, v)| v.trim().parse::<f64>().ok());
            found_headers = true;
        } else if lower.starts_with("x-codex-secondary-used-percent:") {
            secondary_percent = line
                .split_once(':')
                .and_then(|(_, v)| v.trim().parse::<f64>().ok());
            found_headers = true;
        } else if lower.starts_with("x-codex-credits-balance:") {
            credits_balance = line
                .split_once(':')
                .and_then(|(_, v)| v.trim().parse::<f64>().ok());
            found_headers = true;
        }
    }

    if found_headers {
        if let Some(primary) = primary_percent {
            quotas.push(QuotaInfo::with_details(
                QuotaLabelSpec::Session,
                primary,
                100.0,
                QuotaType::Session,
                None,
            ));
        }
        if let Some(secondary) = secondary_percent {
            quotas.push(QuotaInfo::with_details(
                QuotaLabelSpec::Weekly,
                secondary,
                100.0,
                QuotaType::Weekly,
                None,
            ));
        }
        if let Some(credits) = credits_balance {
            quotas.push(QuotaInfo::with_details(
                QuotaLabelSpec::Credits,
                0.0,
                credits,
                QuotaType::Credit,
                None,
            ));
        }
        return Ok(quotas);
    }

    if body.is_empty() {
        return Err(ProviderError::no_data().into());
    }

    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| ProviderError::parse_failed("usage API response"))?;

    if let Some(rate_limit) = json.get("rate_limit") {
        if let Some(primary) = rate_limit.get("primary_window") {
            let used = primary
                .get("used_percent")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let reset_at = primary.get("reset_at").and_then(|v| v.as_i64());

            quotas.push(QuotaInfo::with_details(
                QuotaLabelSpec::Session,
                used,
                100.0,
                QuotaType::Session,
                reset_at.map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs }),
            ));
        }

        if let Some(secondary) = rate_limit.get("secondary_window") {
            let used = secondary
                .get("used_percent")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let reset_at = secondary.get("reset_at").and_then(|v| v.as_i64());

            quotas.push(QuotaInfo::with_details(
                QuotaLabelSpec::Weekly,
                used,
                100.0,
                QuotaType::Weekly,
                reset_at.map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs }),
            ));
        }
    }

    if quotas.is_empty() {
        return Err(ProviderError::no_data().into());
    }

    Ok(quotas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_headers_response() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = "HTTP/1.1 200 OK\r\nx-codex-primary-used-percent: 25\r\nx-codex-secondary-used-percent: 80\r\nx-codex-credits-balance: 12.5\r\n\r\n";
        let quotas = parse_usage_response(raw).unwrap();
        assert_eq!(quotas.len(), 3);
        assert_eq!(quotas[0].label_spec, QuotaLabelSpec::Session);
        assert_eq!(quotas[1].label_spec, QuotaLabelSpec::Weekly);
        assert_eq!(quotas[2].label_spec, QuotaLabelSpec::Credits);
    }

    #[test]
    fn test_parse_json_response() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": { "used_percent": 33, "reset_at": 1767225600 },
                "secondary_window": { "used_percent": 66, "reset_at": 1767312000 }
            }
        }"#;
        let quotas = parse_usage_response(raw).unwrap();
        assert_eq!(quotas.len(), 2);
        assert_eq!(quotas[0].used, 33.0);
        assert_eq!(quotas[1].used, 66.0);
        assert!(matches!(
            quotas[0].detail_spec,
            Some(QuotaDetailSpec::ResetAt { .. })
        ));
    }
}
