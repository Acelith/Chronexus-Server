# Fog, monsters and images (M46–M47)

These tools are implemented in the server/web source, not in published 0.1.1 or
the desktop client. Tables and journals remain separate. Server backups include
the table library, fog settings and original images; player `.nodex` backups do not.

## Fog and vision

Open **Fog and vision** in a table's GM sidebar. Enable fog and save to cover the
scene. Choose rectangle, brush or polygon reveal tools. Polygon points are clicks;
use **Finish polygon** to apply. Cover tools, undo, reveal-all and confirmed
cover-all are available. Coordinate fields provide a keyboard alternative.

Visibility uses a bounded **32 × 32 tile mask**, independent of the battle grid.
Painting selects whole fog tiles by their centres. This is deliberately coarse,
not pixel-accurate freehand fog. The GM sees a translucent overview; players receive
only their visible mask. Spectators have no observers under restricted vision.

For restricted vision, select an owned player token and set its manual range and
darkvision choice. Add lights and walls using image-pixel coordinates. Doors can
be opened and closed explicitly. Every visible tile must have all four corners
within one owned observer's radius, with unobstructed sight; ordinary vision also
requires illumination. Distances retain their original units and display ft/m.
Walls are opaque line segments, lights are circular; elevation, angled lights and
automatic rules effects are not implemented. Each scene permits 128 paint strokes
of 3–64 vertices, 64 wall segments, 16 observers and 16 lights.

**Use UVTT walls and lights** converts an imported scene's geometry into editable
vision settings. The same limits apply; an oversized conversion fails atomically.
UVTT source geometry stays GM-only. Configure token observers and reveal areas
explicitly; importing geometry does not activate a scene or enable vision.

The server filters tokens, difficult terrain, combat and logs before HTTP/live
serialization. Fogged background downloads are redacted PNG copies, even when the
same asset is referenced as a portrait. Original bytes stay available to the GM
and backups. A viewer who already received a revealed image can retain that copy;
covering it again cannot recall prior downloads. Player moves into hidden tiles
are rejected. Reloaded and reconnected clients receive current filtered state.

## Monster library and encounter presets

Create custom monsters manually, or search **Public catalogs → Monster library**.
SRD searches retrieve the public 2014 monster collection and filter names locally
on the server. Importing an entry prepares an editable draft; review and save it.
The copy retains edition, author, licence, source URL and immutable original JSON.
Rules text remains canonical English; interface controls have ten translations.
Reimport creates a separate draft, never overwrites an existing monster.

The 2014 rules material is derived from the System Reference Document 5.1 by
Wizards of the Coast LLC, available under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
The [5e-bits database](https://github.com/5e-bits/5e-database) carries its own MIT
licence. No monster artwork is imported from the rules API.

Add monsters to an encounter with explicit grid positions and hidden/visible
choices, then save the preset. Placing it in a scene requires confirmation and
creates independent GM-owned tokens and private HP/AC in one transaction.
Retries with the same operation ID create no duplicates. Monster copies and
presets never reach player snapshots. Existing tracker controls provide explicit
rolls for selected monster tokens; rolls never apply damage or sheet changes.
Referenced monsters cannot be removed until their encounter references are removed.
Limits: 200 entries per library kind, 32 members per encounter, 2 MiB total library
metadata per table, plus the existing image and board-operation quotas.

## Image browser and computer import

**Image library** accepts PNG/JPEG/WebP from the computer. The server validates and
copies original bytes into its managed table archive; it never depends on the
source path afterwards. Imported images have cached, server-generated 128 px previews and can be placed as
movable, resizable grid artwork or chosen for a monster portrait. Artwork preserves
aspect ratio and PNG transparency. Use the token editor for position, size and
visibility; artwork has no implied game effects. The current browser does not
offer rotation, cropping or map-layer stamps.

**Public catalogs → Image library** searches Wikimedia Commons only when requested.
Results show author/licence before import. The initial licence allowlist is CC0,
public domain, CC BY 4.0 and CC BY-SA 4.0; unknown and non-commercial licences are
excluded. Only raster files at most 8 MiB and 8192 pixels/side are accepted from
that provider, followed by normal managed-image validation. Imported previews are
local; search results are text entries until imported. Attribution stays visible
in the library. Respect the displayed licence when exporting or redistributing.

The server performs anonymous HTTPS GETs only to these fixed destinations:

| Host | Allowed path | Data sent |
| --- | --- | --- |
| `www.dnd5eapi.co` | `/api/2014/monsters` and selected monster IDs | Collection/entry ID only |
| `commons.wikimedia.org` | `/w/api.php` | Explicit public search term or selected page ID |
| `upload.wikimedia.org` | `/wikipedia/commons/` | Selected raster path returned by Commons |

No credentials, campaign names, notes, sheets or uploaded images are sent to those
providers. Redirects, arbitrary URLs, cookies and ambient proxy settings are
disabled. Requests have a 12-second limit, a whole-lookup 18-second deadline and
bounded response sizes. At most two transfers run concurrently; catalog requests
are limited to 30 per account per minute. There are no startup requests or retries.
Previously imported copies and manual editing remain available without providers.
