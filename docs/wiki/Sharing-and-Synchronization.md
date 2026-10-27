# Sharing and synchronization

## Items

Pages, sessions, relationships and party members can be stored **private** (only the
author's devices) or **shared** with the members. Character-sheet sections and images are
**shared-only, read-only projections**. A relationship can be shared only if the pages it
connects are shared too.

## How a device stays aligned

1. It sends each saved change with a unique operation ID and the revision it was based
   on. Retries with the same ID are acknowledged instead of applied twice.
2. It reads everything that changed since its last position (a cursor), including
   tombstones for deletions and withdrawals. If its cursor is too old, it lists the
   campaign again.
3. While connected, it keeps a notification stream open; each hint only says "something
   changed", never what.

## Conflicts

If two devices change the same item, the second write is refused with the current server
version. The app shows both versions; the user chooses. The server never merges text.

## Offline

Devices keep working offline; their changes wait locally and are sent when the server
answers again. Operation IDs older than 30 days are no longer acknowledged; the device
then re-evaluates the item from its current state.

## Limits

Bounded page sizes, payload sizes, image sizes (8 MiB per original for sharing) and
storage quotas protect the server. See `docs/API.md` for exact values.

## Clue board items

The desktop's M31 clue board sends a `clue` only after its author selects a
private copy or shares that item. Its source page and session IDs must already
be present for the same audience. The server validates the typed payload and
keeps the item attributed to its author. Revoking a referenced page or session
withdraws a publicly visible clue; it does not change the author's local note
or create a factual relationship.
