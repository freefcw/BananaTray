mod about_tab;
mod components;
mod debug_tab;
mod display_tab;
mod general_tab;
mod providers;
use crate::application::AppAction;
use crate::application::FormIdentity;
use crate::application::SettingsTab;
use crate::models::{
    format_optional_divisor_value, ProviderId, TokenEditMode, TokenInputCapability,
};
use crate::runtime;
use crate::runtime::AppState;
use crate::theme::Theme;
use crate::ui::widgets::render_svg_icon;
use adabraka_ui::components::hotkey_input::{HotkeyInputState, HotkeyValue};
use adabraka_ui::components::input_state::InputState;
use adabraka_ui::components::textarea_state::TextareaState;
use gpui::{
    div, linear_color_stop, multi_stop_linear_gradient, px, rgba, svg, transparent_black,
    AnyElement, App, AppContext, Context, Div, Entity, Focusable, FontWeight, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Render, StatefulInteractiveElement, Styled,
    Subscription, Window, WindowAppearance,
};
use log::info;
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) fn build_settings_view(
    state: Rc<RefCell<AppState>>,
    cx: &mut App,
) -> Entity<SettingsView> {
    cx.new(|cx| SettingsView::new(state, cx))
}

#[allow(dead_code)]
pub(crate) fn register_shell_hooks() {
    crate::bootstrap::register_build_settings_view(build_settings_view);
}

// ============================================================================
// 设置视图 — 匹配 Lumina Bar 设计稿
// ============================================================================

/// NewAPI 表单输入状态（使用 adabraka-ui InputState，支持鼠标选择、光标闪烁等）
#[derive(Clone)]
pub(crate) struct NewApiFormInputs {
    pub name: Entity<InputState>,
    pub url: Entity<InputState>,
    pub cookie: Entity<TextareaState>,
    pub user_id: Entity<InputState>,
    pub divisor: Entity<InputState>,
}

/// Script Provider 表单输入状态。
pub(crate) struct ScriptProviderFormInputs {
    pub name: Entity<InputState>,
    pub provider_id: Entity<InputState>,
    pub interpreter: Entity<InputState>,
    pub timeout: Entity<InputState>,
    pub script: Entity<TextareaState>,
}

pub(crate) struct FormInputsCache<T> {
    pub identity: FormIdentity,
    pub inputs: T,
}

impl ScriptProviderFormInputs {
    pub fn new_add(cx: &mut Context<SettingsView>) -> Self {
        Self {
            name: cx.new(|cx| {
                let mut s = InputState::new(cx);
                s.placeholder = t!("script_provider.field.name.placeholder")
                    .to_string()
                    .into();
                s.trim_on_blur = false;
                s
            }),
            provider_id: cx.new(|cx| {
                let mut s = InputState::new(cx);
                s.placeholder = "ccswitch:script".to_string().into();
                s.trim_on_blur = false;
                s
            }),
            interpreter: cx.new(|cx| {
                let mut s = InputState::new(cx);
                s.content = crate::models::DEFAULT_SCRIPT_INTERPRETER.into();
                s.trim_on_blur = false;
                s
            }),
            timeout: cx.new(|cx| {
                let mut s = InputState::new(cx);
                s.content = (crate::models::DEFAULT_SCRIPT_TIMEOUT_MS / 1000)
                    .to_string()
                    .into();
                s.trim_on_blur = false;
                s
            }),
            script: cx.new(|cx| {
                let mut s = TextareaState::new(cx);
                s.content = crate::providers::custom::api::default_script_template().into();
                s
            }),
        }
    }

    pub fn new_edit(
        data: &crate::models::ScriptProviderEditData,
        cx: &mut Context<SettingsView>,
    ) -> Self {
        Self {
            name: cx.new(|cx| {
                let mut s = InputState::new(cx);
                s.content = data.display_name.clone().into();
                s.trim_on_blur = false;
                s
            }),
            provider_id: cx.new(|cx| {
                let mut s = InputState::new(cx);
                s.content = data.provider_id.clone().into();
                s.trim_on_blur = false;
                s
            }),
            interpreter: cx.new(|cx| {
                let mut s = InputState::new(cx);
                s.content = data.interpreter.clone().into();
                s.trim_on_blur = false;
                s
            }),
            timeout: cx.new(|cx| {
                let mut s = InputState::new(cx);
                s.content = (data.timeout_ms / 1000).to_string().into();
                s.trim_on_blur = false;
                s
            }),
            script: cx.new(|cx| {
                let mut s = TextareaState::new(cx);
                s.content = data.script.clone().into();
                s
            }),
        }
    }

    pub fn focused_states(&self, window: &Window, cx: &App) -> [bool; 5] {
        [
            self.name.read(cx).focus_handle(cx).is_focused(window),
            self.provider_id
                .read(cx)
                .focus_handle(cx)
                .is_focused(window),
            self.interpreter
                .read(cx)
                .focus_handle(cx)
                .is_focused(window),
            self.timeout.read(cx).focus_handle(cx).is_focused(window),
            self.script.read(cx).focus_handle(cx).is_focused(window),
        ]
    }
}

impl NewApiFormInputs {
    /// 新增模式：创建空表单
    pub fn new_add(cx: &mut Context<SettingsView>) -> Self {
        Self {
            name: newapi_input(cx, t!("newapi.field.name.placeholder").to_string(), ""),
            url: newapi_input(cx, t!("newapi.field.url.placeholder").to_string(), ""),
            cookie: newapi_textarea(cx, t!("newapi.field.cookie.placeholder").to_string(), ""),
            user_id: newapi_input(cx, t!("newapi.field.user_id.placeholder").to_string(), ""),
            divisor: newapi_input(cx, t!("newapi.field.divisor.placeholder").to_string(), ""),
        }
    }

    /// 编辑模式：用已有数据预填表单
    pub fn new_edit(data: &crate::models::NewApiEditData, cx: &mut Context<SettingsView>) -> Self {
        Self {
            name: newapi_input(
                cx,
                t!("newapi.field.name.placeholder").to_string(),
                data.display_name.clone(),
            ),
            url: newapi_input(
                cx,
                t!("newapi.field.url.placeholder").to_string(),
                data.base_url.clone(),
            ),
            cookie: newapi_textarea(
                cx,
                t!("newapi.field.cookie.placeholder").to_string(),
                data.cookie.clone(),
            ),
            user_id: newapi_input(
                cx,
                t!("newapi.field.user_id.placeholder").to_string(),
                data.user_id.clone().unwrap_or_default(),
            ),
            divisor: newapi_input(
                cx,
                t!("newapi.field.divisor.placeholder").to_string(),
                format_optional_divisor_value(data.divisor),
            ),
        }
    }

    /// 返回每个字段是否获得焦点的数组
    pub fn focused_states(&self, window: &Window, cx: &App) -> [bool; 5] {
        [
            self.name.read(cx).focus_handle(cx).is_focused(window),
            self.url.read(cx).focus_handle(cx).is_focused(window),
            self.cookie.read(cx).focus_handle(cx).is_focused(window),
            self.user_id.read(cx).focus_handle(cx).is_focused(window),
            self.divisor.read(cx).focus_handle(cx).is_focused(window),
        ]
    }
}

fn newapi_input(
    cx: &mut Context<SettingsView>,
    placeholder: String,
    content: impl Into<String>,
) -> Entity<InputState> {
    cx.new(|cx| {
        let mut state = InputState::new(cx);
        state.placeholder = placeholder.into();
        state.content = content.into().into();
        state.trim_on_blur = false;
        state
    })
}

fn newapi_textarea(
    cx: &mut Context<SettingsView>,
    placeholder: String,
    content: impl Into<String>,
) -> Entity<TextareaState> {
    cx.new(|cx| {
        let mut state = TextareaState::new(cx);
        state.placeholder = placeholder.into();
        state.content = content.into().into();
        state
    })
}

/// Token 输入框的 view-local 草稿状态。
///
/// 仅在用户进入编辑时创建，保存 / 取消或离开当前 provider 入口后清理。
pub(crate) struct TokenInputDraft {
    pub provider_id: ProviderId,
    pub input: Entity<InputState>,
}

pub(crate) struct SettingsView {
    pub(crate) state: Rc<RefCell<AppState>>,
    /// 当前交互设置面板的 Token 输入框（通用，不绑定特定 provider）
    pub(crate) token_input: Option<TokenInputDraft>,
    /// General Tab 全局热键捕获输入框
    pub(crate) global_hotkey_input: Option<Entity<HotkeyInputState>>,
    /// 上次同步进捕获控件的已保存热键值，用来避免覆盖用户尚未保存的录制结果
    pub(crate) global_hotkey_synced_value: Option<String>,
    /// 监听系统深色模式变化，自动切换主题
    pub(crate) _appearance_sub: Option<Subscription>,
    /// NewAPI 快速添加表单输入组（identity 变化时重建）
    pub(crate) newapi_inputs: Option<FormInputsCache<NewApiFormInputs>>,
    /// Script Provider 表单输入组（identity 变化时重建）
    pub(crate) script_provider_inputs: Option<FormInputsCache<ScriptProviderFormInputs>>,
    /// Debug Tab 的阻塞式系统诊断缓存；渲染阶段只读取该快照。
    pub(crate) debug_diagnostics: Option<runtime::DebugDiagnostics>,
    pub(crate) debug_diagnostics_loading: bool,
    pub(crate) _debug_diagnostics_task: Option<gpui::Task<()>>,
}

impl SettingsView {
    #[allow(dead_code)]
    pub(crate) fn new(state: Rc<RefCell<AppState>>, cx: &mut Context<Self>) -> Self {
        info!(target: "settings", "constructing settings view");
        // 新窗口实例没有 view-local 草稿，清除前一个窗口可能残留的编辑标记
        state
            .borrow_mut()
            .session
            .settings_ui
            .token_editing_provider = None;
        let load_debug_diagnostics =
            state.borrow().session.settings_ui.active_tab == SettingsTab::Debug;
        let mut view = Self {
            state,
            token_input: None,
            global_hotkey_input: None,
            global_hotkey_synced_value: None,
            _appearance_sub: None,
            newapi_inputs: None,
            script_provider_inputs: None,
            debug_diagnostics: None,
            debug_diagnostics_loading: false,
            _debug_diagnostics_task: None,
        };
        if load_debug_diagnostics {
            view.refresh_debug_diagnostics(cx);
        }
        view
    }

    pub(super) fn ensure_global_hotkey_input(
        &mut self,
        saved_hotkey: &str,
        cx: &mut Context<Self>,
    ) -> Entity<HotkeyInputState> {
        let input = match &self.global_hotkey_input {
            Some(input) => input.clone(),
            None => self.create_global_hotkey_input(saved_hotkey, cx),
        };

        if self.global_hotkey_synced_value.as_deref() != Some(saved_hotkey) {
            let synced_hotkey = hotkey_value_from_saved_hotkey(saved_hotkey);
            input.update(cx, |input, cx| {
                input.set_hotkey(synced_hotkey.clone(), cx);
            });
            self.global_hotkey_synced_value = Some(saved_hotkey.to_string());
        }

        input
    }

    fn create_global_hotkey_input(
        &mut self,
        saved_hotkey: &str,
        cx: &mut Context<Self>,
    ) -> Entity<HotkeyInputState> {
        let initial_hotkey = hotkey_value_from_saved_hotkey(saved_hotkey);
        let input = cx.new(|cx| match initial_hotkey {
            Some(hotkey) => HotkeyInputState::with_hotkey(cx, hotkey),
            None => HotkeyInputState::new(cx),
        });
        self.global_hotkey_input = Some(input.clone());
        self.global_hotkey_synced_value = Some(saved_hotkey.to_string());
        input
    }

    pub(in crate::ui::settings_window) fn begin_token_input(
        &mut self,
        provider_id: &ProviderId,
        capability: TokenInputCapability,
        edit_mode: TokenEditMode,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        // 同一 provider 重新进入编辑时复用已有草稿，避免丢失用户输入
        if let Some(draft) = &self.token_input {
            if &draft.provider_id == provider_id {
                return draft.input.clone();
            }
        }
        let placeholder = t!(capability.placeholder_i18n_key).to_string();
        let initial_value = if edit_mode == TokenEditMode::EditStored {
            self.state
                .borrow()
                .session
                .settings
                .provider
                .credentials
                .get_credential(capability.credential_key)
                .map(str::to_string)
        } else {
            None
        };
        let input = cx.new(|cx| {
            let mut state = InputState::new(cx);
            state.placeholder = placeholder.into();
            state.content = initial_value.unwrap_or_default().into();
            state.trim_on_blur = false;
            state
        });
        self.token_input = Some(TokenInputDraft {
            provider_id: provider_id.clone(),
            input: input.clone(),
        });
        input
    }

    pub(in crate::ui::settings_window) fn clear_token_input(&mut self) {
        self.token_input = None;
    }

    /// 根据用户主题设置 + 窗口外观解析设置窗口主题
    pub(super) fn resolve_theme(
        state: &std::cell::RefCell<AppState>,
        appearance: WindowAppearance,
    ) -> Theme {
        let user_theme = state.borrow().session.settings.display.theme;
        Theme::resolve_for_settings(user_theme, appearance)
    }

    // ========================================================================
    // 自定义头部：图标 + "Settings" + ✕ 关闭按钮
    // ========================================================================

    fn render_header(&self, theme: &Theme) -> Div {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(20.0))
            .pt(px(20.0))
            .pb(px(12.0))
            // 左侧：图标 + 标题
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    // 网格图标
                    .child(
                        div()
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.0))
                            .bg(theme.bg.subtle)
                            .child(render_svg_icon(
                                "src/icons/overview.svg",
                                px(18.0),
                                theme.text.accent,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text.primary)
                            .child(t!("settings.title").to_string()),
                    ),
            )
            // 右侧：关闭按钮
            .child(
                div()
                    .w(px(28.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg.subtle))
                    .child(render_svg_icon(
                        "src/icons/close.svg",
                        px(14.0),
                        theme.text.muted,
                    ))
                    .on_mouse_down(MouseButton::Left, |_, window, _| {
                        window.remove_window();
                    }),
            );

        // Linux 无标题栏，在头部区域启用窗口拖拽
        #[cfg(target_os = "linux")]
        let header = header.cursor(gpui::CursorStyle::OpenHand).on_mouse_down(
            MouseButton::Left,
            |_, window, _| {
                window.start_window_move();
            },
        );

        header
    }

    // ========================================================================
    // Tab 导航栏：水平 pill 风格
    // ========================================================================

    fn render_tab_bar(
        &self,
        active_tab: SettingsTab,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let show_debug = self.state.borrow().session.settings.display.show_debug_tab;
        let view_entity = cx.entity().clone();
        settings_tabs(show_debug).into_iter().fold(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(2.0))
                .px(px(16.0))
                .pb(px(10.0))
                .overflow_hidden(),
            |bar, (icon, label, tab)| {
                bar.child(self.render_tab_button(
                    icon,
                    label,
                    tab,
                    active_tab == tab,
                    &view_entity,
                    theme,
                ))
            },
        )
    }

    fn render_tab_button(
        &self,
        icon: &'static str,
        label: String,
        tab: SettingsTab,
        is_active: bool,
        view_entity: &Entity<Self>,
        theme: &Theme,
    ) -> AnyElement {
        let state = self.state.clone();
        let tab_view_entity = view_entity.clone();
        let hover_background = theme.bg.subtle;
        let (background, foreground, border) = if is_active {
            (
                theme.nav.pill_active_bg,
                theme.nav.pill_active_text,
                theme.nav.pill_active_bg,
            )
        } else {
            (transparent_black(), theme.text.muted, transparent_black())
        };

        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(8.0))
            .bg(background)
            .border_1()
            .border_color(border)
            .cursor_pointer()
            .hover(move |style| {
                if is_active {
                    style
                } else {
                    style.bg(hover_background)
                }
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(14.0))
                    .text_color(foreground)
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(foreground)
                    .whitespace_nowrap()
                    .child(label),
            )
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                tab_view_entity.update(cx, |view, cx| {
                    view.clear_token_input();
                    if tab == SettingsTab::Debug {
                        view.refresh_debug_diagnostics(cx);
                    }
                });
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::SetSettingsTab(tab),
                    window,
                    cx,
                );
            })
            .into_any_element()
    }
}

fn settings_tabs(show_debug: bool) -> Vec<(&'static str, String, SettingsTab)> {
    let mut tabs = vec![
        (
            "src/icons/settings.svg",
            t!("settings.tab.general").to_string(),
            SettingsTab::General,
        ),
        (
            "src/icons/overview.svg",
            t!("settings.tab.providers").to_string(),
            SettingsTab::Providers,
        ),
        (
            "src/icons/display.svg",
            t!("settings.tab.display").to_string(),
            SettingsTab::Display,
        ),
        (
            "src/icons/about.svg",
            t!("settings.tab.about").to_string(),
            SettingsTab::About,
        ),
    ];
    if show_debug {
        tabs.push((
            "src/icons/advanced.svg",
            t!("settings.tab.debug").to_string(),
            SettingsTab::Debug,
        ));
    }
    tabs
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Self::resolve_theme(&self.state, window.appearance());
        let active_tab = self.state.borrow().session.settings_ui.active_tab;
        let settings = self.state.borrow().session.settings.clone();
        let viewport = window.viewport_size();

        // ── Content area ─────────────
        // 头部 + tab 合计约 100px
        let content_h = viewport.height - px(100.0);

        let content = if active_tab == SettingsTab::Providers {
            div()
                .id("settings-content-providers")
                .flex_col()
                .h(content_h)
                .overflow_hidden()
                .child(self.render_providers_tab(&theme, window, cx))
        } else {
            div()
                .id("settings-content")
                .flex_col()
                .h(content_h)
                .overflow_y_scroll()
                .child(match active_tab {
                    SettingsTab::General => self.render_general_tab(&settings, &theme, window, cx),
                    SettingsTab::Display => self.render_display_tab(&settings, &theme),
                    SettingsTab::About => self.render_about_tab(&theme),
                    SettingsTab::Debug => self.render_debug_tab(&theme, cx),
                    _ => div(),
                })
        };

        // ── 整体布局 ──
        // 暗色背景 + 圆角 + 模拟发光边缘
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg.base)
            .text_color(theme.text.primary)
            .rounded(px(14.0))
            .overflow_hidden()
            // 顶部微光效果 (amber glow)
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .h(px(2.0))
                    .bg(multi_stop_linear_gradient(
                        90.,
                        &[
                            linear_color_stop(transparent_black(), 0.),
                            linear_color_stop(rgba(0xff8c0040), 0.3),
                            linear_color_stop(rgba(0xff6b0060), 0.5),
                            linear_color_stop(rgba(0xff8c0040), 0.7),
                            linear_color_stop(transparent_black(), 1.),
                        ],
                    )),
            )
            // 头部
            .child(self.render_header(&theme))
            // Tab 栏
            .child(self.render_tab_bar(active_tab, &theme, cx))
            // Tab 栏与内容区分隔线
            .child(div().w_full().h(px(1.0)).bg(theme.border.subtle))
            // 内容区
            .child(content)
    }
}

fn hotkey_value_from_saved_hotkey(saved_hotkey: &str) -> Option<HotkeyValue> {
    runtime::global_hotkey::parse_hotkey_string(saved_hotkey)
        .ok()
        .map(|keystroke| HotkeyValue::new(keystroke.key, keystroke.modifiers))
}
