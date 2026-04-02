# src/app/

GPUI-dependent UI module. Contains all view rendering, window management, and user interaction logic.

> **Build constraint**: This module is behind `cfg(feature = "app")` in `lib.rs`. GPUI proc macros crash during test compilation, so pure logic is extracted to `src/app_state.rs`, `src/application/`, `app/provider_logic.rs`, and `models/`.

## Files

### `app_state.rs` — Runtime State Wrapper

- **`AppState`** — runtime composition container:
  - `session: AppSession` — pure session state from `src/app_state.rs`
  - `refresh_tx: Sender<RefreshRequest>` — channel to `RefreshCoordinator`
  - `view_entity: Option<WeakEntity<AppView>>` — weak ref for UI updates
- **`persist_settings()`** — persists `AppSettings`

### `mod.rs` — AppView + module exports

- **`AppView`** — GPUI view struct implementing `Render`. Renders the tray popup with:
  - Top navigation bar (provider tabs + settings/close buttons)
  - Content area (provider detail panel or settings)
  - Global action footer (dashboard + refresh buttons)
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
- `provider_list_subtitle()` — status-aware list subtitle

### `tray_settings.rs` — Inline Settings Panel

Simple settings content rendered inside the tray popup (as opposed to the full settings window).

### `settings_window/` — Full Settings Window

Separate desktop window with tabbed settings UI:
- `mod.rs` — window shell and tab routing
- `window_mgr.rs` — window lifecycle management (open/close/focus)
- `general_tab.rs` — theme, refresh cadence, auto-hide, launch-at-login
- `providers/` — provider sidebar + detail panel
- `display_tab.rs` / `debug_tab.rs` / `about_tab.rs` — remaining settings tabs

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
       ├─ View reads `state.session` or selector output during render
       ├─ User / background event → `runtime::dispatch_*()`
       │   ├─ `application::reduce(&mut state.session, action)`
       │   └─ execute `AppEffect` in GPUI / App context
       └─ RefreshCoordinator event → `AppAction::ApplyRefreshEvent` → reducer
```

## Constraints

- All files in this module may import from `gpui`. Test-sensitive logic must be in `src/app_state.rs`, `src/application/`, or `provider_logic.rs`.
- `AppState` is wrapped in `Rc<RefCell<...>>` (single-threaded, GPUI is !Send).
- Window sizing uses `PopupLayout` constants from `models/layout.rs`.
- Icon paths are relative to the asset root (e.g. `"src/icons/settings.svg"`).
