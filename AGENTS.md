# Chronexus Server engineering instructions

Maintain the optional self-hosted companion to Chronexus. Read ROADMAP.md and
docs/STATUS.md before domain changes; docs/DECISIONS.md records confirmed choices.
English is the source language. Preserve the desktop's local-first behavior.

- Run `git status --short` before editing. Preserve unrelated changes.
- Rust/axum with local SQLite migrations; parameterized SQL, foreign keys and
  transactions. Never write the desktop database or accept arbitrary file paths.
- The current desktop reference is Chronexus docs/SYNC_API.md and sync/api.rs.
  Generate this repository's OpenAPI and TypeScript schemas from implemented code.
  Never claim the full sharing contract works before M25.3–M25.8 acceptance.
- Registration is invitation-only. HTTPS is terminated by explicitly trusted
  proxies; direct HTTP is an explicit loopback-only development exception.
- Hash passwords with Argon2id; persist only hashes of random tokens/codes.
  Never log credentials, prompts or campaign content. Fail closed on bad config.
- Accounts and private synchronized campaigns do not imply sharing with members.
  No third-party AI, automatic downloads, startup inference or training.
- Use synthetic fixtures only. Ignore databases, secrets, local data and generated
  build output. Do not copy personal campaigns or backups into this repository.
- Before a coherent implementation commit: review status, diff, diff --check and
  staged changes; run cargo fmt --check, clippy --all-targets -- -D warnings,
  cargo test, release build and contract generation checks. Validate Compose when
  changed; report unavailable container/platform checks honestly.
- Commit coherent changes with an English Conventional Commit title and a specific
  body covering reason, changes, validation, compatibility and limitations.
- Update docs/STATUS.md after milestones. Publish only with authorization; never
  force-push or overwrite tags/assets. Planning and partial slices are not releases.
- Consult relevant available Skills before new activities; report actual use.

## GitHub migration — 27 October 2026

Canonical origin: https://github.com/Acelith/Chronexus-Server.git. The user explicitly
authorized the one-time history sanitization; private recovery bundles are external
to both projects. Future work must keep normal history protections. Run
`python3 scripts/check-public-origin.py --history --origin` before publication.
Build containers locally until a GHCR image is explicitly published and verified.
