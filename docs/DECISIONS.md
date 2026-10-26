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
