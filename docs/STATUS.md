# Chronexus Server status

Updated: 26 October 2026.

## Current state

The repository contains introductory documentation and a server roadmap covering
S0 through S11. No service, database schema, HTTP contract, container or account
system has been implemented. Application M24 and M25 remain planned.

The revised plan includes continuous local/server saving after explicit campaign
activation, private owner replicas separate from member sharing, explicit local
or server AI (S10), minimal Docker Compose configuration with configurable ports,
and authenticated web management (S11). These are planned requirements, not
implemented features. Server AI extends the original local-only plan; the local
adapter keeps its loopback boundary.

## Next milestone

S0: confirm hosting, registration, encryption, persistence, account security,
retention and server AI decisions in `ROADMAP.md`, after checking the application
M23 audit gate.
Record confirmed choices in `docs/DECISIONS.md` before implementing S1.

## Verification

- Planning expands application M24/M25 with the user's explicit continuous-save,
  self-hosted AI, Compose and web-management requirements.
- Local Markdown links, S0–S11 coverage and requirement consistency checked.
- `git diff --check`: passed for the planning change.
- Runtime tests, builds and deployments: not run; no implementation exists.
- Skill used: `pages:write-page` for documentation structure and editorial review,
  using repository Markdown as the destination.

No builds, remote publication or infrastructure changes were performed.
