// A service's page: what it holds of the kind chosen in the side
// navigation, in a table with a filter. A row opens the record, or the
// service's own page for it (a bucket, a queue).

import { look, serviceTile } from "./catalog";
import { expanded, h } from "./dom";
import { BROWSERS, container, crumbs, empty, filterBox, filtered, heading, iconButton, openDrawer, table } from "./frame";
import { service, state } from "./state";
import type { Page } from "./types";

export const servicePage: Page = (main, name) => {
  const s = service(name);
  if (!s) {
    main.replaceChildren(heading(name, state.inventory ? "No such service here." : "Loading…"));
    // Drawn again once the inventory names it.
    return null;
  }
  const browser = BROWSERS[name];
  const title = h("span", { class: "with-tile" }, serviceTile(name, 40), s.title);
  if (s.error) {
    main.replaceChildren(heading(title, h("span", { class: "bad" }, s.error)));
    return null;
  }
  if (!s.listings.length) {
    main.replaceChildren(
      crumbs([s.title, null]),
      heading(title, look(name).about),
      container(
        "Nothing to list",
        {},
        empty(
          `${s.title} keeps nothing`,
          "Each request is answered on its own. Tools call it behind the scenes; try it yourself:",
          name === "sts" ? "aws sts get-caller-identity" : `aws ${name} help`,
        ),
      ),
    );
    return null;
  }
  const listing = () => {
    const current = service(name) ?? s;
    return current.listings[Math.min(state.tab[name] ?? 0, current.listings.length - 1)];
  };
  const shown = table(
    listing().columns,
    () =>
      filtered(listing().items).map((item) => ({
        key: item.cells[0],
        cells: item.cells,
        mono: [listing().columns.indexOf("ARN")],
        open: () =>
          browser?.open
            ? browser.open(item)
            : openDrawer(item.cells[0], h("h3", null, "Record"), h("pre", null, JSON.stringify(expanded(item.detail), null, 2))),
      })),
    () =>
      state.filter
        ? empty("Nothing matches the filter")
        : empty(`No ${listing().title.toLowerCase()} yet`, "Create one with the AWS CLI, an SDK or Terraform, and it shows up here."),
  );
  const create = browser?.create
    ? iconButton("plus", `Create ${browser.noun ?? "one"}`, () => browser.create?.(), "primary")
    : null;
  main.replaceChildren(
    crumbs([s.title, `#/s/${name}`], [listing().title, null]),
    heading(title, look(name).about),
    container(
      listing().title,
      { count: listing().items.length, actions: [filterBox(() => shown.update()), create] },
      shown.el,
    ),
  );
  return () => {
    if (!service(name)) return;
    const header = main.querySelector(".container h2 .count");
    if (header) header.textContent = ` (${listing().items.length})`;
    shown.update();
  };
};
