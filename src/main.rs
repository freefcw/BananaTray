mod app;
mod logging;
mod models;
mod providers;
mod settings_store;
mod theme;

use app::{schedule_open_settings_window, AppState};
use gpui::*;
use log::{error, info, warn};
use models::NavTab;
use std::borrow::Cow;
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
        let state = Rc::new(RefCell::new(AppState::new()));
        info!(target: "tray", "initialized tray controller");
        Self {
            window: None,
            state,
        }
    }

    fn toggle_provider(&mut self, cx: &mut App) {
        let provider_tab = {
            let state = self.state.borrow();
            NavTab::Provider(state.last_provider_kind)
        };
        info!(target: "tray", "toggle provider panel for {:?}", provider_tab);

        if let Some(window) = self.window.take() {
            let active_tab = self.state.borrow().active_tab;
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
        if let Some(window) = self.window.take() {
            info!(target: "tray", "closing existing tray panel before opening settings window");
            let _ = window.update(cx, |_, window, _| {
                window.remove_window();
            });
        }
        schedule_open_settings_window(self.state.clone(), cx);
    }

    fn show(&mut self, tab: NavTab, cx: &mut App) {
        info!(target: "tray", "show window for tab {:?}", tab);
        {
            let mut state = self.state.borrow_mut();
            state.active_tab = tab;
            if let NavTab::Provider(kind) = tab {
                state.last_provider_kind = kind;
            }
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

    fn open(&mut self, cx: &mut App) {
        let window_size = size(px(308.0), px(548.0));
        let tray_bounds = cx.tray_icon_bounds().unwrap_or_default();
        let bounds =
            cx.compute_window_bounds(window_size, &WindowPosition::TrayCenter(tray_bounds));

        let state = self.state.clone();

        let result = cx.open_window(
            WindowOptions {
                titlebar: None,
                kind: WindowKind::PopUp,
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
            let _ = handle.update(cx, |view, window, cx| {
                let sub = cx.observe_window_activation(window, move |_view, window, _cx| {
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
    Application::new()
        .with_assets(Assets {
            base: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        })
        .run(|cx: &mut App| {
            let logging = logging::init();
            info!(target: "app", "starting BananaTray application");

            // 1. 初始化
            adabraka_ui::init(cx);
            adabraka_ui::theme::install_theme(cx, adabraka_ui::theme::Theme::dark());
            cx.set_keep_alive_without_windows(true);

            // 2. 配置系统托盘
            cx.set_tray_icon(Some(include_bytes!("tray_icon.png")));
            cx.set_tray_tooltip("BananaTray - AI Quota Monitor");
            cx.set_tray_panel_mode(true);

            // 3. 窗口控制器
            let controller = Rc::new(RefCell::new(TrayController::new()));

            // 4. 托盘点击
            let tray_ctrl = controller.clone();
            cx.on_tray_icon_event(move |event, cx| {
                info!(target: "tray", "received tray event: {:?}", event);
                match event {
                    TrayIconEvent::LeftClick => tray_ctrl.borrow_mut().toggle_provider(cx),
                    TrayIconEvent::RightClick => tray_ctrl.borrow_mut().show_settings(cx),
                    _ => {}
                }
            });

            // 5. 全局热键 Cmd+Shift+S
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

            println!("🚀 BananaTray is running! Look for the tray icon in your menu bar.");
            println!("🪵 Logging target: {}", logging.target_description);
            println!("🪵 Use BANANATRAY_LOG_FILE=1 to write logs into ./banana.log");
            info!(target: "app", "logging initialized with env_logger backend -> {}", logging.target_description);
        });
}
