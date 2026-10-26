# Running Chronexus Server through S4

This source build supports accounts, campaign membership, explicit four-kind
sharing and private content/image APIs. Continuous saving, new image/sheet selection
UI, AI and web management are not implemented. Do not point users at it as a complete synchronization service or replace an existing deployment.

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

`server-data` persists the independent server SQLite database, original image BLOBs
and its WAL files at
`/var/lib/chronexus`. Do not map this to an application's database, a shared/network
SQLite file or an untrusted writable directory. Database paths and existing SQLite
companion files must not be symbolic links. Newly created Unix data directories
use mode 0700 and database files 0600; protect permissions of existing volumes too.

Container recreation preserves the volume. `docker compose down` stops the test
service without deleting data; adding `--volumes` deletes its data. Keep independent
copies before manual operations. The tested automated backup/restore workflow is
planned in S7; this account-only slice is not production-qualified recovery tooling. Schema 003 validates a server-specific
archive identity before changing journal mode or applying migrations; older server
binaries refuse the newer schema. Use a consistent whole-database backup rather
than copying an active SQLite file alone.

## Development and checks

See [DEVELOPMENT.md](DEVELOPMENT.md), [API.md](API.md) and [STATUS.md](STATUS.md).
No registry image or server release has been published by this change.
