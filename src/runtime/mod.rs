use crate::application::{reduce, AppAction, AppEffect, ContextEffect};
use gpui::{App, Context, Window};
use log::warn;

mod app_state;
mod diagnostics_context;
mod effects;
pub(crate) mod global_hotkey;
mod newapi_io;
mod settings_window_opener;
mod settings_writer;
pub mod ui_hooks;

use std::cell::RefCell;
use std::rc::Rc;

use self::global_hotkey::rebind_global_hotkey;
pub use app_state::AppState;
pub(crate) use diagnostics_context::{collect_debug_context, collect_issue_report_context};
pub use settings_window_opener::schedule_open_settings_window;
pub(crate) use settings_writer::SettingsWriter;

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
// - CommonEffect  → effects::run_common_effect（按领域子模块执行）
// - ContextEffect → run_context_effect（单一 match） + ContextCapabilities trait
//
// 新增 ContextEffect：改枚举定义 + run_context_effect。
// 新增 CommonEffect 领域变体：改对应子枚举 + runtime/effects 下同名执行器。
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

    fn apply_global_hotkey(&mut self, _state: &Rc<RefCell<AppState>>, _hotkey: &str) {
        warn!(target: "runtime", "apply_global_hotkey not available in this context");
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
        ui_hooks::clear_popup_view(state);
        self.window.remove_window();
        schedule_open_settings_window(state.clone(), display_id, self.cx);
    }

    fn apply_tray_icon(&mut self, request: crate::application::TrayIconRequest) {
        crate::tray::apply_tray_icon(self.cx, request);
    }

    fn apply_global_hotkey(&mut self, state: &Rc<RefCell<AppState>>, hotkey: &str) {
        rebind_global_hotkey(state, hotkey, self.cx);
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

    fn apply_global_hotkey(&mut self, state: &Rc<RefCell<AppState>>, hotkey: &str) {
        rebind_global_hotkey(state, hotkey, self.cx);
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
        ContextEffect::ApplyGlobalHotkey(hotkey) => caps.apply_global_hotkey(state, &hotkey),
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
        AppEffect::Common(common) => effects::run_common_effect(state, common),
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
        AppEffect::Common(common) => effects::run_common_effect(state, common),
    }
}

fn run_effect_in_app(state: &Rc<RefCell<AppState>>, effect: AppEffect, cx: &mut App) {
    match effect {
        AppEffect::Context(ctx) => {
            run_context_effect(state, ctx, &mut AppCaps { cx });
        }
        AppEffect::Common(common) => effects::run_common_effect(state, common),
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn notify_view_entity(state: &Rc<RefCell<AppState>>, cx: &mut App) {
    ui_hooks::notify_view(state, cx);
}

#[cfg(test)]
mod tests;
