use super::state::SettingsTab;
use crate::models::{AppTheme, NavTab, ProviderId, ProviderKind, QuotaDisplayMode, TrayIconStyle};
use crate::refresh::{RefreshEvent, RefreshReason};

#[derive(Debug)]
pub enum AppAction {
    SelectNavTab(NavTab),
    SetSettingsTab(SettingsTab),
    SelectSettingsProvider(ProviderId),
    ToggleCadenceDropdown,
    SetCopilotTokenEditing(bool),
    SaveCopilotToken(String),
    /// 拖拽排序：将 Provider 移动到目标索引位置
    MoveProviderToIndex {
        id: ProviderId,
        target_index: usize,
    },
    UpdateSetting(SettingChange),
    RefreshProvider {
        id: ProviderId,
        reason: RefreshReason,
    },
    /// Overview 页面刷新所有已启用的 Provider，并重置定期刷新定时器
    RefreshAll,
    ToggleProvider(ProviderId),
    RefreshEventReceived(RefreshEvent),
    OpenSettings {
        provider: Option<ProviderId>,
    },
    OpenDashboard(ProviderId),
    OpenUrl(String),
    UpdateLogLevel(String),
    SendDebugNotification(DebugNotificationKind),
    OpenLogDirectory,
    CopyToClipboard(String),
    /// Debug Tab: 选择调试目标 Provider
    SelectDebugProvider(ProviderId),
    /// Debug Tab: 强制刷新选中的 Provider（跳过 cooldown，临时提升日志级别）
    DebugRefreshProvider,
    /// Debug Tab: 清空日志缓冲区
    ClearDebugLogs,
    /// 弹窗可见性变化（控制 Dynamic 图标延迟更新）
    PopupVisibilityChanged(bool),
    /// 进入"添加 Provider"选择模式（右面板切换为选择列表）
    EnterAddProvider,
    /// 取消添加 Provider（退出选择模式）
    CancelAddProvider,
    /// 将 Provider 添加到 sidebar 列表
    AddProviderToSidebar(ProviderId),
    /// 从 sidebar 列表移除 Provider
    RemoveProviderFromSidebar(ProviderId),
    /// 进入移除 Provider 的二次确认状态
    ConfirmRemoveProvider,
    /// 取消移除 Provider 的二次确认
    CancelRemoveProvider,
    /// 进入 NewAPI 添加模式（显示表单）
    EnterAddNewApi,
    /// 取消 NewAPI 添加（关闭表单）
    CancelAddNewApi,
    /// 提交 NewAPI 配置（生成 YAML + 保存 + 通知重启）
    SubmitNewApi {
        display_name: String,
        base_url: String,
        cookie: String,
        user_id: Option<String>,
        divisor: Option<f64>,
    },
    /// 进入 NewAPI 编辑模式（从磁盘读取已有配置回填表单）
    EditNewApi {
        provider_id: ProviderId,
    },
    /// 删除 NewAPI 自定义 Provider（从磁盘删除 YAML 文件）
    DeleteNewApi {
        provider_id: ProviderId,
    },
    /// 进入删除 NewAPI 的二次确认状态
    ConfirmDeleteNewApi,
    /// 取消删除 NewAPI 的二次确认
    CancelDeleteNewApi,
    QuitApp,
}

#[derive(Debug, Clone)]
pub enum SettingChange {
    ToggleAutoHideWindow,
    ToggleStartAtLogin,
    ToggleSessionQuotaNotifications,
    ToggleNotificationSound,
    ToggleShowDashboardButton,
    ToggleShowRefreshButton,
    ToggleShowDebugTab,
    ToggleShowAccountInfo,
    ToggleShowOverview,
    Theme(AppTheme),
    Language(String),
    RefreshCadence(Option<u64>),
    SetTrayIconStyle(TrayIconStyle),
    SetQuotaDisplayMode(QuotaDisplayMode),
    ToggleQuotaVisibility {
        kind: ProviderKind,
        quota_key: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum DebugNotificationKind {
    Low,
    Exhausted,
    Recovered,
}
