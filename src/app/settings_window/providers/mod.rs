mod detail;
mod sidebar;

use super::SettingsView;
use crate::application::settings_providers_tab_view_state;
use crate::theme::Theme;
use gpui::*;

impl SettingsView {
    // ========================================================================
    // Providers tab (双栏布局：sidebar + detail)
    // ========================================================================

    pub(in crate::app::settings_window) fn render_providers_tab(
        &mut self,
        theme: &Theme,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> Div {
        let view_state = {
            let state = self.state.borrow();
            settings_providers_tab_view_state(&state.session)
        };

        // 竖线分隔符：上下各留 20px 断开
        let divider = div()
            .flex_none()
            .w(px(1.0))
            .py(px(20.0))
            .child(div().w_full().h_full().bg(theme.border.subtle));

        div()
            .flex()
            .h_full()
            .overflow_hidden()
            .child(self.render_provider_sidebar(&view_state.items, theme, viewport, cx))
            .child(divider)
            .child(self.render_provider_detail_panel(&view_state.detail, theme, viewport, cx))
    }
}
