# src/app/

GPUI-dependent UI module. Contains all view rendering, window management, and user interaction logic.

> **Build constraint**: This module is behind `cfg(feature = "app")` in `lib.rs`. GPUI proc macros crash during test compilation, so pure logic is extracted to `app_state.rs`, `app/provider_logic.rs`, and `models/`.

## Files

### `mod.rs` — AppState + AppView

- **`AppState`** — persistent application state (outlives windows). Composition container:
  - `provider_store: ProviderStore` — provider status list
  - `nav: NavigationState` — active tab + last provider
  - `settings_ui: SettingsUiState` — settings window state
  - `settings: AppSettings` — user preferences
  - `refresh_tx: Sender<RefreshRequest>` — channel to RefreshCoordinator
  - `alert_tracker: QuotaAlertTracker` — quota notification state machine
  - `view_entity: Option<WeakEntity<AppView>>` — weak ref for UI updates
  - Key methods: `apply_refresh_event()`, `sync_config_to_coordinator()`, `select_cadence()`, `save_settings()`
- **`AppView`** — GPUI view struct implementing `Render`. Renders the tray popup with:
  - Top navigation bar (provider tabs + settings/close buttons)
  - Content area (provider detail panel or settings)
  - Global action footer (dashboard + refresh buttons)
- **`compute_popup_height()`** — dynamically sizes window based on active provider's quota count
- **`schedule_open_settings_window()`** — re-exported from `settings_window`

### `nav.rs` — Navigation Bar

Renders the tab-style navigation bar at the top of the tray popup. Each enabled provider gets a tab with its icon. Provider order follows `AppSettings::ordered_providers()`.

### `provider_panel.rs` — Provider Detail View

Renders the main content area when a provider tab is selected: header with provider name/account, quota bars, status indicators, error messages.

### `provider_logic.rs` — Pure Display Logic

**No GPUI dependency.** Extracted formatting functions for testability:
- `format_amount()` / `format_quota_usage()` — number formatting
- `provider_empty_message()` — contextual empty state messages
- `provider_account_label()` — account display text
- `provider_list_subtitle()` / `provider_detail_subtitle()` — status-aware subtitles

### `tray_settings.rs` — Inline Settings Panel

Simple settings content rendered inside the tray popup (as opposed to the full settings window).

### `settings_window/` — Full Settings Window

Separate desktop window with tabbed settings UI:
- `mod.rs` — window creation and tab routing
- `window_mgr.rs` — window lifecycle management (open/close/focus)
- `general_tab.rs` — theme, refresh cadence, auto-hide, launch-at-login
- `provider_sidebar.rs` — provider list with enable/disable toggles and reorder
- `provider_detail.rs` — per-provider detail view (status, quotas, account info)

### `widgets/` — Reusable UI Components

Small GPUI components used across views:
- `quota_bar.rs` — progress bar with percentage and color coding
- `toggle.rs` — iOS-style toggle switch
- `checkbox.rs` — checkbox with label
- `cadence_dropdown.rs` — refresh interval dropdown
- `tab.rs` — navigation tab button
- `card.rs` — card container
- `icon.rs` — SVG icon renderer (`render_svg_icon()`)
- `tooltip.rs` — tooltip component

## Data Flow

```
TrayController (main.rs)
  └─ AppState (Rc<RefCell<...>>)
       ├─ AppView reads state on render
       ├─ RefreshCoordinator sends events → AppState.apply_refresh_event()
       │   → updates provider_store + triggers cx.notify() via view_entity
       └─ User actions → AppState mutations → send RefreshRequest
```

## Constraints

- All files in this module may import from `gpui`. Test-sensitive logic must be in `provider_logic.rs` or `app_state.rs`.
- `AppState` is wrapped in `Rc<RefCell<...>>` (single-threaded, GPUI is !Send).
- Window sizing uses `PopupLayout` constants from `models/layout.rs`.
- Icon paths are relative to the asset root (e.g. `"src/icons/settings.svg"`).
