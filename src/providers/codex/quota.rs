use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};

const SECONDS_PER_MINUTE: i64 = 60;
const MINUTES_PER_HOUR: i64 = 60;
const HOURS_PER_DAY: i64 = 24;
const DAYS_PER_WEEK: i64 = 7;
const SESSION_WINDOW_HOURS: i64 = 5;
const SESSION_WINDOW_MINUTES: i64 = SESSION_WINDOW_HOURS * MINUTES_PER_HOUR;
const WEEKLY_WINDOW_MINUTES: i64 = DAYS_PER_WEEK * HOURS_PER_DAY * MINUTES_PER_HOUR;

/// 解析 Codex usage / RPC 响应后的结构化结果。
///
/// `plan_type` 对齐 CodexBar `CodexUsageResponse.planType`，由调用方与 JWT 中的
/// `chatgpt_plan_type` 合并后填入 `RefreshData::account_tier`。
#[derive(Debug, Clone, Default)]
pub(super) struct ParsedUsage {
    pub quotas: Vec<QuotaInfo>,
    pub plan_type: Option<String>,
}

/// Codex rate-limit 窗口的语义角色。
///
/// 与 CodexBar 的 `CodexRateWindowNormalizer` 保持一致：
/// 通过窗口分钟数得到角色，300 分钟 = 5h session，10080 分钟 = weekly。
/// 免费套餐只有 weekly 窗口，API 可能把它返回在 primary 字段内，此时必须按
/// 窗口时长分类，而不是盲目按字段位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowRole {
    Session,
    Weekly,
}

impl WindowRole {
    fn label_spec(self) -> QuotaLabelSpec {
        match self {
            WindowRole::Session => QuotaLabelSpec::Session,
            WindowRole::Weekly => QuotaLabelSpec::Weekly,
        }
    }

    fn quota_type(self) -> QuotaType {
        match self {
            WindowRole::Session => QuotaType::Session,
            WindowRole::Weekly => QuotaType::Weekly,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct WindowQuotaInput {
    pub default_role: WindowRole,
    pub used_percent: f64,
    pub reset_at: Option<i64>,
    pub window_minutes: Option<i64>,
}

pub(super) fn window_seconds_to_minutes(window_seconds: Option<i64>) -> Option<i64> {
    window_seconds.map(|seconds| seconds / SECONDS_PER_MINUTE)
}

/// 根据窗口分钟数判断窗口角色；若缺失或异常则回退到给定的默认角色。
pub(super) fn resolve_role_from_minutes(
    window_minutes: Option<i64>,
    default_role: WindowRole,
) -> WindowRole {
    match window_minutes {
        Some(SESSION_WINDOW_MINUTES) => WindowRole::Session,
        Some(WEEKLY_WINDOW_MINUTES) => WindowRole::Weekly,
        _ => default_role,
    }
}

pub(super) fn build_window_quota(input: WindowQuotaInput) -> QuotaInfo {
    let role = resolve_role_from_minutes(input.window_minutes, input.default_role);

    QuotaInfo::from_used_percent(
        role.label_spec(),
        input.used_percent,
        role.quota_type(),
        input
            .reset_at
            .map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs }),
    )
}

/// 服务端异常返回两个相同角色窗口时，只保留第一个。
pub(super) fn deduplicate_window_roles(quotas: &mut Vec<QuotaInfo>) {
    if quotas.len() == 2 && quotas[0].quota_type == quotas[1].quota_type {
        quotas.truncate(1);
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum CreditBalance<'a> {
    Number(f64),
    Text(&'a str),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CreditsInput<'a> {
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: Option<CreditBalance<'a>>,
}

/// 读取 credits 余额，对齐 CodexBar 的 `CreditDetails`：
/// - `has_credits` 默认由调用方保守解析为 false，需要显式 true 才展示余额
/// - `unlimited == true` 跳过
/// - `balance` 支持数字或字符串（与 CodexBar 的宽松解码一致）
pub(super) fn read_credits_balance(input: CreditsInput<'_>) -> Option<f64> {
    if !input.has_credits || input.unlimited {
        return None;
    }

    match input.balance? {
        CreditBalance::Number(balance) => Some(balance),
        CreditBalance::Text(balance) => balance.parse::<f64>().ok(),
    }
}

pub(super) fn build_credit_balance_quota(balance: f64) -> QuotaInfo {
    QuotaInfo::balance_only(
        QuotaLabelSpec::Credits,
        balance,
        None,
        QuotaType::Credit,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_role_falls_back_for_missing_or_unknown_window() {
        assert_eq!(
            resolve_role_from_minutes(None, WindowRole::Session),
            WindowRole::Session
        );
        assert_eq!(
            resolve_role_from_minutes(None, WindowRole::Weekly),
            WindowRole::Weekly
        );
        assert_eq!(
            resolve_role_from_minutes(Some(999_999), WindowRole::Session),
            WindowRole::Session
        );
    }

    #[test]
    fn window_seconds_to_minutes_converts_codex_api_window_unit() {
        assert_eq!(
            window_seconds_to_minutes(Some(SESSION_WINDOW_MINUTES * SECONDS_PER_MINUTE)),
            Some(SESSION_WINDOW_MINUTES)
        );
        assert_eq!(
            window_seconds_to_minutes(Some(WEEKLY_WINDOW_MINUTES * SECONDS_PER_MINUTE)),
            Some(WEEKLY_WINDOW_MINUTES)
        );
        assert_eq!(window_seconds_to_minutes(None), None);
    }

    #[test]
    fn resolve_role_uses_exact_window_minutes() {
        assert_eq!(
            resolve_role_from_minutes(Some(SESSION_WINDOW_MINUTES), WindowRole::Weekly),
            WindowRole::Session
        );
        assert_eq!(
            resolve_role_from_minutes(Some(WEEKLY_WINDOW_MINUTES), WindowRole::Session),
            WindowRole::Weekly
        );
    }

    #[test]
    fn build_window_quota_maps_role_usage_and_reset() {
        let quota = build_window_quota(WindowQuotaInput {
            default_role: WindowRole::Session,
            used_percent: 42.0,
            reset_at: Some(1_735_000_000),
            window_minutes: Some(WEEKLY_WINDOW_MINUTES),
        });

        assert_eq!(quota.label_spec, QuotaLabelSpec::Weekly);
        assert_eq!(quota.quota_type, QuotaType::Weekly);
        assert_eq!(quota.used, 42.0);
        assert!(matches!(
            quota.detail_spec,
            Some(QuotaDetailSpec::ResetAt {
                epoch_secs: 1_735_000_000
            })
        ));
    }

    #[test]
    fn deduplicate_window_roles_keeps_first_duplicate_role() {
        let mut quotas = vec![
            build_window_quota(WindowQuotaInput {
                default_role: WindowRole::Session,
                used_percent: 10.0,
                reset_at: None,
                window_minutes: Some(WEEKLY_WINDOW_MINUTES),
            }),
            build_window_quota(WindowQuotaInput {
                default_role: WindowRole::Weekly,
                used_percent: 20.0,
                reset_at: None,
                window_minutes: Some(WEEKLY_WINDOW_MINUTES),
            }),
        ];

        deduplicate_window_roles(&mut quotas);

        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].quota_type, QuotaType::Weekly);
        assert_eq!(quotas[0].used, 10.0);
    }

    #[test]
    fn read_credits_balance_requires_explicit_credit_state() {
        assert_eq!(
            read_credits_balance(CreditsInput {
                has_credits: false,
                unlimited: false,
                balance: Some(CreditBalance::Number(12.5)),
            }),
            None
        );
        assert_eq!(
            read_credits_balance(CreditsInput {
                has_credits: true,
                unlimited: true,
                balance: Some(CreditBalance::Number(12.5)),
            }),
            None
        );
    }

    #[test]
    fn read_credits_balance_accepts_number_and_text() {
        assert_eq!(
            read_credits_balance(CreditsInput {
                has_credits: true,
                unlimited: false,
                balance: Some(CreditBalance::Number(12.5)),
            }),
            Some(12.5)
        );
        assert_eq!(
            read_credits_balance(CreditsInput {
                has_credits: true,
                unlimited: false,
                balance: Some(CreditBalance::Text("7.25")),
            }),
            Some(7.25)
        );
    }

    #[test]
    fn build_credit_balance_quota_uses_balance_only_shape() {
        let quota = build_credit_balance_quota(12.5);

        assert_eq!(quota.label_spec, QuotaLabelSpec::Credits);
        assert_eq!(quota.quota_type, QuotaType::Credit);
        assert!(quota.is_balance_only());
        assert_eq!(quota.remaining_balance, Some(12.5));
    }
}
