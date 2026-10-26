# Chronexus Server roadmap

Updated: 26 October 2026. Status: planning only; no server implementation.

Chronexus Server will provide optional, self-hosted sharing between Chronexus
players. The desktop remains local-first and fully usable without an account or
server. Only explicitly selected content may be synchronized. This repository
owns the server, its HTTP contract, releases and container images; the Chronexus
repository owns desktop and Android clients.

This roadmap expands the application's M24 milestone without renumbering it.
Application M23 remains the prerequisite for M24 implementation; M25 Android
depends on a qualified server and compatible desktop synchronization.

## Product boundaries

- Rust service, Docker deployment on user-controlled infrastructure, HTTPS.
- No telemetry, automatic campaign upload, hosted AI or required third-party cloud.
- All content starts private. Account access or campaign membership alone does
  not grant access to every item or every field of a character sheet.
- Preserve stable UUIDs, attribution, source identity and revision history.
  Shared player claims remain attributed claims, never established world facts.
  Mentions and asserted relationships remain distinct; retain relationship certainty.
- Local AI models, conversations, training material, adapters, preferences and
  complete local backups stay outside the synchronization payload. Notes, sheets
  and attachments require explicit selection of the permitted content.
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
| S5 | Incremental synchronization and visible conflicts | S4 |
| S6 | Revocation, deletion and recovery | S5 |
| S7 | Security qualification and operational recovery | S6 |
| S8 | Desktop integration and first server release | S7 and desktop M24 acceptance |
| S9 | Android contract qualification | S8 and application M25 |

## S0 Deployment and security decisions

Confirm these choices before implementing the server:

| Decision | Required outcome |
| --- | --- |
| Hosting | Domain, host OS and CPU architecture, reverse proxy, TLS ownership, persistent storage and operator responsibilities |
| Registration | Invitation-only or open registration; first administrator bootstrap and account recovery policy |
| Encryption | Explicit decision on end-to-end encryption, including key ownership, device enrollment, recovery and metadata visibility |
| Persistence | Choose SQLite or PostgreSQL against expected concurrent users, deployment and backup needs |
| Account security | Choose passkeys, robust passwords or both; define recovery without assuming an email delivery service |
| Retention | Agree on deletion, change-log retention, backup retention, quotas and offline-device behavior |

Record decisions and reasons in `docs/DECISIONS.md`. Encryption must be decided
before fixing the payload and attachment formats: HTTPS alone does not provide
end-to-end encryption. Any proposed implementation dependencies need current
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

Acceptance: clean startup and restart preserve a synthetic database; migrations,
configuration failures, health checks and contract generation are tested. Record
the first supported app/server version range, without inventing released versions.

## S2 Accounts and sessions

- Implement the S0 registration and recovery policy, credential protection,
  authentication attempt limits and expiring/revocable sessions.
- Support distinct devices and revoking one device or all sessions. Store no raw
  reusable credentials in logs; server secrets stay outside Git and images.
- Document client credential storage requirements for desktop secure storage
  and Android Keystore. Sharing disabled means no client startup connection.

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
  in the desktop client; selection defaults to private.
- Validate allowlisted fields and dependencies. A shared link must not upload its
  private target, backlinks, embedded private fields or unrelated original files.
- Use managed attachment IDs, ownership, checksums and reference validation. Bound
  upload size, dimensions and decoding; allow validated PNG, JPEG and WebP only.
  Never accept client filesystem paths or serve uploads as executable content.
- Preserve original imported image bytes; authorize each download and account for
  storage quotas and incomplete upload cleanup.

Acceptance: inspect stored rows, responses and uploaded bytes for synthetic
private markers. Sharing one page or sheet section exposes only the chosen
projection. Attachment corruption, invalid formats and unauthorized access fail safely.

## S5 Incremental synchronization and conflicts

- Introduce transactional changes with stable IDs, revisions, bounded cursors,
  idempotent requests and resumable incremental reads and writes.
- Integrate the desktop client through its existing Store and validation. Local
  edits persist offline; pending writes survive restart and retry without duplication.
- Reject stale writes and present both versions for explicit resolution. Preserve
  drafts and provenance; never use silent last-write-wins for user content.
- Document cursor expiration and safe resynchronization without uploading private
  content or resurrecting revoked items. Evaluate CRDT only for shared documents;
  it is not required for the first revision-based release.

Acceptance: concurrent edits on two clients, network interruption, duplicate
requests, crash recovery, pagination and expired cursors retain all acknowledged
changes or surface a recoverable conflict. Test the actual desktop integration.

## S6 Revocation and deletion

- Stop access immediately on the server when sharing or membership is revoked.
  Remove revoked payloads and attachments from active server storage when no
  authorized references remain; retain only necessary content-free tombstones.
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
  secrets, volumes, upgrades, migrations, monitoring and recovery.
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
- Verify the app remains fully usable offline with sharing disabled and makes no
  sharing requests in that state. Preserve existing local backups and schema migrations.
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

## Delivery evidence

For each milestone, update `docs/STATUS.md` with what works, exact executed checks,
remaining limitations and the next step. Review the diff and whitespace, run the
relevant Rust, contract, security and integration checks, then commit only relevant
files with an English Conventional Commit and specific validation evidence.
Planning does not count as implemented M24 or M25, and elapsed dates alone do not
advance milestone status.
