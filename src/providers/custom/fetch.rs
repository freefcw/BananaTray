use anyhow::Result;
use log::{debug, warn};

use crate::providers::common::{cli, http_client};
use crate::providers::ProviderError;

use super::auth::resolve_auth_headers;
use super::log_utils::mask_auth_header;
use super::schema::{AuthDef, HeaderDef, PreprocessStep, SourceDef};
use super::url::resolve_url;

pub(super) fn fetch(id: &str, base_url: &Option<String>, source: &SourceDef) -> Result<String> {
    match source {
        SourceDef::Cli { command, args } => {
            debug!(target: "providers::custom", "[{}] fetching via CLI: {} {:?}", id, command, args);
            fetch_cli(command, args)
        }
        SourceDef::HttpGet { url, auth, headers } => {
            let resolved = resolve_url(base_url, url);
            debug!(target: "providers::custom", "[{}] fetching via HTTP GET: {}", id, resolved);
            let result = fetch_http_get(base_url, &resolved, auth, headers);
            if let Err(ref e) = result {
                warn!(target: "providers::custom", "[{}] HTTP GET failed: {}", id, e);
            }
            result
        }
        SourceDef::HttpPost {
            url,
            auth,
            headers,
            body,
        } => {
            let resolved = resolve_url(base_url, url);
            debug!(target: "providers::custom", "[{}] fetching via HTTP POST: {} (body {} bytes)", id, resolved, body.len());
            let result = fetch_http_post(base_url, &resolved, auth, headers, body);
            if let Err(ref e) = result {
                warn!(target: "providers::custom", "[{}] HTTP POST failed: {}", id, e);
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
    if steps.is_empty() {
        return raw.to_string();
    }
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

fn fetch_cli(command: &str, args: &[String]) -> Result<String> {
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cli::run_lenient_command(command, &args_ref)
}

fn fetch_http_get(
    base_url: &Option<String>,
    resolved_url: &str,
    auth: &Option<AuthDef>,
    headers: &[HeaderDef],
) -> Result<String> {
    let header_strings = resolve_auth_headers(base_url, auth, headers)?;
    debug!(
        target: "providers::custom",
        "request headers ({}): {:?}",
        header_strings.len(),
        header_strings.iter().map(|h| mask_auth_header(h)).collect::<Vec<_>>()
    );
    let header_refs: Vec<&str> = header_strings.iter().map(|s| s.as_str()).collect();
    http_client::get(resolved_url, &header_refs)
}

fn fetch_http_post(
    base_url: &Option<String>,
    resolved_url: &str,
    auth: &Option<AuthDef>,
    headers: &[HeaderDef],
    body: &str,
) -> Result<String> {
    let header_strings = resolve_auth_headers(base_url, auth, headers)?;
    debug!(
        target: "providers::custom",
        "request headers ({}): {:?}",
        header_strings.len(),
        header_strings.iter().map(|h| mask_auth_header(h)).collect::<Vec<_>>()
    );
    let header_refs: Vec<&str> = header_strings.iter().map(|s| s.as_str()).collect();
    http_client::post_json(resolved_url, &header_refs, body)
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
