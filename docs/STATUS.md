# Chronexus Server status

Updated: 26 October 2026. Source 0.1.0, server schema 006. Not published.

## Implemented server milestones M25.1 through M25.4

- M25.1/M25.2: Rust/axum, independent SQLite WAL/full durability, transactional migrations,
  health/configuration/proxy gates, minimal Compose with safe defaults and adjustable
  ports; invitation-only accounts, Argon2id, hashed secrets, persistent attempt
  limits, session expiry/device revocation and operator-assisted recovery.
- M25.3: campaign creation/listing, owner/player/reader roles, one-use campaign invites
  for new/existing accounts, owner-only invitation/role administration and member
  removal/self-leave. Monotonic membership revisions reject stale leave/rejoin actions.
- M25.4: typed private-by-default content and explicit member projections for pages,
  sessions, relationships, party, selected sheet fields and images. Only authors
  edit content; owners may withdraw published projections but cannot read another
  author's private originals. References never implicitly publish their targets.
- Legacy four-kind sharing now works with the original desktop HTTP client, including
  revision conflicts and bounded per-campaign change feeds. Withdrawal preserves
  private originals, emits payload-free public tombstones and withdraws dependent
  public relationships/party entries. Member removal withdraws authored projections
  atomically; rejoin does not republish them.
- Images: private atomic SQLite BLOBs with UUID/campaign/author ownership, SHA-256,
  byte-preserving PNG/JPEG/WebP validation, strict dimensions/output bounds and
  bounded decode/transfer concurrency. Public downloads require published references;
  referenced originals cannot be deleted accidentally. Quotas bound stored content.
- Generated OpenAPI/TypeScript describe implemented APIs, recursive note blocks and
  binary image MIME types. Server archive identity is checked before journal changes
  or migrations, preserving schemas 001/002 and refusing foreign/desktop archives.

## Licence and distribution setup (not executed: Docker unavailable)

Apache-2.0 licence added. `compose.yaml` now derives the public origin from `CHRONEXUS_DOMAIN`,
accepts `CHRONEXUS_IMAGE`, and offers a `https` profile with a bundled Caddy (`deploy/Caddyfile`,
automatic certificates, fixed trusted address). The compose and Caddy files were reviewed by hand
but never run; verify with `docker compose config` and a real start before relying on them.

## M25.7 security and recovery implemented (Windows)

- `tests/security.rs` (6 tests): role matrix over every campaign route and five principals, credential
  header and scheme abuse, hostile identifiers, malformed/oversized/nesting-bomb bodies, hostile
  uploads, error leakage and login indistinguishability. It found one real defect (a refusal that
  depended on the request body), now fixed.
- `tests/recovery.rs` (3 tests): `backup`/`verify`/`restore` (consistent copy while running,
  checksum verification of every image, corruption/foreign/newer/tampered files refused with the live
  database untouched, restore on a clean instance with sessions revoked and removals intact),
  automatic `.pre-upgrade-v<N>` copy, failed-upgrade rollback and newer-schema refusal. The CLI was
  also exercised as a real process.
- Operator guide: TLS/proxy examples, secrets, volumes, monitoring, backup/restore with the
  encryption/key policy, upgrade and rollback (docs/INSTALLATION.md). Evidence: all 71 integration
  tests, rustfmt and Clippy `-D warnings` pass on Windows x64.
- Not done: Docker/Linux amd64 execution, `cargo audit`/image scans (not available offline), remote CI,
  third-party penetration test, load testing, real TLS. Hosting and licence decisions remain open.
- Next: **M25.8**, the full M24 acceptance with 2-3 users against a deployment and the first release.

## M25.6 lifecycle implemented (schema 006)

- Campaign deletion (owner, exact name), erasing one's own server copy, account deletion
  (password, explicit flag for shared owned campaigns, anonymous stub), former members told
  "deleted" (404), operator `revoke-sessions` and `compact [DAYS]`. Revocation, rejoin, stale
  writes, purged tombstones with stale cursors and a restored backup are covered by 6 new tests
  in `tests/lifecycle.rs`; all 62 integration tests, rustfmt, Clippy `-D warnings` and the
  contract drift check pass on Windows x64 (Rust 1.99.1). Not run: Docker/Linux/macOS.
- Limits (see docs/API.md): exported or offline copies cannot be recalled; backups keep
  deleted data until the operator drops them; no ownership transfer.

## M25.5 implemented (server contract; desktop client in the application repository)

- Schema 005: payload-free change log, `/sync` incremental reads of the caller's current
  authorized state with tombstones, bounded pages (500 log rows / 6 MiB), cursor floor with
  410 on stale or future cursors, and `head` on the full content listing for resync.
- UUIDv7 operation IDs with receipts for saves, deletions, sharing withdrawals and the
  legacy routes (`Idempotency-Key`); 30-day retry horizon, 31-day receipt retention,
  opportunistic compaction, 100,000-receipt cap; content-idempotent attachment uploads.
- `GET .../events` SSE: content-free hints, author-only for private edits, 3 streams per
  account / 64 total, 10-minute lifetime, 25 s keepalive with re-authorization.
- Evidence (Windows x64, Rust 1.99.1): 56 integration tests (7 new in `tests/sync.rs`
  covering authorization of feeds/tombstones, 505-item paging, compaction/future cursors,
  keyed delete/unshare/legacy retries, ID age window and receipt compaction, stream
  scoping/limits); rustfmt, Clippy `-D warnings` and contract generation/drift checks pass.
  Not run: Docker/Linux/macOS, minimum Rust, original-desktop harness, real network latency.
- Desktop integration (application repository, commit 7bf720e): the real desktop client ran
  against this server's debug build over loopback HTTP. Two computers of one account stayed
  aligned with autosave-to-other-computer latency of median ~160 ms (max 488 ms) over 8 edits
  with a 60 s poll, so notification-driven; a server kill and restart on the same database was
  recovered automatically; a stored operation was replayed once; conflicts, sharing,
  withdrawal, 120-item paging and member isolation were verified.
- Not done: a membership-removal push hint (polling covers it, the stream closes at its next
  keepalive check), production TLS/DNS, Linux/macOS, long soak.

## Earlier M25.4 checks and evidence

macOS arm64, Rust 1.97:

- Rust integration tests: **42 passed** (19 auth, 7 campaigns, 16 content/images).
- Formatting, Clippy all targets with warnings denied, locked release build: passed.
- OpenAPI/TypeScript generation and drift checks, strict TypeScript schema check:
  passed. Markdown links/anchors, Python syntax and diff whitespace checked.
- Original desktop `sync/api.rs`, compiled unchanged in a temporary harness:
  meta, redeem/login/logout, campaign creation, campaign invitation signup/join,
  pages/sessions/relationships/party, reader/non-author refusals, 409 conflicts and
  revocation passed against a real local listener. Unrelated domain validators
  remain harness stubs; this is HTTP acceptance, not a GUI/profile test.
- Security/domain cases include cross-account/campaign isolation, private IDs and
  listings, concurrent writes/invitations, stale role revisions, hidden-field and
  active-link rejection, bounded response bytes/counts, image MIME/checksum/format/
  dimensions/body limits, ownership/reference checks, storage quotas and reopen.

Docker/Compose:

- `docker compose config --quiet` with an explicit HTTPS origin: passed.
- Linux arm64 M25.4 image build and updated isolated Compose acceptance: passed.
  Checks covered health, non-root/read-only execution, alternate internal/external
  ports, proxy header gates, auth, private content, explicit sharing/withdrawal,
  original image checksums/bytes and persistence after container recreation. Logs
  contained no credentials or private note marker. QA containers/volumes removed.
- Docker was unavailable on the first attempt; it became available and the final
  acceptance completed. The script now preflights the engine and keeps cleanup
  diagnostics separate. TLS headers are simulated; public certificates/DNS were
  not tested. Linux amd64, native Windows and minimum Rust 1.88 remain unverified.
- CI workflow exists but remote CI has not run. Dependency licenses checked;
  full vulnerability/image and production security review remain M25.7 work.

## Next step and explicit limits

Next: **M25.8** (first release after full M24 acceptance). Open decisions: production hosting and the
server code license.

The application repository now holds the M25.5 desktop client (schema 020, commit 7bf720e) and
the M25.6 access-loss handling; its legacy storage identifiers and personal data were not
touched. Real hosting/operator policy and the server code license remain open before
production distribution. No production deployment, publication or release.

Skills applied in the M25.5/M25.6 slices: `rust-best-practices` (ownership, error and test
patterns, Clippy discipline) and `sqlite-expert` (short atomic transactions, indexed
lookups, FULL durability kept). The earlier M25.4 delivery
recorded `pages:write-page`; the current environment additionally provides Rust and
SQLite skills.
