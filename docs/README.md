# Docs Index

This directory is split into two categories:

- **Current docs** — describe the current architecture, supported workflows, and active implementation guidance
- **Archive docs** — historical reviews, retrospectives, plans, and debugging notes kept only for traceability

## Suggested Reading Order

For a new developer:

1. `architecture.md` — overall system boundaries, state flow, and runtime/UI split
2. module `README.md` files under `src/` — module-level responsibilities and public structure
3. `providers.md` — provider model and extension points
4. `refresh-strategy.md` — refresh scheduling and event flow
5. `logging.md` — runtime diagnostics and log configuration

## By Task

- Architecture changes:
  Start with `architecture.md`, then check the relevant module `README.md` files under `src/`
- Runtime or UI work:
  Read `architecture.md`, `src/runtime/README.md`, and `src/ui/README.md`
- Adding or changing a provider:
  Read `providers.md`, `provider-blueprints.md`, and `antigravity-api.md` when working on Codeium-family providers
- Custom provider support:
  Read `custom-provider.md` and the YAML examples under `docs/examples/`
- Refresh behavior or polling issues:
  Read `refresh-strategy.md`
- Logging or diagnostics:
  Read `logging.md`
- Historical debugging context:
  Use `gpui-sigbus-bug.md`, `window-not-found-fix.md`, and `docs/archive/` only when current docs are insufficient

## Current Docs

- `architecture.md` — current system architecture and module boundaries
- `providers.md` — provider model, built-in/custom provider behavior, and extension guide
- `custom-provider.md` — current user guide for YAML custom providers
- `logging.md` — logging usage and environment variables
- `refresh-strategy.md` — refresh scheduling strategy
- `provider-blueprints.md` — current provider design patterns
- `antigravity-api.md` — current Codeium-family provider architecture notes
- `gpui-sigbus-bug.md` — important GPUI test/build constraint background still relevant to current codebase
- `window-not-found-fix.md` — historical bug record with follow-up notes; still useful for popup/settings lifecycle context

## Archive Docs

Historical material has been moved under `docs/archive/`.

These files may describe old paths, old module names, or intermediate refactor plans that no longer reflect the current codebase.

- `archive/code-review-solid-clean.md`
- `archive/roadmap-directory-restructure.md`
- `archive/custom-provider-plan.md`
- `archive/lessons-gpui-height-calibration.md`
- `archive/ui_layout_troubleshooting.md`
- `archive/app/analytics.md`
- `archive/provider/refactor.md`
- `provider/provider-refactor-retrospective.md` remains in place because it still has reference value for current provider boundaries

When in doubt, treat `architecture.md` and module `README.md` files as the source of truth.
