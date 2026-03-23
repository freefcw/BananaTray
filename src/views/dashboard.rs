use gpui::*;

use crate::models::{AppTheme, ProviderStatus};
use crate::views::provider_card::ProviderCard;

// ============================================================================
// 仪表盘面板
// ============================================================================

/// 主仪表盘：展示所有 Provider 的用量概览
#[derive(IntoElement)]
pub struct Dashboard {
    providers: Vec<ProviderStatus>,
    theme: AppTheme,
}

impl Dashboard {
    pub fn new(providers: Vec<ProviderStatus>, theme: AppTheme) -> Self {
        Self { providers, theme }
    }
}

impl RenderOnce for Dashboard {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let sub_text = match self.theme {
            AppTheme::Dark => rgb(0x94a3b8),
            AppTheme::Light => rgb(0x64748b),
        };

        let enabled_providers: Vec<_> = self
            .providers
            .iter()
            .filter(|p| p.enabled)
            .cloned()
            .collect();

        let provider_count = enabled_providers.len();
        let theme = self.theme;

        div()
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            // 标题区域
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child("📊 Quota Overview"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(sub_text)
                                    .child(format!(
                                        "Monitoring {} provider{}",
                                        provider_count,
                                        if provider_count != 1 { "s" } else { "" }
                                    )),
                            ),
                    ),
            )
            // Provider 卡片网格（2 列）
            .child(
                Self::render_card_grid(enabled_providers, theme),
            )
    }
}

impl Dashboard {
    /// 渲染 Provider 卡片网格
    fn render_card_grid(
        providers: Vec<ProviderStatus>,
        theme: AppTheme,
    ) -> impl IntoElement {
        // 将 providers 分成每行 2 个的网格布局
        let mut rows: Vec<Div> = Vec::new();

        let chunks: Vec<Vec<ProviderStatus>> = providers
            .chunks(2)
            .map(|chunk| chunk.to_vec())
            .collect();

        for chunk in chunks {
            let mut row = div().flex().gap(px(12.0)).w_full();
            for provider in chunk {
                row = row.child(
                    div()
                        .flex_1()
                        .child(ProviderCard::new(provider, theme)),
                );
            }
            rows.push(row);
        }

        div().flex().flex_col().gap(px(12.0)).children(rows)
    }
}
