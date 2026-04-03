use super::provider::{ProviderKind, ProviderMetadata};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// 元数据代理方法生成宏：保持 `provider.display_name()` 等 API 不变，
/// 消除手写代理的样板代码。新增 ProviderMetadata 字段时只需加一行。
macro_rules! delegate_metadata {
    ($($method:ident -> $field:ident),* $(,)?) => {
        $(pub fn $method(&self) -> &str { &self.metadata.$field })*
    };
}

// ============================================================================
// 配额类型
// ============================================================================

/// 配额类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuotaType {
    /// 5h 滑动窗口会话配额
    Session,
    /// 周配额（所有模型合计）
    Weekly,
    /// 按模型的周配额（如 Opus / Sonnet）
    ModelSpecific(String),
    /// 基于金额的信用额度
    Credit,
    /// 通用/不确定类型
    General,
}

// ============================================================================
// 用量状态等级
// ============================================================================

/// 用量状态等级（用于颜色编码）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Green,
    Yellow,
    Red,
}

impl StatusLevel {
    /// 数值严重程度，用于排序比较
    fn severity(self) -> u8 {
        match self {
            Self::Green => 0,
            Self::Yellow => 1,
            Self::Red => 2,
        }
    }
}

impl PartialOrd for StatusLevel {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StatusLevel {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.severity().cmp(&other.severity())
    }
}

// ============================================================================
// 用量信息
// ============================================================================

/// 用量配额信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaInfo {
    /// 已使用量
    pub used: f64,
    /// 总配额
    pub limit: f64,
    /// 配额类型标签（如 "Session (5h)", "Weekly", "Pro"）
    pub label: String,
    /// 配额类型
    #[serde(default = "default_quota_type")]
    pub quota_type: QuotaType,
    /// 重置时间描述（如 "Resets in 2h 15m"）
    pub reset_at: Option<String>,
}

fn default_quota_type() -> QuotaType {
    QuotaType::General
}

impl QuotaInfo {
    pub fn new(label: impl Into<String>, used: f64, limit: f64) -> Self {
        Self {
            used,
            limit,
            label: label.into(),
            quota_type: QuotaType::General,
            reset_at: None,
        }
    }

    /// 创建带完整信息的配额
    pub fn with_details(
        label: impl Into<String>,
        used: f64,
        limit: f64,
        quota_type: QuotaType,
        reset_at: Option<String>,
    ) -> Self {
        Self {
            used,
            limit,
            label: label.into(),
            quota_type,
            reset_at,
        }
    }

    /// 使用百分比 (可负数，当超出配额时)
    pub fn percentage(&self) -> f64 {
        if self.limit <= 0.0 {
            return 0.0;
        }
        // 不 clamp，允许负数（超出配额的情况）
        self.used / self.limit * 100.0
    }

    /// 剩余百分比 (可负数，当超出配额时)
    pub fn percent_remaining(&self) -> f64 {
        if self.limit <= 0.0 {
            return 0.0;
        }
        // 不 clamp，允许负数（超出配额的情况）
        (self.limit - self.used) / self.limit * 100.0
    }

    /// 是否是纯百分比模式（limit == 100.0，数据本身就是百分比）
    #[allow(dead_code)]
    pub fn is_percentage_mode(&self) -> bool {
        (self.limit - 100.0).abs() < f64::EPSILON
    }

    // ========================================================================
    // 状态判断（基于 status_level 单一真理来源）
    // ========================================================================

    /// 状态等级：Green / Yellow / Red
    ///
    /// 阈值定义：
    /// - Green: 剩余 > 50%
    /// - Yellow: 剩余 20% ~ 50%（包含边界）
    /// - Red: 剩余 < 20%
    pub fn status_level(&self) -> StatusLevel {
        let remaining_pct = self.percent_remaining();

        if remaining_pct > 50.0 {
            StatusLevel::Green
        } else if remaining_pct >= 20.0 {
            StatusLevel::Yellow
        } else {
            StatusLevel::Red
        }
    }

    /// 是否已耗尽（已使用 >= 配额）
    pub fn is_depleted(&self) -> bool {
        self.used >= self.limit && self.limit > 0.0
    }

    /// 是否健康（Green 状态）
    pub fn is_healthy(&self) -> bool {
        self.status_level() == StatusLevel::Green
    }

    /// 是否需要警告（Yellow 状态）
    pub fn is_warning(&self) -> bool {
        self.status_level() == StatusLevel::Yellow
    }

    /// 是否紧急（Red 状态且未耗尽）
    pub fn is_critical(&self) -> bool {
        self.status_level() == StatusLevel::Red && !self.is_depleted()
    }

    // ========================================================================
    // 类型判断
    // ========================================================================

    /// 按配额类型查找会话配额
    pub fn is_session(&self) -> bool {
        self.quota_type == QuotaType::Session
    }

    /// 按配额类型查找周配额
    pub fn is_weekly(&self) -> bool {
        self.quota_type == QuotaType::Weekly
    }

    /// 按配额类型查找信用配额
    pub fn is_credit(&self) -> bool {
        self.quota_type == QuotaType::Credit
    }

    // ========================================================================
    // 格式化输出
    // ========================================================================

    /// 剩余量摘要文本（用于 UI 主显示）
    /// - Credit 类型: "$X.XX left" 或 "$X.XX over"（负数）
    /// - 其他类型: "X% left" 或 "X% over"（负数）
    pub fn remaining_text(&self) -> String {
        match self.quota_type {
            QuotaType::Credit => {
                let remaining = self.limit - self.used;
                if remaining >= 0.0 {
                    t!("quota.credit_left", amount = format!("{:.2}", remaining)).to_string()
                } else {
                    t!("quota.credit_over", amount = format!("{:.2}", -remaining)).to_string()
                }
            }
            _ => {
                let pct = self.percent_remaining();
                if pct >= 0.0 {
                    t!("quota.pct_left", pct = format!("{:.0}", pct)).to_string()
                } else {
                    t!("quota.pct_over", pct = format!("{:.0}", -pct)).to_string()
                }
            }
        }
    }

    /// 使用详情文本（用于 UI 详细展示）
    /// - Credit 类型: "$X.XX / $Y.YY"
    /// - 其他类型: "X used / Y total" 或 "X% used"
    pub fn usage_detail_text(&self) -> String {
        match self.quota_type {
            QuotaType::Credit => t!(
                "quota.credit_detail",
                used = format!("{:.2}", self.used),
                limit = format!("{:.2}", self.limit)
            )
            .to_string(),
            _ => {
                if self.is_percentage_mode() {
                    t!("quota.pct_used", pct = format!("{:.0}", self.used)).to_string()
                } else {
                    t!(
                        "quota.count_detail",
                        used = format!("{:.0}", self.used),
                        total = format!("{:.0}", self.limit)
                    )
                    .to_string()
                }
            }
        }
    }
}

// ============================================================================
// 刷新结果数据
// ============================================================================

/// Provider 刷新返回的完整数据
#[derive(Debug, Clone)]
pub struct RefreshData {
    /// 配额信息列表
    pub quotas: Vec<QuotaInfo>,
    /// 账户邮箱（可选）
    pub account_email: Option<String>,
    /// 账户套餐等级（可选）
    pub account_tier: Option<String>,
}

impl RefreshData {
    /// 仅包含配额信息
    pub fn quotas_only(quotas: Vec<QuotaInfo>) -> Self {
        Self {
            quotas,
            account_email: None,
            account_tier: None,
        }
    }

    /// 包含完整信息
    pub fn with_account(
        quotas: Vec<QuotaInfo>,
        account_email: Option<String>,
        account_tier: Option<String>,
    ) -> Self {
        Self {
            quotas,
            account_email,
            account_tier,
        }
    }
}

// ============================================================================
// 连接状态 & Provider 状态
// ============================================================================

/// 错误类型分类（用于 UI 决定操作）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ErrorKind {
    #[default]
    Unknown,
    /// 配置缺失 → 显示"打开配置"
    ConfigMissing,
    /// 认证问题 → 显示"打开配置"
    AuthRequired,
    /// 网络问题 → 显示"重试"
    NetworkError,
}

/// 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Refreshing,
    Error,
}

/// 单个 Provider 的完整运行时状态
///
/// ## 状态转换规则
///
/// ```text
/// ┌──────────────┐
/// │ Disconnected │──mark_refreshing()──→ Refreshing
/// └──────────────┘                          │
///       ↑                              ┌────┴────┐
///  mark_unavailable()            succeeded()   failed()
///  (非 Connected 时)                 │      ┌───┴───┐
///                              Connected  有旧数据？ 无旧数据？
///                                         Connected  Error
/// ```
///
/// - `mark_refresh_failed`: 有旧配额数据 → 保持 Connected（展示陈旧数据）；
///   无旧数据 → Error（触发 UI 空状态/错误提示）
/// - `mark_unavailable`: 仅在非 Connected 时回退到 Disconnected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub kind: ProviderKind,
    /// 静态元数据（名称、图标、链接等）
    pub metadata: ProviderMetadata,
    /// 启用状态（从设置读取）
    pub enabled: bool,
    pub connection: ConnectionStatus,
    pub quotas: Vec<QuotaInfo>,
    /// 账号邮箱（可选，用于 UI 展示）
    pub account_email: Option<String>,
    /// 是否为付费版
    pub is_paid: bool,
    /// 账号层级（如 "Pro", "Max", "Free", "Business"）
    pub account_tier: Option<String>,
    /// 上次更新时间描述（仅用于错误/断连状态的静态文案）
    pub last_updated_at: Option<String>,
    /// 最近一次刷新失败时的提示文案
    pub error_message: Option<String>,
    /// 错误类型分类（用于 UI 决定操作）
    #[serde(default)]
    pub error_kind: ErrorKind,
    /// 上次成功刷新的时刻（不序列化，用于计算相对时间）
    #[serde(skip)]
    pub last_refreshed_instant: Option<Instant>,
}

impl ProviderStatus {
    pub fn new(metadata: ProviderMetadata) -> Self {
        Self {
            kind: metadata.kind,
            metadata,
            enabled: true,
            connection: ConnectionStatus::Disconnected,
            quotas: vec![],
            account_email: None,
            is_paid: false,
            account_tier: None,
            last_updated_at: None,
            error_message: None,
            error_kind: ErrorKind::default(),
            last_refreshed_instant: None,
        }
    }

    pub fn mark_refreshing(&mut self) {
        self.connection = ConnectionStatus::Refreshing;
    }

    pub fn mark_refresh_succeeded(&mut self, data: RefreshData) {
        self.quotas = data.quotas;
        self.account_email = data.account_email;
        self.account_tier = data.account_tier;
        self.connection = ConnectionStatus::Connected;
        self.last_refreshed_instant = Some(Instant::now());
        self.last_updated_at = None;
        self.error_message = None;
        self.error_kind = ErrorKind::default();
    }

    pub fn mark_unavailable(&mut self, message: String) {
        if self.connection != ConnectionStatus::Connected {
            self.connection = ConnectionStatus::Disconnected;
        }
        self.error_message = Some(message);
    }

    /// 标记刷新失败，同时设置错误类型
    pub fn mark_refresh_failed(&mut self, error: String, error_kind: ErrorKind) {
        if self.quotas.is_empty() {
            self.connection = ConnectionStatus::Error;
        } else {
            self.connection = ConnectionStatus::Connected;
        }
        self.last_updated_at = Some(t!("quota.update_failed").to_string());
        self.error_message = Some(error);
        self.error_kind = error_kind;
    }

    // 元数据代理方法（由宏生成，保持 30+ 处调用点兼容）
    delegate_metadata!(
        display_name -> display_name,
        icon_asset -> icon_asset,
        dashboard_url -> dashboard_url,
        brand_name -> brand_name,
        account_hint -> account_hint,
        source_label -> source_label,
    );

    /// 格式化上次刷新的相对时间
    pub fn format_last_updated(&self) -> String {
        if let Some(instant) = self.last_refreshed_instant {
            let secs = instant.elapsed().as_secs();
            if secs < 60 {
                t!("provider.updated_just_now").to_string()
            } else if secs < 3600 {
                t!("provider.updated_min_ago", n = secs / 60).to_string()
            } else {
                t!("provider.updated_hr_ago", n = secs / 3600).to_string()
            }
        } else if let Some(ref text) = self.last_updated_at {
            text.clone()
        } else {
            match self.connection {
                ConnectionStatus::Connected => t!("provider.waiting_for_data").to_string(),
                ConnectionStatus::Refreshing => t!("provider.status.refreshing").to_string(),
                ConnectionStatus::Error => t!("provider.needs_attention").to_string(),
                ConnectionStatus::Disconnected => t!("provider.not_connected").to_string(),
            }
        }
    }

    /// 获取最高用量的状态等级（用于总览显示）
    #[allow(dead_code)]
    pub fn worst_status(&self) -> StatusLevel {
        self.quotas
            .iter()
            .map(|q| q.status_level())
            .max()
            .unwrap_or(StatusLevel::Green)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::test_helpers::{make_test_provider, setup_test_locale as setup_locale};

    // ========================================================================
    // 基础计算测试
    // ========================================================================

    #[test]
    fn test_quota_percentage() {
        let q1 = QuotaInfo::new("test", 50.0, 100.0);
        assert_eq!(q1.percentage(), 50.0);

        let q2 = QuotaInfo::new("test", 150.0, 100.0); // 溢出
        assert_eq!(q2.percentage(), 150.0); // 不 clamp，返回实际值

        let q3 = QuotaInfo::new("test", 0.0, 0.0); // 除零
        assert_eq!(q3.percentage(), 0.0);
    }

    #[test]
    fn test_quota_percent_remaining() {
        let q1 = QuotaInfo::new("test", 30.0, 100.0);
        assert_eq!(q1.percent_remaining(), 70.0);

        let q2 = QuotaInfo::new("test", 100.0, 100.0); // 已用完
        assert_eq!(q2.percent_remaining(), 0.0);

        let q3 = QuotaInfo::new("test", 150.0, 100.0); // 超出
        assert_eq!(q3.percent_remaining(), -50.0); // 返回负数

        let q4 = QuotaInfo::new("test", 0.0, 0.0); // 除零
        assert_eq!(q4.percent_remaining(), 0.0);
    }

    #[test]
    fn test_quota_percent_remaining_precision() {
        // 测试浮点精度
        let q = QuotaInfo::new("test", 33.333, 100.0);
        assert!((q.percent_remaining() - 66.667).abs() < 0.01);
    }

    // ========================================================================
    // 状态判断测试（基于 status_level 单一真理来源）
    // ========================================================================

    #[test]
    fn test_status_level_green() {
        let q = QuotaInfo::new("green", 40.0, 100.0);
        assert_eq!(q.status_level(), StatusLevel::Green);
        assert!(q.is_healthy());
        assert!(!q.is_warning());
        assert!(!q.is_critical());
        assert!(!q.is_depleted());
    }

    #[test]
    fn test_status_level_green_boundary() {
        // 正好 50% 剩余 = Yellow（因为 > 50 才是 Green）
        let q_50_remaining = QuotaInfo::new("boundary", 50.0, 100.0);
        assert_eq!(q_50_remaining.status_level(), StatusLevel::Yellow);

        // 49.9% 剩余 = Yellow
        let q_49_9 = QuotaInfo::new("almost_green", 50.1, 100.0);
        assert_eq!(q_49_9.status_level(), StatusLevel::Yellow);

        // 50.1% 剩余 = Green
        let q_50_1 = QuotaInfo::new("just_green", 49.9, 100.0);
        assert_eq!(q_50_1.status_level(), StatusLevel::Green);
    }

    #[test]
    fn test_status_level_yellow() {
        // 50% 使用 = 50% 剩余 -> Yellow 边界
        let q_50 = QuotaInfo::new("yellow", 50.0, 100.0);
        assert_eq!(q_50.status_level(), StatusLevel::Yellow);
        assert!(!q_50.is_healthy());
        assert!(q_50.is_warning());
        assert!(!q_50.is_critical());

        // 80% 使用 = 20% 剩余 -> Yellow 边界
        let q_80 = QuotaInfo::new("yellow_edge", 80.0, 100.0);
        assert_eq!(q_80.status_level(), StatusLevel::Yellow);
        assert!(!q_80.is_critical()); // 20% 是 Yellow 边界，不是 critical
    }

    #[test]
    fn test_status_level_red() {
        // 81% 使用 = 19% 剩余 -> Red
        let q = QuotaInfo::new("red", 81.0, 100.0);
        assert_eq!(q.status_level(), StatusLevel::Red);
        assert!(!q.is_healthy());
        assert!(!q.is_warning());
        assert!(q.is_critical()); // Red 但未耗尽
        assert!(!q.is_depleted());
    }

    #[test]
    fn test_status_level_red_boundary() {
        // 正好 20% 剩余 = Yellow（因为 >= 20 是 Yellow）
        let q_20 = QuotaInfo::new("boundary", 80.0, 100.0);
        assert_eq!(q_20.status_level(), StatusLevel::Yellow);

        // 19.9% 剩余 = Red
        let q_19_9 = QuotaInfo::new("just_red", 80.1, 100.0);
        assert_eq!(q_19_9.status_level(), StatusLevel::Red);
    }

    #[test]
    fn test_depletion() {
        let q_normal = QuotaInfo::new("normal", 50.0, 100.0);
        assert!(!q_normal.is_depleted());

        let q_exact = QuotaInfo::new("exact", 100.0, 100.0);
        assert!(q_exact.is_depleted());

        let q_exceeded = QuotaInfo::new("exceeded", 150.0, 100.0);
        assert!(q_exceeded.is_depleted());

        // 耗尽时 critical 为 false（因为耗尽不是"接近耗尽"）
        assert!(!q_exact.is_critical());
        assert!(!q_exceeded.is_critical());
    }

    #[test]
    fn test_critical_vs_depleted() {
        // critical 是 Red 且未耗尽
        let q_critical = QuotaInfo::new("critical", 85.0, 100.0);
        assert!(q_critical.is_critical());
        assert!(!q_critical.is_depleted());

        // 耗尽不是 critical
        let q_depleted = QuotaInfo::new("depleted", 100.0, 100.0);
        assert!(!q_depleted.is_critical());
        assert!(q_depleted.is_depleted());
    }

    #[test]
    fn test_status_level_ordering() {
        assert!(StatusLevel::Green < StatusLevel::Yellow);
        assert!(StatusLevel::Yellow < StatusLevel::Red);
        assert_eq!(
            [StatusLevel::Red, StatusLevel::Green, StatusLevel::Yellow]
                .iter()
                .max(),
            Some(&StatusLevel::Red)
        );
    }

    // ========================================================================
    // 类型判断测试
    // ========================================================================

    #[test]
    fn test_quota_type_checks() {
        let q_session = QuotaInfo::with_details("Session", 50.0, 100.0, QuotaType::Session, None);
        assert!(q_session.is_session());
        assert!(!q_session.is_weekly());
        assert!(!q_session.is_credit());

        let q_weekly = QuotaInfo::with_details("Weekly", 50.0, 100.0, QuotaType::Weekly, None);
        assert!(q_weekly.is_weekly());

        let q_credit = QuotaInfo::with_details("Credit", 5.0, 20.0, QuotaType::Credit, None);
        assert!(q_credit.is_credit());

        let q_model = QuotaInfo::with_details(
            "Opus",
            50.0,
            100.0,
            QuotaType::ModelSpecific("Opus".into()),
            None,
        );
        assert!(!q_model.is_session());
        assert!(!q_model.is_weekly());
        assert!(!q_model.is_credit());
    }

    // ========================================================================
    // 格式化输出测试
    // ========================================================================

    #[test]
    fn test_remaining_text_percentage() {
        setup_locale();
        let q = QuotaInfo::new("test", 30.0, 100.0);
        assert_eq!(q.remaining_text(), "70% left");

        let q_depleted = QuotaInfo::new("depleted", 100.0, 100.0);
        assert_eq!(q_depleted.remaining_text(), "0% left");

        // 测试负数（超出配额）
        let q_over = QuotaInfo::new("over", 120.0, 100.0);
        assert_eq!(q_over.remaining_text(), "20% over");
    }

    #[test]
    fn test_remaining_text_credit() {
        setup_locale();
        let q = QuotaInfo::with_details("Credit", 5.0, 20.0, QuotaType::Credit, None);
        assert_eq!(q.remaining_text(), "$15.00 left");

        let q_zero = QuotaInfo::with_details("Credit", 20.0, 20.0, QuotaType::Credit, None);
        assert_eq!(q_zero.remaining_text(), "$0.00 left");

        let q_exceeded = QuotaInfo::with_details("Credit", 25.0, 20.0, QuotaType::Credit, None);
        assert_eq!(q_exceeded.remaining_text(), "$5.00 over"); // 显示超出
    }

    #[test]
    fn test_usage_detail_text_percentage() {
        setup_locale();
        let q = QuotaInfo::new("test", 30.0, 100.0);
        assert_eq!(q.usage_detail_text(), "30% used");

        let q_full = QuotaInfo::new("full", 100.0, 100.0);
        assert_eq!(q_full.usage_detail_text(), "100% used");

        // 非 percentage mode（limit != 100）
        let q_real = QuotaInfo::new("real", 30.0, 50.0);
        assert_eq!(q_real.usage_detail_text(), "30 used / 50 total");
    }

    #[test]
    fn test_usage_detail_text_credit() {
        setup_locale();
        let q = QuotaInfo::with_details("Credit", 5.0, 20.0, QuotaType::Credit, None);
        assert_eq!(q.usage_detail_text(), "$5.00 / $20.00");

        let q_zero = QuotaInfo::with_details("Credit", 0.0, 100.0, QuotaType::Credit, None);
        assert_eq!(q_zero.usage_detail_text(), "$0.00 / $100.00");
    }

    // ========================================================================
    // 边界条件测试
    // ========================================================================

    #[test]
    fn test_edge_cases() {
        // limit 为 0
        let q_zero_limit = QuotaInfo::new("zero", 10.0, 0.0);
        assert_eq!(q_zero_limit.percentage(), 0.0);
        assert_eq!(q_zero_limit.percent_remaining(), 0.0);
        assert!(!q_zero_limit.is_depleted()); // limit 为 0 时不算耗尽

        // used 和 limit 都为 0
        let q_both_zero = QuotaInfo::new("both_zero", 0.0, 0.0);
        assert_eq!(q_both_zero.percentage(), 0.0);
        assert!(!q_both_zero.is_depleted());

        // 负数 used（理论上不应该出现，但测试健壮性）
        // percent_remaining 会返回 > 100（因为剩余量是负的负数）
        let q_negative = QuotaInfo::new("negative", -10.0, 100.0);
        assert_eq!(q_negative.percentage(), -10.0); // 返回负数百分比
                                                    // 浮点数精度：使用 approx_eq
        assert!((q_negative.percent_remaining() - 110.0).abs() < 0.01); // 剩余 110%
    }

    #[test]
    fn test_percentage_mode() {
        let q_pct = QuotaInfo::new("percentage", 50.0, 100.0);
        assert!(q_pct.is_percentage_mode());

        let q_real = QuotaInfo::new("real", 5.0, 10.0);
        assert!(!q_real.is_percentage_mode());
    }

    // ========================================================================
    // format_last_updated 测试
    // ========================================================================

    fn make_provider(connection: ConnectionStatus) -> ProviderStatus {
        make_test_provider(ProviderKind::Claude, connection)
    }

    #[test]
    fn format_last_updated_no_instant_connected() {
        setup_locale();
        let p = make_provider(ConnectionStatus::Connected);
        assert_eq!(p.format_last_updated(), "Waiting for data");
    }

    #[test]
    fn format_last_updated_no_instant_refreshing() {
        setup_locale();
        let p = make_provider(ConnectionStatus::Refreshing);
        assert_eq!(p.format_last_updated(), "Refreshing…");
    }

    #[test]
    fn format_last_updated_no_instant_error() {
        setup_locale();
        let p = make_provider(ConnectionStatus::Error);
        assert_eq!(p.format_last_updated(), "Needs attention");
    }

    #[test]
    fn format_last_updated_no_instant_disconnected() {
        setup_locale();
        let p = make_provider(ConnectionStatus::Disconnected);
        assert_eq!(p.format_last_updated(), "Not connected");
    }

    #[test]
    fn format_last_updated_with_text_fallback() {
        setup_locale();
        let mut p = make_provider(ConnectionStatus::Connected);
        p.last_updated_at = Some("Custom text".to_string());
        assert_eq!(p.format_last_updated(), "Custom text");
    }

    #[test]
    fn format_last_updated_just_now() {
        setup_locale();
        let mut p = make_provider(ConnectionStatus::Connected);
        p.last_refreshed_instant = Some(std::time::Instant::now());
        assert_eq!(p.format_last_updated(), "Updated just now");
    }

    #[test]
    fn mark_refresh_failed_sets_update_text() {
        setup_locale();
        let mut p = make_provider(ConnectionStatus::Connected);
        p.mark_refresh_failed("timeout".to_string(), ErrorKind::NetworkError);
        assert_eq!(p.last_updated_at.as_deref(), Some("Update failed"));
        assert_eq!(p.error_message.as_deref(), Some("timeout"));
        assert_eq!(p.connection, ConnectionStatus::Error);
        assert_eq!(p.error_kind, ErrorKind::NetworkError);
    }
}
