mod detail;
mod newapi_form;
mod picker;
mod script_provider_form;
mod shared;
mod sidebar;
pub(crate) mod token_input_panel;

use super::SettingsView;
use crate::application::{settings_providers_tab_view_state, SettingsProviderRightPaneViewState};
use crate::theme::Theme;
use gpui::{div, px, Context, Div, ParentElement, Styled, Window};
use script_provider_form::ScriptProviderFormView;

impl SettingsView {
    // ========================================================================
    // Providers tab (双栏布局：sidebar + detail)
    // ========================================================================

    pub(in crate::ui::settings_window) fn render_providers_tab(
        &mut self,
        theme: &Theme,
        window: &mut Window,
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

        // 状态同步：退出 NewAPI 表单后释放输入实体
        let is_newapi_form = matches!(
            view_state.right_pane,
            SettingsProviderRightPaneViewState::NewApiForm { .. }
        );
        if !is_newapi_form && self.newapi_inputs.is_some() {
            self.clear_newapi_inputs();
        }
        let is_script_form = matches!(
            view_state.right_pane,
            SettingsProviderRightPaneViewState::ScriptProviderForm { .. }
        );
        if !is_script_form && self.script_provider_inputs.is_some() {
            self.clear_script_provider_inputs();
        }

        let right_panel = match &view_state.right_pane {
            SettingsProviderRightPaneViewState::NewApiForm { edit_data } => {
                self.render_newapi_form(edit_data.is_some(), edit_data.as_ref(), theme, window, cx)
            }
            SettingsProviderRightPaneViewState::ScriptProviderForm {
                edit_data,
                testing,
                test_result,
            } => self.render_script_provider_form(
                ScriptProviderFormView {
                    edit_data: edit_data.as_ref(),
                    test_result: test_result.as_ref(),
                    is_testing: *testing,
                },
                theme,
                window,
                cx,
            ),
            SettingsProviderRightPaneViewState::ProviderPicker => {
                self.render_provider_picker(&view_state.available_providers, theme, cx)
            }
            SettingsProviderRightPaneViewState::Detail => {
                self.render_provider_detail_panel(&view_state.detail, theme, cx)
            }
        };

        div()
            .flex()
            .h_full()
            .overflow_hidden()
            .child(self.render_provider_sidebar(&view_state.items, theme, cx))
            .child(divider)
            .child(right_panel)
    }
}
