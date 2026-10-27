# Security

## Design

- HTTPS only, through a proxy you control; the server refuses to run behind an untrusted
  proxy setup. No redirects are followed by clients.
- Passwords: Argon2id. Tokens and invitation codes: random, only hashes stored. No static
  secret on disk.
- Container: non-root (UID 10001), read-only root filesystem, all capabilities dropped,
  one writable volume, no database port published.
- Every campaign route checks membership and role; refusals do not reveal whether a
  campaign exists. Requests reject unknown fields; bodies, nesting and uploads are
  bounded; uploads are validated PNG/JPEG/WebP only.
- No access log; content, passwords and tokens are never logged.

## Tested

Role matrix over every campaign route, credential abuse, hostile identifiers and bodies,
hostile uploads, error leakage, indistinguishable login failures, backup tampering and
corruption, restore and failed-upgrade recovery (`tests/security.rs`, `tests/recovery.rs`).
On 27 October 2026 the full suite also passed natively on macOS, the Compose stack ran in
Docker with HTTPS through the bundled Caddy, and three real desktop apps (macOS and Linux)
completed the sharing scenario against it.

## Not yet done

Third-party penetration test, image and dependency vulnerability scans in CI, load
testing, a production deployment with public TLS and DNS. **No end-to-end encryption:**
the operator can read stored data.

Report vulnerabilities privately to the maintainer, not in public issues.
