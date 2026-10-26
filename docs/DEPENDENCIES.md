# Dependency choices and licenses

Reviewed on 26 October 2026 from resolved Cargo package metadata and upstream
crate documentation. Cargo.lock fixes the exact graph; no third-party service is
required at runtime. This is compatibility/license inventory, not a vulnerability
audit or a grant of a license for this repository's own code.

| Dependency | Locked version | Declared license | Purpose |
| --- | --- | --- | --- |
| `argon2` | 0.5.3 | MIT OR Apache-2.0 | Argon2id password PHC hashes |
| `axum` | 0.8.9 | MIT | Typed HTTP routing |
| `chrono` | 0.4.45 | MIT OR Apache-2.0 | UTC timestamps |
| `getrandom` | 0.4.3 | MIT OR Apache-2.0 | Operating-system random bytes |
| `http-body-util` | 0.1.5 | MIT | Test response collection |
| `ipnet` | 2.12.2 | MIT OR Apache-2.0 | Explicit trusted proxy networks |
| `rusqlite` | 0.38.0 | MIT | Parameterized SQLite with bundled engine |
| `serde` | 1.0.229 | MIT OR Apache-2.0 | Typed serialization |
| `serde_json` | 1.0.154 | MIT OR Apache-2.0 | JSON bodies and contract artifacts |
| `sha2` | 0.10.9 | MIT OR Apache-2.0 | Credential lookup hashes |
| `tempfile` | 3.27.0 | MIT OR Apache-2.0 | Isolated synthetic test storage |
| `tokio` | 1.54.1 | MIT | Bounded async runtime and blocking dispatch |
| `tower` | 0.5.3 | MIT | In-process HTTP integration tests |
| `url` | 2.5.8 | MIT OR Apache-2.0 | Strict public origin parsing |
| `utoipa` | 5.5.0 | MIT OR Apache-2.0 | Code-derived OpenAPI schemas |
| `uuid` | 1.26.1 | Apache-2.0 OR MIT | Stable account/session IDs |
| `zeroize` | 1.9.1 | Apache-2.0 OR MIT | Best-effort password/code buffer cleanup |

Axum 0.8 supports the selected Tokio/Tower HTTP stack. Argon2 0.5.3 and utoipa 5.5
use their documented APIs compatible with the manifest's Rust 1.85 floor; newer
major versions are not required for this slice. Rusqlite 0.38 matches the desktop's
selected family while keeping the server database completely separate. Compilation,
Clippy and tests provide executable compatibility evidence on the tested platforms.
No claim of execution on Rust 1.85 is made; development and Docker builds use 1.97.

Primary references: [axum](https://docs.rs/crate/axum/0.8.9),
[argon2](https://docs.rs/crate/argon2/0.5.3),
[rusqlite](https://docs.rs/crate/rusqlite/0.38.0),
[utoipa](https://docs.rs/crate/utoipa/5.5.0).
SQLite is bundled through libsqlite3-sys; inspect that package's notices when
preparing distribution. Review the complete transitive graph and advisories for S7.
Base Docker images are official Rust/Debian images pinned by content digest;
operating-system packages and image security need separate release review.
