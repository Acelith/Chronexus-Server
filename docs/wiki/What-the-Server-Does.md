# What the server does

## It does

- Keep **accounts** created by invitation only, with revocable device sessions.
- Host **shared campaigns** with members in three roles (owner, player, reader).
- Store, per account and per item, either a **private copy** (only that account's
  devices) or a **shared** copy that the campaign's members can read.
- Tell connected devices **that** something changed (a content-free hint) so they fetch
  it promptly; devices otherwise poll.
- Keep a change log with **tombstones**, so devices learn about deletions and withdrawals.
- Detect **conflicts** with revisions: a write based on an old revision is refused and the
  device shows both versions to its user.
- Store uploaded **images** byte for byte with a checksum.

## It never does

- Decide what to share: each item is shared only when its author chooses so.
- Merge text or overwrite a newer version.
- Turn a shared claim into a "fact": items stay attributed to their author.
- Run AI, send email or call any other service (server AI is a planned, explicit option).
- Log content, passwords or tokens.

## Who sees what

| Item | Its author | Other members | Operator |
| --- | --- | --- | --- |
| Never synchronized | (only on their computer) | No | No |
| Private copy | Yes, on all their devices | No | Yes¹ |
| Shared | Yes | Read-only, attributed | Yes¹ |

¹ Traffic is protected by HTTPS but data is **not end-to-end encrypted**: whoever controls
the server and its volume can read it. Use a server you trust.

The desktop app's full character sheet, AI data and preferences are never sent; sheet
sections and images can only be shared, as read-only projections.
