# Running Chronexus Server

This source build supports accounts, campaign membership, private and shared content,
managed images, continuous replication (incremental sync, idempotent writes and change
notifications), deletion and recovery. AI and the web interface are not implemented. It has
not been published as a release and has had no production deployment: read the security
summary and the limits at the end of this file before exposing it to anyone.

## Quick start (anyone can run it)

You need Docker with Compose and a domain name whose DNS points at the machine. Everything is
driven by `.env`; the only value you must set is your domain.

```sh
git clone https://github.com/Acelith/Chronexus-Server.git && cd Chronexus-Server
cp .env.example .env            # set CHRONEXUS_DOMAIN=sync.your-domain.example
```

**Mode A: bundled HTTPS (nothing else to install).** Ports 80 and 443 must reach this host; the
bundled Caddy obtains and renews the certificate by itself:

```sh
docker compose --profile https up -d --build
docker compose exec -T server chronexus-server invite
```

**Mode B: you already run a reverse proxy** (Caddy, nginx, Traefik...): leave the profile out,
`docker compose up -d --build`, and point your proxy at `127.0.0.1:8080` as described under
*Production deployment*.

Build from the checked-out GitHub source with `--build`. A GHCR image has not yet
been published and verified; keep the local image default. `CHRONEXUS_IMAGE` remains
available for an image you explicitly choose and verify. Ports, network and image
are configured through `.env`.

The `invite` command prints one server invitation, usable for 24 hours. Pass it to the intended
account holder through a private channel; it is a credential, shown only by that command and not
recoverable from storage. Create a new one if it is lost. Never put it in Git, URLs or proxy logs.

In Chronexus, enter `https://your-domain`, verify the server and redeem the invitation with a
user name, display name and password.

## Ports and reverse proxy

### Local network by IP address

For a separate LAN host at `192.168.162.16`, copy the source checkout (including
`Dockerfile`) to that host and use `compose.lan-standalone.yaml`. It is a complete
Compose stack: server, HTTPS proxy, inline Caddy configuration, network and
persistent volumes. It does not need `.env` or another Compose/Caddy file.
Docker Compose 2.23.1 or later is required for its inline configuration. Change
the IP in the file if the host uses another address.

On that host, run:

```sh
docker compose -f compose.lan-standalone.yaml config --quiet
docker compose -f compose.lan-standalone.yaml up -d --build
```

This stack uses Caddy's internal CA. After the proxy starts, copy its **root
certificate** to a safe location and install it as a trusted root on every
desktop that will connect. The certificate is public; never export the CA's
private key. Keep the `proxy-data` volume, which holds the CA, across restarts.

```sh
docker compose -f compose.lan-standalone.yaml cp proxy:/data/caddy/pki/authorities/local/root.crt ./chronexus-lan-root.crt
```

Check `https://192.168.162.16/api/v1/meta` from a client after trusting the certificate,
then use that exact HTTPS origin in Chronexus. A certificate warning means the
client is not ready; do not bypass it. LAN firewall rules should allow TCP 443
only from the intended local network. If the host IP changes, update the YAML
and trust the renewed certificate for the new address.

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

## Web interface (M25.11)

Open the public origin (for example `https://sync.example.org/`) in a browser and sign in with
a Chronexus account. Writing stays in the desktop app; the web interface manages what lives on
the server:

- **Campaigns**: your role, members, what you keep on the server (items per kind, highest
  revision, last accepted save, images) and the server's limits. It cannot know changes a
  device has not sent yet.
- **Owners**: change members between player and reader, remove members, create and revoke
  invitations, delete the campaign on the server (typing its exact name). Every destructive
  action explains its effect on local copies, offline devices and existing backups first.
- **Players and readers**: leave a campaign.
- **Devices**: list signed-in devices and sign them out.
- **Operators**: user names listed in `CHRONEXUS_OPERATORS` see service status and account
  metadata (sessions, campaign counts, stored bytes). The role grants no access to campaign
  content in the interface or the API; the host operator can still read the database file.

Security: the pages are static files served by the server with a strict Content Security
Policy (own scripts and styles only). They use the same JSON API and permission checks as the
desktop with a Bearer token kept in the browser tab's session storage — no cookies, so no
cross-site request forgery; requests from another origin are refused. Untrusted text is
always shown as text. Close the tab or sign out to end the session.

## Optional server AI (M25.10)

Off by default. With `docker compose --profile ai up -d` a local Ollama runtime runs on an
internal network without internet access; models are pulled by you and named in
`CHRONEXUS_AI_MODELS`. Members then choose per campaign in the desktop whether the assistant
runs locally or on the server. Settings, limits, safeguards and the open GPU qualification:
[AI.md](AI.md).

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

`restore` verifies the file first (including the complete table structure), then works on a copy
beside the database: it applies any newer migrations, **revokes every session** and checks the
copy's structure and integrity again. Only then does it keep the current database as
`.pre-restore-<time>` (with its journal) and swap the copy in atomically; if the swap fails, the
previous database is put back. Sessions are revoked because a backup also contains sessions that
were revoked later and data users deleted later, so everyone signs in again and deletions made since
the backup must be repeated by their owners. A refused or failed restore changes nothing. Practise a
restore on a scratch instance before you need it.

## Upgrades and rollback

Supported desktop versions per server release and step-by-step upgrades:
[COMPATIBILITY.md](COMPATIBILITY.md).

Before any schema upgrade the server copies the existing database to
`<database>.pre-upgrade-v<N>` (a consistent, verifiable file) and does not start the upgrade if
that copy cannot be written. Migrations run in one transaction: a failed upgrade rolls back and
the database stays at its old schema. To roll back after a bad upgrade, stop the service, run the
**older** binary's `restore` with the pre-upgrade file (an older binary refuses a newer archive),
and start it. Recommended procedure: `backup`, pull the new image, `up -d`, watch
`/health/ready`, keep the previous image until the new one has served real traffic.

## Security summary (M25.7)

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
GitHub release 0.1.1 contains Linux binaries. Container deployment currently builds locally; GHCR publication is pending.

## Operator commands for lifecycle and recovery (M25.6)

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
