# Server interface design

The web interface is an operations surface for players and operators. Preserve
its parchment, leather and burgundy identity. Body text uses the existing local
Palatino/Georgia serif stack; no font or asset request leaves the server.
Use paper `#fbf6ea`, parchment `#f2e7ce`, ink `#2e241b`, leather `#2a221b`,
burgundy `#7a2e2a` and brass `#9a7a3f`. Burgundy identifies actions and selection;
the sidebar uses a light keyboard focus ring. Keep prose below 72 characters
where practical and controls at least 44 CSS pixels tall.

## Implemented navigation

- Global navigation: campaigns, devices and operator tools (operators only).
  It becomes a sidebar at 1000 pixels; smaller viewports use wrapping top navigation.
- Campaign navigation: server usage, members, invitations (owners only) and
  deletion/leave. Hash URLs preserve the selected section on reload and support
  browser Back. Invitations load only when opened.
- Destructive actions retain their explanation and confirmation. Campaign deletion
  still requires the exact campaign name. A separate page never authorizes deletion.
- Tables scroll within their own keyboard-focusable wrapper on narrow screens.
  Names wrap and remain inert text. Slow previous requests cannot replace a new view.

## Future M42–M47 integration (planning only)

Add Tables as a separate global workspace when M42 actually exists. Do not nest it
inside campaign administration: a table has its own membership and may be linked
to a journal campaign only explicitly. Keep Operator as a service administration
area; operator status does not confer GM capability or journal access.

A future table route should keep the scene/canvas as its primary workspace, with
contextual tools for maps, tokens, initiative, vision and encounters. Do not place
the canvas inside the current campaign usage layout or render disabled placeholder
features now. Persist table identity in its route, and enforce per-recipient
visibility on the server. Hidden content must never reach players through HTML,
API payloads, search or browser caches. Show distances in ft and m as required by
the desktop `docs/VTT.md`; combat never changes a journal or sheet implicitly.

## Verification and remaining work

Run `node --test scripts/test-web.cjs` with jsdom available through `NODE_PATH`
(the sibling desktop checkout already supplies it in `node_modules`). No new
runtime package is needed. These DOM tests cover navigation races, storage failure,
expired sessions, safe names and keyboard skip navigation; they do not replace
real API permission tests or visual review.

The existing server interface is English-only. This pass reuses its visible copy;
all ten desktop locales still need a server-specific catalog and coverage gate.
The desktop localization gate does not prove server translation coverage. Before
the wider GM interface ships, complete that localization work and verify long
labels, CJK and RTL with real translations.
