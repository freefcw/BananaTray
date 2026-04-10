# src/ui/

GPUI-dependent UI module. Contains all view rendering, window management, and user interaction logic.

> **Build constraint**: This module is behind `cfg(feature = "app")` in `lib.rs`. GPUI proc macros crash during test compilation, so pure logic is extracted to `src/app_state.rs`, `src/application/` (including `selectors/format.rs`), and `models/`.

## Files

### `bridge.rs` — Runtime State Wrapper

- **`AppState`** — runtime composition container:
  - `session: AppSession` — pure session state from `src/app_state.rs`
  - `refresh_tx: Sender<RefreshRequest>` — channel to `RefreshCoordinator`
  - `view_entity: Option<WeakEntity<AppView>>` — weak ref for UI updates
  - `log_path: Option<PathBuf>` — log file path for Debug tab
- **`persist_settings()`** — persists `AppSettings`

### `mod.rs` — Module exports

- **`AppView`** — re-exported from `views/app_view.rs`
- **`schedule_open_settings_window()`** — re-exported from `settings_window`

### `views/` — GPUI View Components

- `app_view.rs` — **`AppView`** GPUI view struct implementing `Render`. Renders the tray popup with top navigation bar, content area, and global action footer.
- `nav.rs` — Tab-style navigation bar. Provider order follows `AppSettings::ordered_providers()`.
- `provider_panel.rs` — Provider detail view: header, quota bars, status indicators, error messages.
- `tray_settings.rs` — Inline settings content rendered inside the tray popup.

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
TrayController (tray/controller.rs)
  └─ AppState (Rc<RefCell<...>>)
       ├─ View reads `state.session` or selector output during render
       ├─ User / background event → `runtime::dispatch_*()`
       │   ├─ `application::reduce(&mut state.session, action)`
       │   └─ execute `AppEffect` in GPUI / App context
       └─ RefreshCoordinator event → `AppAction::RefreshEventReceived` → reducer
```

## Constraints

- All files in this module may import from `gpui`. Test-sensitive logic must be in `src/app_state.rs`, `src/application/`, or `models/`.
- `AppState` is wrapped in `Rc<RefCell<...>>` (single-threaded, GPUI is !Send).
- Window sizing uses `PopupLayout` constants from `models/layout.rs`.
- Icon paths are relative to the asset root (e.g. `"src/icons/settings.svg"`).
