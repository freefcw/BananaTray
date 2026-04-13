---
name: verify
description: Run full verification suite - cargo check, clippy, test, and fmt check. Use before committing or when unsure about code quality.
---

Run the complete verification suite for this Rust project:

1. `cargo check` - Fast compile check
2. `cargo clippy` - Lint checks
3. `cargo test` - Run tests
4. `cargo fmt --check` - Verify formatting

Report any failures clearly and suggest fixes.
