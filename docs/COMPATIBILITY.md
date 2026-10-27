# Compatibility and upgrades

## Server and desktop versions

The desktop checks `GET /api/v1/meta` before saving a server address and refuses a server
without the capabilities it needs (`private-content`, `idempotent-mutations`,
`incremental-sync`; `change-notifications` is used when present).

| Server | API | Schema | Capabilities | Desktop versions | Notes |
| --- | --- | --- | --- | --- | --- |
| 0.1.0 | v1 | 006 | accounts, sessions, campaigns, sharing, private-content, attachments, idempotent-content-saves, idempotent-mutations, incremental-sync, change-notifications, deletion | 0.9.0 – 0.10.1 | First release (Windows x64 binary) |
| 0.1.1 | v1 | 006 | same as 0.1.0 | 0.9.0 – 0.10.1 | Container images (linux/amd64, linux/arm64), backup/compose fixes; no API change |

Feature notes:

- **Member management** (list, role change, removal, leaving) uses endpoints present since
  server 0.1.0; the desktop offers it from **0.10.0**. Older desktops simply do not show it.
- **Role refresh** at every synchronization (desktop 0.10.0+) reads `GET
  /api/v1/campaigns/{id}`, available since server 0.1.0.
- Desktop 0.8.x and older have no sharing client.

Every server release keeps API v1 compatible: new behaviour is announced by a capability,
never by changing an existing response. A future breaking change would use a new API
version and a new row here.

## Upgrading the server

1. **Back up** while it runs:
   `docker compose exec -T server chronexus-server backup /var/lib/chronexus/backups/before-upgrade.sqlite3`
   and copy the file off the host.
2. **Get the new version**: `git pull` and `docker compose build`, or set `CHRONEXUS_IMAGE`
   to the published image of the new version (pin it by digest) and `docker compose pull`.
3. **Restart**: `docker compose up -d`. A schema upgrade first writes
   `<database>.pre-upgrade-v<N>` and runs in one transaction; it changes nothing if it fails.
4. **Check** `docker compose ps` (healthy) and `GET /api/v1/meta` (new `serverVersion`).
5. **Roll back** if needed: stop the service and run the **older** binary's `restore` with
   the backup or the pre-upgrade file (see INSTALLATION.md, *Upgrades and rollback*).

From 0.1.0 to 0.1.1 there is no schema change (006 → 006). Users do not need to update the
desktop app at the same time; desktops from 0.9.0 keep working.
