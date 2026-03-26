mod general_tab;
mod providers_tab;

use super::AppState;
use crate::theme::Theme;
use gpui::*;
use log::{error, info};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

// ============================================================================
// Settings Tab 枚举
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Providers,
    Display,
    Advanced,
    About,
}

// ============================================================================
// 设置窗口管理
// ============================================================================

thread_local! {
    static SETTINGS_WINDOW: RefCell<Option<WindowHandle<SettingsView>>> = const { RefCell::new(None) };
}

pub fn schedule_open_settings_window(state: Rc<RefCell<AppState>>, cx: &mut App) {
    info!(target: "settings", "scheduled async settings window open");
    let async_cx = cx.to_async();
    let delayed_cx = async_cx.clone();
    async_cx
        .foreground_executor()
        .spawn(async move {
            smol::Timer::after(Duration::from_millis(10)).await;
            let _ = delayed_cx.update(|cx| {
                open_settings_window(state, cx);
            });
        })
        .detach();
}

fn open_settings_window(state: Rc<RefCell<AppState>>, cx: &mut App) {
    info!(target: "settings", "requested settings window");

    // Try to activate an existing settings window first
    let activated_existing = SETTINGS_WINDOW.with(|slot| {
        if let Some(handle) = slot.borrow().as_ref() {
            info!(target: "settings", "existing settings window found, attempting to activate it");
            let ok = handle
                .update(cx, |_, window, _| {
                    window.show_window();
                    window.activate_window();
                })
                .is_ok();
            if !ok {
                info!(target: "settings", "existing handle is stale, clearing");
            }
            ok
        } else {
            false
        }
    });

    if activated_existing {
        cx.activate(true);
        info!(target: "settings", "activated existing settings window");
        return;
    }

    SETTINGS_WINDOW.with(|slot| {
        *slot.borrow_mut() = None;
    });

    let settings_state = state.clone();
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(640.0), px(700.0)), cx)),
            window_min_size: Some(size(px(560.0), px(500.0))),
            titlebar: Some(TitlebarOptions {
                title: Some("BananaTray Settings".into()),
                ..Default::default()
            }),
            kind: WindowKind::Normal,
            ..Default::default()
        },
        |_window, cx| cx.new(|cx| SettingsView::new(settings_state, cx)),
    );

    if let Ok(handle) = result {
        info!(target: "settings", "opened new settings window");
        cx.activate(true);
        let _ = handle.update(cx, |_, window, _| {
            window.show_window();
            window.activate_window();
        });
        info!(target: "settings", "requested app/window activation for settings window");
        SETTINGS_WINDOW.with(|slot| {
            *slot.borrow_mut() = Some(handle);
        });
    } else if let Err(err) = result {
        error!(target: "settings", "failed to open settings window: {err:?}");
    }
}

// ============================================================================
// 设置视图
// ============================================================================

pub(super) struct SettingsView {
    pub(super) state: Rc<RefCell<AppState>>,
}

impl SettingsView {
    fn new(state: Rc<RefCell<AppState>>, _cx: &mut Context<Self>) -> Self {
        info!(target: "settings", "constructing settings view");
        Self { state }
    }

    pub(super) fn preferences_theme() -> Theme {
        Theme {
            bg_base: rgb(0xf2f2f7).into(),
            bg_panel: rgb(0xffffff).into(),
            bg_subtle: rgb(0xebebf0).into(),
            bg_card: rgb(0xe0ecfb).into(),
            bg_card_active: rgb(0xe0ecfb).into(),
            text_primary: rgb(0x1c1c1e).into(),
            text_secondary: rgb(0x6e6e73).into(),
            text_muted: rgb(0x8e8e93).into(),
            text_accent: rgb(0x007aff).into(),
            text_accent_soft: rgb(0xc2dcf7).into(),
            border_subtle: rgb(0xdcdce2).into(),
            border_strong: rgb(0xc7c7cc).into(),
            element_active: rgb(0xffffff).into(),
            element_selected: rgb(0x007aff).into(),
            status_success: rgb(0x34c759).into(),
            status_error: rgb(0xff3b30).into(),
            status_warning: rgb(0xff9f0a).into(),
            progress_track: rgb(0xe5e5ea).into(),
        }
    }

    fn render_icon_tab(
        &self,
        icon_path: &'static str,
        label: &str,
        active: bool,
        _theme: &Theme,
    ) -> Div {
        let active_color: Hsla = rgb(0x007aff).into();
        let inactive_color: Hsla = rgb(0x8e8e93).into();
        let active_bg: Hsla = rgb(0xe3eefa).into();

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.0))
            .px(px(14.0))
            .pt(px(4.0))
            .pb(px(8.0))
            .cursor_pointer()
            .border_b_2()
            .border_color(if active {
                active_color
            } else {
                transparent_black()
            })
            .child(
                div()
                    .w(px(30.0))
                    .h(px(30.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.0))
                    .bg(if active {
                        active_bg
                    } else {
                        transparent_black()
                    })
                    .child(svg().path(icon_path).size(px(17.0)).text_color(if active {
                        active_color
                    } else {
                        inactive_color
                    })),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .font_weight(if active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if active { active_color } else { inactive_color })
                    .child(label.to_string()),
            )
    }

    pub(super) fn render_settings_checkbox(&self, checked: bool, theme: &Theme) -> Div {
        let blue: Hsla = rgb(0x007aff).into();
        div()
            .mt(px(1.0))
            .w(px(18.0))
            .h(px(18.0))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .border_1()
            .border_color(if checked { blue } else { theme.border_strong })
            .bg(if checked { blue } else { transparent_black() })
            .text_size(px(11.0))
            .font_weight(FontWeight::BOLD)
            .text_color(if checked {
                theme.element_active
            } else {
                transparent_black()
            })
            .child(if checked { "✓" } else { "" })
    }

    /// A section label like "SYSTEM", "USAGE", etc.
    pub(super) fn render_section_label(title: &str, theme: &Theme) -> Div {
        div()
            .text_size(px(12.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text_muted)
            .px(px(4.0))
            .pb(px(6.0))
            .child(title.to_string())
    }

    /// A white rounded card that groups settings rows (macOS grouped-style)
    pub(super) fn render_card() -> Div {
        div()
            .flex_col()
            .rounded(px(10.0))
            .bg(rgb(0xffffff))
            .overflow_hidden()
    }

    /// Horizontal 1px divider inside a card (with left indent)
    pub(super) fn render_card_separator() -> Div {
        div().h(px(0.5)).w_full().ml(px(14.0)).bg(rgb(0xe5e5ea))
    }

    /// Render a placeholder page for unimplemented tabs
    fn render_placeholder_tab(tab: SettingsTab, theme: &Theme) -> Div {
        let title = match tab {
            SettingsTab::Display => "Display",
            SettingsTab::Advanced => "Advanced",
            SettingsTab::About => "About",
            _ => "",
        };
        let desc = match tab {
            SettingsTab::Display => {
                "Customize appearance, menu bar icon style, and notification preferences."
            }
            SettingsTab::Advanced => "Debug logging, network proxy, and other advanced options.",
            SettingsTab::About => "BananaTray version info, licenses, and acknowledgements.",
            _ => "",
        };
        div()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .px(px(40.0))
            .child(
                div()
                    .flex_col()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(theme.text_muted)
                            .text_align(TextAlign::Center)
                            .line_height(relative(1.5))
                            .child(desc.to_string()),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .px(px(12.0))
                            .py(px(4.0))
                            .rounded(px(6.0))
                            .bg(theme.bg_subtle)
                            .text_size(px(11.5))
                            .text_color(theme.text_secondary)
                            .child("Coming soon"),
                    ),
            )
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Self::preferences_theme();
        let settings = self.state.borrow().settings.clone();
        let active_tab = self.state.borrow().settings_tab;

        let tabs: &[(&str, &str, SettingsTab)] = &[
            ("src/icons/settings.svg", "General", SettingsTab::General),
            (
                "src/icons/overview.svg",
                "Providers",
                SettingsTab::Providers,
            ),
            ("src/icons/display.svg", "Display", SettingsTab::Display),
            ("src/icons/advanced.svg", "Advanced", SettingsTab::Advanced),
            ("src/icons/about.svg", "About", SettingsTab::About),
        ];

        let mut tab_bar = div()
            .flex()
            .justify_center()
            .pt(px(4.0))
            .border_b_1()
            .border_color(theme.border_subtle);

        for &(icon, label, tab) in tabs {
            let state = self.state.clone();
            tab_bar = tab_bar.child(
                self.render_icon_tab(icon, label, active_tab == tab, &theme)
                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                        state.borrow_mut().settings_tab = tab;
                        window.refresh();
                    }),
            );
        }

        // ── Content area (depends on active tab) ─────────────
        let content = div()
            .id("settings-content")
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .child(match active_tab {
                SettingsTab::General => self.render_general_tab(&settings, &theme),
                SettingsTab::Providers => self.render_providers_tab(&settings, &theme),
                _ => Self::render_placeholder_tab(active_tab, &theme),
            });

        div()
            .size_full()
            .bg(theme.bg_base)
            .text_color(theme.text_primary)
            .child(div().flex_col().size_full().child(tab_bar).child(content))
    }
}
