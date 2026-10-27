# Chronexus Server wiki

Chronexus Server is the **optional, self-hosted** companion of the
[Chronexus](https://github.com/Acelith/Chronexus) desktop app. You run it on a home
server, a NAS or a small VPS with Docker Compose. It lets a player keep a campaign on
several computers and share chosen items with the party. Chronexus works fully without
it.

> **Status:** release [0.1.1](https://github.com/Acelith/Chronexus-Server/releases/tag/v0.1.1) is published with container images
> (`ghcr.io/acelith/chronexus-server:0.1.1`, linux/amd64 and linux/arm64). Read [Security](Security) before exposing it
> to anyone.

## For operators and players

| Page | What you learn |
| --- | --- |
| [What the server does](What-the-Server-Does) | What it stores, what it never does, who sees what |
| [Installation](Installation) | Docker Compose, HTTPS, first invitation |
| [Accounts and invitations](Accounts-and-Invitations) | Creating accounts, sessions, recovery |
| [Campaigns and roles](Campaigns-and-Roles) | Owner, player, reader; invitations to a campaign |
| [Sharing and synchronization](Sharing-and-Synchronization) | Private copies, shared items, conflicts, offline |
| [Backups and upgrades](Backups-and-Upgrades) | Backup, verify, restore, upgrade, rollback |
| [Security](Security) | Threat model, limits, what is and is not tested |
| [Operator commands](Operator-Commands) | Every CLI command |

How players use sharing inside the app is explained in the app wiki:
[Sharing with a server](https://github.com/Acelith/Chronexus/wiki/Sharing-with-a-Server).

## For developers

Start at [Developer home](Developer-Home).
