# Server development

Use Rust 1.97 (the verified toolchain), Python 3 for contract checks, and Docker
Compose for container tests. `Cargo.lock` pins dependencies. The manifest declares
Rust 1.88 as the dependency-compatible minimum, but that minimum toolchain has not
been executed. The CI workflow runs formatting, Clippy, tests, build and contract
checks; local results are recorded in [STATUS.md](STATUS.md).

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
python3 scripts/contract.py --check
```

Regenerate the reviewed contract after changing handlers or request/response types:

```sh
python3 scripts/contract.py
```

`api/openapi.json` and `api/types.ts` are intentional, versioned API artifacts.
The small TypeScript schema generator fails on unsupported schema shapes rather
than guessing. The schemas include implemented campaign/content/image APIs. Recursive blocks
use schema references, and binary images use an explicit binary schema.

## Isolated native development

Use a dedicated empty data directory, not a real Chronexus profile:

```sh
CHRONEXUS_PUBLIC_ORIGIN=http://127.0.0.1:8080 \
CHRONEXUS_ALLOW_LOOPBACK_HTTP=true \
cargo run --locked -- serve
```

The default database is `data/chronexus.sqlite3`, ignored by Git. Set
`CHRONEXUS_DATABASE` to another safe path if needed. `CHRONEXUS_LISTEN` defaults to
`127.0.0.1:8080`; changing it requires a matching public origin. Direct development
HTTP cannot bind a public interface. Operator commands `invite` and `recover USERNAME`
use the same database environment setting and do not require a running HTTP service.

## Compose acceptance

```sh
python3 scripts/check-compose.py
```

This builds an image and creates a uniquely named QA project, ephemeral host port
and synthetic data volume, then removes only that project and volume. It verifies
health, non-root/read-only execution, alternate internal/external ports, the proxy
gate, invite redemption, authentication, hash-only credential storage, recreation
persistence and logout. It needs the temporary test subnet `172.30.252.0/24` to be
unused. HTTPS headers simulate a trusted proxy; this is not a certificate/DNS test.

## Original desktop client compatibility

```sh
python3 scripts/check-desktop-auth.py --app-dir /path/to/Chronexus
```

This compiles the original `src-tauri/src/sync/api.rs` unchanged in a temporary test
crate and calls a real loopback server with a synthetic account. Only the unrelated
domain validators are harness stubs. It exercises `meta`, account/device access, campaign creation/invites/join, all
four legacy shared kinds, reader/author restrictions, conflicts and revocation. It neither edits the application nor opens its profile. It
requires its reqwest dependencies in the Cargo cache. This is HTTP client acceptance, not GUI/secure credential storage or continuous
replication acceptance.

## Implementation structure

- `src/config.rs`: validated origin, listener and trusted proxies.
- `src/db.rs`, `migrations/`: independent durable store and transactional schema.
- `src/auth.rs`: credential validation, atomic invitation/recovery, session lifecycle,
  persistent limits and bounded storage.
- `src/routes.rs`: narrow handlers, transport security and generated OpenAPI.
- `src/lib.rs`: four-worker bound around blocking storage/password operations.
- `src/main.rs`: serving, explicit operator commands and health probe.
- `src/campaigns.rs`, `src/campaign_routes.rs`: invitations, bounded membership and
  versioned role administration.
- `src/content.rs`, `src/content_routes.rs`, `src/payload.rs`: typed private/public
  projections, dependencies, revisions and bounded legacy feed.
- `src/attachments.rs`: validated original image bytes, checksums, quotas and access.

No access logger is enabled. Keep secrets out of process arguments, commit bodies,
fixtures and logs. Read [AGENTS.md](../AGENTS.md) before contributing.
