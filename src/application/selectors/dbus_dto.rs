//! D-Bus JSON DTO — 用于 Rust daemon → GNOME Shell Extension 的数据传输
//!
//! 这些类型与 GPUI ViewModel 分离，是扁平的 JSON 友好结构体。
//! GJS 侧解析简单，调试方便（busctl 直接看 JSON），前向兼容。
//!
//! **放在 `application/selectors` 下而非 `dbus/` 下**：DTO 类型和格式化逻辑
//! 不依赖 GPUI / zbus，可在任何平台编译和测试。

use serde::Serialize;

use super::super::state::{AppSession, HeaderStatusKind};
use crate::models::{ConnectionStatus, ProviderId, ProviderStatus, QuotaInfo, StatusLevel};

// ============================================================================
// 顶层快照
// ============================================================================

/// 当前 D-Bus JSON 快照协议版本。
///
/// 兼容规则：同版本内允许新增可选字段；删除/改名/改类型必须提升版本。
pub const DBUS_QUOTA_SCHEMA_VERSION: u32 = 1;

/// D-Bus 传输的配额快照
#[derive(Debug, Clone, Serialize)]
pub struct DBusQuotaSnapshot {
    /// JSON 协议版本，供 Extension 在运行时校验兼容性
    pub schema_version: u32,
    pub providers: Vec<DBusProviderEntry>,
    pub header: DBusHeaderInfo,
}

/// 头部状态信息
#[derive(Debug, Clone, Serialize)]
pub struct DBusHeaderInfo {
    /// 状态文本（如 "Synced"、"5 min ago"）
    pub status_text: String,
    /// 状态种类标识符（"Synced" / "Syncing" / "Stale" / "Offline"）
    pub status_kind: String,
}

// ============================================================================
// Provider 条目
// ============================================================================

/// 单个 Provider 的 D-Bus 传输数据
#[derive(Debug, Clone, Serialize)]
pub struct DBusProviderEntry {
    /// Provider 标识符（如 "claude"、"copilot"）
    pub id: String,
    /// 显示名称（如 "Claude"）
    pub display_name: String,
    /// 图标资源路径
    pub icon_asset: String,
    /// 连接状态（"Connected" / "Refreshing" / "Error" / "Disconnected"）
    pub connection: String,
    /// 账号邮箱
    pub account_email: Option<String>,
    /// 账号层级（如 "Pro"、"Max"）
    pub account_tier: Option<String>,
    /// 配额列表
    pub quotas: Vec<DBusQuotaEntry>,
    /// 最高严重状态等级（"Green" / "Yellow" / "Red"）
    pub worst_status: String,
}

// ============================================================================
// Quota 条目
// ============================================================================

/// 单个配额的 D-Bus 传输数据
#[derive(Debug, Clone, Serialize)]
pub struct DBusQuotaEntry {
    /// 配额标题（如 "Session"、"Weekly"）
    pub label: String,
    /// 已用量
    pub used: f64,
    /// 总配额
    pub limit: f64,
    /// 状态等级（"Green" / "Yellow" / "Red"）
    pub status_level: String,
    /// 预计算的显示文本（如 "55%"、"$15.00"）
    pub display_text: String,
    /// Overview 进度条比例 [0.0, 1.0]，语义与当前 quota_display_mode 对齐
    pub bar_ratio: f32,
    /// 配额类型稳定键（如 "session"、"weekly"、"credit"）
    pub quota_type_key: String,
}

// ============================================================================
// 转换实现
// ============================================================================

impl DBusQuotaSnapshot {
    /// 从 AppSession 构建配额快照
    pub fn from_session(session: &AppSession) -> Self {
        let providers: Vec<DBusProviderEntry> = session
            .provider_store
            .enabled_providers(&session.settings)
            .map(|p| DBusProviderEntry::from_provider(p, session))
            .collect();

        let (status_kind, _) = session.header_status_text();
        DBusQuotaSnapshot {
            schema_version: DBUS_QUOTA_SCHEMA_VERSION,
            providers,
            header: DBusHeaderInfo {
                status_text: dbus_header_status_text(session),
                status_kind: format!("{:?}", status_kind),
            },
        }
    }
}

impl DBusProviderEntry {
    fn from_provider(provider: &ProviderStatus, session: &AppSession) -> Self {
        let visible_quotas = session
            .settings
            .provider
            .visible_quotas(provider.kind(), &provider.quotas);

        let quotas: Vec<DBusQuotaEntry> = visible_quotas
            .iter()
            .map(|q| DBusQuotaEntry::from_quota(q, session.settings.display.quota_display_mode))
            .collect();

        let worst = provider.worst_status();

        DBusProviderEntry {
            id: format_provider_id(&provider.provider_id),
            display_name: provider.display_name().to_string(),
            icon_asset: provider.icon_asset().to_string(),
            connection: format_connection_status(provider.connection),
            account_email: provider.account_email.clone(),
            account_tier: provider.account_tier.clone(),
            quotas,
            worst_status: format_status_level(worst),
        }
    }
}

impl DBusQuotaEntry {
    fn from_quota(quota: &QuotaInfo, display_mode: crate::models::QuotaDisplayMode) -> Self {
        let sl = quota.status_level();
        DBusQuotaEntry {
            label: super::format_quota_label(quota),
            used: quota.used,
            limit: quota.limit,
            status_level: format_status_level(sl),
            display_text: super::compact_quota_display_text(quota, display_mode),
            bar_ratio: super::compact_quota_bar_ratio(quota, sl, display_mode),
            quota_type_key: quota.quota_type.stable_key(),
        }
    }
}

// ============================================================================
// 格式化辅助（公开，供测试和其他模块复用）
// ============================================================================

/// 将 StatusLevel 转为字符串（用于 D-Bus JSON 传输）
pub fn format_status_level(level: StatusLevel) -> String {
    match level {
        StatusLevel::Green => "Green".to_string(),
        StatusLevel::Yellow => "Yellow".to_string(),
        StatusLevel::Red => "Red".to_string(),
    }
}

/// 将 ConnectionStatus 转为字符串（用于 D-Bus JSON 传输）
pub fn format_connection_status(status: ConnectionStatus) -> String {
    match status {
        ConnectionStatus::Connected => "Connected".to_string(),
        ConnectionStatus::Disconnected => "Disconnected".to_string(),
        ConnectionStatus::Refreshing => "Refreshing".to_string(),
        ConnectionStatus::Error => "Error".to_string(),
    }
}

/// 将 ProviderId 转为字符串标识符
pub fn format_provider_id(id: &ProviderId) -> String {
    match id {
        ProviderId::BuiltIn(kind) => kind.id_key().to_string(),
        ProviderId::Custom(s) => s.clone(),
    }
}

/// D-Bus 专用头部状态文本（与 GPUI selector 逻辑对齐，但不依赖 ViewModel）
fn dbus_header_status_text(session: &AppSession) -> String {
    let (status_kind, elapsed) = session.header_status_text();
    match status_kind {
        HeaderStatusKind::Synced => "Synced".to_string(),
        HeaderStatusKind::Syncing => "Syncing".to_string(),
        HeaderStatusKind::Offline => "Offline".to_string(),
        HeaderStatusKind::Stale => {
            let secs = elapsed.unwrap_or(0);
            if secs < 3600 {
                format!("{} min ago", secs / 60)
            } else {
                format!("{} hr ago", secs / 3600)
            }
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::test_helpers::setup_test_locale as setup_locale;

    // ── 格式化函数 ──────────────────────────────────────

    #[test]
    fn status_level_formatting() {
        assert_eq!(format_status_level(StatusLevel::Green), "Green");
        assert_eq!(format_status_level(StatusLevel::Yellow), "Yellow");
        assert_eq!(format_status_level(StatusLevel::Red), "Red");
    }

    #[test]
    fn connection_status_formatting() {
        assert_eq!(
            format_connection_status(ConnectionStatus::Connected),
            "Connected"
        );
        assert_eq!(
            format_connection_status(ConnectionStatus::Disconnected),
            "Disconnected"
        );
        assert_eq!(
            format_connection_status(ConnectionStatus::Refreshing),
            "Refreshing"
        );
        assert_eq!(format_connection_status(ConnectionStatus::Error), "Error");
    }

    #[test]
    fn provider_id_formatting() {
        use crate::models::ProviderKind;
        assert_eq!(
            format_provider_id(&ProviderId::BuiltIn(ProviderKind::Claude)),
            "claude"
        );
        assert_eq!(
            format_provider_id(&ProviderId::Custom("my-api".to_string())),
            "my-api"
        );
    }

    // ── QuotaEntry 序列化 ───────────────────────────────

    #[test]
    fn dbus_quota_entry_from_quota_used_mode() {
        let _g = setup_locale();
        use crate::models::{QuotaLabelSpec, QuotaType};

        let quota = QuotaInfo::with_details(
            QuotaLabelSpec::Session,
            45.0,
            100.0,
            QuotaType::Session,
            None,
        );

        let entry = DBusQuotaEntry::from_quota(&quota, crate::models::QuotaDisplayMode::Used);

        assert_eq!(entry.used, 45.0);
        assert_eq!(entry.limit, 100.0);
        // 45% used = 55% remaining > 50% => Green
        assert_eq!(entry.status_level, "Green");
        assert_eq!(entry.quota_type_key, "session");
        assert!((entry.bar_ratio - 0.45).abs() < f32::EPSILON);

        // JSON 序列化验证
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"used\":45.0"));
        assert!(json.contains("\"limit\":100.0"));
        assert!(json.contains("\"status_level\":\"Green\""));
        assert!(json.contains("\"bar_ratio\":0.45"));
    }

    #[test]
    fn dbus_quota_entry_from_quota_remaining_mode() {
        let _g = setup_locale();
        use crate::models::{QuotaLabelSpec, QuotaType};

        let quota =
            QuotaInfo::with_details(QuotaLabelSpec::Weekly, 85.0, 100.0, QuotaType::Weekly, None);

        let entry = DBusQuotaEntry::from_quota(&quota, crate::models::QuotaDisplayMode::Remaining);

        // 85% used = 15% remaining => Red (remaining < 20%)
        assert_eq!(entry.status_level, "Red");
        assert_eq!(entry.quota_type_key, "weekly");
        // Remaining mode: "15%"
        assert_eq!(entry.display_text, "15%");
        assert!((entry.bar_ratio - 0.15).abs() < f32::EPSILON);
    }

    #[test]
    fn dbus_quota_entry_credit_type() {
        let _g = setup_locale();
        use crate::models::{QuotaLabelSpec, QuotaType};

        let quota =
            QuotaInfo::with_details(QuotaLabelSpec::Credits, 5.0, 20.0, QuotaType::Credit, None);

        let entry = DBusQuotaEntry::from_quota(&quota, crate::models::QuotaDisplayMode::Used);

        assert_eq!(entry.status_level, "Green");
        // Used mode for Credit: "$5.00"
        assert_eq!(entry.display_text, "$5.00");
    }

    #[test]
    fn dbus_snapshot_round_trip_json() {
        let _g = setup_locale();
        use crate::models::test_helpers::make_test_provider;
        use crate::models::{AppSettings, ConnectionStatus, ProviderId, ProviderKind};

        // 构建一个带 1 个 provider 的 AppSession，并启用该 provider
        let mut settings = AppSettings::default();
        settings
            .provider
            .set_enabled(&ProviderId::BuiltIn(ProviderKind::Claude), true);
        let provider = make_test_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        let session = AppSession::new(settings, vec![provider]);

        let snapshot = DBusQuotaSnapshot::from_session(&session);

        // 至少有 1 个 provider
        assert_eq!(snapshot.schema_version, DBUS_QUOTA_SCHEMA_VERSION);
        assert!(!snapshot.providers.is_empty());

        // JSON 序列化 → 反序列化不丢失
        let json = serde_json::to_string(&snapshot).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["schema_version"].as_u64(), Some(1));
        assert!(parsed.get("providers").is_some());
        assert!(parsed.get("header").is_some());
        assert!(parsed["providers"].as_array().unwrap().len() >= 1);
    }
}
