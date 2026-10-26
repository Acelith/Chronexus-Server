# Chronexus Server decisions

## 26 October 2026 Initial implementation through S2

The user authorized reading the current application and starting the server through
the account/session milestone (S2, called M2 in the request). The current application
documents M23 as implemented and M24 as a desktop client tested against a contract
fake. Its server milestone is now **M25**; Android moved to **M27**. Server S0–S11
identifiers remain stable and are independent of application milestone numbers.

The application's `docs/SERVER_NOTES.md`, `docs/SYNC_API.md` and `sync/api.rs` confirm
invitation-only accounts, HTTPS, server-readable v1 content without end-to-end
encryption, and human revision conflict resolution. These decisions supersede the
previous unconfirmed registration/encryption rows in this repository's plan.

The user's additional continuous private replication and self-hosted AI requirements
extend the current client: they remain S5 and S10 work, not existing client behavior.
Private owner replication and member sharing must remain separate permissions.

### Implementation choices

- Use Rust/axum and SQLite WAL with `synchronous=FULL`, versioned transactional
  migrations and foreign keys for a single small-group instance. PostgreSQL and
  horizontal replicas are outside this slice; do not share a live SQLite volume
  between service replicas. This is an engineering default, not a hosting purchase.
- Generate OpenAPI and TypeScript schemas from implemented Rust types. Publish
  only the account/session subset locally. The desktop's remaining draft sharing
  contract stays the reference for S3–S8; do not replace it with the partial schema.
- Use Argon2id PHC hashes with independent random salts, 256-bit opaque random
  Bearer tokens/codes and SHA-256 lookup hashes. Sessions expire after seven idle
  days or thirty absolute days; at most twenty active sessions per account.
- Issue server invites with an operator CLI command, valid for 24 hours and one
  use. No credentials in startup logs or sample configuration. Operator-assisted
  recovery codes expire in one hour, supersede earlier recovery codes and reset
  the password/revoke all prior sessions atomically when redeemed. No email service.
- Persist authentication attempt limits: five per normalized username/code and
  thirty per client IP within fifteen minutes, including successful attempts.
  Hash bucket identifiers, cap active buckets at 10,000, and admit at most four
  blocking operations concurrently. Auth bodies are limited to 8 KiB.
- Bind Compose to host loopback by default. Trust only the dedicated bridge
  gateway as the host reverse proxy, with explicit overrides for another topology.
  Require HTTPS forwarding, an overwritten single client-IP header, exact Host
  and any supplied Origin matching the configured public origin. No cookies or
  CORS permissions; a future web interface needs its own browser security design.
- Allow direct plain HTTP only through an explicit loopback development setting.
  Production TLS is terminated by an existing operator-managed proxy; public
  certificates, DNS and the actual hosting environment remain deployment work.
- Preserve original app identity and schemas; create a separate server schema 001.
  No application edits, production deployment, registry push or release in this slice.

### Outstanding decisions

Real domain, proxy address/topology, public exposure, recovery operator procedures
and production retention/backup policy must be confirmed before production use.
Campaign tombstone retention belongs to S5/S6, recoverable server backups to S7,
server AI hardware/model qualification to S10 and web management to S11. The server
code's license remains to be established before distribution; dependency licenses
are recorded separately in `DEPENDENCIES.md`.

## 26 October 2026 S3 campaign access

Schema 002 adds campaigns, versioned active memberships and separate hashed
campaign invitations. Campaign members see their campaign name, membership roles
and display names; nonmembers receive no list or detail metadata. Only the owner
issues/revokes invitations and changes other roles. Players/readers may leave;
the sole owner must retain ownership until an explicit transfer/deletion workflow
is implemented. The public role is `reader`, matching the original client.

Each account may belong to 100 active campaigns; each campaign admits 100 active
members, 100 pending invites and 1000 retained invitation records. Invites expire
after seven days, are consumed once in the same transaction as registration/join,
and old invitation records can be purged after their expiry plus thirty days.
Removal increments an inactive membership revision; rejoin increments it again,
so an old role/removal request cannot affect a later membership accidentally.
No campaign content is stored until S4; authored content withdrawal on removal
will be added atomically with that storage. No publication in this milestone.

## 26 October 2026 S4 private originals and explicit projections

Schema 003 adds typed content, public legacy projections, dependencies and owned
image originals. Per-item visibility defaults to private and only the author can
read private data, including against the campaign owner. The legacy PUT endpoint
is itself explicit publication. Legacy DELETE withdraws publication while retaining
the private original; extension DELETE explicitly removes its payload. Revisions
remain monotonic across deletion/recreation, preventing stale-write resurrection.

The public feed includes only the original four kinds and uses per-campaign clocks;
private changes and image/sheet extensions do not enter it. Responses are bounded
by both count and encoded bytes. Document properties/styles are allowlisted and
active URL schemes rejected. No target data is pulled from mentions. Public
relationships/party entries need explicitly shared pages; withdrawing one page
withdraws dependent public projections without rewriting their original assertions.
Removing an author withdraws their projections atomically and retains originals.

Use SQLite BLOBs for server image originals within existing persistent storage,
keyed by UUID/campaign/author with SHA-256 and dimensions. This preserves bytes and
atomic quota/reference checks without accepting user paths or introducing a second
filesystem writer. It is independent of the desktop's managed attachment folders.
No partially committed file cleanup is needed; completed unreferenced uploads are
private, quota-accounted and explicitly deletable. S7 backups must include these BLOBs.

Adopt the desktop's pinned image 0.25.10 with PNG/JPEG/WebP features only. Its Rust
minimum raises the server manifest floor from 1.85 to 1.88; the verified toolchain
remains 1.97. Output size/dimensions are checked before full decode, with a bounded
allocation setting, two content-transfer admissions and four blocking workers.
The original bytes, including metadata, are preserved without interpreting photos.

Record a server-specific SQLite application_id and validate legacy server tables
before modifying journal mode or migrating. A desktop/foreign archive with a similar
version number is refused; schemas 001/002 upgrade additively and older server
binaries refuse schema 003. No application storage identifiers or schemas change.

S4 server APIs are implemented. The original app's sharing client is exercised for
four legacy kinds; private whole-campaign replication and image/section selection
UI are not added to the desktop in this repository. S5/S8 must integrate and verify
that explicit scope preview. Continuous save queues, idempotency keys and change
notifications are S5 work; production recovery and full revocation lifecycle remain
S6/S7. AI and web remain S10/S11. No production deployment or publication.


## S5 save receipts (26 October 2026)

Keep idempotency in the existing content transaction rather than HTTP response
middleware: a durable acknowledgment and mutation must commit or roll back together.
Use an optional client-generated operation UUID, scoped to account/campaign and
bound to normalized payload, visibility, base revision and target. Authentication
and current write membership precede receipt lookup. Retrying acknowledges only
the historical accepted revision; it never reapplies content or sharing.

Retain only request digests and revisions, without private response snapshots.
Do not cache conflict bodies that may become unauthorized later. Initial bounded
storage refuses new keyed operations at 10,000 receipts per account/campaign;
never silently expire receipts and risk replaying an old creation after deletion.
This limit is explicit and provisional; S5 must add an acknowledged compaction or
safe retry-horizon protocol before production continuous use. Existing unkeyed
clients and legacy sharing routes remain compatible. Schema 004 is additive;
previous server binaries must refuse the newer archive. No desktop changes,
new dependencies, deployment, publication or complete S5 acceptance in this slice.

## S5 server contract: log, retry horizon, notifications (26 October 2026)

- **Feed = signal, state = truth.** Schema 005 adds a payload-free per-campaign change
  log. `/sync` reads the log only to learn which items to re-examine and always emits
  the *current* state the caller may see (own items fully; other members' items only
  while shared, otherwise a payload-free tombstone). Private edits of other authors
  never reach a member's feed, cursor or notification, so activity cannot be inferred.
  The log reuses the campaign sequence counter shared with the legacy feed.
- **Fail closed on cursors.** Bounded log (100,000 rows per campaign); dropping rows
  raises a floor and stale or future cursors get 410, forcing a full listing. No attempt
  is made to replay deletions the server no longer remembers, because guessing could
  resurrect withdrawn items. The cost is an occasional full resync for very stale clients.
- **Retry horizon instead of acknowledgments.** The provisional 10,000-receipt cap is
  replaced by verifiable age. Operation IDs are UUIDv7; IDs older than 30 days are refused
  (410) and receipts are kept 31 days, so expiry cannot allow a late retry to re-apply an
  old creation after deletion. Chosen over client acknowledgments because it needs no
  extra round trip or state on devices that may be offline for weeks. Existing v4 IDs from
  the unreleased first slice are rejected; no deployed client used them.
- **Idempotent deletes and withdrawals** reuse the same receipts (`op` column). Uploads
  are idempotent by (campaign, author, SHA-256), returning the stored original; attachment
  deletion stays unkeyed and clients treat 404 as done.
- **Notifications** are SSE with Bearer authentication: it needs no WebSocket dependency
  and survives plain reverse proxies. Frames are content-free hints; bounded streams
  (3 per account, 64 total, 10-minute lifetime, 25 s keepalive re-authorization). Polling
  `/sync` remains the required fallback. The only new direct dependency is `futures-core`
  (already in the lock graph via axum) for the response stream trait.
- Schema 005 is additive and backfills one log row per existing content row. Prior server
  binaries refuse the newer archive. No desktop change in this slice; the legacy four-kind
  client keeps its behavior.
