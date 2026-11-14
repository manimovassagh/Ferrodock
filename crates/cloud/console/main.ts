// The console's start: the pages and the services' browsers, routing by
// the address's #, and keeping the data fresh.
//
// Files: types.ts (the API's shapes), dom.ts (building elements),
// state.ts (what's known, and fetching), catalog.ts (how services look),
// frame.ts (the frame and shared pieces), home.ts, service.ts and
// requests.ts (the pages), s3.ts and sqs.ts (services' own browsers).

import { $ } from "./dom";
import { BROWSERS, type Route, closeDrawer, renderPins, renderSide, renderStatus, setUpTopBar } from "./frame";
import { home } from "./home";
import { requestsPage } from "./requests";
import { s3Browser } from "./s3";
import { servicePage } from "./service";
import { sqsBrowser } from "./sqs";
import { get, state } from "./state";
import type { Health, Inventory, Page, RequestEvent, Update } from "./types";

const REFRESH_MS = 2000;
const REQUESTS_MS = 1000;
/** The most requests the console keeps, like the emulator. */
const KEEP = 1000;

BROWSERS.s3 = s3Browser;
BROWSERS.sqs = sqsBrowser;

const PAGES: Record<Route["page"], Page> = {
  home,
  service: servicePage,
  requests: requestsPage,
};

let update: Update = null;

function current(): Route {
  const [, page, name, ...rest] = location.hash.split("/");
  if (page === "s" && name) return { page: "service", name: decodeURIComponent(name), path: rest.join("/") };
  if (page === "requests") return { page: "requests", name: "", path: "" };
  return { page: "home", name: "", path: "" };
}

function route(): void {
  const where = current();
  const main = $("main");
  // The Requests link on a service's page keeps the service as filter.
  if (where.page !== "requests") state.filter = "";
  closeDrawer();
  // A page may take dropped files (a bucket); the next one doesn't.
  main.ondragenter = main.ondragleave = main.ondragover = main.ondrop = null;
  renderSide(where);
  const browser = where.page === "service" && where.path ? BROWSERS[where.name] : undefined;
  update = browser?.page ? browser.page(main, where.path) : PAGES[where.page](main, where.name);
  main.scrollTop = 0;
}

/** Brings the frame and the page up to date with new data. */
function refreshed(): void {
  renderStatus();
  renderPins();
  renderSide(current());
  // A service page waiting for the inventory to name its service.
  if (!update && current().page === "service" && state.inventory) route();
  else update?.();
}

async function loadInventory(): Promise<void> {
  try {
    const [health, inventory] = await Promise.all([get<Health>("health"), get<Inventory>("inventory")]);
    const changed = JSON.stringify(inventory) !== JSON.stringify(state.inventory);
    const wasDown = state.down;
    state.health = health;
    state.inventory = inventory;
    state.down = false;
    if (changed || wasDown) refreshed();
    else renderStatus();
  } catch {
    state.down = true;
    renderStatus();
  }
}

async function loadRequests(): Promise<void> {
  try {
    const answer = await get<{ last: number; requests: RequestEvent[] }>(`requests?after=${state.last}`);
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

/** Runs `load` now and every `ms` while the tab is visible. */
function every(ms: number, load: () => Promise<void>): void {
  void load();
  setInterval(() => {
    if (document.visibilityState === "visible") void load();
  }, ms);
}

function start(): void {
  setUpTopBar();
  addEventListener("hashchange", route);
  route();
  every(REFRESH_MS, loadInventory);
  every(REQUESTS_MS, loadRequests);
}

start();
