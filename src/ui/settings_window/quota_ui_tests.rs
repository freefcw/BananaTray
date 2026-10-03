use super::quota_thresholds::{self, QuotaThresholdChange};
use super::quota_usage::render_quota_usage_stepper;
use super::SettingsView;
use crate::application::{QuotaThresholdUnitViewState, SettingChange};
use crate::i18n::test_locale_guard;
use crate::models::{
    AppSettings, ProviderId, ProviderKind, QuotaThresholdTarget, QuotaThresholdUnit,
    QuotaThresholds, QuotaThresholdsError,
};
use crate::providers::{ProviderManager, ProviderManagerHandle};
use crate::runtime::{AppState, BackgroundJobSender, PersistentJobSender, SettingsWriter};
use crate::theme::Theme;
use gpui::{
    div, point, px, size, AppContext, Bounds, Context, Entity, IntoElement, Modifiers,
    ParentElement, Pixels, Render, Styled, TestAppContext, VisualTestContext, Window,
};
use std::cell::RefCell;
use std::rc::Rc;

fn make_state() -> Rc<RefCell<AppState>> {
    let (tx, _rx) = smol::channel::bounded(1);
    let (custom_provider_tx, _custom_provider_rx) = PersistentJobSender::channel(1);
    let (script_test_tx, _script_test_rx) = BackgroundJobSender::channel(1);
    let manager = ProviderManagerHandle::new(ProviderManager::new());
    let state = Rc::new(RefCell::new(AppState::new(
        crate::refresh::RefreshWorker::detached(tx),
        custom_provider_tx,
        script_test_tx,
        manager,
        AppSettings::default(),
        None,
    )));
    state.borrow_mut().settings_writer = SettingsWriter::spawn_for_test(|_| true);
    state
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear());
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing debug bounds for selector {selector}"))
}

fn click_selector(cx: &mut VisualTestContext, selector: &'static str) {
    draw(cx);
    let target = bounds(cx, selector);
    cx.simulate_click(target.center(), Modifiers::none());
    draw(cx);
}

struct UsageHarness {
    selection: Option<u8>,
    inherited: Option<u8>,
    changes: Rc<RefCell<Vec<Option<u8>>>>,
}

impl Render for UsageHarness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selection = self.selection;
        let inherited = self.inherited;
        let changes = self.changes.clone();
        let entity = cx.entity().clone();
        let theme = Theme::dark();
        div().size_full().child(render_quota_usage_stepper(
            selection,
            inherited,
            &theme,
            move |step, _window, cx| {
                changes.borrow_mut().push(step);
                entity.update(cx, |harness, cx| {
                    harness.selection = step;
                    cx.notify();
                });
            },
        ))
    }
}

fn add_usage_window<'a>(
    cx: &'a mut TestAppContext,
    selection: Option<u8>,
    inherited: Option<u8>,
    changes: &Rc<RefCell<Vec<Option<u8>>>>,
) -> (Entity<UsageHarness>, &'a mut VisualTestContext) {
    let changes = changes.clone();
    cx.add_window_view(move |_, _| UsageHarness {
        selection,
        inherited,
        changes,
    })
}

struct ThresholdHarness {
    view: Entity<SettingsView>,
    target: QuotaThresholdTarget,
    provider_layout: bool,
    changes: Rc<RefCell<Vec<SettingChange>>>,
}

impl Render for ThresholdHarness {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::dark();
        let target = self.target.clone();
        let changes = self.changes.clone();
        let on_change: Rc<QuotaThresholdChange> = Rc::new(move |change, window, _cx| {
            changes.borrow_mut().push(change);
            window.refresh();
        });
        let section = self.view.update(cx, |view, cx| {
            let units: Vec<QuotaThresholdUnitViewState> = {
                let state = view.state.borrow();
                let settings = &state.session.settings;
                QuotaThresholdUnit::ALL
                    .iter()
                    .map(|&unit| match &target {
                        QuotaThresholdTarget::Global => QuotaThresholdUnitViewState {
                            unit,
                            override_thresholds: None,
                            effective: settings.quota.thresholds(unit),
                        },
                        QuotaThresholdTarget::Provider(id) => QuotaThresholdUnitViewState {
                            unit,
                            override_thresholds: settings
                                .provider
                                .quota_threshold_override(id, unit),
                            effective: settings.effective_quota_rules(id).thresholds(unit),
                        },
                    })
                    .collect()
            };
            quota_thresholds::render_quota_thresholds_section(
                view,
                target.clone(),
                &units,
                &theme,
                window,
                cx,
                on_change,
            )
        });
        if self.provider_layout {
            div()
                .size_full()
                .flex()
                .child(div().flex_shrink_0().h_full().w(px(160.0)))
                .child(div().flex_shrink_0().h_full().w(px(1.0)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .px(px(24.0))
                        .child(section),
                )
        } else {
            div().size_full().px(px(16.0)).child(section)
        }
    }
}

fn add_threshold_window<'a>(
    cx: &'a mut TestAppContext,
    target: QuotaThresholdTarget,
    provider_layout: bool,
    changes: &Rc<RefCell<Vec<SettingChange>>>,
) -> (Entity<ThresholdHarness>, &'a mut VisualTestContext) {
    let changes = changes.clone();
    cx.add_window_view(move |_, cx| ThresholdHarness {
        view: cx.new(|cx| SettingsView::new(make_state(), cx)),
        target,
        provider_layout,
        changes,
    })
}

fn harness_view(
    harness: &Entity<ThresholdHarness>,
    cx: &VisualTestContext,
) -> Entity<SettingsView> {
    harness.read_with(cx, |harness, _| harness.view.clone())
}

fn begin_draft(
    view: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
    target: &QuotaThresholdTarget,
    unit: QuotaThresholdUnit,
) {
    view.update(cx, |view, cx| {
        let effective = {
            let settings = &view.state.borrow().session.settings;
            match target {
                QuotaThresholdTarget::Global => settings.quota.thresholds(unit),
                QuotaThresholdTarget::Provider(id) => {
                    settings.effective_quota_rules(id).thresholds(unit)
                }
            }
        };
        view.begin_quota_threshold_draft(target.clone(), unit, effective, cx);
    });
    draw(cx);
}

fn set_draft_values(
    view: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
    warning: &str,
    critical: &str,
    notify: &str,
) {
    let (warning_input, critical_input, notify_input) = view.read_with(cx, |view, _| {
        let draft = view
            .quota_threshold_draft
            .as_ref()
            .expect("threshold draft must be open");
        (
            draft.warning.clone(),
            draft.critical.clone(),
            draft.notify.clone(),
        )
    });
    warning_input.update(cx, |input, _| {
        input.content = warning.into();
    });
    critical_input.update(cx, |input, _| {
        input.content = critical.into();
    });
    notify_input.update(cx, |input, _| {
        input.content = notify.into();
    });
}

fn assert_threshold_fields_stacked(cx: &mut VisualTestContext, case: &str) {
    draw(cx);
    let parent = bounds(cx, "quota-threshold-fields");
    let notify = bounds(cx, "quota-threshold-notify");
    let critical = bounds(cx, "quota-threshold-critical");
    let warning = bounds(cx, "quota-threshold-warning");

    for (name, field, label_selector) in [
        ("notify", notify, "quota-threshold-notify-label"),
        ("critical", critical, "quota-threshold-critical-label"),
        ("warning", warning, "quota-threshold-warning-label"),
    ] {
        assert_eq!(
            field.origin.x, parent.origin.x,
            "[{case}] {name} field x must equal the fields container x: {field:?} vs {parent:?}"
        );
        assert_eq!(
            field.size.width, parent.size.width,
            "[{case}] {name} field must span the fields container width: {field:?} vs {parent:?}"
        );
        let label = bounds(cx, label_selector);
        assert!(
            label.right() <= field.right() + px(0.5),
            "[{case}] {name} label overflows its field: {label:?} vs {field:?}"
        );
    }

    assert!(
        notify.bottom() <= critical.top(),
        "[{case}] notify must sit above critical: {notify:?} vs {critical:?}"
    );
    assert!(
        critical.bottom() <= warning.top(),
        "[{case}] critical must sit above warning: {critical:?} vs {warning:?}"
    );
}

#[gpui::test]
fn quota_usage_inherited_start_increments_from_global(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    let changes = Rc::new(RefCell::new(Vec::new()));
    let (_harness, cx) = add_usage_window(cx, None, Some(5), &changes);

    click_selector(cx, "stepper-increment");
    assert_eq!(changes.borrow().as_slice(), &[Some(6)]);
}

#[gpui::test]
fn quota_usage_inherited_start_decrements_from_global(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    let changes = Rc::new(RefCell::new(Vec::new()));
    let (_harness, cx) = add_usage_window(cx, None, Some(5), &changes);

    click_selector(cx, "stepper-decrement");
    assert_eq!(changes.borrow().as_slice(), &[Some(4)]);
}

#[gpui::test]
fn quota_usage_explicit_zero_stays_off_until_increment(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    let changes = Rc::new(RefCell::new(Vec::new()));
    let (_harness, cx) = add_usage_window(cx, Some(0), Some(5), &changes);

    click_selector(cx, "stepper-decrement");
    assert!(
        changes.borrow().is_empty(),
        "decrement at 0 must not emit a change: {:?}",
        changes.borrow()
    );

    click_selector(cx, "stepper-increment");
    assert_eq!(changes.borrow().as_slice(), &[Some(1)]);
}

#[gpui::test]
fn quota_usage_follow_global_then_steps_from_new_global(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    let changes = Rc::new(RefCell::new(Vec::new()));
    let (harness, cx) = add_usage_window(cx, Some(6), Some(5), &changes);

    click_selector(cx, "quota-usage-follow-global");
    assert_eq!(changes.borrow().as_slice(), &[None]);

    harness.update(cx, |harness, cx| {
        harness.inherited = Some(20);
        cx.notify();
    });
    click_selector(cx, "stepper-increment");
    assert_eq!(changes.borrow().as_slice(), &[None, Some(25)]);
}

#[gpui::test]
fn quota_usage_global_entry_has_no_follow_button(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    let changes = Rc::new(RefCell::new(Vec::new()));
    let (_harness, cx) = add_usage_window(cx, Some(5), None, &changes);

    draw(cx);
    assert!(
        cx.debug_bounds("quota-usage-follow-global").is_none(),
        "global entry must not render the follow-global action"
    );
}

#[gpui::test]
fn quota_usage_stepper_clamps_at_zero_and_hundred(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    let changes = Rc::new(RefCell::new(Vec::new()));
    let (harness, cx) = add_usage_window(cx, Some(0), None, &changes);

    click_selector(cx, "stepper-decrement");
    assert!(
        changes.borrow().is_empty(),
        "decrement at 0 must not emit a change: {:?}",
        changes.borrow()
    );

    harness.update(cx, |harness, cx| {
        harness.selection = Some(100);
        cx.notify();
    });
    click_selector(cx, "stepper-increment");
    assert!(
        changes.borrow().is_empty(),
        "increment at 100 must not emit a change: {:?}",
        changes.borrow()
    );

    click_selector(cx, "stepper-decrement");
    assert_eq!(changes.borrow().as_slice(), &[Some(95)]);
}

#[gpui::test]
fn threshold_edit_fields_stack_full_width_in_global_section(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    let changes = Rc::new(RefCell::new(Vec::new()));
    for locale in ["en", "zh-CN"] {
        rust_i18n::set_locale(locale);
        for window_width in [600.0_f32, 460.0] {
            let (harness, cx) =
                add_threshold_window(cx, QuotaThresholdTarget::Global, false, &changes);
            cx.simulate_resize(size(px(window_width), px(800.0)));
            let view = harness_view(&harness, cx);
            for unit in QuotaThresholdUnit::ALL {
                let case = format!("global/{locale}/{window_width}px/{unit:?}");
                begin_draft(&view, cx, &QuotaThresholdTarget::Global, unit);
                assert_threshold_fields_stacked(cx, &case);
            }
        }
    }
    rust_i18n::set_locale("en");
}

#[gpui::test]
fn threshold_edit_fields_stack_full_width_in_provider_section(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    let changes = Rc::new(RefCell::new(Vec::new()));
    let provider = ProviderId::BuiltIn(ProviderKind::Claude);
    for locale in ["en", "zh-CN"] {
        rust_i18n::set_locale(locale);
        for window_width in [600.0_f32, 460.0] {
            let target = QuotaThresholdTarget::Provider(provider.clone());
            let (harness, cx) = add_threshold_window(cx, target.clone(), true, &changes);
            cx.simulate_resize(size(px(window_width), px(800.0)));
            let view = harness_view(&harness, cx);
            for unit in QuotaThresholdUnit::ALL {
                let case = format!("provider/{locale}/{window_width}px/{unit:?}");
                begin_draft(&view, cx, &target, unit);
                assert_threshold_fields_stacked(cx, &case);
            }
        }
    }
    rust_i18n::set_locale("en");
}

#[gpui::test]
fn threshold_global_save_via_real_input_chain(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    cx.update(adabraka_ui::init);
    let changes = Rc::new(RefCell::new(Vec::new()));
    let (harness, cx) = add_threshold_window(cx, QuotaThresholdTarget::Global, false, &changes);
    cx.simulate_resize(size(px(600.0), px(800.0)));
    let view = harness_view(&harness, cx);
    begin_draft(
        &view,
        cx,
        &QuotaThresholdTarget::Global,
        QuotaThresholdUnit::Percentage,
    );
    set_draft_values(&view, cx, "40", "15", "10");

    let notify_field = bounds(cx, "quota-threshold-notify");
    cx.simulate_click(
        point(notify_field.center().x, notify_field.bottom() - px(4.0)),
        Modifiers::none(),
    );
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("5");
    draw(cx);

    let notify_input = view.read_with(cx, |view, _| {
        view.quota_threshold_draft
            .as_ref()
            .expect("draft must be open")
            .notify
            .clone()
    });
    assert_eq!(
        notify_input.read_with(cx, |input, _| input.content().to_string()),
        "5"
    );

    click_selector(cx, "quota-threshold-save");
    {
        let recorded = changes.borrow();
        assert_eq!(recorded.len(), 1, "expected exactly one SettingChange");
        match &recorded[0] {
            SettingChange::SetGlobalQuotaThresholds { unit, thresholds } => {
                assert_eq!(*unit, QuotaThresholdUnit::Percentage);
                assert_eq!(
                    *thresholds,
                    QuotaThresholds {
                        warning: 40.0,
                        critical: 15.0,
                        notify: 5.0,
                    }
                );
            }
            other => panic!("unexpected change: {other:?}"),
        }
    }
    assert!(
        view.read_with(cx, |view, _| view.quota_threshold_draft.is_none()),
        "draft must be cleared after a successful save"
    );
}

#[gpui::test]
fn threshold_provider_save_dispatches_provider_change(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    cx.update(adabraka_ui::init);
    let changes = Rc::new(RefCell::new(Vec::new()));
    let provider = ProviderId::BuiltIn(ProviderKind::Claude);
    let target = QuotaThresholdTarget::Provider(provider.clone());
    let (harness, cx) = add_threshold_window(cx, target.clone(), true, &changes);
    cx.simulate_resize(size(px(600.0), px(800.0)));
    let view = harness_view(&harness, cx);
    begin_draft(&view, cx, &target, QuotaThresholdUnit::Currency);
    set_draft_values(&view, cx, "40", "15", "5");

    click_selector(cx, "quota-threshold-save");
    {
        let recorded = changes.borrow();
        assert_eq!(recorded.len(), 1, "expected exactly one SettingChange");
        match &recorded[0] {
            SettingChange::SetProviderQuotaThresholds {
                provider_id,
                unit,
                thresholds,
            } => {
                assert_eq!(*provider_id, provider);
                assert_eq!(*unit, QuotaThresholdUnit::Currency);
                assert_eq!(
                    *thresholds,
                    Some(QuotaThresholds {
                        warning: 40.0,
                        critical: 15.0,
                        notify: 5.0,
                    })
                );
            }
            other => panic!("unexpected change: {other:?}"),
        }
    }
    assert!(
        view.read_with(cx, |view, _| view.quota_threshold_draft.is_none()),
        "draft must be cleared after a successful save"
    );
}

#[gpui::test]
fn threshold_cancel_discards_draft_without_change(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    let changes = Rc::new(RefCell::new(Vec::new()));
    let (harness, cx) = add_threshold_window(cx, QuotaThresholdTarget::Global, false, &changes);
    cx.simulate_resize(size(px(600.0), px(800.0)));
    let view = harness_view(&harness, cx);
    let saved_before = view.read_with(cx, |view, _| {
        view.state.borrow().session.settings.quota.percentage
    });

    begin_draft(
        &view,
        cx,
        &QuotaThresholdTarget::Global,
        QuotaThresholdUnit::Percentage,
    );
    set_draft_values(&view, cx, "40", "15", "5");
    click_selector(cx, "quota-threshold-cancel");

    assert!(
        changes.borrow().is_empty(),
        "cancel must not dispatch SettingChange: {:?}",
        changes.borrow()
    );
    assert!(
        view.read_with(cx, |view, _| view.quota_threshold_draft.is_none()),
        "draft must be cleared after cancel"
    );
    let saved_after = view.read_with(cx, |view, _| {
        view.state.borrow().session.settings.quota.percentage
    });
    assert_eq!(
        saved_before, saved_after,
        "saved settings must be unchanged"
    );
}

#[gpui::test]
fn threshold_invalid_order_keeps_draft_and_error(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    let changes = Rc::new(RefCell::new(Vec::new()));
    let (harness, cx) = add_threshold_window(cx, QuotaThresholdTarget::Global, false, &changes);
    cx.simulate_resize(size(px(600.0), px(800.0)));
    let view = harness_view(&harness, cx);
    begin_draft(
        &view,
        cx,
        &QuotaThresholdTarget::Global,
        QuotaThresholdUnit::Percentage,
    );
    set_draft_values(&view, cx, "50", "20", "25");

    click_selector(cx, "quota-threshold-save");
    assert!(
        changes.borrow().is_empty(),
        "invalid order must not dispatch SettingChange: {:?}",
        changes.borrow()
    );

    let (error, warning, critical, notify) = view.read_with(cx, |view, app| {
        let draft = view
            .quota_threshold_draft
            .as_ref()
            .expect("draft must remain after invalid save");
        (
            draft.error,
            draft.warning.read(app).content().to_string(),
            draft.critical.read(app).content().to_string(),
            draft.notify.read(app).content().to_string(),
        )
    });
    assert_eq!(error, Some(QuotaThresholdsError::InvalidOrder));
    assert_eq!(
        (warning.as_str(), critical.as_str(), notify.as_str()),
        ("50", "20", "25")
    );
}
