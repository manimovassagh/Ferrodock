// A service's page: a tab per kind of thing it holds, a filter, and its
// records. A service with a browser of its own (S3) opens rows there.

"use strict";

/// The heading of a service's pages: its tile, title and one line.
function serviceHeading(s) {
  return h(
    "header",
    { class: "heading" },
    serviceTile(s.name, 48),
    h("div", {}, h("h1", {}, s.title), h("p", { class: "lead" }, look(s.name).about || s.name)),
  );
}

PAGES.service = (main, name) => {
  const service = () => (state.inventory ? state.inventory.services.find((s) => s.name === name) : null);
  const s = service();
  if (!s) {
    main.replaceChildren(h("h1", {}, name), h("p", { class: "lead" }, state.inventory ? "No such service here." : "Loading…"));
    return () => state.inventory && service() && route();
  }
  main.replaceChildren(serviceHeading(s));
  if (s.error) {
    main.append(h("p", { class: "bad" }, s.error));
    return null;
  }
  if (!s.listings.length) {
    main.append(
      h(
        "div",
        { class: "empty" },
        h("strong", {}, `${s.title} keeps nothing to list`),
        h("p", {}, "Each request is answered on its own. Tools call it behind the scenes; try it yourself:"),
        h("code", {}, s.name === "sts" ? "aws sts get-caller-identity" : `aws ${s.name} help`),
      ),
    );
    return null;
  }
  const tabs = h("div", { class: "tabs", role: "tablist" });
  const holder = h("div");
  let shown = null;
  const listing = () => service().listings[Math.min(state.tab[name] || 0, service().listings.length - 1)];
  const browser = BROWSERS[name];
  const rows = () =>
    listing()
      .items.filter((item) => matches(item.cells))
      .map((item) => ({
        key: item.cells[0],
        cells: item.cells,
        mono: [1],
        open: () =>
          browser && browser.open
            ? browser.open(item)
            : openDrawer(item.cells[0], h("h3", {}, "Record"), h("pre", {}, JSON.stringify(expanded(item.detail), null, 2))),
      }));
  const showListing = () => {
    const l = listing();
    shown = table(l.columns, rows, () =>
      h(
        "div",
        { class: "empty" },
        h("strong", {}, state.filter ? "Nothing matches the filter" : `No ${l.title.toLowerCase()} yet`),
        state.filter ? null : h("p", {}, "Create one with the AWS CLI, an SDK or Terraform, and it shows up here."),
      ),
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
  const actions = browser && browser.actions ? browser.actions() : [];
  main.append(
    h("div", { class: "toolbar" }, s.listings.length > 1 ? tabs : null, h("span", { class: "grow" }), filterBox(() => shown.update()), ...actions),
    holder,
  );
  return () => {
    if (!service()) return;
    drawTabs();
    shown.update();
  };
};
