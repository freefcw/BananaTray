//! Pure-logic application state, free of GPUI dependency.
//! Extracted for testability (GPUI proc macros crash during test compilation).

use super::quota_alert::QuotaAlertTracker;
use crate::models::{
    AppSettings, ConnectionStatus, NavTab, NewApiEditData, ProviderId, ProviderKind,
    ProviderStatus, ScriptProviderEditData, ScriptProviderTestResult, StatusLevel,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormIdentity {
    NewApiAdd,
    NewApiEdit {
        original_filename: String,
    },
    ScriptProviderAdd,
    ScriptProviderEdit {
        original_yaml_filename: String,
        original_script_filename: String,
    },
}

// ============================================================================
// Provider 面板可见性规则（单一真理来源，供 selector 和 popup_height 共用）
// ============================================================================

/// Provider 面板中各可选区域的可见性标志
pub struct ProviderPanelFlags {
    /// 是否显示账户信息卡片
    pub show_account_info: bool,
    /// 是否显示底部 Dashboard 链接行
    pub show_dashboard_row: bool,
    /// Provider 是否有 Dashboard URL
    pub has_dashboard_url: bool,
}

/// Overview 当前是否会把 Provider 渲染为配额内容，而非单行状态提示。
///
/// selector 与 popup height 必须共用这条规则，否则刷新或断开期间保留的缓存配额
/// 会让窗口高度和实际卡片内容分叉。
pub(super) fn overview_provider_renders_quotas(provider: &ProviderStatus) -> bool {
    provider.supports_refresh()
        && !provider.quotas.is_empty()
        && matches!(
            provider.connection,
            ConnectionStatus::Connected | ConnectionStatus::Error
        )
}

/// 根据设置和 Provider 状态计算面板可见性标志。
///
/// 核心规则：账户卡片已包含 Dashboard 入口时，隐藏底部 Dashboard 行（互斥）。
pub fn provider_panel_flags(
    settings: &AppSettings,
    provider: &ProviderStatus,
) -> ProviderPanelFlags {
    let has_dashboard_url = !provider.dashboard_url().is_empty();
    let show_account_info = settings.display.show_account_info && provider.account_email.is_some();
    let show_dashboard_row =
        settings.display.show_dashboard_button && has_dashboard_url && !show_account_info;

    ProviderPanelFlags {
        show_account_info,
        show_dashboard_row,
        has_dashboard_url,
    }
}

// ============================================================================
// 子状态结构 (SRP: 每个结构体负责一个独立职责)
// ============================================================================

/// Provider 数据存储
pub struct ProviderStore {
    pub providers: Vec<ProviderStatus>,
}

impl ProviderStore {
    /// 通过 ProviderId 查找 Provider
    pub fn find_by_id(&self, id: &ProviderId) -> Option<&ProviderStatus> {
        self.providers.iter().find(|p| p.provider_id == *id)
    }

    /// 通过 ProviderId 查找可变 Provider
    pub fn find_by_id_mut(&mut self, id: &ProviderId) -> Option<&mut ProviderStatus> {
        self.providers.iter_mut().find(|p| p.provider_id == *id)
    }

    /// 通过 ProviderId 标记为刷新中
    pub fn mark_refreshing_by_id(&mut self, id: &ProviderId) {
        if let Some(provider) = self.find_by_id_mut(id) {
            provider.mark_refreshing();
        }
    }

    /// 获取所有自定义 Provider 的 ID 列表
    pub fn custom_provider_ids(&self) -> Vec<ProviderId> {
        self.providers
            .iter()
            .filter(|p| p.provider_id.is_custom())
            .map(|p| p.provider_id.clone())
            .collect()
    }

    /// 按设置顺序迭代所有已启用的 Provider
    ///
    /// 集中了 "custom_ids → ordered → filter enabled → find_by_id" 的公共遍历模式，
    /// 供 `overview_view_state`、`DBusQuotaSnapshot::from_session` 等多处复用。
    pub fn enabled_providers<'a>(
        &'a self,
        settings: &'a super::super::models::AppSettings,
    ) -> impl Iterator<Item = &'a ProviderStatus> {
        let custom_ids = self.custom_provider_ids();
        // 将 ordered_ids 收集到 Vec，避免 lifetime 问题
        let ordered: Vec<_> = settings.provider.ordered_provider_ids(&custom_ids);
        ordered
            .into_iter()
            .filter(move |id| settings.provider.is_enabled(id))
            .filter_map(move |id| self.find_by_id(&id))
    }

    /// 获取所有已启用且支持刷新的 Provider ID 列表。
    ///
    /// 统一了 `build_config_sync_request` / `refresh_all_providers` 等多处重复的
    /// `filter(enabled && supports_refresh).map(id)` 模式。
    pub fn refreshable_provider_ids(
        &self,
        settings: &super::super::models::AppSettings,
    ) -> Vec<ProviderId> {
        self.providers
            .iter()
            .filter(|p| settings.provider.is_enabled(&p.provider_id) && p.supports_refresh())
            .map(|p| p.provider_id.clone())
            .collect()
    }

    /// 根据新的状态列表同步自定义 Provider（热重载用）
    ///
    /// - 保留所有内置 Provider 状态不变
    /// - 新增的自定义 Provider 追加
    /// - 已删除的自定义 Provider 移除
    /// - 已存在的自定义 Provider 更新 definition（metadata + settings capability），保留运行时状态到下次刷新
    ///
    /// 返回新增或更新的自定义 Provider ID 列表（用于触发立即刷新）
    pub fn sync_custom_providers(&mut self, new_statuses: &[ProviderStatus]) -> Vec<ProviderId> {
        use std::collections::HashSet;

        let new_custom: Vec<_> = new_statuses
            .iter()
            .filter(|s| s.provider_id.is_custom())
            .collect();
        let new_custom_ids: HashSet<_> = new_custom.iter().map(|s| &s.provider_id).collect();

        // 移除已不存在的自定义 Provider
        self.providers
            .retain(|p| !p.provider_id.is_custom() || new_custom_ids.contains(&p.provider_id));

        let mut affected = Vec::new();
        for new_status in &new_custom {
            if let Some(existing) = self
                .providers
                .iter_mut()
                .find(|p| p.provider_id == new_status.provider_id)
            {
                // 已存在：同步 definition（metadata + settings capability），保留运行时状态
                if existing.sync_definition_from(new_status) {
                    affected.push(new_status.provider_id.clone());
                }
            } else {
                // 新增
                self.providers.push((*new_status).clone());
                affected.push(new_status.provider_id.clone());
            }
        }

        affected
    }
}

/// 纯逻辑应用会话状态
pub struct AppSession {
    pub provider_store: ProviderStore,
    pub nav: NavigationState,
    pub settings_ui: SettingsUiState,
    pub debug_ui: DebugUiState,
    pub settings: AppSettings,
    pub alert_tracker: QuotaAlertTracker,
    /// 弹窗是否可见（Dynamic 图标在弹窗可见时延迟更新，关闭后同步）
    pub popup_visible: bool,
    /// Overview 面板中展开显示全部配额的 Provider id_key 集合。
    ///
    /// 只活在本次进程内：默认折叠，用户展开后在应用生命周期里保持。
    /// 放在 session 而非 `AppView`，因为 macOS 每次关闭弹窗都会销毁 view。
    pub overview_expanded: std::collections::HashSet<String>,
}

impl AppSession {
    pub fn new(mut settings: AppSettings, providers: Vec<ProviderStatus>) -> Self {
        let provider_store = ProviderStore { providers };
        let custom_ids = provider_store.custom_provider_ids();

        // 自动注册已存在但未在 settings 中登记的自定义 Provider
        // （处理 YAML 文件存在但 settings.json 缺少对应条目的情况）
        settings
            .provider
            .register_discovered_custom_providers(&custom_ids);
        let nav = build_initial_navigation_state(&provider_store, &settings);
        let settings_ui = build_initial_settings_ui_state(&settings, &custom_ids);

        Self {
            provider_store,
            nav,
            settings_ui,
            debug_ui: DebugUiState::default(),
            settings,
            alert_tracker: QuotaAlertTracker::new(),
            popup_visible: false,
            overview_expanded: Default::default(),
        }
    }

    /// Overview 面板中该 Provider 是否展开显示全部可见配额
    pub fn is_overview_expanded(&self, id: &ProviderId) -> bool {
        self.overview_expanded.contains(&id.id_key())
    }

    /// 切换 Overview 展开状态（展开 ↔ 折叠）
    pub fn toggle_overview_expanded(&mut self, id: &ProviderId) {
        let key = id.id_key();
        if !self.overview_expanded.remove(&key) {
            self.overview_expanded.insert(key);
        }
    }

    pub fn header_status_text(&self) -> (HeaderStatusKind, Option<u64>) {
        compute_header_status(&self.nav, &self.provider_store)
    }

    pub fn popup_height(&self) -> f32 {
        if self.nav.active_tab == NavTab::Overview {
            let rows = self.overview_card_rows();
            return crate::models::compute_popup_height_for_overview(&rows);
        }
        compute_popup_height(&self.nav, &self.provider_store, &self.settings)
    }

    /// 各已启用 Provider 在 Overview 面板中占用的配额行数。
    ///
    /// 折叠卡片恒为 1 行；展开卡片按可见配额数展开。
    /// 这个行数只用于打开弹窗 / 切入 Overview 时的窗口高度；停留在 Overview
    /// 时展开折叠不再改原生窗口，多出的行走内容区滚动。
    fn overview_card_rows(&self) -> Vec<usize> {
        self.provider_store
            .enabled_providers(&self.settings)
            .map(|p| {
                if self.is_overview_expanded(&p.provider_id) && overview_provider_renders_quotas(p)
                {
                    self.settings
                        .provider
                        .visible_quota_count(&p.provider_id, &p.quotas)
                } else {
                    1
                }
            })
            .collect()
    }

    pub fn has_enabled_providers(&self) -> bool {
        self.provider_store
            .providers
            .iter()
            .any(|p| self.settings.provider.is_enabled(&p.provider_id))
    }

    /// Settings sidebar 中第一个可用 provider（用于 fallback 选中）。
    ///
    /// 消除了多处重复的 sidebar_provider_ids → first → unwrap_or(Claude) 模式。
    pub fn first_sidebar_provider(&self) -> ProviderId {
        let custom_ids = self.provider_store.custom_provider_ids();
        let sidebar_ids = self.settings.provider.sidebar_provider_ids(&custom_ids);
        sidebar_ids
            .first()
            .cloned()
            .unwrap_or_else(default_builtin_provider_id)
    }

    /// 检查 script provider ID 是否已被占用（用于生成唯一 ID）。
    ///
    /// 检查范围包括：已加载的自定义 provider、已启用 provider、
    /// provider_order、sidebar_providers。
    pub fn is_script_provider_id_occupied(&self, id: &str) -> bool {
        let provider_id = ProviderId::Custom(id.to_string());
        let key = provider_id.id_key();
        self.provider_store
            .custom_provider_ids()
            .iter()
            .any(|existing| existing.id_key() == key)
            || self.settings.provider.enabled_providers.contains_key(&key)
            || self.settings.provider.provider_order.contains(&key)
            || self.settings.provider.sidebar_providers.contains(&key)
    }

    pub fn default_provider_tab(&mut self) -> Option<NavTab> {
        if !self.has_enabled_providers() {
            return None;
        }

        let last = self.nav.last_provider_id.clone();
        let id = if self.settings.provider.is_enabled(&last) {
            last
        } else {
            let fallback = self
                .provider_store
                .providers
                .iter()
                .find(|p| self.settings.provider.is_enabled(&p.provider_id))
                .map(|p| p.provider_id.clone())
                .unwrap_or(last);
            self.nav.last_provider_id = fallback.clone();
            fallback
        };

        Some(NavTab::Provider(id))
    }

    /// 获取所有已启用 Provider 的综合状态等级（取最坏值）。
    /// 仅在 Dynamic 模式下使用，用于决定托盘图标颜色。
    ///
    /// 只统计已启用且已连接（Connected）的 Provider，与刷新链路
    /// （`refreshable_provider_ids`）的启用语义一致；
    /// 无符合条件的 Provider 时返回 Green（安全默认值）。
    pub fn worst_enabled_provider_status(&self) -> StatusLevel {
        self.provider_store
            .providers
            .iter()
            .filter(|p| self.settings.provider.is_enabled(&p.provider_id))
            .filter(|p| p.connection == ConnectionStatus::Connected)
            .map(|p| p.worst_status())
            .max()
            .unwrap_or(StatusLevel::Green)
    }
}

fn build_initial_navigation_state(
    store: &ProviderStore,
    settings: &AppSettings,
) -> NavigationState {
    let first_enabled = first_enabled_provider_id(store, settings);
    let last_provider_id = first_enabled
        .clone()
        .unwrap_or_else(default_builtin_provider_id);
    let active_tab = initial_active_tab(settings, first_enabled);

    NavigationState {
        active_tab,
        last_provider_id,
        generation: 0,
        prev_active_tab: None,
    }
}

fn build_initial_settings_ui_state(
    settings: &AppSettings,
    custom_ids: &[ProviderId],
) -> SettingsUiState {
    // 设置页默认选中 sidebar 列表中的第一个 provider（而非硬编码 Claude）
    let selected_provider = settings
        .provider
        .sidebar_provider_ids(custom_ids)
        .into_iter()
        .next()
        .unwrap_or_else(default_builtin_provider_id);

    SettingsUiState {
        active_tab: SettingsTab::General,
        selected_provider,
        cadence_dropdown_open: false,
        token_editing_provider: None,
        modal: SettingsModalState::Idle,
        script_provider_testing: false,
        script_provider_test_request_id: 0,
        script_provider_pending_test_request_id: None,
        script_provider_test_result: None,
        global_hotkey_error: None,
        global_hotkey_error_candidate: None,
    }
}

fn first_enabled_provider_id(store: &ProviderStore, settings: &AppSettings) -> Option<ProviderId> {
    store
        .providers
        .iter()
        .find(|p| settings.provider.is_enabled(&p.provider_id))
        .map(|p| p.provider_id.clone())
}

fn initial_active_tab(settings: &AppSettings, first_enabled: Option<ProviderId>) -> NavTab {
    if settings.display.show_overview {
        NavTab::Overview
    } else {
        first_enabled
            .map(NavTab::Provider)
            .unwrap_or(NavTab::Settings)
    }
}

/// Manifest 中第一个内置 provider 的 ID，用作各种 fallback。
///
/// 使用 manifest 顺序而非硬编码 Claude，使得 manifest 首项变更时自动适配。
fn default_builtin_provider_id() -> ProviderId {
    ProviderId::BuiltIn(ProviderKind::first())
}

/// Tray 弹出窗口的导航状态
pub struct NavigationState {
    pub active_tab: NavTab,
    pub last_provider_id: ProviderId,
    /// 每次 switch_to 递增，用于让进度条动画在切换时重播
    pub generation: u64,
    /// 切换前的 tab，用于导航栏滑块动画的起点
    pub prev_active_tab: Option<NavTab>,
}

impl NavigationState {
    /// 切换到指定 tab，若为 Provider 则同步 last_provider_id
    pub fn switch_to(&mut self, tab: NavTab) {
        self.prev_active_tab = Some(self.active_tab.clone());
        self.generation += 1;
        if let NavTab::Provider(ref id) = tab {
            self.last_provider_id = id.clone();
        }
        self.active_tab = tab;
    }

    /// 当某个 provider 被禁用时，若它是当前活跃 tab 则回退到下一个已启用的 provider
    pub fn fallback_on_disable(
        &mut self,
        disabled: &ProviderId,
        providers: &[ProviderStatus],
        settings: &AppSettings,
    ) {
        let is_current = matches!(&self.active_tab, NavTab::Provider(id) if id == disabled);
        if !is_current {
            return;
        }
        if let Some(next) = providers
            .iter()
            .find(|p| p.provider_id != *disabled && settings.provider.is_enabled(&p.provider_id))
        {
            self.switch_to(NavTab::Provider(next.provider_id.clone()));
        }
    }
}

/// 设置窗口 Tab 枚举（纯数据，不依赖 GPUI）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Providers,
    Display,
    About,
    Debug,
}

/// 全局热键保存失败原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalHotkeyError {
    Empty,
    InvalidFormat,
    MissingModifier,
    ModifierOnly,
    Conflict(String),
    RegistrationFailed(String),
    PersistenceFailed,
}

impl GlobalHotkeyError {
    /// 仅当配置本身不可用时返回 true；这类错误允许启动阶段回退到默认热键并修正磁盘。
    pub fn is_invalid_configuration(&self) -> bool {
        matches!(
            self,
            Self::Empty | Self::InvalidFormat | Self::MissingModifier | Self::ModifierOnly
        )
    }
}

/// 设置窗口的临时 UI 状态
pub struct SettingsUiState {
    pub active_tab: SettingsTab,
    pub selected_provider: ProviderId,
    pub cadence_dropdown_open: bool,
    /// 正在编辑 Token 的 Provider ID（None = 未编辑）
    pub token_editing_provider: Option<ProviderId>,
    /// 右侧面板的互斥模态状态机。
    ///
    /// 把原本散落的 `adding_newapi` / `editing_newapi` / `adding_provider` /
    /// `confirming_remove_provider` / `confirming_delete_newapi` 等字段折叠成
    /// 单一 enum，使"模式互斥"成为类型层不变量。
    pub modal: SettingsModalState,
    /// 脚本测试是否正在后台执行
    pub script_provider_testing: bool,
    /// 脚本测试请求序号，用于忽略过期异步结果
    pub script_provider_test_request_id: u64,
    /// 当前等待回填的脚本测试请求序号
    pub script_provider_pending_test_request_id: Option<u64>,
    /// 最近一次脚本测试结果
    pub script_provider_test_result: Option<ScriptProviderTestResult>,
    /// General Tab 全局热键设置的最近一次错误
    pub global_hotkey_error: Option<GlobalHotkeyError>,
    /// 与 `global_hotkey_error` 对应的候选热键（持久化格式）
    pub global_hotkey_error_candidate: Option<String>,
}

impl SettingsUiState {
    /// 清理脚本 Provider 测试流程的瞬时状态，不重置请求序号或其他设置页状态。
    pub fn clear_script_provider_transient_state(&mut self) {
        self.script_provider_testing = false;
        self.script_provider_pending_test_request_id = None;
        self.script_provider_test_result = None;
    }

    /// 清理全局热键错误及其对应候选值。
    pub fn clear_global_hotkey_error(&mut self) {
        self.global_hotkey_error = None;
        self.global_hotkey_error_candidate = None;
    }

    /// 成对记录全局热键错误及其对应候选值。
    pub fn record_global_hotkey_error(&mut self, candidate: String, error: GlobalHotkeyError) {
        self.global_hotkey_error = Some(error);
        self.global_hotkey_error_candidate = Some(candidate);
    }
}

/// 设置窗口右侧面板的互斥模态状态。
///
/// `Idle` 是稳态：显示当前 `selected_provider` 的详情面板。其他变体表示用户
/// 触发的非默认交互流（picker / 表单 / 二次确认）。所有变体之间互斥，
/// 切换到任意非 `Idle` 状态时自动取消其他模态。
///
/// 设计要点：
/// - **二次确认（`ConfirmingRemoveProvider` / `ConfirmingDeleteNewApi`）** 与
///   `selected_provider` 绑定，切换 provider 时必须显式回到 `Idle`。
/// - **NewAPI 表单** 用两个变体而非 `Option<NewApiEditData>`：`AddingNewApi`
///   表示新增（空表单），`EditingNewApi(data)` 表示编辑（含回填数据），保证
///   "新增"和"编辑回填数据缺失"在类型层就不可混淆。
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SettingsModalState {
    /// 默认：显示当前 `selected_provider` 的详情面板。
    #[default]
    Idle,
    /// 详情页：对当前 `selected_provider` 的"从 sidebar 移除"二次确认。
    ConfirmingRemoveProvider,
    /// 详情页：对当前 NewAPI `selected_provider` 的"删除 YAML 文件"二次确认。
    ConfirmingDeleteNewApi,
    /// 详情页：对当前脚本 Provider 的"删除配置文件"二次确认。
    ConfirmingDeleteScriptProvider,
    /// 右面板：显示"添加 Provider"选择列表（picker）。
    AddingProvider,
    /// 右面板：NewAPI 新增表单（空表单，提交后会预注册新 provider）。
    AddingNewApi,
    /// 右面板：NewAPI 编辑表单（含从 YAML 读取的回填数据）。
    EditingNewApi(NewApiEditData),
    /// 右面板：脚本 Provider 新增表单。
    AddingScriptProvider,
    /// 右面板：脚本 Provider 编辑表单（含从 YAML 读取的回填数据）。
    EditingScriptProvider(ScriptProviderEditData),
}

impl SettingsModalState {
    /// 当前是否正在展示 NewAPI 表单（新增或编辑）。
    pub fn is_newapi_form(&self) -> bool {
        matches!(self, Self::AddingNewApi | Self::EditingNewApi(_))
    }

    /// 当前是否正在展示 "添加 Provider" 选择列表。
    pub fn is_adding_provider(&self) -> bool {
        matches!(self, Self::AddingProvider)
    }

    /// 是否正在确认"从 sidebar 移除当前 Provider"。
    pub fn is_confirming_remove_provider(&self) -> bool {
        matches!(self, Self::ConfirmingRemoveProvider)
    }

    /// 是否正在确认"删除当前 NewAPI provider"。
    pub fn is_confirming_delete_newapi(&self) -> bool {
        matches!(self, Self::ConfirmingDeleteNewApi)
    }

    /// 是否正在确认"删除当前脚本 provider"。
    pub fn is_confirming_delete_script_provider(&self) -> bool {
        matches!(self, Self::ConfirmingDeleteScriptProvider)
    }

    /// 当前是否正在展示脚本 Provider 表单（新增或编辑）。
    pub fn is_script_provider_form(&self) -> bool {
        matches!(
            self,
            Self::AddingScriptProvider | Self::EditingScriptProvider(_)
        )
    }

    /// 若处于编辑 NewAPI 模式，返回回填数据的引用。
    pub fn newapi_edit_data(&self) -> Option<&NewApiEditData> {
        match self {
            Self::EditingNewApi(data) => Some(data),
            _ => None,
        }
    }

    /// 若处于编辑脚本 Provider 模式，返回回填数据的引用。
    pub fn script_provider_edit_data(&self) -> Option<&ScriptProviderEditData> {
        match self {
            Self::EditingScriptProvider(data) => Some(data),
            _ => None,
        }
    }

    /// 当前表单的稳定身份。
    ///
    /// 用于 view-local 输入缓存判断是否应该复用已有实体：
    /// - 同一 identity 复用，保留用户草稿
    /// - identity 变化时重建，避免把上一个 provider 的输入串到当前表单
    pub fn form_identity(&self) -> Option<FormIdentity> {
        match self {
            Self::AddingNewApi => Some(FormIdentity::NewApiAdd),
            Self::EditingNewApi(data) => Some(FormIdentity::NewApiEdit {
                original_filename: data.original_filename.clone(),
            }),
            Self::AddingScriptProvider => Some(FormIdentity::ScriptProviderAdd),
            Self::EditingScriptProvider(data) => Some(FormIdentity::ScriptProviderEdit {
                original_yaml_filename: data.original_yaml_filename.clone(),
                original_script_filename: data.original_script_filename.clone(),
            }),
            _ => None,
        }
    }
}

/// Debug Tab 的临时 UI 状态（与主设置 UI 解耦）
#[derive(Default)]
pub struct DebugUiState {
    /// 当前选中的调试 Provider
    pub selected_provider: Option<ProviderId>,
    /// Provider 下拉是否展开
    pub provider_dropdown_open: bool,
    /// 是否正在调试刷新中
    pub refresh_active: bool,
    /// 调试刷新前的日志级别（用于刷新完成后恢复）
    pub prev_log_level: Option<log::LevelFilter>,
}

// ============================================================================
// 纯逻辑助手函数
// ============================================================================

/// 头部状态徽章类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderStatusKind {
    Synced,
    Syncing,
    Stale,
    Offline,
}

/// Provider 面板的弹出窗口高度（配额卡片 + 账户信息 / dashboard 行）
///
/// Overview 面板走 `AppSession::popup_height` 的独立分支，因为它的高度取决于
/// session 内的展开记忆，而非单个 Provider 的面板构成。
pub fn compute_popup_height(
    nav: &NavigationState,
    store: &ProviderStore,
    settings: &AppSettings,
) -> f32 {
    let id = match &nav.active_tab {
        NavTab::Provider(id) => id.clone(),
        _ => nav.last_provider_id.clone(),
    };
    let provider = store.find_by_id(&id);
    let quota_count = provider
        .map(|p| {
            let visible = settings.provider.visible_quota_count(&id, &p.quotas);
            if visible == 0 && !p.quotas.is_empty() {
                1 // 全部隐藏时显示空状态，至少预留 1 个卡片高度
            } else {
                visible
            }
        })
        .unwrap_or(1);

    let (show_account, show_dashboard) = provider
        .map(|p| {
            let flags = provider_panel_flags(settings, p);
            (flags.show_account_info, flags.show_dashboard_row)
        })
        .unwrap_or((false, false));

    crate::models::compute_popup_height_detailed(quota_count, show_dashboard, show_account)
}

/// 计算当前头部状态分类和可选的经过秒数
///
/// 返回 `(HeaderStatusKind, Option<elapsed_secs>)`，不做任何文本格式化。
/// 文本翻译和展示格式由 selector 层（`header_view_state`）负责。
pub fn compute_header_status(
    nav: &NavigationState,
    store: &ProviderStore,
) -> (HeaderStatusKind, Option<u64>) {
    let id = match &nav.active_tab {
        NavTab::Provider(id) => id.clone(),
        NavTab::Settings | NavTab::Overview => nav.last_provider_id.clone(),
    };

    let Some(provider) = store.find_by_id(&id) else {
        return (HeaderStatusKind::Offline, None);
    };

    if provider.connection == ConnectionStatus::Refreshing {
        return (HeaderStatusKind::Syncing, None);
    }

    if let Some(instant) = provider.last_refreshed_instant {
        let secs = instant.elapsed().as_secs();
        if secs < 60 {
            (HeaderStatusKind::Synced, Some(secs))
        } else {
            (HeaderStatusKind::Stale, Some(secs))
        }
    } else {
        match provider.connection {
            ConnectionStatus::Error | ConnectionStatus::Disconnected => {
                (HeaderStatusKind::Offline, None)
            }
            _ => (HeaderStatusKind::Syncing, None),
        }
    }
}

#[cfg(test)]
#[path = "state_tests/mod.rs"]
mod tests;
