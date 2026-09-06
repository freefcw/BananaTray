# src/icons/

SVG icon assets used by the GPUI UI layer.

## Naming Convention

- **Provider icons**: `provider-{name}.svg` (e.g. `provider-claude.svg`, `provider-copilot.svg`)
  - The `{name}` matches the lowercase provider identifier
  - Referenced in `ProviderMetadata.icon_asset` as `"src/icons/provider-{name}.svg"`
- **UI icons**: descriptive name (e.g. `settings.svg`, `refresh.svg`, `close.svg`)
- **Tray icon**: `tray_icon.svg` (the system tray icon; the PNG version `tray_icon.png` lives in `src/tray/`)
- **App logo PNG**: `app_logo.png` (the only PNG bundled here; rendered via GPUI `img()` in About / popup header)

> ⚠️ **不要在本目录堆放设计资料 PNG**。打包脚本通过 `cp src/icons/*.png` 通配复制，
> 任何放进来的 PNG 都会被塞进 macOS .app / Linux deb/rpm/AppImage。
> 仅供参考的设计稿请放在 [`docs/design-references/`](../../docs/design-references/)。

## Provider Icon Contract

Provider icons are monochrome brand marks rendered through `currentColor`. Geometry is
normalized in the asset instead of adding provider-specific scaling to UI code.

### Structural rules

- Root element: `width="24" height="24" viewBox="0 0 24 24" fill="none"`.
- Visible paint uses only `currentColor`; hardcoded colors are forbidden so light, dark,
  and muted themes remain valid.
- Every graphical element declares `fill` or `stroke` explicitly.
- Outline icons normally use `stroke-width="1.8"`; the accepted range is `1.5`-`2.0`.
- Keep SVGs self-contained: no scripts, external resources, embedded images, fonts,
  filters, CSS, or event handlers.
- Use compound paths for transparent cutouts instead of painting with an assumed
  background color.

### Optical rules

- Outline or mixed marks normally occupy `16`-`18` units on their longest edge.
- Filled marks normally occupy `16`-`17` units because their optical weight is higher.
- Wide brand marks may use `18`-`20` units of width when their natural aspect ratio
  requires it.
- Keep painted geometry at least about `2` units from the canvas edge and center the
  optical weight near `(12, 12)`. Brand asymmetry takes precedence over mechanical
  centering.
- The deciding minimum size is `15px`, matching the provider navigation UI.

The generated [provider icon review sheet](../../docs/design-references/provider-icons.svg)
shows every asset at `15px`, `16px`, `20px`, `32px`, and `32px` inside the settings header
container across light, dark, and muted colors. Open it at 100% scale when judging the
smallest sizes. The sheet is a fast comparison aid, not a substitute for checking GPUI's
actual rendering in the app.

`just audit-provider-icons` renders every asset with the same `resvg` version used by GPUI
and reports its visible bounds, size, center offset, and ink area. Empty or clipped icons
fail the check; unusual margins, dimensions, or centers are warnings that require visual
review rather than automatic rejection.

## Current Icons

### Provider Icons
`provider-amp.svg`, `provider-antigravity.svg`, `provider-claude.svg`, `provider-cline-pass.svg`, `provider-codex.svg`, `provider-copilot.svg`, `provider-cursor.svg`, `provider-custom.svg`, `provider-gemini.svg`, `provider-grok.svg`, `provider-kilo.svg`, `provider-kimi.svg`, `provider-kiro.svg`, `provider-minimax.svg`, `provider-opencode.svg`, `provider-unknown.svg`, `provider-vertexai.svg`, `provider-devin-desktop.svg`

### UI Icons
`about.svg`, `advanced.svg`, `chevron-left.svg`, `chevron-right.svg`, `close.svg`, `compass.svg`, `display.svg`, `drag-handle.svg`, `overview.svg`, `plus.svg`, `quit.svg`, `refresh.svg`, `settings.svg`, `status.svg`, `switch.svg`, `trash.svg`, `usage.svg`

### Legacy & Deprecated Icons
- `provider-windsurf.svg` (removed) — retired during the Windsurf → Devin rename. The UI now uses `provider-devin-desktop.svg`; the built-in `Windsurf` key (`ProviderKind::Windsurf` / `"windsurf"`) remains as the compatibility stable key.

## Usage in Code

Icons are loaded through GPUI's `AssetSource` (see `src/platform/assets.rs`). Rendered via:

```rust
crate::ui::widgets::render_svg_icon("src/icons/settings.svg", px(15.0), color)
```

The path is relative to the asset root, which resolves to:
1. `BANANATRAY_RESOURCES` env var (AppImage)
2. `.app/Contents/Resources/` (macOS bundle)
3. `/usr/share/bananatray` (Linux deb)
4. `CARGO_MANIFEST_DIR` (development)

## Adding a New Icon

1. Place the SVG file in this directory following the naming convention and provider icon contract.
2. For provider icons, reference it in `ProviderMetadata.icon_asset` as `"src/icons/provider-{name}.svg"`.
3. Run `just render-provider-icons` to regenerate the visual review sheet.
4. Run `just audit-provider-icons` and investigate every warning instead of mechanically suppressing it.
5. Review the sheet at 100% scale, using `15px` as the deciding comparison size.
6. Run the app and inspect the actual GPUI rendering in provider navigation, overview, and the settings detail header; check both light and dark appearances.
7. Run `just check-provider-icons` before committing.
8. For UI icons, use `render_svg_icon()` with the full relative path.
