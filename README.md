# Banana Tray

A cross platform AI provider usage monitor, written in Rust with GPUI.

- Tray usage details open in a compact popover.
- Settings open in a separate desktop window so configuration is not constrained by tray panel size.
- App settings are persisted to `~/Library/Application Support/BananaTray/settings.json` on macOS.
- Runtime logs use `log` with the `env_logger` backend.
- Log format is: `timestamp level target message`
- By default logs go to stderr.
- To write logs into the current directory, run: `BANANATRAY_LOG_FILE=1 RUST_LOG=info cargo run`
- When file logging is enabled, logs are appended to `./banana.log`
