use crate::application::{
    reduce, AppAction, AppEffect, CommonEffect, ContextEffect, DebugNotificationKind, QuotaAlert,
};
use crate::models::ConnectionStatus;
use crate::platform::notification::send_system_notification;
use crate::refresh::RefreshRequest;
use crate::ui::{persist_settings, schedule_open_settings_window, AppState};
use gpui::{App, Context, Window};
use log::{info, warn};

mod newapi_io;

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
// Effect 执行：两级路由 + Capability 适配
//
// - CommonEffect  → run_common_effect（单一 match，类型安全）
// - ContextEffect → run_context_effect（单一 match） + ContextCapabilities trait
//
// 新增任意 Effect 只需改 2 处：枚举定义 + 对应的 run_*_effect match。
// ============================================================================

/// GPUI 上下文能力抽象。
///
/// 不同 GPUI 入口（Context<V> / Window+App / App）通过 adapter 实现此 trait，
/// 将"当前环境能做什么"与"要做什么"解耦。`run_context_effect` 只关心后者。
trait ContextCapabilities {
    fn render(&mut self, state: &Rc<RefCell<AppState>>);

    fn open_settings_window(&mut self, _state: &Rc<RefCell<AppState>>) {
        warn!(target: "runtime", "open_settings_window not available in this context");
    }

    fn open_url(&mut self, url: &str) {
        crate::platform::system::open_url(url);
    }

    fn apply_tray_icon(&mut self, _request: crate::application::TrayIconRequest) {
        warn!(target: "runtime", "apply_tray_icon not available in this context");
    }

    fn quit(&mut self) {
        warn!(target: "runtime", "quit not available in this context");
    }
}

// ── Adapter: Context<V>（仅支持 Render）─────────────

struct ViewCaps<'a, 'b, V: 'static>(&'a mut Context<'b, V>);

impl<V: 'static> ContextCapabilities for ViewCaps<'_, '_, V> {
    fn render(&mut self, _state: &Rc<RefCell<AppState>>) {
        self.0.notify();
    }
    // open_settings_window / apply_tray_icon / quit 使用 trait 默认 warn 实现
    // open_url 使用 trait 默认实现（platform::system::open_url）
}

// ── Adapter: Window + App（全能力）──────────────────

struct WindowCaps<'a> {
    window: &'a mut Window,
    cx: &'a mut App,
}

impl ContextCapabilities for WindowCaps<'_> {
    fn render(&mut self, _state: &Rc<RefCell<AppState>>) {
        self.window.refresh();
    }

    fn open_settings_window(&mut self, state: &Rc<RefCell<AppState>>) {
        let display_id = self.window.display(self.cx).map(|display| display.id());
        state.borrow_mut().view_entity = None;
        self.window.remove_window();
        schedule_open_settings_window(state.clone(), display_id, self.cx);
    }

    fn apply_tray_icon(&mut self, request: crate::application::TrayIconRequest) {
        crate::tray::apply_tray_icon(self.cx, request);
    }

    fn quit(&mut self) {
        self.cx.quit();
    }
}

// ── Adapter: App（大部分能力，Render 使用 view entity）

struct AppCaps<'a> {
    cx: &'a mut App,
}

impl ContextCapabilities for AppCaps<'_> {
    fn render(&mut self, state: &Rc<RefCell<AppState>>) {
        notify_view_entity(state, self.cx);
    }

    fn open_settings_window(&mut self, state: &Rc<RefCell<AppState>>) {
        schedule_open_settings_window(state.clone(), None, self.cx);
    }

    fn apply_tray_icon(&mut self, request: crate::application::TrayIconRequest) {
        crate::tray::apply_tray_icon(self.cx, request);
    }

    fn quit(&mut self) {
        self.cx.quit();
    }
}

// ── ContextEffect 统一分派（单一 match）─────────────

fn run_context_effect(
    state: &Rc<RefCell<AppState>>,
    effect: ContextEffect,
    caps: &mut dyn ContextCapabilities,
) {
    match effect {
        ContextEffect::Render => caps.render(state),
        ContextEffect::OpenSettingsWindow => caps.open_settings_window(state),
        ContextEffect::OpenUrl(url) => caps.open_url(&url),
        ContextEffect::ApplyTrayIcon(request) => caps.apply_tray_icon(request),
        ContextEffect::QuitApp => caps.quit(),
    }
}

// ── Effect 入口（Context / Common 两级路由）─────────

fn run_effect_in_context<V: 'static>(
    state: &Rc<RefCell<AppState>>,
    effect: AppEffect,
    cx: &mut Context<V>,
) {
    match effect {
        AppEffect::Context(ctx) => run_context_effect(state, ctx, &mut ViewCaps(cx)),
        AppEffect::Common(common) => run_common_effect(state, common),
    }
}

fn run_effect_in_window(
    state: &Rc<RefCell<AppState>>,
    effect: AppEffect,
    window: &mut Window,
    cx: &mut App,
) {
    match effect {
        AppEffect::Context(ctx) => {
            run_context_effect(state, ctx, &mut WindowCaps { window, cx });
        }
        AppEffect::Common(common) => run_common_effect(state, common),
    }
}

fn run_effect_in_app(state: &Rc<RefCell<AppState>>, effect: AppEffect, cx: &mut App) {
    match effect {
        AppEffect::Context(ctx) => {
            run_context_effect(state, ctx, &mut AppCaps { cx });
        }
        AppEffect::Common(common) => run_common_effect(state, common),
    }
}

// ============================================================================
// Common Effect 执行：不依赖 GPUI 上下文（类型安全，无 catch-all 分支）
// ============================================================================

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
        CommonEffect::SendPlainNotification { title, body } => {
            // 在独立线程中发送通知，防止 macOS 系统事件导致 GPUI RefCell 重入 panic
            std::thread::spawn(move || {
                crate::platform::notification::send_plain_notification(&title, &body);
            });
        }
        CommonEffect::SendDebugNotification { kind, with_sound } => {
            send_system_notification(&build_debug_alert(kind), with_sound);
        }
        CommonEffect::OpenLogDirectory => {
            let log_path = state.borrow().log_path.clone();
            if let Some(path) = log_path {
                crate::platform::system::open_path_in_finder(&path);
            } else {
                warn!(target: "runtime", "OpenLogDirectory: log_path not available");
            }
        }
        CommonEffect::CopyToClipboard(text) => {
            crate::platform::system::copy_to_clipboard(&text);
        }
        CommonEffect::StartDebugRefresh(kind) => {
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
        CommonEffect::SaveNewApiProvider {
            config,
            original_filename,
            is_editing,
        } => {
            use crate::application::newapi_ops;
            use crate::providers::custom::generator;

            let filename =
                original_filename.unwrap_or_else(|| generator::generate_filename(&config));

            match newapi_io::save_newapi_yaml(&config, &filename) {
                Ok(path) => {
                    info!(target: "runtime", "saved custom provider YAML to {}", path.display());
                    let settings_saved = persist_current_settings(state);
                    let (title_key, body_key) =
                        newapi_ops::newapi_save_notification_keys(is_editing, settings_saved);
                    // 在独立线程中发送通知，与 SendPlainNotification handler 保持一致，
                    // 防止 macOS 系统事件导致 GPUI RefCell 重入 panic
                    let title = rust_i18n::t!(title_key).to_string();
                    let body = rust_i18n::t!(body_key).to_string();
                    std::thread::spawn(move || {
                        crate::platform::notification::send_plain_notification(&title, &body);
                    });
                    let _ = send_refresh_request(state, RefreshRequest::ReloadProviders);
                }
                Err(e) => {
                    warn!(target: "runtime", "failed to save newapi: {}", e);
                    let mut s = state.borrow_mut();
                    if is_editing {
                        newapi_ops::rollback_newapi_edit(&mut s.session, &config, &filename);
                    } else {
                        newapi_ops::rollback_newapi_create(&mut s.session, &config);
                    }
                }
            }
        }
        CommonEffect::DeleteNewApiProvider { provider_id } => {
            use crate::providers::custom::generator;
            if let crate::models::ProviderId::Custom(ref custom_id) = provider_id {
                if let Some(filename) = generator::filename_for_id(custom_id) {
                    let path = crate::platform::paths::custom_provider_path(&filename);
                    match std::fs::remove_file(&path) {
                        Ok(()) => {
                            info!(target: "runtime", "deleted custom provider YAML: {}", path.display());
                            let _ = send_refresh_request(state, RefreshRequest::ReloadProviders);
                        }
                        Err(e) => {
                            warn!(target: "runtime", "failed to delete YAML {}: {}", path.display(), e);
                        }
                    }
                } else {
                    warn!(
                        target: "settings",
                        "DeleteNewApiProvider: not a newapi provider id: {}",
                        custom_id
                    );
                }
            }
        }
        CommonEffect::LoadNewApiConfig { provider_id } => {
            use crate::providers::custom::generator;
            if let crate::models::ProviderId::Custom(ref custom_id) = provider_id {
                if let Some(edit_data) = generator::read_newapi_config(custom_id) {
                    let mut s = state.borrow_mut();
                    s.session.settings_ui.adding_newapi = true;
                    s.session.settings_ui.editing_newapi = Some(edit_data);
                } else {
                    warn!(
                        target: "settings",
                        "LoadNewApiConfig: failed to read config for {}",
                        custom_id
                    );
                }
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn persist_current_settings(state: &Rc<RefCell<AppState>>) -> bool {
    let settings = state.borrow().session.settings.clone();
    persist_settings(&settings)
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
        crate::platform::auto_launch::sync(enabled);
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

#[cfg(test)]
mod tests;
