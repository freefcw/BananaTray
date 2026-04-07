mod cache_source;
mod live_source;
mod parse_strategy;
mod spec;

use super::ProviderError;
use crate::models::{ProviderDescriptor, ProviderMetadata, RefreshData};
use anyhow::Result;
use log::warn;
use rusqlite::{Connection, OpenFlags};
use std::borrow::Cow;
use std::fmt::Write as _;
use std::process::Command;

pub use live_source::matches_process_line;
pub use spec::{CodeiumFamilySpec, ANTIGRAVITY_SPEC, WINDSURF_SPEC};

pub fn descriptor(spec: &CodeiumFamilySpec) -> ProviderDescriptor {
    ProviderDescriptor {
        id: Cow::Borrowed(spec.provider_id),
        metadata: ProviderMetadata {
            kind: spec.kind,
            display_name: spec.display_name.into(),
            brand_name: spec.brand_name.into(),
            icon_asset: spec.icon_asset.into(),
            dashboard_url: spec.dashboard_url.into(),
            account_hint: spec.account_hint.into(),
            source_label: spec.source_label.into(),
        },
    }
}

pub fn debug_report(selector: Option<&str>) -> Result<String> {
    let specs: Vec<CodeiumFamilySpec> = match selector {
        None | Some("all") => vec![ANTIGRAVITY_SPEC, WINDSURF_SPEC],
        Some("antigravity") => vec![ANTIGRAVITY_SPEC],
        Some("windsurf") => vec![WINDSURF_SPEC],
        Some(other) => anyhow::bail!(
            "unknown provider '{}'; expected one of: antigravity, windsurf, all",
            other
        ),
    };

    let mut report = String::new();
    writeln!(&mut report, "# Codeium-family local diagnostics")?;
    writeln!(&mut report)?;
    writeln!(&mut report, "Generated for {} provider(s).", specs.len())?;

    for spec in specs {
        writeln!(&mut report)?;
        writeln!(&mut report, "---")?;
        writeln!(&mut report)?;
        report.push_str(&render_spec_debug(spec)?);
    }

    Ok(report)
}

fn render_spec_debug(spec: CodeiumFamilySpec) -> Result<String> {
    let mut out = String::new();

    writeln!(&mut out, "## {}", spec.display_name)?;
    writeln!(&mut out, "- provider id: `{}`", spec.provider_id)?;
    writeln!(&mut out, "- ide_name: `{}`", spec.ide_name)?;
    writeln!(
        &mut out,
        "- auth key candidates: `{}`",
        spec.auth_status_key_candidates.join("`, `")
    )?;
    writeln!(
        &mut out,
        "- process markers: `{}`",
        spec.process_markers.join("`, `")
    )?;
    writeln!(&mut out)?;

    append_cache_diagnostics(&mut out, spec)?;
    writeln!(&mut out)?;
    append_process_diagnostics(&mut out, spec)?;

    Ok(out)
}

fn append_cache_diagnostics(out: &mut String, spec: CodeiumFamilySpec) -> Result<()> {
    writeln!(out, "### Cache DB")?;

    let Some(home) = dirs::home_dir() else {
        writeln!(out, "- home directory: unavailable")?;
        return Ok(());
    };

    let db_path = home.join(spec.cache_db_relative_path);
    writeln!(out, "- path: `{}`", db_path.display())?;
    writeln!(out, "- exists: {}", db_path.exists())?;

    if !db_path.exists() {
        return Ok(());
    }

    match Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => {
            let interesting_keys = list_interesting_cache_keys(&conn)?;
            if interesting_keys.is_empty() {
                writeln!(out, "- interesting keys: none found")?;
            } else {
                writeln!(out, "- interesting keys:")?;
                for key in interesting_keys {
                    writeln!(out, "  - `{}`", key)?;
                }
            }

            let mut matched_any = false;
            for key in spec.auth_status_key_candidates {
                let exists = cache_key_exists(&conn, key)?;
                matched_any |= exists;
                writeln!(out, "- candidate key `{}` present: {}", key, exists)?;
            }

            if matched_any {
                match cache_source::query_auth_status_json(&conn, &spec) {
                    Ok(value) => writeln!(out, "- selected auth payload bytes: {}", value.len())?,
                    Err(err) => writeln!(out, "- selected auth payload read failed: {}", err)?,
                }
            }
        }
        Err(err) => {
            writeln!(out, "- open/read failed: {}", err)?;
        }
    }

    Ok(())
}

fn append_process_diagnostics(out: &mut String, spec: CodeiumFamilySpec) -> Result<()> {
    writeln!(out, "### Local language server")?;

    let pgrep_output = match Command::new("/usr/bin/pgrep")
        .args(["-lf", "language_server_macos"])
        .output()
    {
        Ok(output) => String::from_utf8_lossy(&output.stdout).into_owned(),
        Err(err) => {
            writeln!(out, "- pgrep failed: {}", err)?;
            return Ok(());
        }
    };

    if pgrep_output.trim().is_empty() {
        writeln!(out, "- matching processes: none")?;
        return Ok(());
    }

    let matching_lines: Vec<&str> = pgrep_output
        .lines()
        .filter(|line| matches_process_line(line, &spec))
        .collect();

    if matching_lines.is_empty() {
        writeln!(out, "- matching processes: none")?;
        return Ok(());
    }

    writeln!(out, "- matching processes: {}", matching_lines.len())?;

    for line in matching_lines {
        writeln!(out, "- raw process line: `{}`", line.trim())?;
        match live_source::parse_process_line(line) {
            Ok(process) => {
                writeln!(out, "  - pid: {}", process.pid)?;
                writeln!(
                    out,
                    "  - csrf token: {}",
                    match &process.csrf_token {
                        Some(token) => mask_secret(token),
                        None => "(not in args)".to_string(),
                    }
                )?;
                writeln!(
                    out,
                    "  - extension_server_port: {:?}",
                    process.extension_port
                )?;

                match live_source::discover_port(&process.pid, &spec) {
                    Ok(port) => {
                        writeln!(out, "  - lsof listen port: {}", port)?;
                        writeln!(out, "  - endpoint hints:")?;
                        for endpoint in
                            live_source::build_endpoint_candidates(port, process.extension_port)
                        {
                            writeln!(out, "    - {}", endpoint.url)?;
                        }
                    }
                    Err(err) => {
                        writeln!(out, "  - lsof listen port: unavailable ({})", err)?;
                        if let Some(ext_port) = process.extension_port {
                            writeln!(out, "  - endpoint hint: http://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/GetUserStatus", ext_port)?;
                        }
                    }
                }
            }
            Err(err) => {
                writeln!(out, "  - parse failed: {}", err)?;
            }
        }
    }

    Ok(())
}

fn list_interesting_cache_keys(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT key FROM ItemTable \
         WHERE key LIKE '%AuthStatus%' \
            OR key LIKE '%windsurf%' \
            OR key LIKE '%antigravity%' \
            OR key LIKE '%codeium%' \
         ORDER BY key",
    )?;

    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut keys = Vec::new();
    for row in rows {
        keys.push(row?);
    }
    Ok(keys)
}

fn cache_key_exists(conn: &Connection, key: &str) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT EXISTS(SELECT 1 FROM ItemTable WHERE key = ?1)")?;
    let exists: i64 = stmt.query_row([key], |row| row.get(0))?;
    Ok(exists != 0)
}

fn mask_secret(secret: &str) -> String {
    if secret.len() <= 8 {
        return "***".to_string();
    }

    let head = &secret[..4];
    let tail = &secret[secret.len() - 4..];
    format!("{}…{}", head, tail)
}

pub fn classify_unavailable(spec: &CodeiumFamilySpec) -> Result<()> {
    if live_source::is_available(spec) || cache_source::is_available(spec) {
        Ok(())
    } else {
        Err(ProviderError::unavailable(spec.unavailable_message).into())
    }
}

pub fn refresh_with_fallback(spec: &CodeiumFamilySpec) -> Result<RefreshData> {
    match live_source::fetch_refresh_data(spec) {
        Ok(data) => Ok(data),
        Err(live_err) => {
            warn!(
                target: "providers",
                "{} live source failed: {}, falling back to local cache",
                spec.log_label,
                live_err
            );

            match cache_source::read_refresh_data(spec) {
                Ok(data) => Ok(data),
                Err(cache_err) => Err(ProviderError::fetch_failed(&format!(
                    "live source failed: {}; cache fallback failed: {}",
                    live_err, cache_err
                ))
                .into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_secret_short() {
        assert_eq!(mask_secret("short"), "***");
    }

    #[test]
    fn test_mask_secret_long() {
        assert_eq!(mask_secret("abcdefgh12345678"), "abcd…5678");
    }
}
