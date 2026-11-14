// The frame every page sits in: the top bar (services menu, search,
// status), the pinned services, a service's own navigation on the left,
// the panel that opens on the right, and the building blocks pages share:
// containers, tables, the filter box, breadcrumbs and notices.

import { PINNED, catalogue, icon, look, ordered, serviceTile, tileOf } from "./catalog";
import { $, type Child, fill, h } from "./dom";
import { matches, services, state } from "./state";
import type { Browser, Item } from "./types";

/** Where the console is: from the address's #. */
export interface Route {
  page: "home" | "service" | "requests";
  name: string;
  path: string;
}

/** Per service name, its browser, if it has one (S3, SQS). */
export const BROWSERS: Record<string, Browser> = {};

// --- Top bar ----------------------------------------------------------------

export function renderStatus(): void {
  const health = state.health;
  if (state.down) {
    fill($("status"), h("span", { class: "pill down" }, h("i"), "Not answering"));
    return;
  }
  if (!health) return;
  fill(
    $("status"),
    h("span", { class: "pill" }, h("i"), "Running"),
    h("span", { class: "pill" }, health.region, h("small", null, "region")),
    h("span", { class: "pill" }, `Account ${health.account.replace(/(\d{4})(?=\d)/g, "$1-")}`),
  );
}

/** The bar of pinned services under the top one. */
export function renderPins(): void {
  const have = new Set(services().map((s) => s.name));
  fill(
    $("pins"),
    PINNED.filter((name) => have.has(name)).map((name) =>
      h("a", { href: `#/s/${name}` }, serviceTile(name, 18), services().find((s) => s.name === name)?.title ?? name),
    ),
    h("span", { class: "sep" }),
    h("a", { href: "#/requests", class: "muted" }, icon("activity", 15), "Requests"),
  );
}

/** The Services menu: every service by category. */
function servicesMenu(): HTMLElement {
  return h(
    "div",
    { class: "menu-panel" },
    catalogue(services()).map(({ category, services: list }) =>
      h(
        "section",
        null,
        h("h3", null, category),
        list.map((s) =>
          h(
            "a",
            { href: s.planned ? null : `#/s/${s.name}`, class: s.planned ? "soon" : null },
            serviceTile(s.name, 28),
            h("span", null, h("b", null, s.title), h("small", null, s.planned ? `${look(s.name).about} · ${s.planned}` : look(s.name).about)),
          ),
        ),
      ),
    ),
  );
}

export function toggleMenu(force?: boolean): void {
  const holder = $("menu");
  const open = force ?? holder.hidden;
  holder.hidden = !open;
  if (open) fill(holder, servicesMenu());
}

/** What the search finds: services, and anything a service holds by name. */
function searchResults(text: string): HTMLElement[] {
  const needle = text.trim().toLowerCase();
  if (!needle) return [];
  const found: HTMLElement[] = [];
  for (const s of ordered(services())) {
    if (`${s.title} ${s.name} ${look(s.name).about}`.toLowerCase().includes(needle)) {
      found.push(result(`#/s/${s.name}`, serviceTile(s.name, 24), s.title, look(s.name).about));
    }
    for (const listing of s.listings) {
      for (const item of listing.items) {
        if (!(item.cells[0] ?? "").toLowerCase().includes(needle)) continue;
        const browser = BROWSERS[s.name];
        const href = browser?.page ? `#/s/${s.name}/${encodeURIComponent(item.cells[0])}/` : `#/s/${s.name}`;
        found.push(result(href, serviceTile(s.name, 24), item.cells[0], `${s.title} · ${listing.title}`));
      }
    }
  }
  return found.slice(0, 12);
}

function result(href: string, tile: HTMLElement, title: string, note: string): HTMLElement {
  return h(
    "a",
    { href, onclick: () => closeSearch() },
    tile,
    h("span", null, h("b", null, title), h("small", null, note)),
  );
}

function closeSearch(): void {
  $("results").hidden = true;
  ($("search") as HTMLInputElement).value = "";
}

/** Wires the menu, the search and their shortcuts. */
export function setUpTopBar(): void {
  $("services").addEventListener("click", (e) => {
    e.stopPropagation();
    toggleMenu();
  });
  const search = $("search") as HTMLInputElement;
  const results = $("results");
  search.addEventListener("input", () => {
    const found = searchResults(search.value);
    results.hidden = !search.value.trim();
    fill(results, found.length ? found : h("p", { class: "muted" }, "Nothing here by that name."));
  });
  search.addEventListener("keydown", (e) => {
    if (e.key === "Enter") (results.querySelector("a") as HTMLAnchorElement | null)?.click();
  });
  addEventListener("click", (e) => {
    if (!(e.target as Element).closest("#menu")) toggleMenu(false);
    if (!(e.target as Element).closest(".search")) $("results").hidden = true;
  });
  addEventListener("keydown", (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      search.focus();
    }
    if (e.key === "Escape") {
      closeDrawer();
      toggleMenu(false);
      closeSearch();
    }
  });
}

// --- A service's navigation -------------------------------------------------

/** On a service's pages, its own navigation: what it holds, by kind. */
export function renderSide(route: Route): void {
  const side = $("side");
  const s = route.page === "service" ? services().find((x) => x.name === route.name) : undefined;
  document.body.classList.toggle("with-side", Boolean(s));
  if (!s) {
    side.replaceChildren();
    return;
  }
  const chosen = state.tab[s.name] ?? 0;
  fill(
    side,
    h("a", { class: "side-title", href: `#/s/${s.name}` }, serviceTile(s.name, 28), h("span", null, s.title)),
    s.listings.map((l, i) =>
      h(
        "a",
        {
          href: `#/s/${s.name}`,
          class: i === chosen && !route.path ? "on" : null,
          onclick: () => {
            state.tab[s.name] = i;
          },
        },
        l.title,
        h("span", { class: "count" }, l.items.length),
      ),
    ),
    h("hr"),
    h("a", { href: "#/requests", onclick: () => (state.filter = s.name) }, "Requests to ", s.title),
  );
}

// --- The panel on the right -------------------------------------------------

export function openDrawer(title: string, ...content: Child[]): void {
  const drawer = $("drawer");
  fill(
    drawer,
    h("header", null, h("h2", null, title), h("button", { class: "close", title: "Close", onclick: closeDrawer }, "×")),
    ...content,
  );
  drawer.hidden = false;
}

export function closeDrawer(): void {
  $("drawer").hidden = true;
  document.querySelectorAll("tbody tr.on").forEach((row) => row.classList.remove("on"));
}

// --- Shared building blocks -------------------------------------------------

/** `Services › S3 › Buckets`: links, the last one where you are. */
export function crumbs(...parts: [string, string | null][]): HTMLElement {
  const links: Child[] = [h("a", { href: "#/" }, "Console Home")];
  for (const [label, href] of parts) {
    links.push(h("span", { class: "sep" }, "›"), href ? h("a", { href }, label) : h("span", null, label));
  }
  return h("nav", { class: "crumbs" }, links);
}

/** A page's heading: its title, a line under it, and buttons. */
export function heading(title: Child, about: Child, ...actions: Child[]): HTMLElement {
  return h(
    "header",
    { class: "page-head" },
    h("div", null, h("h1", null, title), about ? h("p", null, about) : null),
    h("div", { class: "actions" }, actions),
  );
}

/** A panel with a header: title, count, an Info note, and actions. */
export function container(
  title: string,
  options: { count?: number | string; info?: string; actions?: Child[] },
  ...body: Child[]
): HTMLElement {
  return h(
    "section",
    { class: "container" },
    h(
      "header",
      null,
      h("h2", null, title, options.count != null ? h("span", { class: "count" }, ` (${options.count})`) : null),
      options.info ? h("span", { class: "info" }, options.info) : null,
      h("div", { class: "actions" }, options.actions ?? []),
    ),
    body,
  );
}

export function button(label: Child, onclick: (e: Event) => void, kind = ""): HTMLButtonElement {
  return h("button", { class: `button ${kind}`, onclick }, label);
}

/** A button with an icon. */
export function iconButton(glyph: string, label: string, onclick: (e: Event) => void, kind = ""): HTMLButtonElement {
  return button([icon(glyph, 16), label], onclick, kind);
}

/** A notice on top of the page, gone after a while or when closed. */
export function notice(text: string, kind: "ok" | "bad" = "ok"): void {
  const note = h(
    "div",
    { class: `notice ${kind}` },
    h("span", null, text),
    h("button", { class: "close", onclick: () => note.remove() }, "×"),
  );
  $("notices").append(note);
  setTimeout(() => note.remove(), 6000);
}

export function filterBox(onChange: () => void, placeholder = "Find by name"): HTMLInputElement {
  return h("input", {
    class: "filter",
    type: "search",
    placeholder,
    value: state.filter,
    oninput: (e: Event) => {
      state.filter = (e.target as HTMLInputElement).value;
      onChange();
    },
  });
}

/** A row of a table: its key (to keep it marked when opened), cells, and what opening it does. */
export interface Row {
  key: string | number;
  cells: Child[];
  open(): void;
  /** Columns shown in monospace. */
  mono?: number[];
}

/** A table kept up to date by the returned `update`; rows come from `rows()`. */
export function table(columns: string[], rows: () => Row[], empty: () => HTMLElement): { el: HTMLElement; update(): void } {
  const body = h("tbody");
  const wrap = h("div", { class: "table" });
  let opened: string | number | null = null;
  const update = () => {
    const shown = rows();
    if (!shown.length) {
      fill(wrap, empty());
      return;
    }
    fill(
      body,
      shown.map((row) => {
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
            h("td", { class: row.mono?.includes(i) ? "mono" : null, title: typeof cell === "string" ? cell : null }, cell),
          ),
        );
        return tr;
      }),
    );
    if (!wrap.contains(body)) {
      fill(wrap, h("table", null, h("thead", null, h("tr", null, columns.map((c) => h("th", null, c)))), body));
    }
  };
  update();
  return { el: wrap, update };
}

/** The message shown in an empty table: a title, a line, and a command. */
export function empty(title: string, line?: string, command?: string): HTMLElement {
  return h("div", { class: "empty" }, h("strong", null, title), line ? h("p", null, line) : null, command ? h("code", null, command) : null);
}

/** Rows of a listing filtered, for the generic service page. */
export function filtered(items: Item[]): Item[] {
  return items.filter((item) => matches(item.cells));
}

export { tileOf };
