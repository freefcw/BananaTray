use crate::application::{reduce, AppAction, AppEffect, ContextEffect};
use gpui::Context;

mod app_state;
mod background_job;
mod diagnostics_context;
mod effects;
pub(crate) mod global_hotkey;
mod gpu_cache;
mod settings_writer;

use std::cell::RefCell;
use std::rc::Rc;

pub use app_state::AppState;
pub(crate) use background_job::{
    BackgroundJobReceiver, BackgroundJobSender, CustomProviderJob, CustomProviderResults,
    PersistentJobReceiver, PersistentJobSender, ScriptTestJob,
};
pub(crate) use diagnostics_context::{
    collect_debug_diagnostics, collect_issue_report_context, debug_context_from_diagnostics,
    DebugDiagnostics,
};
pub use gpu_cache::register_idle_gpu_cache_trim;
pub(crate) use settings_writer::SettingsWriter;

// runtime 是前台内核：只持有 reducer/effect 执行和上下文能力抽象。
// 具体窗口、托盘、D-Bus 句柄与 App/Window 级 dispatch facade 由 bootstrap 组合。
#[cfg(test)]
pub(crate) fn execute_script_provider_test(
    config: &crate::models::ScriptProviderConfig,
) -> crate::models::ScriptProviderTestResult {
    effects::script_provider::execute_script_test(config)
}

pub(crate) fn execute_custom_provider_job(
    job: CustomProviderJob,
    settings_writer: &settings_writer::SettingsWriterHandle,
) -> AppAction {
    match job {
        CustomProviderJob::NewApi { effect, settings } => {
            effects::newapi::execute(effect, settings, settings_writer)
        }
        CustomProviderJob::ScriptProvider { effect, settings } => {
            effects::script_provider::execute(effect, settings, settings_writer)
        }
    }
}

pub(crate) fn execute_script_test_job(job: ScriptTestJob) -> AppAction {
    AppAction::ScriptProviderTestFinished {
        request_id: job.request_id,
        result: effects::script_provider::execute_script_test(&job.config),
    }
}

pub fn dispatch_in_context<V: 'static>(
    state: &Rc<RefCell<AppState>>,
    action: AppAction,
    cx: &mut Context<V>,
) {
    dispatch_effects(state, action, |effect| {
        run_effect_in_context(state, effect, cx)
    });
}

pub(crate) fn dispatch_with_full_context(
    state: &Rc<RefCell<AppState>>,
    action: AppAction,
    caps: &mut dyn FullContextCapabilities,
) {
    dispatch_effects(state, action, |effect| {
        run_effect_with_full_context(state, effect, caps)
    });
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
    mut run_effect: impl FnMut(AppEffect) -> Vec<AppAction>,
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
    let mut pending_effects = std::collections::VecDeque::from(effects);
    while let Some(effect) = pending_effects.pop_front() {
        let follow_up_actions = run_effect(effect);
        for action in follow_up_actions {
            let effects = {
                let mut state_ref = state.borrow_mut();
                reduce(&mut state_ref.session, action)
            };
            pending_effects.extend(effects);
        }
    }
}

// ============================================================================
// Effect 执行：两级路由 + Capability 适配
//
// - CommonEffect  → effects::run_common_effect（按领域子模块执行）
// - ContextEffect → run_full_context_effect / run_view_context_effect + capability traits
//
// 新增 ContextEffect：改枚举定义 + 对应上下文 effect runner。
// 新增 CommonEffect 领域变体：改对应子枚举 + runtime/effects 下同名执行器。
// ============================================================================

/// GPUI 上下文能力抽象。
///
/// 不同 GPUI 入口（Context<V> / Window+App / App）通过 adapter 实现此 trait，
/// 将"当前环境能做什么"与"要做什么"解耦。effect runner 只关心后者。
pub(crate) trait ContextCapabilities {
    fn render(&mut self, state: &Rc<RefCell<AppState>>);
    fn open_url(&mut self, url: &str) {
        if let Err(err) = crate::platform::system::open_url(url) {
            log::warn!(target: "app", "failed to open URL {url}: {err:#}");
        }
    }
}

pub(crate) trait FullContextCapabilities: ContextCapabilities {
    fn open_settings_window(&mut self, state: &Rc<RefCell<AppState>>);
    fn apply_tray_icon(&mut self, request: crate::application::TrayIconRequest);
    fn apply_global_hotkey(&mut self, state: &Rc<RefCell<AppState>>, hotkey: &str) -> AppAction;
    fn quit(&mut self);
}

// ── Adapter: Context<V>（仅支持 Render）─────────────

struct ViewCaps<'a, 'b, V: 'static>(&'a mut Context<'b, V>);

impl<V: 'static> ContextCapabilities for ViewCaps<'_, '_, V> {
    fn render(&mut self, _state: &Rc<RefCell<AppState>>) {
        self.0.notify();
    }
    // open_url 使用 trait 默认实现（platform::system::open_url）。
    // 其它强上下文能力必须走 WindowCaps / AppCaps，避免在 View context 中静默丢 effect。
}

// ── ContextEffect 统一分派（单一 match）─────────────

fn run_full_context_effect(
    state: &Rc<RefCell<AppState>>,
    effect: ContextEffect,
    caps: &mut dyn FullContextCapabilities,
) -> Vec<AppAction> {
    match effect {
        ContextEffect::Render => caps.render(state),
        ContextEffect::OpenSettingsWindow => caps.open_settings_window(state),
        ContextEffect::OpenUrl(url) => caps.open_url(&url),
        ContextEffect::ApplyTrayIcon(request) => caps.apply_tray_icon(request),
        ContextEffect::ApplyGlobalHotkey(hotkey) => {
            return vec![caps.apply_global_hotkey(state, &hotkey)];
        }
        ContextEffect::QuitApp => caps.quit(),
    }
    Vec::new()
}

fn run_view_context_effect(
    state: &Rc<RefCell<AppState>>,
    effect: ContextEffect,
    caps: &mut dyn ContextCapabilities,
) {
    match effect {
        ContextEffect::Render => caps.render(state),
        ContextEffect::OpenUrl(url) => caps.open_url(&url),
        other => panic!(
            "BUG: ContextEffect::{other:?} requires App or Window context; \
             dispatch this action with bootstrap::dispatch_in_app or bootstrap::dispatch_in_window"
        ),
    }
}

// ── Effect 入口（Context / Common 两级路由）─────────

fn run_effect_in_context<V: 'static>(
    state: &Rc<RefCell<AppState>>,
    effect: AppEffect,
    cx: &mut Context<V>,
) -> Vec<AppAction> {
    match effect {
        AppEffect::Context(ctx) => {
            run_view_context_effect(state, ctx, &mut ViewCaps(cx));
            Vec::new()
        }
        AppEffect::Common(common) => effects::run_common_effect(state, common),
    }
}

fn run_effect_with_full_context(
    state: &Rc<RefCell<AppState>>,
    effect: AppEffect,
    caps: &mut dyn FullContextCapabilities,
) -> Vec<AppAction> {
    match effect {
        AppEffect::Context(ctx) => run_full_context_effect(state, ctx, caps),
        AppEffect::Common(common) => effects::run_common_effect(state, common),
    }
}

#[cfg(test)]
mod tests;
