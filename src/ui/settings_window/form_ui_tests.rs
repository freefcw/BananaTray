//! 设置表单字段级校验回归：fc-ui state 校验规则接线 + 错误可读性。
//!
//! 覆盖 NewAPI / Script Provider 两个表单的 `collect_*` 提交路径：
//! 校验失败必须落到对应字段 state 的 `validation_error`（渲染层据此画红描边
//! 和字段下文案），且不再静默吞掉；修正输入后复验必须清除错误。

use super::quota_ui_tests::make_state;
use super::{FormInputsCache, NewApiFormInputs, ScriptProviderFormInputs, SettingsView};
use crate::application::{AppAction, FormIdentity};
use crate::i18n::test_locale_guard;
use fc_ui::components::input_state::InputState;
use fc_ui::components::textarea_state::TextareaState;
use gpui::{AppContext, Entity, TestAppContext};

fn install_script_inputs(
    view: &Entity<SettingsView>,
    cx: &mut TestAppContext,
) -> ScriptProviderFormInputs {
    view.update(cx, |view, cx| {
        let inputs = ScriptProviderFormInputs::new_add(cx);
        view.script_provider_inputs = Some(FormInputsCache {
            identity: FormIdentity::ScriptProviderAdd,
            inputs: inputs.clone(),
        });
        inputs
    })
}

fn install_newapi_inputs(view: &Entity<SettingsView>, cx: &mut TestAppContext) -> NewApiFormInputs {
    view.update(cx, |view, cx| {
        let inputs = NewApiFormInputs::new_add(cx);
        view.newapi_inputs = Some(FormInputsCache {
            identity: FormIdentity::NewApiAdd,
            inputs: inputs.clone(),
        });
        inputs
    })
}

fn set_input(entity: &Entity<InputState>, value: &str, cx: &mut TestAppContext) {
    entity.update(cx, |state, _| state.content = value.into());
}

fn set_textarea(entity: &Entity<TextareaState>, value: &str, cx: &mut TestAppContext) {
    entity.update(cx, |state, _| state.content = value.into());
}

fn input_error(entity: &Entity<InputState>, cx: &TestAppContext) -> Option<String> {
    entity.read_with(cx, |state, _| {
        state
            .validation_error
            .as_ref()
            .map(|e| e.message.to_string())
    })
}

fn textarea_error(entity: &Entity<TextareaState>, cx: &TestAppContext) -> Option<String> {
    entity.read_with(cx, |state, _| {
        state
            .validation_error
            .as_ref()
            .map(|e| e.message.to_string())
    })
}

/// fc-ui 内置 required 规则的固定英文文案。渲染层依赖它做本地化映射
/// （`providers::shared::localize_validation_message`），fc-ui 升级若改动
/// 该文案，这里的断言会先失败，提醒同步映射表。
const FC_UI_REQUIRED_MESSAGE: &str = "This field is required";

#[gpui::test]
fn script_form_empty_required_fields_block_collect_and_surface_errors(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    cx.update(fc_ui::init);
    let view = cx.new(|cx| SettingsView::new(make_state(), cx));
    let inputs = install_script_inputs(&view, cx);
    // name 本为空；脚本清空以触发 required。interpreter / timeout 有默认值。
    set_textarea(&inputs.script, "", cx);

    view.update(cx, |view, cx| {
        assert!(view.collect_script_provider_config(cx).is_none());
    });

    assert_eq!(
        input_error(&inputs.name, cx).as_deref(),
        Some(FC_UI_REQUIRED_MESSAGE)
    );
    assert_eq!(
        textarea_error(&inputs.script, cx).as_deref(),
        Some(FC_UI_REQUIRED_MESSAGE)
    );
    assert!(input_error(&inputs.interpreter, cx).is_none());
    assert!(input_error(&inputs.timeout, cx).is_none());
}

#[gpui::test]
fn script_form_invalid_timeout_then_fix_clears_error(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    cx.update(fc_ui::init);
    let view = cx.new(|cx| SettingsView::new(make_state(), cx));
    let inputs = install_script_inputs(&view, cx);
    // change 触发即时复验的接线必须存在
    assert!(inputs.timeout.read_with(cx, |s, _| s.validate_on_change));
    assert!(inputs.script.read_with(cx, |s, _| s.validate_on_change));

    set_input(&inputs.name, "My Balance", cx);
    set_input(&inputs.timeout, "abc", cx);
    view.update(cx, |view, cx| {
        assert!(view.collect_script_provider_config(cx).is_none());
    });
    assert_eq!(
        input_error(&inputs.timeout, cx).as_deref(),
        Some("Timeout must be a positive whole number of seconds.")
    );

    // 修正输入后复验通过，错误清除，配置成功产出
    set_input(&inputs.timeout, "45", cx);
    let config = view.update(cx, |view, cx| view.collect_script_provider_config(cx));
    let config = config.expect("valid form must collect");
    assert_eq!(config.display_name, "My Balance");
    assert_eq!(config.timeout_ms, 45_000);
    assert!(!config.provider_id.is_empty());
    assert!(input_error(&inputs.timeout, cx).is_none());
}

#[gpui::test]
fn newapi_form_missing_required_blocks_submit_and_surfaces_field_errors(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    cx.update(fc_ui::init);
    let view = cx.new(|cx| SettingsView::new(make_state(), cx));
    let inputs = install_newapi_inputs(&view, cx);

    view.update(cx, |view, cx| {
        assert!(view.collect_submit_action(cx).is_none());
        // 字段级失败不再触发表单级横幅
        assert!(view.newapi_form_error.is_none());
    });

    assert_eq!(
        input_error(&inputs.name, cx).as_deref(),
        Some(FC_UI_REQUIRED_MESSAGE)
    );
    assert_eq!(
        input_error(&inputs.url, cx).as_deref(),
        Some(FC_UI_REQUIRED_MESSAGE)
    );
    assert_eq!(
        textarea_error(&inputs.cookie, cx).as_deref(),
        Some(FC_UI_REQUIRED_MESSAGE)
    );
    // user_id / divisor 可留空
    assert!(input_error(&inputs.user_id, cx).is_none());
    assert!(input_error(&inputs.divisor, cx).is_none());
}

#[gpui::test]
fn newapi_form_invalid_divisor_then_valid_submit(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    cx.update(fc_ui::init);
    let view = cx.new(|cx| SettingsView::new(make_state(), cx));
    let inputs = install_newapi_inputs(&view, cx);
    assert!(inputs.name.read_with(cx, |s, _| s.validate_on_change));

    set_input(&inputs.name, "Relay", cx);
    set_input(&inputs.url, "https://relay.example.com", cx);
    set_textarea(&inputs.cookie, "session=abc", cx);
    set_input(&inputs.divisor, "oops", cx);

    view.update(cx, |view, cx| {
        assert!(view.collect_submit_action(cx).is_none());
    });
    assert_eq!(
        input_error(&inputs.divisor, cx).as_deref(),
        Some("Credit ratio must be a positive number.")
    );

    set_input(&inputs.divisor, "500000", cx);
    let action = view.update(cx, |view, cx| view.collect_submit_action(cx));
    let Some(AppAction::SubmitNewApi(config)) = action else {
        panic!("valid form must produce SubmitNewApi");
    };
    assert_eq!(config.display_name, "Relay");
    assert_eq!(config.base_url, "https://relay.example.com");
    assert_eq!(config.divisor, Some(500000.0));
    assert!(input_error(&inputs.divisor, cx).is_none());
}

struct BackgroundOnlyTokenProvider {
    foreground_thread: std::thread::ThreadId,
}

#[async_trait::async_trait]
impl crate::providers::AiProvider for BackgroundOnlyTokenProvider {
    fn descriptor(&self) -> crate::models::ProviderDescriptor {
        crate::models::ProviderDescriptor {
            id: "background-token:test".into(),
            metadata: crate::models::ProviderMetadata::fallback(
                crate::models::ProviderKind::Custom,
            ),
        }
    }
}

impl crate::providers::ProviderCapabilities for BackgroundOnlyTokenProvider {
    fn settings_capability(&self) -> crate::models::SettingsCapability {
        crate::providers::copilot_settings_capability()
    }

    fn resolve_token_input_state(
        &self,
        _: &crate::models::AppSettings,
    ) -> Option<crate::models::TokenInputState> {
        assert_ne!(std::thread::current().id(), self.foreground_thread);
        Some(crate::models::TokenInputState {
            has_token: true,
            masked: None,
            source_i18n_key: Some("copilot.source.env_var"),
            edit_mode: crate::models::TokenEditMode::SetNew,
        })
    }
}

#[gpui::test]
fn token_panel_render_reads_snapshot_and_never_probes_provider(cx: &mut TestAppContext) {
    let _locale_guard = test_locale_guard("en");
    cx.update(fc_ui::init);
    let state = make_state();
    let id = crate::models::ProviderId::Custom("background-token:test".to_string());
    let mut manager = crate::providers::ProviderManager::new();
    manager.register(std::sync::Arc::new(BackgroundOnlyTokenProvider {
        foreground_thread: std::thread::current().id(),
    }));
    let crate::models::SettingsCapability::TokenInput(capability) =
        crate::providers::copilot_settings_capability()
    else {
        unreachable!()
    };
    {
        let mut state = state.borrow_mut();
        state.manager = crate::providers::ProviderManagerHandle::new(manager);
        state.session.provider_store.providers = state.manager.snapshot().initial_statuses();
        state.session.settings_ui.selected_provider = id.clone();
    }
    {
        let mut state = state.borrow_mut();
        crate::application::reduce(
            &mut state.session,
            AppAction::SetSettingsTab(crate::application::SettingsTab::Providers),
        );
        crate::application::reduce(&mut state.session, AppAction::SettingsWindowOpened);
    }
    let view = cx.new(|cx| SettingsView::new(state.clone(), cx));
    view.update(cx, |view, cx| {
        let snapshot = crate::application::settings_providers_tab_view_state(&state.borrow().session);
        assert_eq!(snapshot.detail.token_input, crate::application::TokenInputDisplayState::Loading);
        let _panel = super::providers::token_input_panel::render_token_input_panel(
            &id, capability, &snapshot.detail.token_input, view, &crate::theme::Theme::dark(), cx,
        );
        let request_id = state
            .borrow()
            .session
            .settings_ui
            .token_input_query
            .request_id;
        crate::application::reduce(&mut state.borrow_mut().session, AppAction::TokenInputStateResolved {
            provider_id: id.clone(), request_id,
            display: Some(crate::models::TokenInputState {
                has_token: true, masked: None,
                source_i18n_key: Some("copilot.source.env_var"),
                edit_mode: crate::models::TokenEditMode::SetNew,
            }),
        });
        let snapshot = crate::application::settings_providers_tab_view_state(&state.borrow().session);
        assert!(matches!(snapshot.detail.token_input, crate::application::TokenInputDisplayState::Ready(ref display) if display.source_i18n_key == Some("copilot.source.env_var")));
        let _panel = super::providers::token_input_panel::render_token_input_panel(
            &id, capability, &snapshot.detail.token_input, view, &crate::theme::Theme::dark(), cx,
        );
        crate::application::reduce(&mut state.borrow_mut().session, AppAction::SaveProviderToken {
            provider_id: id.clone(), token: "test-local-override".to_string(),
        });
        let snapshot = crate::application::settings_providers_tab_view_state(&state.borrow().session);
        assert!(matches!(snapshot.detail.token_input, crate::application::TokenInputDisplayState::Ready(display) if display.has_token && display.edit_mode == crate::models::TokenEditMode::EditStored));
    });
}
