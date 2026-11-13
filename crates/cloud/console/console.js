// The ferrocloud console. It reads the emulator's own API under
// /_ferrocloud/: what each service holds (inventory) and the requests it
// answered. Pages are picked by the address's #: #/, #/s/<service> and
// #/requests. Everything shown is built with textContent, never as HTML,
// since names come from whoever called the APIs.

"use strict";

const API = "/_ferrocloud/";
const REFRESH_MS = 2000;
const REQUESTS_MS = 1000;
/// The most requests the log keeps, like the emulator's own.
const KEEP = 1000;

const state = {
  health: null,
  down: false,
  inventory: null,
  requests: [],
  last: 0,
  /// The page's own update, called when new data arrives.
  update: null,
  /// Per service, the listing shown.
  tab: {},
  filter: "",
};

// --- Small helpers --------------------------------------------------------

/// An element with attributes (on* ones as listeners) and children, which
/// may be text.
function h(tag, attrs, ...children) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs || {})) {
    if (value == null || value === false) continue;
    if (key.startsWith("on")) el.addEventListener(key.slice(2), value);
    else if (key === "class") el.className = value;
    else el.setAttribute(key, value === true ? "" : value);
  }
  for (const child of children.flat()) {
    if (child == null || child === false) continue;
    el.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
  return el;
}

const $ = (id) => document.getElementById(id);

async function get(path) {
  const response = await fetch(API + path, { cache: "no-store" });
  if (!response.ok) throw new Error(`${path}: ${response.status}`);
  return response.json();
}

function plural(n, word) {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

function time(ms) {
  return new Date(ms).toLocaleTimeString([], { hour12: false });
}

/// Whether any of `fields` holds the filter.
function matches(fields) {
  const needle = state.filter.trim().toLowerCase();
  return !needle || fields.some((f) => String(f).toLowerCase().includes(needle));
}

/// A record with the JSON kept in its text fields (policy documents)
/// opened up, so they read as JSON too.
function expanded(value) {
  if (typeof value === "string" && /^\s*[{[]/.test(value)) {
    try {
      return expanded(JSON.parse(value));
    } catch {
      return value;
    }
  }
  if (Array.isArray(value)) return value.map(expanded);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, expanded(v)]));
  }
  return value;
}

/// A body as text, laid out when it's JSON.
function readable(body) {
  if (!body || body.binary) return body ? `(${body.size} bytes of binary data)` : "";
  let text = body.text || "";
  try {
    text = JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    // Not JSON: as it was.
  }
  return body.truncated ? `${text}\n… (${body.size} bytes in all)` : text;
}

// --- The frame: top bar, side, drawer ---------------------------------------

function renderWhere() {
  const where = $("where");
  if (state.down) {
    where.replaceChildren(h("span", { class: "down" }, "The emulator doesn't answer"));
    return;
  }
  const health = state.health;
  if (!health) return;
  where.replaceChildren(
    h("span", { class: "up" }, `Running · ferrocloud ${health.version}`),
    h("span", { class: "mono" }, location.origin),
    h("span", {}, `account ${health.account || "000000000000"}`),
    h("span", {}, health.region || "us-east-1"),
  );
}

/// The count shown next to a service: everything its listings hold.
function held(service) {
  return service.listings.reduce((n, listing) => n + listing.items.length, 0);
}

function renderSide(route) {
  const link = (href, label, count, on) =>
    h("a", { href, class: on ? "on" : null }, h("span", {}, label), h("span", { class: "count" }, count));
  const services = state.inventory ? state.inventory.services : [];
  $("side").replaceChildren(
    link("#/", "Home", "", route.page === "home"),
    h("h4", {}, "SERVICES"),
    ...services.map((s) =>
      link(`#/s/${s.name}`, s.title, s.listings.length ? held(s) : "", route.page === "service" && route.name === s.name),
    ),
    h("h4", {}, "ACTIVITY"),
    link("#/requests", "Requests", state.requests.length || "", route.page === "requests"),
  );
}

function openDrawer(title, ...content) {
  const drawer = $("drawer");
  drawer.replaceChildren(
    h("header", {}, h("h2", {}, title), h("button", { class: "close", title: "Close", onclick: closeDrawer }, "×")),
    ...content,
  );
  drawer.hidden = false;
}

function closeDrawer() {
  $("drawer").hidden = true;
  document.querySelectorAll("tbody tr.on").forEach((row) => row.classList.remove("on"));
}

// --- Tables -----------------------------------------------------------------

/// A table whose rows `rows()` gives, each `{ key, cells, open, mono }`,
/// kept up to date by the returned function. `cells` are text or elements;
/// the opened row stays marked, by its key, as rows come and go.
function table(columns, rows, empty) {
  const body = h("tbody");
  const wrap = h("div");
  let opened = null;
  const update = () => {
    const shown = rows();
    if (!shown.length) {
      wrap.replaceChildren(empty());
      return;
    }
    body.replaceChildren(
      ...shown.map((row) => {
        const tr = h(
          "tr",
          {
            class: !$("drawer").hidden && row.key === opened ? "on" : null,
            onclick: () => {
              body.querySelectorAll("tr.on").forEach((r) => r.classList.remove("on"));
              tr.classList.add("on");
              opened = row.key;
              row.open();
            },
          },
          row.cells.map((cell, i) =>
            h("td", { class: row.mono && row.mono.includes(i) ? "mono" : null, title: typeof cell === "string" ? cell : null }, cell),
          ),
        );
        return tr;
      }),
    );
    if (!wrap.contains(body)) {
      const head = h("thead", {}, h("tr", {}, columns.map((c) => h("th", {}, c))));
      wrap.replaceChildren(h("div", { class: "scroll" }, h("table", {}, head, body)));
    }
  };
  update();
  return { el: wrap, update };
}

function filterBox(onChange) {
  return h("input", {
    class: "filter",
    type: "search",
    placeholder: "Filter",
    value: state.filter,
    oninput: (e) => {
      state.filter = e.target.value;
      onChange();
    },
  });
}

// --- Pages ------------------------------------------------------------------

function homePage(main) {
  const cards = h("div", { class: "cards" });
  const update = () => {
    const services = state.inventory ? state.inventory.services : [];
    cards.replaceChildren(
      ...services.map((s) =>
        h(
          "a",
          { class: "card", href: `#/s/${s.name}` },
          h("div", {}, h("span", { class: "title" }, s.title), h("span", { class: "short" }, s.name)),
          h(
            "div",
            { class: "facts" },
            s.listings.length
              ? s.listings.map((l) => `${l.title} ${l.items.length}`).join(" · ")
              : `${plural(s.operations, "operation")}, nothing kept`,
          ),
        ),
      ),
      h(
        "a",
        { class: "card", href: "#/requests" },
        h("div", {}, h("span", { class: "title" }, "Requests")),
        h("div", { class: "facts" }, `${plural(state.requests.length, "request")} answered`),
      ),
    );
  };
  main.replaceChildren(
    h("h1", {}, "AWS emulator"),
    h(
      "p",
      { class: "lead" },
      "Everything here lives on your computer. What you create with the AWS CLI, an SDK or Terraform shows up here as it happens.",
    ),
    cards,
    h("p", { class: "note" }, "Nothing is checked against policies or billed: every request is allowed."),
  );
  update();
  return update;
}

function servicePage(main, name) {
  const service = () => (state.inventory ? state.inventory.services.find((s) => s.name === name) : null);
  const s = service();
  if (!s) {
    main.replaceChildren(h("h1", {}, name), h("p", { class: "lead" }, state.inventory ? "No such service here." : "Loading…"));
    return () => state.inventory && service() && route();
  }
  main.replaceChildren(h("h1", {}, s.title), h("p", { class: "lead mono" }, s.name));
  if (s.error) {
    main.append(h("p", { class: "bad" }, s.error));
    return null;
  }
  if (!s.listings.length) {
    main.append(
      h(
        "div",
        { class: "empty" },
        `${s.title} keeps nothing to list: each request is answered on its own.`,
        h("br"),
        h("code", {}, s.name === "sts" ? "aws sts get-caller-identity" : `aws ${s.name} help`),
      ),
    );
    return null;
  }
  const tabs = h("div", { class: "tabs", role: "tablist" });
  const holder = h("div");
  let shown = null;
  const listing = () => service().listings[Math.min(state.tab[name] || 0, service().listings.length - 1)];
  const rows = () =>
    listing()
      .items.filter((item) => matches(item.cells))
      .map((item) => ({
        key: item.cells[0],
        cells: item.cells,
        mono: [1],
        open: () => openDrawer(item.cells[0], h("h3", {}, "RECORD"), h("pre", {}, JSON.stringify(expanded(item.detail), null, 2))),
      }));
  const showListing = () => {
    const l = listing();
    shown = table(l.columns, rows, () =>
      h("div", { class: "empty" }, state.filter ? "Nothing matches the filter." : `No ${l.title.toLowerCase()} yet.`),
    );
    holder.replaceChildren(shown.el);
  };
  const drawTabs = () =>
    tabs.replaceChildren(
      ...service().listings.map((l, i) =>
        h(
          "button",
          {
            role: "tab",
            class: i === (state.tab[name] || 0) ? "on" : null,
            onclick: () => {
              state.tab[name] = i;
              closeDrawer();
              drawTabs();
              showListing();
            },
          },
          l.title,
          h("span", { class: "n" }, l.items.length),
        ),
      ),
    );
  drawTabs();
  showListing();
  main.append(tabs, h("div", { class: "toolbar" }, filterBox(() => shown.update())), holder);
  return () => {
    if (!service()) return;
    drawTabs();
    shown.update();
  };
}

function requestsPage(main) {
  const rows = () =>
    state.requests
      .filter((r) => matches([r.service, r.operation, r.resource, r.status, r.error]))
      .slice()
      .reverse()
      .map((r) => ({
        key: r.id,
        cells: [
          time(r.time),
          r.service || "—",
          r.operation || `${r.method} ${r.path}`,
          r.resource || "",
          h("span", { class: r.status < 400 ? "ok" : "bad" }, r.error ? `${r.status} ${r.error}` : String(r.status)),
          `${r.duration_ms} ms`,
        ],
        mono: [0, 5],
        open: () => openRequest(r),
      }));
  const shown = table(["Time", "Service", "Operation", "Resource", "Status", "Took"], rows, () =>
    h(
      "div",
      { class: "empty" },
      state.filter ? "Nothing matches the filter." : "No requests yet. Point a tool at this emulator and they appear here as they're answered.",
      h("br"),
      h("code", {}, `AWS_ENDPOINT_URL=${location.origin} aws sts get-caller-identity`),
    ),
  );
  main.replaceChildren(
    h("h1", {}, "Requests"),
    h("p", { class: "lead" }, "Every request the emulator answered, newest first. Secrets in them are masked."),
    h("div", { class: "toolbar" }, filterBox(() => shown.update())),
    shown.el,
  );
  return shown.update;
}

async function openRequest(r) {
  const title = r.operation || `${r.method} ${r.path}`;
  openDrawer(title, h("p", { class: "dim" }, "Loading…"));
  let found;
  try {
    found = await get(`requests/${r.id}`);
  } catch {
    openDrawer(title, h("p", { class: "dim" }, "That request is no longer remembered."));
    return;
  }
  const { detail } = found;
  const pairs = (list) => h("dl", {}, list.flatMap(([k, v]) => [h("dt", {}, k), h("dd", { class: "mono" }, v)]));
  openDrawer(
    title,
    h(
      "dl",
      {},
      [
        ["Status", r.error ? `${r.status} ${r.error}` : r.status],
        ["Message", r.message],
        ["Service", r.service],
        ["Resource", r.resource],
        ["Region", r.region],
        ["Sent by", r.user_agent],
        ["Request ID", r.request_id],
        ["Took", `${r.duration_ms} ms`],
      ]
        .filter(([, v]) => v !== "" && v != null)
        .flatMap(([k, v]) => [h("dt", {}, k), h("dd", {}, String(v))]),
    ),
    h("h3", {}, "REQUEST"),
    h("pre", {}, readable(detail.request_body) || "(no body)"),
    h("h3", {}, "RESPONSE"),
    h("pre", {}, readable(detail.response_body) || "(no body)"),
    h("h3", {}, "REQUEST HEADERS"),
    pairs(detail.request_headers),
    h("h3", {}, "RESPONSE HEADERS"),
    pairs(detail.response_headers),
  );
}

// --- Routing and refreshing -------------------------------------------------

function current() {
  const [, page, name] = location.hash.split("/");
  if (page === "s" && name) return { page: "service", name: decodeURIComponent(name) };
  if (page === "requests") return { page: "requests" };
  return { page: "home" };
}

function route() {
  const where = current();
  const main = $("main");
  state.filter = "";
  closeDrawer();
  renderSide(where);
  if (where.page === "service") state.update = servicePage(main, where.name);
  else if (where.page === "requests") state.update = requestsPage(main);
  else state.update = homePage(main);
  main.focus();
}

function refreshed() {
  renderWhere();
  renderSide(current());
  if (state.update) state.update();
}

async function loadInventory() {
  try {
    const [health, inventory] = await Promise.all([get("health"), get("inventory")]);
    const changed = JSON.stringify(inventory) !== JSON.stringify(state.inventory);
    state.health = health;
    state.inventory = inventory;
    if (changed || state.down) {
      state.down = false;
      refreshed();
    }
  } catch {
    state.down = true;
    renderWhere();
  }
}

async function loadRequests() {
  try {
    const answer = await get(`requests?after=${state.last}`);
    // A reset starts the count again.
    if (answer.last < state.last) {
      state.requests = [];
      state.last = 0;
      return;
    }
    if (answer.requests.length) {
      state.requests = state.requests.concat(answer.requests).slice(-KEEP);
      state.last = answer.requests[answer.requests.length - 1].id;
      refreshed();
    }
  } catch {
    // The inventory says when the emulator is down.
  }
}

/// Runs `load` now and every `ms` while the tab is visible.
function every(ms, load) {
  load();
  setInterval(() => document.visibilityState === "visible" && load(), ms);
}

addEventListener("hashchange", route);
addEventListener("keydown", (e) => e.key === "Escape" && closeDrawer());
route();
every(REFRESH_MS, loadInventory);
every(REQUESTS_MS, loadRequests);
