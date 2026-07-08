use super::quota_alert::QuotaAlert;
use crate::application::DebugNotificationKind;
use crate::models::{NewApiConfig, ProviderId, ScriptProviderConfig, StatusLevel, TrayIconStyle};
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
/// Runtime 通过 context effect runner match 这些变体，再由 capability adapter
/// 根据当前 GPUI 入口（`Context<V>` / `Window + App` / `App`）执行。
/// `Context<V>` 只允许 View-safe effect；强上下文 effect 必须走 `Window + App` 或 `App`。
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
    ScriptProvider(ScriptProviderEffect),
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
    /// 自启动状态变更通知，由 runtime 层负责 i18n
    AutoLaunchToggled {
        enabled: bool,
    },
    /// 普通 i18n 文本通知，用于 reducer 集中选择用户可见结果。
    PlainI18n {
        title_key: &'static str,
        body_key: &'static str,
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
    /// 保存 NewAPI Provider：runtime 负责 YAML 生成、文件写入和同步持久化，
    /// 然后通过 `NewApiSaveFinished` 把结果交回 reducer 处理通知、reload 或回滚。
    SaveProvider {
        config: NewApiConfig,
        original_filename: Option<String>,
        /// 编辑模式标志：失败时不回滚预注册（旧文件仍有效）
        is_editing: bool,
    },
    /// 删除 NewAPI Provider：runtime 负责文件定位 + 文件删除，然后回传 `NewApiDeleteFinished`。
    DeleteProvider { provider_id: ProviderId },
    /// 从磁盘加载 NewAPI 配置，由 runtime 执行 I/O 后回传 `NewApiLoadFinished`。
    LoadConfig { provider_id: ProviderId },
}

#[derive(Debug)]
pub enum ScriptProviderEffect {
    /// Queue a script test request; queue failure returns `ScriptProviderTestFinished`
    /// through the foreground action pump.
    TestProvider {
        request_id: u64,
        config: ScriptProviderConfig,
    },
    /// Save script + generated YAML, then return `ScriptProviderSaveFinished`.
    SaveProvider {
        config: ScriptProviderConfig,
        original_yaml_filename: Option<String>,
        original_script_filename: Option<String>,
        is_editing: bool,
    },
    /// Delete script-generated YAML and companion script file, then return `ScriptProviderDeleteFinished`.
    DeleteProvider { provider_id: ProviderId },
    /// Load script-generated config from disk for editing, then return `ScriptProviderLoadFinished`.
    LoadConfig { provider_id: ProviderId },
}

/// Reducer 产出的副作用（两级路由）。
///
/// Runtime 层根据外层 variant 先分流：
/// - `Context` → view-safe context effect runner + capability adapter
/// - `Common` → `effects::run_common_effect`
///
/// 新增领域 effect 需改对应子枚举 + runtime/effects 下的对应执行器
/// 新增 `ContextEffect` 需同步判断 View-safe / Full context runner 的能力边界
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

/// 为子 Effect 类型生成 `From<SubEffect> for CommonEffect` 和 `From<SubEffect> for AppEffect`。
macro_rules! impl_common_effect_from {
    ($sub:ident => $variant:ident) => {
        impl From<$sub> for CommonEffect {
            fn from(e: $sub) -> Self {
                Self::$variant(e)
            }
        }
        impl From<$sub> for AppEffect {
            fn from(e: $sub) -> Self {
                CommonEffect::from(e).into()
            }
        }
    };
}

impl_common_effect_from!(SettingsEffect => Settings);
impl_common_effect_from!(NotificationEffect => Notification);
impl_common_effect_from!(RefreshEffect => Refresh);
impl_common_effect_from!(DebugEffect => Debug);
impl_common_effect_from!(NewApiEffect => NewApi);
impl_common_effect_from!(ScriptProviderEffect => ScriptProvider);

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
