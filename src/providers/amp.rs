use super::{
    AiProvider, ProviderCapabilities, ProviderError, ProviderExecutionContext, ProviderResult,
};
use crate::models::{
    ProviderDescriptor, ProviderKind, ProviderMetadata, QuotaDetailSpec, QuotaInfo, QuotaLabelSpec,
    QuotaType, RefreshData,
};
use crate::providers::common::cli;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

// 预编译的正则表达式
static EMAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Signed in as\s+(\S+)\s+\(").unwrap());
// "Amp Free: $10/$10 remaining (replenishes ...)" 或 "Monthly credits: $15.00 / $20.00 remaining"
// 后面可能跟 "(replenishes ...)" 或 "- https://..." 等附加文本。
static CREDIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(.+?):\s*\$([0-9]+(?:\.[0-9]+)?)\s*/\s*\$([0-9]+(?:\.[0-9]+)?)\s+remaining")
        .unwrap()
});
// 无 total 的余额格式 — 必须放在 CREDIT_RE 之后作为回退：
// "Individual credits: $0 remaining" 或 "Credits: $50.00 remaining"
// 后面可能跟 url 或其他文本（如 "- https://..."），用 \s 终止金额匹配。
static BALANCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(.+?):\s*\$([0-9]+(?:\.[0-9]+)?)\s+remaining").unwrap());
// 百分比配额 — amp CLI 上游将 Free 档从信用额度改为每日百分比重置：
// "Amp Free: 100% remaining today (resets daily) - https://..."
// 不硬依赖 "today"，以兼容上游改变重置周期措辞（如 "remaining this week"）。
// 必须放在 CREDIT_RE / BALANCE_RE 之前，因为三者都以 "<label>:" 开头，但只有此格式用 `%` 而非 `$`。
static PERCENT_REMAINING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(.+?):\s*([0-9]+(?:\.[0-9]+)?)%\s+remaining\b").unwrap());
// 百分比行中括号内的重置说明，如 "(resets daily)"，原文透传到卡片详情行。
static RESET_NOTE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(([^)]+)\)").unwrap());

// 订阅制（Megawatt / Gigawatt）输出，2026 年 amp 上线月度订阅后的主推计费方式：
//   "Subscription Megawatt: 81% other usage and 100% orb usage remaining"
// 一行含两个独立月度池：other usage（agent 调用额度）与 orb usage（远程实例额度）。
// 两个池独立计费、独立耗尽，拆成独立 quota 分别展示。
// 行前缀 "Subscription <Plan>:" 锚定；池片段用通用正则全局匹配，
// 支持任意池数量 / 顺序 / 未来新增池名。
static SUBSCRIPTION_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^Subscription\s+(.+?):\s*(.+)$").unwrap());
static SUBSCRIPTION_POOL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([0-9]+(?:\.[0-9]+)?)%\s+(\w+)\s+usage").unwrap());

super::define_unit_provider!(AmpProvider);

impl AmpProvider {
    fn quota_label_spec(label: &str) -> QuotaLabelSpec {
        match label.to_ascii_lowercase().as_str() {
            "monthly credits" => QuotaLabelSpec::MonthlyCredits,
            "credits" => QuotaLabelSpec::Credits,
            "bonus credits" => QuotaLabelSpec::BonusCredits,
            _ => QuotaLabelSpec::Raw(label.to_string()),
        }
    }

    fn run_usage() -> Result<String> {
        let output = cli::run_lenient_command("amp", &["usage", "--no-color"])?;
        Ok(output)
    }

    fn parse_usage_output(output_str: &str) -> Result<RefreshData> {
        let mut quotas = Vec::new();
        let mut account_email = None;

        for line in output_str.lines() {
            let line = line.trim();

            if account_email.is_none() {
                if let Some(caps) = EMAIL_RE.captures(line) {
                    account_email = Some(caps[1].to_string());
                }
            }

            // 订阅制：拆成多个独立池 quota（other / orb 等），百分比模式。
            // 放在最前以避免被后续百分比 / 信用正则误匹配订阅行。
            if let Some(caps) = SUBSCRIPTION_LINE_RE.captures(line) {
                let plan = caps[1].trim();
                let rest = &caps[2];
                for pool_caps in SUBSCRIPTION_POOL_RE.captures_iter(rest) {
                    let remaining_percent: f64 = pool_caps[1].parse().unwrap_or(0.0);
                    // pool 保留 CLI 原文小写（other / orb），selector 再按 locale 渲染
                    let pool = pool_caps[2].to_ascii_lowercase();
                    quotas.push(QuotaInfo::from_remaining_percent(
                        QuotaLabelSpec::SubscriptionUsage {
                            plan: plan.to_string(),
                            pool,
                        },
                        remaining_percent,
                        QuotaType::General,
                        None,
                    ));
                }
                continue;
            }

            if let Some(caps) = PERCENT_REMAINING_RE.captures(line) {
                // amp Free 档现为每日百分比重置配额（非信用额度）。
                // label 保留原文（如 "Amp Free"），stable_key 与历史 Credit 模式一致，
                // 设置持久化（hidden_quotas）不受影响。
                let label = caps[1].trim();
                let remaining_percent: f64 = caps[2].parse().unwrap_or(0.0);
                // 括号内的重置说明（如 "resets daily"）原文透传到详情行
                let detail = RESET_NOTE_RE
                    .captures(line)
                    .map(|c| QuotaDetailSpec::Raw(c[1].trim().to_string()));
                quotas.push(QuotaInfo::from_remaining_percent(
                    Self::quota_label_spec(label),
                    remaining_percent,
                    QuotaType::General,
                    detail,
                ));
            } else if let Some(caps) = CREDIT_RE.captures(line) {
                let label = caps[1].trim();
                let remaining: f64 = caps[2].parse().unwrap_or(0.0);
                let total: f64 = caps[3].parse().unwrap_or(0.0);
                let used = total - remaining;
                quotas.push(QuotaInfo::with_details(
                    Self::quota_label_spec(label),
                    used.max(0.0),
                    total,
                    QuotaType::Credit,
                    Some(QuotaDetailSpec::CreditRemaining { remaining, total }),
                ));
            } else if let Some(caps) = BALANCE_RE.captures(line) {
                let label = caps[1].trim();
                let balance: f64 = caps[2].parse().unwrap_or(0.0);

                // $0 remaining 表示"未购买付费信用额度"，不等同于"额度耗尽"，
                // 展示为 Red 会误导免费用户。跳过零余额条目。
                if balance <= 0.0 {
                    continue;
                }

                // 使用 balance_only 模式：状态由余额绝对值决定（>=5 Green, >=1 Yellow, <1 Red），
                // 而非百分比——避免 limit=0 时 percent_remaining=0% 误判为 Red。
                quotas.push(QuotaInfo::balance_only(
                    Self::quota_label_spec(label),
                    balance,
                    None,
                    QuotaType::Credit,
                    Some(QuotaDetailSpec::CreditRemaining {
                        remaining: balance,
                        total: balance,
                    }),
                ));
            }
        }

        if quotas.is_empty() {
            return Err(ProviderError::parse_failed(&format!(
                "cannot parse amp usage output ({} bytes)",
                output_str.len()
            ))
            .into());
        }

        Ok(RefreshData::with_account(quotas, account_email, None))
    }
}

#[async_trait]
impl AiProvider for AmpProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("amp:cli"),
            metadata: ProviderMetadata {
                kind: ProviderKind::Amp,
                display_name: "Amp".into(),
                brand_name: "Amp".into(),
                icon_asset: "src/icons/provider-amp.svg".into(),
                dashboard_url: "https://ampcode.com/settings".into(),
                account_hint: "Amp CLI".into(),
                source_label: "amp cli".into(),
            },
        }
    }

    async fn check_availability(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<()> {
        if cli::command_exists("amp") {
            Ok(())
        } else {
            Err(ProviderError::cli_not_found("amp"))
        }
    }

    async fn refresh(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<RefreshData> {
        let start = std::time::Instant::now();
        log::debug!(target: "providers", "amp: running `amp usage --no-color`");

        let output = Self::run_usage()?;
        log::debug!(
            target: "providers",
            "amp: cli completed in {:.2}s, output_bytes={}",
            start.elapsed().as_secs_f64(),
            output.len()
        );

        Ok(Self::parse_usage_output(&output)?)
    }
}

impl ProviderCapabilities for AmpProvider {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_credit_with_total() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output =
            "Signed in as user@example.com (Pro)\nMonthly credits: $15.00 / $20.00 remaining\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.account_email.as_deref(), Some("user@example.com"));
        assert_eq!(data.quotas.len(), 1);

        let q = &data.quotas[0];
        assert_eq!(q.label_spec, crate::models::QuotaLabelSpec::MonthlyCredits);
        assert_eq!(q.used, 5.0);
        assert_eq!(q.limit, 20.0);
        assert_eq!(q.quota_type, QuotaType::Credit);
        assert_eq!(
            q.detail_spec,
            Some(QuotaDetailSpec::CreditRemaining {
                remaining: 15.0,
                total: 20.0,
            })
        );
    }

    #[test]
    fn test_parse_balance_only() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output = "Credits: $50.00 remaining\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.quotas.len(), 1);
        let q = &data.quotas[0];
        assert_eq!(q.label_spec, crate::models::QuotaLabelSpec::Credits);
        assert!(q.is_balance_only());
        assert!((q.remaining_balance.unwrap() - 50.0).abs() < f64::EPSILON);
        assert_eq!(q.quota_type, QuotaType::Credit);
        assert_eq!(q.status_level(), crate::models::StatusLevel::Green);
    }

    /// 实际 amp CLI 输出（2026-07 上游变更后）：Free 档改为每日百分比重置 + 零余额次要额度
    #[test]
    fn test_parse_real_world_free_tier() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output = "Signed in as user@example.com (user)\n\
            Amp Free: 100% remaining today (resets daily) - https://ampcode.com/settings#amp-free\n\
            Individual credits: $0 remaining - https://ampcode.com/settings\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.account_email.as_deref(), Some("user@example.com"));
        // $0 余额的 Individual credits 应被跳过
        assert_eq!(data.quotas.len(), 1);
        let q = &data.quotas[0];
        assert_eq!(
            q.label_spec,
            crate::models::QuotaLabelSpec::Raw("Amp Free".to_string())
        );
        // 百分比模式：limit=100, used=0（100% remaining）
        assert_eq!(q.used, 0.0);
        assert_eq!(q.limit, 100.0);
        assert_eq!(q.quota_type, QuotaType::General);
        assert_eq!(q.status_level(), crate::models::StatusLevel::Green);
        // 括号内的重置说明透传到详情行
        assert_eq!(
            q.detail_spec,
            Some(QuotaDetailSpec::Raw("resets daily".to_string()))
        );
    }

    /// 每日百分比部分消耗：38% remaining → used=62%, Yellow
    #[test]
    fn test_parse_daily_percent_partial() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output =
            "Amp Free: 38% remaining today (resets daily) - https://ampcode.com/settings\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.quotas.len(), 1);
        let q = &data.quotas[0];
        assert!((q.used - 62.0).abs() < f64::EPSILON);
        assert_eq!(q.limit, 100.0);
        assert_eq!(q.quota_type, QuotaType::General);
        assert_eq!(q.status_level(), crate::models::StatusLevel::Yellow);
        assert_eq!(
            q.detail_spec,
            Some(QuotaDetailSpec::Raw("resets daily".to_string()))
        );
    }

    /// 不硬依赖 "today"：上游若改变重置周期措辞仍可解析；无括号说明时详情为空
    #[test]
    fn test_parse_percent_without_today_or_reset_note() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output = "Amp Free: 75% remaining - https://ampcode.com/settings\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.quotas.len(), 1);
        let q = &data.quotas[0];
        assert!((q.used - 25.0).abs() < f64::EPSILON);
        assert_eq!(q.limit, 100.0);
        assert_eq!(q.detail_spec, None);
    }

    /// 零余额纯信用额度行应被跳过
    #[test]
    fn test_parse_zero_balance_only_is_skipped() {
        let output = "Individual credits: $0 remaining\n";
        assert!(
            AmpProvider::parse_usage_output(output).is_err(),
            "zero-balance-only output should produce no quotas → error"
        );
    }

    /// CREDIT_RE + 非零 BALANCE_RE 同时存在时都应产出 quota
    #[test]
    fn test_parse_mixed_credit_and_balance() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output = "Monthly credits: $5.00 / $20.00 remaining\n\
            Bonus credits: $3.00 remaining\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.quotas.len(), 2);
        assert_eq!(
            data.quotas[0].label_spec,
            crate::models::QuotaLabelSpec::MonthlyCredits
        );
        assert!(!data.quotas[0].is_balance_only());
        assert_eq!(
            data.quotas[1].label_spec,
            crate::models::QuotaLabelSpec::BonusCredits
        );
        assert!(data.quotas[1].is_balance_only());
        assert_eq!(
            data.quotas[1].status_level(),
            crate::models::StatusLevel::Yellow,
            "$3.00 should be Yellow (>=1 && <5)"
        );
    }

    #[test]
    fn test_parse_error_does_not_expose_cli_output() {
        let output = "Signed in as private@example.com (user)\nno quota data\n";
        let error = AmpProvider::parse_usage_output(output)
            .unwrap_err()
            .to_string();

        assert!(error.contains("cannot parse amp usage output"));
        assert!(!error.contains("private@example.com"));
        assert!(!error.contains("no quota data"));
    }

    /// 实际 amp CLI 输出（订阅制）：Megawatt 套餐含 other / orb 两个独立月度池。
    /// 2026 年 amp 上线月度订阅后的主推计费格式。
    #[test]
    fn test_parse_subscription_megawatt() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output = "Signed in as user@example.com (user)\n\
            Subscription Megawatt: 81% other usage and 100% orb usage remaining\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.account_email.as_deref(), Some("user@example.com"));
        assert_eq!(data.quotas.len(), 2);

        // other usage（agent 调用额度）：81% remaining -> used=19
        let q0 = &data.quotas[0];
        assert_eq!(
            q0.label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "Megawatt".into(),
                pool: "other".into(),
            }
        );
        assert!((q0.used - 19.0).abs() < f64::EPSILON);
        assert_eq!(q0.limit, 100.0);
        assert_eq!(q0.quota_type, QuotaType::General);
        assert_eq!(q0.status_level(), crate::models::StatusLevel::Green);

        // orb usage（远程实例额度）：100% remaining -> used=0
        let q1 = &data.quotas[1];
        assert_eq!(
            q1.label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "Megawatt".into(),
                pool: "orb".into(),
            }
        );
        assert!((q1.used - 0.0).abs() < f64::EPSILON);
        assert_eq!(q1.limit, 100.0);
        assert_eq!(q1.status_level(), crate::models::StatusLevel::Green);
    }

    /// 订阅池状态等级：other 38%（Yellow 20-50）、orb 5%（Red <20）
    #[test]
    fn test_parse_subscription_status_levels() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output = "Subscription Gigawatt: 38% other usage and 5% orb usage remaining\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.quotas.len(), 2);
        assert_eq!(
            data.quotas[0].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "Gigawatt".into(),
                pool: "other".into(),
            }
        );
        assert_eq!(
            data.quotas[0].status_level(),
            crate::models::StatusLevel::Yellow
        );
        assert_eq!(
            data.quotas[1].status_level(),
            crate::models::StatusLevel::Red
        );
    }

    /// stable_key 应包含计划名与池标识，用于 hidden_quotas 持久化去重
    #[test]
    fn test_parse_subscription_stable_key() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output = "Subscription Megawatt: 81% other usage and 100% orb usage remaining\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();
        assert_eq!(data.quotas[0].stable_key, "subscription:megawatt:other");
        assert_eq!(data.quotas[1].stable_key, "subscription:megawatt:orb");
    }

    /// 订阅用户耗尽 agent usage 后追加 credits：订阅行与信用行应各自解析
    #[test]
    fn test_parse_subscription_mixed_with_credits() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output = "Subscription Megawatt: 0% other usage and 50% orb usage remaining\n\
            Monthly credits: $5.00 / $20.00 remaining\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.quotas.len(), 3);
        // 前两个为订阅池（General），第三个为信用额度（Credit）
        assert_eq!(data.quotas[0].quota_type, QuotaType::General);
        assert_eq!(data.quotas[1].quota_type, QuotaType::General);
        assert_eq!(data.quotas[2].quota_type, QuotaType::Credit);
        assert_eq!(
            data.quotas[2].label_spec,
            crate::models::QuotaLabelSpec::MonthlyCredits
        );
    }
}
