use crate::models::{
    AppSettings, AppTheme, ConnectionStatus, NavTab, ProviderKind, ProviderStatus, StatusLevel,
};
use crate::theme::Theme;
use crate::views::settings::SettingsPanel;
use gpui::*;
use log::{error, info, warn};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

const SETTINGS_ICON: &str = "src/icons/settings.svg";
const SWITCH_ICON: &str = "src/icons/switch.svg";
const USAGE_ICON: &str = "src/icons/usage.svg";
const STATUS_ICON: &str = "src/icons/status.svg";
const AUTO_HIDE_ICON: &str = "src/icons/display.svg";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Providers,
    Display,
    Advanced,
    About,
}

// ============================================================================
// 外部持久状态 (不随窗口销毁)
// ============================================================================

/// 应用持久状态，在窗口生命周期之外保持
pub struct AppState {
    pub providers: Vec<ProviderStatus>,
    pub settings: AppSettings,
    pub active_tab: NavTab,
    pub last_provider_kind: ProviderKind,
    pub manager: Arc<crate::providers::ProviderManager>,
    pub refreshed: bool,
    pub settings_tab: SettingsTab,
}

impl AppState {
    pub fn new() -> Self {
        let settings = match crate::settings_store::load() {
            Ok(settings) => {
                info!(target: "settings", "loaded settings from {}", crate::settings_store::config_path().display());
                settings
            }
            Err(err) => {
                warn!(target: "settings", "failed to load saved settings: {err}");
                AppSettings::default()
            }
        };
        let manager = Arc::new(crate::providers::ProviderManager::new());
        let providers = manager.initial_statuses();
        Self {
            providers,
            settings,
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            manager,
            refreshed: false,
            settings_tab: SettingsTab::General,
        }
    }
}

fn persist_settings(settings: &AppSettings) {
    match crate::settings_store::save(settings) {
        Ok(path) => {
            info!(target: "settings", "saved settings to {}", path.display());
        }
        Err(err) => {
            warn!(target: "settings", "failed to save settings: {err}");
        }
    }
}

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

pub fn open_settings_window(state: Rc<RefCell<AppState>>, cx: &mut App) {
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
                // Handle is stale (window was closed); clear it
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
            window_bounds: Some(WindowBounds::centered(size(px(460.0), px(700.0)), cx)),
            window_min_size: Some(size(px(400.0), px(500.0))),
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

pub struct SettingsView {
    state: Rc<RefCell<AppState>>,
}

impl SettingsView {
    pub fn new(state: Rc<RefCell<AppState>>, _cx: &mut Context<Self>) -> Self {
        info!(target: "settings", "constructing settings view");
        Self { state }
    }

    fn preferences_theme() -> Theme {
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

    fn render_settings_checkbox(&self, checked: bool, theme: &Theme) -> Div {
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
    fn render_section_label(title: &str, theme: &Theme) -> Div {
        div()
            .text_size(px(12.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text_muted)
            .px(px(4.0))
            .pb(px(6.0))
            .child(title.to_string())
    }

    /// A white rounded card that groups settings rows (macOS grouped-style)
    fn render_card() -> Div {
        div()
            .flex_col()
            .rounded(px(10.0))
            .bg(rgb(0xffffff))
            .overflow_hidden()
    }

    /// Horizontal 1px divider inside a card (with left indent)
    fn render_card_separator() -> Div {
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

    /// Render Providers settings tab
    fn render_providers_tab(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let github_token = settings.providers.github_token.clone().unwrap_or_default();
        let has_token = !github_token.is_empty();

        div()
            .flex_col()
            .flex_1()
            .px(px(16.0))
            .pt(px(16.0))
            .pb(px(20.0))
            // ═══════ COPILOT ═══════
            .child(
                div()
                    .flex_col()
                    .child(Self::render_section_label("GITHUB COPILOT", theme))
                    .child(
                        Self::render_card()
                            // GitHub Token
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .flex_1()
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("GitHub Token"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("Classic PAT with 'copilot' scope. Auto-detects plan & quota."),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_shrink_0()
                                            .items_center()
                                            .ml(px(12.0))
                                            .px(px(10.0))
                                            .py(px(4.0))
                                            .rounded(px(6.0))
                                            .border_1()
                                            .border_color(theme.border_strong)
                                            .bg(theme.element_active)
                                            .text_size(px(12.0))
                                            .text_color(if github_token.is_empty() {
                                                theme.text_muted
                                            } else {
                                                theme.text_primary
                                            })
                                            .child(if github_token.is_empty() {
                                                "Not set".to_string()
                                            } else {
                                                format!("{}...", &github_token[..8.min(github_token.len())])
                                            }),
                                    ),
                            )
                            .child(Self::render_card_separator())
                            // Status
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Status"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child(if has_token {
                                                        "Token configured. Copilot quota will be auto-detected."
                                                    } else {
                                                        "Set your GitHub token to enable Copilot monitoring."
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .px(px(8.0))
                                            .py(px(4.0))
                                            .rounded(px(6.0))
                                            .bg(if has_token {
                                                theme.status_success
                                            } else {
                                                theme.status_warning
                                            })
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.element_active)
                                            .child(if has_token { "Ready" } else { "Not Configured" }),
                                    ),
                            ),
                    ),
            )
            // ═══════ CONFIG FILE INFO ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(Self::render_section_label("CONFIGURATION", theme))
                    .child(
                        Self::render_card()
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .flex_1()
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Config File Location"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.5))
                                                    .text_color(theme.text_muted)
                                                    .child("~/Library/Application Support/BananaTray/settings.json"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .mt(px(4.0))
                                                    .child("Click 'Edit Config' to open the file in your default editor."),
                                            ),
                                    ),
                            )
                            .child(Self::render_card_separator())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .text_color(theme.text_secondary)
                                            .child("Example format:"),
                                    )
                                    .child(
                                        div()
                                            .px(px(12.0))
                                            .py(px(6.0))
                                            .rounded(px(8.0))
                                            .bg(theme.text_accent)
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.element_active)
                                            .cursor_pointer()
                                            .child("Edit Config")
                                            .on_mouse_down(MouseButton::Left, |_, _, _| {
                                                let path = dirs::config_dir()
                                                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                                                    .join("BananaTray")
                                                    .join("settings.json");
                                                // 确保目录存在
                                                if let Some(parent) = path.parent() {
                                                    let _ = std::fs::create_dir_all(parent);
                                                }
                                                // 用系统默认编辑器打开
                                                let _ = std::process::Command::new("open")
                                                    .arg(&path)
                                                    .spawn();
                                            }),
                                    ),
                            ),
                    ),
            )
            // ═══════ ENV VARIABLES INFO ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(Self::render_section_label("ALTERNATIVE", theme))
                    .child(
                        Self::render_card()
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Environment Variables"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("You can also set GITHUB_USERNAME and GITHUB_TOKEN environment variables instead of using the config file."),
                                            ),
                                    ),
                            ),
                    ),
            )
    }

    /// Render General settings tab
    fn render_general_tab(&self, settings: &AppSettings, theme: &Theme) -> Div {
        div()
            .flex_col()
            .flex_1()
            .px(px(16.0))
            .pt(px(16.0))
            .pb(px(20.0))
            // ═══════ SYSTEM ═══════
            .child(
                div()
                    .flex_col()
                    .child(Self::render_section_label("SYSTEM", theme))
                    .child(
                        Self::render_card()
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(self.render_settings_checkbox(false, theme))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Start at Login"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("Automatically opens BananaTray when you start your Mac."),
                                            ),
                                    ),
                            ),
                    ),
            )
            // ═══════ USAGE ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(Self::render_section_label("USAGE", theme))
                    .child(
                        Self::render_card()
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(self.render_settings_checkbox(true, theme))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Show cost summary"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("Reads local usage logs. Shows today + last 30 days cost in the menu."),
                                            )
                                            .child(
                                                div()
                                                    .flex_col()
                                                    .gap(px(1.0))
                                                    .mt(px(4.0))
                                                    .text_size(px(11.5))
                                                    .text_color(theme.text_muted)
                                                    .child(div().child("Auto-refresh: hourly · Timeout: 10m"))
                                                    .child(div().child("Claude: no data yet"))
                                                    .child(div().child("Codex: no data yet")),
                                            ),
                                    ),
                            ),
                    ),
            )
            // ═══════ AUTOMATION ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(Self::render_section_label("AUTOMATION", theme))
                    .child(
                        Self::render_card()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .flex_1()
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Refresh cadence"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("How often BananaTray polls providers in the background."),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_shrink_0()
                                            .items_center()
                                            .gap(px(3.0))
                                            .ml(px(12.0))
                                            .px(px(10.0))
                                            .py(px(4.0))
                                            .rounded(px(6.0))
                                            .border_1()
                                            .border_color(theme.border_strong)
                                            .bg(theme.element_active)
                                            .text_size(px(12.0))
                                            .text_color(theme.text_primary)
                                            .child(format!("{} sec", settings.refresh_interval_secs))
                                            .child(
                                                div()
                                                    .text_size(px(7.0))
                                                    .text_color(theme.text_muted)
                                                    .ml(px(1.0))
                                                    .child("▲▼"),
                                            ),
                                    ),
                            )
                            .child(Self::render_card_separator())
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(self.render_settings_checkbox(true, theme))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Check provider status"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("Polls provider status pages, surfacing incidents in the icon and menu."),
                                            ),
                                    ),
                            )
                            .child(Self::render_card_separator())
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(self.render_settings_checkbox(true, theme))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Session quota notifications"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("Notifies when the 5-hour session quota hits 0% and when it becomes available again."),
                                            ),
                                    ),
                            ),
                    ),
            )
            // ═══════ Quit ═══════
            .child(
                div()
                    .flex()
                    .justify_center()
                    .mt(px(12.0))
                    .pb(px(4.0))
                    .child(
                        div()
                            .px(px(22.0))
                            .py(px(8.0))
                            .rounded_full()
                            .bg(theme.status_error)
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.element_active)
                            .cursor_pointer()
                            .child("Quit BananaTray")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.quit();
                            }),
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

// ============================================================================
// 窗口视图 (可多次创建/销毁)
// ============================================================================

pub struct AppView {
    state: Rc<RefCell<AppState>>,
    pub _activation_sub: Option<gpui::Subscription>,
}

impl AppView {
    pub fn new(state: Rc<RefCell<AppState>>, cx: &mut Context<Self>) -> Self {
        let theme = match state.borrow().settings.theme {
            AppTheme::Light => Theme::light(),
            AppTheme::Dark => Theme::dark(),
        };
        cx.set_global(theme);

        // 只在首次打开时刷新 provider 数据
        if !state.borrow().refreshed {
            info!(target: "providers", "starting first background refresh pass");
            state.borrow_mut().refreshed = true;
            Self::start_background_refresh(state.borrow().manager.clone(), cx);
        }

        Self {
            state,
            _activation_sub: None,
        }
    }

    fn start_background_refresh(
        manager: Arc<crate::providers::ProviderManager>,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().clone();
        cx.spawn(|_view, cx: &mut gpui::AsyncApp| {
            let async_cx = cx.clone();
            async move {
                let all_kinds = crate::models::ProviderKind::all().to_vec();
                for kind in all_kinds {
                    let mgr = manager.clone();
                    info!(target: "providers", "refreshing provider {:?}", kind);
                    let result =
                        smol::unblock(move || smol::block_on(mgr.refresh_provider(kind))).await;

                    let entity = entity.clone();
                    match result {
                        Ok(quotas) => {
                            info!(target: "providers", "provider {:?} refresh succeeded with {} quotas", kind, quotas.len());
                            async_cx
                                .update(|cx| {
                                    entity.update(cx, |view, cx| {
                                        let mut s = view.state.borrow_mut();
                                        if let Some(p) =
                                            s.providers.iter_mut().find(|p| p.kind == kind)
                                        {
                                            p.quotas = quotas;
                                            p.connection = ConnectionStatus::Connected;
                                            p.last_updated_at =
                                                Some("Updated just now".to_string());
                                            p.error_message = None;
                                        }
                                        cx.notify();
                                    });
                                })
                                .ok();
                        }
                        Err(err) => {
                            warn!(target: "providers", "provider {:?} refresh failed: {err}", kind);
                            async_cx
                                .update(|cx| {
                                    entity.update(cx, |view, cx| {
                                        let mut s = view.state.borrow_mut();
                                        if let Some(p) =
                                            s.providers.iter_mut().find(|p| p.kind == kind)
                                        {
                                            if p.quotas.is_empty() {
                                                p.connection = ConnectionStatus::Error;
                                            }
                                            p.last_updated_at = Some("Update failed".to_string());
                                            p.error_message = Some(err.to_string());
                                        }
                                        cx.notify();
                                    });
                                })
                                .ok();
                        }
                    }
                }
            }
        })
        .detach();
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let state = self.state.borrow();
        let active_tab = state.active_tab;
        drop(state);

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_panel)
            .text_color(theme.text_primary)
            .child(self.render_top_nav(active_tab, cx))
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(match active_tab {
                        NavTab::Provider(kind) => div()
                            .px(px(12.0))
                            .py(px(10.0))
                            .child(self.render_provider_detail(kind, cx))
                            .into_any_element(),
                        NavTab::Settings => self.render_settings_content(cx),
                    }),
            )
    }
}

// ============================================================================
// 渲染方法
// ============================================================================

impl AppView {
    fn render_settings_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.state.borrow().settings.clone();
        let theme = cx.global::<Theme>();
        let state = self.state.clone();
        let entity = cx.entity().clone();
        let auto_hide_state = state.clone();
        let auto_hide_entity = entity.clone();

        div()
            .px(px(12.0))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .rounded(px(14.0))
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .px(px(14.0))
                    .py(px(12.0))
                    .cursor_pointer()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(self.render_footer_glyph(AUTO_HIDE_ICON, theme))
                            .child(
                                div()
                                    .flex_col()
                                    .gap(px(3.0))
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_primary)
                                            .child("Auto-hide window"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme.text_secondary)
                                            .child(
                                                "Close the tray popover when focus leaves the app.",
                                            ),
                                    ),
                            ),
                    )
                    .child(self.render_toggle_switch(settings.auto_hide_window, theme))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        let settings = {
                            let mut app_state = auto_hide_state.borrow_mut();
                            app_state.settings.auto_hide_window =
                                !app_state.settings.auto_hide_window;
                            app_state.settings.clone()
                        };
                        persist_settings(&settings);
                        auto_hide_entity.update(cx, |_, cx| {
                            cx.notify();
                        });
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .rounded(px(14.0))
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .px(px(14.0))
                    .py(px(12.0))
                    .child(
                        div()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child("Visible providers"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_secondary)
                                    .child(
                                    "Show only the providers you care about in the tray header.",
                                ),
                            ),
                    )
                    .child(div().flex().gap(px(6.0)).children((3..=5).map(|count| {
                        let state = state.clone();
                        let entity = entity.clone();
                        let is_active = settings.visible_provider_count == count;
                        div()
                            .min_w(px(28.0))
                            .px(px(8.0))
                            .py(px(5.0))
                            .rounded_full()
                            .bg(if is_active {
                                theme.element_selected
                            } else {
                                theme.bg_subtle
                            })
                            .border_1()
                            .border_color(theme.border_subtle)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if is_active {
                                theme.element_active
                            } else {
                                theme.text_primary
                            })
                            .cursor_pointer()
                            .child(count.to_string())
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                let settings = {
                                    let mut app_state = state.borrow_mut();
                                    app_state.settings.visible_provider_count = count;
                                    app_state.settings.clone()
                                };
                                persist_settings(&settings);
                                entity.update(cx, |_, cx| {
                                    cx.notify();
                                });
                            })
                    }))),
            )
            .child(SettingsPanel::new(settings))
            .child(
                div()
                    .rounded(px(14.0))
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .p(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child("Quit BananaTray"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_secondary)
                                    .child("Stop tray monitoring and close the app completely."),
                            ),
                    )
                    .child(
                        div()
                            .px(px(12.0))
                            .py(px(8.0))
                            .rounded(px(12.0))
                            .bg(theme.status_error)
                            .text_color(theme.element_active)
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .child("Quit")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.quit();
                            }),
                    ),
            )
            .into_any_element()
    }

    fn render_top_nav(&self, active_tab: NavTab, cx: &mut Context<Self>) -> impl IntoElement {
        let settings_action = self.render_settings_trigger(cx);
        let theme = cx.global::<Theme>();
        let visible_provider_count = self
            .state
            .borrow()
            .settings
            .visible_provider_count
            .clamp(3, 5);
        let provider_order = [
            ProviderKind::Claude,
            ProviderKind::Gemini,
            ProviderKind::Copilot,
            ProviderKind::Amp,
            ProviderKind::Kimi,
            ProviderKind::Codex,
        ];
        let nav_items: Vec<_> = provider_order
            .into_iter()
            .take(visible_provider_count)
            .map(|kind| {
                (
                    kind.icon_asset(),
                    kind.display_name(),
                    NavTab::Provider(kind),
                )
            })
            .collect();

        div()
            .flex_col()
            .w_full()
            .border_b_1()
            .border_color(theme.border_subtle)
            .px(px(10.0))
            .pt(px(8.0))
            .pb(px(6.0))
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text_primary)
                            .child("BananaTray"),
                    )
                    .child(settings_action),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(1.0))
                    .rounded(px(8.0))
                    .bg(theme.bg_subtle)
                    .p(px(2.0))
                    .children(nav_items.into_iter().map(|(icon, label, tab)| {
                        self.render_nav_item(icon, label, tab, active_tab, cx)
                    })),
            )
    }
    fn render_nav_item(
        &self,
        icon_path: &'static str,
        label: &'static str,
        tab: NavTab,
        active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = tab == active_tab;
        let theme = cx.global::<Theme>();
        let state = self.state.clone();
        let entity = cx.entity().clone();

        let item = div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(5.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .bg(if is_active {
                theme.bg_card
            } else {
                transparent_black()
            })
            .child(
                svg()
                    .path(icon_path)
                    .size(px(13.0))
                    .text_color(if is_active {
                        theme.text_accent
                    } else {
                        theme.text_muted
                    }),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(if is_active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if is_active {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    })
                    .child(label),
            );

        item.on_mouse_down(MouseButton::Left, move |_, _, cx| {
            let mut app_state = state.borrow_mut();
            app_state.active_tab = tab;
            if let NavTab::Provider(kind) = tab {
                app_state.last_provider_kind = kind;
            }
            entity.update(cx, |_, cx| {
                cx.notify();
            });
        })
    }

    fn render_settings_trigger(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let state = self.state.clone();

        div()
            .w(px(28.0))
            .h(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .cursor_pointer()
            .bg(theme.bg_subtle)
            .border_1()
            .border_color(theme.border_subtle)
            .child(self.render_svg_icon(SETTINGS_ICON, px(13.0), theme.text_secondary))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                info!(target: "settings", "settings trigger clicked from tray header");
                window.remove_window();
                let settings_state = state.clone();
                schedule_open_settings_window(settings_state, cx);
            })
    }

    fn render_provider_detail(&self, kind: ProviderKind, cx: &mut Context<Self>) -> AnyElement {
        let state = self.state.borrow();
        let provider = state.providers.iter().find(|p| p.kind == kind).cloned();
        drop(state);

        if let Some(provider) = provider {
            self.render_provider_panel(&provider, true, true, cx)
        } else {
            div().child("Provider not found").into_any_element()
        }
    }

    fn render_provider_panel(
        &self,
        provider: &ProviderStatus,
        highlighted: bool,
        show_actions: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.global::<Theme>();
        let has_quotas = !provider.quotas.is_empty();
        let card_bg = if highlighted {
            theme.bg_card_active
        } else {
            theme.bg_card
        };
        let card_border = theme.border_subtle;
        let status_tint = if provider.connection != ConnectionStatus::Connected {
            theme.status_error
        } else {
            match provider.worst_status() {
                StatusLevel::Green => theme.status_success,
                StatusLevel::Yellow => theme.status_warning,
                StatusLevel::Red => theme.status_error,
            }
        };
        let title_color = if highlighted {
            theme.element_active
        } else {
            theme.text_primary
        };
        let sub_color = theme.text_secondary;
        let status_text = self.provider_status_label(provider);
        let account_text = self.provider_account_label(provider, highlighted);
        let last_updated =
            provider
                .last_updated_at
                .clone()
                .unwrap_or_else(|| match provider.connection {
                    ConnectionStatus::Connected => "Updated recently".to_string(),
                    ConnectionStatus::Error => "Needs attention".to_string(),
                    ConnectionStatus::Disconnected => "Not connected".to_string(),
                });

        let shell = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(12.0))
            .rounded(px(14.0))
            .bg(card_bg)
            .border_1()
            .border_color(card_border)
            // Row 1: Provider name (left) + account email (right)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(title_color)
                            .child(provider.kind.display_name()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(sub_color)
                            .child(account_text),
                    ),
            )
            // Row 2: Updated time (left) + status badge (right)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child(last_updated),
                    )
                    .child(self.render_provider_badge(
                        status_text,
                        highlighted,
                        status_tint,
                        theme,
                    )),
            );

        let shell =
            if has_quotas {
                shell.children(provider.quotas.iter().enumerate().map(|(index, quota)| {
                    self.render_quota_bar(quota, highlighted, index > 0, theme)
                }))
            } else {
                shell.child(self.render_provider_empty_state(provider, highlighted, theme))
            };

        let shell = if show_actions {
            shell.child(
                div()
                    .flex_col()
                    .gap(px(2.0))
                    .border_t_1()
                    .border_color(theme.border_subtle)
                    .pt(px(8.0))
                    .mt(px(2.0))
                    .child(self.render_menu_item(SWITCH_ICON, "Switch Account...", highlighted, cx))
                    .child(self.render_menu_item(USAGE_ICON, "Usage Dashboard", highlighted, cx))
                    .child(self.render_menu_item(STATUS_ICON, "Status Page", highlighted, cx)),
            )
        } else {
            shell
        };

        shell.into_any_element()
    }

    fn render_provider_empty_state(
        &self,
        provider: &ProviderStatus,
        highlighted: bool,
        theme: &Theme,
    ) -> impl IntoElement {
        let title = match provider.connection {
            ConnectionStatus::Connected => "Waiting for usage data",
            ConnectionStatus::Disconnected => "Connection required",
            ConnectionStatus::Error => "Refresh failed",
        };
        let message = self.provider_empty_message(provider);

        div()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(14.0))
            .bg(theme.bg_subtle)
            .px(px(12.0))
            .py(px(10.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if highlighted {
                        theme.element_active
                    } else {
                        theme.text_primary
                    })
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(relative(1.4))
                    .text_color(if highlighted {
                        theme.element_active
                    } else {
                        theme.text_secondary
                    })
                    .child(message),
            )
    }

    fn render_quota_bar(
        &self,
        q: &crate::models::QuotaInfo,
        highlighted: bool,
        show_divider: bool,
        theme: &Theme,
    ) -> impl IntoElement {
        let pct = q.percentage();
        let remaining_pct = (100.0 - pct).max(0.0);
        let bar_fill = match q.status_level() {
            StatusLevel::Green => theme.status_success,
            StatusLevel::Yellow => theme.status_warning,
            StatusLevel::Red => theme.status_error,
        };
        let title_color = if highlighted {
            theme.element_active
        } else {
            theme.text_primary
        };
        let secondary_color = if highlighted {
            theme.text_secondary
        } else {
            theme.text_muted
        };

        let row = div()
            .flex_col()
            .gap(px(3.0))
            // Quota label
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(title_color)
                    .child(q.label.clone()),
            )
            // Progress bar
            .child(
                div()
                    .w_full()
                    .h(px(8.0))
                    .bg(theme.progress_track)
                    .rounded_full()
                    .child(
                        div()
                            .w(relative(pct as f32 / 100.0))
                            .h_full()
                            .bg(if highlighted {
                                theme.element_active
                            } else {
                                bar_fill
                            })
                            .rounded_full(),
                    ),
            )
            // Bottom info row: "XX% left" on left, reset info on right
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(secondary_color)
                            .child(format!("{:.0}% left", remaining_pct)),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(secondary_color)
                            .child(self.format_quota_usage(q)),
                    ),
            );

        if show_divider {
            row.mt(px(6.0))
        } else {
            row
        }
    }

    fn render_provider_badge(
        &self,
        label: &str,
        highlighted: bool,
        tint: Hsla,
        theme: &Theme,
    ) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(4.0))
            .bg(theme.bg_subtle)
            .text_size(px(11.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(if highlighted {
                theme.text_secondary
            } else {
                tint
            })
            .child(label.to_string())
    }

    fn render_menu_item(
        &self,
        icon_path: &'static str,
        label: &'static str,
        highlighted: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();

        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(4.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .text_size(px(13.0))
            .text_color(if highlighted {
                theme.element_active
            } else {
                theme.text_primary
            })
            .child(self.render_svg_icon(
                icon_path,
                px(13.0),
                if highlighted {
                    theme.text_secondary
                } else {
                    theme.text_muted
                },
            ))
            .child(label)
    }

    fn provider_status_label(&self, provider: &ProviderStatus) -> &'static str {
        match provider.connection {
            ConnectionStatus::Connected => "Ready",
            ConnectionStatus::Disconnected => "Setup needed",
            ConnectionStatus::Error => "Needs attention",
        }
    }

    fn provider_empty_message(&self, provider: &ProviderStatus) -> String {
        if let Some(error) = &provider.error_message {
            if error.contains("Missing environment variable") {
                return format!(
                    "Connect {} credentials before quota tracking can start.",
                    provider.kind.display_name()
                );
            }

            if error.contains("session cookie expired") {
                return "Session expired. Sign in again to refresh usage.".to_string();
            }

            return error.clone();
        }

        match provider.connection {
            ConnectionStatus::Error => {
                format!(
                    "{} usage could not be refreshed right now.",
                    provider.kind.display_name()
                )
            }
            ConnectionStatus::Disconnected => {
                format!(
                    "Connect {} to start tracking quota.",
                    provider.kind.display_name()
                )
            }
            ConnectionStatus::Connected => "No usage details available yet.".to_string(),
        }
    }

    fn provider_account_label(&self, provider: &ProviderStatus, compact: bool) -> String {
        if let Some(email) = &provider.account_email {
            return email.clone();
        }

        if compact {
            match provider.kind {
                ProviderKind::Claude => "Anthropic".to_string(),
                ProviderKind::Gemini => "Google".to_string(),
                ProviderKind::Copilot => "GitHub".to_string(),
                ProviderKind::Codex => "OpenAI".to_string(),
                ProviderKind::Kimi => "Moonshot".to_string(),
                ProviderKind::Amp => "Amp CLI".to_string(),
            }
        } else {
            provider.kind.account_hint().to_string()
        }
    }

    fn format_quota_usage(&self, quota: &crate::models::QuotaInfo) -> String {
        format!(
            "{} / {} used",
            self.format_amount(quota.used),
            self.format_amount(quota.limit)
        )
    }

    fn format_amount(&self, value: f64) -> String {
        if (value.fract() - 0.0).abs() < f64::EPSILON {
            format!("{:.0}", value)
        } else {
            format!("{:.1}", value)
        }
    }

    // render_action_link is replaced by render_menu_item above

    fn render_footer_glyph(&self, icon_path: &'static str, theme: &Theme) -> impl IntoElement {
        div()
            .w(px(18.0))
            .h(px(18.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .border_1()
            .border_color(theme.text_accent_soft)
            .bg(theme.bg_subtle)
            .child(self.render_svg_icon(icon_path, px(11.0), theme.text_accent))
    }

    fn render_svg_icon(&self, path: &'static str, size: Pixels, color: Hsla) -> impl IntoElement {
        svg().path(path).size(size).text_color(color)
    }

    fn render_toggle_switch(&self, enabled: bool, theme: &Theme) -> impl IntoElement {
        div()
            .w(px(36.0))
            .h(px(20.0))
            .flex()
            .items_center()
            .rounded_full()
            .px(px(2.0))
            .bg(if enabled {
                theme.element_selected
            } else {
                theme.bg_subtle
            })
            .border_1()
            .border_color(if enabled {
                theme.text_accent_soft
            } else {
                theme.border_strong
            })
            .child(
                div()
                    .w(px(14.0))
                    .h(px(14.0))
                    .rounded_full()
                    .bg(theme.element_active)
                    .ml(if enabled { px(16.0) } else { px(0.0) }),
            )
    }
}
