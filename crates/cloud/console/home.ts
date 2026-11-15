// Console Home: how the emulator is doing, what it holds, its traffic,
// ways to start, the latest requests, and every service by category.

import { catalogue, category, icon, look, ordered, serviceTile, tileOf } from "./catalog";
import { type Child, copy, duration, h, sender, time } from "./dom";
import { confirmDanger } from "./dialog";
import { BROWSERS, button, container, heading, iconButton, notice } from "./frame";
import { statusPill, operation } from "./requests";
import { API, services, state } from "./state";
import type { Page, RequestEvent, ServiceInventory } from "./types";

/** The window the traffic chart covers, in minutes. */
const MINUTES = 30;
/** How many requests Recent activity lists. */
const RECENT = 6;

/** The share of requests that worked, as a ring. */
function ring(share: number | null): SVGSVGElement {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 100 100");
  svg.setAttribute("class", "ring");
  const circle = (stroke: string, dash?: number) => {
    const c = document.createElementNS(ns, "circle");
    for (const [k, v] of Object.entries({ cx: 50, cy: 50, r: 42, fill: "none", stroke, "stroke-width": 9 })) {
      c.setAttribute(k, String(v));
    }
    if (dash != null) {
      c.setAttribute("stroke-linecap", "round");
      c.setAttribute("stroke-dasharray", "264");
      c.setAttribute("stroke-dashoffset", String(264 * (1 - dash)));
      c.setAttribute("transform", "rotate(-90 50 50)");
    }
    return c;
  };
  const text = (y: number, size: number, value: string, cls: string) => {
    const t = document.createElementNS(ns, "text");
    for (const [k, v] of Object.entries({ x: 50, y, "text-anchor": "middle", "font-size": size, class: cls })) {
      t.setAttribute(k, String(v));
    }
    t.textContent = value;
    return t;
  };
  svg.append(
    circle("var(--border)"),
    circle(share == null || share >= 0.9 ? "var(--ok)" : "var(--bad)", share ?? 1),
    text(52, 20, share == null ? "—" : `${Math.round(share * 100)}%`, "big"),
    text(68, 9, share == null ? "no requests" : "succeeded", "small"),
  );
  return svg;
}

function health(all: ServiceInventory[]): HTMLElement {
  const h0 = state.health;
  const answered = state.requests.length;
  const ok = state.requests.filter((r) => r.status < 400).length;
  const copyButton = button([icon("copy", 14), "Copy"], (e) => copy(location.origin, e.currentTarget as HTMLElement), "link");
  return container(
    "Emulator health",
    { info: "Live" },
    h(
      "div",
      { class: "body health" },
      ring(answered ? ok / answered : null),
      h(
        "dl",
        null,
        h("dt", null, "Endpoint"),
        h("dd", { class: "mono" }, location.host, copyButton),
        h("dt", null, "Up for"),
        h("dd", null, h0 ? duration(Date.now() - h0.started) : "…"),
        h("dt", null, "Version"),
        h("dd", null, h0 ? `ferrocloud ${h0.version}` : "…"),
        h("dt", null, "Region"),
        h("dd", null, h0?.region ?? "…"),
      ),
    ),
    h("div", { class: "chips" }, ordered(all).map((s) => h("span", { class: "chip ok" }, h("i"), s.title))),
  );
}

/** One figure in Your resources. */
interface Figure {
  service: string;
  glyph: string;
  value: number;
  label: string;
  href: string;
}

/** What every service holds, counted, plus what S3 and SQS keep inside. */
function figures(all: ServiceInventory[]): Figure[] {
  const out: Figure[] = [];
  for (const s of ordered(all)) {
    s.listings.forEach((listing, i) => {
      // A service's first kind always counts; the others once there are some.
      if (i > 0 && !listing.items.length) return;
      out.push({
        service: s.name,
        glyph: i === 0 ? look(s.name).icon : "cube",
        value: listing.items.length,
        label: `${s.title} ${listing.title.toLowerCase()}`,
        href: `#/s/${s.name}`,
      });
    });
    const items = s.listings[0]?.items ?? [];
    if (s.name === "s3" && items.length) {
      const objects = items.reduce((n, b) => n + (b.detail.objects ?? 0), 0);
      const size = items.reduce((n, b) => n + (b.detail.size ?? 0), 0);
      out.push({ service: "s3", glyph: "cube", value: objects, label: `Objects · ${bytesShort(size)}`, href: "#/s/s3" });
    }
    if (s.name === "sqs" && items.length) {
      const waiting = items.reduce((n, q) => n + (q.detail.counts?.visible ?? 0), 0);
      out.push({ service: "sqs", glyph: "messages", value: waiting, label: "Messages waiting", href: "#/s/sqs" });
    }
  }
  return out;
}

function bytesShort(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = n / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}

function resources(all: ServiceInventory[]): HTMLElement {
  const list = figures(all);
  return container(
    "Your resources",
    { info: "Everything the emulator holds" },
    list.length
      ? h(
          "div",
          { class: "figures" },
          list.map((f) =>
            h(
              "a",
              { href: f.href },
              tileOf(f.glyph, category(f.service).tile, 40),
              h("span", null, h("strong", null, f.value.toLocaleString()), h("span", { class: "link" }, f.label)),
            ),
          ),
        )
      : h("div", { class: "body muted" }, "Nothing yet. Create a bucket or a queue and it shows here."),
  );
}

/** Requests per minute over the window, and the errors among them. */
function perMinute(requests: RequestEvent[]): { all: number[]; bad: number[] } {
  const now = Date.now();
  const all = new Array<number>(MINUTES).fill(0);
  const bad = new Array<number>(MINUTES).fill(0);
  for (const r of requests) {
    const ago = Math.floor((now - r.time) / 60000);
    if (ago < 0 || ago >= MINUTES) continue;
    all[MINUTES - 1 - ago] += 1;
    if (r.status >= 400) bad[MINUTES - 1 - ago] += 1;
  }
  return { all, bad };
}

/** A line through `values`, scaled to `top`, in a 640 × 160 box. */
function line(values: number[], top: number, close: boolean): string {
  const step = 640 / (values.length - 1);
  const y = (v: number) => 150 - (v / top) * 130;
  const points = values.map((v, i) => `${(i * step).toFixed(1)} ${y(v).toFixed(1)}`);
  const path = `M${points.join(" L")}`;
  return close ? `${path} L640 160 L0 160 Z` : path;
}

function traffic(): HTMLElement {
  const window = state.requests.filter((r) => r.time >= Date.now() - MINUTES * 60000);
  const { all, bad } = perMinute(window);
  const top = Math.max(4, ...all);
  const errors = window.filter((r) => r.status >= 400).length;
  const took = window.map((r) => r.duration_ms).sort((a, b) => a - b);
  const median = took.length ? took[Math.floor(took.length / 2)] : 0;
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 640 160");
  svg.setAttribute("preserveAspectRatio", "none");
  svg.setAttribute("class", "chart");
  svg.innerHTML =
    '<defs><linearGradient id="fill" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ec7211" stop-opacity=".35"/>' +
    '<stop offset="1" stop-color="#ec7211" stop-opacity="0"/></linearGradient></defs>' +
    '<g class="grid"><line x1="0" y1="20" x2="640" y2="20"/><line x1="0" y1="85" x2="640" y2="85"/><line x1="0" y1="150" x2="640" y2="150"/></g>';
  const path = (d: string, cls: string) => {
    const p = document.createElementNS(ns, "path");
    p.setAttribute("d", d);
    p.setAttribute("class", cls);
    return p;
  };
  svg.append(path(line(all, top, true), "area"), path(line(all, top, false), "requests"), path(line(bad, top, false), "errors"));
  const kpi = (label: string, value: string, cls = "") => h("div", { class: cls }, h("span", null, label), h("strong", null, value));
  return container(
    "API traffic",
    { info: `Last ${MINUTES} minutes` },
    h(
      "div",
      { class: "body" },
      h(
        "div",
        { class: "kpis" },
        kpi("Requests", window.length.toLocaleString()),
        kpi("In the last minute", String(all[MINUTES - 1])),
        kpi("Errors", String(errors), errors ? "bad" : ""),
        kpi("Median time", `${median} ms`),
      ),
      h("div", { class: "legend" }, h("span", null, h("i", { class: "requests" }), "Requests"), h("span", null, h("i", { class: "errors" }), "Errors")),
      svg,
    ),
  );
}

/** Ways to start, each with the command to copy. */
function start(): HTMLElement {
  const endpoint = location.origin;
  const ways: [string, string, string, string][] = [
    ["s3", "Create a bucket", "Store files", "aws s3 mb s3://photos"],
    ["sqs", "Create a queue", "Send work between services", "aws sqs create-queue --queue-name jobs"],
    ["iam", "Create a role", "For your app or Terraform", "aws iam create-role --role-name app …"],
  ];
  const row = (tile: HTMLElement, title: string, about: string, command: string) =>
    h(
      "button",
      { class: "way", title: "Copy the command", onclick: (e: Event) => copy(command, e.currentTarget as HTMLElement) },
      tile,
      h("span", null, h("b", null, title), h("small", null, about)),
      h("code", null, command),
    );
  return container(
    "Build something",
    { info: "Click to copy" },
    h(
      "div",
      { class: "body ways" },
      ways.map(([service, title, about, command]) => row(serviceTile(service, 34), title, about, command)),
      row(tileOf("lock", "linear-gradient(135deg, #c8511b, #ff9900)", 34), "Connect your tools", "CLI, SDKs, Terraform", `export AWS_ENDPOINT_URL=${endpoint}`),
    ),
  );
}

function recent(seen: number): HTMLElement {
  const latest = state.requests.slice(-RECENT).reverse();
  const rows = latest.map((r) =>
    h(
      "tr",
      { class: r.id > seen ? "new" : null, onclick: () => (location.hash = "#/requests") },
      h("td", { class: "mono" }, time(r.time)),
      h("td", null, h("span", { class: "svc" }, r.service ? serviceTile(r.service, 22) : null, r.service ? (services().find((s) => s.name === r.service)?.title ?? r.service) : "—")),
      h("td", null, operation(r)),
      h("td", { class: "link" }, r.resource),
      h("td", null, statusPill(r)),
      h("td", null, sender(r.user_agent)),
      h("td", { class: "mono" }, `${r.duration_ms} ms`),
    ),
  );
  return container(
    "Recent activity",
    { info: "Live", actions: [h("a", { class: "link", href: "#/requests" }, "View all requests →")] },
    latest.length
      ? h(
          "div",
          { class: "table" },
          h(
            "table",
            null,
            h("thead", null, h("tr", null, ["Time", "Service", "Operation", "Resource", "Status", "Sent by", "Took"].map((c) => h("th", null, c)))),
            h("tbody", null, rows),
          ),
        )
      : h("div", { class: "body muted" }, "Waiting for the first request. Point a tool here: ", h("code", null, `export AWS_ENDPOINT_URL=${location.origin}`)),
  );
}

function allServices(all: ServiceInventory[]): HTMLElement {
  return container(
    "All services",
    { info: "What this emulator speaks" },
    h(
      "div",
      { class: "catalogue" },
      catalogue(all).map(({ category, services: list }) =>
        h(
          "section",
          null,
          h("h3", null, category),
          list.map((s) => {
            const held = s.service?.listings.reduce((n, l) => n + l.items.length, 0) ?? 0;
            return h(
              "a",
              { href: s.planned ? null : `#/s/${s.name}`, class: s.planned ? "soon" : null },
              serviceTile(s.name, 36),
              h("span", null, h("b", null, s.title), h("small", null, s.planned ? `${look(s.name).about} · ${s.planned}` : look(s.name).about)),
              held ? h("span", { class: "badge" }, held) : null,
            );
          }),
        ),
      ),
    ),
  );
}

/** The Create menu: what each service with a browser can make. */
function createMenu(): Child {
  const makers = Object.entries(BROWSERS).filter(([, b]) => b.create);
  if (!makers.length) return null;
  const menu = h(
    "div",
    { class: "dropdown", hidden: true },
    makers.map(([name, b]) =>
      h("button", { onclick: () => b.create?.() }, serviceTile(name, 22), `Create ${b.noun ?? "one"}`),
    ),
  );
  const toggle = iconButton("plus", "Create", (e) => {
    e.stopPropagation();
    menu.hidden = !menu.hidden;
  }, "primary");
  addEventListener("click", () => (menu.hidden = true), { once: true });
  return h("span", { class: "dropdown-holder" }, toggle, menu);
}

function reset(): void {
  confirmDanger({
    title: "Empty the emulator?",
    warning: "Every bucket, object, queue, message, role and request it holds is deleted. This can't be undone.",
    typeName: "empty",
    action: "Empty the emulator",
    run: async () => {
      const response = await fetch(`${API}reset`, { method: "POST" });
      if (!response.ok) throw new Error("The emulator couldn't be emptied.");
      notice("The emulator is empty.");
    },
  });
}

export const home: Page = (main) => {
  let seen = state.last;
  const update = () => {
    const all = services();
    main.replaceChildren(
      heading(
        "Console Home",
        "Your local AWS, on this computer. Nothing here leaves it, and nothing is billed.",
        iconButton("refresh", "Reset data", reset),
        createMenu(),
      ),
      h(
        "div",
        { class: "grid" },
        h("div", { class: "c4" }, health(all)),
        h("div", { class: "c8" }, resources(all)),
        h("div", { class: "c7" }, traffic()),
        h("div", { class: "c5" }, start()),
        h("div", { class: "c12" }, recent(seen)),
        h("div", { class: "c12" }, allServices(all)),
      ),
    );
    seen = state.last;
  };
  update();
  return update;
};
