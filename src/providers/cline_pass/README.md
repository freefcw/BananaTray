# ClinePass Provider

`ClinePassProvider` monitors the three Cline plan limits exposed by the dashboard usage API.

## Data Contract

- Endpoint: `GET https://api.cline.bot/api/v1/users/me/plan/usage-limits`
- Authentication: `Authorization: Bearer <token>`
- `data.limits[].type` maps in canonical display order:
  - `five_hour` -> `QuotaType::Session`
  - `weekly` -> `QuotaType::Weekly`
  - `monthly` -> `QuotaType::Monthly`
- `percentUsed` is required for every known limit. Unknown types are ignored.
- `resetsAt` is parsed when present; BananaTray does not calculate reset times locally.

This implementation tracks Cline's dashboard endpoint and its upstream-owned response shape. Keep the parser fixtures in sync if Cline changes that contract.

## Credential Resolution

Refresh resolves credentials on every call in this order:

1. BananaTray-managed `cline_api_key` from `ProviderExecutionContext`
2. `CLINE_API_KEY`
3. API key from Cline `providers.json`
4. Unexpired OAuth access token from Cline `providers.json`

The settings file path follows Cline's own precedence:

1. `CLINE_PROVIDER_SETTINGS_PATH`
2. `$CLINE_DATA_DIR/settings/providers.json`
3. `$CLINE_DIR/data/settings/providers.json`
4. `~/.cline/data/settings/providers.json`

Current credentials live under `providers.cline.settings`; `providers.cline-pass` is read only when the current shared entry is absent, for legacy compatibility. API keys may be stored at `apiKey` or `auth.apiKey`. OAuth uses `auth.accessToken`, normalized to Cline's `workos:` form, and an epoch-millisecond `auth.expiresAt`.

When `auth.expiresAt` is absent or invalid, BananaTray falls back to the JWT `exp` claim. Tokens with unknown expiry, expired tokens, and tokens expiring within the HTTP client's 20-second timeout window return `SessionExpired` with `FailureAdvice::OpenAppToRefresh` (`app = "Cline"`), telling users to open Cline (VS Code extension or Cline CLI — both refresh `providers.json` on use) or configure an API key override in BananaTray settings.

BananaTray only reads Cline-owned credentials. It deliberately does not refresh OAuth or write `providers.json`, because refresh-token rotation by two processes can invalidate Cline's own session.

BananaTray-managed credentials and `CLINE_API_KEY` short-circuit local file access. When local discovery is needed, a missing file behaves like no configured credential and eventually returns `ConfigMissing`; malformed JSON returns `ParseFailed`; other read errors return `Unavailable`.

## Module Boundaries

- `auth.rs`: credential precedence, path resolution, local JSON parsing, masking
- `client.rs`: endpoint and Bearer-authenticated HTTP GET
- `parser.rs`: response decoding and stable quota mapping
- `mod.rs`: provider metadata, availability, refresh orchestration, settings capability
