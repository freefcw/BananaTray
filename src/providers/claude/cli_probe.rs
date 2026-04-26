//! Claude CLI Probe
//!
//! 通过执行 `claude /usage` 命令获取配额信息。

use super::probe::UsageProbe;
use crate::models::{FailureAdvice, QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::providers::common::runner::{InteractiveOptions, InteractiveRunner};
use crate::providers::{ProviderError, ProviderResult};
use crate::utils::text_utils;
use log::debug;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

// 预编译的正则表达式
static PCT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+)%\s+(left|used)").unwrap());
static RESET_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^Resets?\s+(.+)").unwrap());
static COST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$([0-9,]+\.?\d*)\s*/\s*\$([0-9,]+\.?\d*)").unwrap());
static MODEL_NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(([^)]+)\)").unwrap());

/// Claude CLI 获取方式
pub struct ClaudeCliProbe;

impl ClaudeCliProbe {
    /// 获取用于探测的工作目录（专用信任目录）
    fn probe_working_directory() -> PathBuf {
        let base = dirs::cache_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join("bananatray").join("claude-probe");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// 获取自动应答映射
    fn auto_responses() -> HashMap<String, String> {
        let mut map = HashMap::new();
        // 信任提示 - 发送 Enter 确认
        map.insert("Esc to cancel".to_string(), "\r".to_string());
        map.insert("Ready to code here?".to_string(), "\r".to_string());
        map.insert("Press Enter to continue".to_string(), "\r".to_string());
        map.insert("ctrl+t to disable".to_string(), "\r".to_string());
        map.insert("Yes, I trust this folder".to_string(), "\r".to_string());
        map.insert(
            "Do you trust the files in this folder?".to_string(),
            "y\r".to_string(),
        );
        // /usage 命令面板操作
        map.insert("Show plan".to_string(), "\r".to_string());
        map.insert("Show plan usage limits".to_string(), "\r".to_string());
        map
    }

    /// 解析 `claude /usage` 输出
    fn parse_usage_output(raw: &str) -> ProviderResult<Vec<QuotaInfo>> {
        let clean = text_utils::strip_ansi(raw);

        // 按空行分割段落
        let sections = Self::split_sections(&clean);

        let mut quotas = Vec::new();

        for section in &sections {
            let lines: Vec<&str> = section.lines().map(|l| l.trim()).collect();
            if lines.is_empty() {
                continue;
            }

            // 第一个非空行是段落标题
            let header = lines[0];
            let header_lower = header.to_lowercase();

            // 确定配额类型和显示标签
            let (quota_type, label) = if header_lower.contains("extra usage") {
                (QuotaType::Credit, QuotaLabelSpec::ExtraUsage)
            } else if header_lower.contains("session") {
                (QuotaType::Session, QuotaLabelSpec::Session)
            } else if header_lower.contains("week") {
                if let Some(model) = Self::extract_model_name(header) {
                    (
                        QuotaType::ModelSpecific(model.clone()),
                        QuotaLabelSpec::WeeklyModel { model },
                    )
                } else {
                    (QuotaType::Weekly, QuotaLabelSpec::Weekly)
                }
            } else {
                continue;
            };

            let section_text = lines.join("\n");

            // 提取重置时间（CLI 直接输出，不经过 format_countdown，需手动加 ⏱ 前缀）
            let reset_at = lines.iter().find_map(|line| {
                RESET_RE
                    .captures(line)
                    .map(|caps| QuotaDetailSpec::ResetDate {
                        date: caps[1].trim().to_string(),
                    })
            });

            // 对于 Credit 类型，优先尝试提取美元金额（可能没有百分比）
            if quota_type == QuotaType::Credit {
                if let Some(caps) = COST_RE.captures(&section_text) {
                    let spent: f64 = caps[1].replace(',', "").parse().unwrap_or(0.0);
                    let budget: f64 = caps[2].replace(',', "").parse().unwrap_or(0.0);
                    quotas.push(QuotaInfo::with_details(
                        label, spent, budget, quota_type, reset_at,
                    ));
                    continue;
                }
                // 如果没有匹配到金额，继续尝试百分比解析
            }

            // 提取百分比（非 Credit 类型或 Credit 类型没有金额时）
            let (used_pct, _percent_left) = if let Some(caps) = PCT_RE.captures(&section_text) {
                let value: f64 = caps[1].parse().unwrap_or(0.0);
                let direction = &caps[2];
                if direction == "used" {
                    (value, 100.0 - value)
                } else {
                    // "left"
                    (100.0 - value, value)
                }
            } else {
                continue;
            };

            // 基于百分比的配额
            quotas.push(QuotaInfo::with_details(
                label, used_pct, 100.0, quota_type, reset_at,
            ));
        }

        Ok(quotas)
    }

    /// 将清理后的文本按空行分割为段落，跳过版本标题行
    fn split_sections(text: &str) -> Vec<String> {
        let mut sections = Vec::new();
        let mut current = String::new();

        for line in text.lines() {
            let trimmed = line.trim();
            // 跳过版本标题（如 "Claude Code v1.0.27"）
            if trimmed.starts_with("Claude Code") {
                continue;
            }
            if trimmed.is_empty() {
                if !current.trim().is_empty() {
                    sections.push(current.clone());
                }
                current.clear();
            } else {
                if !current.is_empty() {
                    current.push('\n');
                }
                current.push_str(trimmed);
            }
        }
        if !current.trim().is_empty() {
            sections.push(current);
        }

        sections
    }

    /// 从标题中提取模型名称（如 "Current week (Opus)"）
    fn extract_model_name(header: &str) -> Option<String> {
        let caps = MODEL_NAME_RE.captures(header)?;
        let name = caps[1].trim().to_string();
        let lower = name.to_lowercase();
        // "all models" 是汇总周配额，不是特定模型
        if lower == "all models" {
            None
        } else {
            Some(name)
        }
    }
}

impl UsageProbe for ClaudeCliProbe {
    fn probe(&self) -> ProviderResult<Vec<QuotaInfo>> {
        let runner = InteractiveRunner::new();
        let options = InteractiveOptions {
            timeout: Duration::from_secs(25),
            idle_timeout: Duration::from_secs(4),
            working_directory: Some(Self::probe_working_directory()),
            arguments: vec!["--allowed-tools".to_string(), "".to_string()],
            auto_responses: Self::auto_responses(),
            environment_exclusions: vec!["CLAUDE_CODE_OAUTH_TOKEN".to_string()],
            send_enter_every: Some(Duration::from_millis(500)), // 周期性发送 Enter 以渲染 /usage
            ..Default::default()
        };

        let result = runner
            .run("claude", "/usage", options)
            .map_err(|err| ProviderError::classify(&err))?;

        debug!(target: "providers", "claude command completed, output length: {} bytes", result.output.len());

        // 检查错误条件
        let output_lower = result.output.to_lowercase();

        if output_lower.contains("not logged in") || output_lower.contains("authentication") {
            return Err(ProviderError::auth_required(Some(
                FailureAdvice::LoginCli {
                    cli: "claude".to_string(),
                },
            )));
        }
        if output_lower.contains("update") && output_lower.contains("required") {
            return Err(ProviderError::update_required(None));
        }

        // 解析配额
        let quotas = Self::parse_usage_output(&result.output)?;

        if quotas.is_empty() {
            // 检查特定问题
            if output_lower.contains("not logged in") || output_lower.contains("authentication") {
                return Err(ProviderError::auth_required(Some(
                    FailureAdvice::LoginCli {
                        cli: "claude".to_string(),
                    },
                )));
            }
            if output_lower.contains("update") && output_lower.contains("required") {
                return Err(ProviderError::update_required(None));
            }
            // 检查信任提示是否阻塞
            if output_lower.contains("trust the files") && !output_lower.contains("current session")
            {
                return Err(ProviderError::unavailable_with_advice(
                    FailureAdvice::TrustFolder {
                        cli: "claude".to_string(),
                    },
                ));
            }
            return Err(ProviderError::parse_failed_with_advice(
                FailureAdvice::CannotParseQuota,
            ));
        }

        Ok(quotas)
    }

    fn is_available(&self) -> bool {
        which::which("claude").is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_sections() {
        let output = r#"
Extra usage
$5.00 / $20.00
"#;
        let sections = ClaudeCliProbe::split_sections(output);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].contains("Extra usage"));
    }

    #[test]
    fn test_cost_regex() {
        let text = "$5.00 / $20.00";
        if let Some(caps) = COST_RE.captures(text) {
            let spent: f64 = caps[1].replace(',', "").parse().unwrap_or(0.0);
            let budget: f64 = caps[2].replace(',', "").parse().unwrap_or(0.0);
            assert_eq!(spent, 5.0);
            assert_eq!(budget, 20.0);
        } else {
            panic!("COST_RE did not match");
        }
    }

    #[test]
    fn test_parse_session_quota() {
        let output = r#"
Claude Code v1.0.27

Current session
45% used
Resets in 2h 15m

Current week
30% left
"#;
        let quotas = ClaudeCliProbe::parse_usage_output(output).unwrap();
        assert_eq!(quotas.len(), 2);

        // Session
        assert_eq!(quotas[0].label_spec, QuotaLabelSpec::Session);
        assert_eq!(quotas[0].used, 45.0);
        assert_eq!(quotas[0].quota_type, QuotaType::Session);

        // Weekly
        assert_eq!(quotas[1].label_spec, QuotaLabelSpec::Weekly);
        assert_eq!(quotas[1].used, 70.0); // 30% left = 70% used
    }

    #[test]
    fn test_parse_model_specific_quota() {
        let output = r#"
Current week (Opus)
60% used

Current week (Sonnet)
20% left
"#;
        let quotas = ClaudeCliProbe::parse_usage_output(output).unwrap();
        assert_eq!(quotas.len(), 2);

        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::WeeklyModel {
                model: "Opus".to_string()
            }
        );
        assert_eq!(
            quotas[0].quota_type,
            QuotaType::ModelSpecific("Opus".to_string())
        );

        assert_eq!(
            quotas[1].label_spec,
            QuotaLabelSpec::WeeklyModel {
                model: "Sonnet".to_string()
            }
        );
        assert_eq!(
            quotas[1].quota_type,
            QuotaType::ModelSpecific("Sonnet".to_string())
        );
    }

    #[test]
    fn test_parse_credit_quota() {
        let output = r#"
Extra usage
$5.00 / $20.00
"#;
        let quotas = ClaudeCliProbe::parse_usage_output(output).unwrap();
        assert_eq!(quotas.len(), 1);

        assert_eq!(quotas[0].label_spec, QuotaLabelSpec::ExtraUsage);
        assert_eq!(quotas[0].used, 5.0);
        assert_eq!(quotas[0].limit, 20.0);
        assert_eq!(quotas[0].quota_type, QuotaType::Credit);
    }
}
