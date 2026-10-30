# Architecture

| Module | Role |
| --- | --- |
| `main.rs` | CLI: `serve`, `invite`, `recover`, `backup`, `verify`, `restore`, `revoke-sessions`, `compact`, `openapi` |
| `config.rs` | Environment configuration, public origin, trusted proxies |
| `routes.rs` | Router, meta/health, auth routes, OpenAPI assembly |
| `auth.rs` | Accounts, Argon2id, sessions, invitations, attempt limits, recovery |
| `campaigns.rs`, `campaign_routes.rs` | Campaigns, invitations, members, roles |
| `content.rs`, `content_routes.rs`, `payload.rs` | Items, visibility, revisions, receipts, change log, `/sync` |
| `attachments.rs` | Image uploads and downloads with checksums |
| `vtt.rs`, `uvtt.rs` | Table scenes, original map assets, grid/movement, templates and filtered live snapshots |
| `combat.rs` | Explicit initiative/turns, conditions, owner-only vitals and audience-filtered logs |
| `events.rs` | Content-free change notifications (SSE) |
| `lifecycle.rs` | Deletion of campaigns, own content and accounts |
| `backup.rs` | Backup, verification, restore |
| `db.rs` | Database opening, safe paths, migrations `001`–`012` |
| `error.rs` | Error type; responses without internals |

**Request flow:** proxy → origin/forwarded-header checks → bearer session → membership and
role check → database work on a blocking thread in one transaction → JSON response.

**Database:** SQLite in WAL mode with full durability; the server identifies its archive
with an application ID and refuses foreign or newer databases. Every write that changes
visibility also appends to the change log read by `/sync`.
