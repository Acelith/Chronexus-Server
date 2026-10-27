# Backups and upgrades

The whole server is **one SQLite file** (accounts, campaigns, content, change log and
every uploaded original) in the `server-data` volume.

## Backup

```sh
docker compose exec -T server chronexus-server backup /var/lib/chronexus/backups/2026-10-27.sqlite3
docker compose exec -T server chronexus-server verify /var/lib/chronexus/backups/2026-10-27.sqlite3
```

`backup` makes a consistent copy while the server runs, verifies it and refuses to
overwrite. `verify` checks identity, schema, SQLite integrity, references and the SHA-256
of every image without changing anything. Copy backups off the host and encrypt them with
your own tools: the server holds no backup key.

## Restore

```sh
docker compose stop server
docker compose run --rm --no-deps server chronexus-server restore /var/lib/chronexus/backups/2026-10-27.sqlite3
docker compose up -d server
```

The file is verified, prepared on a copy (migrated if older), **every session is revoked**
and only then swapped in; the previous database is kept as `.pre-restore-<time>`. A failed
restore changes nothing. Users sign in again and repeat deletions made after the backup.

## Upgrade and rollback

Before a schema upgrade the server writes `<database>.pre-upgrade-v<N>`; migrations run in
one transaction. Recommended: `backup`, pull or build the new image, `up -d`, watch
`/health/ready`. To roll back, stop the service and run the **older** binary's `restore`
with the pre-upgrade file. An older binary refuses a newer database.
