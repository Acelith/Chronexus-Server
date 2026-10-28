// Chronexus Server web interface (M25.11). Plain module, no dependencies.
// Signs in through the JSON API and keeps the Bearer token in this tab's session storage
// only: no cookies. Every action calls the same API and permission checks as the desktop.
// All server text is inserted with textContent: campaign names and display names are
// untrusted and never interpreted as HTML.

const TOKEN_KEY = "chronexus-web-session";
const main = document.getElementById("main");
const nav = document.getElementById("nav");
document.querySelector(".skip").addEventListener("click", (event) => {
  event.preventDefault();
  main.focus();
  main.scrollIntoView();
});
let me = null;
let memoryToken = null;
let viewVersion = 0;

// ---------- helpers ----------
function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value === undefined || value === null || value === false) continue;
    if (key.startsWith("on")) node.addEventListener(key.slice(2), value);
    else if (key === "text") node.textContent = value;
    else node.setAttribute(key, value === true ? "" : String(value));
  }
  for (const child of children.flat()) {
    if (child === null || child === undefined || child === false) continue;
    node.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
  return node;
}
const bytes = (n) =>
  n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KiB` : `${(n / 1048576).toFixed(1)} MiB`;
const when = (iso) => (iso ? new Date(iso).toLocaleString() : "never");
const roleName = (role) => ({ owner: "Owner", player: "Player", reader: "Reader" })[role] ?? role;
function token() {
  try { return sessionStorage.getItem(TOKEN_KEY) ?? memoryToken; } catch { return memoryToken; }
}
function setToken(value) {
  memoryToken = value;
  try { value ? sessionStorage.setItem(TOKEN_KEY, value) : sessionStorage.removeItem(TOKEN_KEY); } catch { /* private mode: memory only */ }
}
class ApiError extends Error {
  constructor(status, message) { super(message); this.status = status; }
}
async function api(method, path, body) {
  const headers = { Accept: "application/json" };
  const auth = token();
  if (auth) headers.Authorization = `Bearer ${auth}`;
  if (body !== undefined) headers["Content-Type"] = "application/json";
  const response = await fetch(path, {
    method, headers, body: body === undefined ? undefined : JSON.stringify(body),
    credentials: "omit", redirect: "error", cache: "no-store",
  });
  const text = await response.text();
  if (response.status === 401 && auth && auth === token()) {
    setToken(null); me = null;
    route();
    throw new ApiError(401, "Your session ended. Sign in again.");
  }
  let value;
  try { value = text ? JSON.parse(text) : null; }
  catch { throw new ApiError(response.status, `Request failed (HTTP ${response.status}).`); }
  if (!response.ok) throw new ApiError(response.status, value?.error ?? `Request failed (HTTP ${response.status}).`);
  return value;
}
function show(...nodes) {
  main.classList.toggle("sign-in", !me);
  main.replaceChildren(...nodes);
  document.title = `${main.querySelector("h1")?.textContent ?? "Chronexus Server"} | Chronexus Server`;
  main.focus();
}
function errorBox(error) {
  return el("p", { class: "error", role: "alert", text: error.message || String(error) });
}
function loading(text = "Loading…") {
  show(el("p", { role: "status", text }));
}
/** Accessible confirmation with an explanation; resolves true only on the explicit button. */
function confirmAction(title, paragraphs, okLabel, typedName) {
  const dialog = document.getElementById("confirm");
  document.getElementById("confirm-title").textContent = title;
  const ok = document.getElementById("confirm-ok");
  ok.textContent = okLabel;
  const body = document.getElementById("confirm-body");
  body.replaceChildren(...paragraphs.map((p) => el("p", { text: p })));
  let input = null;
  if (typedName) {
    input = el("input", { id: "confirm-name", autocomplete: "off" });
    body.append(el("label", { for: "confirm-name", text: `Type the exact name: ${typedName}` }), input);
    ok.disabled = true;
    input.addEventListener("input", () => { ok.disabled = input.value !== typedName; });
  } else {
    ok.disabled = false;
  }
  dialog.showModal();
  (input ?? document.getElementById("confirm-cancel")).focus();
  return new Promise((resolve) => {
    dialog.addEventListener("close", () => resolve(dialog.returnValue === "ok"), { once: true });
  });
}
/** Runs an action with a busy state; shows the error in place without losing the page. */
async function act(button, status, action) {
  button.disabled = true;
  status.replaceChildren(el("span", { role: "status", text: "Working…" }));
  try {
    const message = await action();
    status.replaceChildren(el("span", { role: "status", text: message ?? "Done." }));
  } catch (error) {
    status.replaceChildren(errorBox(error));
  } finally {
    button.disabled = false;
  }
}

// ---------- navigation ----------
function renderNav(current) {
  document.body.classList.toggle("authenticated", !!me);
  if (!me) { nav.hidden = true; nav.replaceChildren(); return; }
  const link = (hash, label) =>
    el("a", { href: hash, "aria-current": current === hash ? "page" : undefined, text: label });
  const items = [
    link("#/campaigns", "Campaigns"),
    link("#/devices", "Devices"),
    me.operator ? link("#/operator", "Operator") : null,
    el("button", {
      type: "button",
      text: `Sign out (${me.displayName})`,
      class: "sign-out",
      onclick: async () => {
        try { await api("POST", "/api/v1/auth/logout"); } catch { /* already ended */ }
        setToken(null); me = null; location.hash = "#/";
        route();
      },
    }),
  ];
  nav.replaceChildren(...items.filter(Boolean));
  nav.hidden = false;
}
async function route() {
  const version = ++viewVersion;
  const hash = location.hash || "#/";
  if (!token()) { renderNav(null); return signIn(); }
  try {
    if (!me) {
      const account = await api("GET", "/api/v1/account");
      if (version !== viewVersion) return;
      me = account;
    }
  } catch (error) {
    if (!token() || version !== viewVersion) return;
    return show(errorBox(error), el("a", { href: "#/campaigns", text: "Campaigns" }));
  }
  if (version !== viewVersion) return;
  const [, section, id, panel] = hash.split("/");
  const current = `#/${section === "campaign" ? "campaigns" : section || "campaigns"}`;
  renderNav(current);
  if (section === "campaign" && id) {
    try { return campaignView(decodeURIComponent(id), undefined, panel); }
    catch (error) { return show(errorBox(error)); }
  }
  if (section === "devices") return devicesView();
  if (section === "operator" && me.operator) return operatorView();
  return campaignsView();
}
window.addEventListener("hashchange", route);

// ---------- views ----------
function signIn() {
  const status = el("div");
  const username = el("input", { id: "username", autocomplete: "username", required: true });
  const password = el("input", { id: "password", type: "password", autocomplete: "current-password", required: true });
  const submit = el("button", { class: "primary", type: "submit", text: "Sign in" });
  const form = el("form", {
    onsubmit: (event) => {
      event.preventDefault();
      act(submit, status, async () => {
        const signed = await api("POST", "/api/v1/auth/login", {
          username: username.value.trim(), password: password.value, device: "Web browser",
        });
        password.value = "";
        setToken(signed.token);
        location.hash = "#/campaigns";
        await route();
        return "Signed in.";
      });
    },
  },
    el("label", { for: "username", text: "User name" }), username,
    el("label", { for: "password", text: "Password" }), password,
    el("div", { class: "actions" }, submit), status);
  show(
    el("h1", { text: "Sign in" }),
    el("p", { class: "muted", text: "Manage your shared campaigns, members, invitations and devices. Writing notes happens in the Chronexus desktop app." }),
    el("div", { class: "card" }, form),
    el("p", { class: "note", text: "The session is kept only in this browser tab and ends when you sign out or close it." }),
  );
  username.focus();
}

async function campaignsView() {
  const version = ++viewVersion;
  loading();
  try {
    const campaigns = await api("GET", "/api/v1/campaigns");
    if (version !== viewVersion) return;
    const list = campaigns.length
      ? el("ul", { class: "plain" }, campaigns.map((c) =>
          el("li", { class: "campaign-row" }, el("a", { href: `#/campaign/${encodeURIComponent(c.id)}`, text: c.name }),
            el("span", { class: "role", text: roleName(c.role) }))))
      : el("p", { class: "muted", text: "No shared campaign yet. Create one from the Sharing view of a campaign in the desktop app, or join with an invitation." });
    show(el("h1", { text: "Your campaigns on this server" }), list,
      el("p", { class: "note", text: "This page shows what the server holds. Changes a device has not sent yet are only known to that device." }));
  } catch (error) { if (version === viewVersion) show(errorBox(error)); }
}

async function campaignView(id, notice, selectedPanel = location.hash.split("/")[3]) {
  const version = ++viewVersion;
  loading();
  let overview, members;
  try {
    [overview, members] = await Promise.all([
      api("GET", `/api/v1/campaigns/${encodeURIComponent(id)}/overview`),
      api("GET", `/api/v1/campaigns/${encodeURIComponent(id)}/members`),
    ]);
  } catch (error) { if (version === viewVersion) show(el("p", {}, el("a", { href: "#/campaigns", text: "← Campaigns" })), errorBox(error)); return; }
  if (version !== viewVersion) return;
  const owner = overview.role === "owner";
  const q = overview.quotas;
  const kinds = overview.myItems;
  const status = el("div");
  if (notice) status.append(el("p", { class: "note", role: "status", text: notice }));
  const header = [
    el("p", {}, el("a", { href: "#/campaigns", text: "← Campaigns" })),
    el("h1", { text: overview.name }),
    el("p", { class: "muted", text: `Your role: ${roleName(overview.role)} · ${overview.members} members · created ${when(overview.createdAt)}` }),
    status,
  ];
  const summary = [
    el("h2", { text: "What you keep on the server" }),
    el("div", { class: "grid" },
      el("div", { class: "stat" }, el("b", { text: bytes(overview.myContentBytes) }), "your items"),
      el("div", { class: "stat" }, el("b", { text: `${overview.myAttachments} · ${bytes(overview.myAttachmentBytes)}` }), "your images here"),
      el("div", { class: "stat" }, el("b", { text: when(overview.myLastSavedAt) }), "last save accepted"),
      el("div", { class: "stat" }, el("b", { text: String(overview.sharedByOthers) }), "items others share")),
    kinds.length
      ? el("div", { class: "table-wrap", tabindex: 0 }, el("table", {},
          el("caption", { class: "muted", text: "Your items by kind (server revisions)" }),
          el("thead", {}, el("tr", {}, ["Kind", "Private copies", "Shared", "Size", "Highest revision"].map((h) => el("th", { scope: "col", text: h })))),
          el("tbody", {}, kinds.map((u) => el("tr", {},
            el("th", { scope: "row", text: u.kind }), el("td", { text: String(u.privateItems) }),
            el("td", { text: String(u.sharedItems) }), el("td", { text: bytes(u.bytes) }),
            el("td", { text: String(u.highestRevision) }))))))
      : el("p", { class: "muted", text: "You have not synchronized anything to this campaign." }),
    el("p", { class: "note", text: `The server cannot know changes still waiting on a device. Limits for the whole campaign: ${q.campaignItems} items and ${bytes(q.campaignContentBytes)} of text; ${q.campaignAttachments} images and ${bytes(q.campaignAttachmentBytes)}; ${bytes(q.accountAttachmentBytes)} of images per account (you use ${bytes(overview.accountAttachmentBytes)}).` }),
  ];
  const panels = [
    ["overview", "What you keep on the server", () => summary],
    ["members", "Members", () => [el("h2", { text: "Members" }), membersTable(id, overview, members, status)]],
  ];
  if (owner) panels.push(["invitations", "Invitations", () => invitesSection(id, status)]);
  panels.push(["access", owner ? "Delete" : "Leave", () => [owner ? dangerSection(id, overview, status) : leaveSection(id, members, status)]]);
  const active = panels.find(([key]) => key === selectedPanel) ?? panels[0];
  const panelNav = el("nav", { class: "panel-nav", "aria-label": overview.name },
    panels.map(([key, label]) => el("a", {
      href: `#/campaign/${encodeURIComponent(id)}/${key}`,
      "aria-current": key === active[0] ? "page" : undefined, text: label,
    })));
  show(...header, panelNav, el("section", { class: "campaign-panel" }, active[2]()));
}

function membersTable(id, overview, members, status) {
  const owner = overview.role === "owner";
  return el("div", { class: "table-wrap", tabindex: 0 }, el("table", {},
    el("thead", {}, el("tr", {}, ["Name", "Role", "Joined", owner ? "Actions" : null].filter(Boolean).map((h) => el("th", { scope: "col", text: h })))),
    el("tbody", {}, members.map((m) => {
      const isSelf = m.accountId === me.accountId;
      const cells = [el("th", { scope: "row" }, m.displayName, isSelf ? el("span", { class: "muted", text: " (you)" }) : null)];
      if (owner && m.role !== "owner") {
        const select = el("select", { "aria-label": `Role of ${m.displayName}` },
          ["player", "reader"].map((r) => el("option", { value: r, selected: r === m.role, text: roleName(r) })));
        select.addEventListener("change", () => act(select, status, async () => {
          await api("PATCH", `/api/v1/campaigns/${encodeURIComponent(id)}/members/${encodeURIComponent(m.accountId)}`, { role: select.value, baseRevision: m.revision });
          await campaignView(id, `${m.displayName} is now ${roleName(select.value).toLowerCase()}.`);
        }));
        cells.push(el("td", {}, select));
      } else {
        cells.push(el("td", { text: roleName(m.role) }));
      }
      cells.push(el("td", { text: when(m.joinedAt) }));
      if (owner) {
        if (m.role === "owner") cells.push(el("td"));
        else {
          const remove = el("button", { type: "button", text: "Remove" });
          remove.addEventListener("click", async () => {
            const ok = await confirmAction(`Remove ${m.displayName}?`, [
              "They lose access to this campaign. What they shared is withdrawn for everyone, and their devices clear what others shared at their next synchronization.",
              "They keep everything on their own computer. Devices that are offline learn it when they reconnect.",
            ], "Remove");
            if (ok) act(remove, status, async () => {
              await api("DELETE", `/api/v1/campaigns/${encodeURIComponent(id)}/members/${encodeURIComponent(m.accountId)}?baseRevision=${m.revision}`);
              await campaignView(id, `${m.displayName} was removed.`);
            });
          });
          cells.push(el("td", {}, remove));
        }
      }
      return el("tr", {}, cells);
    }))));
}

function invitesSection(id, status) {
  const list = el("div", {}, el("p", { role: "status", class: "muted", text: "Loading invitations…" }));
  const codeBox = el("div", { "aria-live": "polite" });
  const load = async () => {
    try {
      const invites = (await api("GET", `/api/v1/campaigns/${encodeURIComponent(id)}/invites`)).filter((i) => i.status === "pending");
      list.replaceChildren(invites.length
        ? el("ul", { class: "plain" }, invites.map((i) => {
            const revoke = el("button", { type: "button", text: "Revoke" });
            revoke.addEventListener("click", () => act(revoke, status, async () => {
              await api("DELETE", `/api/v1/campaigns/${encodeURIComponent(id)}/invites/${encodeURIComponent(i.id)}`);
              await load();
              return "Invitation revoked.";
            }));
            return el("li", {}, `${roleName(i.role)} · valid until ${when(i.expiresAt)} `, revoke);
          }))
        : el("p", { class: "muted", text: "No pending invitation." }));
    } catch (error) { list.replaceChildren(errorBox(error)); }
  };
  const create = (role) => {
    const button = el("button", { type: "button", text: `Invite a ${roleName(role).toLowerCase()}` });
    button.addEventListener("click", () => act(button, status, async () => {
      const invite = await api("POST", `/api/v1/campaigns/${encodeURIComponent(id)}/invites`, { role });
      codeBox.replaceChildren(el("p", { class: "note" },
        `Invitation code for a ${roleName(invite.role).toLowerCase()} (works once, until ${when(invite.expiresAt)}): `,
        el("code", { class: "secret", text: invite.code }),
        el("br"), "Send it privately; it is shown only now."));
      await load();
      return "Invitation created.";
    }));
    return button;
  };
  load();
  return [el("h2", { text: "Invitations" }), el("div", { class: "actions" }, create("player"), create("reader")), codeBox, list];
}

function leaveSection(id, members, status) {
  const mine = members.find((m) => m.accountId === me.accountId);
  const leave = el("button", { type: "button", class: "danger", text: "Leave the campaign" });
  leave.addEventListener("click", async () => {
    const ok = await confirmAction("Leave this shared campaign?", [
      "What the other players shared is cleared from your devices at their next synchronization, and the items you shared are withdrawn.",
      "Everything you wrote stays on your computer. You can join again only with a new invitation.",
    ], "Leave");
    if (ok && mine) act(leave, status, async () => {
      await api("DELETE", `/api/v1/campaigns/${encodeURIComponent(id)}/members/${encodeURIComponent(me.accountId)}?baseRevision=${mine.revision}`);
      location.hash = "#/campaigns";
      return "You left the campaign.";
    });
  });
  return el("div", {}, el("h2", { text: "Leave" }), el("div", { class: "actions" }, leave));
}

function dangerSection(id, overview, status) {
  const remove = el("button", { type: "button", class: "danger", text: "Delete the campaign on the server" });
  remove.addEventListener("click", async () => {
    const ok = await confirmAction("Delete the campaign on the server?", [
      "Every private and shared copy, upload and invitation of this campaign is deleted on the server, for every member. It cannot be undone.",
      "Each player keeps what is on their own computer. Server backups made earlier still contain the campaign until the operator removes them.",
    ], "Delete on the server", overview.name);
    if (ok) act(remove, status, async () => {
      await api("POST", `/api/v1/campaigns/${encodeURIComponent(id)}/deletion`, { confirmName: overview.name });
      location.hash = "#/campaigns";
      return "Campaign deleted on the server.";
    });
  });
  return el("div", {}, el("h2", { text: "Delete" }), el("div", { class: "actions" }, remove));
}

async function devicesView() {
  const version = ++viewVersion;
  loading();
  const status = el("div");
  try {
    const sessions = await api("GET", "/api/v1/auth/sessions");
    if (version !== viewVersion) return;
    show(el("h1", { text: "Devices signed in" }), status,
      el("ul", { class: "plain" }, sessions.map((s) => {
        const revoke = el("button", { type: "button", text: s.current ? "Sign out here" : "Sign out this device" });
        revoke.addEventListener("click", () => act(revoke, status, async () => {
          await api("DELETE", `/api/v1/auth/sessions/${encodeURIComponent(s.id)}`);
          if (s.current) { setToken(null); me = null; await route(); return "Signed out."; }
          await devicesView();
          return "Device signed out.";
        }));
        return el("li", {}, el("strong", { text: s.device }), s.current ? el("span", { class: "muted", text: " (this browser)" }) : null,
          el("div", { class: "muted", text: `Last used ${when(s.lastUsedAt)} · expires ${when(s.expiresAt)}` }), revoke);
      })),
      el("p", { class: "note", text: "A device signed out keeps its local data; it stops synchronizing until someone signs in there again." }));
  } catch (error) { if (version === viewVersion) show(errorBox(error)); }
}

async function operatorView() {
  const version = ++viewVersion;
  loading();
  try {
    const [s, accounts] = await Promise.all([api("GET", "/api/v1/operator/status"), api("GET", "/api/v1/operator/accounts")]);
    if (version !== viewVersion) return;
    const stat = (value, label) => el("div", { class: "stat" }, el("b", { text: String(value) }), label);
    show(el("h1", { text: "Service" }),
      el("p", { class: "note", text: "The operator role shows service health and account metadata only. It grants no access to campaign content through this interface or the API." }),
      el("div", { class: "grid" },
        stat(s.serverVersion, "server version"), stat(s.schema, "database schema"), stat(bytes(s.databaseBytes), "database size"),
        stat(s.accounts, "accounts"), stat(s.activeSessions, "active sessions"), stat(s.campaigns, "campaigns"),
        stat(`${s.contentItems} · ${bytes(s.contentBytes)}`, "items stored"), stat(`${s.attachments} · ${bytes(s.attachmentBytes)}`, "images stored"),
        stat(s.aiService, "server AI")),
      el("h2", { text: "Accounts" }),
      el("div", { class: "table-wrap", tabindex: 0 }, el("table", {},
        el("thead", {}, el("tr", {}, ["User", "Created", "State", "Sessions", "Campaigns", "Stored"].map((h) => el("th", { scope: "col", text: h })))),
        el("tbody", {}, accounts.map((a) => el("tr", {},
          el("th", { scope: "row" }, a.displayName, el("div", { class: "muted", text: a.username })),
          el("td", { text: when(a.createdAt) }),
          el("td", { text: a.deleted ? "deleted" : a.disabled ? "disabled" : "active" }),
          el("td", { text: String(a.activeSessions) }), el("td", { text: String(a.campaigns) }),
          el("td", { text: bytes(a.storedBytes) })))))));
  } catch (error) { if (version === viewVersion) show(errorBox(error)); }
}

route();
