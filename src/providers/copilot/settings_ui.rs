//! Copilot provider 的 Settings UI 渲染
//!
//! 支持交互式 Token 配置，匹配设计稿卡片样式。

use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;

use super::CopilotTokenSource;

use crate::application::AppAction;
use crate::runtime;
use crate::ui::settings_window::SettingsView;
use crate::ui::widgets::register_input_actions;

#[derive(IntoElement)]
struct CopilotInputBox {
    input_entity: Entity<adabraka_ui::components::input_state::InputState>,
    theme: Theme,
    focus_handle: FocusHandle,
}

impl RenderOnce for CopilotInputBox {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let input_entity = self.input_entity;
        let is_focused = self.focus_handle.is_focused(window);

        let input_div = div()
            .id("custom_copilot_input")
            .track_focus(&self.focus_handle)
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(14.0))
            .py(px(10.0))
            .h(px(40.0))
            .rounded(px(8.0))
            .bg(hsla(145.0 / 360.0, 0.6, 0.3, 0.15))
            .border_1()
            .border_color(if is_focused {
                theme.status.success
            } else {
                hsla(145.0 / 360.0, 0.6, 0.4, 0.35)
            })
            .text_color(theme.status.success)
            .on_mouse_down(MouseButton::Left, {
                let handle = self.focus_handle.clone();
                move |_, window, _| handle.focus(window)
            });

        // 使用辅助函数注册所有键盘事件
        register_input_actions(input_div, &input_entity, window).child(
            div()
                .flex_1()
                .overflow_hidden()
                .text_size(px(13.0))
                .child(input_entity),
        )
    }
}

/// 渲染 Copilot 设置 UI（带交互，用于设置窗口）
/// 设计稿：深色卡片容器 → 标题+描述 → 状态徽章 → token 信息 → 操作按钮
pub(crate) fn render_settings_interactive(
    view: &mut SettingsView,
    theme: &Theme,
    cx: &mut Context<SettingsView>,
) -> Div {
    // 获取当前 token 状态
    // resolve_token 使用基于时间的缓存（5秒有效期），避免频繁的文件 I/O
    let settings = view.state.borrow().session.settings.clone();
    let mem_token = settings.provider.credentials.github_token.as_deref();
    let status = super::resolve_token(mem_token);

    let has_token = status.token.is_some();
    let masked = status.masked();
    let source = status.source;

    // ── 外层深色卡片容器 ──
    let mut card = div()
        .flex_col()
        .w_full()
        .rounded(px(12.0))
        .bg(theme.bg.card_inner)
        .border_1()
        .border_color(theme.border.strong)
        .px(px(20.0))
        .py(px(20.0))
        .gap(px(14.0));

    let hover_color = theme.text.primary;
    let help_icon = crate::ui::with_multiline_tooltip(
        "copilot-token-help",
        &t!("copilot.token_sources_tip"),
        theme,
        div()
            .flex()
            .items_center()
            .justify_center()
            .w(px(18.0))
            .h(px(18.0))
            .rounded(px(9.0))
            .bg(theme.bg.subtle)
            .text_size(px(11.0))
            .font_weight(FontWeight::BOLD)
            .text_color(theme.text.muted)
            .cursor_pointer()
            .hover(move |s| s.text_color(hover_color))
            .child("?"),
    );

    card = card.child(
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text.primary)
                    .child(t!("copilot.github_login").to_string()),
            )
            .child(help_icon),
    );

    // ── 描述 ──
    card = card.child(
        div()
            .text_size(px(12.5))
            .line_height(relative(1.4))
            .text_color(theme.text.secondary)
            .py(px(4.0))
            .child(t!("copilot.requires_auth").to_string()),
    );

    // ── Token 状态区 or 输入框 ──
    let is_editing = view
        .state
        .borrow()
        .session
        .settings_ui
        .copilot_token_editing;

    // ── Token 状态区 or 输入框 ──
    if is_editing {
        // 编辑模式：每次都重新创建 InputState
        view.copilot_input = Some(cx.new(|cx| {
            let mut state = adabraka_ui::components::input_state::InputState::new(cx);
            state.placeholder = "粘贴或输入 GitHub Token (ghp_...)".into();
            state
        }));

        let input_entity = view.copilot_input.as_ref().unwrap().clone();
        let focus_handle = input_entity.read(cx).focus_handle(cx);

        card = card.child(CopilotInputBox {
            input_entity,
            theme: theme.clone(),
            focus_handle,
        });
    } else if has_token {
        // 默认模式：Token 已配置状态
        card = card.child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(14.0))
                .py(px(10.0))
                .h(px(40.0))
                .rounded(px(8.0))
                .bg(hsla(145.0 / 360.0, 0.6, 0.3, 0.15))
                .border_1()
                .border_color(hsla(145.0 / 360.0, 0.6, 0.4, 0.35))
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(theme.status.success)
                        .child("✓"),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.status.success)
                        .child(t!("copilot.token_configured").to_string()),
                ),
        );
    } else {
        // 默认模式：Token 未配置提示
        card = card.child(
            div()
                .h(px(40.0)) // 与 InputBox 高度大致对齐，保持一致性
                .flex()
                .items_center()
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(relative(1.5))
                        .text_color(theme.text.muted)
                        .child(t!("copilot.token_hint").to_string()),
                ),
        );
    }

    // ── Token 来源信息行 (始终站位，保持布局稳定) ──
    let (source_info, text_color) = if !is_editing && has_token {
        let source_label = match source {
            CopilotTokenSource::ConfigFile => t!("copilot.source.config_file").to_string(),
            CopilotTokenSource::CopilotOAuth => t!("copilot.source.copilot_oauth").to_string(),
            CopilotTokenSource::CopilotCli => t!("copilot.source.copilot_cli").to_string(),
            CopilotTokenSource::EnvVar => t!("copilot.source.env_var").to_string(),
            CopilotTokenSource::None => String::new(),
        };

        (
            t!(
                "copilot.token_via",
                masked = masked.unwrap_or_default(),
                source = &source_label
            )
            .to_string(),
            theme.text.muted,
        )
    } else {
        // 编辑模式或未配置时，使用占位字符并设置颜色与背景一致，实现“隐形”站位
        ("placeholder".to_string(), theme.bg.card_inner)
    };

    card = card.child(
        div().py(px(6.0)).child(
            div()
                .text_size(px(12.0))
                .text_color(text_color)
                .child(source_info),
        ),
    );

    // ── 操作按钮 ──
    card = card.child(render_action_buttons(
        view, has_token, source, theme, is_editing,
    ));

    card
}

fn render_action_buttons(
    view: &mut SettingsView,
    has_token: bool,
    source: CopilotTokenSource,
    theme: &Theme,
    is_editing: bool,
) -> Div {
    let is_user_configured = source == CopilotTokenSource::ConfigFile;

    let mut row = div().flex().gap(px(10.0)).mt(px(2.0));

    // ── 左侧按钮 (原：创建 Token) ──
    let left_label = if is_editing {
        t!("copilot.save_token").to_string()
    } else {
        t!("copilot.create_token").to_string()
    };

    // 编辑模式下用 input 的实体内容，否则跳链
    let input_entity_opt = view.copilot_input.clone();
    let state_left_click = view.state.clone();

    row = row.child(
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .px(px(16.0))
            .py(px(10.0))
            .rounded(px(8.0))
            .bg(theme.text.accent)
            .text_size(px(13.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.element.active)
            .cursor_pointer()
            .hover(|s| s.opacity(0.9))
            .child(left_label)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                if is_editing {
                    // 保存操作
                    if let Some(entity) = &input_entity_opt {
                        let text = entity.read(cx).content().trim().to_string();
                        runtime::dispatch_in_window(
                            &state_left_click,
                            AppAction::SaveCopilotToken(text),
                            window,
                            cx,
                        );
                    }
                } else {
                    runtime::dispatch_in_window(
                        &state_left_click,
                        AppAction::OpenUrl(
                            "https://github.com/settings/personal-access-tokens".to_string(),
                        ),
                        window,
                        cx,
                    );
                }
            }),
    );

    // ── 右侧按钮 (原：修改/设置 Token) ──
    let right_label = if is_editing {
        t!("copilot.cancel_setup").to_string()
    } else if has_token && is_user_configured {
        t!("copilot.edit_token").to_string()
    } else {
        t!("copilot.set_token").to_string()
    };

    let state_right_click = view.state.clone();

    row = row.child(
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .px(px(16.0))
            .py(px(10.0))
            .rounded(px(8.0))
            .bg(theme.bg.subtle)
            .border_1()
            .border_color(theme.border.strong)
            .text_size(px(13.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text.primary)
            .cursor_pointer()
            .hover(|s| s.opacity(0.9))
            .child(right_label)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                runtime::dispatch_in_window(
                    &state_right_click,
                    AppAction::SetCopilotTokenEditing(!is_editing),
                    window,
                    cx,
                );
            }),
    );

    row
}
