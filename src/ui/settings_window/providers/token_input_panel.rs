//! 通用 Token 输入设置面板
//!
//! 从 `SettingsCapability::TokenInput` 的声明字段驱动渲染（OCP）。
//! 任何 provider 只要声明了 `TokenInput` capability，即可自动获得此面板，
//! 无需额外注册或编写 provider-specific UI 代码。
//! 草稿由 `SettingsView::begin_token_input()` 创建，并在保存 / 取消 / 导航离开时清理。

use super::super::SettingsView;
use crate::application::AppAction;
use crate::models::{ProviderId, TokenEditMode, TokenInputCapability, TokenInputState};
use crate::theme::Theme;
use crate::ui::widgets::register_input_actions;
use gpui::{
    div, hsla, px, relative, AnyElement, App, Context, Div, ElementId, Entity, FocusHandle,
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
            .key_context("Input")
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
/// 只需在 `ProviderCapabilities::settings_capability()` 返回正确字段，无需编写额外 UI 代码。
pub(crate) fn render_token_input_panel(
    provider_id: &ProviderId,
    capability: TokenInputCapability,
    view: &mut SettingsView,
    theme: &Theme,
    cx: &mut Context<SettingsView>,
) -> Div {
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
    let mut card = token_panel_card(theme)
        .child(render_token_panel_header(capability, theme))
        .child(render_token_panel_description(capability, theme));

    // ── Token 状态区 or 输入框 ──
    let editing_provider_matches = view
        .state
        .borrow()
        .session
        .settings_ui
        .token_editing_provider
        .as_ref()
        .is_some_and(|id| id == provider_id);
    let token_input_entity = view
        .token_input
        .as_ref()
        .filter(|draft| &draft.provider_id == provider_id)
        .map(|draft| draft.input.clone());
    let is_editing = editing_provider_matches && token_input_entity.is_some();

    if editing_provider_matches && token_input_entity.is_none() {
        log::warn!(
            target: "settings",
            "token editing requested for {} but no matching input draft exists; rendering read-only state",
            provider_id
        );
    }

    card = card
        .child(render_token_value(
            provider_id,
            token_input_entity,
            has_token,
            is_editing,
            theme,
            cx,
        ))
        .child(render_token_source(&display_info, is_editing, theme));

    // ── 操作按钮 ──
    card = card.child(render_token_action_buttons(
        provider_id.clone(),
        capability,
        view,
        display_info.edit_mode,
        theme,
        is_editing,
        cx,
    ));

    card
}

fn token_panel_card(theme: &Theme) -> Div {
    div()
        .flex_col()
        .w_full()
        .rounded(px(12.0))
        .bg(theme.bg.card_inner)
        .border_1()
        .border_color(theme.border.strong)
        .px(px(20.0))
        .py(px(20.0))
        .gap(px(14.0))
}

fn render_token_panel_header(capability: TokenInputCapability, theme: &Theme) -> Div {
    let hover_color = theme.text.primary;
    let help_icon = crate::ui::with_multiline_tooltip(
        "token-input-help",
        &t!(capability.help_tip_i18n_key),
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
            .hover(move |style| style.text_color(hover_color))
            .child("?"),
    );

    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(FontWeight::BOLD)
                .text_color(theme.text.primary)
                .child(t!(capability.title_i18n_key).to_string()),
        )
        .child(help_icon)
}

fn render_token_panel_description(capability: TokenInputCapability, theme: &Theme) -> Div {
    div()
        .text_size(px(12.5))
        .line_height(relative(1.4))
        .text_color(theme.text.secondary)
        .py(px(4.0))
        .child(t!(capability.description_i18n_key).to_string())
}

fn render_token_value(
    provider_id: &ProviderId,
    input_entity: Option<Entity<adabraka_ui::components::input_state::InputState>>,
    has_token: bool,
    is_editing: bool,
    theme: &Theme,
    cx: &App,
) -> AnyElement {
    if let (true, Some(input_entity)) = (is_editing, input_entity) {
        let focus_handle = input_entity.read(cx).focus_handle(cx);
        return TokenInputBox {
            provider_id: provider_id.clone(),
            input_entity,
            theme: theme.clone(),
            focus_handle,
        }
        .into_any_element();
    }
    if has_token {
        return render_configured_token(theme).into_any_element();
    }

    div()
        .h(px(40.0))
        .flex()
        .items_center()
        .child(
            div()
                .text_size(px(12.0))
                .line_height(relative(1.5))
                .text_color(theme.text.muted)
                .child(t!("settings.token.hint").to_string()),
        )
        .into_any_element()
}

fn render_configured_token(theme: &Theme) -> Div {
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
        )
}

fn render_token_source(display: &TokenInputState, is_editing: bool, theme: &Theme) -> Div {
    let (source, color) = token_source_text(display, is_editing, theme);
    div()
        .py(px(6.0))
        .child(div().text_size(px(12.0)).text_color(color).child(source))
}

fn token_source_text(
    display: &TokenInputState,
    is_editing: bool,
    theme: &Theme,
) -> (String, gpui::Hsla) {
    if is_editing || !display.has_token {
        return ("placeholder".to_string(), theme.bg.card_inner);
    }
    if let Some(source_i18n_key) = display.source_i18n_key {
        let masked = display.masked.as_deref().unwrap_or_default();
        return (
            t!(
                "settings.token.via",
                masked = masked,
                source = t!(source_i18n_key).to_string()
            )
            .to_string(),
            theme.text.muted,
        );
    }
    display
        .masked
        .clone()
        .map(|masked| (masked, theme.text.muted))
        .unwrap_or_else(|| ("placeholder".to_string(), theme.bg.card_inner))
}

// ============================================================================
// 操作按钮
// ============================================================================

fn render_token_action_buttons(
    provider_id: ProviderId,
    capability: TokenInputCapability,
    view: &mut SettingsView,
    edit_mode: TokenEditMode,
    theme: &Theme,
    is_editing: bool,
    cx: &mut Context<SettingsView>,
) -> Div {
    div()
        .flex()
        .gap(px(10.0))
        .mt(px(2.0))
        .child(render_primary_token_action(
            &provider_id,
            capability,
            view,
            theme,
            is_editing,
            cx,
        ))
        .child(render_secondary_token_action(
            provider_id,
            capability,
            view,
            edit_mode,
            theme,
            is_editing,
            cx,
        ))
}

fn render_primary_token_action(
    provider_id: &ProviderId,
    capability: TokenInputCapability,
    view: &SettingsView,
    theme: &Theme,
    is_editing: bool,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let label = if is_editing {
        t!("settings.token.save").to_string()
    } else {
        t!("settings.token.create").to_string()
    };
    let input_entity = view
        .token_input
        .as_ref()
        .filter(|draft| &draft.provider_id == provider_id)
        .map(|draft| draft.input.clone());
    let state = view.state.clone();
    let provider_id = provider_id.clone();
    let view_entity = cx.entity().clone();

    token_action_button(label, theme.text.accent, None, theme.element.active)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if !is_editing {
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::OpenUrl(capability.create_url.to_string()),
                    window,
                    cx,
                );
                return;
            }
            let Some(input_entity) = input_entity.as_ref() else {
                log::warn!(
                    target: "settings",
                    "token save requested for {} but input draft is missing; cancelling edit state",
                    provider_id
                );
                crate::bootstrap::dispatch_in_window(
                    &state,
                    AppAction::SetTokenEditing {
                        provider_id: provider_id.clone(),
                        editing: false,
                    },
                    window,
                    cx,
                );
                return;
            };

            let token = input_entity.read(cx).content().trim().to_string();
            view_entity.update(cx, |view, _| view.clear_token_input());
            crate::bootstrap::dispatch_in_window(
                &state,
                AppAction::SaveProviderToken {
                    provider_id: provider_id.clone(),
                    token,
                },
                window,
                cx,
            );
        })
        .into_any_element()
}

fn render_secondary_token_action(
    provider_id: ProviderId,
    capability: TokenInputCapability,
    view: &SettingsView,
    edit_mode: TokenEditMode,
    theme: &Theme,
    is_editing: bool,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let label = if is_editing {
        t!("settings.token.cancel").to_string()
    } else if edit_mode == TokenEditMode::EditStored {
        t!("settings.token.edit").to_string()
    } else {
        t!("settings.token.set").to_string()
    };
    let state = view.state.clone();
    let view_entity = cx.entity().clone();

    token_action_button(
        label,
        theme.bg.subtle,
        Some(theme.border.strong),
        theme.text.primary,
    )
    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        if is_editing {
            view_entity.update(cx, |view, _| view.clear_token_input());
        } else {
            view_entity.update(cx, |view, cx| {
                view.begin_token_input(&provider_id, capability, edit_mode, cx);
            });
        }
        crate::bootstrap::dispatch_in_window(
            &state,
            AppAction::SetTokenEditing {
                provider_id: provider_id.clone(),
                editing: !is_editing,
            },
            window,
            cx,
        );
    })
    .into_any_element()
}

fn token_action_button(
    label: String,
    background: gpui::Hsla,
    border: Option<gpui::Hsla>,
    text: gpui::Hsla,
) -> Div {
    let button = div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .px(px(16.0))
        .py(px(10.0))
        .rounded(px(8.0))
        .bg(background)
        .text_size(px(13.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(text)
        .cursor_pointer()
        .hover(|style| style.opacity(0.9))
        .child(label);
    match border {
        Some(color) => button.border_1().border_color(color),
        None => button,
    }
}
