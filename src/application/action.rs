use super::state::{GlobalHotkeyError, SettingsTab};
use crate::models::{
    AppTheme, CustomProviderLifecycleFailure, NavTab, NewApiEditData, NewApiSaveSuccess,
    ProviderId, QuotaDisplayMode, ScriptProviderConfig, ScriptProviderDeleteSuccess,
    ScriptProviderEditData, ScriptProviderSaveSuccess, ScriptProviderTestResult, TrayIconStyle,
};
use crate::refresh::{RefreshEvent, RefreshReason};
use std::path::PathBuf;

#[derive(Debug)]
pub enum AppAction {
    SelectNavTab(NavTab),
    SetSettingsTab(SettingsTab),
    SelectSettingsProvider(ProviderId),
    ToggleCadenceDropdown,
    SetTokenEditing {
        provider_id: ProviderId,
        editing: bool,
    },
    SaveProviderToken {
        provider_id: ProviderId,
        token: String,
    },
    /// 拖拽排序：将 Provider 移动到目标索引位置
    MoveProviderToIndex {
        id: ProviderId,
        target_index: usize,
    },
    SaveGlobalHotkey(String),
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    SaveTrayPopupPosition(crate::models::SavedWindowPosition),
    /// 平台注册与设置持久化完成后，由 reducer 唯一提交最终状态。
    GlobalHotkeyApplyFinished {
        requested: String,
        result: Result<String, GlobalHotkeyError>,
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
    SubmitNewApi(crate::models::NewApiConfig),
    /// NewAPI 保存完成（由 runtime I/O 回传，reducer 统一处理状态和通知）
    NewApiSaveFinished {
        config: crate::models::NewApiConfig,
        filename: String,
        /// 编辑模式下原始 YAML 的 Provider ID（随 `SaveProvider` 透传，供失败回滚重建表单）
        original_id: Option<String>,
        is_editing: bool,
        result: Result<NewApiSaveSuccess, CustomProviderLifecycleFailure>,
    },
    /// 进入 NewAPI 编辑模式（从磁盘读取已有配置回填表单）
    EditNewApi {
        provider_id: ProviderId,
    },
    /// NewAPI 编辑数据加载完成（由 runtime I/O 回传）
    NewApiLoadFinished {
        provider_id: ProviderId,
        result: Result<NewApiEditData, CustomProviderLifecycleFailure>,
    },
    /// 删除 NewAPI 自定义 Provider（从磁盘删除 YAML 文件）
    DeleteNewApi {
        provider_id: ProviderId,
    },
    /// NewAPI 删除完成（由 runtime I/O 回传）
    NewApiDeleteFinished {
        provider_id: ProviderId,
        result: Result<PathBuf, CustomProviderLifecycleFailure>,
    },
    /// 进入删除 NewAPI 的二次确认状态
    ConfirmDeleteNewApi,
    /// 取消删除 NewAPI 的二次确认
    CancelDeleteNewApi,
    /// 进入脚本 Provider 添加模式（显示脚本编辑表单）
    EnterAddScriptProvider,
    /// 取消脚本 Provider 添加 / 编辑（关闭表单）
    CancelAddScriptProvider,
    /// 测试脚本 Provider 配置（执行脚本并解析 stdout）
    TestScriptProvider(ScriptProviderConfig),
    /// 脚本测试结果回填（由 runtime 后台任务发回前台）
    ScriptProviderTestFinished {
        request_id: u64,
        result: ScriptProviderTestResult,
    },
    /// 提交脚本 Provider 配置（生成脚本文件 + YAML）
    SubmitScriptProvider(ScriptProviderConfig),
    /// 脚本 Provider 保存完成（由 runtime I/O 回传）
    ScriptProviderSaveFinished {
        config: ScriptProviderConfig,
        yaml_filename: String,
        script_filename: String,
        is_editing: bool,
        result: Result<ScriptProviderSaveSuccess, CustomProviderLifecycleFailure>,
    },
    /// 进入脚本 Provider 编辑模式
    EditScriptProvider {
        provider_id: ProviderId,
    },
    /// 脚本 Provider 编辑数据加载完成（由 runtime I/O 回传）
    ScriptProviderLoadFinished {
        provider_id: ProviderId,
        result: Result<ScriptProviderEditData, CustomProviderLifecycleFailure>,
    },
    /// 删除脚本 Provider
    DeleteScriptProvider {
        provider_id: ProviderId,
    },
    /// 脚本 Provider 删除完成（由 runtime I/O 回传）
    ScriptProviderDeleteFinished {
        provider_id: ProviderId,
        result: Result<ScriptProviderDeleteSuccess, CustomProviderLifecycleFailure>,
    },
    /// 进入删除脚本 Provider 的二次确认状态
    ConfirmDeleteScriptProvider,
    /// 取消删除脚本 Provider 的二次确认
    CancelDeleteScriptProvider,
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
        provider_id: ProviderId,
        quota_key: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum DebugNotificationKind {
    Low,
    Exhausted,
    Recovered,
}
