# Provider Detail UI

`providers/detail/` renders the right-hand detail panel in the Settings Providers tab.

## Boundary

- `mod.rs` owns the scroll container and routes between the two sub-tabs (`ProviderDetailTab::Settings` vs `ProviderDetailTab::UsageHistory`) for monitorable providers. The scroll element id is keyed by sub-tab so GPUI does not reuse the previous tab's `scroll_offset`.
- `header.rs` renders provider identity (with the inline status/source/updated summary from `info.rs`), remove-from-sidebar, refresh, and enable actions (with the primary enable toggle on the far right), plus the `[ 配置 | 用量与历史 ]` sub-tab bar when `can_refresh` is true.
- `info.rs` renders the compact single-line status summary below the provider title (a status-colored dot + source + updated/error status, or disabled state).
- `usage.rs` renders quota usage and provider error/empty states in the `UsageHistory` sub-tab（空态/失败走共享 detail card）。
- `history.rs` renders the quota history card in the `UsageHistory` sub-tab (range/interval toolbar, stacked single-unit or merged multi-series step charts with colored dot legends, and a footer combining notes with the clear-history ghost button).
- `quota_visibility.rs` renders per-quota tray visibility toggles plus the overview
  primary-quota star (≥2 visible quotas only; toggling dispatches
  `SettingChange::TogglePrimaryQuota`) inside a unified settings card (`shared::render_settings_card` + `render_settings_card_header`).
- `settings_section.rs` renders the `Settings` sub-tab content:
  1. Provider settings capability card (`TokenInput` / `NewApiEditable` / `ScriptEditable`); for `SettingsCapability::None` on a monitorable provider, no redundant placeholder box is rendered, while non-monitorable (`Informational` / `Placeholder`) providers render a single explanatory card.
  2. The `quota_visibility` card (when `show_quota_visibility` is true).
  3. The unified `render_rules_card` combining the per-provider usage-step row (`quota_usage::render_provider_quota_usage_row`) and per-unit quota-thresholds rows (`quota_thresholds::append_provider_threshold_units`). Uncustomized threshold units in provider view collapse to a single-line summary (`● notify ● critical ● warning [Customize]`) and expand into the 4-segment strip/editor when customized or being edited.
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
- Token display consumes `detail.token_input` (Loading / Ready / Unavailable), not live manager or credential lookups. Unknown external status is not a missing-token result; input drafts remain view-local.
- New detail-panel state should first be derived in `application/selectors/settings.rs` and covered by
  selector tests before a renderer consumes it.
- Section renderers must not inspect `SettingsModalState`, provider store internals, or persisted
  settings directly.

## Interaction Rule

Use `DetailActionDispatcher` for actions that dispatch `AppAction` or need to clear view-local drafts
(`TokenInputDraft` and `QuotaThresholdDraft` via `clear_drafts`) before switching modes. Section
modules should receive this dispatcher instead of borrowing `SettingsView` state directly, except
for token input rendering and quota-threshold drafts where `SettingsView` is required to create and
reuse input entities.
Active sub-tab pills do not dispatch `SetProviderDetailTab`: the reducer no-ops the same tab without
`Render`, so a re-click must not clear view-local drafts. Switching to the other sub-tab still goes
through `interactive_cleanup_action`. Sidebar selection, Open Settings with a provider, adding a
provider to the sidebar, and removing a sidebar provider that leaves another selected all write
`provider_detail_tab` back to `Settings`.

## Boundary Check

Before changing this module, run:

```bash
rg -g '*.rs' "selected_provider_modal|settings_ui\\.modal|\\.state\\.borrow|\\.state\\.borrow_mut" src/ui/settings_window/providers/detail
```

The expected result is that section modules do not read live settings state. `mod.rs` may create
`DetailActionDispatcher`; token input rendering may still use `SettingsView` for GPUI entity
lifecycle, but business decisions should stay in the selector snapshot.
