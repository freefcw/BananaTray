//! Codex CLI fallback：解析 `codex /status` 输出。
//!
//! 与 CodexBar `CodexStatusProbe` 行为一致的简化版：
//! - 通过 PTY 启动 `codex -s read-only -a untrusted`，发送 `/status\r`（InteractiveRunner
//!   会在 input 末尾统一 append `\r`），捕获输出
//! - 抽取 `Credits:`、`5h limit ... N% left`、`Weekly limit ... N% left`
//! - 把 "% left" 翻译为 BananaTray 内部的 "% used"
//!
//! 设计上把"PTY 调用"与"纯文本解析"拆开：[`parse`] 只接收字符串，便于单元
//! 测试覆盖各种 codex 版本的输出形态；[`fetch_via_cli`] 才负责 I/O。
//!
//! 注意：本 fallback 不抽取 `reset_at` 时间戳。CodexBar 的 reset 解析依赖运行时
//! 时区与"明日 anchor"逻辑，端到端复现成本不低；OAuth 已能给精确 epoch_secs，
//! CLI fallback 退而其次只给百分比，是合理降级。

use crate::models::{QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::providers::common::{
    path_resolver,
    runner::{InteractiveOptions, InteractiveRunner},
};
use crate::providers::{ProviderError, ProviderResult};
use crate::utils::text_utils;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use super::parser::ParsedUsage;

const CODEX_BINARY: &str = "codex";
const STATUS_INPUT: &str = "/status";
/// 整体超时：codex 在 PTY 内 spawn 后通常 2-4s 内输出 /status；留 12s 给慢机器。
const STATUS_TIMEOUT: Duration = Duration::from_secs(12);
/// idle 超时：连续 2s 无新数据视为输出已完成。
const STATUS_IDLE: Duration = Duration::from_secs(2);
/// init 延迟：codex 启动渲染 TUI 需要时间，过早发 `/status` 会被吞。
const STATUS_INIT_DELAY: Duration = Duration::from_millis(800);

static CREDITS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)Credits:\s*([0-9][0-9.,]*)").unwrap());
static PERCENT_LEFT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([0-9]{1,3})%\s+left").unwrap());

/// I/O 入口：通过 PTY 启动 codex CLI 并解析 `/status` 输出。
///
/// 入口先做可执行文件存在性检查来快速失败：避免每次 OAuth 失败都白费数秒走
/// PTY spawn 流程。使用与真正 spawn 相同的解析函数（`locate_executable`），
/// 以免 macOS GUI 启动（PATH 不含 Homebrew 前缀）下 `which` 失败却其实 runner
/// 能在 `/opt/homebrew/bin` 等兜底目录里找到 codex，造成伪 CliNotFound。
pub(super) fn fetch_via_cli() -> ProviderResult<ParsedUsage> {
    ensure_cli_present(CODEX_BINARY)?;

    let runner = InteractiveRunner::new();
    let options = InteractiveOptions {
        timeout: STATUS_TIMEOUT,
        idle_timeout: STATUS_IDLE,
        // 独立探测目录：避免 GUI tray 启动 cwd（可能是 /、~/Desktop 等任意位置）引发
        // codex 对未知目录的不可预期行为；与 Claude probe 同样的模式。
        working_directory: Some(probe_working_directory()),
        // -s read-only：只读模式，禁止写盘
        // -a untrusted：声明当前目录非可信，避免 trust prompt
        arguments: vec![
            "-s".to_string(),
            "read-only".to_string(),
            "-a".to_string(),
            "untrusted".to_string(),
        ],
        // 应对 codex 未来可能弹出的常见 prompt，避免 PTY 卡到 idle timeout。
        // 列出的都是从 Claude / 一般 CLI 习惯中归纳的“抢狭”型 prompt；却未在 codex 现版本
        // 中观察到，属于防御性。避免以 `/` 开头的 prompt（则会跳 codex 自己的类 slash 命令）。
        auto_responses: default_auto_responses(),
        init_delay: STATUS_INIT_DELAY,
        ..InteractiveOptions::default()
    };
    let result = runner
        .run(CODEX_BINARY, STATUS_INPUT, options)
        .map_err(|err| ProviderError::classify(&err))?;
    parse(&result.output)
}

/// 快速检查给定 binary 是否可被定位。使用与 `InteractiveRunner::run` 相同的
/// `path_resolver::locate_executable` 规则（PATH + 常见用户/Homebrew/usr 兜底），
/// 确保短路判定与真实 spawn 能力一致。
fn ensure_cli_present(binary: &str) -> ProviderResult<()> {
    if path_resolver::locate_executable(binary).is_none() {
        return Err(ProviderError::cli_not_found(binary));
    }
    Ok(())
}

/// 独立探测目录：与 Claude probe 使用同一个父目录（`~/.cache/bananatray/`）。
/// 跳过失败（目录创建不上）也不影响：PTY runner 会回退到父进程的 cwd。
fn probe_working_directory() -> PathBuf {
    let base = dirs::cache_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("bananatray").join("codex-probe");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 默认 prompt 自动应答：覆盖常见交互 prompt，均发送 Enter。
///
/// 不包含 codex 自己的 slash 命令（如 `/status`）作为 key，避免递归触发。
fn default_auto_responses() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("Press Enter to continue".to_string(), "\r".to_string());
    map.insert("Press any key".to_string(), "\r".to_string());
    map.insert("Esc to cancel".to_string(), "\r".to_string());
    map
}

/// 纯函数：从 `codex /status` 文本中提取配额。
///
/// 本函数对调用者公开以便单测直接传入原始文本：即使 runner 出口已 strip 一次
/// ANSI，这里还是再 strip 一次。`strip_ansi` 是幂等的，运行时不会被调两次以上。
pub(super) fn parse(raw: &str) -> ProviderResult<ParsedUsage> {
    let clean = text_utils::strip_ansi(raw);

    if clean.to_lowercase().contains("data not available yet") {
        return Err(ProviderError::no_data());
    }

    let credits_balance = extract_credits(&clean);
    let session_left = extract_percent_left_in_lane(&clean, "5h limit");
    let weekly_left = extract_percent_left_in_lane(&clean, "Weekly limit");

    let mut quotas = Vec::new();
    if let Some(left) = session_left {
        quotas.push(quota_from_left(
            QuotaLabelSpec::Session,
            QuotaType::Session,
            left,
        ));
    }
    if let Some(left) = weekly_left {
        quotas.push(quota_from_left(
            QuotaLabelSpec::Weekly,
            QuotaType::Weekly,
            left,
        ));
    }
    if let Some(balance) = credits_balance {
        quotas.push(QuotaInfo::balance_only(
            QuotaLabelSpec::Credits,
            balance,
            None,
            QuotaType::Credit,
            None,
        ));
    }

    if quotas.is_empty() {
        return Err(ProviderError::parse_failed("codex /status output"));
    }

    Ok(ParsedUsage {
        quotas,
        plan_type: None,
    })
}

/// `% left` → `% used` 的对称转换，clamp 到 [0, 100]。
fn quota_from_left(label: QuotaLabelSpec, quota_type: QuotaType, left_percent: u32) -> QuotaInfo {
    let used = (100.0 - left_percent as f64).clamp(0.0, 100.0);
    QuotaInfo::with_details(label, used, 100.0, quota_type, None)
}

/// 在文本中找到包含 `lane_label` 的第一行，再从该行抽取 `N% left`。
///
/// 按行而非全文匹配是为了避免把 weekly 行的百分比错误地归给 session lane
/// （codex 的两个 lane 行格式完全相同，仅靠 lane 标签区分）。
fn extract_percent_left_in_lane(text: &str, lane_label: &str) -> Option<u32> {
    let lane_lower = lane_label.to_lowercase();
    for line in text.lines() {
        if !line.to_lowercase().contains(&lane_lower) {
            continue;
        }
        if let Some(caps) = PERCENT_LEFT_RE.captures(line) {
            if let Some(m) = caps.get(1) {
                if let Ok(n) = m.as_str().parse::<u32>() {
                    return Some(n.min(100));
                }
            }
        }
    }
    None
}

fn extract_credits(text: &str) -> Option<f64> {
    let caps = CREDITS_RE.captures(text)?;
    parse_loose_number(caps.get(1)?.as_str())
}

/// 宽松数字解析：先按"原值 / 去逗号"两步尝试。
///
/// **已知局限**：不识别欧式 `1.234,56` 或混合格式。
/// 当前可接受的依据：
/// - codex CLI 的 `Credits:` 输出由 OpenAI 服务端控制，不随用户 locale 变化
/// - CodexBar `StatusProbeTests` 的样本均为 ASCII 整数 / `12.5` 等美式表示
/// - CodexBar 自己的 `parseNumber` 是为多种 dashboard 文本写的更通用版本，对此场景过度
///
/// 如果将来真观察到欧式数字，可参考 `bars/CodexBar/.../TextParsing.swift::parseNumber`
/// 引入"数千分位 / 小数最右"启发式判别。
fn parse_loose_number(raw: &str) -> Option<f64> {
    let cleaned: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if let Ok(v) = cleaned.parse::<f64>() {
        return Some(v);
    }
    cleaned.replace(',', "").parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_quota(q: &QuotaInfo, expected_used: f64, expected_type: QuotaType) {
        assert_eq!(q.quota_type, expected_type, "quota type mismatch");
        assert!(
            (q.used - expected_used).abs() < f64::EPSILON,
            "used {} != expected {}",
            q.used,
            expected_used
        );
    }

    // ────────────────────────────────────────────────────────────────────────
    // parse：与 CodexBar StatusProbeTests 等价的最小回归集
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn parse_basic_5h_and_weekly_with_credits() {
        let _g = crate::i18n::test_locale_guard("en");
        let sample = "\
Model: gpt
Credits: 980 credits
5h limit: [#####] 75% left
Weekly limit: [##] 25% left
";
        let parsed = parse(sample).unwrap();
        assert_eq!(parsed.quotas.len(), 3);
        // session: 100 - 75 = 25
        assert_quota(&parsed.quotas[0], 25.0, QuotaType::Session);
        // weekly: 100 - 25 = 75
        assert_quota(&parsed.quotas[1], 75.0, QuotaType::Weekly);
        let credits = parsed
            .quotas
            .iter()
            .find(|q| q.is_credit())
            .expect("credits");
        assert!(credits.is_balance_only());
        assert!((credits.remaining_balance.unwrap() - 980.0).abs() < f64::EPSILON);
        assert!(parsed.plan_type.is_none());
    }

    #[test]
    fn parse_with_ansi_and_reset_descriptions() {
        let _g = crate::i18n::test_locale_guard("en");
        let sample = "\
\x1b[38;5;245mCredits:\x1b[0m 557 credits
5h limit: [█████     ] 50% left (resets 09:01)
Weekly limit: [███████   ] 85% left (resets 04:01 on 27 Nov)
";
        let parsed = parse(sample).unwrap();
        assert_eq!(parsed.quotas.len(), 3);
        assert_quota(&parsed.quotas[0], 50.0, QuotaType::Session);
        assert_quota(&parsed.quotas[1], 15.0, QuotaType::Weekly);
        let credits = parsed
            .quotas
            .iter()
            .find(|q| q.is_credit())
            .expect("credits");
        assert!((credits.remaining_balance.unwrap() - 557.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_weekly_only_lane() {
        let _g = crate::i18n::test_locale_guard("en");
        let sample = "\
Model: gpt
Credits: 980 credits
Weekly limit: [##] 25% left
";
        let parsed = parse(sample).unwrap();
        // session 缺失，只有 weekly + credits
        assert!(parsed
            .quotas
            .iter()
            .all(|q| q.quota_type != QuotaType::Session));
        assert_quota(
            parsed
                .quotas
                .iter()
                .find(|q| q.quota_type == QuotaType::Weekly)
                .unwrap(),
            75.0,
            QuotaType::Weekly,
        );
    }

    #[test]
    fn parse_5h_only_lane() {
        let _g = crate::i18n::test_locale_guard("en");
        let sample = "5h limit: [#####] 60% left";
        let parsed = parse(sample).unwrap();
        assert_eq!(parsed.quotas.len(), 1);
        assert_quota(&parsed.quotas[0], 40.0, QuotaType::Session);
    }

    #[test]
    fn parse_data_not_available_returns_no_data() {
        let _g = crate::i18n::test_locale_guard("en");
        let sample = "Model: gpt\nData not available yet, please try again later.";
        let err = parse(sample).unwrap_err();
        assert!(matches!(err, ProviderError::NoData));
    }

    #[test]
    fn parse_garbage_returns_parse_failed() {
        let _g = crate::i18n::test_locale_guard("en");
        let err = parse("totally unrelated terminal noise").unwrap_err();
        assert!(matches!(err, ProviderError::ParseFailed { .. }));
    }

    #[test]
    fn parse_empty_input_returns_parse_failed() {
        let _g = crate::i18n::test_locale_guard("en");
        let err = parse("").unwrap_err();
        assert!(matches!(err, ProviderError::ParseFailed { .. }));
    }

    #[test]
    fn parse_credits_only_without_lanes() {
        // 防御性覆盖：未在 codex CLI 现版本中观察到仅有 Credits 行的输出，
        // 但如果上游未来出现部分输出丢失限额行的场景，至少 credits 还能走到。
        let _g = crate::i18n::test_locale_guard("en");
        let sample = "Credits: 50 credits";
        let parsed = parse(sample).unwrap();
        assert_eq!(parsed.quotas.len(), 1);
        let q = &parsed.quotas[0];
        assert_eq!(q.quota_type, QuotaType::Credit);
        assert!(q.is_balance_only());
        assert!((q.remaining_balance.unwrap() - 50.0).abs() < f64::EPSILON);
    }

    // ────────────────────────────────────────────────────────────────────────
    // 子函数边界
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn extract_percent_left_picks_correct_lane_when_both_present() {
        let text = "\
5h limit: [###] 30% left
Weekly limit: [#######] 70% left
";
        assert_eq!(
            extract_percent_left_in_lane(text, "5h limit"),
            Some(30),
            "session lane"
        );
        assert_eq!(
            extract_percent_left_in_lane(text, "Weekly limit"),
            Some(70),
            "weekly lane"
        );
    }

    #[test]
    fn extract_percent_left_returns_none_when_lane_missing() {
        let text = "5h limit: [#] 90% left";
        assert!(extract_percent_left_in_lane(text, "Weekly limit").is_none());
    }

    #[test]
    fn extract_percent_left_clamps_to_100() {
        let text = "5h limit: [#] 250% left";
        assert_eq!(extract_percent_left_in_lane(text, "5h limit"), Some(100));
    }

    #[test]
    fn extract_percent_left_is_case_insensitive_for_label() {
        let text = "WEEKLY LIMIT: [#] 42% left";
        assert_eq!(extract_percent_left_in_lane(text, "Weekly limit"), Some(42));
    }

    #[test]
    fn extract_credits_handles_thousand_separator() {
        // 不严格区分欧美格式：先按整体 parse，失败时去逗号兜底。
        assert_eq!(extract_credits("Credits: 1,234 credits"), Some(1234.0));
        assert_eq!(extract_credits("Credits: 980"), Some(980.0));
        assert_eq!(extract_credits("Credits: 12.5"), Some(12.5));
    }

    #[test]
    fn extract_credits_returns_none_when_absent() {
        assert!(extract_credits("Model: gpt-5").is_none());
    }

    #[test]
    fn extract_credits_is_case_insensitive() {
        assert_eq!(extract_credits("credits: 42"), Some(42.0));
        assert_eq!(extract_credits("CREDITS: 100"), Some(100.0));
    }

    #[test]
    fn parse_loose_number_handles_basic_cases() {
        assert_eq!(parse_loose_number("980"), Some(980.0));
        assert_eq!(parse_loose_number("12.5"), Some(12.5));
        assert_eq!(parse_loose_number("1,234"), Some(1234.0));
        assert!(parse_loose_number("not-a-number").is_none());
    }

    #[test]
    fn quota_from_left_round_trip() {
        let q = quota_from_left(QuotaLabelSpec::Session, QuotaType::Session, 25);
        assert_quota(&q, 75.0, QuotaType::Session);
        assert_eq!(q.limit, 100.0);
    }

    // ─────────────────────────────────────────────────────────────────────
    // ensure_cli_present：验证短路逻辑走 ProviderError::CliNotFound
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn ensure_cli_present_returns_cli_not_found_for_missing_binary() {
        // 不存在的二进制名：加上随机后缀避免偶发。
        let bogus = "bananatray_definitely_not_a_binary_xyz_91237";
        let err = ensure_cli_present(bogus).unwrap_err();
        match err {
            ProviderError::CliNotFound { cli_name } => assert_eq!(cli_name, bogus),
            other => panic!("expected CliNotFound, got {other:?}"),
        }
    }

    #[test]
    fn ensure_cli_present_succeeds_for_known_system_binary() {
        // 选一个在 macOS / Linux 的 PATH 上几乎必定存在的二进制，验证成功路径。
        // 若 PATH 异常、sh 不存在，这个测试会被跳过 (软断言)，避免误判定。
        if which::which("sh").is_err() {
            return;
        }
        ensure_cli_present("sh").expect("sh should be present on PATH");
    }

    // fallback（PATH 之外的 `/opt/homebrew/bin` 等目录）的正确性由 runner 层
    // `locate_in_dirs_*` 单测覆盖，这里只负责把 `ensure_cli_present` 的
    // 成功/失败语义跑通。
}
