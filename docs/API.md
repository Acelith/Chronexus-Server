# Implemented API and desktop compatibility

Source version 0.1.0 implements server milestones S1–S4, schema 004. This is not a
published release. The original Chronexus source 0.8.1 has an M24 sharing client;
server delivery is application M25 and Android M27. The server now implements its
account, campaign and four-kind sharing protocol, plus explicit private-content,
selected-sheet and image extensions. Continuous desktop replication remains S5.

- [Generated OpenAPI](../api/openapi.json), also `GET /api/v1/openapi.json`.
- [Generated TypeScript schemas](../api/types.ts).
- [Original application contract](https://github.com/Acelith/Chronexus/blob/main/docs/SYNC_API.md).

## Authentication and campaigns

Bodies use camelCase JSON and reject unknown fields. Bearer tokens are required
except for meta, login, invitation redemption and recovery. Tokens/codes contain
256 random bits; storage keeps only their SHA-256 hashes. Credentials and client
IDs are never accepted as URL query authentication. All responses use no-store.

| Route | Action |
| --- | --- |
| `GET /api/v1/meta` | Protocol v1 and implemented capability names |
| `POST /api/v1/auth/redeem` | Register with a server or campaign invite; campaign membership is atomic |
| `POST /api/v1/auth/login` | Sign in on a named device |
| `POST /api/v1/auth/logout` | Revoke the presented token |
| `POST /api/v1/auth/recover` | Redeem an operator-issued recovery code and revoke all old sessions |
| `GET /api/v1/auth/sessions` | List own active devices without tokens |
| `DELETE /api/v1/auth/sessions/{sessionId}` | Revoke one own device |
| `DELETE /api/v1/auth/sessions` | Revoke all own sessions |
| `POST /api/v1/campaigns` | Create a campaign as owner |
| `GET /api/v1/campaigns` | List own memberships, at most 100 |
| `GET /api/v1/campaigns/{campaignId}` | Read an authorized campaign |
| `POST /api/v1/campaigns/join` | Accept a one-use campaign invite |
| `POST /api/v1/campaigns/{campaignId}/invites` | Owner issues a player/reader invite |
| `GET /api/v1/campaigns/{campaignId}/invites` | Owner lists bounded invite metadata, without codes |
| `DELETE /api/v1/campaigns/{campaignId}/invites/{inviteId}` | Owner revokes an unused invite |
| `GET /api/v1/campaigns/{campaignId}/members` | Read current roles/display names, no usernames or secrets |
| `PATCH /api/v1/campaigns/{campaignId}/members/{accountId}` | Owner changes player/reader role with baseRevision |
| `DELETE /api/v1/campaigns/{campaignId}/members/{accountId}?baseRevision=n` | Owner removes a member, or member leaves |

The sole owner cannot leave or be demoted. Stale membership revisions return 409;
leave/rejoin does not reset the revision. Removal withdraws that author's published
content in the same transaction and preserves private original payloads. A returning
member does not automatically republish them. Membership is still required to access
retained content; a removed author retains their independent local originals.

The current desktop stores its token in `sync-account.json` outside backups.
This server change does not implement secure OS storage or new device/recovery UI
in the app. Protect the desktop profile; that client hardening remains separate.

## Existing desktop sharing protocol

`PUT /api/v1/campaigns/{campaignId}/items/{kind}/{itemId}` accepts baseRevision and
payload for **page, session, relationship, party**. It is an explicit request to
publish the submitted projection to members. Only its author may replace content;
readers cannot write, and the campaign owner cannot rewrite another author's claim.

`GET .../changes?since=` returns latest states in per-campaign sequence order,
at most 1000 items and about 6 MiB per page, below the desktop's 8 MiB limit. Only
published legacy kinds enter this feed. Private writes do not advance its clock.
Tombstones contain no payload. No retention pruning is implemented yet.

`DELETE .../items/{kind}/{itemId}?baseRevision=n` withdraws the shared projection
and emits a tombstone while preserving its private original. It does not erase the
private online copy. Explicit private-content deletion uses the extension below.
Revisions always increase, including deletion/recreation; baseRevision 0 can recreate
an actual tombstone, while a retained private row requires its current revision.
This refines the draft's reset-to-1 wording and is accepted by the real client.
409 item conflicts contain revision and payload (null for a deleted item), never a
private payload belonging to another author.

Documents are typed inert note-format-v1 projections. Supported blocks: paragraph,
heading, bulletListItem, numberedListItem, checkListItem and quote; inline text,
HTTP/HTTPS/mailto links and pageMention. Properties/styles are allowlisted; custom
hidden fields, active URL schemes, duplicate block IDs and excessive nesting fail.
A mention retains its explicitly shared label/UUID without fetching its target or
asserting a relationship. Relationships retain author and certainty; source-reference
fields that could expose private notes are rejected.

Relationships and party references require accessible pages in the same campaign;
public references require explicitly public pages. Withdrawing/deleting a page also
withdraws dependent published relationship/party projections without deleting their
original assertions. Sharing a reference never shares its target automatically.

## Private content and selected projections

`PUT .../content/{kind}/{itemId}` accepts baseRevision, payload and optional
visibility: **private** (default) or **members**. Resource kinds are the four legacy
kinds plus **sheetSection** and **image**. Private data is visible only to its author,
including against the campaign owner. Unauthorized private IDs return 404; lists
filter inaccessible rows before pagination.

- `GET .../content/{kind}/{itemId}` reads an authorized resource.
- `GET .../content?after=` lists current authorized resources, at most 50/about 6 MiB;
  use the returned cursor verbatim. This is a current listing, not an offline queue.
- `DELETE .../content/{kind}/{itemId}?baseRevision=n` explicitly deletes the author's
  payload and emits a public tombstone if needed.
- `DELETE .../content/{kind}/{itemId}/sharing?baseRevision=n` withdraws a projection,
  retaining its private original. Its author or the campaign owner may withdraw it.

A sheetSection contains characterId, name, section (identity, combat, equipment,
spellcasting or custom) and 1–64 selected fields, each with key, label and string
value. Values such as `0` and `No` remain present. No whole sheet, private notes,
formulas or implicit source updates are accepted. An image projection contains an
owned attachmentId and caption; upload alone does not make bytes public.

These extensions never enter the legacy feed: the original app rejects unknown
kinds. Its existing UI can explicitly share the four legacy types; private replica,
image and selected-sheet selection/preview UI remains client integration work.
Do not enable automatic whole-campaign upload based on this server capability.

## Managed images

`POST .../attachments` receives original binary PNG/JPEG/WebP bytes, the matching
Content-Type and mandatory lowercase-hex X-Content-SHA256. Maximum 8 MiB, 8192 per
axis, 16 million pixels and 64 MiB of decoded output; decoder allocation limits
are best-effort, alongside strict dimension/output checks and container limits.
Credentials/membership are checked before reading a large body; two concurrent
content transfers and four blocking workers bound processing.

Metadata includes id, authorId, checksum, MIME, dimensions, byteLength and createdAt.
`GET .../attachments/{attachmentId}` checks current access for every download,
verifies SHA-256 and returns unchanged bytes with a generated download filename.
The author can read a private upload; other members need a published image reference.
No filename, URL or filesystem path is accepted. Original image metadata is preserved
as part of the deliberately uploaded bytes.

`DELETE .../attachments/{attachmentId}` is author-only and refuses referenced
originals (409). Remove image content references first. Uploads are atomic SQLite
BLOBs, so interrupted uploads leave no partially stored file. Unreferenced completed
uploads remain private and count towards quotas; deletion is explicit.

## Limits and errors

Campaign content: 5000 rows (including tombstones), 64 MiB payload total, 2 MB per
projection; JSON envelopes at most 2.1 MB. Images: 512 per campaign, 128 MiB campaign
bytes and 256 MiB per author. See INSTALLATION for membership/invitation/session limits.

400 validation, 401 invalid credentials, 403 role/transport refusal, 404 absent or
inaccessible content, 409 stale revisions/referenced originals, 410 unavailable
invitation, 413 oversized body, 429 quota/attempt limits, 503 worker/transfer saturation,
500 safe storage failure. Ordinary errors use an English error field; the app maps
status codes to localized text. Complete production security/recovery is still S7.


## S5 server contract: incremental sync, idempotent mutations, notifications

Capabilities in `GET /api/v1/meta`: `idempotent-content-saves`, `idempotent-mutations`,
`incremental-sync`, `change-notifications`. Schema 005.

### Idempotent mutations

Every keyed mutation carries a **UUIDv7** operation ID that the client persists with
the exact outbound operation before sending: `requestId` on `PUT .../content/...`,
`requestId` query parameter on `DELETE .../content/...` and `DELETE .../sharing`, and
for the legacy `/items` routes either `requestId` (delete) or the `Idempotency-Key`
header (put and delete; supplying both is 400). Unkeyed calls keep the previous
revision-conflict behavior.

- The server stores the accepted revision and a normalized SHA-256 digest of the
  operation (kind, item, operation, base revision, payload, visibility), atomically
  with the change. Retries with the same ID return the original revision and never
  re-apply, even after later edits, withdrawals or deletions. This acknowledges a
  historical operation; it is not a statement of the current revision.
- Reusing an ID for a different target, operation, base revision or payload is 409
  `Request ID already used for a different save.` without disclosing content.
  Failed and conflicting operations do not consume the ID.
- Current authentication and write membership are required on every retry. Receipts are
  scoped to account and campaign, so another device of the same account can resume.
- **Retry horizon.** IDs older than 30 days are refused with 410, and IDs more than five
  minutes in the future or not UUIDv7 with 400. Receipts are kept 31 days and compacted
  opportunistically on later keyed operations, so an expired receipt can never matter:
  a late retry is refused instead of silently re-applied. After 410 the client must
  re-read the item and create a new operation from the current revision. The live cap is
  100,000 receipts per account and campaign (429 `Save receipt quota reached`); existing
  retries still succeed and nothing is partially written.
- `POST .../attachments` is idempotent by content: identical bytes from the same author
  in the campaign return the stored attachment (200) instead of a duplicate (201).
  `DELETE .../attachments/{id}` has no receipt: after a lost response a retry sees 404,
  which the client treats as already removed.

### Incremental reads

`GET /api/v1/campaigns/{campaignId}/sync?cursor=` returns the **current authorized
state** of every item changed since an opaque cursor, as `{items, cursor, more}`.
Each item is a content item with `deleted`. Rules:

- An author receives every own item, private or shared, with its payload, and a
  tombstone (`deleted: true`, no payload) when it is deleted.
- Other members receive an item only while it is shared. When it is withdrawn, unshared,
  or its page dependency was withdrawn, or it is deleted after having been shared, they
  receive a payload-free tombstone; edits, creation and deletion of private items
  produce nothing for them.
- Pages hold at most 500 log rows and 6 MiB; repeated changes to one item inside a page
  deliver one item. Persist the returned cursor only after the page has been applied
  locally, then repeat while `more` is true. When `more` is false the cursor is the
  campaign head. Cursors are opaque decimal strings.
- `GET .../content` now also returns `head`. Full resynchronization: list every page of
  `/content`, replace the local shared copies (items absent from the listing are gone),
  then continue `/sync` from the `head` of the first page; duplicates are harmless
  because delivery is by current revision.
- **Cursor expiry.** Per campaign the server keeps at most 100,000 log rows; older rows
  are dropped and the floor is raised. A cursor below the floor, or above the head (the
  server was restored from an older backup), returns 410 `Sync cursor is no longer
  valid`. The empty cursor means "from the beginning" and is subject to the same rule.
  The server never guesses: a stale client must resynchronize from the listing, so a
  revoked item cannot be resurrected by an old cursor.
- The legacy `/changes` feed is unchanged and independent.

### Notifications

`GET .../events` is an authenticated `text/event-stream` (Bearer header). Frames are
content-free hints: `ready` (subscription is active; poll once), `changed` (poll
`/sync`), `reconnect` (lifetime reached, 10 minutes) and `closed` (access lost). They
never contain identifiers, kinds, authors or cursors. A private edit notifies only its
author's streams; sharing, withdrawal, deletion of shared items notify every member.
Frames coalesce, a slow reader just receives `changed`, a keepalive comment arrives every
25 seconds and session/membership are re-checked on each keepalive (revocation closes
the stream within about 25 seconds). Limits: 3 streams per account, 64 total (429
beyond that). Clients must keep a polling fallback: poll `/sync` about every 30 seconds
while offline-push is unavailable, and always poll once after every (re)connection.
Membership removal does not yet push a hint; polling covers it until S6.

## S6 revocation, deletion and recovery boundaries

Schema 006, capability `deletion`.

**What each action removes**

| Action | Effect on the server |
| --- | --- |
| Unshare an item, or remove a member (S4) | Projections become tombstones for the other members; the author's **private original stays**. Removal is immediate: the old session can neither read nor write. Rejoining by a new invitation never republishes anything; a stale offline write conflicts (409). |
| `POST /campaigns/{id}/own-content/deletion` `{confirm:true}` | Any member: every item they synchronized to this campaign becomes a tombstone (for members who saw it) and their uploads are deleted. Other members' content and the membership stay. Idempotency receipts are kept, so a lost-response retry is acknowledged and never brings the item back. Returns `{items, attachments}`. |
| `POST /campaigns/{id}/deletion` `{confirmName}` | Owner only, exact campaign name required (400 otherwise). Removes every payload, upload, change-log row, receipt and invitation. The campaign id, deletion time and inactive membership rows stay. Former members (and the owner) then get **404 "This campaign was deleted."** on every campaign call; outsiders still get 403. |
| `POST /auth/account/deletion` `{password, deleteOwnedCampaigns}` | The account holder, password checked (5 attempts per 15 minutes). Owning a campaign other members use returns 409 unless `deleteOwnedCampaigns` is true (there is no ownership transfer: the last owner cannot leave; delete the campaign or invite a replacement and delete the account later). Memberships end, authored content is erased as above, sessions are revoked, and the account becomes an anonymous stub (`deleted-…`, "Deleted account", disabled) that content-free history still references. The user name becomes available again. |

**Operator commands** (same database environment as `invite`):

```sh
chronexus-server revoke-sessions   # ends every session; run after restoring an older backup
chronexus-server compact [DAYS]    # drops tombstones older than DAYS (default 180)
```

`compact` raises each affected campaign's cursor floor, so a client that missed the
deletion must list the campaign again; an old offline write against a purged item is
refused (404) and cannot recreate it. Keep tombstones at least as long as the longest
time a device may stay offline.

**Limits that no server action can remove**

- A member's own exported or cached copies, and any device that has not reconnected yet,
  keep what they already received; revocation reaches them at their next synchronization
  (the desktop client clears received copies then and keeps its own originals).
- Server backups keep deleted data until the operator's retention drops them. A restored
  backup brings back the memberships, sessions and content it contained; the operator
  should run `revoke-sessions` and re-apply deletions the backup predates (S7 documents the
  restore procedure). Revocations recorded after the backup are not in it.
