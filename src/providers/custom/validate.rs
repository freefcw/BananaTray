//! 自定义 Provider 定义的 schema 校验（纯函数，加载时 fail-fast）。
//!
//! 从 `loader.rs` 拆出：校验规则随 schema 演进、迁移代码随版本淘汰、
//! 扫描加载随启动逻辑演进，三者变更原因不同。

use anyhow::Result;
use regex::Regex;

use crate::models::ProviderKind;

use super::schema::{
    AuthDef, CustomProviderDef, HeaderDef, HttpMethodDef, ParserDef, PlanStepDef, RegexQuotaRule,
    SourceDef,
};

/// 校验定义的合法性，在加载时 fail-fast
pub(super) fn validate(def: &CustomProviderDef) -> Result<()> {
    if def.schema_version != 2 {
        anyhow::bail!("'schema_version' is {} but must be 2", def.schema_version);
    }
    if def.id.trim().is_empty() {
        anyhow::bail!("'id' cannot be empty");
    }
    if ProviderKind::from_id_key(&def.id).is_some() {
        anyhow::bail!(
            "custom provider id '{}' is reserved by a built-in provider; choose a unique id",
            def.id
        );
    }
    if def.metadata.display_name.is_empty() {
        anyhow::bail!("'metadata.display_name' cannot be empty");
    }

    if def.plan.steps.is_empty() {
        anyhow::bail!("'plan.steps' must contain at least one step");
    }
    for step in &def.plan.steps {
        validate_step(step)?;
    }

    Ok(())
}

fn validate_step(step: &PlanStepDef) -> Result<()> {
    if step.name.trim().is_empty() {
        anyhow::bail!("plan step name cannot be empty");
    }
    validate_source(&step.source)?;
    if !matches!(step.source, SourceDef::Placeholder { .. }) && step.parser.is_none() {
        anyhow::bail!(
            "plan step '{}': 'parser' is required unless source.type is placeholder",
            step.name
        );
    }
    validate_parser(&step.parser)?;
    Ok(())
}

fn validate_source(source: &SourceDef) -> Result<()> {
    match source {
        SourceDef::Cli { command, .. } => {
            if command.is_empty() {
                anyhow::bail!("'source.command' cannot be empty");
            }
        }
        SourceDef::Http {
            method,
            url,
            auth,
            headers,
            body,
            ..
        } => {
            if url.is_empty() {
                anyhow::bail!("'source.url' cannot be empty");
            }
            if *method == HttpMethodDef::Post && body.as_ref().is_none_or(|body| body.is_empty()) {
                anyhow::bail!("'source.body' cannot be empty for HTTP POST");
            }
            validate_auth_header(auth)?;
            validate_headers(headers)?;
        }
        SourceDef::Placeholder { reason } => {
            if reason.is_empty() {
                anyhow::bail!("'source.reason' cannot be empty for placeholder provider");
            }
        }
    }
    Ok(())
}

fn validate_auth_header(auth: &Option<AuthDef>) -> Result<()> {
    if let Some(AuthDef::HeaderEnv { header, .. }) = auth {
        validate_header_name(header, "source.auth.header")?;
    }
    Ok(())
}

fn validate_headers(headers: &[HeaderDef]) -> Result<()> {
    for header in headers {
        validate_header_name(&header.name, "source.headers[].name")?;
        ureq::http::HeaderValue::try_from(header.value.as_str()).map_err(|err| {
            anyhow::anyhow!("invalid header value for '{}': {}", header.name, err)
        })?;
    }
    Ok(())
}

fn validate_header_name(name: &str, field: &str) -> Result<()> {
    ureq::http::HeaderName::try_from(name)
        .map(|_| ())
        .map_err(|err| anyhow::anyhow!("invalid header name '{}' in {}: {}", name, field, err))
}

fn validate_parser(parser: &Option<ParserDef>) -> Result<()> {
    let Some(parser) = parser else {
        // parser 可以为 None（placeholder source）
        return Ok(());
    };
    match parser {
        ParserDef::Json { quotas, .. } => {
            if quotas.is_empty() {
                anyhow::bail!("'parser.quotas' must contain at least one rule");
            }
            for rule in quotas {
                // 校验模式互斥：remaining 模式 vs 传统 used+limit 模式
                let has_remaining = rule.remaining.as_ref().is_some_and(|s| !s.is_empty());
                let has_limit = rule.limit.as_ref().is_some_and(|s| !s.is_empty());
                let has_used = rule.used.as_ref().is_some_and(|s| !s.is_empty());

                if has_remaining && has_limit {
                    anyhow::bail!(
                        "quota rule '{}': 'remaining' and 'limit' are mutually exclusive",
                        rule.label
                    );
                }
                if !has_remaining && !has_limit {
                    anyhow::bail!(
                        "quota rule '{}': must specify either 'remaining' or 'used'+'limit'",
                        rule.label
                    );
                }
                if has_limit && !has_used {
                    anyhow::bail!(
                        "quota rule '{}': 'used' is required when 'limit' is specified",
                        rule.label
                    );
                }

                validate_divisor(&rule.label, rule.divisor)?;
            }
        }
        ParserDef::Regex { quotas, .. } => {
            if quotas.is_empty() {
                anyhow::bail!("'parser.quotas' must contain at least one rule");
            }
            for rule in quotas {
                validate_regex_rule(rule)?;
                validate_divisor(&rule.label, rule.divisor)?;
            }
        }
    }
    Ok(())
}

fn validate_regex_rule(rule: &RegexQuotaRule) -> Result<()> {
    let re = Regex::new(&rule.pattern).map_err(|e| {
        anyhow::anyhow!(
            "quota rule '{}': invalid regex '{}': {}",
            rule.label,
            rule.pattern,
            e
        )
    })?;

    let capture_count = re.captures_len() - 1; // group 0 是整个匹配
    if rule.used_group > capture_count {
        anyhow::bail!(
            "quota rule '{}': used_group {} exceeds capture groups ({})",
            rule.label,
            rule.used_group,
            capture_count
        );
    }
    if rule.limit_group > capture_count {
        anyhow::bail!(
            "quota rule '{}': limit_group {} exceeds capture groups ({})",
            rule.label,
            rule.limit_group,
            capture_count
        );
    }

    Ok(())
}

/// 校验 divisor 必须为正数
fn validate_divisor(label: &str, divisor: Option<f64>) -> Result<()> {
    if let Some(d) = divisor {
        if d <= 0.0 {
            anyhow::bail!(
                "quota rule '{}': divisor must be positive, got {}",
                label,
                d
            );
        }
    }
    Ok(())
}
