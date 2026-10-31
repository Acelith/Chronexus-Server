# Implemented API and desktop compatibility

## Chronexus Table (server M42/M43 source)

The operator CLI command `chronexus-server invite --gm` issues a one-use account
registration code with GM capability. Operators and GMs do not inherit journal
access. Existing table GMs from schema 008 retain capability during migration.

| Route | Action |
| --- | --- |
| `POST /api/v1/tables` | GM creates a table |
| `GET /api/v1/tables`, `GET /api/v1/tables/{tableId}` | List/read own tables |
| `POST /api/v1/tables/{tableId}/invites` | GM issues a one-use player or spectator code |
| `POST /api/v1/tables/join` | Existing account takes a seat; `auth/redeem` also accepts table codes for new accounts |
| `GET /api/v1/tables/{tableId}/seats` | Members list seats |
| `DELETE /api/v1/tables/{tableId}/seats/{accountId}` | GM removes a non-GM seat or member leaves |
| `POST`, `DELETE /api/v1/tables/{tableId}/journal-grants` | Author grants/revokes one currently member-shared item for that table's GM |
| `GET /api/v1/tables/{tableId}/journal-grants` | GM reads only still-authorized grants |
| `POST /api/v1/tables/{tableId}/live-ticket` | Member obtains a 30-second, one-use ticket |
| `GET /api/v1/tables/{tableId}/live?ticket=…&since=n` | WebSocket state, presence and public table activity |

Journal grants are returned as an array of at most 100 items, ordered by
`campaignId`, `kind`, then `itemId`. To read the next page, pass the last item's
`campaignId/kind/itemId` as the URL-encoded `after` query parameter. Stop when a
page has fewer than 100 items. Each page checks the GM seat and current grants;
revoking the cursor item does not skip subsequent items. The cursor grants no
access. This is a current listing, not a snapshot: concurrent new grants before
the cursor appear on the next full refresh. The web workspace loads every page.

The WebSocket accepts JSON `{operationId,baseRevision,text,audience:"public"}`.
`operationId` is UUIDv7. The server validates current seat/session and revision,
stores each operation once and sends an acknowledgment. `state` frames contain
revision, public events, online count and a reset flag for a stale cursor. The
ticket is consumed at upgrade and is never a reusable account credential. Only
public activity is accepted at present; secret combat data belongs to later
milestones and never enters this event stream.

The web client waits for the initial state before enabling sends. After a
disconnect, it reconciles any unacknowledged message with that state and, if
absent, retries the exact operation ID and payload once on the new connection.
A revision conflict preserves the draft for an explicit new send; it never
silently rebases the old operation. Acknowledgment does not erase a newer draft.

M42/M43 introduced schema 010; current source schema is 012. Published release 0.1.1 predates M42/M43;
these table endpoints have not been published. This server retains the M25
account, campaign, sharing and synchronization contracts. The desktop app's
table audience controls are separate work.

- [Generated OpenAPI](../api/openapi.json), also `GET /api/v1/openapi.json`.
- [Generated TypeScript schemas](../api/types.ts).
- [Original application contract](https://github.com/Acelith/Chronexus/blob/main/docs/SYNC_API.md).

## Authentication and campaigns

Bodies use camelCase JSON and reject unknown fields. Bearer tokens are required
except for meta, login, invitation redemption and recovery. Tokens/codes contain
256 random bits; storage keeps only their SHA-256 hashes. Session tokens and
client IDs are never accepted as URL query authentication. The live WebSocket
uses a separate 30-second, one-use ticket in its URL; access logs must omit
query strings. All responses use no-store.

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

## Web interface overviews (M25.11)

| Endpoint | Who | Returns |
| --- | --- | --- |
| `GET /api/v1/account` | any signed-in account | id, user name, display name, creation time, `operator` flag |
| `GET /api/v1/campaigns/{id}/overview` | members | role, member count, the caller's own items per kind (private/shared counts, bytes, highest revision), own images, last accepted save, recorded operations, count of items others share, server limits |
| `GET /api/v1/operator/status` | operators | version, schema, database size, account/session/campaign/item/image counts, server AI state |
| `GET /api/v1/operator/accounts` | operators | per account: user and display name, creation, state, active sessions, campaign count, stored bytes |

The overview never reports campaign-wide totals, which would reveal how much other members
keep private. Operator endpoints never return campaign names, members or content. All four
are read-only; the web interface performs changes through the existing endpoints.

## Limits and errors

Campaign content: 5000 rows (including tombstones), 64 MiB payload total, 2 MB per
projection; JSON envelopes at most 2.1 MB. Images: 512 per campaign, 128 MiB campaign
bytes and 256 MiB per author. See INSTALLATION for membership/invitation/session limits.

400 validation, 401 invalid credentials, 403 role/transport refusal, 404 absent or
inaccessible content, 409 stale revisions/referenced originals, 410 unavailable
invitation, 413 oversized body, 429 quota/attempt limits, 503 worker/transfer saturation,
500 safe storage failure. Ordinary errors use an English error field; the app maps
status codes to localized text. Complete production security/recovery is still M25.7.


## M25.5 server contract: incremental sync, idempotent mutations, notifications

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
Membership removal does not yet push a hint; polling covers it until M25.6.

## M25.6 revocation, deletion and recovery boundaries

Schema 006, capability `deletion`.

**What each action removes**

| Action | Effect on the server |
| --- | --- |
| Unshare an item, or remove a member (M25.4) | Projections become tombstones for the other members; the author's **private original stays**. Removal is immediate: the old session can neither read nor write. Rejoining by a new invitation never republishes anything; a stale offline write conflicts (409). |
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
  should run `revoke-sessions` and re-apply deletions the backup predates (M25.7 documents the
  restore procedure). Revocations recorded after the backup are not in it.

## M44 table board (schema 011, source only)

- `GET /tables/{tableId}/board` under `/api/v1` returns the caller's filtered
  scenes, tokens and area templates, active scene and table revision. Only GMs
  receive preparation scenes, hidden objects or imported geometry. Membership in
  the table never grants journal access.
- `POST /tables/{tableId}/board/operations`: `{operationId,baseRevision,command}`;
  see generated `BoardAction` for scene save/import/activation, token save/remove/
  movement and template save/remove. Commands also work on the authenticated live
  socket; `state.board` is the same projection, `boardAck`/`boardError` are distinct
  from public-message replies. Reconnect always includes a current board snapshot.
- `POST /tables/{tableId}/maps`: original PNG/JPEG/WebP body with its exact MIME;
  returns asset metadata, deduplicated by table/checksum. `GET /maps/{assetId}`
  requires the GM seat or an authorized current visible map/portrait reference.
- `POST /tables/{tableId}/uvtt`: `{operationId,baseRevision,sceneId,name,document}`,
  containing a [Universal VTT 0.2/0.3 document](https://arkenforge.com/universal-vtt-files/).
  Embedded PNG/grid/occluders only; unknown producer data is discarded. Import
  prepares a scene; showing it to players requires explicit activation. A rejected
  import may leave a private deduplicated uploaded asset within the table quota.

Limits: 50 scenes/table, 200 tokens/scene and 1000/table, 100 templates/table,
100 map assets and 256 MiB original images/table. Maps are at most 32 MiB,
16384 px/side, 64 million pixels and 256 MiB decoded. Commands are at most 256 KiB;
120 new operations/seat/minute, 50000 receipts and 16 MiB receipt payloads/table.
Receipts never expire silently; reaching the cap refuses new writes. Public
messages keep their separate M43 bounds. Failed writes create no receipt; exact
retries return the previous revision, different payloads for the same ID conflict.

Grid coordinates are square/axial hex cell centres or gridless image coordinates;
paths have at most 100 adjacent cells (one direct Euclidean segment without a grid).
Spent distance uses the scene's rules unit. Difficult cells cost double; exceeding
speed is flagged in the UI, not blocked. Players require their own visible active-scene token. Outside combat the GM must
allow free movement; during combat only the current token may move (GM movement
remains unrestricted).
Rulers/converter are local previews. Explicitly shared templates persist; they do
not apply damage, conditions or sheet changes. Server backups include all table
state/assets; desktop `.nodex` and campaign AI never read these tables.


## M45 explicit combat (schema 012, source only)

Use the same board operation envelope with `command: {type:"combat",action:...}`.
Generated `CombatAction` defines the full JSON contract. The GM prepares selected
same-scene tokens (`start`), owners enter initiative or explicitly roll d20 plus
a manual modifier, the GM resolves ties/reorders, then `next` begins round 1.
Subsequent `next` advances turns/rounds; `delay`, `add`, `remove` and `end` are
explicit GM actions. Changing initiative later preserves the current token.
Players move only their own current token; new turns reset spent distance and
diagonal parity. Removing a physical token repairs the tracker in the same transaction.

`setVitals` sets absolute table-only HP/max HP/AC, optional public manual `status`,
HP disclosure and a GM-only note for GM-owned tokens. Only the token's owner may
change these values, including against the GM: the GM edits its own monsters,
not a player's character token. GM reassignment cannot take over a player-owned
token (including after the player leaves). HP is absent for others unless its
owner explicitly enables disclosure; AC remains owner-only. The GM's private
notes never reach other seats. The web asks for confirmation for HP changes.
No automatic roll-to-damage, death, resource consumption or journal/sheet write.

`setCondition`/`removeCondition` allow the GM or token owner to manage SRD condition
markers with an optional manual round duration. Round boundaries decrement it;
zero is retained as an explicit expiry reminder, never silently removed. Names
are stable condition IDs with localized display labels; no rule effects inferred.
`roll` supports 1–20 dice of 2–1000 sides and an explicit integer modifier. Owners
roll their tokens; only the GM can mark its rolls secret. Initiative uses the same
bounded server randomness; there is no implicit advantage or character formula.

`board.combat`, `board.vitals` and `board.combatLog` are filtered together with visible
active-scene tokens. Hidden turns have no player-visible token ID. Secret rolls,
private HP deltas, preparation-scene events and GM notes are filtered before HTTP
or WebSocket serialization. The server keeps the latest 1000 log rows per table;
snapshots return at most 200 recent authorized candidates, further filtered by
visible token/scene. This is a bounded activity log, not an immutable audit archive.
Server backups validate map checksums as well as campaign-image checksums and
preserve combat; restore still revokes all sessions. Native desktop character
projection/mirroring and three-desktop end-to-end qualification are separate work.

## M46–M47 table tools (schemas 013–014, source only)

The existing board operation envelope now accepts `saveFog`, `importVision`,
`saveMonster`, `saveEncounter`, `deployEncounter`, `removeLibrary` and `saveAsset`.
These are GM-only, revisioned, transactional and covered by the existing durable
operation receipts. `BoardToken.artwork` defaults to false for older tokens.

`BoardView.fog` contains only the viewer's visible tile indices. `fogEditor`,
`monsters`, `encounters` and `assets` are structurally absent for non-GM seats.
Fogged map downloads return redacted PNG bytes; the GM retains original bytes.

`POST /tables/{tableId}/catalog` accepts `{kind,query,entry}` (`monsters` or
`assets`), with an explicit public term or selected entry ID. It returns search
entries, an editable monster draft, or an imported managed image plus attribution.
It never saves a monster automatically. Saving the reviewed draft/asset uses a
normal board operation. An interrupted image import can leave a private,
deduplicated original within the existing table quota.

See [Table tools](TABLE_TOOLS.md) for visibility resolution, capacities, licences,
network hosts, deadlines and the user workflows. OpenAPI/TypeScript are generated
from the implemented types. These APIs do not change journals, character sheets,
AI retrieval, or the original campaign synchronization contract.
