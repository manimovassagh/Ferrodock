// The request log: every request the emulator answered, newest first,
// and one opened with its headers and bodies.

import { serviceTile } from "./catalog";
import { h, readable, sender, time } from "./dom";
import { container, crumbs, empty, filterBox, heading, openDrawer, table } from "./frame";
import { get, matches, services, state } from "./state";
import type { Detail, Page, RequestEvent } from "./types";

/** A request's status, green under 400 and red from there. */
export function statusPill(r: RequestEvent): HTMLElement {
  return h("span", { class: `status ${r.status < 400 ? "ok" : "bad"}` }, r.error ? `${r.status} ${r.error}` : String(r.status));
}

/** What a request did: its operation, or its method and path. */
export function operation(r: RequestEvent): string {
  return r.operation || `${r.method} ${r.path}`;
}

function serviceTitle(name: string): string {
  return services().find((s) => s.name === name)?.title ?? name;
}

export const requestsPage: Page = (main) => {
  const shown = table(
    ["Time", "Service", "Operation", "Resource", "Status", "Sent by", "Took"],
    () =>
      state.requests
        .filter((r) => matches([r.service, r.operation, r.resource, r.status, r.error]))
        .slice()
        .reverse()
        .map((r) => ({
          key: r.id,
          cells: [
            time(r.time),
            h("span", { class: "svc" }, r.service ? serviceTile(r.service, 22) : null, r.service ? serviceTitle(r.service) : "—"),
            operation(r),
            r.resource,
            statusPill(r),
            sender(r.user_agent),
            `${r.duration_ms} ms`,
          ],
          mono: [0, 6],
          open: () => void openRequest(r),
        })),
    () =>
      state.filter
        ? empty("Nothing matches the filter")
        : empty("No requests yet", "Point a tool at this emulator and its requests appear here as they're answered:", `AWS_ENDPOINT_URL=${location.origin} aws s3 ls`),
  );
  const box = container(
    "Requests",
    { count: state.requests.length, info: "Live", actions: [filterBox(() => shown.update(), "Filter by service, operation, resource, status")] },
    shown.el,
  );
  main.replaceChildren(
    crumbs(["Requests", null]),
    heading("Requests", "Every request the emulator answered, newest first. Secrets in them are masked."),
    box,
  );
  return () => {
    const count = box.querySelector("h2 .count");
    if (count) count.textContent = ` (${state.requests.length})`;
    shown.update();
  };
};

async function openRequest(r: RequestEvent): Promise<void> {
  const title = operation(r);
  openDrawer(title, h("p", { class: "muted" }, "Loading…"));
  let detail: Detail;
  try {
    detail = (await get<{ detail: Detail }>(`requests/${r.id}`)).detail;
  } catch {
    openDrawer(title, h("p", { class: "muted" }, "That request is no longer remembered."));
    return;
  }
  const pairs = (list: [string, string][]) =>
    h("dl", null, list.map(([k, v]) => [h("dt", null, k), h("dd", { class: "mono" }, v)]));
  const facts: [string, string][] = [
    ["Message", r.message],
    ["Service", r.service],
    ["Resource", r.resource],
    ["Region", r.region],
    ["Sent by", r.user_agent],
    ["Request ID", r.request_id],
    ["Took", `${r.duration_ms} ms`],
  ];
  openDrawer(
    title,
    statusPill(r),
    h("dl", null, facts.filter(([, v]) => v).map(([k, v]) => [h("dt", null, k), h("dd", null, v)])),
    h("h3", null, "Request"),
    h("pre", null, readable(detail.request_body) || "(no body)"),
    h("h3", null, "Response"),
    h("pre", null, readable(detail.response_body) || "(no body)"),
    h("h3", null, "Request headers"),
    pairs(detail.request_headers),
    h("h3", null, "Response headers"),
    pairs(detail.response_headers),
  );
}
