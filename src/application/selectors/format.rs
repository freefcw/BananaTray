//! 格式化与展示文案函数
//!
//! 将 Provider 状态/Quota/Failure → 展示文本 的转换逻辑集中于此。
//! 上次更新时间、配额标签/详情、错误文案等。
//! 从原 `app/provider_logic.rs` 合并而来。

use super::QuotaDisplayViewState;
use crate::models::{
    ConnectionStatus, FailureAdvice, FailureReason, ProviderCapability, ProviderFailure,
    ProviderKind, ProviderStatus, QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType,
    UpdateStatus,
};
use rust_i18n::t;

/// 格式化上次刷新的相对时间
///
/// 从 `ProviderStatus` 的实例方法提取到 selector 层，
/// 消除数据模型对 i18n 的依赖（DIP 原则）。
pub fn format_last_updated(provider: &ProviderStatus) -> String {
    if let Some(instant) = provider.last_refreshed_instant {
        let secs = instant.elapsed().as_secs();
        if secs < 60 {
            t!("provider.updated_just_now").to_string()
        } else if secs < 3600 {
            t!("provider.updated_min_ago", n = secs / 60).to_string()
        } else {
            t!("provider.updated_hr_ago", n = secs / 3600).to_string()
        }
    } else if let Some(status) = provider.update_status {
        match status {
            UpdateStatus::Failed => t!("quota.update_failed").to_string(),
        }
    } else {
        match provider.connection {
            ConnectionStatus::Connected => t!("provider.waiting_for_data").to_string(),
            ConnectionStatus::Refreshing => t!("provider.status.refreshing").to_string(),
            ConnectionStatus::Error => t!("provider.needs_attention").to_string(),
            ConnectionStatus::Disconnected => t!("provider.not_connected").to_string(),
        }
    }
}

/// 格式化 Provider 最近一次失败消息。
pub fn format_failure_message(failure: &ProviderFailure) -> String {
    match &failure.reason {
        FailureReason::CliNotFound { cli_name } => {
            t!("error.cli_not_found", cli = cli_name).to_string()
        }
        FailureReason::AuthRequired => failure
            .advice
            .as_ref()
            .map(format_failure_advice)
            .unwrap_or_else(|| t!("error.auth_required_default").to_string()),
        FailureReason::SessionExpired => failure
            .advice
            .as_ref()
            .map(format_failure_advice)
            .unwrap_or_else(|| t!("error.session_expired_default").to_string()),
        FailureReason::FolderTrustRequired => t!("error.folder_trust").to_string(),
        FailureReason::UpdateRequired { version } => match version {
            Some(v) => t!("error.update_required_ver", version = v).to_string(),
            None => t!("error.update_required").to_string(),
        },
        FailureReason::ConfigMissing { key } => t!("error.config_missing", key = key).to_string(),
        FailureReason::Unavailable | FailureReason::ParseFailed | FailureReason::FetchFailed => {
            failure
                .advice
                .as_ref()
                .map(format_failure_advice)
                .or_else(|| failure.raw_detail.clone())
                .unwrap_or_else(|| t!("provider.unknown_error").to_string())
        }
        FailureReason::Timeout => t!("error.timeout").to_string(),
        FailureReason::NoData => t!("error.no_data").to_string(),
        FailureReason::NetworkFailed => match failure.raw_detail.as_deref() {
            Some(reason) => t!("error.network_failed", reason = reason).to_string(),
            None => t!("error.timeout").to_string(),
        },
    }
}

/// 为非可监控 provider 生成统一说明文案。
pub fn format_non_monitoring_message(provider: &ProviderStatus) -> String {
    if let Some(failure) = &provider.last_failure {
        return format_failure_message(failure);
    }

    match (provider.provider_capability, provider.kind()) {
        (ProviderCapability::Informational, ProviderKind::VertexAi) => {
            t!("hint.vertex_shared_quota").to_string()
        }
        (ProviderCapability::Placeholder, ProviderKind::Kilo) => {
            t!("hint.kilo_no_api", name = provider.display_name()).to_string()
        }
        (ProviderCapability::Informational | ProviderCapability::Placeholder, _) => {
            t!("hint.no_monitoring", name = provider.display_name()).to_string()
        }
        (ProviderCapability::Monitorable, _) => t!("provider.unknown_error").to_string(),
    }
}

fn format_failure_advice(advice: &FailureAdvice) -> String {
    match advice {
        FailureAdvice::LoginCli { cli } => t!("hint.login_cli", cli = cli).to_string(),
        FailureAdvice::ReloginCli { cli } => t!("hint.relogin_cli", cli = cli).to_string(),
        FailureAdvice::RefreshCli { cli } => t!("hint.refresh_cli", cli = cli).to_string(),
        FailureAdvice::LoginApp { app } => t!("hint.login_app", app = app).to_string(),
        FailureAdvice::CliExitFailed { code } => {
            t!("hint.cli_exit_failed", code = code).to_string()
        }
        FailureAdvice::ApiHttpError { status } => {
            t!("hint.api_http_error", status = status).to_string()
        }
        FailureAdvice::ApiError { message } => t!("hint.api_error", msg = message).to_string(),
        FailureAdvice::NoOauthCreds { cli } => t!("hint.no_oauth_creds", cli = cli).to_string(),
        FailureAdvice::BothUnavailable { name } => {
            t!("hint.both_unavailable", name = name).to_string()
        }
        FailureAdvice::TrustFolder { cli } => t!("hint.trust_folder", cli = cli).to_string(),
        FailureAdvice::CannotParseQuota => t!("hint.cannot_parse_quota").to_string(),
        FailureAdvice::TokenStillInvalid => t!("hint.token_still_invalid").to_string(),
    }
}

/// 格式化配额标题。
pub fn format_quota_label(quota: &QuotaInfo) -> String {
    match &quota.label_spec {
        QuotaLabelSpec::Raw(label) => label.clone(),
        QuotaLabelSpec::Daily => t!("quota.label.daily").to_string(),
        QuotaLabelSpec::Session => t!("quota.label.session").to_string(),
        QuotaLabelSpec::Weekly => t!("quota.label.weekly").to_string(),
        QuotaLabelSpec::WeeklyModel { model } => {
            t!("quota.label.weekly_model", model = model).to_string()
        }
        QuotaLabelSpec::WeeklyTier { tier } => {
            format!("{} ({})", t!("quota.label.weekly"), tier)
        }
        QuotaLabelSpec::MonthlyCredits => t!("quota.label.monthly_credits").to_string(),
        QuotaLabelSpec::Credits => t!("quota.label.credits").to_string(),
        QuotaLabelSpec::BonusCredits => t!("quota.label.bonus_credits").to_string(),
        QuotaLabelSpec::ExtraUsage => t!("quota.label.extra_usage").to_string(),
        QuotaLabelSpec::PremiumRequests { plan } => {
            t!("quota.label.premium_requests", plan = plan).to_string()
        }
        QuotaLabelSpec::ChatCompletions { plan } => {
            t!("quota.label.chat_completions", plan = plan).to_string()
        }
        QuotaLabelSpec::MonthlyTier { tier } => {
            t!("quota.label.monthly_tier", tier = tier).to_string()
        }
        QuotaLabelSpec::OnDemand => t!("quota.label.on_demand").to_string(),
        QuotaLabelSpec::Team => t!("quota.label.team").to_string(),
    }
}

/// 格式化配额详情（卡片第四行）。
pub fn format_quota_detail(quota: &QuotaInfo) -> String {
    match &quota.detail_spec {
        Some(QuotaDetailSpec::Raw(text)) => text.clone(),
        Some(QuotaDetailSpec::Unlimited) => t!("quota.label.unlimited").to_string(),
        Some(QuotaDetailSpec::RequestCount { used, total }) => {
            t!("quota.label.request_detail", used = used, total = total).to_string()
        }
        Some(QuotaDetailSpec::CreditRemaining { remaining, total }) => t!(
            "quota.label.credit_remaining",
            remaining = format!("{remaining:.2}"),
            total = format!("{total:.2}")
        )
        .to_string(),
        Some(QuotaDetailSpec::ResetAt { epoch_secs }) => {
            crate::utils::time_utils::format_reset_from_epoch(*epoch_secs)
        }
        Some(QuotaDetailSpec::ResetDate { date }) => {
            t!("quota.label.resets_on", date = date).to_string()
        }
        Some(QuotaDetailSpec::ExpiresInDays { days }) => {
            t!("quota.label.expires_in_days", days = days).to_string()
        }
        None => String::new(),
    }
}

/// 将 domain quota 转为 UI 可直接消费的展示 ViewState。
pub fn quota_display_view_state(quota: &QuotaInfo) -> QuotaDisplayViewState {
    QuotaDisplayViewState {
        quota: quota.clone(),
        label: format_quota_label(quota),
        detail: format_quota_detail(quota),
    }
}

/// 使用详情文本（用于 UI 详细展示）
///
/// 从 `QuotaInfo` 的实例方法提取到 selector 层，
/// 消除数据模型对 i18n 的依赖（DIP 原则）。
///
/// - 余额模式: "Used: $X.XX" 或 空
/// - Credit 类型: "$X.XX / $Y.YY"
/// - 其他类型: "X used / Y total" 或 "X% used"
pub fn quota_usage_detail_text(quota: &QuotaInfo) -> String {
    if quota.remaining_balance.is_some() {
        // 余额模式：显示已用额度
        if quota.used > 0.0 {
            if matches!(quota.quota_type, QuotaType::Credit) {
                t!("quota.used_credit", amount = format!("{:.2}", quota.used)).to_string()
            } else {
                t!("quota.used_amount", amount = format!("{:.2}", quota.used)).to_string()
            }
        } else {
            String::new()
        }
    } else {
        match quota.quota_type {
            QuotaType::Credit => t!(
                "quota.credit_detail",
                used = format!("{:.2}", quota.used),
                limit = format!("{:.2}", quota.limit)
            )
            .to_string(),
            _ => {
                if quota.is_percentage_mode() {
                    t!("quota.pct_used", pct = format!("{:.0}", quota.used)).to_string()
                } else {
                    t!(
                        "quota.count_detail",
                        used = format!("{:.0}", quota.used),
                        total = format!("{:.0}", quota.limit)
                    )
                    .to_string()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::test_helpers::{
        make_test_provider as make_provider, setup_test_locale as setup_locale,
    };
    use crate::models::{
        ConnectionStatus, FailureAdvice, FailureReason, ProviderFailure, ProviderKind,
        QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType, UpdateStatus,
    };

    // ── format_last_updated ──────────────────────────────────

    #[test]
    fn format_last_updated_no_instant_connected() {
        let _locale_guard = setup_locale();
        let p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        assert_eq!(format_last_updated(&p), "Waiting for data");
    }

    #[test]
    fn format_last_updated_no_instant_refreshing() {
        let _locale_guard = setup_locale();
        let p = make_provider(ProviderKind::Claude, ConnectionStatus::Refreshing);
        assert_eq!(format_last_updated(&p), "Refreshing…");
    }

    #[test]
    fn format_last_updated_no_instant_error() {
        let _locale_guard = setup_locale();
        let p = make_provider(ProviderKind::Claude, ConnectionStatus::Error);
        assert_eq!(format_last_updated(&p), "Needs attention");
    }

    #[test]
    fn format_last_updated_no_instant_disconnected() {
        let _locale_guard = setup_locale();
        let p = make_provider(ProviderKind::Claude, ConnectionStatus::Disconnected);
        assert_eq!(format_last_updated(&p), "Not connected");
    }

    #[test]
    fn format_last_updated_with_failed_status() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.update_status = Some(UpdateStatus::Failed);
        assert_eq!(format_last_updated(&p), "Update failed");
    }

    #[test]
    fn format_last_updated_just_now() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.last_refreshed_instant = Some(std::time::Instant::now());
        assert_eq!(format_last_updated(&p), "Updated just now");
    }

    #[test]
    fn format_last_updated_exactly_60s() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.last_refreshed_instant =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(60));
        assert_eq!(format_last_updated(&p), "Updated 1 min ago");
    }

    #[test]
    fn format_last_updated_exactly_3600s() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.last_refreshed_instant =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(3600));
        assert_eq!(format_last_updated(&p), "Updated 1 hr ago");
    }

    #[test]
    fn format_last_updated_instant_takes_priority_over_status() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Error);
        p.last_refreshed_instant = Some(std::time::Instant::now());
        p.update_status = Some(UpdateStatus::Failed);
        // instant 存在时，优先显示时间，不显示 "Update failed"
        assert_eq!(format_last_updated(&p), "Updated just now");
    }

    // ── format_failure_message ──────────────────────────────

    #[test]
    fn failure_message_uses_advice() {
        let _locale_guard = setup_locale();
        let failure = ProviderFailure {
            reason: FailureReason::AuthRequired,
            advice: Some(FailureAdvice::LoginCli {
                cli: "claude".to_string(),
            }),
            raw_detail: None,
        };
        assert_eq!(
            format_failure_message(&failure),
            "Please run `claude` to login"
        );
    }

    #[test]
    fn failure_message_falls_back_to_raw_detail() {
        let _locale_guard = setup_locale();
        let failure = ProviderFailure {
            reason: FailureReason::FetchFailed,
            advice: None,
            raw_detail: Some("upstream 502".to_string()),
        };
        assert_eq!(format_failure_message(&failure), "upstream 502");
    }

    // ── quota label/detail ─────────────────────────────────

    #[test]
    fn format_quota_label_weekly_tier() {
        let _locale_guard = setup_locale();
        let quota = QuotaInfo::with_details(
            QuotaLabelSpec::WeeklyTier {
                tier: "Moderato".to_string(),
            },
            25.0,
            100.0,
            QuotaType::Weekly,
            None,
        );
        assert_eq!(format_quota_label(&quota), "Weekly (Moderato)");
    }

    #[test]
    fn format_quota_label_daily() {
        let _locale_guard = setup_locale();
        let quota =
            QuotaInfo::with_details(QuotaLabelSpec::Daily, 25.0, 100.0, QuotaType::General, None);
        assert_eq!(format_quota_label(&quota), "Daily");
    }

    #[test]
    fn format_quota_label_monthly_credits() {
        let _locale_guard = setup_locale();
        let quota = QuotaInfo::with_details(
            QuotaLabelSpec::MonthlyCredits,
            5.0,
            20.0,
            QuotaType::Credit,
            None,
        );
        assert_eq!(format_quota_label(&quota), "Monthly Credits");
    }

    #[test]
    fn format_quota_detail_reset_at() {
        let _locale_guard = setup_locale();
        let future = crate::utils::time_utils::now_epoch_secs() + 3600;
        let quota = QuotaInfo::with_details(
            QuotaLabelSpec::Session,
            10.0,
            100.0,
            QuotaType::Session,
            Some(QuotaDetailSpec::ResetAt { epoch_secs: future }),
        );
        assert!(format_quota_detail(&quota).contains("Resets in 1h"));
    }

    // ── quota_usage_detail_text ──────────────────────────────

    #[test]
    fn usage_detail_text_percentage() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::new("test", 30.0, 100.0);
        assert_eq!(quota_usage_detail_text(&q), "30% used");

        let q_full = QuotaInfo::new("full", 100.0, 100.0);
        assert_eq!(quota_usage_detail_text(&q_full), "100% used");

        // 非 percentage mode（limit != 100）
        let q_real = QuotaInfo::new("real", 30.0, 50.0);
        assert_eq!(quota_usage_detail_text(&q_real), "30 used / 50 total");
    }

    #[test]
    fn usage_detail_text_credit() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::with_details("Credit", 5.0, 20.0, QuotaType::Credit, None);
        assert_eq!(quota_usage_detail_text(&q), "$5.00 / $20.00");

        let q_zero = QuotaInfo::with_details("Credit", 0.0, 100.0, QuotaType::Credit, None);
        assert_eq!(quota_usage_detail_text(&q_zero), "$0.00 / $100.00");
    }

    #[test]
    fn usage_detail_text_balance_with_used() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::balance_only("B", 10.0, Some(3.50), QuotaType::Credit, None);
        assert_eq!(quota_usage_detail_text(&q), "Used: $3.50");
    }

    #[test]
    fn usage_detail_text_balance_zero_used() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::balance_only("B", 10.0, None, QuotaType::Credit, None);
        assert_eq!(quota_usage_detail_text(&q), "");
    }
}
