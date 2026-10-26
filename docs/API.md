# Implemented API and desktop compatibility

Source version 0.1.0 implements server milestones S1–S4, schema 003. This is not a
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
