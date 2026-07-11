//! 格式化与展示文案函数
//!
//! 将 Provider 状态/Quota/Failure → 展示文本 的转换逻辑集中于此。
//! 上次更新时间、配额标签/详情、错误文案等。
//! 从原 `app/provider_logic.rs` 合并而来。

use super::QuotaDisplayViewState;
use crate::models::{
    ConnectionStatus, FailureAdvice, FailureReason, ProviderCapability, ProviderFailure,
    ProviderKind, ProviderStatus, QuotaDetailSpec, QuotaDisplayMode, QuotaInfo, QuotaLabelSpec,
    QuotaType, StatusLevel, UpdateStatus,
};
use rust_i18n::t;

/// 将内部 `source_label` 转为设置页副标题用的用户向文案。
pub fn display_source_label(raw: &str) -> String {
    match raw {
        "github api" => t!("provider.source_label.github_api").to_string(),
        "seat api" => t!("provider.source_label.seat_api").to_string(),
        "seat api + local cache" => t!("provider.source_label.seat_api_local_cache").to_string(),
        "local api" => t!("provider.source_label.local_api").to_string(),
        "local cache" => t!("provider.source_label.local_cache").to_string(),
        "local/cloud fallback" => t!("provider.source_label.local_cloud_fallback").to_string(),
        "cursor api" => t!("provider.source_label.cursor_api").to_string(),
        "gemini api" => t!("provider.source_label.gemini_api").to_string(),
        "openai api" => t!("provider.source_label.openai_api").to_string(),
        "claude" => t!("provider.source_label.claude").to_string(),
        "vertex ai api" => t!("provider.source_label.vertex_ai_api").to_string(),
        "kiro cli" => t!("provider.source_label.kiro_cli").to_string(),
        "amp cli" => t!("provider.source_label.amp_cli").to_string(),
        "kilo api" => t!("provider.source_label.kilo_api").to_string(),
        "kimi api" => t!("provider.source_label.kimi_api").to_string(),
        "minimax api" => t!("provider.source_label.minimax_api").to_string(),
        "opencode api" => t!("provider.source_label.opencode_api").to_string(),
        "newapi api" => t!("provider.source_label.newapi_api").to_string(),
        "merged" => t!("provider.source_label.merged").to_string(),
        "" => t!("provider.source_label.auto").to_string(),
        _ if raw.ends_with(" cli") => t!("provider.source_label.custom_cli").to_string(),
        _ => t!("provider.source_label.custom").to_string(),
    }
}

/// Provider 数据来源的用户向展示文案（UI 统一入口）。
pub fn provider_source_label(provider: &ProviderStatus) -> String {
    display_source_label(provider.source_label())
}

/// 相对刷新时间（仅在有 `last_refreshed_instant` 时使用）。
pub fn format_relative_refresh_age(secs: u64) -> String {
    if secs < 60 {
        t!("provider.time.just_now").to_string()
    } else if secs < 3600 {
        t!("provider.time.min_ago", n = secs / 60).to_string()
    } else {
        t!("provider.time.hr_ago", n = secs / 3600).to_string()
    }
}

/// 无刷新时间戳时的连接 / 更新状态文案（Debug、托盘等独立展示场景）。
pub fn format_refresh_status(provider: &ProviderStatus) -> String {
    if let Some(status) = provider.update_status {
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

/// 设置页 info table「更新时间」列：有 instant 返回相对时间，否则「尚未获取」。
pub fn format_provider_updated_at(provider: &ProviderStatus) -> String {
    provider
        .last_refreshed_instant
        .map(|instant| format_relative_refresh_age(instant.elapsed().as_secs()))
        .unwrap_or_else(|| t!("provider.not_fetched").to_string())
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
pub(crate) fn format_quota_label(quota: &QuotaInfo) -> String {
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

/// 将用量严重程度转换为稳定、可本地化的短标签。
///
/// `StatusLevel` 描述的是配额余量，不是连接状态；因此 Red 必须显示为余量偏低，
/// 不能复用 Offline/Out 等连接或耗尽语义。
#[allow(dead_code)] // 仅 app feature 下的托盘和设置 UI 调用
pub fn format_quota_status_label(level: StatusLevel) -> String {
    match level {
        StatusLevel::Green => t!("quota.status.ok").to_string(),
        StatusLevel::Yellow => t!("quota.status.warn").to_string(),
        StatusLevel::Red => t!("quota.status.low").to_string(),
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
/// - Points 类型: "X.XX / Y.YY"
/// - 其他类型: "X used / Y total" 或 "X% used"
#[allow(dead_code)] // 仅 app feature 下的 ui/widgets/display/quota_bar.rs 调用
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
            QuotaType::Points => t!(
                "quota.count_detail",
                used = format!("{:.2}", quota.used),
                total = format!("{:.2}", quota.limit)
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

/// 设置页 quota 卡片的主显示文本。
#[allow(dead_code)] // 仅 app feature 下的 ui/widgets/display/quota_bar.rs 调用
pub fn format_quota_card_display_text(quota: &QuotaInfo, display_mode: QuotaDisplayMode) -> String {
    if quota.is_balance_only() {
        let balance = quota.remaining_balance.unwrap_or(0.0);
        return if matches!(quota.quota_type, QuotaType::Credit) {
            format!("${:.2}", balance)
        } else {
            format!("{:.2}", balance)
        };
    }

    let remaining_pct = quota.percent_remaining();
    match (&quota.quota_type, display_mode) {
        (QuotaType::Credit, QuotaDisplayMode::Remaining) => quota.format_remaining_signed("$"),
        (QuotaType::Credit, QuotaDisplayMode::Used) => format!("${:.2}", quota.used),
        (QuotaType::Points, QuotaDisplayMode::Remaining) => quota.format_remaining_signed(""),
        (QuotaType::Points, QuotaDisplayMode::Used) => format!("{:.2}", quota.used),
        (_, QuotaDisplayMode::Remaining) => format!("{:.0}", remaining_pct.max(0.0)),
        (_, QuotaDisplayMode::Used) => format!("{:.0}", quota.percentage().clamp(0.0, 100.0)),
    }
}

/// 设置页 quota 卡片的模式标签。
#[allow(dead_code)] // 仅 app feature 下的 ui/widgets/display/quota_bar.rs 调用
pub fn format_quota_card_mode_label(is_balance: bool, display_mode: QuotaDisplayMode) -> String {
    if is_balance {
        return t!("quota.mode.balance").to_string();
    }

    match display_mode {
        QuotaDisplayMode::Remaining => t!("quota.mode.remaining").to_string(),
        QuotaDisplayMode::Used => t!("quota.mode.used").to_string(),
    }
}

/// 设置页 quota 卡片是否需要额外的百分号单位。
#[allow(dead_code)] // 仅 app feature 下的 ui/widgets/display/quota_bar.rs 调用
pub fn format_quota_card_has_unit(quota: &QuotaInfo) -> bool {
    !quota.is_balance_only() && !matches!(quota.quota_type, QuotaType::Credit | QuotaType::Points)
}

/// 设置页 quota 卡片的第四行详情文本。
#[allow(dead_code)] // 仅 app feature 下的 ui/widgets/display/quota_bar.rs 调用
pub fn format_quota_card_detail_text(quota_view: &QuotaDisplayViewState) -> String {
    let quota = &quota_view.quota;
    if !quota.is_balance_only() {
        return quota_view.detail.clone();
    }

    let used_text = quota_usage_detail_text(quota);
    if used_text.is_empty() {
        quota_view.detail.clone()
    } else if quota_view.detail.is_empty() {
        used_text
    } else {
        format!("{} · {}", used_text, quota_view.detail)
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
        QuotaDetailSpec, QuotaDisplayMode, QuotaInfo, QuotaLabelSpec, QuotaType, StatusLevel,
        UpdateStatus,
    };

    // ── display_source_label ─────────────────────────────────

    #[test]
    fn provider_source_label_delegates_to_display_mapping() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Copilot, ConnectionStatus::Connected);
        p.runtime_source_label = Some("github api".to_string());
        assert_eq!(provider_source_label(&p), "GitHub");
    }

    #[test]
    fn display_source_label_maps_known_labels() {
        let _locale_guard = setup_locale();
        assert_eq!(display_source_label("github api"), "GitHub");
        assert_eq!(display_source_label("seat api"), "Devin Cloud");
        assert_eq!(
            display_source_label("seat api + local cache"),
            "Devin Cloud + Local cache"
        );
        assert_eq!(display_source_label("local api"), "Local language server");
    }

    #[test]
    fn display_source_label_falls_back_for_unknown() {
        let _locale_guard = setup_locale();
        assert_eq!(display_source_label("my-script"), "Custom");
        assert_eq!(display_source_label("foo cli"), "Local CLI");
        assert_eq!(display_source_label(""), "Automatic");
    }

    // ── format_relative_refresh_age / format_refresh_status ──

    #[test]
    fn format_relative_refresh_age_formats_compact_time() {
        let _locale_guard = setup_locale();
        assert_eq!(format_relative_refresh_age(0), "just now");
        assert_eq!(format_relative_refresh_age(120), "2 min ago");
        assert_eq!(format_relative_refresh_age(3600), "1 hr ago");
    }

    #[test]
    fn format_refresh_status_reflects_connection() {
        let _locale_guard = setup_locale();
        let connected = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        assert_eq!(format_refresh_status(&connected), "Waiting for data");

        let refreshing = make_provider(ProviderKind::Claude, ConnectionStatus::Refreshing);
        assert_eq!(format_refresh_status(&refreshing), "Refreshing…");

        let error = make_provider(ProviderKind::Claude, ConnectionStatus::Error);
        assert_eq!(format_refresh_status(&error), "Needs attention");

        let disconnected = make_provider(ProviderKind::Claude, ConnectionStatus::Disconnected);
        assert_eq!(format_refresh_status(&disconnected), "Not connected");
    }

    #[test]
    fn format_refresh_status_reports_update_failed() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.update_status = Some(UpdateStatus::Failed);
        assert_eq!(format_refresh_status(&p), "Update failed");
    }

    #[test]
    fn format_provider_updated_at_uses_not_fetched_without_instant() {
        let _locale_guard = setup_locale();
        let p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        assert_eq!(format_provider_updated_at(&p), "Not fetched yet");
    }

    #[test]
    fn format_provider_updated_at_uses_relative_time_with_instant() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.last_refreshed_instant = Some(std::time::Instant::now());
        assert_eq!(format_provider_updated_at(&p), "just now");
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
    fn quota_status_labels_describe_usage_severity_not_connectivity() {
        let _locale_guard = setup_locale();
        assert_eq!(format_quota_status_label(StatusLevel::Green), "OK");
        assert_eq!(format_quota_status_label(StatusLevel::Yellow), "WARN");
        assert_eq!(format_quota_status_label(StatusLevel::Red), "LOW");
    }

    #[test]
    fn quota_status_labels_are_localized() {
        let _locale_guard = crate::i18n::test_locale_guard("zh-CN");
        assert_eq!(format_quota_status_label(StatusLevel::Green), "充足");
        assert_eq!(format_quota_status_label(StatusLevel::Yellow), "注意");
        assert_eq!(format_quota_status_label(StatusLevel::Red), "偏低");
    }

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

    #[test]
    fn quota_card_display_text_balance_mode() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::balance_only("Balance", 10.0, Some(3.0), QuotaType::Credit, None);
        assert_eq!(
            format_quota_card_display_text(&q, QuotaDisplayMode::Remaining),
            "$10.00"
        );
    }

    #[test]
    fn quota_card_display_text_used_mode_for_percentage_quota() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::new("test", 25.0, 100.0);
        assert_eq!(
            format_quota_card_display_text(&q, QuotaDisplayMode::Used),
            "25"
        );
    }

    #[test]
    fn quota_card_mode_label_uses_balance_variant() {
        let _locale_guard = setup_locale();
        assert_eq!(
            format_quota_card_mode_label(true, QuotaDisplayMode::Remaining),
            "Balance"
        );
    }

    #[test]
    fn quota_card_has_unit_skips_credit_and_points() {
        let _locale_guard = setup_locale();
        assert!(!format_quota_card_has_unit(&QuotaInfo::with_details(
            "Credit",
            5.0,
            20.0,
            QuotaType::Credit,
            None,
        )));
        assert!(format_quota_card_has_unit(&QuotaInfo::new(
            "General", 25.0, 100.0
        )));
    }

    #[test]
    fn quota_card_detail_merges_usage_when_balance_mode_has_existing_detail() {
        let _locale_guard = setup_locale();
        let quota = QuotaInfo::balance_only("Balance", 10.0, Some(3.5), QuotaType::Credit, None);
        let view = QuotaDisplayViewState {
            quota,
            label: "Balance".to_string(),
            detail: "Resets tomorrow".to_string(),
        };
        assert_eq!(
            format_quota_card_detail_text(&view),
            "Used: $3.50 · Resets tomorrow"
        );
    }
}
