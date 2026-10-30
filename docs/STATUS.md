# Chronexus Server status

## 30 October 2026: M45 server/web combat (schema 012, source only)

- M44 committed first as `10774c4`; M45 adds an explicit initiative phase, manual
  or d20-plus-manual-modifier initiative, GM tie ordering, turn/round advancement,
  delay, add/remove/end and per-turn movement/parity reset. Player movement is
  restricted to their own visible active-scene token on its turn.
- HP/max HP/AC are changed only by the token owner: a GM controls its own monsters,
  not a player character. Owner-only values/HP deltas and GM notes are structurally
  filtered. Optional public HP/status is explicit; status is manual. Reassignment
  cannot take over a player's token. HP actions require web confirmation.
- Conditions have localized stable IDs, round durations and retained expiry
  reminders, including canvas markers; removal is explicit. Public/GM-secret rolls,
  movement and applied effects have a bounded audience-filtered log. Preparation
  scenes, hidden turns and secret rolls never enter other seats' HTTP/live views.
  Dice have no automatic HP, sheet, journal, resource or AI effects.
- Backups include combat and original maps. Verification now checks table-map
  SHA-256/size as well as campaign images, counts both in its upload report and
  refuses a corrupted-map copy. Restore continues to revoke every prior session.
- macOS arm64 / Rust 1.97: `cargo fmt --check`, Clippy `--all-targets -- -D warnings`,
  full `cargo test --locked` (106 passed), release build, generated-contract drift,
  strict TypeScript check of generated types and JavaScript syntax passed.
  Twenty-nine web DOM/board tests passed. Server ten-locale catalog/placeholder
  checks and desktop `pnpm i18n:check` passed; existing candidate-catalog backlog
  remains unchanged. Tracker controls tested in English, Italian and Arabic.
- Real isolated browser/server: schema 011 preview migrated to 012 with its table,
  map settings and tokens retained; three combat rounds, confirmation and HP/status
  save checked. Arabic RTL at a narrow viewport had no horizontal document overflow.
  One real WebSocket loopback observation was under 1 ms at integer-ms resolution;
  this is not a LAN measurement or the three-native-desktop scenario.
- Skills: Frontend Design for the established table layout; UI/UX Pro Max for
  keyboard/single-pointer controls, confirmation and draft/error handling.
  No source changes in the desktop repository, version bump, remote publication,
  release, deployment or Windows/Linux qualification. Desktop M42–M45 integration
  remains pending. Next planned server milestone: M46 fog/vision, then M47 monsters.


## 30 October 2026: M44 server and web board (schema 011, source only)

- Persistent scene/map/grid, owned and hidden tokens, square/axial hex/gridless
  movement, difficult terrain, alternating diagonals, rulers, shared area templates
  and labelled ft/m conversion. GM activation is explicit; prepared scenes, hidden
  objects and UVTT geometry are filtered on the server for HTTP and live snapshots.
- Maps retain validated original PNG/JPEG/WebP bytes; UVTT 0.2/0.3 embeds PNG,
  grid, walls, doors and lights as inert GM preparation data. Vision remains M46.
  No journal/sheet/AI writes. Backups preserve table state and original map bytes.
- macOS arm64: full Rust suite (101 tests), fmt, Clippy with warnings denied,
  release build and generated contract check passed. Twenty existing web DOM
  tests and five board tests passed, including conversions/decimal formatting in
  ten locales, both hex grids, keyboard movement and disconnected write gating.
  Server catalog checks and desktop pnpm i18n:check passed; existing candidate
  catalog backlog unchanged. Browser check on a real isolated server: Italian
  board rendered, keyboard move persisted and its dual-unit spent budget updated.
- Limits/architecture are in docs/API.md and docs/DECISIONS.md. Conservative area
  overlap is explicitly labelled; templates do not apply game effects. Native
  Windows/Linux, three-desktop scenario and LAN latency target remain unverified;
  desktop M42/M43/M44 integration is still pending. No release or deployment.
- Skills: Frontend Design (existing palette, map-led layout) and UI/UX Pro Max
  (keyboard/single-pointer alternatives and recovery). Next authorized work: M45
  combat tracker; then planned M46 fog/vision.

## 30 October 2026: M42/M43 recovery and GM listing fixes (source only)

- The web live channel waits for its initial state, reconciles an unacknowledged
  message and retries its exact operation once per reconnection. Already committed
  messages are not resent; conflicts keep the draft for an explicit retry. Newer
  text typed while awaiting confirmation is preserved, and obsolete socket callbacks
  cannot change the current view.
- GM journal grants use an optional validated `after=campaignId/kind/itemId` cursor,
  stable composite ordering and 100-item pages. The web workspace reads all pages.
  Every page rechecks access; revoking a cursor row does not skip later grants.
  The response remains an array and schema 010 is unchanged. OpenAPI regenerated.
- Fixed the M43 unit-test temporary directory on macOS without relaxing the
  production database's symlink checks.
- macOS arm64: `cargo fmt --check`, Clippy `--all-targets -- -D warnings`,
  `cargo test --locked` (95 passed), release build and contract drift check passed.
  The full suite initially hit the restricted local-port environment in the fake
  AI runtime tests; it passed when rerun with local listeners permitted.
  Twenty web DOM tests passed, including reconnect outcomes in English, Italian
  and Arabic/RTL, and loading 101 GM grants. All ten new web cases fail against
  the previous script. JavaScript syntax checks passed.
- Localization: no new visible strings; existing translated connection/error
  messages reused. Server ten-language catalog/placeholder test and desktop
  `pnpm i18n:check` passed (zero missing registered UI messages; the reported
  existing candidate-catalog backlog remains). No layout change or new browser run.
- Skill: UI/UX Pro Max, for error recovery, draft preservation and retry feedback.
  No release, deployment or publication. Windows/Linux checks were not rerun.
  Next: complete desktop M42/M43 integration; the next planned server feature is
  M44 maps/grid/tokens. Server AI hardware qualification and production acceptance
  remain separate open M25 work.

## 30 October 2026: M42/M43 server source implemented; no release

- Operator `invite --gm` grants GM capability; ordinary accounts cannot create tables.
  One-use table codes register new accounts or seat existing ones. Members can leave;
  GMs can remove players/spectators. Table membership grants no journal membership.
- Authors may explicitly grant a currently shared journal item to their table's GM.
  The GM reads only these items; private items, withdrawn items, removed authors and
  other campaigns remain inaccessible. This is a server API; desktop audience UI
  remains a separate app change.
- Live WebSocket uses a 30-second one-use ticket issued via Bearer authentication,
  bounded public messages, operation IDs and table revisions. It checks current
  seats/sessions on writes and notifications, has presence and reconnect snapshots.
  GM-only map/token/combat content does not exist yet and cannot be sent by this
  channel. Table web pages show seats, explicit GM journal grants and live activity.
- Windows checks: Rust fmt, Clippy `-D warnings`, full cargo test, release build,
  generated contract check, ten web DOM tests, server ten-locale catalog check,
  desktop `pnpm i18n:check` and a synthetic two-browser GM/player live test passed.
  The browser test covered reload, mobile layout and Arabic RTL. Linux/macOS and
  production TLS remain unverified. No deployment, tag or release.

## 30 October 2026: M42 foundation in progress (source only)

Historical checkpoint, superseded by the M42/M43 implementation and fixes above.

- Schema 008 adds tables, separate GM/player/spectator seats and one-use table invitations.
  Account membership in a table grants no journal campaign access. The web interface
  now lists, creates and joins tables and lets a GM issue player/spectator codes.
- M42 is incomplete: GM enrollment policy, invitation sign-up, seat administration,
  journal audience *shared with the GM too*, desktop UI and full authorization matrix
  remain. M43 live channel and GM web workspace remain unimplemented.
- JavaScript syntax, eight web DOM tests, ten-locale server catalog coverage,
  desktop `pnpm i18n:check`, Impeccable detector and SQLite migration syntax passed.
  Rust fmt, Clippy, tests, release build and generated
  contract checks could not run because Cargo is unavailable on this machine.
- No release or deployment from this partial slice.

## 29 October 2026: M25 completion on local Docker (source only)

- Android (M25.9) removed from the scope at the user's request.
- Local test deployment: Compose `https` + `ai` profiles as project `chronexus-m25q` at
  `https://chronexus.localhost:8443` (Caddy local CA), Ollama 0.40.10 on the internal network,
  `qwen3:4b` copied by digest from an existing local volume (no download). Runtime port closed
  on the host; no internet route from the `ai` network.
- **Defect fixed:** the general 20 s request bound returned 408 for every AI answer longer than
  20 s, so `CHRONEXUS_AI_TIMEOUT_SECONDS` never applied. AI answers now use only their own
  deadline (unit test `only_ai_answers_skip_the_general_request_bound`).
- `scripts/qualify-ai.py` (new, synthetic data): outsider refused, private note never cited,
  instruction-like note never sent, median 28 s / max 76 s on CPU, runtime peak about 5 GiB.
  Quality: **2 of 14** answers pass; qwen3:4b leaks its reasoning despite `think: false`. Not
  qualified. Qwen3.8 27B and Gemma 4 12B do not fit this 8 GB Docker VM; GPU hosts untested.
- Web interface translated into the ten desktop languages (`web/i18n.js`), browser-language
  default, language menu, Arabic right to left; a right-to-left skip-link offset that scrolled
  the page to an empty area was fixed. API messages stay English. Browser check at 375 px
  (Arabic) and desktop width (Italian) with a synthetic account.
- Checks: cargo fmt, clippy `-D warnings`, cargo test (90 passed), contract check, 7 web DOM
  tests (jsdom from the desktop checkout), desktop `sync::tests::live` 5/5 against this source.
- Still open: a qualified server AI model (needs more memory or a GPU), GHCR images (the GitHub
  token lacks `write:packages`), a production deployment with real DNS/TLS. No release built.

## 28 October 2026: web interface refinement (source only)

- Retained the existing parchment/leather identity; added responsive sidebar/top
  navigation, focused sign-in layout, readable campaign rows, keyboard-scrollable
  tables and campaign subpages for usage, members, invitations and deletion/leave.
  Campaign navigation remains selected; invitations load only on their own page.
- Fixed memory-only login when sessionStorage is blocked, stale response navigation,
  non-JSON 401 logout handling and the skip link accidentally changing the hash route.
  Names remain inert text; API permissions, database schema and confirmations unchanged.
- Five jsdom regression tests and `node --check web/app.js` pass; all five tests
  fail when run against the previous committed script. Browser review with
  a synthetic HTTP fixture covered sign-in, campaign pages, members, invitations,
  devices, operator status and cancelling typed-name deletion, at 375, 736 and 1440 CSS pixels.
  This is frontend review, not new real-server acceptance. Rust fmt, Clippy, tests,
  release build and contract generation were attempted but blocked: `cargo` is not
  available in this environment. No binary was rebuilt or published.
- `pnpm i18n:check` passed in the desktop checkout (zero missing registered UI
  translations); it does not scan this repository. Server English-only localization
  remains open. No new visible copy was introduced; complete ten-language server
  acceptance is not claimed. Native browser/OS coverage and RTL translations pending.
- Skills: Impeccable and Frontend Design for visual hierarchy and responsive review;
  Systematic Debugging for navigation/session causes. Webapp Testing guidance informed
  regression coverage; its Python Playwright runtime was unavailable, so browser
  checks used the supported computer-use browser tools and DOM regressions used jsdom.
- `docs/WEB_DESIGN.md` records boundaries for a separate future Tables workspace;
  M42–M47 remain unimplemented. Next milestone remains M25 completion (GPU/model
  qualification, GHCR and public deployment; Android acceptance depends on M27).

Updated: 27 October 2026. Source 0.1.1 + unreleased M25.10/M31 support, server schema 007.

## 27 October 2026: M31 clue payload for the desktop board

- Schema 007 widens the typed content kind to `clue` without changing existing
  rows, after a pre-upgrade backup. Clues retain the author's status and source
  IDs; the server validates linked page and session visibility before private
  replication or sharing. Withdrawing a referenced page or session withdraws
  public dependent clues. A claim never becomes a relationship.
- OpenAPI and TypeScript contracts regenerated. Focused server tests cover private
  source refusal, explicit sharing, attribution, no factual relationship and
  source revocation. The desktop client's real-server test copied a clue to a
  second local device with the block ID intact. Windows `cargo test --locked`,
  Clippy with warnings denied, rustfmt and the release build passed.
  This server change is not a public deployment or a published image.

## 27 October 2026 (night): M25.10 server AI without GPU qualification

- Optional Compose profile `ai` (Ollama 0.40.10 pinned by digest, no published port, internal
  network without internet, `OLLAMA_NO_CLOUD=1`, all capabilities dropped); `compose.ai-gpu.yaml`
  for NVIDIA and `compose.ai-pull.yaml` for an operator's temporary model download.
- `GET /api/v1/ai/status` and `POST /api/v1/campaigns/{id}/ai/answers`: membership check,
  sources limited to what the account may read, instruction-like items excluded, citation
  check, per-account hourly allowance, bounded concurrency and queue, timeout, cancellation
  frees the place, cloud-disabled and model-digest checks before every answer. Nothing is
  stored or logged. Settings in `CHRONEXUS_AI_*` (docs/AI.md); `meta` lists `server-ai`.
- Evidence: 8 tests in `tests/ai.rs` against a fake runtime; 88 tests, Clippy `-D warnings`,
  fmt, contract and public-origin audit pass; `scripts/check-compose.py` passes again (it
  lacked `CHRONEXUS_DOMAIN` and the dynamic range since the environment-driven deployment).
  Docker run with the real runtime and no weights: status ready, cloud disabled, missing model
  refused, no internet from the runtime, internet only with the pull override.
- **Open:** GPU/model qualification (quality, citations, latency, memory) needs hardware and
  weights; no model was downloaded. M25.9 still needs application M27 on a real device.

## 27 October 2026: GitHub migration — reachable history sanitized

- GitHub is the canonical fetch/push and release destination. The owner authorized
  the one-time complete history cleanup; external recovery bundles are verified.
  All 30 commits and both tags were retained, sanitized and published. A fresh GitHub
  mirror passed the complete reachable-object audit and git fsck; origin is GitHub only.
- Release publishing uses the GitHub API and verifies private draft/public downloads.
  Credentials stay on API/upload hosts. GHCR is the only registry publishing target;
  image publication is pending, so Compose defaults to building the local source.
- Handbooks use ordinary repository Markdown links. Retired hosting workflows are
  removed; GitHub CI audits full fetched history for retired-host fingerprints.
- Validation: 75 Rust integration tests, rustfmt, Clippy with warnings denied,
  release build and contract/TypeScript drift check passed. Five Python transport/origin
  regressions passed. No server domain or schema change.
- GitHub CI reached Compose validation after its other checks passed; its required
  domain fixture was missing. The workflow now sets the explicit test domain, and
  Compose configuration passes with an empty environment file.
- Both release descriptions are clean; historical binary assets are unchanged. Previous
  main SHAs still resolve in GitHub's cache, outside published branches/tags. A private
  support-request draft is prepared; no message was sent or repository deleted.
- No specialized history-migration skill is installed; official GitHub/git-filter-repo
  guidance was used. Procedure: docs/GITHUB_MIGRATION.md. Next: M25.8 deployment acceptance.

## 27 October 2026 (evening): M25.11 web interface, M25.8 closure

- **M25.11 implemented.** Static web interface at the public origin (`web/`, embedded in the
  binary, strict CSP) using the existing Bearer API: no cookies, same permission checks, a
  foreign origin is refused. New read-only endpoints: `/api/v1/account`,
  `/campaigns/{id}/overview` (own usage only), `/operator/status`, `/operator/accounts`;
  operator role from `CHRONEXUS_OPERATORS` (no campaign data). Evidence: 5 tests in
  `tests/web.rs`; 80 tests, Clippy `-D warnings`, fmt, contract and public-origin audit pass;
  a real browser run on a loopback server with synthetic data (sign-in, HTML-like campaign
  name shown as text, role change, invitation, cancelled and confirmed removal, devices,
  operator view at 375 px); the Docker image (needs `web/` in the build context, fixed)
  served through Caddy over HTTPS with the existing data. English only.
- **M25.8:** compatibility table and upgrade checklist (`docs/COMPATIBILITY.md`), versioned
  contract attached to release 0.1.1, desktop proof of no network without a configured
  server (application `scripts/qa/linux/no-network-check.sh`: 0 connections outside
  loopback). Container images on GHCR still need a `write:packages` GitHub credential.
- **M25.9** not started: the Android contract needs application M27 on a real device.
  (M25.10 was implemented later the same day without GPU qualification; see above.)

## 27 October 2026 (afternoon): release 0.1.1 and registry images (M25.8)

- Release [0.1.1](https://github.com/Acelith/Chronexus-Server/releases/tag/v0.1.1) (tag on
  0e8faf1) with Linux amd64/arm64 binaries and SHA256SUMS.txt, verified anonymously.
- Historical images version `0.1.1` and `latest` on the retired registry (index
  `sha256:ce86254e…`, linux/amd64 + linux/arm64) pushed with `scripts/push-image.py`:
  `docker push` from Docker Desktop timed out on large layers. Pulled back by digest; both
  platforms report 0.1.1; the HTTPS test deployment now runs that image and kept its data.
  Binaries extracted from the registry image match the local build byte for byte.
- Fixed a race in backup verification (shared reference file name; recovery tests failed in
  2 of 3 runs, now 0 of 6). 75 tests pass on macOS and on emulated Linux amd64.
- `.forgejo/workflows/ci.yml` added; no Forgejo runner is registered yet.
- Historical CI image `chronexus-ci:2026.10.27` on the retired registry (amd64+arm64) pushed for the
  application's Forgejo workflow.
- GitHub mirror: main, tags and releases 0.1.0 and 0.1.1 with verified identical assets.
  Container images remain on retired infrastructure; they have not been published to GHCR.
- Planned: application M42–M47 (virtual tabletop) server work, see ROADMAP.
- Still open: a public deployment with real DNS/TLS, image/dependency scans in CI.

## 27 October 2026: Docker deployment and desktop integration on macOS

- Milestones renamed M25.0–M25.11 (were S0–S11).
- `cargo test --locked`: 75 passed natively on macOS arm64 (Rust 1.97) after fixing
  `backup`/`verify` (reference structure built in the symlinked macOS temp dir) and test
  temp paths; clippy `-D warnings`, fmt and contract check passed (contract regenerated:
  licence drift).
- Compose `https` profile fixed (the server took the proxy's fixed IP) and run for real:
  Docker 29.9.2 linux/arm64, Caddy local CA on `chronexus.localhost:8443`.
- Desktop acceptance against this deployment: three real apps (macOS + two Linux arm64
  containers) passed 37/37 steps of the app's `scripts/qa/sync` scenario, including role
  changes, leaving and removal, a proxy stop/start; an installed Linux release build signed
  in and downloaded a campaign over HTTPS. Not done: public TLS/DNS, Linux amd64, image
  scan, release.
- Wiki sources for operators and developers in `docs/wiki/` (application M41), not yet
  pushed to the wiki repository.
- Next: **M25.8** first release, after the user's go-ahead.

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
