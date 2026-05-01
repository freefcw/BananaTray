use super::components::{render_dark_card, render_divider, render_section_header};
use super::SettingsView;
use crate::application::{AppAction, GlobalHotkeyError, SettingChange};
use crate::models::AppSettings;
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::{
    render_action_button, render_hotkey_field_inline, render_icon_row, ButtonVariant,
};
use adabraka_ui::components::hotkey_input::HotkeyValue;
use gpui::{
    div, prelude::FluentBuilder, px, relative, rgb, Context, Div, InteractiveElement, Keystroke,
    MouseButton, ParentElement, Styled, Window,
};
use rust_i18n::t;

// 设计稿颜色常量 — 各设置项的彩色图标背景
const ICON_BG_LOGIN: u32 = 0x3b30a6; // 紫蓝色 (Start at Login)
const ICON_BG_REFRESH: u32 = 0xb55a10; // 琥珀橙色 (Refresh Rate)
const ICON_BG_NOTIF: u32 = 0xa62828; // 深红色 (Quota Notifications)
const ICON_BG_SOUND: u32 = 0x6b3fa0; // 紫色 (Notification Sound)
const ICON_BG_HOTKEY: u32 = 0x165a93; // 深蓝色 (Global Hotkey)
const ICON_FG: u32 = 0xffffff; // 图标前景色统一白色

impl SettingsView {
    /// Render General settings tab — 匹配设计稿风格
    pub(super) fn render_general_tab(
        &mut self,
        settings: &AppSettings,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let state = self.state.clone();

        // ── SYSTEM section ───────────────────────────────────
        let login_state = state.clone();
        let login_checked = settings.system.start_at_login;

        // ── AUTOMATION section ───────────────────────────────
        let notif_state = state.clone();
        let notif_checked = settings.notification.session_quota_notifications;
        let sound_state = state.clone();
        let sound_checked = settings.notification.notification_sound;

        // Cadence dropdown (复用现有组件)
        let cadence_mins = if settings.system.refresh_interval_mins == 0 {
            None
        } else {
            Some(settings.system.refresh_interval_mins)
        };

        div()
            .flex_col()
            .px(px(16.0))
            .pb(px(16.0))
            // ═══════ SYSTEM ═══════
            .child(render_section_header(&t!("settings.section.system"), theme))
            .child(
                render_dark_card(theme)
                    .child(Self::render_icon_switch_row(
                        "src/icons/switch.svg",
                        rgb(ICON_FG).into(),
                        rgb(ICON_BG_LOGIN).into(),
                        &t!("settings.start_at_login"),
                        &t!("settings.start_at_login.desc"),
                        login_checked,
                        theme,
                        move |_, window, cx| {
                            runtime::dispatch_in_window(
                                &login_state,
                                AppAction::UpdateSetting(SettingChange::ToggleStartAtLogin),
                                window,
                                cx,
                            );
                        },
                    ))
                    .child(render_divider(theme))
                    .child(self.render_global_hotkey_setting(settings, theme, window, cx)),
            )
            // ═══════ AUTOMATION ═══════
            .child(render_section_header(
                &t!("settings.section.automation"),
                theme,
            ))
            .child(
                render_dark_card(theme)
                    // Refresh Rate — 带下拉选择器
                    .child(Self::render_icon_dropdown_row(
                        "src/icons/refresh.svg",
                        rgb(ICON_FG).into(),
                        rgb(ICON_BG_REFRESH).into(),
                        &t!("settings.refresh_cadence"),
                        &t!("settings.refresh_cadence.desc"),
                        theme,
                        crate::ui::widgets::render_cadence_trigger(&state, cadence_mins, theme),
                    ))
                    .child(render_divider(theme))
                    // Quota Notifications
                    .child(Self::render_icon_switch_row(
                        "src/icons/status.svg",
                        rgb(ICON_FG).into(),
                        rgb(ICON_BG_NOTIF).into(),
                        &t!("settings.session_quota_notifications"),
                        &t!("settings.session_quota_notifications.desc"),
                        notif_checked,
                        theme,
                        move |_, window, cx| {
                            runtime::dispatch_in_window(
                                &notif_state,
                                AppAction::UpdateSetting(
                                    SettingChange::ToggleSessionQuotaNotifications,
                                ),
                                window,
                                cx,
                            );
                        },
                    ))
                    .child(render_divider(theme))
                    // Notification Sound
                    .child(Self::render_icon_switch_row(
                        "src/icons/usage.svg",
                        rgb(ICON_FG).into(),
                        rgb(ICON_BG_SOUND).into(),
                        &t!("settings.notification_sound"),
                        &t!("settings.notification_sound.desc"),
                        sound_checked,
                        theme,
                        move |_, window, cx| {
                            runtime::dispatch_in_window(
                                &sound_state,
                                AppAction::UpdateSetting(SettingChange::ToggleNotificationSound),
                                window,
                                cx,
                            );
                        },
                    )),
            )
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
        let state = self.state.clone();
        let view_entity = cx.entity().clone();
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
        let save_input = input_entity.clone();

        // ── 紧凑内联热键录入控件 ──
        let hotkey_chip = render_hotkey_field_inline(
            &input_entity,
            t!("settings.global_hotkey.placeholder").to_string().into(),
            move |cx| {
                view_entity.update(cx, |_, cx| cx.notify());
            },
            theme,
            window,
            cx,
        );

        // ── trailing 组合：hotkey chip + 按需出现的 save 按钮 ──
        let trailing = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .child(hotkey_chip)
            .when(show_save, |el| {
                el.child(
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
                            runtime::dispatch_in_window(
                                &state,
                                AppAction::SaveGlobalHotkey(hotkey),
                                window,
                                cx,
                            );
                        }),
                )
            });

        // ── 整体布局：icon_row + 可选的 inline note ──
        div()
            .flex_col()
            .child(render_icon_row(
                "src/icons/settings.svg",
                rgb(ICON_FG).into(),
                rgb(ICON_BG_HOTKEY).into(),
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
                true,
                theme,
                move |_, window, cx| {
                    runtime::dispatch_in_window(&state, AppAction::QuitApp, window, cx);
                },
            )))
    }
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
