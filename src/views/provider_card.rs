use gpui::*;

use crate::models::{AppTheme, ProviderStatus, StatusLevel};

// ============================================================================
// Provider 卡片组件
// ============================================================================

/// 单个 Provider 的卡片展示
#[derive(IntoElement)]
pub struct ProviderCard {
    provider: ProviderStatus,
    theme: AppTheme,
}

impl ProviderCard {
    pub fn new(provider: ProviderStatus, theme: AppTheme) -> Self {
        Self { provider, theme }
    }
}

impl RenderOnce for ProviderCard {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (card_bg, card_border, sub_text) = match self.theme {
            AppTheme::Dark => (rgb(0x0a0a0a), rgb(0x262626), rgb(0xa3a3a3)), // Flat bg neutral-950, border neutral-800, subtext neutral-400
            AppTheme::Light => (rgb(0xffffff), rgb(0xe5e5e5), rgb(0x737373)), // Flat white, border neutral-200, subtext neutral-500
        };

        let status_color: Hsla = match self.provider.worst_status() {
            StatusLevel::Green => rgb(0x22c55e).into(),
            StatusLevel::Yellow => rgb(0xeab308).into(),
            StatusLevel::Red => rgb(0xef4444).into(),
        };

        div()
            .p(px(16.0))
            .rounded(px(6.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            .child(
                // 卡片头部：Provider 名称 + 状态指示灯
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                // 状态指示灯
                                div()
                                    .size(px(6.0))
                                    .rounded_full()
                                    .bg(status_color),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(self.provider.kind.display_name().to_string()),
                            ),
                    )
                    .child(
                        // 连接状态标签
                        div()
                            .text_size(px(11.0))
                            .text_color(sub_text)
                            .child(format!("{:?}", self.provider.connection)),
                    ),
            )
            .children(
                // 用量进度条列表
                self.provider.quotas.iter().map(|quota| {
                    let pct = quota.percentage();
                    let bar_color = match quota.status_level() {
                        StatusLevel::Green | StatusLevel::Yellow => match self.theme {
                            AppTheme::Dark => rgb(0x3b82f6), // blue-500
                            AppTheme::Light => rgb(0x2563eb), // blue-600
                        },
                        StatusLevel::Red => rgb(0xef4444), // red-500
                    };
                    let bar_bg = match self.theme {
                        AppTheme::Dark => rgb(0x171717), // neutral-900
                        AppTheme::Light => rgb(0xf5f5f5), // neutral-100
                    };

                    div()
                        .mb(px(8.0))
                        .child(
                            // 标签行
                            div()
                                .flex()
                                .justify_between()
                                .mb(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(sub_text)
                                        .child(quota.label.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(format!("{:.0}%", pct)),
                                ),
                        )
                        .child(
                            // 进度条背景
                            div()
                                .h(px(4.0))
                                .rounded(px(2.0))
                                .bg(bar_bg)
                                .overflow_hidden()
                                .child(
                                    // 进度条填充
                                    div()
                                        .h_full()
                                        .rounded(px(3.0))
                                        .bg(bar_color)
                                        .w(relative(pct as f32 / 100.0)),
                                ),
                        )
                }).collect::<Vec<_>>(),
            )
    }
}
