# Chronexus Server status

Updated: 26 October 2026. Source version 0.1.0, server schema 001, not published.

## Implemented through S2

- S1: Rust/axum service, independent SQLite WAL with full synchronous durability,
  transactional migration, health probes and fail-closed configuration. Docker
  Compose uses a non-root/read-only image, persistent volume, resource limits and
  configurable host/internal ports. The public origin is required; other settings
  have documented defaults.
- S2: one-use operator-issued account invitations, login/logout, Argon2id password
  hashing, hashed random tokens/codes, persistent IP/principal attempt limits,
  idle/absolute session expiry, own-device listing/revocation, all-device revocation
  and operator-assisted recovery that resets credentials and revokes old sessions.
- Code-generated account/session OpenAPI and TypeScript artifacts, with drift
  verification. The full campaign contract is deliberately not claimed implemented.
- Native HTTP compatibility with the original app client: meta, redeem, login and
  logout on a real loopback listener using synthetic accounts, no app profile access.

The application's current source records M23 as implemented and M24 as a sharing
client tested against a fake. Server delivery is application **M25**, Android
**M27**. Server roadmap S0–S11 keeps its own IDs. The earlier planning checkpoint's
M25 Android reference is superseded by this current application state.

## Verification actually executed

- macOS arm64, Rust 1.97: `cargo fmt --check`, Clippy all targets with warnings
  denied, 19 Rust integration tests and `cargo build --locked --release`: passed.
- `python3 scripts/contract.py --check`: generated OpenAPI/TypeScript match source.
  Generated schemas also passed `tsc --noEmit --strict api/types.ts`, using the
  existing desktop checkout's TypeScript compiler.
- `python3 scripts/check-desktop-auth.py --app-dir <Chronexus checkout>`: one
  original-client integration test passed; domain validators unrelated to auth are
  harness stubs. No GUI/keychain or complete sharing acceptance claimed.
- Linux arm64 Docker: image built; isolated `scripts/check-compose.py` passed
  health, non-root/read-only execution, alternate host/internal ports, HTTPS proxy
  header checks, account redemption, authentication, recreation persistence,
  hash-only storage, secret-free logs and logout. Synthetic containers and data
  volumes were removed. Public HTTPS/certificates were not exercised.
- Compose rejects a missing mandatory public origin; configuration with an explicit
  origin validates. Markdown local links/anchors and `git diff --check` verified.
- CI workflow added; remote CI has not run. Dependency license inventory recorded;
  vulnerability/image audits remain S7 work.

## Next milestone and limits

Next: **S3 campaign invitations and authorization**, then S4 selected content and
S5 continuous private local/server replication. No campaign data routes or campaign
storage exist yet; the app's campaign operations currently return 404.

The user-requested private online replica, optional server AI (S10) and web
management (S11) remain planned. The current desktop only has explicit sharing
operations, not continuous whole-campaign replication or server AI. Do not report
S1/S2 as a completed application M25 release.

S0 protocol choices are resolved: invitation-only registration, HTTPS,
server-readable v1 and manual conflict resolution. Real domain/proxy deployment,
production recovery/retention policies and license for the server code remain open;
see `docs/DECISIONS.md`. No Windows, Linux amd64, Rust 1.85, full browser UI,
production TLS or public deployment verification. Existing application files,
identifiers, schema 019 and user data were not changed.

Skill applied: `pages:write-page` for repository documentation and checkpoints.
The available skill catalog contains no dedicated Rust/server/database skill;
implementation used source inspection, upstream crate documentation and focused
executable tests. No production deployment, registry push or release performed.
