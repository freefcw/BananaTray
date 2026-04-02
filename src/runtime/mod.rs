use crate::app::{persist_settings, schedule_open_settings_window, AppState};
use crate::application::{reduce, AppAction, AppEffect, DebugNotificationKind};
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
    let effects = reduce_action(state, action);
    for effect in effects {
        run_effect_in_context(state, effect, cx);
    }
}

pub fn dispatch_in_window(
    state: &Rc<RefCell<AppState>>,
    action: AppAction,
    window: &mut Window,
    cx: &mut App,
) {
    let effects = reduce_action(state, action);
    for effect in effects {
        run_effect_in_window(state, effect, window, cx);
    }
}

pub fn dispatch_in_app(state: &Rc<RefCell<AppState>>, action: AppAction, cx: &mut App) {
    let effects = reduce_action(state, action);
    for effect in effects {
        run_effect_in_app(state, effect, cx);
    }
}

fn reduce_action(state: &Rc<RefCell<AppState>>, action: AppAction) -> Vec<AppEffect> {
    let mut state_ref = state.borrow_mut();
    reduce(&mut state_ref.session, action)
}

fn run_effect_in_context<V: 'static>(
    state: &Rc<RefCell<AppState>>,
    effect: AppEffect,
    cx: &mut Context<V>,
) {
    match effect {
        AppEffect::Render => cx.notify(),
        AppEffect::PersistSettings => persist_current_settings(state),
        AppEffect::SendRefreshRequest(request) => {
            let _ = send_refresh_request(state, request);
        }
        AppEffect::ApplyLocale(language) => crate::i18n::apply_locale(&language),
        AppEffect::UpdateLogLevel(level) => update_log_level(&level),
        AppEffect::SendQuotaNotification { alert, with_sound } => {
            send_system_notification(&alert, with_sound);
        }
        AppEffect::SendDebugNotification { kind, with_sound } => {
            send_system_notification(&build_debug_alert(kind), with_sound);
        }
        AppEffect::SyncAutoLaunch(enabled) => sync_auto_launch(enabled),
        AppEffect::OpenSettingsWindow => {
            warn!(target: "runtime", "OpenSettingsWindow effect ignored: not available in Context<V>");
        }
        AppEffect::OpenUrl(url) => {
            warn!(target: "runtime", "OpenUrl({}) effect ignored: not available in Context<V>", url);
        }
        AppEffect::QuitApp => {
            warn!(target: "runtime", "QuitApp effect ignored: not available in Context<V>");
        }
    }
}

fn run_effect_in_window(
    state: &Rc<RefCell<AppState>>,
    effect: AppEffect,
    window: &mut Window,
    cx: &mut App,
) {
    match effect {
        AppEffect::Render => window.refresh(),
        AppEffect::PersistSettings => persist_current_settings(state),
        AppEffect::SendRefreshRequest(request) => {
            let _ = send_refresh_request(state, request);
        }
        AppEffect::OpenSettingsWindow => {
            let display_id = window.display(cx).map(|display| display.id());
            state.borrow_mut().view_entity = None;
            window.remove_window();
            schedule_open_settings_window(state.clone(), display_id, cx);
        }
        AppEffect::OpenUrl(url) => crate::utils::platform::open_url(&url),
        AppEffect::SyncAutoLaunch(enabled) => sync_auto_launch(enabled),
        AppEffect::ApplyLocale(language) => crate::i18n::apply_locale(&language),
        AppEffect::UpdateLogLevel(level) => update_log_level(&level),
        AppEffect::SendQuotaNotification { alert, with_sound } => {
            send_system_notification(&alert, with_sound);
        }
        AppEffect::SendDebugNotification { kind, with_sound } => {
            send_system_notification(&build_debug_alert(kind), with_sound);
        }
        AppEffect::QuitApp => cx.quit(),
    }
}

fn run_effect_in_app(state: &Rc<RefCell<AppState>>, effect: AppEffect, cx: &mut App) {
    match effect {
        AppEffect::Render => notify_view_entity(state, cx),
        AppEffect::PersistSettings => persist_current_settings(state),
        AppEffect::SendRefreshRequest(request) => {
            let _ = send_refresh_request(state, request);
        }
        AppEffect::OpenSettingsWindow => schedule_open_settings_window(state.clone(), None, cx),
        AppEffect::OpenUrl(url) => crate::utils::platform::open_url(&url),
        AppEffect::SyncAutoLaunch(enabled) => sync_auto_launch(enabled),
        AppEffect::ApplyLocale(language) => crate::i18n::apply_locale(&language),
        AppEffect::UpdateLogLevel(level) => update_log_level(&level),
        AppEffect::SendQuotaNotification { alert, with_sound } => {
            send_system_notification(&alert, with_sound);
        }
        AppEffect::SendDebugNotification { kind, with_sound } => {
            send_system_notification(&build_debug_alert(kind), with_sound);
        }
        AppEffect::QuitApp => cx.quit(),
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

        let mut notification = notify_rust::Notification::new();
        notification
            .appname("BananaTray")
            .summary(&title)
            .body(&body);

        if let Err(err) = notification.show() {
            warn!(
                target: "settings",
                "failed to show auto-launch notification: {}",
                err
            );
        }
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
