mod actions;
mod header;
mod info;
mod quota_visibility;
mod settings_section;
mod usage;

use super::super::SettingsView;
use crate::application::{AppAction, SettingsProviderDetailViewState};
use crate::models::ProviderId;
use crate::runtime::AppState;
use crate::theme::Theme;
use gpui::{
    div, px, Context, Div, Entity, InteractiveElement, MouseButton, MouseDownEvent, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub(super) struct DetailActionDispatcher {
    state: Rc<RefCell<AppState>>,
    view_entity: Entity<SettingsView>,
}

impl DetailActionDispatcher {
    fn new(state: Rc<RefCell<AppState>>, view_entity: Entity<SettingsView>) -> Self {
        Self { state, view_entity }
    }

    pub(super) fn dispatch(&self, action: AppAction, window: &mut Window, cx: &mut gpui::App) {
        crate::bootstrap::dispatch_in_window(&self.state, action, window, cx);
    }

    pub(super) fn clear_token_input(&self, cx: &mut gpui::App) {
        self.view_entity.update(cx, |view, _| {
            view.clear_token_input();
        });
    }

    pub(super) fn dispatch_after_clearing_token_input(
        &self,
        action: AppAction,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        self.clear_token_input(cx);
        self.dispatch(action, window, cx);
    }

    pub(super) fn interactive_action(
        &self,
        action: impl Fn() -> AppAction + 'static,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static {
        let dispatcher = self.clone();
        move |_, window, cx| {
            dispatcher.dispatch(action(), window, cx);
        }
    }

    pub(super) fn interactive_cleanup_action(
        &self,
        action: impl Fn() -> AppAction + 'static,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static {
        let dispatcher = self.clone();
        move |_, window, cx| {
            dispatcher.dispatch_after_clearing_token_input(action(), window, cx);
        }
    }
}

impl SettingsView {
    pub(in crate::ui::settings_window) fn render_provider_detail_panel(
        &mut self,
        detail: &SettingsProviderDetailViewState,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let dispatcher = DetailActionDispatcher::new(self.state.clone(), cx.entity().clone());
        let mut content = render_detail_content(detail, theme, &dispatcher);

        if detail.show_quota_visibility {
            content = content.child(quota_visibility::render_quota_visibility_section(
                detail.id.clone(),
                &detail.quota_visibility,
                &dispatcher,
                theme,
            ));
        }

        content = content.child(settings_section::render_settings_section(
            self,
            detail,
            &dispatcher,
            theme,
            cx,
        ));

        render_detail_scroll(content)
    }
}

fn render_detail_content(
    detail: &SettingsProviderDetailViewState,
    theme: &Theme,
    dispatcher: &DetailActionDispatcher,
) -> Div {
    div()
        .flex_col()
        .px(px(24.0))
        .pt(px(20.0))
        .pb(px(60.0))
        .child(header::render_header(detail, dispatcher, theme))
        .child(info::render_info_table(&detail.info, theme))
        .child(usage::render_usage_section(
            &detail.usage,
            theme,
            detail.quota_display_mode,
        ))
}

fn render_detail_scroll(content: Div) -> Div {
    div().flex_col().flex_1().h_full().overflow_hidden().child(
        div()
            .id("provider-detail-scroll")
            .flex_col()
            .h_full()
            .overflow_y_scroll()
            .child(content),
    )
}

fn icon_button(
    id: Option<&'static str>,
    icon: &'static str,
    size: f32,
    theme: &Theme,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
) -> Div {
    if let Some(id) = id {
        return div().child(
            div()
                .id(id)
                .w(px(28.0))
                .h(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .bg(theme.bg.subtle)
                .cursor_pointer()
                .hover(|s| s.opacity(0.8))
                .child(crate::ui::widgets::render_svg_icon(
                    icon,
                    px(size),
                    theme.text.muted,
                ))
                .on_mouse_down(MouseButton::Left, on_click),
        );
    }

    div()
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .bg(theme.bg.subtle)
        .cursor_pointer()
        .hover(|s| s.opacity(0.8))
        .child(crate::ui::widgets::render_svg_icon(
            icon,
            px(size),
            theme.text.muted,
        ))
        .on_mouse_down(MouseButton::Left, on_click)
}

fn provider_id_for_action(id: &ProviderId) -> ProviderId {
    id.clone()
}
