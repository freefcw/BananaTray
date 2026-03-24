use gpui::*;

use crate::models::{ProviderStatus, StatusLevel};
use crate::theme::Theme;

// ============================================================================
// Provider 卡片组件
// ============================================================================

/// 单个 Provider 的卡片展示
#[derive(IntoElement)]
pub struct ProviderCard {
    provider: ProviderStatus,
}

impl ProviderCard {
    pub fn new(provider: ProviderStatus) -> Self {
        Self { provider }
    }
}

impl RenderOnce for ProviderCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        
        let card_bg = theme.bg_panel;
        let card_border = theme.border_subtle;
        let sub_text = theme.text_secondary;

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
                        StatusLevel::Green | StatusLevel::Yellow => theme.element_active,
                        StatusLevel::Red => rgb(0xef4444).into(), // always red for errors
                    };
                    
                    let bar_bg = theme.border_subtle; // Use subtle border color for unfilled bar track

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
