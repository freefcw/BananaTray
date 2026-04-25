use super::quota_alert::QuotaAlert;
use crate::application::DebugNotificationKind;
use crate::models::{NewApiConfig, ProviderId, StatusLevel, TrayIconStyle};
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

// ============================================================================
// 两级 Effect 架构：ContextEffect（需要 GPUI 上下文）/ CommonEffect（GPUI-free）
// ============================================================================

/// 需要 GPUI 上下文才能执行的 effect。
///
/// Runtime 通过 `run_context_effect` 统一 match 这些变体，再由 capability adapter
/// 根据当前 GPUI 入口（`Context<V>` / `Window + App` / `App`）执行或降级。
#[derive(Debug)]
pub enum ContextEffect {
    Render,
    OpenSettingsWindow,
    OpenUrl(String),
    ApplyTrayIcon(TrayIconRequest),
    ApplyGlobalHotkey(String),
    QuitApp,
}

/// 不依赖 GPUI 上下文的 effect。
///
/// 顶层只负责按领域分派；具体副作用参数放在对应子枚举里，runtime/effects
/// 下的同名执行器负责真实 I/O 或平台调用。
#[derive(Debug)]
pub enum CommonEffect {
    Settings(SettingsEffect),
    Notification(NotificationEffect),
    Refresh(RefreshEffect),
    Debug(DebugEffect),
    NewApi(NewApiEffect),
}

#[derive(Debug)]
pub enum SettingsEffect {
    PersistSettings,
    SyncAutoLaunch(bool),
    ApplyLocale(String),
    UpdateLogLevel(String),
}

#[derive(Debug)]
pub enum NotificationEffect {
    /// 发送简单文本通知（无 QuotaAlert 包装）
    Plain {
        title: String,
        body: String,
    },
    Quota {
        alert: QuotaAlert,
        with_sound: bool,
    },
    Debug {
        kind: DebugNotificationKind,
        with_sound: bool,
    },
}

#[derive(Debug)]
pub enum RefreshEffect {
    SendRequest(RefreshRequest),
}

#[derive(Debug)]
pub enum DebugEffect {
    OpenLogDirectory,
    CopyToClipboard(String),
    /// 启用日志捕获 → 提升日志级别 → 发送 RefreshOne
    StartRefresh(ProviderId),
    /// 恢复调试刷新前的日志级别
    RestoreLogLevel(log::LevelFilter),
    /// 清空调试日志缓冲区
    ClearLogs,
}

#[derive(Debug)]
pub enum NewApiEffect {
    /// 保存 NewAPI Provider：runtime 负责 YAML 生成 + 文件写入 + 持久化 + 通知 + 热重载
    ///
    /// 只有写入成功时才执行 SettingsEffect::PersistSettings 和 NotificationEffect::Plain，
    /// 确保失败时不会产生幽灵 Provider 或虚假成功通知。
    SaveProvider {
        config: NewApiConfig,
        original_filename: Option<String>,
        /// 编辑模式标志：失败时不回滚预注册（旧文件仍有效）
        is_editing: bool,
    },
    /// 删除 NewAPI Provider：runtime 负责文件名推导 + 文件删除 + 热重载
    DeleteProvider { provider_id: ProviderId },
    /// 从磁盘加载 NewAPI 配置（填充编辑表单），由 runtime 执行 I/O
    LoadConfig { provider_id: ProviderId },
}

/// Reducer 产出的副作用（两级路由）。
///
/// Runtime 层根据外层 variant 先分流：
/// - `Context` → `run_context_effect` + capability adapter
/// - `Common` → `effects::run_common_effect`
///
/// 新增领域 effect 需改对应子枚举 + runtime/effects 下的对应执行器
/// 新增 `ContextEffect` 只需改 2 处：枚举定义 + `run_context_effect`
#[derive(Debug)]
pub enum AppEffect {
    Context(ContextEffect),
    Common(CommonEffect),
}

// ── From impls ───────────────────────────────────────
// reducer 使用 `ContextEffect::Render.into()` / `SettingsEffect::PersistSettings.into()`
// 保持构造简洁，避免为每个 effect 再维护一层样板构造方法。

impl From<ContextEffect> for AppEffect {
    fn from(e: ContextEffect) -> Self {
        Self::Context(e)
    }
}

impl From<CommonEffect> for AppEffect {
    fn from(e: CommonEffect) -> Self {
        Self::Common(e)
    }
}

impl From<SettingsEffect> for CommonEffect {
    fn from(e: SettingsEffect) -> Self {
        Self::Settings(e)
    }
}

impl From<SettingsEffect> for AppEffect {
    fn from(e: SettingsEffect) -> Self {
        CommonEffect::from(e).into()
    }
}

impl From<NotificationEffect> for CommonEffect {
    fn from(e: NotificationEffect) -> Self {
        Self::Notification(e)
    }
}

impl From<NotificationEffect> for AppEffect {
    fn from(e: NotificationEffect) -> Self {
        CommonEffect::from(e).into()
    }
}

impl From<RefreshEffect> for CommonEffect {
    fn from(e: RefreshEffect) -> Self {
        Self::Refresh(e)
    }
}

impl From<RefreshEffect> for AppEffect {
    fn from(e: RefreshEffect) -> Self {
        CommonEffect::from(e).into()
    }
}

impl From<DebugEffect> for CommonEffect {
    fn from(e: DebugEffect) -> Self {
        Self::Debug(e)
    }
}

impl From<DebugEffect> for AppEffect {
    fn from(e: DebugEffect) -> Self {
        CommonEffect::from(e).into()
    }
}

impl From<NewApiEffect> for CommonEffect {
    fn from(e: NewApiEffect) -> Self {
        Self::NewApi(e)
    }
}

impl From<NewApiEffect> for AppEffect {
    fn from(e: NewApiEffect) -> Self {
        CommonEffect::from(e).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProviderKind;
    use crate::refresh::{RefreshReason, RefreshRequest};

    #[test]
    fn context_effect_into_wraps_context_variant() {
        let effect: AppEffect = ContextEffect::OpenUrl("https://example.com".to_string()).into();

        assert!(matches!(
            effect,
            AppEffect::Context(ContextEffect::OpenUrl(url)) if url == "https://example.com"
        ));
    }

    #[test]
    fn common_effect_into_wraps_common_variant() {
        let effect: AppEffect = RefreshEffect::SendRequest(RefreshRequest::RefreshOne {
            id: ProviderId::BuiltIn(ProviderKind::Claude),
            reason: RefreshReason::Manual,
        })
        .into();

        assert!(matches!(
            effect,
            AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
                RefreshRequest::RefreshOne {
                    id: ProviderId::BuiltIn(ProviderKind::Claude),
                    reason: RefreshReason::Manual,
                }
            )))
        ));
    }

    #[test]
    fn tray_icon_request_preserves_dynamic_status() {
        let effect: AppEffect =
            ContextEffect::ApplyTrayIcon(TrayIconRequest::DynamicStatus(StatusLevel::Red)).into();

        assert!(matches!(
            effect,
            AppEffect::Context(ContextEffect::ApplyTrayIcon(
                TrayIconRequest::DynamicStatus(StatusLevel::Red)
            ))
        ));
    }
}
