use super::SEAT_API_SOURCE_LABEL;
use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType, RefreshData};
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
    weekly_quota_remaining_percent: Option<i64>,
    #[serde(default)]
    daily_quota_reset_at_unix: Option<String>,
    #[serde(default)]
    weekly_quota_reset_at_unix: Option<String>,
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

    parse_seat_response(seat_response)
}

fn parse_seat_response(seat_response: SeatResponse) -> Result<RefreshData> {
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
        quotas.push(build_seat_quota(
            "daily-quota",
            QuotaLabelSpec::Daily,
            QuotaType::General,
            daily_pct,
            plan_status.daily_quota_reset_at_unix,
        ));
    }

    if let Some(weekly_pct) = plan_status.weekly_quota_remaining_percent {
        quotas.push(build_seat_quota(
            "weekly-quota",
            QuotaLabelSpec::Weekly,
            QuotaType::Weekly,
            weekly_pct,
            plan_status.weekly_quota_reset_at_unix,
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

fn build_seat_quota(
    stable_key: &'static str,
    label: QuotaLabelSpec,
    quota_type: QuotaType,
    remaining_percent: i64,
    reset_at_unix: Option<String>,
) -> QuotaInfo {
    let reset_detail = reset_at_unix
        .and_then(|s| s.parse::<i64>().ok())
        .map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs });

    QuotaInfo::with_key(
        stable_key,
        label,
        100.0 - remaining_percent as f64,
        100.0,
        quota_type,
        reset_detail,
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
        .or_else(read_linux_windsurf_version)
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

fn read_linux_windsurf_version() -> Option<String> {
    let output = Command::new("windsurf").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
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
                    "weeklyQuotaRemainingPercent": 45,
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
        assert_eq!(plan_status.weekly_quota_remaining_percent, Some(45));
        assert_eq!(
            plan_status.daily_quota_reset_at_unix,
            Some("1776585600".to_string())
        );
        assert_eq!(
            plan_status.weekly_quota_reset_at_unix,
            Some("1776585600".to_string())
        );
        assert_eq!(
            plan_status.plan_info.unwrap().plan_name,
            Some("Pro".to_string())
        );
    }

    #[test]
    fn test_parse_seat_response_builds_daily_and_weekly_quotas() {
        let json = r#"{
            "userStatus": {
                "planStatus": {
                    "dailyQuotaRemainingPercent": 100,
                    "weeklyQuotaRemainingPercent": 45,
                    "dailyQuotaResetAtUnix": "1777449600",
                    "weeklyQuotaResetAtUnix": "1777795200",
                    "planInfo": {
                        "planName": "Pro"
                    }
                },
                "email": "test@example.com"
            }
        }"#;

        let response: SeatResponse = serde_json::from_str(json).unwrap();
        let data = parse_seat_response(response).unwrap();

        assert_eq!(data.quotas.len(), 2);
        assert_eq!(data.account_email, Some("test@example.com".to_string()));
        assert_eq!(data.account_tier, Some("Pro".to_string()));
        assert_eq!(data.source_label, Some(SEAT_API_SOURCE_LABEL.to_string()));

        let daily = &data.quotas[0];
        assert_eq!(daily.stable_key, "daily-quota");
        assert_eq!(daily.label_spec, QuotaLabelSpec::Daily);
        assert_eq!(daily.quota_type, QuotaType::General);
        assert!((daily.used - 0.0).abs() < 0.01);
        assert!(matches!(
            daily.detail_spec,
            Some(QuotaDetailSpec::ResetAt {
                epoch_secs: 1777449600
            })
        ));

        let weekly = &data.quotas[1];
        assert_eq!(weekly.stable_key, "weekly-quota");
        assert_eq!(weekly.label_spec, QuotaLabelSpec::Weekly);
        assert_eq!(weekly.quota_type, QuotaType::Weekly);
        assert!((weekly.used - 55.0).abs() < 0.01);
        assert!(matches!(
            weekly.detail_spec,
            Some(QuotaDetailSpec::ResetAt {
                epoch_secs: 1777795200
            })
        ));
    }

    #[test]
    fn test_parse_seat_response_uses_remaining_percent_for_current_windsurf_values() {
        let json = r#"{
            "userStatus": {
                "planStatus": {
                    "dailyQuotaRemainingPercent": 65,
                    "weeklyQuotaRemainingPercent": 83,
                    "dailyQuotaResetAtUnix": "1777881600",
                    "weeklyQuotaResetAtUnix": "1778400000",
                    "planInfo": {
                        "planName": "Pro"
                    }
                },
                "email": "test@example.com"
            }
        }"#;

        let response: SeatResponse = serde_json::from_str(json).unwrap();
        let data = parse_seat_response(response).unwrap();

        let daily = data
            .quotas
            .iter()
            .find(|quota| quota.stable_key == "daily-quota")
            .unwrap();
        assert!((daily.used - 35.0).abs() < 0.01);

        let weekly = data
            .quotas
            .iter()
            .find(|quota| quota.stable_key == "weekly-quota")
            .unwrap();
        assert!((weekly.used - 17.0).abs() < 0.01);
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
