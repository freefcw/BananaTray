use crate::application::DebugNotificationKind;
use crate::models::{ProviderId, StatusLevel, TrayIconStyle};
use crate::notification::QuotaAlert;
use crate::refresh::RefreshRequest;

/// 托盘图标请求 — 区分用户手选的静态样式和动态模式下的状态着色
#[derive(Debug, Clone, Copy)]
pub enum TrayIconRequest {
    /// 用户手选的静态样式（Monochrome / Yellow / Colorful）
    Static(TrayIconStyle),
    /// Dynamic 模式下根据额度状态自动选择的颜色
    /// - Green → Monochrome（减少视觉干扰）
    /// - Yellow → 黄色香蕉
    /// - Red → 红色香蕉
    DynamicStatus(StatusLevel),
}

/// Reducer 产出的副作用。
///
/// Runtime 层（`runtime/mod.rs`）根据当前 GPUI 上下文直接 match 这个枚举，
/// 不再经过中间的路由层。
#[derive(Debug)]
pub enum AppEffect {
    Render,
    PersistSettings,
    SendRefreshRequest(RefreshRequest),
    OpenSettingsWindow,
    OpenUrl(String),
    SyncAutoLaunch(bool),
    /// 发送简单文本通知（无 QuotaAlert 包装）
    SendPlainNotification {
        title: String,
        body: String,
    },
    ApplyLocale(String),
    UpdateLogLevel(String),
    SendQuotaNotification {
        alert: QuotaAlert,
        with_sound: bool,
    },
    SendDebugNotification {
        kind: DebugNotificationKind,
        with_sound: bool,
    },
    OpenLogDirectory,
    CopyToClipboard(String),
    /// 启用日志捕获 → 提升日志级别 → 发送 RefreshOne
    StartDebugRefresh(ProviderId),
    /// 恢复调试刷新前的日志级别
    RestoreLogLevel(log::LevelFilter),
    /// 清空调试日志缓冲区
    ClearDebugLogs,
    /// 切换托盘图标
    ApplyTrayIcon(TrayIconRequest),
    QuitApp,
}
