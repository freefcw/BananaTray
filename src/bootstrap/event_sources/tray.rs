use crate::tray::TrayController;
use gpui::{App, TrayIconClickEvent};
use log::info;
#[cfg(target_os = "linux")]
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayCommand {
    TogglePopup,
    ShowSettings,
    #[cfg(target_os = "linux")]
    Quit,
}

/// 注册托盘图标事件（左键/右键）和 Linux 菜单。
pub(crate) fn register_tray_events(controller: &Rc<RefCell<TrayController>>, cx: &mut App) {
    #[cfg(target_os = "linux")]
    if !crate::tray::should_use_gpui_tray() {
        info!(target: "tray", "GNOME extension mode detected, skipping GPUI tray event setup");
        return;
    }

    let ctrl = controller.clone();
    cx.on_tray_icon_click_event(move |event, cx| {
        info!(
            target: "tray",
            "received tray click event: {:?} position={:?}",
            event.kind,
            event.position
        );
        // 将点击坐标传递给 controller，用于 Linux 上构造 TrayAnchor。
        ctrl.borrow().set_click_position(event.position);
        if let Some(command) = command_for_tray_icon_event(&event) {
            run_tray_command(command, &ctrl, cx);
        }
    });

    // Linux: 注册右键菜单和菜单动作回调。GNOME Shell Extension 模式下跳过菜单安装。
    #[cfg(target_os = "linux")]
    {
        install_linux_tray_menu(cx);
        let ctrl = controller.clone();
        cx.on_tray_menu_action(move |id, cx| {
            info!(target: "tray", "received tray menu action: {}", id);
            if let Some(command) = command_for_tray_menu_action(&id) {
                run_tray_command(command, &ctrl, cx);
            }
        });
    }
}

fn command_for_tray_icon_event(event: &TrayIconClickEvent) -> Option<TrayCommand> {
    use gpui::TrayIconEvent;
    match &event.kind {
        TrayIconEvent::LeftClick => Some(TrayCommand::TogglePopup),
        TrayIconEvent::RightClick => Some(TrayCommand::ShowSettings),
        _ => None,
    }
}

fn run_tray_command(command: TrayCommand, controller: &Rc<RefCell<TrayController>>, cx: &mut App) {
    match command {
        TrayCommand::TogglePopup => controller.borrow_mut().toggle_popup(cx),
        TrayCommand::ShowSettings => controller.borrow_mut().show_settings(cx),
        #[cfg(target_os = "linux")]
        TrayCommand::Quit => cx.quit(),
    }
}

#[cfg(target_os = "linux")]
const TRAY_ACTION_OPEN: &str = "tray.open";
#[cfg(target_os = "linux")]
const TRAY_ACTION_SETTINGS: &str = "tray.settings";
#[cfg(target_os = "linux")]
const TRAY_ACTION_QUIT: &str = "tray.quit";

#[cfg(target_os = "linux")]
fn install_linux_tray_menu(cx: &mut App) {
    use gpui::TrayMenuItem;

    cx.set_tray_menu(vec![
        TrayMenuItem::Action {
            label: t!("tray.menu.open").to_string().into(),
            id: TRAY_ACTION_OPEN.into(),
        },
        TrayMenuItem::Action {
            label: t!("tray.menu.settings").to_string().into(),
            id: TRAY_ACTION_SETTINGS.into(),
        },
        TrayMenuItem::Separator,
        TrayMenuItem::Action {
            label: t!("tray.menu.quit").to_string().into(),
            id: TRAY_ACTION_QUIT.into(),
        },
    ]);
}

#[cfg(target_os = "linux")]
fn command_for_tray_menu_action(id: &str) -> Option<TrayCommand> {
    match id {
        TRAY_ACTION_OPEN => Some(TrayCommand::TogglePopup),
        TRAY_ACTION_SETTINGS => Some(TrayCommand::ShowSettings),
        TRAY_ACTION_QUIT => Some(TrayCommand::Quit),
        _ => None,
    }
}
