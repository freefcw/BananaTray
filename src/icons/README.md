# src/icons/

SVG icon assets used by the GPUI UI layer.

## Naming Convention

- **Provider icons**: `provider-{name}.svg` (e.g. `provider-claude.svg`, `provider-copilot.svg`)
  - The `{name}` matches the lowercase provider identifier
  - Referenced in `ProviderMetadata.icon_asset` as `"src/icons/provider-{name}.svg"`
- **UI icons**: descriptive name (e.g. `settings.svg`, `refresh.svg`, `close.svg`)
- **Tray icon**: `tray_icon.svg` (the system tray icon; the PNG version `tray_icon.png` lives in `src/`)

## Current Icons

### Provider Icons
`provider-amp.svg`, `provider-antigravity.svg`, `provider-claude.svg`, `provider-codex.svg`, `provider-copilot.svg`, `provider-cursor.svg`, `provider-gemini.svg`, `provider-kilo.svg`, `provider-kimi.svg`, `provider-kiro.svg`, `provider-minimax.svg`, `provider-opencode.svg`, `provider-vertexai.svg`, `provider-windsurf.svg`

### UI Icons
`about.svg`, `advanced.svg`, `chevron-left.svg`, `chevron-right.svg`, `close.svg`, `compass.svg`, `display.svg`, `drag-handle.svg`, `overview.svg`, `plus.svg`, `quit.svg`, `refresh.svg`, `settings.svg`, `status.svg`, `switch.svg`, `trash.svg`, `usage.svg`

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

1. Place the SVG file in this directory following the naming convention
2. For provider icons: reference in `ProviderMetadata.icon_asset` as `"src/icons/provider-{name}.svg"`
3. For UI icons: use `render_svg_icon()` with the full relative path
