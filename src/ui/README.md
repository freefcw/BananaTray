# src/ui/

GPUI-dependent UI module. Contains concrete view types, rendering logic, widgets, and view-local state. This module consumes `runtime::AppState` but does not own reducer/effect execution.

> **Build constraint**: This module is behind `cfg(feature = "app")` in `lib.rs`. GPUI proc macros crash during test compilation, so pure logic is extracted to `src/application/state.rs`, `src/application/` (including `selectors/format.rs`), and `models/`.

## Files

- `mod.rs` — exports `AppView`; shared state now comes from `runtime::AppState`

## Responsibilities

- render tray popup and settings window views
- keep GPUI-only state local to views
- translate user interaction into `AppAction`
- register UI hooks into `runtime` during bootstrap

## Boundaries

- may depend on `gpui`
- should read/write shared app state only through `runtime::AppState`
- should not own settings persistence, refresh scheduling, or reducer execution
- should keep concrete view handles and weak refs inside `ui`, not inside `AppState`

### `mod.rs` — Module exports

- **`AppView`** — re-exported from `views/app_view.rs`

### `views/` — GPUI View Components

- `app_view.rs` — **`AppView`** GPUI view struct implementing `Render`. Renders the tray popup with top navigation bar, content area, and global action footer.
- `nav.rs` — Tab-style navigation bar. Provider order follows `AppSettings::ordered_providers()`. Overview pill inserted first when enabled.
- `overview_panel.rs` — Overview panel: compact provider cards showing all enabled providers' quota status at a glance. Click-through to provider detail.
- `provider_panel.rs` — Provider detail view: header, quota bars, status indicators, error messages.
- `tray_settings.rs` — Inline settings content rendered inside the tray popup (overview toggle, auto-hide, account info).

### `settings_window/` — Full Settings Window

Separate desktop window with tabbed settings UI:
- `mod.rs` — window shell, tab routing, `SettingsView`, and runtime hook registration
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
  └─ runtime::AppState (Rc<RefCell<...>>)
       ├─ View reads `state.session` or selector output during render
       ├─ User / background event → `runtime::dispatch_*()`
       │   ├─ `application::reduce(&mut state.session, action)`
       │   └─ execute `AppEffect` in GPUI / App context
       └─ RefreshCoordinator event → `AppAction::RefreshEventReceived` → reducer

bootstrap_ui()
  └─ ui::settings_window::register_runtime_hooks()
       ├─ runtime requests popup rerender through UI hook
       ├─ runtime requests popup-view cleanup through UI hook
       └─ runtime requests settings-window view construction through UI hook
```

## Constraints

- All files in this module may import from `gpui`. Test-sensitive logic must be in `src/application/state.rs`, `src/application/`, or `models/`.
- `runtime::AppState` is wrapped in `Rc<RefCell<...>>` (single-threaded, GPUI is !Send).
- Window sizing uses `PopupLayout` constants from `models/layout.rs`.
- Icon paths are relative to the asset root (e.g. `"src/icons/settings.svg"`).
- `use gpui::*;` is forbidden in `src/`. CI enforces this via `scripts/check-gpui-imports.sh`.

## GPUI Import Rules

- Prefer explicit type/function imports such as `use gpui::{div, px, App, Window};`.
- Import GPUI extension traits explicitly when method chains require them. Common ones are `Styled`, `ParentElement`, `InteractiveElement`, `StatefulInteractiveElement`, `IntoElement`, `AnimationExt`, and `AppContext`.
- Keep `gpui::prelude::FluentBuilder as _` only where builder helpers are actually used.
- If a file uses `id()` early and becomes `Stateful<Div>`, expect to need stateful traits like `StatefulInteractiveElement` or animation traits.
- When a hover or animation closure stops inferring, prefer adding the concrete GPUI type import, for example `StyleRefinement`, instead of widening imports.
