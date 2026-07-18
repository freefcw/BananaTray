use anyhow::Result;
use log::{debug, warn};
use std::time::Duration;

use crate::providers::common::{cli, http_client};
use crate::providers::ProviderError;

use super::auth::resolve_auth_headers;
use super::log_utils::mask_auth_header;
use super::schema::{AuthDef, HeaderDef, HttpMethodDef, PreprocessStep, SourceDef};
use super::url::resolve_url;

pub(super) fn fetch(id: &str, base_url: &Option<String>, source: &SourceDef) -> Result<String> {
    match source {
        SourceDef::Cli {
            command,
            args,
            timeout_ms,
        } => {
            debug!(target: "providers::custom", "[{}] fetching via CLI: {} {:?}", id, command, args);
            fetch_cli(command, args, *timeout_ms)
        }
        SourceDef::Http {
            method,
            url,
            timeout_ms,
            auth,
            headers,
            body,
        } => {
            let resolved = resolve_url(base_url, url);
            debug!(target: "providers::custom", "[{}] fetching via HTTP {:?}: {}", id, method, resolved);
            let result = fetch_http(
                base_url,
                *method,
                &resolved,
                *timeout_ms,
                auth,
                headers,
                body,
            );
            if let Err(ref e) = result {
                warn!(target: "providers::custom", "[{}] HTTP {:?} failed: {}", id, method, e);
            }
            result
        }
        SourceDef::Placeholder { reason } => {
            debug!(target: "providers::custom", "[{}] placeholder source, reason: {}", id, reason);
            Err(ProviderError::unavailable(reason).into())
        }
    }
}

/// 应用预处理管道。
pub(super) fn apply_preprocess(raw: &str, steps: &[PreprocessStep]) -> String {
    let mut result = raw.to_string();
    for step in steps {
        match step {
            PreprocessStep::StripAnsi => {
                result = crate::utils::text_utils::strip_terminal_noise(&result);
            }
        }
    }
    result
}

fn fetch_cli(command: &str, args: &[String], timeout_ms: Option<u64>) -> Result<String> {
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cli::run_lenient_command_with_timeout(command, &args_ref, timeout_ms.map(Duration::from_millis))
}

fn fetch_http(
    base_url: &Option<String>,
    method: HttpMethodDef,
    resolved_url: &str,
    timeout_ms: Option<u64>,
    auth: &Option<AuthDef>,
    headers: &[HeaderDef],
    body: &Option<String>,
) -> Result<String> {
    let header_strings = resolve_auth_headers(base_url, auth, headers)?;
    debug!(
        target: "providers::custom",
        "request headers ({}): {:?}",
        header_strings.len(),
        header_strings.iter().map(|h| mask_auth_header(h)).collect::<Vec<_>>()
    );
    let header_refs: Vec<&str> = header_strings.iter().map(|s| s.as_str()).collect();
    let timeout = timeout_ms.map(Duration::from_millis);
    match method {
        HttpMethodDef::Get => http_client::get_with_timeout(resolved_url, &header_refs, timeout),
        HttpMethodDef::Post => {
            let body = body.as_deref().ok_or_else(|| {
                anyhow::anyhow!("HTTP POST requires a body but none was provided")
            })?;
            http_client::post_json_with_timeout(resolved_url, &header_refs, body, timeout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder_source_returns_unavailable() {
        let result = fetch(
            "test:placeholder",
            &None,
            &SourceDef::Placeholder {
                reason: "No public API available".to_string(),
            },
        );
        let err = result.unwrap_err();
        assert!(err.to_string().contains("No public API available"));
    }

    #[test]
    fn test_apply_preprocess_empty_steps() {
        let raw = "hello \x1b[32mworld\x1b[0m";
        let result = apply_preprocess(raw, &[]);
        assert_eq!(result, raw);
    }

    #[test]
    fn test_apply_preprocess_strip_ansi() {
        let raw = "Usage: \x1b[1m\x1b[32m25\x1b[0m / \x1b[1m100\x1b[0m requests";
        let result = apply_preprocess(raw, &[PreprocessStep::StripAnsi]);
        assert_eq!(result, "Usage: 25 / 100 requests");
    }

    #[test]
    fn test_apply_preprocess_strip_ansi_with_progress_chars() {
        let raw = "⣾⣽⣻ Loading...\x1b[2K\x1b[1AUsage: 10/50\n";
        let result = apply_preprocess(raw, &[PreprocessStep::StripAnsi]);
        assert!(result.contains("Usage: 10/50"));
        assert!(!result.contains("\x1b["));
    }
}
