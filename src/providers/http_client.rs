//! Shared HTTP client utilities for providers.
//!
//! Wraps `curl` invocations into ergonomic helpers so each provider
//! doesn't have to duplicate the Command::new("curl") boilerplate.
use anyhow::{bail, Context, Result};
use std::process::Command;

/// Perform an HTTP GET via curl and return the response body as a String.
///
/// `headers` is a list of header strings like `"Authorization: Bearer xxx"`.
#[allow(dead_code)]
pub fn curl_get(url: &str, headers: &[&str]) -> Result<String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-s"]);

    for h in headers {
        cmd.args(["-H", h]);
    }

    cmd.arg(url);

    let output = cmd
        .output()
        .with_context(|| format!("Failed to execute curl GET {}", url))?;

    if !output.status.success() {
        bail!("curl GET {} failed with status {:?}", url, output.status);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Perform an HTTP GET via curl and return the full raw output (headers + body).
///
/// Useful when status codes need to be inspected (e.g. Codex checks for 401/403).
pub fn curl_get_with_headers(url: &str, headers: &[&str]) -> Result<String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-s", "-i"]); // -i includes response headers

    for h in headers {
        cmd.args(["-H", h]);
    }

    cmd.arg(url);

    let output = cmd
        .output()
        .with_context(|| format!("Failed to execute curl GET {}", url))?;

    if !output.status.success() {
        bail!("curl GET {} failed with status {:?}", url, output.status);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Perform an HTTP GET via curl and return `(body, http_status_code)`.
///
/// Uses curl's `-w` flag to append the status code.
pub fn curl_get_with_status(url: &str, headers: &[&str]) -> Result<(String, String)> {
    let mut cmd = Command::new("curl");
    cmd.args(["-s", "-w", "\n%{http_code}"]);

    for h in headers {
        cmd.args(["-H", h]);
    }

    cmd.arg(url);

    let output = cmd
        .output()
        .with_context(|| format!("Failed to execute curl GET {}", url))?;

    if !output.status.success() {
        bail!("curl GET {} failed with status {:?}", url, output.status);
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    let trimmed = output_str.trim();

    if let Some(pos) = trimmed.rfind('\n') {
        let code = trimmed[pos + 1..].trim().to_string();
        let body = trimmed[..pos].to_string();
        Ok((body, code))
    } else {
        Ok((trimmed.to_string(), String::new()))
    }
}

/// Perform an HTTP POST via curl with a JSON body (Content-Type: application/json).
pub fn curl_post_json(url: &str, headers: &[&str], body: &str) -> Result<String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-s", "-X", "POST", "-H", "Content-Type: application/json"]);

    for h in headers {
        cmd.args(["-H", h]);
    }

    cmd.args(["-d", body, url]);

    let output = cmd
        .output()
        .with_context(|| format!("Failed to execute curl POST {}", url))?;

    if !output.status.success() {
        bail!("curl POST {} failed with status {:?}", url, output.status);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Perform an HTTP POST via curl with a form-urlencoded body.
pub fn curl_post_form(url: &str, headers: &[&str], body: &str) -> Result<String> {
    let mut cmd = Command::new("curl");
    cmd.args([
        "-s",
        "-X",
        "POST",
        "-H",
        "Content-Type: application/x-www-form-urlencoded",
    ]);

    for h in headers {
        cmd.args(["-H", h]);
    }

    cmd.args(["-d", body, url]);

    let output = cmd
        .output()
        .with_context(|| format!("Failed to execute curl POST {}", url))?;

    if !output.status.success() {
        bail!("curl POST {} failed with status {:?}", url, output.status);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
