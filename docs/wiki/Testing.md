# Testing

| Suite | Covers |
| --- | --- |
| `tests/accounts.rs` | Invitations, sign-in, limits, sessions, recovery |
| `tests/campaigns.rs` | Campaigns, invitations, roles, membership revisions |
| `tests/content.rs` | Items, visibility, projections, images, quotas, migrations |
| `tests/sync.rs` | Incremental sync, receipts, cursors, compaction |
| `tests/events.rs` | Notification streams and their limits |
| `tests/lifecycle.rs` | Removal, erasure, deletion, restored backups |
| `tests/security.rs` | Authorization matrix and hostile input |
| `tests/recovery.rs` | Backup, verify, restore, upgrade rollback |

```sh
cargo test --locked
```

Tests create temporary databases under `/private/tmp` when it exists, because database
paths may not pass through symbolic links (the macOS temp folder does).

**With the real desktop client:** the app repository runs its client against this
server's binary (`sync::tests::live`) and three real app instances against a Compose
deployment with HTTPS (`scripts/qa/sync/`, see its `docs/QA.md`).
