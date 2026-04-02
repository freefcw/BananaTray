use super::super::SettingsView;
use crate::app::widgets::{render_detail_section_title, render_svg_icon};
use crate::app::{persist_settings, provider_logic};
use crate::models::{AppSettings, ConnectionStatus, ProviderKind};
use crate::refresh::RefreshReason;
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::AppState;

// ══════ 可复用的 detail 区域组件 ══════

/// Provider 标题区：大图标 + 名称 + 副标题（设计稿风格）
fn render_detail_header_info(icon: &str, display_name: &str, subtitle: &str, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(14.0))
        .child(
            // 图标容器：加大尺寸
            div()
                .w(px(56.0))
                .h(px(56.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(14.0))
                .bg(theme.bg_subtle)
                .flex_shrink_0()
                .child(
                    svg()
                        .path(icon.to_string())
                        .size(px(32.0))
                        .text_color(theme.text_primary),
                ),
        )
        .child(
            div()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .child(display_name.to_string()),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(theme.text_muted)
                        .child(subtitle.to_string()),
                ),
        )
}

/// 刷新按钮（⟳）— 设计稿：较大尺寸，清晰可见
fn render_refresh_button(state: Rc<RefCell<AppState>>, kind: ProviderKind, theme: &Theme) -> Div {
    div()
        .w(px(36.0))
        .h(px(36.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.0))
        .bg(theme.bg_subtle)
        .cursor_pointer()
        .hover(|s| s.opacity(0.8))
        .child(crate::app::widgets::render_svg_icon(
            "src/icons/refresh.svg",
            px(22.0),
            theme.text_muted,
        ))
        .on_mouse_down(MouseButton::Left, move |_, window, _| {
            let mut s = state.borrow_mut();
            s.request_provider_refresh(kind, RefreshReason::Manual);
            drop(s);
            window.refresh();
        })
}

/// Header 右侧操作区：刷新按钮 + 启用/禁用开关
fn render_detail_action_buttons(
    state: Rc<RefCell<AppState>>,
    kind: ProviderKind,
    is_enabled: bool,
    theme: &Theme,
) -> Div {
    let state_toggle = state.clone();

    div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(render_refresh_button(state, kind, theme))
        .child(
            crate::app::widgets::render_toggle_switch(
                is_enabled,
                px(44.0),
                px(24.0),
                px(18.0),
                theme,
            )
            .on_mouse_down(MouseButton::Left, move |_, window, _cx| {
                let settings = state_toggle.borrow_mut().toggle_provider(kind);
                persist_settings(&settings);
                window.refresh();
            }),
        )
}

/// 渲染单个信息单元格（标签 + 值），水平排列：label  value
/// 设计稿样式：一行内 label(灰) value(亮) label(灰) value(亮)
fn render_info_cell(label: &str, value: &str, value_color: Hsla, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .flex_1()
        .child(
            div()
                .text_size(px(12.5))
                .text_color(theme.text_muted)
                .flex_shrink_0()
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(value_color)
                .child(value.to_string()),
        )
}

impl SettingsView {
    // ══════ Right detail panel ══════

    pub(in crate::app::settings_window) fn render_provider_detail_panel(
        &mut self,
        providers: &[crate::models::ProviderStatus],
        selected: ProviderKind,
        settings: &AppSettings,
        theme: &Theme,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> Div {
        let provider = providers.iter().find(|p| p.kind == selected).cloned();
        let is_enabled = settings.is_provider_enabled(selected);

        let (icon, display_name, subtitle) = if let Some(ref p) = provider {
            (
                p.icon_asset().to_string(),
                p.display_name().to_string(),
                provider_logic::provider_detail_subtitle(p),
            )
        } else {
            (
                "src/icons/provider-unknown.svg".to_string(),
                format!("{:?}", selected),
                format!("{:?} · {}", selected, t!("provider.not_available")),
            )
        };

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
                        &icon,
                        &display_name,
                        &subtitle,
                        theme,
                    ))
                    .child(render_detail_action_buttons(
                        self.state.clone(),
                        selected,
                        is_enabled,
                        theme,
                    )),
            )
            // ── Info table (两列布局) ──
            .child(self.render_info_table(provider.as_ref(), is_enabled, theme))
            // ── Usage section ──
            .child(self.render_usage_section(provider.as_ref(), is_enabled, theme))
            // ── Settings section ──
            .child(self.render_settings_section(selected, settings, theme, cx));

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
        provider: Option<&crate::models::ProviderStatus>,
        enabled: bool,
        theme: &Theme,
    ) -> Div {
        let state_text = if enabled {
            t!("provider.state.enabled").to_string()
        } else {
            t!("provider.state.disabled").to_string()
        };
        let source_text = t!("provider.source.auto").to_string();
        let updated_text = provider
            .map(|p| p.format_last_updated())
            .unwrap_or_else(|| t!("provider.not_fetched").to_string());

        let status_text = provider
            .map(|p| match p.connection {
                ConnectionStatus::Connected => t!("provider.status.operational").to_string(),
                ConnectionStatus::Disconnected => t!("provider.status.not_detected").to_string(),
                ConnectionStatus::Refreshing => t!("provider.status.refreshing").to_string(),
                ConnectionStatus::Error => t!("provider.status.error").to_string(),
            })
            .unwrap_or_else(|| t!("provider.status.unknown").to_string());

        // 服务状态颜色：运行正常用绿色
        let status_color = provider
            .map(|p| match p.connection {
                ConnectionStatus::Connected => theme.status_success,
                ConnectionStatus::Error => theme.status_error,
                _ => theme.text_primary,
            })
            .unwrap_or(theme.text_primary);

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
                        &state_text,
                        theme.text_primary,
                        theme,
                    ))
                    .child(render_info_cell(
                        &t!("provider.info.source"),
                        &source_text,
                        theme.text_primary,
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
                        &updated_text,
                        theme.text_primary,
                        theme,
                    ))
                    .child(render_info_cell(
                        &t!("provider.info.status"),
                        &status_text,
                        status_color,
                        theme,
                    )),
            )
    }

    // ══════ Usage section ══════

    fn render_usage_section(
        &self,
        provider: Option<&crate::models::ProviderStatus>,
        enabled: bool,
        theme: &Theme,
    ) -> Div {
        let mut section = div()
            .flex_col()
            .mt(px(20.0))
            .child(render_detail_section_title(
                &t!("provider.section.usage"),
                theme,
            ));

        if !enabled {
            return section.child(
                div()
                    .mt(px(8.0))
                    .text_size(px(12.0))
                    .text_color(theme.text_secondary)
                    .child(t!("provider.enable_tracking").to_string()),
            );
        }

        if let Some(p) = provider {
            if !p.quotas.is_empty() {
                // 为每个 quota 卡片单独添加间距，避免 gap 对 impl IntoElement 不生效
                for quota in p.quotas.iter() {
                    section = section.child(
                        div()
                            .mt(px(10.0))
                            .child(crate::app::widgets::render_quota_bar(quota, theme, 0)),
                    );
                }
            } else if p.connection == ConnectionStatus::Error {
                let title = t!("provider.last_fetch_failed", name = p.display_name()).to_string();
                let msg = p
                    .error_message
                    .clone()
                    .unwrap_or_else(|| t!("provider.unknown_error").to_string());
                section = section
                    .child(
                        div()
                            .mt(px(8.0))
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(title),
                    )
                    .child(
                        div()
                            .px(px(10.0))
                            .py(px(8.0))
                            .rounded(px(6.0))
                            .bg(theme.bg_subtle)
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .line_height(relative(1.4))
                                    .text_color(theme.text_secondary)
                                    .child(msg),
                            ),
                    );
            } else {
                section = section.child(
                    div()
                        .mt(px(8.0))
                        .text_size(px(12.0))
                        .text_color(theme.text_secondary)
                        .child(t!("provider.no_usage").to_string()),
                );
            }
        } else {
            section = section.child(
                div()
                    .mt(px(8.0))
                    .text_size(px(12.0))
                    .text_color(theme.text_secondary)
                    .child(t!("provider.not_available").to_string()),
            );
        }

        section
    }

    // ══════ Provider-specific settings ══════

    fn render_settings_section(
        &mut self,
        kind: ProviderKind,
        _settings: &AppSettings,
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

        match kind {
            ProviderKind::Copilot => {
                // 3. 使用交互式 UI（支持 Token 输入和保存）
                section = section.child(div().mt(px(10.0)).child(
                    crate::providers::copilot::settings_ui::render_settings_interactive(
                        self, theme, cx,
                    ),
                ));
            }
            _ => {
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
