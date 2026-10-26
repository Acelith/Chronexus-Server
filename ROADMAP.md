# Chronexus Server roadmap

Updated: 26 October 2026. Status: planning only; no server implementation.

Chronexus Server will provide optional, self-hosted campaign synchronization and
sharing between Chronexus players. Enabling synchronization for a campaign keeps
a durable local copy and a private server copy continuously updated while online.
Sharing with other players is a separate explicit choice. Users may also explicitly
select local AI or AI running on their own server. The desktop remains fully usable
offline without an account or server. This repository owns the server, its HTTP
contract, releases and container images; the Chronexus
repository owns desktop and Android clients.

This roadmap expands the application's M24 milestone without renumbering it.
Application M23 remains the prerequisite for M24 implementation; M25 Android
depends on a qualified server and compatible desktop synchronization.

## Product boundaries

- Rust service, Docker Compose deployment on user-controlled infrastructure, HTTPS.
- No telemetry, unapproved campaign upload or third-party cloud AI. Server AI uses
  an explicitly enabled self-hosted runtime, separate from the local-only adapter.
- Campaign synchronization is opt-in. Once enabled, saves automatically replicate
  to the server; the local copy remains durable and usable offline.
- Synchronization to the owner account does not share content with other players.
  All content starts private. Account access or campaign membership alone does
  not grant access to every item or every field of a character sheet.
- Preserve stable UUIDs, attribution, source identity and revision history.
  Shared player claims remain attributed claims, never established world facts.
  Mentions and asserted relationships remain distinct; retain relationship certainty.
- Campaign synchronization scope must be visible before activation: campaign
  notes, sheets, relationships and managed attachments may form the private online
  copy. Unselected campaigns and device settings stay local. AI weights, training
  data, adapters and complete local backups stay outside synchronization.
- AI conversations stay local by default. Explicit server inference may transmit
  the submitted prompt and a previewable, authorized campaign context for that
  request; this does not grant other players access or authorize training.
- Server storage is separate from desktop SQLite. Never synchronize database
  files or make the server a second writer to a desktop Store.
- Campaign content and attachments are untrusted data, never executable code or
  authority for instructions. No GM-only metadata or speculative spoilers.
- English is the source language for code, contracts and documentation. Preserve
  canonical campaign text and support client-selected locales without translating data.

## Milestone sequence

All milestones below are **planned**. Each requires implementation, focused
automated tests and a reviewed commit before it can be marked complete.

| ID | Deliverable | Dependency |
| --- | --- | --- |
| S0 | Confirm deployment and security decisions | User decisions and application M23 |
| S1 | Persistent service foundation and versioned contract | S0 |
| S2 | Accounts, sessions and device access | S1 |
| S3 | Campaign invitations and authorization | S2 |
| S4 | Explicit selective sharing and managed attachments | S3 |
| S5 | Continuous local and server saving with visible conflicts | S4 |
| S6 | Revocation, deletion and recovery | S5 |
| S7 | Security qualification and operational recovery | S6 |
| S8 | Desktop integration and first server release | S7 and desktop M24 acceptance |
| S9 | Android contract qualification | S8 and application M25 |
| S10 | Explicit local or self-hosted server AI | S7 and S8; can follow S8 independently of S9 |
| S11 | Web campaign and server management | S7 and S8; can follow S8 independently of S9/S10 |

## S0 Deployment and security decisions

Confirm these choices before implementing the server:

| Decision | Required outcome |
| --- | --- |
| Hosting | Domain, host OS and CPU architecture, reverse proxy, TLS ownership, persistent storage and operator responsibilities; configurable bind address and ports |
| Registration | Invitation-only or open registration; first administrator bootstrap and account recovery policy |
| Encryption | Explicit decision on end-to-end encryption, including key ownership, device enrollment, recovery and metadata visibility |
| Persistence | Choose SQLite or PostgreSQL against expected concurrent users, deployment and backup needs |
| Account security | Choose passkeys, robust passwords or both; define recovery without assuming an email delivery service |
| Retention | Agree on deletion, change-log retention, backup retention, quotas and offline-device behavior |
| Server AI | Confirm available CPU/GPU/RAM, qualified models and licenses, resource quotas, inference retention and the encrypted-content access policy |

Record decisions and reasons in `docs/DECISIONS.md`. Encryption must be decided
before fixing the payload and attachment formats: HTTPS alone does not provide
end-to-end encryption. Server inference requires access to the submitted prompt
and selected context in plaintext on the inference host. If end-to-end encryption
is selected, the client must explicitly authorize that disclosure for each request
or a clearly scoped saved policy; the server must not gain a general decryption key.
Any proposed implementation dependencies need current
compatibility and license verification before adoption.

Acceptance: decisions confirmed by the user, threat model and authorization
matrix documented, M23 findings relevant to sharing resolved or explicitly gated.
This planning change does not authorize deployment, installers or publication.

## S1 Persistent service foundation and versioned contract

- Bootstrap the Rust service with axum, validated configuration, versioned database
  migrations, parameterized queries and safe transaction boundaries.
- Add liveness and readiness checks without exposing secrets or campaign content.
- Publish an OpenAPI contract under a versioned API such as `/api/v1`, with
  consistent errors, bounded pagination, request limits and compatibility rules.
- Generate client types from the published contract; CI detects contract drift.
  Clients must not maintain manually copied server types.
- Add reproducible container builds, non-root execution and documented persistent
  volumes. Create repository-specific `AGENTS.md` before implementation work.
- Deliver a Compose file and a minimal `.env.example`: safe defaults for optional
  settings, generated secrets through an explicit bootstrap step, no default
  credentials. Require only values essential to the selected deployment, such as
  the public HTTPS origin and TLS/proxy trust configuration. Refuse unsafe public
  startup with an actionable error instead of silently relaxing security.
- Configure the published host port and bind address independently of the internal
  service port. Default application binding to loopback behind a reverse proxy;
  support deliberate LAN/public exposure with HTTPS. Document port collisions,
  firewall rules and trusted proxy boundaries without assuming ports 80/443 are free.
- Keep database and optional AI runtime ports private to the Compose network.
  Use one public origin for API and future web UI; no public management or AI port
  by default. Add optional AI/GPU services through explicit Compose profiles.
- Prefer a single documented `docker compose up -d` path after bootstrap; include
  named volumes, health checks and resource limits. Do not mount the Docker socket,
  run privileged containers or expose secrets in the checked-in configuration.

Planned configuration surface (final names fixed during S1):

| Setting | Default or requirement |
| --- | --- |
| Public origin | Required for public deployment; exact HTTPS origin, including a nonstandard port when used |
| Host bind address and published port | Optional; loopback and a documented high port, both overridable |
| Internal API port | Documented default; independently overridable with matching health checks and proxy routing |
| Trusted proxies | None trusted by default; explicit addresses required when forwarding headers are used |
| Session and bootstrap secrets | Generated securely; required before authenticated use, never shipped as sample values |
| Database and storage | Persistent managed volumes and selected database defaults; external database configuration optional |
| AI and GPU profile | Off by default; runtime/model and resource overrides required only when enabled |
| Limits and retention | Conservative bounded defaults; optional operator overrides validated at startup |

Acceptance: clean startup and restart preserve a synthetic database; migrations,
configuration failures, health checks and contract generation are tested. Record
the first supported app/server version range, without inventing released versions.
Validate Compose configuration, minimal setup and an alternate host port; inspect
actual exposed ports, persistent data after recreation and failed insecure startup.

## S2 Accounts and sessions

- Implement the S0 registration and recovery policy, credential protection,
  authentication attempt limits and expiring/revocable sessions.
- Support distinct devices and revoking one device or all sessions. Store no raw
  reusable credentials in logs; server secrets stay outside Git and images.
- Document client credential storage requirements for desktop secure storage
  and Android Keystore. Synchronization and server AI both disabled means no
  server startup connection.
  After explicit campaign activation, remembered synchronization may reconnect
  automatically; this does not authorize startup inference.

Acceptance: synthetic accounts cover registration, login, expiration, logout,
revocation, recovery and invalid credentials. Replay and brute-force tests enforce
the selected policy; one user's session cannot access another user's resources.

## S3 Campaign membership and authorization

- Add owner, player and read-only roles, expiring invitations and explicit acceptance.
- Define who may publish, edit, reshare, revoke and delete each item, including
  behavior when its original author leaves the campaign.
- Enforce authorization on every read, write, download, change feed and metadata
  response. UUIDs are identifiers, never access credentials.
- Keep security events minimal and content-free; separate operator administration
  from campaign ownership and document operator visibility under the chosen encryption.

Acceptance: two or three synthetic users exercise role changes, expired/reused
invitations, removal and cross-campaign access attempts. Read-only users cannot
write; list endpoints and error messages do not disclose private item metadata.

## S4 Selective sharing and attachments

- Define explicit shareable projections for pages, sessions, relationships, parties,
  images and selected sheet sections. Show the selected scope before first upload
  in the desktop client; selection defaults to private. Distinguish the owner
  account’s private synchronized campaign from projections shared with members.
  Synchronizing a whole campaign never grants members access to the whole copy.
- Validate allowlisted fields and dependencies. A shared link must not upload its
  private target, backlinks, embedded private fields or unrelated original files.
- Use managed attachment IDs, ownership, checksums and reference validation. Bound
  upload size, dimensions and decoding; allow validated PNG, JPEG and WebP only.
  Never accept client filesystem paths or serve uploads as executable content.
- Preserve original imported image bytes; authorize each download and account for
  storage quotas and incomplete upload cleanup.

Acceptance: inspect stored rows, responses and uploaded bytes for synthetic
private markers from unselected campaigns and fields. Owner-only synchronized
content remains unavailable to other members. Sharing one page or sheet section
exposes only the chosen projection. Attachment corruption, invalid formats and
unauthorized access fail safely.

## S5 Continuous local and server saving

- Introduce transactional changes with stable IDs, revisions, bounded cursors,
  idempotent requests and resumable incremental reads and writes.
- Integrate the desktop client through its existing Store and validation. Commit
  each autosave locally together with a durable outbound change before reporting
  local success. For an activated campaign, automatically send bounded, debounced
  changes while connected without requiring a manual sync button for every save.
- Acknowledge remote success only after the server transaction is durably committed.
  Show separate states for saved locally, syncing, saved locally and on server,
  offline/pending, conflict and failure, with retry and actionable errors. Never
  present a local-only save as already saved online.
- Keep both complete copies for the selected campaign scope, including managed
  attachments. Local edits and the outbound queue survive restarts and outages;
  reconnect automatically after activation, resume in order and retry idempotently.
  Remote changes reach active clients through an authenticated, bounded change
  notification channel followed by authorized incremental fetches; keep a bounded
  polling fallback. Notifications must not disclose private content.
- Define a measurable online save/notification latency target and maximum debounce
  during implementation, then test it. Network failure never blocks local editing.
  Pausing synchronization preserves the local copy and clearly shows pending
  changes; deleting the remote copy is a separate confirmed action.
- Reject stale writes and present both versions for explicit resolution. Preserve
  drafts and provenance; never use silent last-write-wins for user content.
- Document cursor expiration and safe resynchronization without uploading
  unselected content or resurrecting revoked items. Evaluate CRDT only for shared
  documents;
  it is not required for the first revision-based release.

Acceptance: concurrent edits on two clients, network interruption, duplicate
requests, crash recovery, pagination and expired cursors retain all acknowledged
changes or surface a recoverable conflict. Verify automatic online replication
after autosave, durable server acknowledgment, attachment completion and measured
latency. A server outage must leave recoverable local changes that appear on the
server after reconnection. Test the actual desktop integration. The two live copies
are replicas: deletion can propagate, so independent versioned backups remain required.

## S6 Revocation and deletion

- Stop access immediately on the server when sharing or membership is revoked.
  Remove revoked member access and member projections; retain the owner’s private
  synchronized copy unless its owner explicitly requests remote deletion. Remove
  deleted payloads and attachments when no authorized references remain; retain
  only necessary content-free tombstones.
- Deliver revocation to clients on their next synchronization. Clear shared cached
  copies while preserving independently owned local originals and warning about
  affected drafts. Prevent older offline writes from restoring revoked access.
- Define account/campaign deletion, last-owner handling and recovery boundaries.
  Explain that revocation cannot erase copies a recipient already exported or an
  offline device that has not reconnected. State backup retention separately.

Acceptance: offline recipient, returning former member, pending edits and stale
cursor scenarios cannot restore revoked items. Backup restore preserves revocations
and does not silently grant old sessions access again.

## S7 Security and operational recovery

- Test the authentication and authorization matrix, malformed inputs, injection,
  attachment traversal, resource exhaustion and limits with synthetic fixtures.
- Provide a self-hosted installation guide covering TLS, reverse proxy settings,
  secrets, volumes, upgrades, migrations, monitoring and recovery. Include the
  minimal Compose quick start, configurable ports, optional settings, security
  requirements and an example for an existing reverse proxy.
- Implement consistent backup and restore for the selected database and managed
  attachments, with integrity checks and the chosen encryption/key recovery policy.
  Server backups do not replace private desktop `.nodex` backups.
- Exercise restoration on a clean instance and a failed upgrade recovery path.
  Define safe behavior when newer migrations prevent running an older binary.

Acceptance: documented security results and limitations; successful restore of
synthetic campaigns, permissions, attachments and revocations; logs contain no
tokens or campaign content. Record tested host architectures separately.

## S8 Desktop integration and first release

- Complete application M24 acceptance with two or three users: invitations, partial
  sharing, offline edits, conflicts, revocation and reconnection.
- Verify the app remains fully usable offline with synchronization and server AI
  disabled and makes no server requests in that state. Preserve existing local backups and schema migrations.
- Publish versioned contract/types, compatibility table, release notes and actual
  verified container artifacts only after explicit release authorization. Use the
  confirmed Forgegit remote and registry; never overwrite tags or assets.
- Target Linux amd64 and arm64 container images where feasible; report untested
  architectures explicitly. Desktop installer publication follows the application
  repository's separate signing and platform requirements.

Acceptance: reproducible installation from the documented release, verified
container digests, end-to-end desktop evidence and no unresolved release-blocking
data-loss or authorization defects. Record supported versions and upgrade steps.

## S9 Android contract qualification

- Reuse the desktop account and scoped API for application M25. Start with campaign
  lists and read-only shared content, bounded offline caching and incremental updates.
- Validate session revocation, device management and logout cache deletion with
  Android Keystore credentials on a real device.
- Add mobile writes only after their conflict and permission behavior is tested.
  Local AI, training and full desktop backup/import remain outside this milestone.

Acceptance: application M25 real-device evidence, compatible API versions and no
additional private desktop content uploaded to enable mobile access.

## S10 Local or self-hosted server AI

- Provide an explicit per-campaign execution choice: disabled, local device or the
  selected self-hosted server. Local inference retains its fixed-loopback and
  cloud-disabled safeguards. Server mode uses a separate authenticated HTTPS
  adapter to the user-confirmed server; never repoint the local adapter remotely.
- Require explicit installation/enablement of the optional server inference
  Compose profile and explicit model selection. Preserve qualified model identity,
  weight checksums, language/edition policy and source citations; qualify each
  supported server hardware/model combination before claiming support.
- Keep model downloads operator-initiated, runtime access internal and campaign
  activation separate from runtime installation. No startup inference, third-party
  cloud fallback, automatic provider switch or implicit training.
- Before a request, show the execution location and permitted context scope.
  Retrieve only authorized sources for that user and campaign, including private
  owner content only for its owner. Never cross campaign or member boundaries;
  retain immutable source/revision citations and treat answers as untrusted drafts.
- Define prompt retention, cancellation, timeouts, queue bounds, per-user resource
  quotas and GPU/CPU concurrency limits. Logs contain no prompts or answers;
  transient inference data is not a new shared campaign record. Offline/server
  failure offers retry or an explicit switch to local AI, never a silent fallback.
- Server inference does not authorize uploading local training datasets, checkpoints
  or adapters. Campaign writes still require the existing explicit user workflow.

Acceptance: local mode sends no AI requests to the server; server mode actually
uses the server runtime and its resources. Test authorization, encrypted-content
consent, citations, cancellation, queue exhaustion, offline failure and independent
campaign settings with synthetic data. Verify Compose runtime ports are not public
and document actual hardware, model, memory and latency results.

## S11 Web campaign and server management

- Add an authenticated browser interface as a separate milestone, served through
  the same configured HTTPS origin and host port as the API.
- Let users inspect their synchronized campaigns, server save revisions, pending
  or failed operations known to the server, storage usage, membership, invitations
  and sharing scope. Only clients can report unsent local changes; the web UI must
  not claim to know a disconnected device's current save state.
- Provide confirmed actions for access revocation and remote campaign deletion,
  explaining effects on local replicas, offline devices and retained backups.
  Reuse the versioned API and permission checks; do not add a second data writer.
- Give operators a separate role for service health, quotas, accounts and optional
  AI service/queue status. Operator status alone does not grant campaign content
  access through the application; document host-level visibility according to the
  encryption policy. Never expose unrestricted shell, filesystem or Docker controls.
- Protect browser sessions, CSRF boundaries, origins and rendering of untrusted
  campaign text. Include keyboard access, loading/error/confirmation states and
  responsive layouts. Full browser campaign editing is outside this milestone.

Acceptance: synthetic owner, member, read-only user and operator cover the web
permission matrix, session expiry and confirmed destructive actions. Verify no
cross-account campaign metadata leaks, API/web behavior agrees, alternate host
ports work and the UI remains accessible at desktop and narrow widths.

## Delivery evidence

For each milestone, update `docs/STATUS.md` with what works, exact executed checks,
remaining limitations and the next step. Review the diff and whitespace, run the
relevant Rust, contract, security and integration checks, then commit only relevant
files with an English Conventional Commit and specific validation evidence.
Planning does not count as implemented M24 or M25, and elapsed dates alone do not
advance milestone status.
