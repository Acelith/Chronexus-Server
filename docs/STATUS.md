# Chronexus Server status

Updated: 26 October 2026. Source 0.1.0, server schema 004. Not published.

## Implemented server milestones S1 through S4

- S1/S2: Rust/axum, independent SQLite WAL/full durability, transactional migrations,
  health/configuration/proxy gates, minimal Compose with safe defaults and adjustable
  ports; invitation-only accounts, Argon2id, hashed secrets, persistent attempt
  limits, session expiry/device revocation and operator-assisted recovery.
- S3: campaign creation/listing, owner/player/reader roles, one-use campaign invites
  for new/existing accounts, owner-only invitation/role administration and member
  removal/self-leave. Monotonic membership revisions reject stale leave/rejoin actions.
- S4: typed private-by-default content and explicit member projections for pages,
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

## S5 in progress: durable content-save receipts

- Optional `requestId` on typed content PUT, bound to account/campaign, target,
  normalized payload, base revision and visibility. Receipt and mutation commit in
  one SQLite transaction. Existing unkeyed and legacy clients remain compatible.
- Concurrent duplicates and retries after reopen return the accepted historical
  revision without another mutation/feed event. Later edits/deletions/withdrawals
  are never undone. Current authentication and write role are checked on retry.
- Changed-request reuse fails with a payload-free 409 error; ordinary stale writes
  keep their existing revision conflict. Failed operations do not consume IDs.
- Additive migration 004 preserves content and credentials. Receipts hold only
  IDs/digests/revisions, capped at 10,000 per account/campaign with no silent eviction.
  Capacity blocks new keyed saves before mutation, while existing retries still work.
  Safe compaction/retry horizons remain required before production continuous use.
- Generated contract/types include the optional request and both conflict shapes;
  meta advertises `idempotent-content-saves`.

Windows x64, Rust 1.99.1 (current slice):

- `cargo fmt --check`, `cargo clippy --locked --offline --all-targets -- -D warnings`:
  passed.
- `cargo test --locked --offline`: **48 passed** (19 auth, 7 campaigns, 22 content).
  Six new cases cover restart/deletion, duplicate concurrency/current roles,
  request binding, failed/conflicting retries, quotas and schema 003 migration.
- `cargo build --locked --offline --release`: passed.
- `python scripts/contract.py` and `--check`: passed; generated TypeScript checked
  with the existing desktop TypeScript compiler (`--noEmit --strict --skipLibCheck`).
- Dependencies fetched from the unchanged lockfile after the restricted process
  could not establish TLS. No dependency versions changed. Docker is unavailable
  here; no new container, Linux/macOS or minimum-Rust qualification in this slice.
  Earlier S4 platform evidence below applies to its original source only.

## Earlier S4 checks and evidence

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
- Linux arm64 S4 image build and updated isolated Compose acceptance: passed.
  Checks covered health, non-root/read-only execution, alternate internal/external
  ports, proxy header gates, auth, private content, explicit sharing/withdrawal,
  original image checksums/bytes and persistence after container recreation. Logs
  contained no credentials or private note marker. QA containers/volumes removed.
- Docker was unavailable on the first attempt; it became available and the final
  acceptance completed. The script now preflights the engine and keeps cleanup
  diagnostics separate. TLS headers are simulated; public certificates/DNS were
  not tested. Linux amd64, native Windows and minimum Rust 1.88 remain unverified.
- CI workflow exists but remote CI has not run. Dependency licenses checked;
  full vulnerability/image and production security review remain S7 work.

## Next step and explicit limits

Next: **S5 continuous local/server saving**: extend retry-safe operations to
uploads/deletes/withdrawals, add the authorized private incremental feed and bounded
notification channel, then integrate the durable desktop outbound queue, automatic
reconnection and visible conflicts. Define safe receipt compaction and measure
end-to-end latency. The content-save receipt slice does not complete S5.
The original app supports explicit sharing of four legacy kinds; new private
replica/image/selected-sheet selection and scope preview UI remains pending.
S4 backend completion is not a claim of full desktop S4 acceptance or a completed
application M25 release. S6/S7 still own comprehensive deletion/recovery, retention
and production backup qualification. AI and web remain S10/S11.

The current application records M23 implemented, M24 client implemented, M25 server
and M27 Android. Application files, schema 019, identifiers and personal data were
not modified. Real hosting/operator policy and the server code license remain open
before production distribution. No production deployment, publication or release.

Skills applied in the current slice: `rust-best-practices` for ownership/error and
test patterns, `sqlite-expert` for short atomic transactions/indexed receipt lookup
(keep the existing FULL durability), and `systematic-debugging` to trace a new test's
incorrect member-response assumption before correcting it. The earlier S4 delivery
recorded `pages:write-page`; the current environment additionally provides Rust and
SQLite skills.
