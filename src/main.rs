mod app;
mod models;
mod theme;
mod views;
mod providers;

use app::AppView;
use gpui::*;

fn main() {
    Application::new().run(|cx: &mut App| {
        // ====================================================================
        // 1. 初始化 adabraka-ui 组件库和主题
        // ====================================================================
        adabraka_ui::init(cx);
        adabraka_ui::theme::install_theme(cx, adabraka_ui::theme::Theme::dark());

        // ====================================================================
        // 2. 启用 daemon 模式：无窗口也保持运行
        // ====================================================================
        cx.set_keep_alive_without_windows(true);

        // ====================================================================
        // 3. 配置系统托盘
        // ====================================================================
        cx.set_tray_icon(Some(include_bytes!("tray_icon.png")));
        cx.set_tray_tooltip("BananaTray - AI Quota Monitor");

        cx.set_tray_menu(vec![
            TrayMenuItem::Action {
                label: "Show Dashboard".into(),
                id: "show_dashboard".into(),
            },
            TrayMenuItem::Separator,
            TrayMenuItem::Action {
                label: "Settings".into(),
                id: "settings".into(),
            },
            TrayMenuItem::Separator,
            TrayMenuItem::Action {
                label: "Quit BananaTray".into(),
                id: "quit".into(),
            },
        ]);

        // ====================================================================
        // 4. 打开主窗口
        // ====================================================================
        let window_options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("BananaTray".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(720.0), px(520.0)),
                cx,
            ))),
            ..Default::default()
        };

        let window = cx
            .open_window(window_options, |_window, cx| cx.new(|cx| AppView::new(cx)))
            .expect("Failed to open main window");

        // ====================================================================
        // 5. 处理托盘菜单事件
        // ====================================================================
        cx.on_tray_menu_action(move |id: SharedString, cx: &mut App| {
            cx.activate(true); // 激活应用从而能够来到前台显示
            
            match id.as_ref() {
                "show_dashboard" => {
                    let _ = window.update(cx, |view, window, cx| {
                        window.activate_window(); 
                        view.model.show_dashboard();
                        cx.notify();
                    });
                }
                "settings" => {
                    let _ = window.update(cx, |view, window, cx| {
                        window.activate_window(); 
                        view.model.show_settings();
                        cx.notify();
                    });
                }
                "quit" => cx.quit(),
                _ => {}
            }
        });

        // ====================================================================
        // 6. 注册全局热键 (Cmd+Shift+S)
        // ====================================================================
        if let Ok(keystroke) = Keystroke::parse("cmd-shift-s") {
            if let Err(e) = cx.register_global_hotkey(1, &keystroke) {
                eprintln!("Warning: Failed to register global hotkey: {}", e);
            }
        }

        // 监听全局热键
        let async_cx = cx.to_async();
        let hotkey_window = window.clone();
        cx.on_global_hotkey(move |id| {
            if id == 1 {
                let _ = async_cx.update(|cx| {
                    cx.activate(true);
                    let _ = hotkey_window.update(cx, |_, window, _| {
                        window.activate_window();
                    });
                });
            }
        });

        println!("🚀 BananaTray is running! Look for the tray icon in your menu bar.");
    });
}
