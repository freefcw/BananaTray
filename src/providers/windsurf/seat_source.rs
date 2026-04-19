use super::SEAT_API_SOURCE_LABEL;
use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaType, RefreshData};
use crate::providers::codeium_family::{self, CodeiumFamilySpec};
use crate::providers::ProviderError;
use anyhow::{Context, Result};
use log::{debug, info};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

const SEAT_API_BASE: &str = "https://server.self-serve.windsurf.com";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeatUserStatus {
    #[serde(default)]
    plan_status: Option<SeatPlanStatus>,
    #[serde(default)]
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeatPlanStatus {
    #[serde(default)]
    daily_quota_remaining_percent: Option<i64>,
    #[serde(default)]
    daily_quota_reset_at_unix: Option<String>,
    #[serde(default)]
    plan_info: Option<SeatPlanInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeatPlanInfo {
    #[serde(default)]
    plan_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeatResponse {
    #[serde(default)]
    user_status: Option<SeatUserStatus>,
}

pub fn fetch_refresh_data(spec: &CodeiumFamilySpec) -> Result<RefreshData> {
    if spec.kind != crate::models::ProviderKind::Windsurf {
        return Err(ProviderError::unavailable("seat API only available for Windsurf").into());
    }

    let api_key = get_api_key(spec)?;
    let url = format!(
        "{}/exa.seat_management_pb.SeatManagementService/GetUserStatus",
        SEAT_API_BASE
    );
    let app_version = detect_windsurf_app_version(spec);

    info!(
        target: "providers",
        "{}: fetching user status from seat management API",
        spec.log_label
    );
    debug!(
        target: "providers",
        "{} seat API app version hint: {:?}",
        spec.log_label,
        app_version
    );

    let request_body = build_request_body(spec, &api_key, app_version.as_deref())?;
    let response_text = crate::providers::common::http_client::post_json(&url, &[], &request_body)
        .with_context(|| format!("POST {} failed", url))?;

    let seat_response: SeatResponse = serde_json::from_str(&response_text)
        .with_context(|| "Failed to parse seat API response")?;

    let user_status = seat_response
        .user_status
        .ok_or_else(|| anyhow::anyhow!("no user_status in seat API response"))?;

    let plan_status = user_status
        .plan_status
        .ok_or_else(|| anyhow::anyhow!("no plan_status in seat API response"))?;

    let email = user_status.email;
    let plan_name = plan_status.plan_info.and_then(|p| p.plan_name);

    let mut quotas = Vec::new();

    if let Some(daily_pct) = plan_status.daily_quota_remaining_percent {
        let used_percent = 100.0 - daily_pct as f64;
        let reset_detail = plan_status
            .daily_quota_reset_at_unix
            .and_then(|s| s.parse::<i64>().ok())
            .map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs });

        quotas.push(QuotaInfo::with_details(
            "Daily Quota",
            used_percent,
            100.0,
            QuotaType::ModelSpecific("Daily Quota".to_string()),
            reset_detail,
        ));
    }

    if quotas.is_empty() {
        anyhow::bail!("no quota data in seat API response");
    }

    Ok(
        RefreshData::with_account(quotas, email, plan_name)
            .with_source_label(SEAT_API_SOURCE_LABEL),
    )
}

fn get_api_key(spec: &CodeiumFamilySpec) -> Result<String> {
    use rusqlite::{Connection, OpenFlags};

    let db_path = codeium_family::cache_db_path(spec)?;

    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("cannot open {} cache DB", spec.log_label))?;
    let auth_status_json = codeium_family::query_auth_status_json(&conn, spec)?;
    let auth_status: serde_json::Value = serde_json::from_str(&auth_status_json)
        .with_context(|| "invalid auth status JSON while reading apiKey")?;

    auth_status
        .get("apiKey")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            ProviderError::parse_failed(&format!(
                "cannot find apiKey in auth status: {}",
                spec.auth_status_key_candidates.join(", ")
            ))
            .into()
        })
}

fn build_request_body(
    spec: &CodeiumFamilySpec,
    api_key: &str,
    app_version: Option<&str>,
) -> Result<String> {
    let mut metadata = Map::new();
    metadata.insert(
        "ideName".to_string(),
        Value::String(spec.ide_name.to_string()),
    );
    metadata.insert("apiKey".to_string(), Value::String(api_key.to_string()));

    if let Some(version) = app_version.filter(|v| !v.trim().is_empty()) {
        metadata.insert(
            "extensionVersion".to_string(),
            Value::String(version.to_string()),
        );
        metadata.insert("ideVersion".to_string(), Value::String(version.to_string()));
    }

    serde_json::to_string(&serde_json::json!({ "metadata": metadata }))
        .with_context(|| "failed to serialize seat API request body")
}

fn detect_windsurf_app_version(spec: &CodeiumFamilySpec) -> Option<String> {
    if spec.kind != crate::models::ProviderKind::Windsurf {
        return None;
    }

    codeium_family::detect_process(spec)
        .ok()
        .and_then(|process| process.binary_path)
        .and_then(|binary_path| info_plist_from_binary_path(&binary_path))
        .and_then(|path| read_cf_bundle_short_version(&path))
        .or_else(|| {
            info_plist_candidates()
                .into_iter()
                .find_map(|path| read_cf_bundle_short_version(&path))
        })
}

fn info_plist_from_binary_path(binary_path: &str) -> Option<PathBuf> {
    Path::new(binary_path)
        .ancestors()
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("app"))
        .map(|app_bundle| app_bundle.join("Contents/Info.plist"))
}

fn info_plist_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from(
        "/Applications/Windsurf.app/Contents/Info.plist",
    )];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("Applications/Windsurf.app/Contents/Info.plist"));
    }
    candidates
}

fn read_cf_bundle_short_version(plist_path: &Path) -> Option<String> {
    let output = Command::new("/usr/bin/defaults")
        .arg("read")
        .arg(plist_path)
        .arg("CFBundleShortVersionString")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let version = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seat_response_parsing() {
        let json = r#"{
            "userStatus": {
                "planStatus": {
                    "dailyQuotaRemainingPercent": 39,
                    "dailyQuotaResetAtUnix": "1776585600",
                    "weeklyQuotaResetAtUnix": "1776585600",
                    "planInfo": {
                        "planName": "Pro"
                    }
                },
                "email": "test@example.com"
            }
        }"#;

        let response: SeatResponse = serde_json::from_str(json).unwrap();
        assert!(response.user_status.is_some());

        let user_status = response.user_status.unwrap();
        assert_eq!(user_status.email, Some("test@example.com".to_string()));

        let plan_status = user_status.plan_status.unwrap();
        assert_eq!(plan_status.daily_quota_remaining_percent, Some(39));
        assert_eq!(
            plan_status.daily_quota_reset_at_unix,
            Some("1776585600".to_string())
        );
        assert_eq!(
            plan_status.plan_info.unwrap().plan_name,
            Some("Pro".to_string())
        );
    }

    #[test]
    fn test_seat_response_missing_fields() {
        let json = r#"{
            "userStatus": {
                "planStatus": {
                    "dailyQuotaRemainingPercent": 50
                }
            }
        }"#;

        let response: SeatResponse = serde_json::from_str(json).unwrap();
        assert!(response.user_status.is_some());

        let plan_status = response.user_status.unwrap().plan_status.unwrap();
        assert_eq!(plan_status.daily_quota_remaining_percent, Some(50));
        assert!(plan_status.daily_quota_reset_at_unix.is_none());
        assert!(plan_status.plan_info.is_none());
    }

    #[test]
    fn test_build_request_body_includes_version_when_available() {
        let body =
            build_request_body(&codeium_family::WINDSURF_SPEC, "api-key", Some("2.1.0")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(
            json.pointer("/metadata/ideName").and_then(|v| v.as_str()),
            Some("windsurf")
        );
        assert_eq!(
            json.pointer("/metadata/apiKey").and_then(|v| v.as_str()),
            Some("api-key")
        );
        assert_eq!(
            json.pointer("/metadata/extensionVersion")
                .and_then(|v| v.as_str()),
            Some("2.1.0")
        );
        assert_eq!(
            json.pointer("/metadata/ideVersion")
                .and_then(|v| v.as_str()),
            Some("2.1.0")
        );
    }

    #[test]
    fn test_build_request_body_omits_version_when_unavailable() {
        let body = build_request_body(&codeium_family::WINDSURF_SPEC, "api-key", None).unwrap();
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(
            json.pointer("/metadata/ideName").and_then(|v| v.as_str()),
            Some("windsurf")
        );
        assert!(json.pointer("/metadata/extensionVersion").is_none());
        assert!(json.pointer("/metadata/ideVersion").is_none());
    }

    #[test]
    fn test_info_plist_from_binary_path() {
        let path = info_plist_from_binary_path(
            "/Applications/Windsurf.app/Contents/Resources/app/extensions/windsurf/bin/language_server_macos_arm",
        );

        assert_eq!(
            path,
            Some(PathBuf::from(
                "/Applications/Windsurf.app/Contents/Info.plist"
            ))
        );
    }
}
