use super::super::SettingsView;
use crate::app::widgets::{render_detail_section_title, render_info_cell, render_svg_icon};
use crate::application::{
    AppAction, ProviderSettingsMode, QuotaVisibilityItem, SettingChange,
    SettingsProviderDetailViewState, SettingsProviderStatusKind, SettingsProviderUsageViewState,
};
use crate::models::{ProviderId, ProviderKind, QuotaDisplayMode};
use crate::refresh::RefreshReason;
use crate::runtime;
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::AppState;

/// 配额可见性行是否显示左侧小图标（默认开启，视觉更灵动）
const SHOW_QUOTA_ROW_ICON: bool = true;

// ══════ 可复用的 detail 区域组件 ══════

/// Provider 标题区：大图标 + 名称 + 副标题（设计稿风格）
fn render_detail_header_info(icon: &str, display_name: &str, subtitle: &str, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(14.0))
        .child(crate::app::widgets::render_provider_icon_boxed(
            icon,
            px(56.0),
            px(32.0),
            theme.text.primary,
            theme.bg.subtle,
        ))
        .child(
            div()
                .flex_col()
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

/// 刷新按钮（⟳）— 设计稿：较大尺寸，清晰可见
fn render_refresh_button(state: Rc<RefCell<AppState>>, id: ProviderId, theme: &Theme) -> Div {
    div()
        .w(px(36.0))
        .h(px(36.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.0))
        .bg(theme.bg.subtle)
        .cursor_pointer()
        .hover(|s| s.opacity(0.8))
        .child(crate::app::widgets::render_svg_icon(
            "src/icons/refresh.svg",
            px(22.0),
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

/// Header 右侧操作区：刷新按钮 + 启用/禁用开关
fn render_detail_action_buttons(
    state: Rc<RefCell<AppState>>,
    id: &ProviderId,
    is_enabled: bool,
    theme: &Theme,
) -> Div {
    let state_toggle = state.clone();
    let id_toggle = id.clone();

    div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(render_refresh_button(state, id.clone(), theme))
        .child(
            crate::app::widgets::render_toggle_switch(
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

impl SettingsView {
    // ══════ Right detail panel ══════

    pub(in crate::app::settings_window) fn render_provider_detail_panel(
        &mut self,
        detail: &SettingsProviderDetailViewState,
        theme: &Theme,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> Div {
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
                        theme,
                    )),
            )
            // ── Info table (两列布局) ──
            .child(self.render_info_table(&detail.info, theme))
            // ── Usage section ──
            .child(self.render_usage_section(&detail.usage, theme, detail.quota_display_mode))
            // ── Quota visibility section ──
            .child(self.render_quota_visibility_section(
                detail.id.kind(),
                &detail.quota_visibility,
                theme,
            ))
            // ── Settings section ──
            .child(self.render_settings_section(detail.settings_mode, theme, cx));

        let detail_scroll_h = viewport.height - px(65.0);

        div().flex_col().flex_1().overflow_hidden().child(
            div()
                .id("provider-detail-scroll")
                .flex_col()
                .h(detail_scroll_h)
                .overflow_y_scroll()
                .child(inner),
        )
    }

    // ══════ Info table (两列布局，匹配设计稿) ══════

    fn render_info_table(
        &self,
        info: &crate::application::SettingsProviderInfoViewState,
        theme: &Theme,
    ) -> Div {
        let status_color = match info.status_kind {
            SettingsProviderStatusKind::Success => theme.status.success,
            SettingsProviderStatusKind::Error => theme.status.error,
            SettingsProviderStatusKind::Neutral => theme.text.primary,
        };

        // 设计稿：两列布局，第一行 "状态 + 来源"，第二行 "更新时间 + 服务状态"
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

    // ══════ Usage section ══════

    fn render_usage_section(
        &self,
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
                        crate::app::widgets::render_quota_bar(quota, theme, 0, display_mode),
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

    // ══════ Quota visibility (托盘弹窗中显示哪些模型) ══════

    fn render_quota_visibility_section(
        &self,
        kind: ProviderKind,
        items: &[QuotaVisibilityItem],
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
            let list = div()
                .flex_col()
                .mt(px(8.0))
                .rounded(px(10.0))
                .bg(theme.bg.card)
                .border_1()
                .border_color(theme.border.subtle)
                .overflow_hidden();

            let item_count = items.len();
            let mut list = list;
            for (i, item) in items.iter().enumerate() {
                list = list.child(self.render_quota_visibility_row(kind, item, theme));
                if i + 1 < item_count {
                    list = list.child(div().h(px(0.5)).w_full().bg(theme.border.subtle));
                }
            }
            section = section.child(list);
        }

        section
    }

    /// 单行：（可选图标 +）配额标签 + 小号 toggle switch
    fn render_quota_visibility_row(
        &self,
        kind: ProviderKind,
        item: &QuotaVisibilityItem,
        theme: &Theme,
    ) -> Div {
        let state = self.state.clone();
        let quota_key = item.quota_key.clone();
        let visible = item.visible;
        let show_icon = SHOW_QUOTA_ROW_ICON;

        let mut label_row = div().flex().items_center().gap(px(8.0));
        if show_icon {
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
                crate::app::widgets::render_toggle_switch(
                    visible,
                    px(36.0),
                    px(20.0),
                    px(14.0),
                    theme,
                )
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

    // ══════ Provider-specific settings ══════

    fn render_settings_section(
        &mut self,
        settings_mode: ProviderSettingsMode,
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

        match settings_mode {
            ProviderSettingsMode::Interactive => {
                // 3. 使用交互式 UI（支持 Token 输入和保存）
                section = section.child(div().mt(px(10.0)).child(
                    crate::providers::copilot::settings_ui::render_settings_interactive(
                        self, theme, cx,
                    ),
                ));
            }
            ProviderSettingsMode::AutoManaged => {
                // 设计稿：无需配置的 provider — 虚线边框 + 居中图标 + 淡色文字，无背景色
                let muted_color = hsla(0.0, 0.0, 0.45, 0.5); // 比 text_muted 更淡
                section = section.child(
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
                        .child(div().flex().items_center().justify_center().w_full().child(
                            render_svg_icon("src/icons/settings.svg", px(32.0), muted_color),
                        ))
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
                        ),
                );
            }
        }

        section
    }
}
