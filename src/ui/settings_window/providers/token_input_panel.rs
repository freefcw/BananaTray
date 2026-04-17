//! 通用 Token 输入设置面板
//!
//! 从 `SettingsCapability::TokenInput` 的声明字段驱动渲染（OCP）。
//! 任何 provider 只要声明了 `TokenInput` capability，即可自动获得此面板，
//! 无需额外注册或编写 provider-specific UI 代码。

use super::super::SettingsView;
use crate::application::AppAction;
use crate::models::{ProviderId, TokenEditMode, TokenInputCapability};
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::register_input_actions;
use gpui::{
    div, hsla, px, relative, App, AppContext, Context, Div, ElementId, Entity, FocusHandle,
    FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce, Styled,
    Window,
};
use rust_i18n::t;

// ============================================================================
// Token 输入框组件
// ============================================================================

#[derive(IntoElement)]
struct TokenInputBox {
    provider_id: ProviderId,
    input_entity: Entity<adabraka_ui::components::input_state::InputState>,
    theme: Theme,
    focus_handle: FocusHandle,
}

impl RenderOnce for TokenInputBox {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let input_entity = self.input_entity;
        let is_focused = self.focus_handle.is_focused(window);

        let input_div = div()
            .id(ElementId::Name(
                format!("token_input_box_{}", self.provider_id.id_key()).into(),
            ))
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

        register_input_actions(input_div, &input_entity, window).child(
            div()
                .flex_1()
                .overflow_hidden()
                .text_size(px(13.0))
                .child(input_entity),
        )
    }
}

// ============================================================================
// 通用渲染入口
// ============================================================================

/// 渲染 Token 输入型设置面板，完全从 `SettingsCapability::TokenInput` 字段驱动。
///
/// 此函数是 TokenInput 类型 provider 的唯一渲染入口。新增 TokenInput provider 时
/// 只需在 `AiProvider::settings_capability()` 返回正确字段，无需编写额外 UI 代码。
pub(crate) fn render_token_input_panel(
    provider_id: &ProviderId,
    capability: TokenInputCapability,
    view: &mut SettingsView,
    theme: &Theme,
    cx: &mut Context<SettingsView>,
) -> Div {
    let TokenInputCapability {
        placeholder_i18n_key,
        help_tip_i18n_key,
        title_i18n_key,
        description_i18n_key,
        create_url,
        ..
    } = capability;

    // 统一通过 ProviderManager 解析运行时 token 展示状态。
    // manager 会优先走 provider 自定义逻辑，必要时自动回落到通用 credential 存储。
    let display_info = {
        let state = view.state.borrow();
        state.manager.snapshot().resolve_token_input_state(
            provider_id,
            capability,
            &state.session.settings,
        )
    };

    let has_token = display_info.has_token;

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

    // ── 标题 + 帮助图标 ──
    let hover_color = theme.text.primary;
    let help_icon = crate::ui::with_multiline_tooltip(
        "token-input-help",
        &t!(help_tip_i18n_key),
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
                    .child(t!(title_i18n_key).to_string()),
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
            .child(t!(description_i18n_key).to_string()),
    );

    // ── Token 状态区 or 输入框 ──
    let is_editing = view
        .state
        .borrow()
        .session
        .settings_ui
        .token_editing_provider
        .as_ref()
        .is_some_and(|id| id == provider_id);

    if is_editing {
        // 编辑模式：创建输入框
        view.token_input = Some(cx.new(|cx| {
            let mut state = adabraka_ui::components::input_state::InputState::new(cx);
            state.placeholder = t!(placeholder_i18n_key).to_string().into();
            state
        }));

        let input_entity = view.token_input.as_ref().unwrap().clone();
        let focus_handle = input_entity.read(cx).focus_handle(cx);

        card = card.child(TokenInputBox {
            provider_id: provider_id.clone(),
            input_entity,
            theme: theme.clone(),
            focus_handle,
        });
    } else if has_token {
        // Token 已配置状态
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
                        .child(t!("settings.token.configured").to_string()),
                ),
        );
    } else {
        // Token 未配置提示
        card = card.child(
            div().h(px(40.0)).flex().items_center().child(
                div()
                    .text_size(px(12.0))
                    .line_height(relative(1.5))
                    .text_color(theme.text.muted)
                    .child(t!("settings.token.hint").to_string()),
            ),
        );
    }

    // ── Token 来源信息行（由 provider 解析器提供 source_i18n_key） ──
    let (source_info, text_color) = if !is_editing && has_token {
        if let Some(source_i18n_key) = display_info.source_i18n_key {
            let masked = display_info.masked.as_deref().unwrap_or_default();
            (
                t!(
                    "settings.token.via",
                    masked = masked,
                    source = t!(source_i18n_key).to_string()
                )
                .to_string(),
                theme.text.muted,
            )
        } else if let Some(masked) = &display_info.masked {
            (masked.clone(), theme.text.muted)
        } else {
            ("placeholder".to_string(), theme.bg.card_inner)
        }
    } else {
        // 编辑模式或未配置时，使用占位字符实现"隐形"站位
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
    card = card.child(render_token_action_buttons(
        provider_id.clone(),
        create_url,
        view,
        display_info.edit_mode,
        theme,
        is_editing,
    ));

    card
}

// ============================================================================
// 操作按钮
// ============================================================================

fn render_token_action_buttons(
    provider_id: ProviderId,
    create_url: &'static str,
    view: &mut SettingsView,
    edit_mode: TokenEditMode,
    theme: &Theme,
    is_editing: bool,
) -> Div {
    let mut row = div().flex().gap(px(10.0)).mt(px(2.0));

    // ── 左侧按钮：编辑模式=保存，浏览模式=创建 Token ──
    let left_label = if is_editing {
        t!("settings.token.save").to_string()
    } else {
        t!("settings.token.create").to_string()
    };

    let input_entity_opt = view.token_input.clone();
    let state_left = view.state.clone();
    let left_provider_id = provider_id.clone();

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
                    if let Some(entity) = &input_entity_opt {
                        let text = entity.read(cx).content().trim().to_string();
                        runtime::dispatch_in_window(
                            &state_left,
                            AppAction::SaveProviderToken {
                                provider_id: left_provider_id.clone(),
                                token: text,
                            },
                            window,
                            cx,
                        );
                    }
                } else {
                    runtime::dispatch_in_window(
                        &state_left,
                        AppAction::OpenUrl(create_url.to_string()),
                        window,
                        cx,
                    );
                }
            }),
    );

    // ── 右侧按钮：编辑模式=取消，浏览模式=设置/修改 ──
    let right_label = if is_editing {
        t!("settings.token.cancel").to_string()
    } else if edit_mode == TokenEditMode::EditStored {
        t!("settings.token.edit").to_string()
    } else {
        t!("settings.token.set").to_string()
    };

    let state_right = view.state.clone();

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
                    &state_right,
                    AppAction::SetTokenEditing {
                        provider_id: provider_id.clone(),
                        editing: !is_editing,
                    },
                    window,
                    cx,
                );
            }),
    );

    row
}
