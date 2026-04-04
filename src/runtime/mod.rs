use crate::app::{persist_settings, schedule_open_settings_window, AppState};
use crate::application::effect::{route_effect, CommonEffect, RoutedEffect};
use crate::application::{reduce, AppAction, DebugNotificationKind};
use crate::models::ConnectionStatus;
use crate::notification::{send_system_notification, QuotaAlert};
use crate::refresh::RefreshRequest;
use gpui::*;
use log::{info, warn};
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

pub fn dispatch_in_context<V: 'static>(
    state: &Rc<RefCell<AppState>>,
    action: AppAction,
    cx: &mut Context<V>,
) {
    dispatch_effects(state, action, |effect| {
        run_effect_in_context(state, effect, cx)
    });
}

pub fn dispatch_in_window(
    state: &Rc<RefCell<AppState>>,
    action: AppAction,
    window: &mut Window,
    cx: &mut App,
) {
    dispatch_effects(state, action, |effect| {
        run_effect_in_window(state, effect, window, cx);
    });
}

pub fn dispatch_in_app(state: &Rc<RefCell<AppState>>, action: AppAction, cx: &mut App) {
    dispatch_effects(state, action, |effect| run_effect_in_app(state, effect, cx));
}

fn reduce_action(state: &Rc<RefCell<AppState>>, action: AppAction) -> Vec<RoutedEffect> {
    let mut state_ref = state.borrow_mut();
    reduce(&mut state_ref.session, action)
        .into_iter()
        .map(route_effect)
        .collect()
}

fn dispatch_effects(
    state: &Rc<RefCell<AppState>>,
    action: AppAction,
    mut run_effect: impl FnMut(RoutedEffect),
) {
    for effect in reduce_action(state, action) {
        run_effect(effect);
    }
}

fn run_effect_in_context<V: 'static>(
    state: &Rc<RefCell<AppState>>,
    effect: RoutedEffect,
    cx: &mut Context<V>,
) {
    match effect {
        RoutedEffect::Common(effect) => run_common_effect(state, effect),
        RoutedEffect::Render => cx.notify(),
        RoutedEffect::OpenSettingsWindow => {
            warn!(target: "runtime", "OpenSettingsWindow effect ignored: not available in Context<V>");
        }
        RoutedEffect::OpenUrl(url) => {
            warn!(target: "runtime", "OpenUrl({}) effect ignored: not available in Context<V>", url);
        }
        RoutedEffect::ApplyTrayIcon(_) => {
            warn!(target: "runtime", "ApplyTrayIcon effect ignored: not available in Context<V>");
        }
        RoutedEffect::QuitApp => {
            warn!(target: "runtime", "QuitApp effect ignored: not available in Context<V>");
        }
    }
}

fn run_effect_in_window(
    state: &Rc<RefCell<AppState>>,
    effect: RoutedEffect,
    window: &mut Window,
    cx: &mut App,
) {
    match effect {
        RoutedEffect::Common(effect) => run_common_effect(state, effect),
        RoutedEffect::Render => window.refresh(),
        RoutedEffect::OpenSettingsWindow => {
            let display_id = window.display(cx).map(|display| display.id());
            state.borrow_mut().view_entity = None;
            window.remove_window();
            schedule_open_settings_window(state.clone(), display_id, cx);
        }
        _ => {
            if let Err(err_effect) = run_app_logic_effect(effect, cx) {
                warn!(target: "runtime", "Effect {:?} ignored in Window context", err_effect);
            }
        }
    }
}

fn run_effect_in_app(state: &Rc<RefCell<AppState>>, effect: RoutedEffect, cx: &mut App) {
    match effect {
        RoutedEffect::Common(effect) => run_common_effect(state, effect),
        RoutedEffect::Render => notify_view_entity(state, cx),
        RoutedEffect::OpenSettingsWindow => schedule_open_settings_window(state.clone(), None, cx),
        _ => {
            if let Err(err_effect) = run_app_logic_effect(effect, cx) {
                warn!(target: "runtime", "Effect {:?} ignored in App context", err_effect);
            }
        }
    }
}

fn run_app_logic_effect(effect: RoutedEffect, cx: &mut App) -> Result<(), RoutedEffect> {
    match effect {
        RoutedEffect::OpenUrl(url) => {
            crate::utils::platform::open_url(&url);
            Ok(())
        }
        RoutedEffect::ApplyTrayIcon(style) => {
            crate::tray_icon_helper::apply_tray_icon(cx, style);
            Ok(())
        }
        RoutedEffect::QuitApp => {
            cx.quit();
            Ok(())
        }
        _ => Err(effect),
    }
}

fn run_common_effect(state: &Rc<RefCell<AppState>>, effect: CommonEffect) {
    match effect {
        CommonEffect::PersistSettings => {
            persist_current_settings(state);
        }
        CommonEffect::SendRefreshRequest(request) => {
            let _ = send_refresh_request(state, request);
        }
        CommonEffect::SyncAutoLaunch(enabled) => {
            sync_auto_launch(enabled);
        }
        CommonEffect::ApplyLocale(language) => {
            crate::i18n::apply_locale(&language);
        }
        CommonEffect::UpdateLogLevel(level) => {
            update_log_level(&level);
        }
        CommonEffect::SendQuotaNotification { alert, with_sound } => {
            send_system_notification(&alert, with_sound);
        }
        CommonEffect::SendDebugNotification { kind, with_sound } => {
            send_system_notification(&build_debug_alert(kind), with_sound);
        }
        CommonEffect::OpenLogDirectory => {
            let log_path = state.borrow().log_path.clone();
            if let Some(path) = log_path {
                crate::utils::platform::open_path_in_finder(&path);
            } else {
                warn!(target: "runtime", "OpenLogDirectory: log_path not available");
            }
        }
        CommonEffect::CopyToClipboard(text) => {
            crate::utils::platform::copy_to_clipboard(&text);
        }
        CommonEffect::StartDebugRefresh(kind) => {
            use crate::utils::log_capture::LogCapture;
            info!(target: "runtime", "starting debug refresh for {:?}", kind);
            // 1. 保存当前日志级别到 state（供 RestoreLogLevel 使用）
            state.borrow_mut().session.settings_ui.debug_prev_log_level = Some(log::max_level());
            // 2. 清空并启用日志捕获
            LogCapture::global().clear();
            LogCapture::global().enable();
            // 3. 临时提升日志级别到 Debug
            log::set_max_level(log::LevelFilter::Debug);
            // 4. 发送手动刷新请求（跳过 cooldown）
            let request = crate::refresh::RefreshRequest::RefreshOne {
                kind,
                reason: crate::refresh::RefreshReason::Manual,
            };
            let _ = send_refresh_request(state, request);
        }
        CommonEffect::RestoreLogLevel(level) => {
            use crate::utils::log_capture::LogCapture;
            info!(target: "runtime", "debug refresh complete, restoring log level to {:?}", level);
            // 停用日志捕获，恢复日志级别
            LogCapture::global().disable();
            log::set_max_level(level);
        }
        CommonEffect::ClearDebugLogs => {
            crate::utils::log_capture::LogCapture::global().clear();
        }
    }
}

fn persist_current_settings(state: &Rc<RefCell<AppState>>) {
    let settings = state.borrow().session.settings.clone();
    persist_settings(&settings);
}

fn notify_view_entity(state: &Rc<RefCell<AppState>>, cx: &mut App) {
    let view_entity = state.borrow().view_entity.clone();
    if let Some(entity) = view_entity {
        let _ = entity.update(cx, |_, cx| {
            cx.notify();
        });
    }
}

fn send_refresh_request(state: &Rc<RefCell<AppState>>, request: RefreshRequest) -> bool {
    let failed_kind = match &request {
        RefreshRequest::RefreshOne { kind, .. } => Some(*kind),
        _ => None,
    };
    let send_result = state.borrow().send_refresh(request);
    if let Err(err) = send_result {
        warn!(target: "refresh", "failed to send refresh request: {}", err);
        if let Some(kind) = failed_kind {
            if let Some(provider) = state.borrow_mut().session.provider_store.find_mut(kind) {
                provider.connection = ConnectionStatus::Disconnected;
            }
        }
        false
    } else {
        true
    }
}

fn sync_auto_launch(enabled: bool) {
    std::thread::spawn(move || {
        crate::auto_launch::sync(enabled);

        let (title, body) = if enabled {
            (
                t!("notification.auto_launch.enabled.title").to_string(),
                t!("notification.auto_launch.enabled.body").to_string(),
            )
        } else {
            (
                t!("notification.auto_launch.disabled.title").to_string(),
                t!("notification.auto_launch.disabled.body").to_string(),
            )
        };

        // 使用 osascript 发送通知，绕过 mac-notification-sys 的 "use_default" bug
        crate::notification::send_plain_notification(&title, &body);
    });
}

fn update_log_level(level: &str) {
    std::env::set_var("RUST_LOG", level);
    if let Some(filter) = parse_log_level(level) {
        log::set_max_level(filter);
        info!(target: "settings", "log level changed to: {}", level);
    }
}

fn parse_log_level(value: &str) -> Option<log::LevelFilter> {
    match value.to_lowercase().as_str() {
        "error" => Some(log::LevelFilter::Error),
        "warn" => Some(log::LevelFilter::Warn),
        "info" => Some(log::LevelFilter::Info),
        "debug" => Some(log::LevelFilter::Debug),
        "trace" => Some(log::LevelFilter::Trace),
        _ => None,
    }
}

fn build_debug_alert(kind: DebugNotificationKind) -> QuotaAlert {
    match kind {
        DebugNotificationKind::Low => QuotaAlert::LowQuota {
            provider_name: "TestProvider".to_string(),
            remaining_pct: 8.0,
        },
        DebugNotificationKind::Exhausted => QuotaAlert::Exhausted {
            provider_name: "TestProvider".to_string(),
        },
        DebugNotificationKind::Recovered => QuotaAlert::Recovered {
            provider_name: "TestProvider".to_string(),
            remaining_pct: 50.0,
        },
    }
}
