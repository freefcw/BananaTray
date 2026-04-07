use crate::app::{persist_settings, schedule_open_settings_window, AppState};
use crate::application::{reduce, AppAction, AppEffect, DebugNotificationKind};
use crate::models::ConnectionStatus;
use crate::notification::{send_system_notification, QuotaAlert};
use crate::refresh::RefreshRequest;
use gpui::*;
use log::{info, warn};

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

/// 将 action 通过 reducer 转换为 effects 并逐个执行。
///
/// **RefCell 安全约束**：`run_effect` 回调中**不得**再次调用 `dispatch_*` 系列函数，
/// 否则会导致 `borrow_mut` 重入 panic。当前所有 effect handler 遵守此约束：
/// 需要异步分派的场景（如 OpenSettingsWindow）使用 `schedule_*` 延迟到下一轮事件循环。
///
/// 此函数内置重入护卫（dispatch guard），在重入时会立即 panic 并给出清晰的错误信息，
/// 而不是等到 RefCell 报出难以定位的 "already borrowed"。
fn dispatch_effects(
    state: &Rc<RefCell<AppState>>,
    action: AppAction,
    mut run_effect: impl FnMut(AppEffect),
) {
    use std::cell::Cell;

    thread_local! {
        static DISPATCHING: Cell<bool> = const { Cell::new(false) };
    }

    // RAII 护卫：即使 effect handler panic 也能正确重置标志位
    struct DispatchGuard;
    impl Drop for DispatchGuard {
        fn drop(&mut self) {
            DISPATCHING.with(|flag| flag.set(false));
        }
    }

    DISPATCHING.with(|flag| {
        assert!(
            !flag.get(),
            "BUG: reentrant dispatch detected! \
             Effect handlers must not call dispatch_* directly. \
             Use schedule_* for deferred dispatch."
        );
        flag.set(true);
    });
    let _guard = DispatchGuard;

    let effects = {
        let mut state_ref = state.borrow_mut();
        reduce(&mut state_ref.session, action)
    };
    for effect in effects {
        run_effect(effect);
    }
}

// ============================================================================
// Effect 执行：按 GPUI 上下文分派
// ============================================================================

fn run_effect_in_context<V: 'static>(
    state: &Rc<RefCell<AppState>>,
    effect: AppEffect,
    cx: &mut Context<V>,
) {
    match effect {
        AppEffect::Render => cx.notify(),
        // Context<V> 中无法执行以下 effect
        AppEffect::OpenSettingsWindow
        | AppEffect::OpenUrl(_)
        | AppEffect::ApplyTrayIcon(_)
        | AppEffect::QuitApp => {
            warn!(target: "runtime", "effect {:?} ignored: not available in Context<V>", effect);
        }
        other => run_common_effect(state, other),
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
        AppEffect::OpenSettingsWindow => {
            let display_id = window.display(cx).map(|display| display.id());
            state.borrow_mut().view_entity = None;
            window.remove_window();
            schedule_open_settings_window(state.clone(), display_id, cx);
        }
        AppEffect::OpenUrl(url) => crate::utils::platform::open_url(&url),
        AppEffect::ApplyTrayIcon(request) => crate::tray_icon_helper::apply_tray_icon(cx, request),
        AppEffect::QuitApp => cx.quit(),
        other => run_common_effect(state, other),
    }
}

fn run_effect_in_app(state: &Rc<RefCell<AppState>>, effect: AppEffect, cx: &mut App) {
    match effect {
        AppEffect::Render => notify_view_entity(state, cx),
        AppEffect::OpenSettingsWindow => schedule_open_settings_window(state.clone(), None, cx),
        AppEffect::OpenUrl(url) => crate::utils::platform::open_url(&url),
        AppEffect::ApplyTrayIcon(request) => crate::tray_icon_helper::apply_tray_icon(cx, request),
        AppEffect::QuitApp => cx.quit(),
        other => run_common_effect(state, other),
    }
}

// ============================================================================
// Common Effect 执行：不依赖 GPUI 上下文
// ============================================================================

fn run_common_effect(state: &Rc<RefCell<AppState>>, effect: AppEffect) {
    match effect {
        AppEffect::PersistSettings => {
            persist_current_settings(state);
        }
        AppEffect::SendRefreshRequest(request) => {
            let _ = send_refresh_request(state, request);
        }
        AppEffect::SyncAutoLaunch(enabled) => {
            sync_auto_launch(enabled);
        }
        AppEffect::ApplyLocale(language) => {
            crate::i18n::apply_locale(&language);
        }
        AppEffect::UpdateLogLevel(level) => {
            update_log_level(&level);
        }
        AppEffect::SendQuotaNotification { alert, with_sound } => {
            send_system_notification(&alert, with_sound);
        }
        AppEffect::SendPlainNotification { title, body } => {
            // 在独立线程中发送通知，防止 macOS 系统事件导致 GPUI RefCell 重入 panic
            std::thread::spawn(move || {
                crate::notification::send_plain_notification(&title, &body);
            });
        }
        AppEffect::SendDebugNotification { kind, with_sound } => {
            send_system_notification(&build_debug_alert(kind), with_sound);
        }
        AppEffect::OpenLogDirectory => {
            let log_path = state.borrow().log_path.clone();
            if let Some(path) = log_path {
                crate::utils::platform::open_path_in_finder(&path);
            } else {
                warn!(target: "runtime", "OpenLogDirectory: log_path not available");
            }
        }
        AppEffect::CopyToClipboard(text) => {
            crate::utils::platform::copy_to_clipboard(&text);
        }
        AppEffect::StartDebugRefresh(kind) => {
            use crate::utils::log_capture::LogCapture;
            info!(target: "runtime", "starting debug refresh for {:?}", kind);
            // 1. 保存当前日志级别到 state（供 RestoreLogLevel 使用）
            state.borrow_mut().session.debug_ui.prev_log_level = Some(log::max_level());
            // 2. 清空并启用日志捕获
            LogCapture::global().clear();
            LogCapture::global().enable();
            // 3. 临时提升日志级别到 Debug
            log::set_max_level(log::LevelFilter::Debug);
            // 4. 发送手动刷新请求（跳过 cooldown）
            let request = crate::refresh::RefreshRequest::RefreshOne {
                id: kind,
                reason: crate::refresh::RefreshReason::Manual,
            };
            let _ = send_refresh_request(state, request);
        }
        AppEffect::RestoreLogLevel(level) => {
            use crate::utils::log_capture::LogCapture;
            info!(target: "runtime", "debug refresh complete, restoring log level to {:?}", level);
            // 停用日志捕获，恢复日志级别
            LogCapture::global().disable();
            log::set_max_level(level);
        }
        AppEffect::ClearDebugLogs => {
            crate::utils::log_capture::LogCapture::global().clear();
        }
        AppEffect::Render
        | AppEffect::OpenSettingsWindow
        | AppEffect::OpenUrl(_)
        | AppEffect::ApplyTrayIcon(_)
        | AppEffect::QuitApp => {
            warn!(target: "runtime", "unexpected effect in run_common_effect: {:?}", effect);
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

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
    let failed_id = match &request {
        RefreshRequest::RefreshOne { id, .. } => Some(id.clone()),
        _ => None,
    };
    let send_result = state.borrow().send_refresh(request);
    if let Err(err) = send_result {
        warn!(target: "refresh", "failed to send refresh request: {}", err);
        if let Some(ref id) = failed_id {
            if let Some(provider) = state.borrow_mut().session.provider_store.find_by_id_mut(id) {
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
    });
}

fn update_log_level(level: &str) {
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
