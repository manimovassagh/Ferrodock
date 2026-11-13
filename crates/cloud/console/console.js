// The ferrocloud console's core: what every page shares. It reads the
// emulator's own API under /_ferrocloud/: what each service holds
// (inventory) and the requests it answered.
//
// Pages are picked by the address's #: #/ (home.js), #/s/<service>
// (service.js), #/s/<service>/<path> for a service's own browser (s3.js)
// and #/requests (requests.js). Each file registers itself in PAGES or
// BROWSERS; catalog.js says how each service looks. Everything shown is
// built with textContent, never as HTML, since names come from whoever
// called the APIs.

"use strict";

const API = "/_ferrocloud/";
const REFRESH_MS = 2000;
const REQUESTS_MS = 1000;
/// The most requests the log keeps, like the emulator's own.
const KEEP = 1000;

/// The pages, by name: `home`, `service`, `requests`. Each is
/// `(main, ...args) => update`, where `update` is called on new data.
const PAGES = {};

/// Per service name, a browser: `{ open(item), page(main, path), actions() }`.
/// `open` replaces showing a listing row's record, `page` draws
/// #/s/<service>/<path>, `actions` adds buttons next to the filter.
const BROWSERS = {};

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

/// A size for people: `512 B`, `1.4 MB`.
function bytes(n) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${n} B` : `${value.toFixed(1)} ${units[unit]}`;
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

/// XML laid out one element to a line, indented by depth. An element
/// holding only text (or nothing) stays on one line: `<Key>a.txt</Key>`.
function prettyXml(text) {
  const tokens = text.replace(/>\s+</g, "><").match(/<[^>]+>|[^<]+/g) || [];
  const lines = [];
  let depth = 0;
  // Whether the line being built is an opening tag with no child yet.
  let open = false;
  for (const token of tokens) {
    const tag = token.startsWith("<");
    const closing = token.startsWith("</");
    const single = token.startsWith("<?") || token.startsWith("<!") || token.endsWith("/>");
    if (!tag || (closing && open)) {
      lines[lines.length - 1] += token;
      if (closing) {
        depth -= 1;
        open = false;
      }
      continue;
    }
    if (closing) depth -= 1;
    lines.push("  ".repeat(Math.max(0, depth)) + token);
    open = !closing && !single;
    if (open) depth += 1;
  }
  return lines.join("\n");
}

/// A body as text, laid out when it's JSON or XML.
function readable(body) {
  if (!body) return "";
  if (body.streamed) return `(${bytes(body.size)} streamed by, not kept)`;
  if (body.binary) return `(${bytes(body.size)} of binary data)`;
  let text = body.text || "";
  if (/^\s*</.test(text) && !body.truncated) {
    text = prettyXml(text);
  } else {
    try {
      text = JSON.stringify(JSON.parse(text), null, 2);
    } catch {
      // Neither: as it was.
    }
  }
  return body.truncated ? `${text}\n… (${body.size} bytes in all)` : text;
}

// --- The frame: top bar, side, drawer ---------------------------------------

/// Copies `text`, and says so on `button` for a moment.
async function copy(text, button) {
  try {
    await navigator.clipboard.writeText(text);
    const was = button.textContent;
    button.classList.add("done");
    button.lastChild.textContent = "Copied";
    setTimeout(() => {
      button.classList.remove("done");
      button.lastChild.textContent = was;
    }, 1400);
  } catch {
    prompt("Copy this:", text);
  }
}

function renderWhere() {
  const where = $("where");
  if (state.down) {
    where.replaceChildren(h("span", { class: "status down" }, h("i"), "Not answering"));
    return;
  }
  const health = state.health;
  if (!health) return;
  where.replaceChildren(
    h("span", { class: "chip" }, health.region || "us-east-1"),
    h("span", { class: "status up" }, h("i"), "Running"),
  );
}

/// The count shown next to a service: everything its listings hold.
function held(service) {
  return service.listings.reduce((n, listing) => n + listing.items.length, 0);
}

function renderSide(route) {
  const link = (href, glyph, label, count, on, color) =>
    h(
      "a",
      { href, class: on ? "on" : null, style: color ? `--c:${color}` : null },
      icon(glyph, 17),
      h("span", { class: "label" }, label),
      h("span", { class: "count" }, count),
    );
  const services = state.inventory ? state.inventory.services : [];
  $("side").replaceChildren(
    link("#/", "home", "Overview", "", route.page === "home"),
    ...grouped(services).flatMap(({ group, services: list }) => [
      h("h4", {}, group),
      ...list.map((s) =>
        link(
          `#/s/${s.name}`,
          look(s.name).icon,
          s.title,
          s.listings.length ? held(s) : "",
          route.page === "service" && route.name === s.name,
          look(s.name).color,
        ),
      ),
    ]),
    h("h4", {}, "Activity"),
    link("#/requests", "activity", "Requests", state.requests.length || "", route.page === "requests"),
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
      wrap.replaceChildren(h("div", { class: "scroll panel" }, h("table", {}, head, body)));
    }
  };
  update();
  return { el: wrap, update };
}

function filterBox(onChange) {
  return h("input", {
    class: "filter",
    type: "search",
    placeholder: "Filter…",
    value: state.filter,
    oninput: (e) => {
      state.filter = e.target.value;
      onChange();
    },
  });
}

// --- Routing and refreshing -------------------------------------------------

function current() {
  const [, page, name, ...rest] = location.hash.split("/");
  if (page === "s" && name) return { page: "service", name: decodeURIComponent(name), path: rest.join("/") };
  if (page === "requests") return { page: "requests" };
  return { page: "home" };
}

function route() {
  const where = current();
  const main = $("main");
  state.filter = "";
  closeDrawer();
  // A page may take dropped files (a bucket); the next one doesn't.
  main.ondragenter = main.ondragleave = main.ondragover = main.ondrop = null;
  renderSide(where);
  const browser = where.page === "service" && where.path ? BROWSERS[where.name] : null;
  if (browser && browser.page) state.update = browser.page(main, where.path);
  else state.update = PAGES[where.page](main, where.name);
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

/// Starts once every script is in, so each service's browser is there.
function start() {
  addEventListener("hashchange", route);
  addEventListener("keydown", (e) => e.key === "Escape" && closeDrawer());
  route();
  every(REFRESH_MS, loadInventory);
  every(REQUESTS_MS, loadRequests);
}

addEventListener("DOMContentLoaded", start);
