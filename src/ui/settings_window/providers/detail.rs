use super::super::SettingsView;
use crate::application::{
    AppAction, QuotaVisibilityItem, SettingChange, SettingsProviderDetailViewState,
    SettingsProviderInfoViewState, SettingsProviderStatusKind, SettingsProviderUsageViewState,
};
use crate::models::{ProviderId, ProviderKind, QuotaDisplayMode, SettingsCapability};
use crate::refresh::RefreshReason;
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::{render_detail_section_title, render_info_cell, render_svg_icon};
use gpui::{
    div, hsla, px, relative, App, Context, Div, FontWeight, Hsla, InteractiveElement, MouseButton,
    MouseDownEvent, ParentElement, StatefulInteractiveElement, Styled, TextAlign, Window,
};
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::AppState;

/// 配额可见性行是否显示左侧小图标（默认开启，视觉更灵动）
const SHOW_QUOTA_ROW_ICON: bool = true;

// ══════ 静态 UI 组件（无状态，纯 theme 参数） ══════

/// Provider 标题区：大图标 + 名称 + 副标题（设计稿风格）
fn render_detail_header_info(icon: &str, display_name: &str, subtitle: &str, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(14.0))
        .child(crate::ui::widgets::render_provider_icon_boxed(
            icon,
            px(56.0),
            px(32.0),
            theme.text.primary,
            theme.bg.subtle,
        ))
        .child(
            div()
                .flex_col()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text.primary)
                        .child(display_name.to_string()),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(theme.text.muted)
                        .child(subtitle.to_string()),
                ),
        )
}

/// Info table：两列布局，第一行 "状态 + 来源"，第二行 "更新时间 + 服务状态"
fn render_info_table(info: &SettingsProviderInfoViewState, theme: &Theme) -> Div {
    let status_color = match info.status_kind {
        SettingsProviderStatusKind::Success => theme.status.success,
        SettingsProviderStatusKind::Error => theme.status.error,
        SettingsProviderStatusKind::Neutral => theme.text.primary,
    };

    div()
        .flex_col()
        .gap(px(12.0))
        .mt(px(20.0))
        // 第一行：状态 + 来源
        .child(
            div()
                .flex()
                .gap(px(16.0))
                .child(render_info_cell(
                    &t!("provider.info.state"),
                    &info.state_text,
                    theme.text.primary,
                    theme,
                ))
                .child(render_info_cell(
                    &t!("provider.info.source"),
                    &info.source_text,
                    theme.text.primary,
                    theme,
                )),
        )
        // 第二行：更新时间 + 服务状态
        .child(
            div()
                .flex()
                .gap(px(16.0))
                .child(render_info_cell(
                    &t!("provider.info.updated"),
                    &info.updated_text,
                    theme.text.primary,
                    theme,
                ))
                .child(render_info_cell(
                    &t!("provider.info.status"),
                    &info.status_text,
                    status_color,
                    theme,
                )),
        )
}

/// Usage section
fn render_usage_section(
    usage: &SettingsProviderUsageViewState,
    theme: &Theme,
    display_mode: QuotaDisplayMode,
) -> Div {
    let mut section = div()
        .flex_col()
        .mt(px(20.0))
        .child(render_detail_section_title(
            &t!("provider.section.usage"),
            theme,
        ));

    match usage {
        SettingsProviderUsageViewState::Disabled { message }
        | SettingsProviderUsageViewState::Empty { message }
        | SettingsProviderUsageViewState::Missing { message } => {
            section = section.child(
                div()
                    .mt(px(8.0))
                    .text_size(px(12.0))
                    .text_color(theme.text.secondary)
                    .child(message.clone()),
            );
        }
        SettingsProviderUsageViewState::Quotas { quotas } => {
            for quota in quotas {
                section = section.child(div().mt(px(10.0)).child(
                    crate::ui::widgets::render_quota_bar(quota, theme, 0, display_mode),
                ));
            }
        }
        SettingsProviderUsageViewState::Error { title, message } => {
            section = section
                .child(
                    div()
                        .mt(px(8.0))
                        .text_size(px(12.0))
                        .text_color(theme.text.muted)
                        .child(title.clone()),
                )
                .child(
                    div()
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(6.0))
                        .bg(theme.bg.subtle)
                        .child(
                            div()
                                .text_size(px(11.5))
                                .line_height(relative(1.4))
                                .text_color(theme.text.secondary)
                                .child(message.clone()),
                        ),
                );
        }
    }

    section
}

/// 无需配置的 provider 占位卡片：虚线边框 + 居中图标 + 淡色文字
fn render_automanaged_placeholder() -> Div {
    let muted_color = hsla(0.0, 0.0, 0.45, 0.5); // 比 text_muted 更淡
    div()
        .mt(px(10.0))
        .w_full()
        .flex_col()
        .items_center()
        .justify_center()
        .py(px(36.0))
        .px(px(20.0))
        .rounded(px(12.0))
        .border_1()
        .border_dashed()
        .border_color(hsla(0.0, 0.0, 0.3, 0.3)) // 比 border_subtle 更淡，降低虚线密集感
        // 居中齿轮图标
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w_full()
                .child(render_svg_icon(
                    "src/icons/settings.svg",
                    px(32.0),
                    muted_color,
                )),
        )
        // 居中说明文字
        .child(
            div()
                .mt(px(16.0))
                .w_full()
                .flex_col()
                .items_center()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(muted_color)
                        .text_align(TextAlign::Center)
                        .child(t!("provider.settings.auto_title").to_string()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(muted_color)
                        .text_align(TextAlign::Center)
                        .child(t!("provider.settings.auto_desc").to_string()),
                ),
        )
}

// ══════ 交互组件（携带 state / 事件回调） ══════

/// NewAPI 操作按钮（编辑 / 删除）— 精致小按钮
fn render_action_button(
    label: &str,
    icon: &'static str,
    color: Hsla,
    theme: &Theme,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(5.0))
        .px(px(12.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .bg(theme.bg.subtle)
        .border_1()
        .border_color(theme.border.strong)
        .text_size(px(12.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .cursor_pointer()
        .hover(|s| s.opacity(0.85))
        .child(render_svg_icon(icon, px(14.0), color))
        .child(label.to_string())
        .on_mouse_down(MouseButton::Left, on_click)
}

/// 刷新按钮（⟳）— 精致小尺寸
fn render_refresh_button(state: Rc<RefCell<AppState>>, id: ProviderId, theme: &Theme) -> Div {
    div()
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .bg(theme.bg.subtle)
        .cursor_pointer()
        .hover(|s| s.opacity(0.8))
        .child(crate::ui::widgets::render_svg_icon(
            "src/icons/refresh.svg",
            px(16.0),
            theme.text.muted,
        ))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            runtime::dispatch_in_window(
                &state,
                AppAction::RefreshProvider {
                    id: id.clone(),
                    reason: RefreshReason::Manual,
                },
                window,
                cx,
            );
        })
}

/// 二次确认按钮组（通用）：确认（红色）+ 取消
fn render_confirm_cancel_buttons(
    confirm_label: &str,
    cancel_label: &str,
    on_confirm: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    theme: &Theme,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(
            div()
                .h(px(24.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(4.0))
                .rounded(px(6.0))
                .bg(theme.status.error)
                .cursor_pointer()
                .hover(|s| s.opacity(0.85))
                .child(crate::ui::widgets::render_svg_icon(
                    "src/icons/trash.svg",
                    px(12.0),
                    gpui::white(),
                ))
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(gpui::white())
                        .child(confirm_label.to_string()),
                )
                .on_mouse_down(MouseButton::Left, on_confirm),
        )
        .child(
            div()
                .h(px(24.0))
                .px(px(6.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .bg(theme.bg.subtle)
                .cursor_pointer()
                .hover(|s| s.opacity(0.8))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.text.muted)
                        .child(cancel_label.to_string()),
                )
                .on_mouse_down(MouseButton::Left, on_cancel),
        )
}

/// Header 右侧操作区：移除按钮（二次确认） + 刷新按钮 + 启用/禁用开关
fn render_detail_action_buttons(
    state: Rc<RefCell<AppState>>,
    id: &ProviderId,
    is_enabled: bool,
    confirming_remove: bool,
    theme: &Theme,
) -> Div {
    let state_toggle = state.clone();
    let state_confirm = state.clone();
    let id_toggle = id.clone();

    let remove_button = if confirming_remove {
        let state_confirm = state.clone();
        let state_cancel = state.clone();
        let id_remove = id.clone();
        render_confirm_cancel_buttons(
            &t!("common.confirm"),
            &t!("common.cancel"),
            move |_, window, cx| {
                runtime::dispatch_in_window(
                    &state_confirm,
                    AppAction::RemoveProviderFromSidebar(id_remove.clone()),
                    window,
                    cx,
                );
            },
            move |_, window, cx| {
                runtime::dispatch_in_window(
                    &state_cancel,
                    AppAction::CancelRemoveProvider,
                    window,
                    cx,
                );
            },
            theme,
        )
    } else {
        // 常态：小图标按钮
        div().child(
            div()
                .id("remove-from-sidebar")
                .w(px(28.0))
                .h(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .bg(theme.bg.subtle)
                .cursor_pointer()
                .hover(|s| s.opacity(0.8))
                .child(crate::ui::widgets::render_svg_icon(
                    "src/icons/trash.svg",
                    px(14.0),
                    theme.text.muted,
                ))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    runtime::dispatch_in_window(
                        &state_confirm,
                        AppAction::ConfirmRemoveProvider,
                        window,
                        cx,
                    );
                }),
        )
    };

    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(remove_button)
        .child(render_refresh_button(state, id.clone(), theme))
        .child(
            crate::ui::widgets::render_toggle_switch(
                is_enabled,
                px(44.0),
                px(24.0),
                px(18.0),
                theme,
            )
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                runtime::dispatch_in_window(
                    &state_toggle,
                    AppAction::ToggleProvider(id_toggle.clone()),
                    window,
                    cx,
                );
            }),
        )
}

/// NewAPI 型 provider 的操作按钮行：编辑 + 删除（删除需二次确认）
fn render_newapi_action_row(
    provider_id: ProviderId,
    confirming_delete: bool,
    state: Rc<RefCell<AppState>>,
    theme: &Theme,
) -> Div {
    let state_edit = state.clone();
    let provider_id_edit = provider_id.clone();
    let provider_id_delete = provider_id.clone();

    let mut row = div()
        .mt(px(10.0))
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(10.0))
        .child(render_action_button(
            &t!("newapi.edit_button"),
            "src/icons/settings.svg",
            theme.text.accent,
            theme,
            move |_, window, cx| {
                runtime::dispatch_in_window(
                    &state_edit,
                    AppAction::EditNewApi {
                        provider_id: provider_id_edit.clone(),
                    },
                    window,
                    cx,
                );
            },
        ));

    if confirming_delete {
        // 确认态：复用通用确认/取消按钮组
        let state_delete = state.clone();
        let state_cancel = state.clone();
        row = row.child(render_confirm_cancel_buttons(
            &t!("newapi.confirm_delete"),
            &t!("newapi.cancel_delete"),
            move |_, window, cx| {
                runtime::dispatch_in_window(
                    &state_delete,
                    AppAction::DeleteNewApi {
                        provider_id: provider_id_delete.clone(),
                    },
                    window,
                    cx,
                );
            },
            move |_, window, cx| {
                runtime::dispatch_in_window(
                    &state_cancel,
                    AppAction::CancelDeleteNewApi,
                    window,
                    cx,
                );
            },
            theme,
        ));
    } else {
        let state_confirm = state.clone();
        row = row.child(render_action_button(
            &t!("newapi.delete_button"),
            "src/icons/trash.svg",
            theme.status.error,
            theme,
            move |_, window, cx| {
                runtime::dispatch_in_window(
                    &state_confirm,
                    AppAction::ConfirmDeleteNewApi,
                    window,
                    cx,
                );
            },
        ));
    }

    row
}

/// 单行：（可选图标 +）配额标签 + 小号 toggle switch
fn render_quota_visibility_row(
    kind: ProviderKind,
    item: &QuotaVisibilityItem,
    state: Rc<RefCell<AppState>>,
    theme: &Theme,
) -> Div {
    let quota_key = item.quota_key.clone();
    let visible = item.visible;

    let mut label_row = div().flex().items_center().gap(px(8.0));
    if SHOW_QUOTA_ROW_ICON {
        label_row = label_row.child(render_svg_icon(
            "src/icons/status.svg",
            px(14.0),
            if visible {
                theme.text.accent
            } else {
                theme.text.muted
            },
        ));
    }
    label_row = label_row.child(
        div()
            .text_size(px(12.5))
            .text_color(if visible {
                theme.text.primary
            } else {
                theme.text.muted
            })
            .child(item.label.clone()),
    );

    div()
        .flex()
        .items_center()
        .justify_between()
        .px(px(12.0))
        .py(px(8.0))
        .cursor_pointer()
        .hover(|s| s.bg(theme.bg.subtle))
        .child(label_row)
        .child(
            crate::ui::widgets::render_toggle_switch(visible, px(36.0), px(20.0), px(14.0), theme)
                .flex_shrink_0(),
        )
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            runtime::dispatch_in_window(
                &state,
                AppAction::UpdateSetting(SettingChange::ToggleQuotaVisibility {
                    kind,
                    quota_key: quota_key.clone(),
                }),
                window,
                cx,
            );
        })
}

/// Quota visibility section：托盘弹窗中显示哪些模型
fn render_quota_visibility_section(
    kind: ProviderKind,
    items: &[QuotaVisibilityItem],
    state: Rc<RefCell<AppState>>,
    theme: &Theme,
) -> Div {
    let mut section = div()
        .flex_col()
        .mt(px(20.0))
        .child(render_detail_section_title(
            &t!("provider.section.quota_visibility"),
            theme,
        ));

    if items.is_empty() {
        section = section.child(
            div()
                .mt(px(8.0))
                .text_size(px(12.0))
                .text_color(theme.text.secondary)
                .child(t!("provider.quota_visibility.empty").to_string()),
        );
    } else {
        let item_count = items.len();
        let mut list = div()
            .flex_col()
            .mt(px(8.0))
            .rounded(px(10.0))
            .bg(theme.bg.card)
            .border_1()
            .border_color(theme.border.subtle)
            .overflow_hidden();

        for (i, item) in items.iter().enumerate() {
            list = list.child(render_quota_visibility_row(
                kind,
                item,
                state.clone(),
                theme,
            ));
            if i + 1 < item_count {
                list = list.child(div().h(px(0.5)).w_full().bg(theme.border.subtle));
            }
        }
        section = section.child(list);
    }

    section
}

// ══════ SettingsView impl（仅保留必须持有 &mut self 的入口） ══════

impl SettingsView {
    pub(in crate::ui::settings_window) fn render_provider_detail_panel(
        &mut self,
        detail: &SettingsProviderDetailViewState,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let confirming_remove = self
            .state
            .borrow()
            .session
            .settings_ui
            .confirming_remove_provider;

        let inner = div()
            .flex_col()
            .px(px(24.0))
            .pt(px(20.0))
            .pb(px(60.0)) // 底部留足空间，确保滚动到底时内容完全可见
            // ── Header: icon + name + refresh + toggle ──
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(render_detail_header_info(
                        &detail.icon,
                        &detail.display_name,
                        &detail.subtitle,
                        theme,
                    ))
                    .child(render_detail_action_buttons(
                        self.state.clone(),
                        &detail.id,
                        detail.is_enabled,
                        confirming_remove,
                        theme,
                    )),
            )
            // ── Info table (两列布局) ──
            .child(render_info_table(&detail.info, theme))
            // ── Usage section ──
            .child(render_usage_section(
                &detail.usage,
                theme,
                detail.quota_display_mode,
            ))
            // ── Quota visibility section ──
            .child(render_quota_visibility_section(
                detail.id.kind(),
                &detail.quota_visibility,
                self.state.clone(),
                theme,
            ))
            // ── Settings section ──
            .child(self.render_settings_section(
                detail.id.clone(),
                detail.settings_capability.clone(),
                theme,
                cx,
            ));

        div().flex_col().flex_1().h_full().overflow_hidden().child(
            div()
                .id("provider-detail-scroll")
                .flex_col()
                .h_full()
                .overflow_y_scroll()
                .child(inner),
        )
    }

    // render_settings_section 保留为方法：TokenInput 分支需要 &mut self（创建 input entity）
    fn render_settings_section(
        &mut self,
        provider_id: ProviderId,
        settings_capability: SettingsCapability,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut section =
            div()
                .flex_col()
                .mt(px(20.0))
                .pb(px(20.0))
                .child(render_detail_section_title(
                    &t!("provider.section.settings"),
                    theme,
                ));

        match settings_capability {
            SettingsCapability::TokenInput(capability) => {
                // 直接从 capability 驱动渲染，消费声明的元数据字段（OCP）
                section = section.child(div().mt(px(10.0)).child(
                    super::token_input_panel::render_token_input_panel(
                        &provider_id,
                        capability,
                        self,
                        theme,
                        cx,
                    ),
                ));
            }
            SettingsCapability::NewApiEditable => {
                let confirming_delete = self
                    .state
                    .borrow()
                    .session
                    .settings_ui
                    .confirming_delete_newapi;
                section = section.child(render_newapi_action_row(
                    provider_id,
                    confirming_delete,
                    self.state.clone(),
                    theme,
                ));
            }
            SettingsCapability::None => {
                section = section.child(render_automanaged_placeholder());
            }
        }

        section
    }
}
