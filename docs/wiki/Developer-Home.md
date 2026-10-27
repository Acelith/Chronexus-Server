# Developer home

Rust (axum) service with one SQLite database, shipped as a Docker image. The desktop
client lives in the [Chronexus repository](https://github.com/acelith/Chronexus); this
repository owns the server and its versioned API.

| Page | Contents |
| --- | --- |
| [Architecture](Architecture.md) | Modules, request flow, database |
| [API contract](API-Contract.md) | OpenAPI, TypeScript types, compatibility |
| [Testing](Testing.md) | Test suites and how to run them |
| [Milestones](Milestones.md) | M25.0–M25.11 and their state |

Canonical files: `ROADMAP.md`, `AGENTS.md`, `docs/STATUS.md`, `docs/DECISIONS.md`,
`docs/API.md`, `docs/INSTALLATION.md`, `docs/DEVELOPMENT.md`.

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
python3 scripts/contract.py --check
```
