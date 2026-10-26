<div align="center">

<img src="assets/brand/chronexus-wordmark.webp" alt="Chronexus" width="520">

# Chronexus Server

**Your campaign, on your device and your server. Connected on your terms.**

Keep a local and private online copy of your campaign, choose what other players
can see, and decide whether AI runs on your computer or your own server.

[![Status](https://img.shields.io/badge/status-planning-b89b60)](ROADMAP.md)
[![Deployment](https://img.shields.io/badge/deployment-Docker%20Compose-354b3e)](#deployment-and-configuration)
[![Hosting](https://img.shields.io/badge/hosting-self--hosted-6c302b)](#privacy-and-data)

[Chronexus](https://github.com/Acelith/Chronexus) · [Planned features](#planned-features) · [Roadmap](ROADMAP.md) · [Status](docs/STATUS.md)

</div>

> [!NOTE]
> Chronexus Server is in planning. The features below are not implemented yet;
> there is no runnable server, Compose package or published server release.

## What is Chronexus Server

Chronexus Server is the planned self-hosted companion to
[Chronexus](https://github.com/Acelith/Chronexus), the offline-first desktop
journal and campaign manager for tabletop RPG players. It adds optional accounts,
continuous campaign synchronization and selective sharing on infrastructure you
control. The original desktop app remains usable without a server or account.

The projects have separate repositories and releases: this repository owns the
server and its versioned API; the [original Chronexus repository](https://github.com/Acelith/Chronexus)
owns the desktop app and planned Android client. Synchronization belongs to
application milestone M24; Android support follows in M25.

## Why Chronexus Server

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

## Planned features

| Feature | In short | Milestone |
| --- | --- | --- |
| **Accounts and devices** | Authentication, revocable sessions and account recovery under the selected registration policy | S2 |
| **Campaign membership** | Invitations, owner/player/read-only roles and authorization on every item | S3 |
| **Selective sharing** | Private owner copies, explicit member access, selected sheet sections and managed images | S4 |
| **Continuous saving** | Local autosave, automatic online replication, durable pending changes and visible save status | S5 |
| **Conflict handling** | Both versions remain visible when concurrent edits need your decision | S5 |
| **Revocation** | Remove member access and clear shared copies on their next synchronization | S6 |
| **Backup and recovery** | Consistent server backups, integrity checks and documented restoration | S7 |
| **Desktop integration** | Tested synchronization with the original Chronexus app | S8 |
| **Android access** | Same account and shared content, with read-only access first | S9 / app M25 |
| **Local or server AI** | Explicit execution choice, scoped sources and citations, bounded server resource use | S10 |
| **Web management** | Authenticated campaign, membership, storage and service management | S11 |

## Deployment and configuration

The planned distribution uses **Docker Compose**, persistent volumes and a Rust
service. The goal is a short bootstrap followed by `docker compose up -d`, with
an example configuration that makes most parameters optional. This is a deployment
target, not a command supported by this repository today.

- Host bind address, published port and internal API port will be configurable.
- API and web management will share one HTTPS origin. Database and optional AI
  services will stay on the internal container network.
- Public deployments will require a valid HTTPS origin and appropriate TLS/proxy
  configuration. Credentials will be generated securely; no default password.
- AI and GPU services will use optional profiles, disabled until explicitly enabled.
- Linux amd64 and arm64 images are planned; supported targets will be listed only
  after their artifacts and deployment have been verified.

Hosting, registration, encryption and persistence decisions are tracked in
[roadmap S0](ROADMAP.md#s0-deployment-and-security-decisions). No installers or
infrastructure changes are part of the current planning work.

## Synchronization and backups

For an activated campaign, every autosave will persist locally before its change
is sent to the server. The interface will distinguish local success from confirmed
online success, offline pending changes, failures and conflicts. A server outage
must not stop local writing or discard pending edits.

The two synchronized copies do not replace independent backups: deletions can
propagate. Server recovery and the desktop's existing `.nodex` backups have
separate roles. Sharing revocation will remove access immediately on the server
and reach offline recipients when they reconnect; it cannot erase exported copies.

## Local or server assistant

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
- Authentication, per-item permissions, HTTPS and bounded resource use are release
  requirements. End-to-end encryption remains an explicit S0 decision.

## Roadmap

[ROADMAP.md](ROADMAP.md) defines S0–S11, dependencies and acceptance criteria.
[docs/STATUS.md](docs/STATUS.md) records what exists, checks actually performed
and the next step. The first implementation gate is S0, alongside the original
application's M23 audit prerequisite.

## Development and contributing

The planned stack is Rust with axum, a versioned HTTP/OpenAPI contract and Docker
Compose. The database choice will be recorded during S0. Generated client types
will come from the server contract so desktop and Android compatibility can be
tested explicitly.

There are no build or test commands for a server implementation yet. Report ideas
and issues in [Chronexus Server](https://github.com/Acelith/Chronexus-Server/issues);
report desktop issues in [Chronexus](https://github.com/Acelith/Chronexus/issues).

## Credits and related projects

- [Chronexus](https://github.com/Acelith/Chronexus): original desktop application,
  campaign model, player knowledge principles and project identity.
- [Chronexus roadmap](https://github.com/Acelith/Chronexus/blob/main/ROADMAP.md):
  application milestones, including M24 sharing and M25 Android.
- Logo and herald artwork: original Chronexus project assets, copied unchanged;
  see [asset provenance](assets/brand/README.md).
- The original Chronexus code is Apache-2.0 licensed. This server repository does
  not yet contain a license file; its own licensing must be established before
  distributing implementation code.

<div align="center">

<img src="assets/brand/chronexus-icon.webp" alt="" width="72">

*People, places, ideas. Everything is connected.*

</div>
