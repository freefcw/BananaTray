mod app;
mod models;
mod theme;
mod views;
mod providers;

use app::AppState;
use gpui::*;
use std::rc::Rc;
use std::cell::RefCell;

/// 窗口管理器：持有全局窗口句柄，纯数据，不含任何锁操作
struct TrayController {
    window: Option<WindowHandle<app::AppView>>,
    state: Rc<RefCell<AppState>>,
}

impl TrayController {
    fn new() -> Self {
        let state = Rc::new(RefCell::new(AppState::new()));
        Self {
            window: None,
            state,
        }
    }

    fn toggle(&mut self, cx: &mut App) {
        if let Some(window) = self.window.take() {
            // 有窗口 → 销毁它
            let _ = window.update(cx, |_, window, _| {
                window.remove_window();
            });
            // 即使 update 失败 (窗口已被系统回收)，window 也已从 self 取走
        } else {
            // 无窗口 → 创建
            self.open(cx);
        }
    }

    fn open(&mut self, cx: &mut App) {
        let window_size = size(px(320.0), px(520.0));
        let tray_bounds = cx.tray_icon_bounds().unwrap_or_default();
        let bounds = cx.compute_window_bounds(
            window_size,
            &WindowPosition::TrayCenter(tray_bounds),
        );

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
            |_window, cx| {
                cx.new(|cx| app::AppView::new(state, cx))
            },
        );

        if let Ok(handle) = result {
            self.window = Some(handle);
        }
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
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
            if matches!(event, TrayIconEvent::LeftClick | TrayIconEvent::RightClick) {
                tray_ctrl.borrow_mut().toggle(cx);
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
                let _ = async_cx.update(|cx| {
                    hotkey_ctrl.borrow_mut().toggle(cx);
                });
            }
        });

        println!("🚀 BananaTray is running! Look for the tray icon in your menu bar.");
    });
}
