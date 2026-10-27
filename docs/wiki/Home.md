# Chronexus Server wiki

Chronexus Server is the **optional, self-hosted** companion of the
[Chronexus](https://github.com/acelith/Chronexus) desktop app. You run it on a home
server, a NAS or a small VPS with Docker Compose. It lets a player keep a campaign on
several computers and share chosen items with the party. Chronexus works fully without
it.

> **Status:** release [0.1.1](https://github.com/Acelith/Chronexus-Server/releases/tag/v0.1.1) is published with Linux amd64 and arm64 binaries.
> Container deployment builds locally; GHCR publication is pending. Read [Security](Security.md) before exposing it
> to anyone.

## For operators and players

| Page | What you learn |
| --- | --- |
| [What the server does](What-the-Server-Does.md) | What it stores, what it never does, who sees what |
| [Installation](Installation.md) | Docker Compose, HTTPS, first invitation |
| [Accounts and invitations](Accounts-and-Invitations.md) | Creating accounts, sessions, recovery |
| [Campaigns and roles](Campaigns-and-Roles.md) | Owner, player, reader; invitations to a campaign |
| [Sharing and synchronization](Sharing-and-Synchronization.md) | Private copies, shared items, conflicts, offline |
| [Backups and upgrades](Backups-and-Upgrades.md) | Backup, verify, restore, upgrade, rollback |
| [Security](Security.md) | Threat model, limits, what is and is not tested |
| [Web interface](Web-Interface.md) | Manage campaigns, members, invitations, devices and the service in a browser |
| [Operator commands](Operator-Commands.md) | Every CLI command |

How players use sharing inside the app is explained in the app wiki:
[Sharing with a server](https://github.com/Acelith/Chronexus/blob/main/docs/wiki/Sharing-with-a-Server.md).

## For developers

Start at [Developer home](Developer-Home.md).
