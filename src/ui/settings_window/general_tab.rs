use super::components::{
    render_dark_card, render_divider, render_section_header, render_setting_row,
    render_toggle_setting, TAB_PADDING_BOTTOM, TAB_PADDING_X,
};
use super::SettingsView;
use crate::application::{AppAction, GlobalHotkeyError, SettingChange, SettingsModalState};
use crate::models::{AppSettings, RETENTION_PRESETS};
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::{
    render_action_button, render_history_retention_dropdown, render_hotkey_field_inline,
    ButtonSize, ButtonVariant, HistoryRetentionMenu,
};
use fc_ui::components::hotkey_input::HotkeyValue;
use gpui::{
    div, prelude::FluentBuilder, px, relative, Context, Div, Entity, InteractiveElement, Keystroke,
    MouseButton, ParentElement, Styled, Window,
};
use rust_i18n::t;

impl SettingsView {
    /// Render General settings tab — 系统与窗口、数据同步与历史、高级选项
    pub(super) fn render_general_tab(
        &mut self,
        settings: &AppSettings,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let state = self.state.clone();
        let login_checked = settings.system.start_at_login;
        let auto_hide_checked = settings.system.auto_hide_window;
        let debug_checked = settings.display.show_debug_tab;

        let cadence_mins = if settings.system.refresh_interval_mins == 0 {
            None
        } else {
            Some(settings.system.refresh_interval_mins)
        };
        let history_days = settings.history.retention_days;
        let history_open = state
            .borrow()
            .session
            .settings_ui
            .history_retention_dropdown_open;
        let confirming_clear_all = state.borrow().session.settings_ui.modal
            == SettingsModalState::ConfirmingClearAllHistory;

        div()
            .flex_col()
            .px(px(TAB_PADDING_X))
            .pb(px(TAB_PADDING_BOTTOM))
            // ═══════ 系统与窗口 ═══════
            .child(render_section_header(&t!("settings.section.system"), theme))
            .child(
                render_dark_card(theme)
                    .child(render_toggle_setting(
                        &state,
                        &t!("settings.start_at_login"),
                        &t!("settings.start_at_login.desc"),
                        login_checked,
                        theme,
                        SettingChange::ToggleStartAtLogin,
                    ))
                    .child(render_divider(theme))
                    .child(render_toggle_setting(
                        &state,
                        &t!("settings.auto_hide"),
                        &t!("settings.auto_hide.desc"),
                        auto_hide_checked,
                        theme,
                        SettingChange::ToggleAutoHideWindow,
                    ))
                    .child(render_divider(theme))
                    .child(self.render_global_hotkey_setting(settings, theme, window, cx)),
            )
            // ═══════ 数据同步与历史 ═══════
            .child(render_section_header(
                &t!("settings.section.data_sync"),
                theme,
            ))
            .child(
                render_dark_card(theme)
                    .child(render_setting_row(
                        &t!("settings.refresh_cadence"),
                        &t!("settings.refresh_cadence.desc"),
                        theme,
                        crate::ui::widgets::render_cadence_trigger(&state, cadence_mins, theme),
                    ))
                    .child(render_divider(theme))
                    .child(render_setting_row(
                        &t!("settings.history_retention"),
                        &t!("settings.history_retention.desc"),
                        theme,
                        render_global_history_retention(&state, history_days, history_open, theme),
                    ))
                    .child(render_divider(theme))
                    .child(render_clear_all_history_row(
                        confirming_clear_all,
                        &state,
                        theme,
                    )),
            )
            // ═══════ 开发者 ═══════
            .child(render_section_header(
                &t!("settings.section.developer"),
                theme,
            ))
            .child(render_dark_card(theme).child(render_toggle_setting(
                &state,
                &t!("settings.show_debug_tab"),
                &t!("settings.show_debug_tab.desc"),
                debug_checked,
                theme,
                SettingChange::ToggleShowDebugTab,
            )))
            // ═══════ Quit ═══════
            .child(self.render_quit_button(theme))
    }

    fn render_global_hotkey_setting(
        &mut self,
        settings: &AppSettings,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let input_entity = self.ensure_global_hotkey_input(&settings.system.global_hotkey, cx);
        let (captured_hotkey, is_recording) = {
            let input = input_entity.read(cx);
            (input.hotkey().cloned(), input.is_recording())
        };
        let captured_persisted = captured_hotkey.as_ref().map(Self::persist_hotkey_candidate);
        let is_dirty =
            captured_persisted.as_deref() != Some(settings.system.global_hotkey.as_str());
        let preview_error = if is_dirty {
            captured_persisted
                .as_deref()
                .and_then(|hotkey| runtime::global_hotkey::parse_hotkey_string(hotkey).err())
        } else {
            None
        };
        let can_save = !is_recording && captured_hotkey.is_some() && preview_error.is_none();
        let show_save = can_save && is_dirty;
        let (runtime_error, runtime_error_candidate) = {
            let state = self.state.borrow();
            (
                state.session.settings_ui.global_hotkey_error.clone(),
                state
                    .session
                    .settings_ui
                    .global_hotkey_error_candidate
                    .clone(),
            )
        };
        let hotkey_error = displayed_hotkey_error(
            preview_error,
            runtime_error.as_ref(),
            runtime_error_candidate.as_deref(),
            captured_persisted.as_deref(),
            is_dirty,
        );
        let trailing =
            self.render_global_hotkey_actions(&input_entity, show_save, theme, window, cx);

        div()
            .flex_col()
            .child(render_setting_row(
                &t!("settings.global_hotkey"),
                &t!("settings.global_hotkey.desc"),
                theme,
                trailing,
            ))
            .when_some(hotkey_error.as_ref(), |el, error| {
                el.child(Self::render_inline_note(
                    &Self::global_hotkey_error_text(error),
                    theme.status.error,
                ))
            })
            .when(is_recording, |el| {
                el.child(Self::render_inline_note(
                    &t!("settings.global_hotkey.hint"),
                    theme.text.muted,
                ))
            })
    }

    fn render_global_hotkey_actions(
        &self,
        input_entity: &Entity<fc_ui::components::hotkey_input::HotkeyInputState>,
        show_save: bool,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let view_entity = cx.entity().clone();
        let hotkey_chip = render_hotkey_field_inline(
            input_entity,
            t!("settings.global_hotkey.placeholder").to_string().into(),
            move |cx| {
                view_entity.update(cx, |_, cx| cx.notify());
            },
            theme,
            window,
            cx,
        );
        let state = self.state.clone();
        let save_input = input_entity.clone();

        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .child(hotkey_chip)
            .when(show_save, |actions| {
                actions.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .px(px(10.0))
                        .py(px(5.0))
                        .rounded(px(6.0))
                        .bg(theme.text.accent)
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.element.active)
                        .cursor_pointer()
                        .hover(|style| style.opacity(0.9))
                        .child(t!("settings.global_hotkey.save").to_string())
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            let hotkey = save_input
                                .read(cx)
                                .hotkey()
                                .cloned()
                                .map(|hotkey| Self::persist_hotkey_candidate(&hotkey))
                                .unwrap_or_default();
                            crate::bootstrap::dispatch_in_window(
                                &state,
                                AppAction::SaveGlobalHotkey(hotkey),
                                window,
                                cx,
                            );
                        }),
                )
            })
    }

    /// 行下方的紧凑右对齐备注（错误/提示复用）
    fn render_inline_note(text: &str, color: gpui::Hsla) -> Div {
        div().px(px(14.0)).pb(px(8.0)).flex().justify_end().child(
            div()
                .text_size(px(11.0))
                .text_color(color)
                .child(text.to_string()),
        )
    }

    fn global_hotkey_error_text(error: &GlobalHotkeyError) -> String {
        match error {
            GlobalHotkeyError::Empty => t!("settings.global_hotkey.error.empty").to_string(),
            GlobalHotkeyError::InvalidFormat => {
                t!("settings.global_hotkey.error.invalid").to_string()
            }
            GlobalHotkeyError::MissingModifier => {
                t!("settings.global_hotkey.error.modifier_required").to_string()
            }
            GlobalHotkeyError::ModifierOnly => {
                t!("settings.global_hotkey.error.key_required").to_string()
            }
            GlobalHotkeyError::Conflict(detail) => {
                t!("settings.global_hotkey.error.conflict", detail = detail).to_string()
            }
            GlobalHotkeyError::RegistrationFailed(detail) => {
                t!("settings.global_hotkey.error.register", detail = detail).to_string()
            }
            GlobalHotkeyError::PersistenceFailed => {
                t!("settings.global_hotkey.error.persist").to_string()
            }
        }
    }

    fn persist_hotkey_candidate(hotkey: &HotkeyValue) -> String {
        runtime::global_hotkey::format_hotkey_for_settings(&Keystroke {
            modifiers: hotkey.modifiers,
            key: hotkey.key.clone(),
            key_char: None,
        })
    }

    /// 退出按钮 — 使用 render_action_button (Danger 变体)，1/3 宽度右对齐
    fn render_quit_button(&self, theme: &Theme) -> Div {
        let state = self.state.clone();
        div()
            .mt(px(16.0))
            .flex()
            .justify_end()
            .child(div().w(relative(1.0 / 3.0)).child(render_action_button(
                &t!("settings.quit"),
                Some(("src/icons/switch.svg", theme.status.error)),
                ButtonVariant::Danger,
                ButtonSize::FullWidth,
                theme,
                move |_, window, cx| {
                    crate::bootstrap::dispatch_in_window(&state, AppAction::QuitApp, window, cx);
                },
            )))
    }
}

fn render_global_history_retention(
    state: &std::rc::Rc<std::cell::RefCell<runtime::AppState>>,
    days: u16,
    open: bool,
    theme: &Theme,
) -> Div {
    let toggle_state = state.clone();
    let select_state = state.clone();
    let options = RETENTION_PRESETS
        .into_iter()
        .map(|days| {
            (
                t!("settings.history_retention.days", n = days).to_string(),
                days,
            )
        })
        .collect();
    render_history_retention_dropdown(
        t!("settings.history_retention.days", n = days).to_string(),
        HistoryRetentionMenu {
            open,
            compact: false,
        },
        options,
        &days,
        theme,
        move |_, window, cx| {
            crate::bootstrap::dispatch_in_window(
                &toggle_state,
                AppAction::ToggleHistoryRetentionDropdown,
                window,
                cx,
            );
        },
        move |days, window, cx| {
            crate::bootstrap::dispatch_in_window(
                &select_state,
                AppAction::UpdateSetting(SettingChange::SetHistoryRetentionDays(days)),
                window,
                cx,
            );
        },
    )
}

fn render_clear_all_history_row(
    confirming: bool,
    state: &std::rc::Rc<std::cell::RefCell<runtime::AppState>>,
    theme: &Theme,
) -> Div {
    if confirming {
        let confirm_state = state.clone();
        let cancel_state = state.clone();
        return render_setting_row(
            &t!("settings.history_clear_all"),
            &t!("settings.history_clear_all.confirm"),
            theme,
            div()
                .flex()
                .gap(px(8.0))
                .child(render_action_button(
                    &t!("common.confirm"),
                    None,
                    ButtonVariant::Danger,
                    ButtonSize::Compact,
                    theme,
                    move |_, window, cx| {
                        crate::bootstrap::dispatch_in_window(
                            &confirm_state,
                            AppAction::ConfirmClearAllHistory,
                            window,
                            cx,
                        );
                    },
                ))
                .child(render_action_button(
                    &t!("common.cancel"),
                    None,
                    ButtonVariant::Subtle,
                    ButtonSize::Compact,
                    theme,
                    move |_, window, cx| {
                        crate::bootstrap::dispatch_in_window(
                            &cancel_state,
                            AppAction::CancelClearAllHistory,
                            window,
                            cx,
                        );
                    },
                )),
        );
    }

    let clear_state = state.clone();
    render_setting_row(
        &t!("settings.history_clear_all"),
        &t!("settings.history_clear_all.desc"),
        theme,
        render_action_button(
            &t!("settings.history_clear_all.action"),
            None,
            ButtonVariant::Danger,
            ButtonSize::Compact,
            theme,
            move |_, window, cx| {
                crate::bootstrap::dispatch_in_window(
                    &clear_state,
                    AppAction::BeginClearAllHistory,
                    window,
                    cx,
                );
            },
        ),
    )
}

fn displayed_hotkey_error(
    preview_error: Option<GlobalHotkeyError>,
    runtime_error: Option<&GlobalHotkeyError>,
    runtime_error_candidate: Option<&str>,
    current_candidate: Option<&str>,
    is_dirty: bool,
) -> Option<GlobalHotkeyError> {
    if let Some(error) = preview_error {
        return Some(error);
    }

    let runtime_error = runtime_error.cloned()?;
    if !is_dirty {
        return Some(runtime_error);
    }

    if runtime_error_candidate.is_some() && runtime_error_candidate == current_candidate {
        return Some(runtime_error);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_input_still_shows_runtime_error_for_same_failed_candidate() {
        let error = GlobalHotkeyError::Conflict("taken".to_string());

        assert_eq!(
            displayed_hotkey_error(
                None,
                Some(&error),
                Some("cmd-shift-s"),
                Some("cmd-shift-s"),
                true
            ),
            Some(error)
        );
    }

    #[test]
    fn dirty_input_hides_runtime_error_after_user_changes_candidate() {
        assert_eq!(
            displayed_hotkey_error(
                None,
                Some(&GlobalHotkeyError::Conflict("taken".to_string())),
                Some("cmd-shift-s"),
                Some("cmd-shift-k"),
                true
            ),
            None
        );
    }

    #[test]
    fn preview_error_takes_priority_over_runtime_error() {
        assert_eq!(
            displayed_hotkey_error(
                Some(GlobalHotkeyError::MissingModifier),
                Some(&GlobalHotkeyError::Conflict("taken".to_string())),
                Some("cmd-shift-s"),
                Some("s"),
                true
            ),
            Some(GlobalHotkeyError::MissingModifier)
        );
    }
}
