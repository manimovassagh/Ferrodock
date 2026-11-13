// The home page: where the emulator is, numbers at a glance, the services
// by group, and the latest requests as they come in.

"use strict";

/// How many recent requests the home page shows.
const RECENT = 8;

/// The banner on top: the name, that it runs, and the endpoint to copy.
function hero() {
  const health = state.health || {};
  const endpoint = location.origin;
  const copyButton = h("button", { class: "copy", onclick: (e) => copy(endpoint, e.currentTarget) }, icon("copy", 15), h("span", {}, "Copy"));
  return h(
    "section",
    { class: "hero" },
    h(
      "div",
      { class: "hero-text" },
      h("span", { class: "status up" }, h("i"), "Running on your computer"),
      h("h1", {}, "AWS emulator"),
      h("p", {}, "Point the AWS CLI, an SDK or Terraform here. What they create shows up on this page as it happens, and nothing leaves this computer."),
    ),
    h(
      "div",
      { class: "hero-card" },
      h("span", { class: "k" }, "Endpoint"),
      h("div", { class: "endpoint" }, h("code", {}, endpoint), copyButton),
      h(
        "div",
        { class: "facts" },
        h("span", {}, h("span", { class: "k" }, "Account "), health.account || "000000000000"),
        h("span", {}, h("span", { class: "k" }, "Region "), health.region || "us-east-1"),
        h("span", {}, h("span", { class: "k" }, "Version "), health.version || "…"),
      ),
    ),
  );
}

/// The numbers at a glance: what S3 holds, and the requests answered.
function stats() {
  const services = state.inventory ? state.inventory.services : [];
  const s3 = services.find((s) => s.name === "s3");
  const buckets = s3 && s3.listings[0] ? s3.listings[0].items : [];
  const sum = (field) => buckets.reduce((n, b) => n + (b.detail[field] || 0), 0);
  const minuteAgo = Date.now() - 60000;
  const lastMinute = state.requests.filter((r) => r.time >= minuteAgo).length;
  const errors = state.requests.filter((r) => r.status >= 400).length;
  const tile = (label, value, note, tone) =>
    h("div", { class: `stat ${tone || ""}` }, h("span", { class: "k" }, label), h("strong", {}, value), h("span", { class: "note" }, note));
  return h(
    "section",
    { class: "stats" },
    s3 ? tile("Buckets", buckets.length, "in S3") : null,
    s3 ? tile("Objects", sum("objects"), bytes(sum("size")) + " stored") : null,
    tile("Requests", state.requests.length, `${lastMinute} in the last minute`),
    tile("Errors", errors, errors ? "open the log to see why" : "none so far", errors ? "bad" : "ok"),
  );
}

/// One card per service, its tile, name, what it is and what it holds.
function serviceCard(s) {
  const counts = s.listings.length
    ? s.listings.map((l) => h("span", {}, h("strong", {}, l.items.length), ` ${l.title.toLowerCase()}`))
    : [h("span", { class: "dim" }, plural(s.operations, "operation"))];
  const basic = look(s.name).group === "Basics";
  return h(
    "a",
    { class: `card${basic ? " basic" : ""}`, href: `#/s/${s.name}`, style: `--c:${look(s.name).color}` },
    serviceTile(s.name, basic ? 32 : 44),
    h("div", { class: "card-text" }, h("span", { class: "title" }, s.title), h("span", { class: "about" }, look(s.name).about)),
    h("div", { class: "counts" }, counts),
  );
}

function servicesSection() {
  const services = state.inventory ? state.inventory.services : [];
  return h(
    "section",
    { class: "groups" },
    grouped(services).map(({ group, services: list }) =>
      h(
        "div",
        { class: `group${group === "Basics" ? " basics" : ""}` },
        h("h2", {}, group),
        h("div", { class: "cards" }, list.map(serviceCard)),
      ),
    ),
  );
}

/// The latest requests, the newest sliding in.
function activity(seen) {
  const recent = state.requests.slice(-RECENT).reverse();
  return h(
    "section",
    { class: "activity panel" },
    h("header", {}, h("h2", {}, "Live activity"), h("a", { href: "#/requests" }, "All requests →")),
    recent.length
      ? h(
          "ul",
          {},
          recent.map((r) =>
            h(
              "li",
              { class: r.id > seen ? "new" : null, onclick: () => (location.hash = "#/requests") },
              r.service ? serviceTile(r.service, 26) : h("span", { class: "tile", style: "--c:#8a8580;width:26px;height:26px" }),
              h("span", { class: "op" }, operation(r), r.resource ? h("span", { class: "dim" }, ` · ${r.resource}`) : null),
              statusPill(r),
              h("span", { class: "dim mono" }, time(r.time)),
            ),
          ),
        )
      : h(
          "div",
          { class: "empty slim" },
          h("p", {}, "Waiting for the first request. Try:"),
          h("code", {}, `aws --endpoint-url ${location.origin} s3 mb s3://my-bucket`),
        ),
  );
}

PAGES.home = (main) => {
  let seen = state.last;
  const update = () => {
    main.replaceChildren(hero(), stats(), h("div", { class: "home-grid" }, servicesSection(), activity(seen)));
    seen = state.last;
  };
  update();
  return update;
};
