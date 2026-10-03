# Provider Detail UI

`providers/detail/` renders the right-hand detail panel in the Settings Providers tab.

## Boundary

- `mod.rs` owns the scroll container and section ordering.
- `header.rs` renders provider identity plus enable, refresh, and remove-from-sidebar actions.
- `info.rs` renders status/source/update/service-state cells.
- `usage.rs` renders quota usage and provider error/empty states（空态/失败走共享 detail card）。
- `quota_visibility.rs` renders per-quota tray visibility toggles.
- `settings_section.rs` renders provider settings capability branches, the per-provider
  usage-step （用量提醒） card, and the quota-thresholds （额度状态与提醒阈值） card.
  The usage-step card is gated on the `can_refresh` snapshot field
  (`ProviderStatus::supports_refresh()` = `ProviderCapability::Monitorable`) and renders regardless
  of which settings capability (token / NewAPI / script / none) the provider exposes —
  a `SettingsCapability::None` provider still gets the usage-step entry; non-monitorable providers
  never get it. Its state comes from the snapshot fields `quota_usage_step_pct` （provider override,
  `None` = follow global) and `global_quota_usage_step_pct`; the shared stepper
  (`quota_usage::render_quota_usage_stepper`) dispatches `SettingChange::SetProviderQuotaUsageStep`
  through `DetailActionDispatcher` — `−`/`+` create an override starting from the effective value,
  and「恢复跟随全局」dispatches `step_pct: None`.
  The quota-thresholds card is gated on `show_quota_thresholds`
  (`provider_capability == Monitorable`, likewise independent of `SettingsCapability`), so a
  monitorable provider with no interactive settings still gets per-unit threshold overrides, and
  thresholds can be pre-configured before any quota data arrives. Its three-unit rows come from the
  `quota_thresholds` snapshot entries (unit / `override_thresholds` / `effective`); editing reuses
  the shared `quota_thresholds::render_quota_thresholds_section` component and the view-local
  `QuotaThresholdDraft`, dispatching `SettingChange::SetProviderQuotaThresholds` on save.
- `actions.rs` owns the editable-provider edit/delete flow. It renders the same settings card as the
  Token panel (`shared::render_settings_card` + `render_action_button(.., ButtonSize::Panel, ..)`),
  so the settings section looks identical for token-input and editable providers. Add new card-level
  chrome to `providers/shared.rs`, not to one of the two callers.
  Between title and buttons the card shows one config summary row (`render_kv_info_row`) built from
  the capability payload — NewAPI's site URL, the script provider's interpreter — so the card states
  which config it manages instead of restating the buttons in prose. An empty payload hides the row.
  Descriptive prose is deliberately not shared card chrome: only the Token panel has a description,
  and this card shows a config value instead. Do not add a description helper to `shared.rs`.
  Confirm/cancel buttons come from `shared::render_confirm_cancel_buttons` directly; do not add a
  re-export shim here.

The module consumes `SettingsProviderDetailViewState` from `application/selectors`.
It must not rebuild provider business state, quota visibility decisions, or settings capability
rules inside GPUI rendering code. Confirmation flags for remove/delete flows also belong in this
view-state snapshot, not in live `SettingsView.state` reads from section renderers.

## Snapshot Contract

- Detail renderers read provider identity, info, usage, quota visibility, settings capability, and
  confirmation flags from `SettingsProviderDetailViewState`.
- New detail-panel state should first be derived in `application/selectors/settings.rs` and covered by
  selector tests before a renderer consumes it.
- Section renderers must not inspect `SettingsModalState`, provider store internals, or persisted
  settings directly.

## Interaction Rule

Use `DetailActionDispatcher` for actions that dispatch `AppAction` or need to clear token input before
switching modes. Section modules should receive this dispatcher instead of borrowing `SettingsView`
state directly, except for token input rendering and quota-threshold drafts where `SettingsView`
is required to create and reuse input entities.

## Boundary Check

Before changing this module, run:

```bash
rg -g '*.rs' "selected_provider_modal|settings_ui\\.modal|\\.state\\.borrow|\\.state\\.borrow_mut" src/ui/settings_window/providers/detail
```

The expected result is that section modules do not read live settings state. `mod.rs` may create
`DetailActionDispatcher`; token input rendering may still use `SettingsView` for GPUI entity
lifecycle, but business decisions should stay in the selector snapshot.
