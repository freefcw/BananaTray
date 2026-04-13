mod detail;
mod newapi_form;
mod picker;
mod sidebar;

use super::SettingsView;
use crate::application::settings_providers_tab_view_state;
use crate::theme::Theme;
use gpui::{div, px, Context, Div, ParentElement, Styled, Window};

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

        // 状态同步：退出添加模式后释放表单输入实体
        if !view_state.adding_newapi && self.newapi_inputs.is_some() {
            self.clear_newapi_inputs();
        }

        // 右侧面板：三态切换
        let right_panel = if view_state.adding_newapi {
            // NewAPI 表单
            let is_editing = view_state.editing_newapi_data.is_some();
            self.render_newapi_form(
                is_editing,
                view_state.editing_newapi_data.as_ref(),
                theme,
                window,
                cx,
            )
        } else if view_state.adding_provider {
            // Provider 选择列表
            self.render_provider_picker(&view_state.available_providers, theme, cx)
        } else {
            // Provider 详情
            self.render_provider_detail_panel(&view_state.detail, theme, cx)
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
