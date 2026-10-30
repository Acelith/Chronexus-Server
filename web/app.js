// Chronexus Server web interface (M25.11). Plain module, no dependencies.
// Signs in through the JSON API and keeps the Bearer token in this tab's session storage
// only: no cookies. Every action calls the same API and permission checks as the desktop.
// All server text is inserted with textContent: campaign names and display names are
// untrusted and never interpreted as HTML.

const TOKEN_KEY = "chronexus-web-session";
const { t } = window.ChronexusI18n;
const main = document.getElementById("main");
const nav = document.getElementById("nav");
const language = document.getElementById("language");
/** Static page text and the language menu; called again after a language change. */
function staticText() {
  document.querySelector(".skip").textContent = t("skip");
  nav.setAttribute("aria-label", t("sections"));
  document.getElementById("language-label").textContent = t("language");
  document.getElementById("confirm-cancel").textContent = t("cancel");
}
language.replaceChildren(...window.ChronexusI18n.locales.map(([code, name]) =>
  el("option", { value: code, lang: code, selected: code === window.ChronexusI18n.locale, text: name })));
language.addEventListener("change", () => {
  window.ChronexusI18n.setLocale(language.value);
  staticText();
  route();
});
document.querySelector(".skip").addEventListener("click", (event) => {
  event.preventDefault();
  main.focus();
  main.scrollIntoView();
});
let me = null;
let memoryToken = null;
let viewVersion = 0;
let activeTableSocket = null;

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
const when = (iso) => (iso ? new Date(iso).toLocaleString(window.ChronexusI18n.locale) : t("never"));
const roleName = (role) => (["owner", "player", "reader", "gm", "spectator"].includes(role) ? t(`role.${role}`) : role);
const kindName = (kind) => (window.ChronexusI18n.has(`kind.${kind}`) ? t(`kind.${kind}`) : kind);
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
    throw new ApiError(401, t("sessionEnded"));
  }
  let value;
  try { value = text ? JSON.parse(text) : null; }
  catch { throw new ApiError(response.status, t("requestFailed", { status: response.status })); }
  if (!response.ok) throw new ApiError(response.status, value?.error ?? t("requestFailed", { status: response.status }));
  return value;
}
function show(...nodes) {
  main.classList.toggle("sign-in", !me);
  main.replaceChildren(...nodes.filter(node => node !== null && node !== undefined && node !== false));
  document.title = `${main.querySelector("h1")?.textContent ?? "Chronexus Server"} | Chronexus Server`;
  main.focus();
}
function errorBox(error) {
  return el("p", { class: "error", role: "alert", text: error.message || String(error) });
}
function loading(text = t("loading")) {
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
    body.append(el("label", { for: "confirm-name", text: t("typeName", { name: typedName }) }), input);
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
  status.replaceChildren(el("span", { role: "status", text: t("working") }));
  try {
    const message = await action();
    status.replaceChildren(el("span", { role: "status", text: message ?? t("done") }));
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
    link("#/campaigns", t("nav.campaigns")),
    link("#/tables", t("nav.tables")),
    link("#/devices", t("nav.devices")),
    me.operator ? link("#/operator", t("nav.operator")) : null,
    el("button", {
      type: "button",
      text: t("signOutAs", { name: me.displayName }),
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
  if (activeTableSocket) { activeTableSocket.close(); activeTableSocket = null; }
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
    return show(errorBox(error), el("a", { href: "#/campaigns", text: t("nav.campaigns") }));
  }
  if (version !== viewVersion) return;
  const [, section, id, panel] = hash.split("/");
  const current = `#/${section === "campaign" ? "campaigns" : section === "table" ? "tables" : section || "campaigns"}`;
  renderNav(current);
  if (section === "campaign" && id) {
    try { return campaignView(decodeURIComponent(id), undefined, panel); }
    catch (error) { return show(errorBox(error)); }
  }
  if (section === "devices") return devicesView();
  if (section === "tables") return tablesView();
  if (section === "table" && id) return tableView(decodeURIComponent(id));
  if (section === "operator" && me.operator) return operatorView();
  return campaignsView();
}
window.addEventListener("hashchange", route);

async function tablesView() {
  const version = ++viewVersion;
  loading();
  try {
    const tables = await api("GET", "/api/v1/tables");
    if (version !== viewVersion) return;
    const name = el("input", { required: true, maxlength: "200", id: "table-name" });
    const code = el("input", { required: true, id: "table-code", autocomplete: "off" });
    const status = el("div", { role: "status" });
    const create = el("button", { class: "primary", type: "submit", text: t("createTable") });
    const join = el("button", { type: "submit", text: t("joinTable") });
    show(el("h1", { text: t("tablesTitle") }),
      tables.length ? el("ul", { class: "plain" }, tables.map((table) => el("li", { class: "campaign-row" },
        el("a", { href: `#/table/${encodeURIComponent(table.id)}`, text: table.name }),
        el("span", { class: "role", text: roleName(table.role) })))) : el("p", { class: "muted", text: t("tablesEmpty") }),
      el("div", { class: "table-actions" },
        me.gmEnabled ? el("form", { class: "card", onsubmit: (event) => { event.preventDefault(); act(create, status, async () => { const table = await api("POST", "/api/v1/tables", { name: name.value.trim() }); location.hash = `#/table/${table.id}`; return t("done"); }); } },
          el("h2", { text: t("createTable") }), el("label", { for: "table-name", text: t("tableName") }), name, create) : null,
        el("form", { class: "card", onsubmit: (event) => { event.preventDefault(); act(join, status, async () => { const table = await api("POST", "/api/v1/tables/join", { inviteCode: code.value.trim() }); location.hash = `#/table/${table.id}`; return t("done"); }); } },
          el("h2", { text: t("joinTable") }), el("label", { for: "table-code", text: t("inviteCode") }), code, join)), status);
  } catch (error) { if (version === viewVersion) show(errorBox(error)); }
}

async function tableView(id) {
  const version = ++viewVersion;
  loading();
  try {
    const table = await api("GET", `/api/v1/tables/${encodeURIComponent(id)}`);
    const loadGrants = async () => {
      const items = [];
      let after = "";
      while (version === viewVersion) {
        const page = await api("GET", `/api/v1/tables/${encodeURIComponent(id)}/journal-grants${after ? `?after=${encodeURIComponent(after)}` : ""}`);
        items.push(...page);
        if (page.length < 100) break;
        const last = page.at(-1);
        after = `${last.campaignId}/${last.kind}/${last.itemId}`;
      }
      return items;
    };
    const [seats, grants] = await Promise.all([
      api("GET", `/api/v1/tables/${encodeURIComponent(id)}/seats`),
      table.role === "gm" ? loadGrants() : Promise.resolve([]),
    ]);
    if (version !== viewVersion) return;
    const status = el("div", { role: "status" });
    const liveStatus = el("span", { class: "live-status", role: "status", text: t("liveConnecting") });
    const online = el("span", { class: "muted" });
    const log = el("ol", { class: "live-log", "aria-live": "polite" });
    const message = el("input", { id: "live-message", maxlength: "500", required: true, autocomplete: "off" });
    const sendButton = el("button", { type: "submit", class: "primary", text: t("liveSend"), disabled: true });
    let socket = null;
    let revision = table.revision;
    let lastEvent = 0;
    let pending = null;
    const seen = new Set();
    const operationId = () => {
      const bytes = crypto.getRandomValues(new Uint8Array(16));
      const millis = Date.now();
      for (let i = 0; i < 6; i++) bytes[i] = Math.floor(millis / 2 ** ((5 - i) * 8)) & 255;
      bytes[6] = (bytes[6] & 15) | 0x70;
      bytes[8] = (bytes[8] & 63) | 0x80;
      const hex = [...bytes].map(n => n.toString(16).padStart(2, "0")).join("");
      return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
    };
    const connect = async () => {
      if (version !== viewVersion) return;
      liveStatus.textContent = t("liveConnecting");
      try {
        const ticket = await api("POST", `/api/v1/tables/${encodeURIComponent(id)}/live-ticket`);
        if (version !== viewVersion) return;
        const url = new URL(`/api/v1/tables/${encodeURIComponent(id)}/live`, location.href);
        url.protocol = location.protocol === "https:" ? "wss:" : "ws:";
        url.searchParams.set("ticket", ticket.ticket);
        url.searchParams.set("since", String(lastEvent));
        const connection = new WebSocket(url);
        let ready = false;
        socket = connection;
        activeTableSocket = socket;
        const current = () => version === viewVersion && socket === connection;
        const acknowledge = () => {
          if (message.value.trim() === pending.text) message.value = "";
          pending = null;
        };
        socket.onopen = () => { if (current()) sendButton.disabled = true; };
        socket.onmessage = (event) => {
          if (!current()) return;
          let data;
          try { data = JSON.parse(event.data); } catch { return; }
          if (data.type === "state") {
            revision = data.revision;
            if (data.reset) { seen.clear(); log.replaceChildren(); lastEvent = 0; }
            online.textContent = t("liveOnline", { count: data.online });
            for (const item of data.events ?? []) {
              if (seen.has(item.operationId)) continue;
              seen.add(item.operationId);
              if (pending?.operationId === item.operationId) acknowledge();
              lastEvent = Math.max(lastEvent, item.revision);
              log.append(el("li", {}, el("strong", { text: item.authorName }), document.createTextNode(`: ${item.text}`)));
            }
            // Reconcile committed events first, then retry the exact unacknowledged
            // operation once per connection. The server checks its ID and revision.
            if (!ready && pending) connection.send(JSON.stringify(pending));
            ready = true;
            liveStatus.textContent = t("liveConnected");
            sendButton.disabled = table.role === "spectator" || !!pending;
          } else if (data.type === "presence") online.textContent = t("liveOnline", { count: data.online });
          else if (data.type === "ack" && pending) { revision = Math.max(revision, data.revision); acknowledge(); sendButton.disabled = table.role === "spectator" || !ready; }
          else if (data.type === "error") { pending = null; sendButton.disabled = table.role === "spectator" || !ready; status.replaceChildren(el("p", { class: "error", role: "alert", text: t("liveSendFailed") })); }
        };
        socket.onclose = () => {
          if (!current()) return;
          sendButton.disabled = true;
          liveStatus.textContent = t("liveReconnecting");
          setTimeout(connect, 2000);
        };
      } catch (error) {
        if (version !== viewVersion) return;
        liveStatus.textContent = t("liveReconnecting");
        status.replaceChildren(errorBox(error));
        setTimeout(connect, 5000);
      }
    };
    const invite = (role, label) => el("button", { type: "button", text: label, onclick: async (event) => {
      const button = event.currentTarget;
      button.disabled = true;
      status.replaceChildren(el("span", { text: t("working") }));
      try {
        const result = await api("POST", `/api/v1/tables/${encodeURIComponent(id)}/invites`, { role });
        status.replaceChildren(el("p", { text: t("copyInvite") }), el("code", { class: "secret", text: result.code }));
      } catch (error) { status.replaceChildren(errorBox(error)); }
      finally { button.disabled = false; }
    } });
    show(el("a", { href: "#/tables", text: t("nav.tables") }), el("h1", { text: table.name }),
      el("p", { class: "muted", text: roleName(table.role) }),
      table.role === "gm" ? el("div", { class: "actions" }, invite("player", t("invitePlayer")), invite("spectator", t("inviteSpectator"))) : null,
      status,
      el("section", { class: "table-roster" }, el("h2", { text: t("tableSeats") }),
        el("ul", { class: "plain" }, seats.map(seat => el("li", { class: "campaign-row" },
          el("span", { text: seat.displayName }), el("span", { class: "role", text: roleName(seat.role) }),
          seat.role !== "gm" && (table.role === "gm" || seat.accountId === me.accountId) ? el("button", {
            type: "button", text: seat.accountId === me.accountId ? t("tableLeave") : t("tableRemove"),
            onclick: async event => {
              const own = seat.accountId === me.accountId;
              if (!await confirmAction(own ? t("tableLeave") : t("tableRemoveConfirm", { name: seat.displayName }),
                own ? [t("tableLeaveConfirm")] : [], own ? t("tableLeave") : t("tableRemove"))) return;
              await act(event.currentTarget, status, async () => {
                await api("DELETE", `/api/v1/tables/${encodeURIComponent(id)}/seats/${encodeURIComponent(seat.accountId)}`);
                if (own) location.hash = "#/tables"; else tableView(id);
                return t("done");
              });
            },
          }) : null)))),
      table.role === "gm" ? el("section", { class: "gm-shared" }, el("h2", { text: t("gmShared") }),
        grants.length ? el("ul", { class: "plain" }, grants.map(item => el("li", {},
          el("strong", { text: item.payload?.title || kindName(item.kind) }),
          el("span", { class: "muted", text: ` · ${item.authorName}` })))) : el("p", { class: "muted", text: t("gmSharedEmpty") })) : null,
      el("section", { class: "live-panel", "aria-label": t("liveTitle") },
        el("div", { class: "live-heading" }, el("h2", { text: t("liveTitle") }), liveStatus, online),
        log,
        table.role === "spectator" ? null : el("form", { class: "live-compose", onsubmit: event => {
          event.preventDefault();
          if (sendButton.disabled || pending || socket?.readyState !== WebSocket.OPEN || !message.value.trim()) return;
          pending = { operationId: operationId(), baseRevision: revision, text: message.value.trim(), audience: "public" };
          socket.send(JSON.stringify(pending));
          sendButton.disabled = true;
        } }, el("label", { for: "live-message", text: t("liveMessage") }), message, sendButton)));
    connect();
  } catch (error) { if (version === viewVersion) show(errorBox(error)); }
}

// ---------- views ----------
function signIn() {
  const status = el("div");
  const username = el("input", { id: "username", autocomplete: "username", required: true });
  const password = el("input", { id: "password", type: "password", autocomplete: "current-password", required: true });
  const submit = el("button", { class: "primary", type: "submit", text: t("signIn") });
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
        return t("signedIn");
      });
    },
  },
    el("label", { for: "username", text: t("username") }), username,
    el("label", { for: "password", text: t("password") }), password,
    el("div", { class: "actions" }, submit), status);
  show(
    el("h1", { text: t("signIn") }),
    el("p", { class: "muted", text: t("signInIntro") }),
    el("div", { class: "card" }, form),
    el("p", { class: "note", text: t("signInNote") }),
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
      : el("p", { class: "muted", text: t("campaignsEmpty") });
    show(el("h1", { text: t("campaignsTitle") }), list,
      el("p", { class: "note", text: t("campaignsNote") }));
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
  } catch (error) { if (version === viewVersion) show(el("p", {}, el("a", { href: "#/campaigns", text: t("backCampaigns") })), errorBox(error)); return; }
  if (version !== viewVersion) return;
  const owner = overview.role === "owner";
  const q = overview.quotas;
  const kinds = overview.myItems;
  const status = el("div");
  if (notice) status.append(el("p", { class: "note", role: "status", text: notice }));
  const header = [
    el("p", {}, el("a", { href: "#/campaigns", text: t("backCampaigns") })),
    el("h1", { text: overview.name }),
    el("p", { class: "muted", text: t("campaignMeta", { role: roleName(overview.role), members: overview.members, date: when(overview.createdAt) }) }),
    status,
  ];
  const summary = [
    el("h2", { text: t("keepTitle") }),
    el("div", { class: "grid" },
      el("div", { class: "stat" }, el("b", { text: bytes(overview.myContentBytes) }), t("statItems")),
      el("div", { class: "stat" }, el("b", { text: `${overview.myAttachments} · ${bytes(overview.myAttachmentBytes)}` }), t("statImages")),
      el("div", { class: "stat" }, el("b", { text: when(overview.myLastSavedAt) }), t("statLastSave")),
      el("div", { class: "stat" }, el("b", { text: String(overview.sharedByOthers) }), t("statShared"))),
    kinds.length
      ? el("div", { class: "table-wrap", tabindex: 0 }, el("table", {},
          el("caption", { class: "muted", text: t("kindsCaption") }),
          el("thead", {}, el("tr", {}, [t("thKind"), t("thPrivate"), t("thShared"), t("thSize"), t("thRevision")].map((h) => el("th", { scope: "col", text: h })))),
          el("tbody", {}, kinds.map((u) => el("tr", {},
            el("th", { scope: "row", text: kindName(u.kind) }), el("td", { text: String(u.privateItems) }),
            el("td", { text: String(u.sharedItems) }), el("td", { text: bytes(u.bytes) }),
            el("td", { text: String(u.highestRevision) }))))))
      : el("p", { class: "muted", text: t("kindsEmpty") }),
    el("p", { class: "note", text: t("limits", { items: q.campaignItems, text: bytes(q.campaignContentBytes), images: q.campaignAttachments, imageBytes: bytes(q.campaignAttachmentBytes), account: bytes(q.accountAttachmentBytes), used: bytes(overview.accountAttachmentBytes) }) }),
  ];
  const panels = [
    ["overview", t("keepTitle"), () => summary],
    ["members", t("members"), () => [el("h2", { text: t("members") }), membersTable(id, overview, members, status)]],
  ];
  if (owner) panels.push(["invitations", t("invitations"), () => invitesSection(id, status)]);
  panels.push(["access", owner ? t("deletePanel") : t("leavePanel"), () => [owner ? dangerSection(id, overview, status) : leaveSection(id, members, status)]]);
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
    el("thead", {}, el("tr", {}, [t("thName"), t("thRole"), t("thJoined"), owner ? t("thActions") : null].filter(Boolean).map((h) => el("th", { scope: "col", text: h })))),
    el("tbody", {}, members.map((m) => {
      const isSelf = m.accountId === me.accountId;
      const cells = [el("th", { scope: "row" }, m.displayName, isSelf ? el("span", { class: "muted", text: t("you") }) : null)];
      if (owner && m.role !== "owner") {
        const select = el("select", { "aria-label": t("roleOf", { name: m.displayName }) },
          ["player", "reader"].map((r) => el("option", { value: r, selected: r === m.role, text: roleName(r) })));
        select.addEventListener("change", () => act(select, status, async () => {
          await api("PATCH", `/api/v1/campaigns/${encodeURIComponent(id)}/members/${encodeURIComponent(m.accountId)}`, { role: select.value, baseRevision: m.revision });
          await campaignView(id, t("roleChanged", { name: m.displayName, role: roleName(select.value) }));
        }));
        cells.push(el("td", {}, select));
      } else {
        cells.push(el("td", { text: roleName(m.role) }));
      }
      cells.push(el("td", { text: when(m.joinedAt) }));
      if (owner) {
        if (m.role === "owner") cells.push(el("td"));
        else {
          const remove = el("button", { type: "button", text: t("remove") });
          remove.addEventListener("click", async () => {
            const ok = await confirmAction(t("removeTitle", { name: m.displayName }), [t("removeP1"), t("removeP2")], t("remove"));
            if (ok) act(remove, status, async () => {
              await api("DELETE", `/api/v1/campaigns/${encodeURIComponent(id)}/members/${encodeURIComponent(m.accountId)}?baseRevision=${m.revision}`);
              await campaignView(id, t("removed", { name: m.displayName }));
            });
          });
          cells.push(el("td", {}, remove));
        }
      }
      return el("tr", {}, cells);
    }))));
}

function invitesSection(id, status) {
  const list = el("div", {}, el("p", { role: "status", class: "muted", text: t("loadingInvites") }));
  const codeBox = el("div", { "aria-live": "polite" });
  const load = async () => {
    try {
      const invites = (await api("GET", `/api/v1/campaigns/${encodeURIComponent(id)}/invites`)).filter((i) => i.status === "pending");
      list.replaceChildren(invites.length
        ? el("ul", { class: "plain" }, invites.map((i) => {
            const revoke = el("button", { type: "button", text: t("revoke") });
            revoke.addEventListener("click", () => act(revoke, status, async () => {
              await api("DELETE", `/api/v1/campaigns/${encodeURIComponent(id)}/invites/${encodeURIComponent(i.id)}`);
              await load();
              return t("inviteRevoked");
            }));
            return el("li", {}, t("inviteLine", { role: roleName(i.role), date: when(i.expiresAt) }), revoke);
          }))
        : el("p", { class: "muted", text: t("noInvites") }));
    } catch (error) { list.replaceChildren(errorBox(error)); }
  };
  const create = (role) => {
    const button = el("button", { type: "button", text: role === "reader" ? t("inviteReader") : t("invitePlayer") });
    button.addEventListener("click", () => act(button, status, async () => {
      const invite = await api("POST", `/api/v1/campaigns/${encodeURIComponent(id)}/invites`, { role });
      codeBox.replaceChildren(el("p", { class: "note" },
        t("inviteCode", { role: roleName(invite.role), date: when(invite.expiresAt) }),
        el("code", { class: "secret", text: invite.code }),
        el("br"), t("inviteSecret")));
      await load();
      return t("inviteCreated");
    }));
    return button;
  };
  load();
  return [el("h2", { text: t("invitations") }), el("div", { class: "actions" }, create("player"), create("reader")), codeBox, list];
}

function leaveSection(id, members, status) {
  const mine = members.find((m) => m.accountId === me.accountId);
  const leave = el("button", { type: "button", class: "danger", text: t("leaveButton") });
  leave.addEventListener("click", async () => {
    const ok = await confirmAction(t("leaveTitle"), [t("leaveP1"), t("leaveP2")], t("leavePanel"));
    if (ok && mine) act(leave, status, async () => {
      await api("DELETE", `/api/v1/campaigns/${encodeURIComponent(id)}/members/${encodeURIComponent(me.accountId)}?baseRevision=${mine.revision}`);
      location.hash = "#/campaigns";
      return t("left");
    });
  });
  return el("div", {}, el("h2", { text: t("leavePanel") }), el("div", { class: "actions" }, leave));
}

function dangerSection(id, overview, status) {
  const remove = el("button", { type: "button", class: "danger", text: t("deleteButton") });
  remove.addEventListener("click", async () => {
    const ok = await confirmAction(t("deleteTitle"), [t("deleteP1"), t("deleteP2")], t("deleteOk"), overview.name);
    if (ok) act(remove, status, async () => {
      await api("POST", `/api/v1/campaigns/${encodeURIComponent(id)}/deletion`, { confirmName: overview.name });
      location.hash = "#/campaigns";
      return t("deleted");
    });
  });
  return el("div", {}, el("h2", { text: t("deletePanel") }), el("div", { class: "actions" }, remove));
}

async function devicesView() {
  const version = ++viewVersion;
  loading();
  const status = el("div");
  try {
    const sessions = await api("GET", "/api/v1/auth/sessions");
    if (version !== viewVersion) return;
    show(el("h1", { text: t("devicesTitle") }), status,
      el("ul", { class: "plain" }, sessions.map((s) => {
        const revoke = el("button", { type: "button", text: s.current ? t("signOutHere") : t("signOutDevice") });
        revoke.addEventListener("click", () => act(revoke, status, async () => {
          await api("DELETE", `/api/v1/auth/sessions/${encodeURIComponent(s.id)}`);
          if (s.current) { setToken(null); me = null; await route(); return t("signedOut"); }
          await devicesView();
          return t("deviceSignedOut");
        }));
        return el("li", {}, el("strong", { text: s.device }), s.current ? el("span", { class: "muted", text: t("thisBrowser") }) : null,
          el("div", { class: "muted", text: t("deviceLine", { used: when(s.lastUsedAt), expires: when(s.expiresAt) }) }), revoke);
      })),
      el("p", { class: "note", text: t("devicesNote") }));
  } catch (error) { if (version === viewVersion) show(errorBox(error)); }
}

async function operatorView() {
  const version = ++viewVersion;
  loading();
  try {
    const [s, accounts] = await Promise.all([api("GET", "/api/v1/operator/status"), api("GET", "/api/v1/operator/accounts")]);
    if (version !== viewVersion) return;
    const stat = (value, label) => el("div", { class: "stat" }, el("b", { text: String(value) }), label);
    show(el("h1", { text: t("service") }),
      el("p", { class: "note", text: t("operatorNote") }),
      el("div", { class: "grid" },
        stat(s.serverVersion, t("statVersion")), stat(s.schema, t("statSchema")), stat(bytes(s.databaseBytes), t("statDbSize")),
        stat(s.accounts, t("statAccounts")), stat(s.activeSessions, t("statSessions")), stat(s.campaigns, t("statCampaigns")),
        stat(`${s.contentItems} · ${bytes(s.contentBytes)}`, t("statItemsStored")), stat(`${s.attachments} · ${bytes(s.attachmentBytes)}`, t("statImagesStored")),
        stat(window.ChronexusI18n.has(`ai.${s.aiService}`) ? t(`ai.${s.aiService}`) : s.aiService, t("statAi"))),
      el("h2", { text: t("accounts") }),
      el("div", { class: "table-wrap", tabindex: 0 }, el("table", {},
        el("thead", {}, el("tr", {}, [t("thUser"), t("thCreated"), t("thState"), t("thSessions"), t("thCampaigns"), t("thStored")].map((h) => el("th", { scope: "col", text: h })))),
        el("tbody", {}, accounts.map((a) => el("tr", {},
          el("th", { scope: "row" }, a.displayName, el("div", { class: "muted", text: a.username })),
          el("td", { text: when(a.createdAt) }),
          el("td", { text: a.deleted ? t("state.deleted") : a.disabled ? t("state.disabled") : t("state.active") }),
          el("td", { text: String(a.activeSessions) }), el("td", { text: String(a.campaigns) }),
          el("td", { text: bytes(a.storedBytes) })))))));
  } catch (error) { if (version === viewVersion) show(errorBox(error)); }
}

staticText();
route();
