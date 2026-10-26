# Implemented API and desktop compatibility

Server source version 0.1.0 implements the **account/session subset** of the
Chronexus sync protocol v1. This is not a released server version. The original
desktop source 0.8.1 has an M24 client; its full campaign protocol is documented in
[Chronexus SYNC_API.md](https://github.com/Acelith/Chronexus/blob/main/docs/SYNC_API.md).
The application now identifies server delivery as M25 and Android as M27.

Generated artifacts for the implemented subset:

- [OpenAPI](../api/openapi.json), also `GET /api/v1/openapi.json`.
- [TypeScript schemas](../api/types.ts), generated from the same OpenAPI document.

The current desktop stores its session in `sync-account.json` outside backups.
This delivery does not migrate it to secure OS credential storage or certify that
client-side storage policy; protect the desktop profile and track that hardening
in the application before broader deployment.

The partial schema must not replace the desktop's remaining sharing contract.
Campaign creation, campaign invitations/membership, changes, item writes and image
upload have not been implemented. Unknown routes return 404, never synthetic success.

| Route | Authentication | Status |
| --- | --- | --- |
| `GET /api/v1/meta` | Public, transport checks apply | Implemented; `api=chronexus-sync`, `version=1`, capabilities `accounts`, `sessions` |
| `POST /api/v1/auth/redeem` | Server invitation | Implemented; camelCase request/response compatible with original client |
| `POST /api/v1/auth/login` | Username/password | Implemented; compatible with original client |
| `POST /api/v1/auth/logout` | Bearer token | Implemented; 204 on success |
| `POST /api/v1/auth/recover` | Operator-issued recovery code | Extension; no current desktop UI |
| `GET /api/v1/auth/sessions` | Bearer token | Extension; current account only |
| `DELETE /api/v1/auth/sessions/{sessionId}` | Bearer token | Extension; own device only |
| `DELETE /api/v1/auth/sessions` | Bearer token | Extension; all own sessions |
| `GET /health/live`, `GET /health/ready` | Direct loopback probe or checked proxy | Implemented |

Successful login/redemption/recovery responds with `token`, `accountId`, `username`,
`displayName`. IDs are stable UUIDs; tokens are 64 lowercase hex characters from
32 cryptographically random bytes. Dates use RFC 3339 UTC. The API uses JSON,
rejects unknown request fields and caps auth bodies at 8192 bytes.

Failures use `{ "error": "safe English description" }`: 400 validation, 401 invalid
credentials, 403 transport/origin refusal, 404 absent/other-account session, 409
unavailable username, 410 used/expired invitation, 413 oversized body, 429 attempt
limit (with `Retry-After`), 503 worker saturation or 500 generic storage failure.
The desktop maps status codes to its own localized messages.

The proxy must overwrite client IP and protocol headers; credentials use Bearer,
never URLs or cookies. All responses use `Cache-Control: no-store`. See
[INSTALLATION.md](INSTALLATION.md) for exact proxy and account policies.
