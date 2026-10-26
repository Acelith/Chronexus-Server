# Running Chronexus Server

This source build supports accounts, campaign membership, private and shared content,
managed images, continuous replication (incremental sync, idempotent writes and change
notifications), deletion and recovery. AI and the web interface are not implemented. It has
not been published as a release and has had no production deployment: read the security
summary and the limits at the end of this file before exposing it to anyone.

## Compose quick start

Requires Docker with Compose and an existing HTTPS reverse proxy. On a private
test host, from this repository:

```sh
cp .env.example .env
```

Set `CHRONEXUS_PUBLIC_ORIGIN` to the exact external HTTPS origin in `.env`, then:

```sh
docker compose up -d --build
docker compose exec -T server chronexus-server invite
```

The second command prints one server invitation, usable for 24 hours. Pass it to
the intended account holder through a private channel; it is a credential. It is
shown only by that explicit command and cannot be recovered from storage. Create
a new invite if it is lost. Never put it in Git, URLs or proxy logs.

In Chronexus, enter the HTTPS origin, verify the server and redeem the invitation
with a username, display name and password. Login/logout, campaign creation, campaign invitations and joining are supported.
Explicit page/session/relationship/party sharing is supported. Share referenced
pages first when publishing relationships or party entries; nothing is uploaded
implicitly. Image/selected-sheet/private-copy UI remains client integration work. The current desktop has
no UI for this server's extended recovery/device-management endpoints yet.

## Ports and reverse proxy

Defaults: `127.0.0.1:8080` on the host forwards to container port 8080. The bridge
subnet is `172.30.253.0/24`; its gateway `172.30.253.1/32` is the only default trusted
proxy address. If that subnet is already used, change the subnet, gateway and trust
setting together. Never widen proxy trust to `0.0.0.0/0` or `::/0`.

Optional `.env` overrides:

```dotenv
CHRONEXUS_HOST_PORT=18080
CHRONEXUS_INTERNAL_PORT=8181
CHRONEXUS_BIND_ADDRESS=127.0.0.1
```

The host proxy connects to `http://127.0.0.1:18080`. Internal-port overrides are
automatically reflected in the listener, mapping and health check. The public
origin can also contain a nonstandard port, such as `https://sync.example.org:8443`.
That external port belongs to the HTTPS proxy, independently of the application port.

The proxy must validate TLS and **overwrite** these upstream headers:

- `Host`: the configured public authority, including its nonstandard port.
- `X-Forwarded-Proto: https`.
- `X-Forwarded-For`: exactly one validated client IP. Do not append a chain or trust
  an incoming client-supplied value. In a multi-proxy deployment, configure trusted
  upstream client-IP handling at the final proxy deliberately.

No HTTP redirect is followed by the desktop. Requests must reach the exact origin
and path. For a proxy in another container/network, explicitly configure the
reachable network and trusted socket peer; do not expose the backend publicly as
a workaround. The host-gateway default is for a proxy running on the Docker host.
Check the actual topology on your host; Docker Desktop and remote hosts can differ.

The image runs as UID/GID 10001 with a read-only root, dropped capabilities and a
single writable data volume. No database or AI ports are published. API error
responses deliberately omit raw request/SQL details. Request content and credentials
are never logged by the application; configure your proxy with the same policy.

## Accounts and recovery

Usernames use 3–64 ASCII letters/digits or `.`, `_`, `-`, normalized to lowercase.
Display names and device labels use 1–100 characters. Passwords use 12–128 characters
and at most 512 UTF-8 bytes, without control characters. Requests reject unknown fields.

Authentication attempts are limited to five per account/code and thirty per client
IP in fifteen minutes; successful attempts count too. Limits persist across restarts.
When limited, wait for the current window to expire. Multiple users behind the same
IP share its limit. Sessions have seven days of inactivity expiry and thirty days
absolute expiry; sign in again when required. Twenty active sessions are allowed.

To assist an account holder who has lost access, verify their identity outside
the service and run:

```sh
docker compose exec -T server chronexus-server recover USERNAME
```

This prints a one-use recovery code, valid for one hour. Redeem it with
`POST /api/v1/auth/recover` and JSON fields `recoveryCode`, `password`, `device`.
Recovery changes the password, invalidates every old session and issues a new
session in one transaction. A newly issued recovery code invalidates earlier
unused recovery codes for the account. The Docker/host operator is a trusted
administrator; protect volume and CLI access accordingly.

Authenticated device management:

| Endpoint | Action |
| --- | --- |
| `GET /api/v1/auth/sessions` | List your active session IDs and device labels, without tokens |
| `DELETE /api/v1/auth/sessions/{sessionId}` | Revoke one of your sessions |
| `DELETE /api/v1/auth/sessions` | Revoke all your sessions, including the current one |
| `POST /api/v1/auth/logout` | Revoke the presented Bearer token |

## Storage and restart

`server-data` persists one SQLite database (accounts, campaigns, content, change logs,
receipts and every uploaded original as a BLOB) with its WAL files at `/var/lib/chronexus`.
Do not map it to an application's database, a shared/network SQLite file or an untrusted
writable directory. Database paths and SQLite companion files must not be symbolic links.
New Unix data directories use mode 0700 and database files 0600; protect existing volumes too.

`docker compose down` stops the service without deleting data; `--volumes` deletes it.
Schema versions are recorded in the database; an older binary refuses a newer archive
("Database schema is newer than this server") and changes nothing.

## Production deployment

1. **TLS and reverse proxy.** The server speaks plain HTTP to a proxy you control and refuses
   to run otherwise (loopback HTTP is a development exception). The proxy terminates TLS with a
   valid certificate for the exact public origin and must **overwrite** `Host`,
   `X-Forwarded-Proto: https` and a single `X-Forwarded-For`. Example for an existing Caddy:

   ```caddyfile
   sync.example.org {
       reverse_proxy 127.0.0.1:8080 {
           header_up Host {host}
           header_up X-Forwarded-Proto https
           header_up X-Forwarded-For {remote_host}
       }
   }
   ```

   Example for nginx (inside the `server` block of the TLS site):

   ```nginx
   location / {
       proxy_pass http://127.0.0.1:8080;
       proxy_set_header Host $host;
       proxy_set_header X-Forwarded-Proto https;
       proxy_set_header X-Forwarded-For $remote_addr;
       proxy_buffering off;            # change notifications are a long-lived stream
       proxy_read_timeout 15m;
   }
   ```

   Keep the proxy's access log free of `Authorization` headers and request bodies. Set
   `CHRONEXUS_PUBLIC_ORIGIN` to the exact HTTPS origin and `CHRONEXUS_TRUSTED_PROXIES` to the
   proxy's address as the container sees it. Never trust `0.0.0.0/0`.
2. **Secrets.** The server has no static secret to store: passwords are Argon2id hashes, session
   tokens and invitation codes are random and only their hashes are kept. Invitation and recovery
   codes are shown once by the CLI; treat them as credentials. The `.env` file holds only
   addresses and ports. Protect the data volume and the CLI: the operator is a trusted administrator.
3. **Volumes.** One named or bind-mounted volume at `/var/lib/chronexus`, on storage with
   working `fsync` (SQLite runs with full durability). Keep backups on different storage.
4. **First account.** `docker compose exec -T server chronexus-server invite` prints a 24-hour
   invitation; the first person redeems it in the desktop app.
5. **Monitoring.** `GET /health/live` and `/health/ready` (loopback only, also used by the
   Compose healthcheck) report process and database health. Alert on repeated unhealthy checks,
   on 5xx rates and 429 bursts at the proxy, on free space of the data volume and on a failing
   nightly backup. The server writes no access log and never logs content or credentials.

## Backup and restore

One file is a complete backup. Commands work on the database named by `CHRONEXUS_DATABASE`:

```sh
# consistent copy while the server runs (VACUUM INTO), verified before success is reported
docker compose exec -T server chronexus-server backup /var/lib/chronexus/backups/2026-10-26.sqlite3
# check any file without changing anything: identity, supported schema, SQLite integrity,
# foreign keys and the SHA-256 of every uploaded image
docker compose exec -T server chronexus-server verify /var/lib/chronexus/backups/2026-10-26.sqlite3
```

`backup` refuses to overwrite and removes a copy that fails verification. Copy the file off the
host. **Encryption and keys:** the server neither encrypts backups nor holds any backup key.
Encrypt with your own tooling (an encrypted volume, `age` or `gpg`) and keep that key apart from
the backup; losing it makes the backup unreadable and the server cannot help. Server backups do
not replace users' private desktop `.nodex` backups.

Restore (also onto a clean instance), with the service **stopped**:

```sh
docker compose stop server
docker compose run --rm --no-deps server chronexus-server restore /var/lib/chronexus/backups/2026-10-26.sqlite3
docker compose up -d server
```

`restore` verifies the file first, copies it beside the database, checks the copy, keeps the
current database as `.pre-restore-<time>` (with its journal), replaces it atomically, applies any
newer migrations and **revokes every session**: a backup also contains sessions that were revoked
later and data users deleted later, so everyone signs in again and deletions made since the backup
must be repeated by their owners. A refused or failed restore changes nothing. Practise a restore on
a scratch instance before you need it.

## Upgrades and rollback

Before any schema upgrade the server copies the existing database to
`<database>.pre-upgrade-v<N>` (a consistent, verifiable file) and does not start the upgrade if
that copy cannot be written. Migrations run in one transaction: a failed upgrade rolls back and
the database stays at its old schema. To roll back after a bad upgrade, stop the service, run the
**older** binary's `restore` with the pre-upgrade file (an older binary refuses a newer archive),
and start it. Recommended procedure: `backup`, pull the new image, `up -d`, watch
`/health/ready`, keep the previous image until the new one has served real traffic.

## Security summary (S7)

Tested on Windows x64 in `tests/security.rs` and `tests/recovery.rs` with synthetic data:
every campaign route against anonymous, outsider, reader, player and owner principals (refusals
never disclose the campaign); malformed, duplicated and wrong-scheme credentials, tokens in the
query string or cookies; hostile identifiers and queries (SQL-shaped, traversal-shaped, NUL,
non-ASCII); syntax errors, unknown fields, control characters, a 5,000-level nesting bomb and
oversized bodies; script/SVG/HTML/oversized/spoofed uploads; error bodies without internals;
wrong password and unknown account are indistinguishable; backup, verification, tampering and
corruption detection, restore on a clean instance, failed-upgrade recovery.
Not done: Docker/Linux/macOS runs of these tests, a third-party penetration test, image and
dependency vulnerability scans (run `cargo audit` and an image scanner in CI before release),
load testing beyond the built-in limits, and real TLS/DNS.

## Development and checks

See [DEVELOPMENT.md](DEVELOPMENT.md), [API.md](API.md) and [STATUS.md](STATUS.md).
No registry image or server release has been published by this change.

## Operator commands for lifecycle and recovery (S6)

Run with the same database environment as `invite` and `recover`:

```sh
docker compose exec -T server chronexus-server revoke-sessions
docker compose exec -T server chronexus-server compact 180
```

`revoke-sessions` ends every session: run it after restoring an older backup, which would
otherwise bring back sessions that were revoked later. `compact DAYS` drops tombstones older
than the given days (default 180); devices that were offline longer must list the campaign
again. Deleting a campaign or an account is done by the user through the API (see
[API.md](API.md)); the operator's backups keep deleted data until the operator's own retention
removes it.
