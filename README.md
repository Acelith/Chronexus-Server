<div align="center">

<img src="assets/brand/chronexus-wordmark.webp" alt="Chronexus" width="520">

# Chronexus Server

**Your campaign, on your device and your server. Connected on your terms.**

Keep a local and private online copy of your campaign, choose what other players
can see, and decide whether AI runs on your computer or your own server.

[![Status](https://img.shields.io/badge/status-in%20development-b89b60)](ROADMAP.md)
[![Deployment](https://img.shields.io/badge/deployment-Docker%20Compose-354b3e)](#deployment-and-configuration)
[![Hosting](https://img.shields.io/badge/hosting-self--hosted-6c302b)](#privacy-and-data)

[Chronexus](https://github.com/Acelith/Chronexus) · [Features and roadmap](#features-and-roadmap) · [Roadmap](ROADMAP.md) · [Status](docs/STATUS.md)

</div>

> [!NOTE]
> The service foundation, accounts, campaign membership and selective-sharing APIs (S1–S4) are implemented in source.
> Continuous desktop saving, new image/sheet selection UI, AI and web management remain planned.
> Build locally with Compose; no server release has been published.

## What is Chronexus Server

Chronexus Server is the self-hosted companion under development for
[Chronexus](https://github.com/Acelith/Chronexus), the offline-first desktop
journal and campaign manager for tabletop RPG players. It adds optional accounts,
continuous campaign synchronization and selective sharing on infrastructure you
control. The original desktop app remains usable without a server or account.

The projects have separate repositories and releases: this repository owns the
server and its versioned API; the [original Chronexus repository](https://github.com/Acelith/Chronexus)
owns the desktop app and planned Android client. Synchronization belongs to
the M24 client contract; the server is application M25 and Android follows in M27.

## Why Chronexus Server

The delivery goal:

- **Two durable copies.** Once you enable campaign synchronization, edits save
  locally and automatically reach your server while connected. Offline changes
  wait safely and resume synchronization when the connection returns.
- **Private before shared.** Your online copy belongs to your account. Choose
  individual content or sheet sections to share with campaign members.
- **Your resources, your choice.** Keep AI on your device or explicitly use the
  CPU/GPU resources of your self-hosted server.
- **The player's point of view.** Shared claims retain their author, source and
  certainty. Mentions do not become factual relationships or reveal GM secrets.
- **Simple to operate.** A Docker Compose setup with safe defaults, configurable
  ports and only essential security settings required from the operator.

## Features and roadmap

| Feature | In short | Milestone | Status |
| --- | --- | --- | --- |
| **Accounts and devices** | Authentication, revocable sessions and account recovery under the selected registration policy | S2 | Implemented |
| **Campaign membership** | Invitations, owner/player/read-only roles and authorization on every item | S3 | Implemented |
| **Selective sharing** | Private owner copies, explicit member access, selected sheet sections and managed images | S4 | Server implemented; client extensions pending |
| **Continuous saving** | Local autosave, automatic online replication, durable pending changes and visible save status | S5 | Implemented (server and desktop client); Windows tested |
| **Conflict handling** | Both versions remain visible when concurrent edits need your decision | S5 | Implemented (server and desktop client); Windows tested |
| **Revocation** | Remove member access and clear shared copies on their next synchronization | S6 | Implemented (server and desktop client); Windows tested |
| **Backup and recovery** | Consistent server backups, integrity checks and documented restoration | S7 | Planned |
| **Desktop integration** | Tested synchronization with the original Chronexus app | S8 | Planned |
| **Android access** | Same account and shared content, with read-only access first | S9 / app M27 | Planned |
| **Local or server AI** | Explicit execution choice, scoped sources and citations, bounded server resource use | S10 | Planned |
| **Web management** | Authenticated campaign, membership, storage and service management | S11 | Planned |

## Deployment and configuration

The source build uses **Docker Compose**, persistent volumes and a Rust service.
Copy `.env.example` to `.env`, set the public HTTPS origin and configure your host
reverse proxy, then run:

```sh
docker compose up -d --build
docker compose exec -T server chronexus-server invite
```

The second command prints a private, one-use registration invitation. See the
[installation guide](docs/INSTALLATION.md) for the complete proxy setup, optional
ports, account policy and recovery. This build supports accounts, campaign roles, explicit sharing and managed image APIs.

- Host bind address, published port and internal API port are configurable.
- The API uses one HTTPS origin; future web management will use the same origin.
  Only the application port is published.
- Public access requires a valid HTTPS origin and explicitly trusted TLS proxy.
  Credentials are generated securely; no default password.
- AI/GPU services remain S10 work; no model is downloaded or runtime started.
- The current S4 Linux arm64 image and Compose persistence/authorization checks
  passed locally. Linux amd64 and native Windows remain unverified.
  No registry image is published.

Confirmed protocol decisions and remaining hosting work are tracked in
[roadmap S0](ROADMAP.md#s0-deployment-and-security-decisions) and
[docs/DECISIONS.md](docs/DECISIONS.md).

## Synchronization and backups

S4 implements explicit per-item sharing and private-content APIs. The original app
can share pages, sessions, relationships and party members; new image/selected-sheet
selection UI and continuous local/server saving remain S5/S8 client work.

For an activated campaign, every autosave will persist locally before its change
is sent to the server. The interface will distinguish local success from confirmed
online success, offline pending changes, failures and conflicts. A server outage
must not stop local writing or discard pending edits.

The two synchronized copies do not replace independent backups: deletions can
propagate. Server recovery and the desktop's existing `.nodex` backups have
separate roles. Sharing revocation will remove access immediately on the server
and reach offline recipients when they reconnect; it cannot erase exported copies.

## Local or server assistant

Planned in S10.

AI will remain optional, with an explicit per-campaign choice of disabled, local
device or the selected self-hosted server. Server mode will use your server's
runtime and resources through an authenticated connection, with authorized
campaign context and source citations. Local mode keeps its loopback-only adapter.

No third-party cloud AI, automatic provider switch or implicit training is planned.
Server AI needs access to the submitted prompt and approved context; the encryption
policy must make that disclosure explicit. Runtime installation, model download
and campaign AI activation will remain separate actions. See
[roadmap S10](ROADMAP.md#s10-local-or-self-hosted-server-ai).

## Privacy and data

- Synchronization is optional; unselected campaigns stay on the device.
- A private server copy does not make a campaign visible to other players.
- Campaign notes and images are data, never instructions to execute.
- No telemetry, campaign content in logs or automatic upload of local AI weights,
  training datasets, adapters or complete desktop backups.
- Authentication, session isolation, HTTPS proxy checks and bounded auth resources
  are implemented. Campaign and per-item permissions are enforced. V1 uses server-readable
  storage without end-to-end encryption, as confirmed for the application.

## Roadmap

[ROADMAP.md](ROADMAP.md) defines S0–S11, dependencies and acceptance criteria.
[docs/STATUS.md](docs/STATUS.md) records what exists, checks actually performed
and the next step: **S7 security qualification and operational recovery**. Production
hosting and full campaign acceptance remain open.

## Development and contributing

The stack is Rust with axum, SQLite WAL and Docker Compose. The implemented
[OpenAPI contract](api/openapi.json) and [TypeScript schemas](api/types.ts) are
generated from Rust; the full campaign contract is still pending.

See [development and checks](docs/DEVELOPMENT.md), [API compatibility](docs/API.md),
[dependency licenses](docs/DEPENDENCIES.md) and [project rules](AGENTS.md).
Formatting, Clippy, Rust tests, release build, contract generation and the original
desktop sharing HTTP client are checked locally. The Linux arm64 Compose test
also verifies private/shared content and unchanged images after container recreation.

Report ideas and issues in [Chronexus Server](https://github.com/Acelith/Chronexus-Server/issues);
report desktop issues in [Chronexus](https://github.com/Acelith/Chronexus/issues).

## Credits and related projects

- [Chronexus](https://github.com/Acelith/Chronexus): original desktop application,
  campaign model, player knowledge principles and project identity.
- [Chronexus roadmap](https://github.com/Acelith/Chronexus/blob/main/ROADMAP.md):
  application milestones, including M24 client, M25 server and M27 Android.
- Logo and herald artwork: original Chronexus project assets, copied unchanged;
  see [asset provenance](assets/brand/README.md).
- The original Chronexus code is Apache-2.0 licensed. This server repository does
  not yet contain a license file; its own licensing must be established before
  distributing implementation code.

<div align="center">

<img src="assets/brand/chronexus-icon.webp" alt="" width="72">

*People, places, ideas. Everything is connected.*

</div>
