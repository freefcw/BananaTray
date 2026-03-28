#![recursion_limit = "512"]

mod app;
mod logging;
pub mod models;
mod providers;
mod settings_store;
mod theme;
mod utils;

use app::{schedule_open_settings_window, AppState};
use gpui::*;
use log::{error, info, warn};
use models::NavTab;
use std::borrow::Cow;
use std::cell::Cell;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

struct Assets {
    base: PathBuf,
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        fs::read(self.base.join(path))
            .map(|data| Some(Cow::Owned(data)))
            .map_err(|err| err.into())
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        fs::read_dir(self.base.join(path))
            .map(|entries| {
                entries
                    .filter_map(|entry| {
                        entry
                            .ok()
                            .and_then(|entry| entry.file_name().into_string().ok())
                            .map(SharedString::from)
                    })
                    .collect()
            })
            .map_err(|err| err.into())
    }
}

/// 窗口管理器：持有全局窗口句柄，纯数据，不含任何锁操作
struct TrayController {
    window: Option<WindowHandle<app::AppView>>,
    state: Rc<RefCell<AppState>>,
}

impl TrayController {
    fn new() -> Self {
        info!(target: "tray", "initializing tray controller");
        let state = Rc::new(RefCell::new(AppState::new()));
        info!(target: "tray", "tray controller initialized");
        Self {
            window: None,
            state,
        }
    }

    fn toggle_provider(&mut self, cx: &mut App) {
        let has_any_enabled = {
            let state = self.state.borrow();
            crate::models::ProviderKind::all()
                .iter()
                .any(|k| state.settings.is_provider_enabled(*k))
        };

        if !has_any_enabled {
            info!(target: "tray", "no providers enabled, opening settings directly");
            self.show_settings(cx);
            return;
        }

        let provider_tab = {
            let mut state = self.state.borrow_mut();
            let last = state.nav.last_provider_kind;
            // 如果上次选中的 provider 已经被禁用了，切到第一个可用的
            let kind = if state.settings.is_provider_enabled(last) {
                last
            } else {
                let fallback = crate::models::ProviderKind::all()
                    .iter()
                    .find(|k| state.settings.is_provider_enabled(**k))
                    .copied()
                    .unwrap_or(last);
                state.nav.last_provider_kind = fallback;
                fallback
            };
            NavTab::Provider(kind)
        };
        info!(target: "tray", "toggle provider panel for {:?}", provider_tab);

        if let Some(window) = self.window.take() {
            let active_tab = self.state.borrow().nav.active_tab;
            if matches!(active_tab, NavTab::Provider(_)) {
                info!(target: "tray", "provider panel already open, closing existing panel");
                let result = window.update(cx, |_, window, _| {
                    window.remove_window();
                });
                if result.is_err() {
                    warn!(target: "tray", "failed to close existing panel cleanly, reopening provider panel");
                    self.show(provider_tab, cx);
                }
            } else {
                info!(target: "tray", "reusing existing window handle for provider panel");
                self.window = Some(window);
                self.show(provider_tab, cx);
            }
        } else {
            info!(target: "tray", "no open panel, opening provider panel");
            self.show(provider_tab, cx);
        }
    }

    fn show_settings(&mut self, cx: &mut App) {
        info!(target: "tray", "requested settings window from tray controller");
        let mut display_id = None;
        if let Some(window) = self.window.take() {
            info!(target: "tray", "closing existing tray panel before opening settings window");
            let _ = window.update(cx, |_, window, cx| {
                display_id = window.display(cx).map(|d| d.id());
                window.remove_window();
            });
        }
        schedule_open_settings_window(self.state.clone(), display_id, cx);
    }

    fn show(&mut self, tab: NavTab, cx: &mut App) {
        info!(target: "tray", "show window for tab {:?}", tab);
        {
            let mut state = self.state.borrow_mut();
            state.nav.switch_to(tab);
        }

        if let Some(window) = self.window.as_ref() {
            info!(target: "tray", "notifying existing tray window to rerender");
            let _ = window.update(cx, |view, window, cx| {
                let _ = view;
                let _ = window;
                cx.notify();
            });
        } else {
            info!(target: "tray", "opening a fresh tray window");
            self.open(cx);
        }
    }

    fn preferred_window_kind() -> WindowKind {
        if cfg!(target_os = "linux") {
            WindowKind::Floating
        } else {
            WindowKind::PopUp
        }
    }

    fn preferred_window_bounds(cx: &App, window_size: Size<Pixels>) -> Bounds<Pixels> {
        let tray_bounds = cx
            .tray_icon_bounds()
            .filter(|b| b.size.width > px(0.0) && b.size.height > px(0.0));

        let position = if let Some(tray_bounds) = tray_bounds {
            WindowPosition::TrayCenter(tray_bounds)
        } else if cfg!(target_os = "linux") {
            WindowPosition::TopRight { margin: px(16.0) }
        } else {
            WindowPosition::Center
        };

        cx.compute_window_bounds(window_size, &position)
    }

    fn open(&mut self, cx: &mut App) {
        let dynamic_height = app::compute_popup_height(&self.state.borrow());
        info!(target: "tray", "opening window with dynamic height: {}px", dynamic_height);
        let window_size = size(px(app::PopupLayout::WIDTH), px(dynamic_height));
        let bounds = Self::preferred_window_bounds(cx, window_size);
        let kind = Self::preferred_window_kind();

        let state = self.state.clone();

        let result = cx.open_window(
            WindowOptions {
                titlebar: None,
                kind,
                focus: true,
                show: true,
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| app::AppView::new(state, cx)),
        );

        if let Ok(handle) = result {
            info!(target: "tray", "tray popup opened successfully");
            // 监听窗口失焦，自动关闭
            let auto_hide_state = self.state.clone();
            let activation_initialized = Rc::new(Cell::new(false));
            let _ = handle.update(cx, |view, window, cx| {
                let activation_initialized = activation_initialized.clone();
                let sub = cx.observe_window_activation(window, move |_view, window, _cx| {
                    if !activation_initialized.replace(true) {
                        return;
                    }
                    let should_auto_hide = auto_hide_state.borrow().settings.auto_hide_window;
                    if should_auto_hide && !window.is_window_active() {
                        info!(target: "tray", "auto-hide closing inactive tray popup");
                        window.remove_window();
                    }
                });
                view._activation_sub = Some(sub);
            });
            self.window = Some(handle);
        } else if let Err(err) = result {
            error!(target: "tray", "failed to open tray popup: {err:?}");
        }
    }
}

fn main() {
    match logging::init() {
        Ok(init) => {
            log::info!(target: "app", "logging initialized at {}", init.log_path.display());
        }
        Err(err) => {
            eprintln!("failed to initialize logging: {err:#}");
        }
    }

    Application::new()
        .with_assets(Assets {
            base: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        })
        .run(|cx: &mut App| {
            // 1. 初始化
            adabraka_ui::init(cx);
            adabraka_ui::theme::install_theme(cx, adabraka_ui::theme::Theme::light());
            cx.set_keep_alive_without_windows(true);

            // 2. 配置系统托盘
            cx.set_tray_icon(Some(include_bytes!("tray_icon.png")));
            cx.set_tray_tooltip("BananaTray - AI Quota Monitor");
            cx.set_tray_panel_mode(true);

            // 3. 窗口控制器
            let controller = Rc::new(RefCell::new(TrayController::new()));

            // 4. 启动时立即刷新所有已启用 Provider
            {
                let state = controller.borrow().state.clone();
                AppState::spawn_startup_refresh(state, cx);
            }

            // 5. 托盘点击
            let tray_ctrl = controller.clone();
            cx.on_tray_icon_event(move |event, cx| {
                info!(target: "tray", "received tray event: {:?}", event);
                match event {
                    TrayIconEvent::LeftClick => tray_ctrl.borrow_mut().toggle_provider(cx),
                    TrayIconEvent::RightClick => tray_ctrl.borrow_mut().show_settings(cx),
                    _ => {}
                }
            });

            // 6. 全局热键 Cmd+Shift+S
            info!(target: "hotkey", "registering global hotkey Cmd+Shift+S");
            if let Ok(keystroke) = Keystroke::parse("cmd-shift-s") {
                let _ = cx.register_global_hotkey(1, &keystroke);
            }
            let async_cx = cx.to_async();
            let hotkey_ctrl = controller.clone();
            cx.on_global_hotkey(move |id| {
                if id == 1 {
                    info!(target: "hotkey", "received global hotkey 1");
                    let _ = async_cx.update(|cx| {
                        hotkey_ctrl.borrow_mut().toggle_provider(cx);
                    });
                }
            });

            info!(target: "app", "BananaTray is running - look for the tray icon");
        });
}
