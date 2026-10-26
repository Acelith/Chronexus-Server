# Chronexus Server status

Updated: 26 October 2026. Source 0.1.0, server schema 005. Not published.

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

## S5 server contract implemented (desktop queue pending)

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
- Not done: membership-removal push hint, desktop durable queue, conflict UI, latency
  measurement under real conditions, S6 revocation lifecycle.

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

Next: **S5 desktop integration** (durable outbound queue, states, conflicts, latency) on the contract above; earlier plan: extend retry-safe operations to
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

Skills applied in the S5 contract slice: `rust-best-practices` (ownership, error and test
patterns, Clippy discipline) and `sqlite-expert` (short atomic transactions, indexed
lookups, FULL durability kept). The earlier S4 delivery
recorded `pages:write-page`; the current environment additionally provides Rust and
SQLite skills.
