mod detail;
mod sidebar;

use super::SettingsView;
use crate::theme::Theme;
use gpui::*;

impl SettingsView {
    // ========================================================================
    // Providers tab (双栏布局：sidebar + detail)
    // ========================================================================

    pub(in crate::app::settings_window) fn render_providers_tab(
        &mut self,
        settings: &crate::models::AppSettings,
        theme: &Theme,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> Div {
        let selected = self.state.borrow().settings_ui.selected_provider;
        let providers = self.state.borrow().provider_store.providers.clone();

        div()
            .flex()
            .h_full()
            .overflow_hidden()
            .child(
                self.render_provider_sidebar(&providers, selected, settings, theme, viewport, cx),
            )
            .child(
                self.render_provider_detail_panel(
                    &providers, selected, settings, theme, viewport, cx,
                ),
            )
    }
}
