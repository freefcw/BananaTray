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

        div()
            .flex()
            .h_full()
            .overflow_hidden()
            .child(self.render_provider_sidebar(&view_state.items, theme, viewport, cx))
            .child(self.render_provider_detail_panel(&view_state.detail, theme, viewport, cx))
    }
}
