use anyhow::Result;
use log::{debug, info, warn};

use crate::models::RefreshData;
use crate::providers::{ProviderError, ProviderResult};

use super::extractor::{self, CompiledPatterns, CompiledStep};
use super::schema::{PlanDef, PlanMode, PlanStepDef, SourceDef};

/// 编译后的自定义 Provider 执行计划。
///
/// 持有 PlanDef（定义）和 CompiledPatterns（编译产物），
/// 保证两者始终一致，消除索引失配风险。
/// `CustomProvider` 只保留 provider 门面职责。
pub(super) struct CompiledPlan {
    plan: PlanDef,
    compiled_steps: Vec<CompiledStep>,
}

impl CompiledPlan {
    pub fn compile(plan: &PlanDef) -> Result<Self> {
        let compiled_steps = plan
            .steps
            .iter()
            .map(|step| {
                CompiledPatterns::compile(&step.parser).map(|pattern| CompiledStep { pattern })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            plan: plan.clone(),
            compiled_steps,
        })
    }

    pub fn step_count(&self) -> usize {
        self.compiled_steps.len()
    }

    pub fn mode(&self) -> PlanMode {
        self.plan.mode
    }

    /// 检查 plan 中所有必须可用的 step 是否就绪。
    pub fn check_availability(&self) -> ProviderResult<()> {
        match self.plan.mode {
            PlanMode::FirstSuccess => check_first_success_availability(&self.plan),
            PlanMode::Merge => check_merge_availability(&self.plan),
        }
    }

    /// 检查是否所有 step 均为 placeholder source（无实际数据获取能力）。
    pub fn is_placeholder_only(&self) -> bool {
        self.plan
            .steps
            .iter()
            .all(|step| matches!(step.source, SourceDef::Placeholder { .. }))
    }

    pub fn execute(
        &self,
        provider_id: &str,
        base_url: &Option<String>,
    ) -> ProviderResult<RefreshData> {
        match self.plan.mode {
            PlanMode::FirstSuccess => self.execute_first_success(provider_id, base_url),
            PlanMode::Merge => self.execute_merge(provider_id, base_url),
        }
    }

    fn execute_first_success(
        &self,
        provider_id: &str,
        base_url: &Option<String>,
    ) -> ProviderResult<RefreshData> {
        let mut errors = Vec::new();

        for (index, step) in self.plan.steps.iter().enumerate() {
            match self.execute_step(provider_id, base_url, index, step) {
                Ok(data) => return Ok(data.with_source_label(step.name.clone())),
                Err(err) => {
                    warn!(
                        target: "providers::custom",
                        "[{}] step '{}' failed: {}",
                        provider_id, step.name, err
                    );
                    errors.push(format!("{}: {}", step.name, err));
                    if step.required && !should_try_next_step(&err) {
                        return Err(err);
                    }
                }
            }
        }

        Err(ProviderError::fetch_failed(&format!(
            "all plan steps failed{}",
            format_errors(&errors)
        )))
    }

    fn execute_merge(
        &self,
        provider_id: &str,
        base_url: &Option<String>,
    ) -> ProviderResult<RefreshData> {
        let mut merged = RefreshData::quotas_only(Vec::new());
        let mut success_count = 0usize;
        let mut errors = Vec::new();

        for (index, step) in self.plan.steps.iter().enumerate() {
            match self.execute_step(provider_id, base_url, index, step) {
                Ok(data) => {
                    merge_refresh_data(&mut merged, data);
                    success_count += 1;
                }
                Err(err) if step.required => {
                    return Err(ProviderError::fetch_failed(&format!(
                        "required step '{}' failed: {}",
                        step.name, err
                    )));
                }
                Err(err) => {
                    warn!(
                        target: "providers::custom",
                        "[{}] optional step '{}' failed: {}",
                        provider_id, step.name, err
                    );
                    errors.push(format!("{}: {}", step.name, err));
                }
            }
        }

        // 防御层：extractor 已保证成功 step 必含 quota（见 extractor.rs:142, 239），
        // 但此处保留二次检查作为防御性编程，避免未来 extractor 变更后遗漏。
        if success_count == 0 || merged.quotas.is_empty() {
            return Err(ProviderError::no_data());
        }

        if !errors.is_empty() {
            debug!(
                target: "providers::custom",
                "[{}] merge completed with optional failures: {}",
                provider_id,
                errors.join("; ")
            );
        }

        Ok(merged.with_source_label("merged"))
    }

    fn execute_step(
        &self,
        provider_id: &str,
        base_url: &Option<String>,
        index: usize,
        step: &PlanStepDef,
    ) -> ProviderResult<RefreshData> {
        // 有意在 execute 阶段重新检查 availability：
        // check_availability() 是快速预检，但运行环境可能在预检与执行之间变化
        // （如 CLI 被卸载、文件被删除），因此执行时再次确认。
        if let Some(availability) = &step.availability {
            super::availability::check(availability)?;
        }

        let raw = super::fetch::fetch(provider_id, base_url, &step.source)?;
        debug!(target: "providers::custom", "[{}] step '{}' raw response ({} bytes): {}", provider_id, step.name, raw.len(), super::log_utils::truncate_for_log(&raw, 500));

        let raw = super::fetch::apply_preprocess(&raw, &step.preprocess);
        let parser = step.parser.as_ref().ok_or_else(|| {
            warn!(
                target: "providers::custom",
                "[{}] step '{}' has no parser configured",
                provider_id, step.name
            );
            ProviderError::unavailable("no parser configured")
        })?;

        let result = extractor::extract(parser, &raw, &self.compiled_steps[index]);
        match &result {
            Ok(data) => info!(
                target: "providers::custom",
                "[{}] step '{}' parsed {} quotas, email={:?}",
                provider_id, step.name, data.quotas.len(), data.account_email
            ),
            Err(e) => warn!(
                target: "providers::custom",
                "[{}] step '{}' parse failed: {}\n  raw response: {}",
                provider_id, step.name, e, super::log_utils::truncate_for_log(&raw, 300)
            ),
        }
        Ok(result?)
    }
}

fn check_first_success_availability(plan: &PlanDef) -> ProviderResult<()> {
    let mut errors = Vec::new();
    for step in &plan.steps {
        match check_step_availability(step) {
            Ok(()) => return Ok(()),
            Err(err) => {
                errors.push(format!("{}: {}", step.name, err));
                if step.required && !should_try_next_step(&err) {
                    return Err(err);
                }
            }
        }
    }

    Err(ProviderError::unavailable(&format!(
        "no plan step available{}",
        format_errors(&errors)
    )))
}

fn check_merge_availability(plan: &PlanDef) -> ProviderResult<()> {
    let has_required = plan.steps.iter().any(|step| step.required);
    if !has_required {
        return check_first_success_availability(plan);
    }

    let mut errors = Vec::new();
    for step in plan.steps.iter().filter(|step| step.required) {
        if let Err(err) = check_step_availability(step) {
            errors.push((step.name.clone(), err));
        }
    }

    match errors.len() {
        0 => Ok(()),
        1 => Err(errors.remove(0).1),
        _ => Err(ProviderError::unavailable(&format!(
            "required plan steps unavailable{}",
            format_errors(
                &errors
                    .iter()
                    .map(|(name, err)| format!("{}: {}", name, err))
                    .collect::<Vec<_>>()
            )
        ))),
    }
}

fn check_step_availability(step: &PlanStepDef) -> ProviderResult<()> {
    let Some(availability) = &step.availability else {
        return Ok(());
    };
    super::availability::check(availability).map_err(ProviderError::from)
}

fn should_try_next_step(err: &ProviderError) -> bool {
    if is_rate_limit_error(err) {
        return false;
    }
    matches!(
        err,
        ProviderError::Timeout
            | ProviderError::NetworkFailed { .. }
            | ProviderError::FetchFailed { .. }
            | ProviderError::ParseFailed { .. }
            | ProviderError::NoData
            | ProviderError::CliNotFound { .. }
            | ProviderError::Unavailable { .. }
    )
}

fn is_rate_limit_error(err: &ProviderError) -> bool {
    matches!(
        err,
        ProviderError::FetchFailed {
            advice: Some(crate::models::FailureAdvice::ApiHttpError { status }),
            ..
        } if status == "429"
    )
}

fn merge_refresh_data(target: &mut RefreshData, data: RefreshData) {
    target.quotas.extend(data.quotas);
    if target.account_email.is_none() {
        target.account_email = data.account_email;
    }
    if target.account_tier.is_none() {
        target.account_tier = data.account_tier;
    }
}

fn format_errors(errors: &[String]) -> String {
    if errors.is_empty() {
        String::new()
    } else {
        format!(": {}", errors.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use crate::models::{FailureAdvice, QuotaInfo};

    use super::*;

    fn unavailable_step(name: &str, required: bool) -> PlanStepDef {
        PlanStepDef {
            name: name.to_string(),
            required,
            availability: Some(super::super::schema::AvailabilityDef::EnvVar {
                value: format!("BANANATRAY_MISSING_TEST_ENV_{}", name),
            }),
            source: SourceDef::Placeholder {
                reason: "test placeholder".to_string(),
            },
            parser: None,
            preprocess: vec![],
        }
    }

    fn missing_file_step(name: &str, required: bool) -> PlanStepDef {
        PlanStepDef {
            name: name.to_string(),
            required,
            availability: Some(super::super::schema::AvailabilityDef::FileExists {
                value: format!("/nonexistent/bananatray-test-{}", name),
            }),
            source: SourceDef::Placeholder {
                reason: "test placeholder".to_string(),
            },
            parser: None,
            preprocess: vec![],
        }
    }

    fn available_step(name: &str, required: bool) -> PlanStepDef {
        PlanStepDef {
            name: name.to_string(),
            required,
            availability: None,
            source: SourceDef::Placeholder {
                reason: "test placeholder".to_string(),
            },
            parser: None,
            preprocess: vec![],
        }
    }

    #[test]
    fn merge_refresh_data_keeps_first_account_metadata() {
        let mut target = RefreshData::with_account(
            vec![QuotaInfo::new("First", 1.0, 10.0)],
            Some("first@example.com".to_string()),
            Some("Pro".to_string()),
        );
        let data = RefreshData::with_account(
            vec![QuotaInfo::new("Second", 2.0, 20.0)],
            Some("second@example.com".to_string()),
            Some("Team".to_string()),
        );

        merge_refresh_data(&mut target, data);

        assert_eq!(target.quotas.len(), 2);
        assert_eq!(target.account_email.as_deref(), Some("first@example.com"));
        assert_eq!(target.account_tier.as_deref(), Some("Pro"));
    }

    #[test]
    fn should_not_fallback_on_rate_limit() {
        let err = ProviderError::FetchFailed {
            advice: Some(FailureAdvice::ApiHttpError {
                status: "429".to_string(),
            }),
            raw_detail: Some("rate limited".to_string()),
        };

        assert!(!should_try_next_step(&err));
    }

    #[test]
    fn should_fallback_on_parse_failed() {
        assert!(should_try_next_step(&ProviderError::parse_failed(
            "bad payload"
        )));
    }

    #[test]
    fn first_success_availability_accepts_optional_fallback_step() {
        let plan = PlanDef {
            mode: PlanMode::FirstSuccess,
            steps: vec![
                missing_file_step("primary", true),
                available_step("fallback", false),
            ],
        };

        assert!(check_first_success_availability(&plan).is_ok());
    }

    #[test]
    fn first_success_availability_stops_on_config_missing_required_step() {
        let plan = PlanDef {
            mode: PlanMode::FirstSuccess,
            steps: vec![
                unavailable_step("primary", true),
                available_step("fallback", false),
            ],
        };
        let err = check_first_success_availability(&plan).unwrap_err();

        assert!(matches!(err, ProviderError::ConfigMissing { .. }));
    }

    #[test]
    fn merge_availability_requires_all_required_steps() {
        let plan = PlanDef {
            mode: PlanMode::Merge,
            steps: vec![
                available_step("usage", true),
                unavailable_step("credits", true),
                available_step("optional", false),
            ],
        };
        let err = check_merge_availability(&plan).unwrap_err();

        assert!(matches!(err, ProviderError::ConfigMissing { .. }));
    }

    #[test]
    fn merge_availability_accepts_when_only_optional_available() {
        let plan = PlanDef {
            mode: PlanMode::Merge,
            steps: vec![
                unavailable_step("optional-a", false),
                available_step("optional-b", false),
            ],
        };

        assert!(check_merge_availability(&plan).is_ok());
    }

    // ── execute tests ──────────────────────────

    /// 构造一个 CLI echo step，输出可被 regex 解析的 "used/limit" 数据
    fn cli_echo_step(name: &str, output: &str, required: bool) -> PlanStepDef {
        PlanStepDef {
            name: name.to_string(),
            required,
            availability: None,
            source: SourceDef::Cli {
                command: "echo".to_string(),
                args: vec![output.to_string()],
                timeout_ms: None,
            },
            parser: Some(super::super::schema::ParserDef::Regex {
                account_email: None,
                quotas: vec![super::super::schema::RegexQuotaRule {
                    label: name.to_string(),
                    pattern: r"(\d+)/(\d+)".to_string(),
                    used_group: 1,
                    limit_group: 2,
                    quota_type: super::super::schema::QuotaTypeDef::General,
                    divisor: None,
                }],
            }),
            preprocess: vec![],
        }
    }

    #[test]
    fn execute_first_success_returns_first_ok() {
        let plan = PlanDef {
            mode: PlanMode::FirstSuccess,
            steps: vec![
                cli_echo_step("primary", "10/100", true),
                cli_echo_step("fallback", "20/200", false),
            ],
        };
        let compiled = CompiledPlan::compile(&plan).unwrap();
        let data = compiled.execute("test", &None).unwrap();

        // 应该返回第一个 step 的数据
        assert_eq!(data.quotas.len(), 1);
        assert_eq!(data.quotas[0].used, 10.0);
        assert_eq!(data.quotas[0].limit, 100.0);
    }

    #[test]
    fn execute_first_success_falls_back_on_failure() {
        let plan = PlanDef {
            mode: PlanMode::FirstSuccess,
            steps: vec![
                // 第一个 step 不可用（文件不存在）
                missing_file_step("primary", false),
                cli_echo_step("fallback", "30/300", false),
            ],
        };
        let compiled = CompiledPlan::compile(&plan).unwrap();
        let data = compiled.execute("test", &None).unwrap();

        assert_eq!(data.quotas.len(), 1);
        assert_eq!(data.quotas[0].used, 30.0);
        assert_eq!(data.quotas[0].limit, 300.0);
    }

    #[test]
    fn execute_first_success_all_fail_returns_error() {
        let plan = PlanDef {
            mode: PlanMode::FirstSuccess,
            steps: vec![missing_file_step("a", false), missing_file_step("b", false)],
        };
        let compiled = CompiledPlan::compile(&plan).unwrap();
        let err = compiled.execute("test", &None).unwrap_err();

        assert!(matches!(err, ProviderError::FetchFailed { .. }));
    }

    #[test]
    fn execute_merge_combines_quotas() {
        let plan = PlanDef {
            mode: PlanMode::Merge,
            steps: vec![
                cli_echo_step("usage", "10/100", true),
                cli_echo_step("credits", "5/50", true),
            ],
        };
        let compiled = CompiledPlan::compile(&plan).unwrap();
        let data = compiled.execute("test", &None).unwrap();

        assert_eq!(data.quotas.len(), 2);
    }

    #[test]
    fn execute_merge_tolerates_optional_failure() {
        let plan = PlanDef {
            mode: PlanMode::Merge,
            steps: vec![
                cli_echo_step("usage", "10/100", true),
                // optional step 不可用，不应导致整体失败
                missing_file_step("optional-info", false),
            ],
        };
        let compiled = CompiledPlan::compile(&plan).unwrap();
        let data = compiled.execute("test", &None).unwrap();

        assert_eq!(data.quotas.len(), 1);
        assert_eq!(data.quotas[0].used, 10.0);
    }

    #[test]
    fn execute_merge_fails_on_required_step_failure() {
        let plan = PlanDef {
            mode: PlanMode::Merge,
            steps: vec![
                cli_echo_step("usage", "10/100", false),
                // required step 不可用，应导致整体失败
                missing_file_step("credits", true),
            ],
        };
        let compiled = CompiledPlan::compile(&plan).unwrap();
        let err = compiled.execute("test", &None).unwrap_err();

        assert!(matches!(err, ProviderError::FetchFailed { .. }));
    }

    #[test]
    fn execute_merge_single_step_returns_data() {
        // 单 step merge 模式也应正常返回数据
        let plan = PlanDef {
            mode: PlanMode::Merge,
            steps: vec![cli_echo_step("usage", "10/100", true)],
        };
        let compiled = CompiledPlan::compile(&plan).unwrap();
        let data = compiled.execute("test", &None).unwrap();

        assert_eq!(data.quotas.len(), 1);
        assert_eq!(data.quotas[0].used, 10.0);
    }
}
