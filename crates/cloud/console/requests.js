// The request log: every request the emulator answered, newest first,
// and one opened with its headers and bodies.

"use strict";

/// A request's status, coloured: green under 400, red from there.
function statusPill(r) {
  return h("span", { class: `pill ${r.status < 400 ? "ok" : "bad"}` }, r.error ? `${r.status} ${r.error}` : String(r.status));
}

/// What a request did, as a line: the operation, or its method and path.
function operation(r) {
  return r.operation || `${r.method} ${r.path}`;
}

PAGES.requests = (main) => {
  const rows = () =>
    state.requests
      .filter((r) => matches([r.service, r.operation, r.resource, r.status, r.error]))
      .slice()
      .reverse()
      .map((r) => ({
        key: r.id,
        cells: [
          time(r.time),
          h("span", { class: "svc" }, r.service ? serviceTile(r.service, 22) : null, r.service || "—"),
          operation(r),
          r.resource || "",
          statusPill(r),
          `${r.duration_ms} ms`,
        ],
        mono: [0, 5],
        open: () => openRequest(r),
      }));
  const shown = table(["Time", "Service", "Operation", "Resource", "Status", "Took"], rows, () =>
    h(
      "div",
      { class: "empty" },
      h("strong", {}, state.filter ? "Nothing matches the filter" : "No requests yet"),
      state.filter ? null : h("p", {}, "Point a tool at this emulator and its requests appear here as they're answered:"),
      state.filter ? null : h("code", {}, `AWS_ENDPOINT_URL=${location.origin} aws s3 ls`),
    ),
  );
  main.replaceChildren(
    h(
      "header",
      { class: "heading" },
      h("span", { class: "tile", style: "--c:var(--accent);width:48px;height:48px" }, icon("activity", 26)),
      h("div", {}, h("h1", {}, "Requests"), h("p", { class: "lead" }, "Every request the emulator answered, newest first. Secrets in them are masked.")),
    ),
    h("div", { class: "toolbar" }, h("span", { class: "grow" }), filterBox(() => shown.update())),
    shown.el,
  );
  return shown.update;
};

async function openRequest(r) {
  const title = operation(r);
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
    statusPill(r),
    h(
      "dl",
      {},
      [
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
    h("h3", {}, "Request"),
    h("pre", {}, readable(detail.request_body) || "(no body)"),
    h("h3", {}, "Response"),
    h("pre", {}, readable(detail.response_body) || "(no body)"),
    h("h3", {}, "Request headers"),
    pairs(detail.request_headers),
    h("h3", {}, "Response headers"),
    pairs(detail.response_headers),
  );
}
