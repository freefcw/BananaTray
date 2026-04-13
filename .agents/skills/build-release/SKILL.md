---
name: build-release
description: Build optimized release binary. Use when preparing a distribution build.
disable-model-invocation: true
---

Build the release version of BananaTray:

1. `cargo build --release`

Report the output binary location (typically `target/release/bananatray` on macOS/Linux or `target/release/bananatray.exe` on Windows).

Note: For macOS apps, the release binary may need additional packaging steps.
