// Built from crates/cloud/console/*.ts by 'npm run build': don't edit by hand.
"use strict";
(() => {
  // dom.ts
  function h(tag, attributes, ...children) {
    const el = document.createElement(tag);
    for (const [key, value] of Object.entries(attributes ?? {})) {
      if (value == null || value === false) continue;
      if (key.startsWith("on") && typeof value === "function") el.addEventListener(key.slice(2), value);
      else if (key === "class") el.className = String(value);
      else el.setAttribute(key, value === true ? "" : String(value));
    }
    append(el, children);
    return el;
  }
  function append(el, children) {
    for (const child of children) {
      if (child == null || child === false) continue;
      if (Array.isArray(child)) append(el, child);
      else el.append(child instanceof Node ? child : document.createTextNode(String(child)));
    }
  }
  function fill(el, ...children) {
    el.replaceChildren();
    append(el, children);
  }
  function $(id) {
    const el = document.getElementById(id);
    if (!el) throw new Error(`no #${id} on the page`);
    return el;
  }
  function plural(n, word) {
    return `${n.toLocaleString()} ${word}${n === 1 ? "" : "s"}`;
  }
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
  function duration(ms) {
    const s = Math.floor(ms / 1e3);
    if (s < 60) return `${s} s`;
    const m = Math.floor(s / 60);
    if (m < 60) return `${m} min`;
    const d = Math.floor(m / 1440);
    if (d > 0) return `${d} d ${Math.floor(m % 1440 / 60)} h`;
    return `${Math.floor(m / 60)} h ${m % 60} min`;
  }
  async function copy(text, button2) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const area = h("textarea", { style: "position:fixed;opacity:0" }, text);
      document.body.append(area);
      area.select();
      document.execCommand("copy");
      area.remove();
    }
    button2.classList.add("done");
    setTimeout(() => button2.classList.remove("done"), 1400);
  }
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
  function prettyXml(text) {
    const tokens = text.replace(/>\s+</g, "><").match(/<[^>]+>|[^<]+/g) ?? [];
    const lines = [];
    let depth = 0;
    let open = false;
    for (const token of tokens) {
      const tag = token.startsWith("<");
      const closing = token.startsWith("</");
      const single = token.startsWith("<?") || token.startsWith("<!") || token.endsWith("/>");
      if (!tag || closing && open) {
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
      }
    }
    return body.truncated ? `${text}
… (${body.size} bytes in all)` : text;
  }
  function sender(agent) {
    const known = [
      [/aws-cli\/(\d+\.\d+)/, "AWS CLI"],
      [/Boto3\/(\d+\.\d+)/i, "boto3"],
      [/aws-sdk-js\/(\d+\.\d+)/, "JavaScript SDK"],
      [/aws-sdk-java\/(\d+\.\d+)/, "Java SDK"],
      [/aws-sdk-go-v2\/(\d+\.\d+)/, "Go SDK"],
      [/aws-sdk-rust\/(\d+\.\d+)/, "Rust SDK"],
      [/Terraform\/(\d+\.\d+)/, "Terraform"],
      [/Mozilla/, "Browser"]
    ];
    for (const [pattern, name] of known) {
      const found = pattern.exec(agent);
      if (found) return found[1] ? `${name} ${found[1]}` : name;
    }
    return agent.split(" ")[0] ?? "";
  }

  // catalog.ts
  var CATEGORIES = [
    { name: "Storage", tile: "linear-gradient(135deg, #2e8b57, #7ac143)" },
    { name: "Application integration", tile: "linear-gradient(135deg, #b0084d, #ff4f8b)" },
    { name: "Databases", tile: "linear-gradient(135deg, #2e27ad, #527fff)" },
    { name: "Compute", tile: "linear-gradient(135deg, #c8511b, #ff9900)" },
    { name: "Other services", tile: "linear-gradient(135deg, #545b64, #8d99a8)" },
    { name: "Security and identity", tile: "linear-gradient(135deg, #bd0816, #ff5252)" }
  ];
  var CATALOG = {
    s3: { category: "Storage", icon: "bucket", about: "Buckets and objects" },
    sqs: { category: "Application integration", icon: "queue", about: "Message queues" },
    sns: { title: "SNS", category: "Application integration", icon: "bell", about: "Topics and subscriptions", planned: "coming next" },
    dynamodb: { title: "DynamoDB", category: "Databases", icon: "table", about: "Tables of items", planned: "planned" },
    lambda: { title: "Lambda", category: "Compute", icon: "bolt", about: "Functions", planned: "planned" },
    iam: { category: "Security and identity", icon: "key", about: "Roles, users and policies" },
    sts: { category: "Security and identity", icon: "badge", about: "Temporary credentials, used by tools" }
  };
  var PINNED = ["s3", "sqs", "iam"];
  function look(name) {
    return CATALOG[name] || { category: "Other services", icon: "cube", about: "" };
  }
  function category(name) {
    return CATEGORIES.find((c) => c.name === look(name).category) ?? CATEGORIES[4];
  }
  var ICONS = {
    bucket: "M4 7c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zm0 0 1.6 11.2c.2 1.6 3 2.8 6.4 2.8s6.2-1.2 6.4-2.8L20 7",
    queue: "M3 7h12M3 12h12M3 17h8m6-10 4 5-4 5",
    bell: "M6 16V11a6 6 0 1 1 12 0v5l1.5 2h-15zM10 20a2 2 0 0 0 4 0",
    table: "M4 5h16v14H4zM4 10h16M4 15h16M10 5v14",
    bolt: "M13 3 5 14h6l-1 7 8-11h-6z",
    key: "M8 16a4 4 0 1 1 3.9-5H21v3h-2v2h-3v-2h-4.1A4 4 0 0 1 8 16zm0-4h.01",
    badge: "M12 3 5 6v5c0 4.5 3 8.3 7 10 4-1.7 7-5.5 7-10V6zm-3 9 2 2 4-4",
    cube: "M12 3 4 7.5v9L12 21l8-4.5v-9zM4 7.5 12 12l8-4.5M12 12v9",
    messages: "M5 4h14v16H5zM8 8h8M8 12h8M8 16h5",
    activity: "M3 12h4l3-7 4 14 3-7h4",
    folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
    file: "M7 3h7l5 5v13H7zM14 3v5h5",
    upload: "M12 16V4m0 0-5 5m5-5 5 5M5 20h14",
    download: "M12 4v12m0 0-5-5m5 5 5-5M5 20h14",
    copy: "M9 9h10v10H9zM5 15V5h10",
    grid: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
    search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zm5-2 4 4",
    cloud: "M7 18h10.5a4.5 4.5 0 0 0 .6-8.96A6 6 0 0 0 6.6 9.5 4.25 4.25 0 0 0 7 18Z",
    plus: "M12 5v14M5 12h14",
    refresh: "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7",
    lock: "M6 11h12v9H6zM8 11V8a4 4 0 0 1 8 0v3",
    send: "M4 12 20 4l-6 16-3-7z",
    trash: "M5 7h14M10 7V4h4v3m-7 0 1 13h8l1-13"
  };
  function icon(name, size = 18) {
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("width", String(size));
    svg.setAttribute("height", String(size));
    svg.setAttribute("aria-hidden", "true");
    svg.setAttribute("class", "icon");
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", ICONS[name] || ICONS.cube);
    svg.append(path);
    return svg;
  }
  function tileOf(glyph, gradient, size) {
    return h(
      "span",
      { class: "tile", style: `background:${gradient};width:${size}px;height:${size}px` },
      icon(glyph, Math.round(size * 0.56))
    );
  }
  function serviceTile(name, size = 40) {
    return tileOf(look(name).icon, category(name).tile, size);
  }
  function ordered(list) {
    const rank = (name) => CATEGORIES.indexOf(category(name));
    return [...list].sort((a, b) => rank(a.name) - rank(b.name));
  }
  function catalogue(services2) {
    const have = new Set(services2.map((s) => s.name));
    const planned = Object.entries(CATALOG).filter(([name, entry]) => entry.planned && !have.has(name)).map(([name, entry]) => ({ name, title: entry.title ?? name, planned: entry.planned }));
    const all = [...services2.map((s) => ({ name: s.name, title: s.title, service: s })), ...planned];
    return CATEGORIES.map((c) => ({ category: c.name, services: all.filter((s) => look(s.name).category === c.name) })).filter(
      (g) => g.services.length
    );
  }

  // state.ts
  var API = "/_ferrocloud/";
  var state = {
    health: null,
    down: false,
    inventory: null,
    requests: [],
    /** The ID of the newest request seen. */
    last: 0,
    /** The text in the page's filter box. */
    filter: "",
    /** Per service, the listing shown. */
    tab: {}
  };
  async function get(path) {
    const response = await fetch(API + path, { cache: "no-store" });
    if (!response.ok) throw new Error(`${path}: ${response.status}`);
    return await response.json();
  }
  function services() {
    return state.inventory?.services ?? [];
  }
  function service(name) {
    return services().find((s) => s.name === name);
  }
  function matches(fields) {
    const needle = state.filter.trim().toLowerCase();
    return !needle || fields.some((f) => String(f).toLowerCase().includes(needle));
  }

  // frame.ts
  var BROWSERS = {};
  function renderStatus() {
    const health2 = state.health;
    if (state.down) {
      fill($("status"), h("span", { class: "pill down" }, h("i"), "Not answering"));
      return;
    }
    if (!health2) return;
    fill(
      $("status"),
      h("span", { class: "pill" }, h("i"), "Running"),
      h("span", { class: "pill" }, health2.region, h("small", null, "region")),
      h("span", { class: "pill" }, `Account ${health2.account.replace(/(\d{4})(?=\d)/g, "$1-")}`)
    );
  }
  function renderPins() {
    const have = new Set(services().map((s) => s.name));
    fill(
      $("pins"),
      PINNED.filter((name) => have.has(name)).map(
        (name) => h("a", { href: `#/s/${name}` }, serviceTile(name, 18), services().find((s) => s.name === name)?.title ?? name)
      ),
      h("span", { class: "sep" }),
      h("a", { href: "#/requests", class: "muted" }, icon("activity", 15), "Requests")
    );
  }
  function servicesMenu() {
    return h(
      "div",
      { class: "menu-panel" },
      catalogue(services()).map(
        ({ category: category2, services: list }) => h(
          "section",
          null,
          h("h3", null, category2),
          list.map(
            (s) => h(
              "a",
              { href: s.planned ? null : `#/s/${s.name}`, class: s.planned ? "soon" : null },
              serviceTile(s.name, 28),
              h("span", null, h("b", null, s.title), h("small", null, s.planned ? `${look(s.name).about} · ${s.planned}` : look(s.name).about))
            )
          )
        )
      )
    );
  }
  function toggleMenu(force) {
    const holder = $("menu");
    const open = force ?? holder.hidden;
    holder.hidden = !open;
    if (open) fill(holder, servicesMenu());
  }
  function searchResults(text) {
    const needle = text.trim().toLowerCase();
    if (!needle) return [];
    const found = [];
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
  function result(href, tile, title, note) {
    return h(
      "a",
      { href, onclick: () => closeSearch() },
      tile,
      h("span", null, h("b", null, title), h("small", null, note))
    );
  }
  function closeSearch() {
    $("results").hidden = true;
    $("search").value = "";
  }
  function setUpTopBar() {
    $("services").addEventListener("click", (e) => {
      e.stopPropagation();
      toggleMenu();
    });
    const search = $("search");
    const results = $("results");
    search.addEventListener("input", () => {
      const found = searchResults(search.value);
      results.hidden = !search.value.trim();
      fill(results, found.length ? found : h("p", { class: "muted" }, "Nothing here by that name."));
    });
    search.addEventListener("keydown", (e) => {
      if (e.key === "Enter") results.querySelector("a")?.click();
    });
    addEventListener("click", (e) => {
      if (!e.target.closest("#menu")) toggleMenu(false);
      if (!e.target.closest(".search")) $("results").hidden = true;
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
  function renderSide(route2) {
    const side = $("side");
    const s = route2.page === "service" ? services().find((x) => x.name === route2.name) : void 0;
    document.body.classList.toggle("with-side", Boolean(s));
    if (!s) {
      side.replaceChildren();
      return;
    }
    const chosen = state.tab[s.name] ?? 0;
    fill(
      side,
      h("a", { class: "side-title", href: `#/s/${s.name}` }, serviceTile(s.name, 28), h("span", null, s.title)),
      s.listings.map(
        (l, i) => h(
          "a",
          {
            href: `#/s/${s.name}`,
            class: i === chosen && !route2.path ? "on" : null,
            onclick: () => {
              state.tab[s.name] = i;
            }
          },
          l.title,
          h("span", { class: "count" }, l.items.length)
        )
      ),
      h("hr"),
      h("a", { href: "#/requests", onclick: () => state.filter = s.name }, "Requests to ", s.title)
    );
  }
  function openDrawer(title, ...content) {
    const drawer = $("drawer");
    fill(
      drawer,
      h("header", null, h("h2", null, title), h("button", { class: "close", title: "Close", onclick: closeDrawer }, "×")),
      ...content
    );
    drawer.hidden = false;
  }
  function closeDrawer() {
    $("drawer").hidden = true;
    document.querySelectorAll("tbody tr.on").forEach((row) => row.classList.remove("on"));
  }
  function crumbs(...parts) {
    const links = [h("a", { href: "#/" }, "Console Home")];
    for (const [label, href] of parts) {
      links.push(h("span", { class: "sep" }, "›"), href ? h("a", { href }, label) : h("span", null, label));
    }
    return h("nav", { class: "crumbs" }, links);
  }
  function heading(title, about, ...actions) {
    return h(
      "header",
      { class: "page-head" },
      h("div", null, h("h1", null, title), about ? h("p", null, about) : null),
      h("div", { class: "actions" }, actions)
    );
  }
  function container(title, options, ...body) {
    return h(
      "section",
      { class: "container" },
      h(
        "header",
        null,
        h("h2", null, title, options.count != null ? h("span", { class: "count" }, ` (${options.count})`) : null),
        options.info ? h("span", { class: "info" }, options.info) : null,
        h("div", { class: "actions" }, options.actions ?? [])
      ),
      body
    );
  }
  function button(label, onclick, kind = "") {
    return h("button", { class: `button ${kind}`, onclick }, label);
  }
  function iconButton(glyph, label, onclick, kind = "") {
    return button([icon(glyph, 16), label], onclick, kind);
  }
  function notice(text, kind = "ok") {
    const note = h(
      "div",
      { class: `notice ${kind}` },
      h("span", null, text),
      h("button", { class: "close", onclick: () => note.remove() }, "×")
    );
    $("notices").append(note);
    setTimeout(() => note.remove(), 6e3);
  }
  function filterBox(onChange, placeholder = "Find by name") {
    return h("input", {
      class: "filter",
      type: "search",
      placeholder,
      value: state.filter,
      oninput: (e) => {
        state.filter = e.target.value;
        onChange();
      }
    });
  }
  function table(columns, rows, empty2) {
    const body = h("tbody");
    const wrap = h("div", { class: "table" });
    let opened = null;
    const update2 = () => {
      const shown = rows();
      if (!shown.length) {
        fill(wrap, empty2());
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
              }
            },
            row.cells.map(
              (cell, i) => h("td", { class: row.mono?.includes(i) ? "mono" : null, title: typeof cell === "string" ? cell : null }, cell)
            )
          );
          return tr;
        })
      );
      if (!wrap.contains(body)) {
        fill(wrap, h("table", null, h("thead", null, h("tr", null, columns.map((c) => h("th", null, c)))), body));
      }
    };
    update2();
    return { el: wrap, update: update2 };
  }
  function empty(title, line2, command) {
    return h("div", { class: "empty" }, h("strong", null, title), line2 ? h("p", null, line2) : null, command ? h("code", null, command) : null);
  }
  function filtered(items) {
    return items.filter((item) => matches(item.cells));
  }

  // form.ts
  function field(label, control, options = {}) {
    const error = h("div", { class: "field-error", hidden: true });
    const el = h(
      "div",
      { class: "field" },
      h("label", null, label, options.optional ? h("span", { class: "optional" }, " - optional") : null),
      options.hint ? h("div", { class: "field-hint" }, options.hint) : null,
      control,
      error,
      options.constraint ? h("div", { class: "field-constraint" }, options.constraint) : null
    );
    return {
      el,
      error(message) {
        error.hidden = !message;
        error.textContent = message ? `⚠ ${message}` : "";
        control.classList.toggle("invalid", Boolean(message));
      }
    };
  }
  function textInput(options = {}) {
    return h("input", {
      class: `input${options.mono ? " mono" : ""}`,
      type: "text",
      value: options.value ?? "",
      placeholder: options.placeholder ?? "",
      autocomplete: "off",
      spellcheck: "false"
    });
  }
  function radioTiles(name, choices, chosen) {
    let current2 = chosen;
    const listeners = [];
    const tiles = choices.map((choice) => {
      const input = h("input", { type: "radio", name, value: choice.value, checked: choice.value === chosen });
      const tile = h("label", { class: `tile-choice${choice.value === chosen ? " on" : ""}` }, input, h("span", null, h("b", null, choice.title), h("small", null, choice.about)));
      input.addEventListener("change", () => {
        current2 = choice.value;
        tiles.forEach((t) => t.classList.toggle("on", t === tile));
        listeners.forEach((listen) => listen(current2));
      });
      return tile;
    });
    return {
      el: h("div", { class: "tiles" }, tiles),
      value: () => current2,
      onChange: (listener) => listeners.push(listener)
    };
  }

  // dialog.ts
  function dialog(options) {
    const problem = h("div", { class: "alert bad", hidden: true });
    const main = h("button", { class: `button ${options.danger ? "danger" : "primary"}` }, options.action);
    const backdrop = h(
      "div",
      { class: "backdrop", role: "presentation" },
      h(
        "div",
        { class: "dialog", role: "dialog", "aria-modal": "true", "aria-label": options.title },
        h("header", null, h("h2", null, options.title), h("button", { class: "close", title: "Close", onclick: () => close() }, "×")),
        h("div", { class: "dialog-body" }, problem, options.body),
        h("footer", null, h("button", { class: "button link", onclick: () => close() }, "Cancel"), main)
      )
    );
    const before = document.activeElement;
    const close = () => {
      backdrop.remove();
      removeEventListener("keydown", keys, true);
      before?.focus();
    };
    const focusable = () => [...backdrop.querySelectorAll("input, button:not([disabled]), select, textarea")];
    const keys = (e) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        close();
      }
      if (e.key === "Enter" && !(e.target instanceof HTMLTextAreaElement) && !main.disabled) {
        e.preventDefault();
        main.click();
      }
      if (e.key === "Tab") {
        const all = focusable();
        const at = all.indexOf(document.activeElement);
        const next = e.shiftKey ? at <= 0 ? all.length - 1 : at - 1 : (at + 1) % all.length;
        e.preventDefault();
        all[next]?.focus();
      }
    };
    main.addEventListener("click", async () => {
      main.disabled = true;
      problem.hidden = true;
      try {
        await options.confirm();
        close();
      } catch (e) {
        problem.textContent = e.message;
        problem.hidden = false;
        main.disabled = false;
      }
    });
    backdrop.addEventListener("mousedown", (e) => {
      if (e.target === backdrop) close();
    });
    addEventListener("keydown", keys, true);
    document.body.append(backdrop);
    (focusable().find((el) => el.tagName === "INPUT") ?? main).focus();
    return {
      enable(on) {
        main.disabled = !on;
      }
    };
  }
  function confirmDanger(options) {
    const body = [h("div", { class: "alert warn" }, h("span", null, options.warning))];
    let typed = null;
    if (options.typeName) {
      typed = textInput({ placeholder: options.typeName, mono: true });
      body.push(field(`To confirm, type "${options.typeName}" in the field.`, typed).el);
    }
    const open = dialog({ title: options.title, body, action: options.action, danger: true, confirm: options.run });
    if (typed) {
      const input = typed;
      open.enable(false);
      input.addEventListener("input", () => open.enable(input.value === options.typeName));
    }
  }

  // requests.ts
  function statusPill(r) {
    return h("span", { class: `status ${r.status < 400 ? "ok" : "bad"}` }, r.error ? `${r.status} ${r.error}` : String(r.status));
  }
  function operation(r) {
    return r.operation || `${r.method} ${r.path}`;
  }
  function serviceTitle(name) {
    return services().find((s) => s.name === name)?.title ?? name;
  }
  var requestsPage = (main) => {
    const shown = table(
      ["Time", "Service", "Operation", "Resource", "Status", "Sent by", "Took"],
      () => state.requests.filter((r) => matches([r.service, r.operation, r.resource, r.status, r.error])).slice().reverse().map((r) => ({
        key: r.id,
        cells: [
          time(r.time),
          h("span", { class: "svc" }, r.service ? serviceTile(r.service, 22) : null, r.service ? serviceTitle(r.service) : "—"),
          operation(r),
          r.resource,
          statusPill(r),
          sender(r.user_agent),
          `${r.duration_ms} ms`
        ],
        mono: [0, 6],
        open: () => void openRequest(r)
      })),
      () => state.filter ? empty("Nothing matches the filter") : empty("No requests yet", "Point a tool at this emulator and its requests appear here as they're answered:", `AWS_ENDPOINT_URL=${location.origin} aws s3 ls`)
    );
    const box = container(
      "Requests",
      { count: state.requests.length, info: "Live", actions: [filterBox(() => shown.update(), "Filter by service, operation, resource, status")] },
      shown.el
    );
    main.replaceChildren(
      crumbs(["Requests", null]),
      heading("Requests", "Every request the emulator answered, newest first. Secrets in them are masked."),
      box
    );
    return () => {
      const count = box.querySelector("h2 .count");
      if (count) count.textContent = ` (${state.requests.length})`;
      shown.update();
    };
  };
  async function openRequest(r) {
    const title = operation(r);
    openDrawer(title, h("p", { class: "muted" }, "Loading…"));
    let detail;
    try {
      detail = (await get(`requests/${r.id}`)).detail;
    } catch {
      openDrawer(title, h("p", { class: "muted" }, "That request is no longer remembered."));
      return;
    }
    const pairs = (list) => h("dl", null, list.map(([k, v]) => [h("dt", null, k), h("dd", { class: "mono" }, v)]));
    const facts = [
      ["Message", r.message],
      ["Service", r.service],
      ["Resource", r.resource],
      ["Region", r.region],
      ["Sent by", r.user_agent],
      ["Request ID", r.request_id],
      ["Took", `${r.duration_ms} ms`]
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
      pairs(detail.response_headers)
    );
  }

  // home.ts
  var MINUTES = 30;
  var RECENT = 6;
  function ring(share) {
    const ns = "http://www.w3.org/2000/svg";
    const svg = document.createElementNS(ns, "svg");
    svg.setAttribute("viewBox", "0 0 100 100");
    svg.setAttribute("class", "ring");
    const circle = (stroke, dash) => {
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
    const text = (y, size, value, cls) => {
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
      text(68, 9, share == null ? "no requests" : "succeeded", "small")
    );
    return svg;
  }
  function health(all) {
    const h0 = state.health;
    const answered = state.requests.length;
    const ok = state.requests.filter((r) => r.status < 400).length;
    const copyButton = button([icon("copy", 14), "Copy"], (e) => copy(location.origin, e.currentTarget), "link");
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
          h("dd", null, h0?.region ?? "…")
        )
      ),
      h("div", { class: "chips" }, ordered(all).map((s) => h("span", { class: "chip ok" }, h("i"), s.title)))
    );
  }
  function figures(all) {
    const out = [];
    for (const s of ordered(all)) {
      s.listings.forEach((listing, i) => {
        if (i > 0 && !listing.items.length) return;
        out.push({
          service: s.name,
          glyph: i === 0 ? look(s.name).icon : "cube",
          value: listing.items.length,
          label: `${s.title} ${listing.title.toLowerCase()}`,
          href: `#/s/${s.name}`
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
  function bytesShort(n) {
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
  function resources(all) {
    const list = figures(all);
    return container(
      "Your resources",
      { info: "Everything the emulator holds" },
      list.length ? h(
        "div",
        { class: "figures" },
        list.map(
          (f) => h(
            "a",
            { href: f.href },
            tileOf(f.glyph, category(f.service).tile, 40),
            h("span", null, h("strong", null, f.value.toLocaleString()), h("span", { class: "link" }, f.label))
          )
        )
      ) : h("div", { class: "body muted" }, "Nothing yet. Create a bucket or a queue and it shows here.")
    );
  }
  function perMinute(requests) {
    const now = Date.now();
    const all = new Array(MINUTES).fill(0);
    const bad = new Array(MINUTES).fill(0);
    for (const r of requests) {
      const ago = Math.floor((now - r.time) / 6e4);
      if (ago < 0 || ago >= MINUTES) continue;
      all[MINUTES - 1 - ago] += 1;
      if (r.status >= 400) bad[MINUTES - 1 - ago] += 1;
    }
    return { all, bad };
  }
  function line(values, top, close) {
    const step = 640 / (values.length - 1);
    const y = (v) => 150 - v / top * 130;
    const points = values.map((v, i) => `${(i * step).toFixed(1)} ${y(v).toFixed(1)}`);
    const path = `M${points.join(" L")}`;
    return close ? `${path} L640 160 L0 160 Z` : path;
  }
  function traffic() {
    const window = state.requests.filter((r) => r.time >= Date.now() - MINUTES * 6e4);
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
    svg.innerHTML = '<defs><linearGradient id="fill" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ec7211" stop-opacity=".35"/><stop offset="1" stop-color="#ec7211" stop-opacity="0"/></linearGradient></defs><g class="grid"><line x1="0" y1="20" x2="640" y2="20"/><line x1="0" y1="85" x2="640" y2="85"/><line x1="0" y1="150" x2="640" y2="150"/></g>';
    const path = (d, cls) => {
      const p = document.createElementNS(ns, "path");
      p.setAttribute("d", d);
      p.setAttribute("class", cls);
      return p;
    };
    svg.append(path(line(all, top, true), "area"), path(line(all, top, false), "requests"), path(line(bad, top, false), "errors"));
    const kpi = (label, value, cls = "") => h("div", { class: cls }, h("span", null, label), h("strong", null, value));
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
          kpi("Median time", `${median} ms`)
        ),
        h("div", { class: "legend" }, h("span", null, h("i", { class: "requests" }), "Requests"), h("span", null, h("i", { class: "errors" }), "Errors")),
        svg
      )
    );
  }
  function start() {
    const endpoint = location.origin;
    const ways = [
      ["s3", "Create a bucket", "Store files", "aws s3 mb s3://photos"],
      ["sqs", "Create a queue", "Send work between services", "aws sqs create-queue --queue-name jobs"],
      ["iam", "Create a role", "For your app or Terraform", "aws iam create-role --role-name app …"]
    ];
    const row = (tile, title, about, command) => h(
      "button",
      { class: "way", title: "Copy the command", onclick: (e) => copy(command, e.currentTarget) },
      tile,
      h("span", null, h("b", null, title), h("small", null, about)),
      h("code", null, command)
    );
    return container(
      "Build something",
      { info: "Click to copy" },
      h(
        "div",
        { class: "body ways" },
        ways.map(([service2, title, about, command]) => row(serviceTile(service2, 34), title, about, command)),
        row(tileOf("lock", "linear-gradient(135deg, #c8511b, #ff9900)", 34), "Connect your tools", "CLI, SDKs, Terraform", `export AWS_ENDPOINT_URL=${endpoint}`)
      )
    );
  }
  function recent(seen) {
    const latest = state.requests.slice(-RECENT).reverse();
    const rows = latest.map(
      (r) => h(
        "tr",
        { class: r.id > seen ? "new" : null, onclick: () => location.hash = "#/requests" },
        h("td", { class: "mono" }, time(r.time)),
        h("td", null, h("span", { class: "svc" }, r.service ? serviceTile(r.service, 22) : null, r.service ? services().find((s) => s.name === r.service)?.title ?? r.service : "—")),
        h("td", null, operation(r)),
        h("td", { class: "link" }, r.resource),
        h("td", null, statusPill(r)),
        h("td", null, sender(r.user_agent)),
        h("td", { class: "mono" }, `${r.duration_ms} ms`)
      )
    );
    return container(
      "Recent activity",
      { info: "Live", actions: [h("a", { class: "link", href: "#/requests" }, "View all requests →")] },
      latest.length ? h(
        "div",
        { class: "table" },
        h(
          "table",
          null,
          h("thead", null, h("tr", null, ["Time", "Service", "Operation", "Resource", "Status", "Sent by", "Took"].map((c) => h("th", null, c)))),
          h("tbody", null, rows)
        )
      ) : h("div", { class: "body muted" }, "Waiting for the first request. Point a tool here: ", h("code", null, `export AWS_ENDPOINT_URL=${location.origin}`))
    );
  }
  function allServices(all) {
    return container(
      "All services",
      { info: "What this emulator speaks" },
      h(
        "div",
        { class: "catalogue" },
        catalogue(all).map(
          ({ category: category2, services: list }) => h(
            "section",
            null,
            h("h3", null, category2),
            list.map((s) => {
              const held = s.service?.listings.reduce((n, l) => n + l.items.length, 0) ?? 0;
              return h(
                "a",
                { href: s.planned ? null : `#/s/${s.name}`, class: s.planned ? "soon" : null },
                serviceTile(s.name, 36),
                h("span", null, h("b", null, s.title), h("small", null, s.planned ? `${look(s.name).about} · ${s.planned}` : look(s.name).about)),
                held ? h("span", { class: "badge" }, held) : null
              );
            })
          )
        )
      )
    );
  }
  function createMenu() {
    const makers = Object.entries(BROWSERS).filter(([, b]) => b.create);
    if (!makers.length) return null;
    const menu = h(
      "div",
      { class: "dropdown", hidden: true },
      makers.map(
        ([name, b]) => h("button", { onclick: () => b.create?.() }, serviceTile(name, 22), `Create ${b.noun ?? "one"}`)
      )
    );
    const toggle = iconButton("plus", "Create", (e) => {
      e.stopPropagation();
      menu.hidden = !menu.hidden;
    }, "primary");
    addEventListener("click", () => menu.hidden = true, { once: true });
    return h("span", { class: "dropdown-holder" }, toggle, menu);
  }
  function reset() {
    confirmDanger({
      title: "Empty the emulator?",
      warning: "Every bucket, object, queue, message, role and request it holds is deleted. This can't be undone.",
      typeName: "empty",
      action: "Empty the emulator",
      run: async () => {
        const response = await fetch(`${API}reset`, { method: "POST" });
        if (!response.ok) throw new Error("The emulator couldn't be emptied.");
        notice("The emulator is empty.");
      }
    });
  }
  var home = (main) => {
    let seen = state.last;
    const update2 = () => {
      const all = services();
      main.replaceChildren(
        heading(
          "Console Home",
          "Your local AWS, on this computer. Nothing here leaves it, and nothing is billed.",
          iconButton("refresh", "Reset data", reset),
          createMenu()
        ),
        h(
          "div",
          { class: "grid" },
          h("div", { class: "c4" }, health(all)),
          h("div", { class: "c8" }, resources(all)),
          h("div", { class: "c7" }, traffic()),
          h("div", { class: "c5" }, start()),
          h("div", { class: "c12" }, recent(seen)),
          h("div", { class: "c12" }, allServices(all))
        )
      );
      seen = state.last;
    };
    update2();
    return update2;
  };

  // s3.ts
  var PAGE = 1e3;
  function keyPath(key) {
    return key.split("/").map(encodeURIComponent).join("/");
  }
  function objectUrl(bucket, key) {
    return `/${encodeURIComponent(bucket)}/${keyPath(key)}`;
  }
  function folderHref(bucket, prefix) {
    return `#/s/s3/${encodeURIComponent(bucket)}/${keyPath(prefix)}`;
  }
  async function s3(method, path, body, headers) {
    const response = await fetch(path, { method, body, headers, cache: "no-store" });
    if (!response.ok) {
      const text = await response.text();
      const message = /<Message>([^<]*)<\/Message>/.exec(text);
      throw new Error(message ? message[1] : `${method} ${path}: ${response.status}`);
    }
    return response;
  }
  async function listFolder(bucket, prefix, token) {
    const query = new URLSearchParams({ "list-type": "2", delimiter: "/", prefix, "max-keys": String(PAGE) });
    if (token) query.set("continuation-token", token);
    const response = await s3("GET", `/${encodeURIComponent(bucket)}?${query}`);
    const doc = new DOMParser().parseFromString(await response.text(), "application/xml");
    const text = (el, tag) => el.getElementsByTagName(tag)[0]?.textContent ?? "";
    return {
      folders: [...doc.getElementsByTagName("CommonPrefixes")].map((el) => text(el, "Prefix")),
      objects: [...doc.getElementsByTagName("Contents")].map(
        (el) => ({
          key: text(el, "Key"),
          size: Number(text(el, "Size")),
          modified: text(el, "LastModified"),
          etag: text(el, "ETag")
        })
      ),
      next: text(doc, "IsTruncated") === "true" ? text(doc, "NextContinuationToken") : null
    };
  }
  async function openObject(bucket, object, reload) {
    const url = objectUrl(bucket, object.key);
    const name = object.key.split("/").pop() ?? object.key;
    const facts = (type) => {
      const rows = [
        ["Key", object.key, false],
        ["Size", `${bytes(object.size)} (${object.size.toLocaleString()} bytes)`, false],
        ["Modified", object.modified, false],
        ["ETag", object.etag, true],
        ["Type", type, false],
        ["URL", location.origin + url, true]
      ];
      return h("dl", null, rows.filter(([, v]) => v).map(([k, v, mono]) => [h("dt", null, k), h("dd", { class: mono ? "mono" : null }, v)]));
    };
    const remove = () => confirmDanger({
      title: `Delete ${name}?`,
      warning: `The object ${object.key} (${bytes(object.size)}) is deleted. This can't be undone.`,
      action: "Delete object",
      run: async () => {
        await s3("DELETE", url);
        closeDrawer();
        notice(`Deleted ${object.key}`);
        reload();
      }
    });
    const actions = h(
      "div",
      { class: "actions" },
      h("a", { class: "button primary", href: url, download: name }, icon("download", 16), "Download"),
      button([icon("trash", 16), "Delete"], remove, "danger")
    );
    openDrawer(name, facts(""), actions);
    try {
      const head = await s3("HEAD", url);
      if (!$("drawer").hidden) openDrawer(name, facts(head.headers.get("content-type") ?? ""), actions);
    } catch {
    }
  }
  function dropZone(main, upload) {
    const overlay = h("div", { class: "drop", hidden: true }, h("div", null, icon("upload", 34), "Drop to upload here"));
    let depth = 0;
    const hasFiles = (e) => Boolean(e.dataTransfer && [...e.dataTransfer.types].includes("Files"));
    main.ondragenter = (e) => {
      if (!hasFiles(e)) return;
      depth += 1;
      overlay.hidden = false;
    };
    main.ondragleave = () => {
      depth = Math.max(0, depth - 1);
      if (!depth) overlay.hidden = true;
    };
    main.ondragover = (e) => {
      if (hasFiles(e)) e.preventDefault();
    };
    main.ondrop = (e) => {
      e.preventDefault();
      depth = 0;
      overlay.hidden = true;
      if (e.dataTransfer?.files.length) upload([...e.dataTransfer.files]);
    };
    return overlay;
  }
  function usage(bucket) {
    const row = service("s3")?.listings[0]?.items.find((i) => i.cells[0] === bucket);
    return row ? `${row.cells[1]}/${row.detail.size}` : "";
  }
  function bucketPage(main, path) {
    const [bucketPart, ...rest] = path.split("/");
    const bucket = decodeURIComponent(bucketPart);
    const prefix = rest.map(decodeURIComponent).join("/");
    let folders = [];
    let objects = [];
    let next = null;
    let problem = "";
    const status = h("span");
    const shown = table(
      ["Name", "Size", "Last modified"],
      () => [
        ...folders.filter((f) => matches([f])).map((f) => ({
          key: f,
          cells: [h("span", { class: "name-cell folder" }, icon("folder", 17), f.slice(prefix.length)), "—", ""],
          open: () => location.hash = folderHref(bucket, f)
        })),
        ...objects.filter((o) => matches([o.key])).map((o) => ({
          key: o.key,
          cells: [h("span", { class: "name-cell" }, icon("file", 17), o.key.slice(prefix.length)), bytes(o.size), o.modified],
          mono: [2],
          open: () => void openObject(bucket, o, () => void load(false))
        }))
      ],
      () => problem ? empty(problem) : empty("This folder is empty", "Drop files here, click Upload, or copy one in:", `aws s3 cp ./file.txt s3://${bucket}/${prefix}`)
    );
    const more = button("Show more", () => void load(true));
    more.hidden = true;
    async function load(append2) {
      try {
        const page = await listFolder(bucket, prefix, append2 ? next : null);
        folders = append2 ? folders.concat(page.folders) : page.folders;
        objects = append2 ? objects.concat(page.objects) : page.objects;
        next = page.next;
        problem = "";
      } catch (e) {
        folders = [];
        objects = [];
        next = null;
        problem = e.message;
      }
      more.hidden = !next;
      const parts2 = [folders.length && plural(folders.length, "folder"), plural(objects.length, "object")].filter(Boolean);
      status.textContent = problem ? "" : `${parts2.join(" · ")}${next ? " so far" : ""}`;
      shown.update();
    }
    async function upload(files) {
      for (const [i, file] of files.entries()) {
        status.textContent = `Uploading ${file.name} (${i + 1} of ${files.length})…`;
        try {
          await s3("PUT", objectUrl(bucket, prefix + file.name), file, file.type ? { "content-type": file.type } : {});
        } catch (e) {
          notice(`${file.name}: ${e.message}`, "bad");
        }
      }
      notice(`Uploaded ${plural(files.length, "file")}`);
      void load(false);
    }
    const picker = h("input", {
      type: "file",
      multiple: true,
      hidden: true,
      onchange: (e) => {
        const input = e.target;
        const files = [...input.files ?? []];
        input.value = "";
        void upload(files);
      }
    });
    const parts = prefix.split("/").filter(Boolean);
    main.replaceChildren(
      dropZone(main, (files) => void upload(files)),
      crumbs(
        ["S3", "#/s/s3"],
        ["Buckets", "#/s/s3"],
        [bucket, parts.length ? folderHref(bucket, "") : null],
        ...parts.map((part, i) => [part, i < parts.length - 1 ? folderHref(bucket, parts.slice(0, i + 1).join("/") + "/") : null])
      ),
      heading(h("span", { class: "with-tile" }, serviceTile("s3", 40), parts.length ? parts[parts.length - 1] : bucket), status),
      container(
        "Objects",
        {
          info: "Drop files anywhere to upload",
          actions: [filterBox(() => shown.update(), "Find objects by prefix"), iconButton("upload", "Upload", () => picker.click(), "primary"), picker]
        },
        shown.el,
        more
      )
    );
    void load(false);
    let seen = usage(bucket);
    return () => {
      const now = usage(bucket);
      if (now !== seen) {
        seen = now;
        void load(false);
      }
    };
  }
  function bucketNameProblem(name) {
    if (name.length < 3 || name.length > 63) return "Bucket names are 3 to 63 characters long.";
    if (!/^[a-z0-9.-]+$/.test(name)) return "Bucket names can use only lowercase letters, numbers, dots (.) and hyphens (-).";
    if (!/^[a-z0-9].*[a-z0-9]$/.test(name)) return "Bucket names start and end with a letter or a number.";
    if (name.includes("..")) return "Bucket names can't have two dots in a row.";
    if (/^\d+\.\d+\.\d+\.\d+$/.test(name)) return "Bucket names can't look like an IP address.";
    return null;
  }
  function newBucket() {
    const name = textInput({ placeholder: "my-bucket", mono: true });
    const nameField = field("Bucket name", name, {
      constraint: "3 to 63 characters: lowercase letters, numbers, dots (.) and hyphens (-), starting and ending with a letter or number."
    });
    const open = dialog({
      title: "Create bucket",
      body: [nameField.el],
      action: "Create bucket",
      confirm: async () => {
        const bucket = name.value.trim();
        await s3("PUT", `/${encodeURIComponent(bucket)}`);
        notice(`Created bucket ${bucket}`);
        location.hash = `#/s/s3/${encodeURIComponent(bucket)}/`;
      }
    });
    open.enable(false);
    name.addEventListener("input", () => {
      const problem = name.value ? bucketNameProblem(name.value.trim()) : null;
      nameField.error(problem);
      open.enable(Boolean(name.value) && !problem);
    });
  }
  var s3Browser = {
    noun: "bucket",
    open: (item) => location.hash = `#/s/s3/${encodeURIComponent(item.cells[0])}/`,
    page: bucketPage,
    create: newBucket
  };

  // service.ts
  var servicePage = (main, name) => {
    const s = service(name);
    if (!s) {
      main.replaceChildren(heading(name, state.inventory ? "No such service here." : "Loading…"));
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
            name === "sts" ? "aws sts get-caller-identity" : `aws ${name} help`
          )
        )
      );
      return null;
    }
    const listing = () => {
      const current2 = service(name) ?? s;
      return current2.listings[Math.min(state.tab[name] ?? 0, current2.listings.length - 1)];
    };
    const shown = table(
      listing().columns,
      () => filtered(listing().items).map((item) => ({
        key: item.cells[0],
        cells: item.cells,
        mono: [listing().columns.indexOf("ARN")],
        open: () => browser?.open ? browser.open(item) : openDrawer(item.cells[0], h("h3", null, "Record"), h("pre", null, JSON.stringify(expanded(item.detail), null, 2)))
      })),
      () => state.filter ? empty("Nothing matches the filter") : empty(`No ${listing().title.toLowerCase()} yet`, "Create one with the AWS CLI, an SDK or Terraform, and it shows up here.")
    );
    const create = browser?.create ? iconButton("plus", `Create ${browser.noun ?? "one"}`, () => browser.create?.(), "primary") : null;
    main.replaceChildren(
      crumbs([s.title, `#/s/${name}`], [listing().title, null]),
      heading(title, look(name).about),
      container(
        listing().title,
        { count: listing().items.length, actions: [filterBox(() => shown.update()), create] },
        shown.el
      )
    );
    return () => {
      if (!service(name)) return;
      const header = main.querySelector(".container h2 .count");
      if (header) header.textContent = ` (${listing().items.length})`;
      shown.update();
    };
  };

  // sqs.ts
  async function sqs(operation2, input) {
    const response = await fetch("/", {
      method: "POST",
      headers: { "x-amz-target": `AmazonSQS.${operation2}`, "content-type": "application/x-amz-json-1.0" },
      body: JSON.stringify(input)
    });
    const answer = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(String(answer.message ?? `${operation2}: ${response.status}`));
    return answer;
  }
  function queueUrl(name) {
    return `${location.origin}/${state.health?.account ?? "000000000000"}/${name}`;
  }
  function queueItem(name) {
    return service("sqs")?.listings[0]?.items.find((i) => i.cells[0] === name);
  }
  function counts(name) {
    return JSON.stringify(queueItem(name)?.detail.counts ?? null);
  }
  function stateLabel(s) {
    return h("span", { class: `status ${s === "available" ? "ok" : s === "in flight" ? "warn" : "info"}` }, s);
  }
  function openMessage(m) {
    let body = m.body;
    try {
      body = JSON.stringify(JSON.parse(m.body), null, 2);
    } catch {
    }
    const attributes = Object.entries(m.attributes ?? {});
    openDrawer(
      "Message",
      stateLabel(m.state),
      h(
        "dl",
        null,
        h("dt", null, "ID"),
        h("dd", { class: "mono" }, m.id),
        h("dt", null, "Sent"),
        h("dd", null, m.sent),
        h("dt", null, "Received"),
        h("dd", null, `${m.receives} times`),
        m.group ? [h("dt", null, "Group"), h("dd", null, m.group)] : null
      ),
      h("h3", null, "Body"),
      h("pre", null, body),
      attributes.length ? [
        h("h3", null, "Attributes"),
        h("dl", null, attributes.map(([k, v]) => [h("dt", null, k), h("dd", null, `${v.StringValue ?? v.BinaryValue ?? ""} `, h("small", { class: "muted" }, v.DataType))]))
      ] : null
    );
  }
  function figure(label, value, tone = "") {
    return h("div", { class: `figure ${tone}` }, h("span", null, label), h("strong", null, value));
  }
  function queuePage(main, path) {
    const name = decodeURIComponent(path.split("/")[0]);
    const url = queueUrl(name);
    let messages = [];
    let problem = "";
    const details = h("div");
    const drawDetails = () => {
      const item = queueItem(name);
      if (!item) {
        details.replaceChildren(empty("This queue isn't here", "It may have been deleted."));
        return;
      }
      const d = item.detail;
      const settings = d.settings ?? {};
      const copyUrl = button([icon("copy", 14), "Copy"], (e) => void copy(url, e.currentTarget), "link");
      const redrive = settings.redrive ? `${settings.redrive.target}, after ${settings.redrive.max_receives} receives` : "None";
      details.replaceChildren(
        h(
          "div",
          { class: "figures-row" },
          figure("Available", String(d.counts?.visible ?? 0), "ok"),
          figure("In flight", String(d.counts?.in_flight ?? 0), "warn"),
          figure("Delayed", String(d.counts?.delayed ?? 0), "info")
        ),
        h(
          "dl",
          { class: "details" },
          h("dt", null, "Type"),
          h("dd", null, d.fifo ? "FIFO" : "Standard"),
          h("dt", null, "URL"),
          h("dd", { class: "mono" }, url, copyUrl),
          h("dt", null, "Visibility timeout"),
          h("dd", null, `${settings.visibility_timeout ?? 30} s`),
          h("dt", null, "Delivery delay"),
          h("dd", null, `${settings.delay ?? 0} s`),
          h("dt", null, "Retention"),
          h("dd", null, `${Math.round((settings.retention ?? 345600) / 86400)} days`),
          h("dt", null, "Dead-letter queue"),
          h("dd", null, redrive),
          h("dt", null, "Created"),
          h("dd", null, d.created ?? "")
        )
      );
    };
    const list = table(
      ["Sent", "Body", "Group", "Received", "State"],
      () => messages.map((m) => ({
        key: m.id,
        cells: [time(Date.parse(m.sent)), m.body.length > 80 ? `${m.body.slice(0, 80)}…` : m.body, m.group ?? "", String(m.receives), stateLabel(m.state)],
        mono: [0, 1],
        open: () => openMessage(m)
      })),
      () => problem ? empty(problem) : empty("No messages", "Send one above, or from your code:", `aws sqs send-message --queue-url ${url} --message-body hello`)
    );
    async function load() {
      try {
        messages = (await get(`browse/sqs/${encodeURIComponent(name)}/messages`)).messages;
        problem = "";
      } catch (e) {
        messages = [];
        problem = e.message;
      }
      drawDetails();
      list.update();
    }
    const fifo = name.endsWith(".fifo");
    const body = h("textarea", { class: "input", rows: 4, placeholder: '{"order": 7}' });
    const group = h("input", { class: "input", placeholder: "Message group ID", value: "default" });
    const send = async () => {
      if (!body.value.trim()) return;
      try {
        const input = { QueueUrl: url, MessageBody: body.value };
        if (fifo) {
          input.MessageGroupId = group.value || "default";
          input.MessageDeduplicationId = `${Date.now()}`;
        }
        await sqs("SendMessage", input);
        body.value = "";
        notice("Message sent");
        void load();
      } catch (e) {
        notice(e.message, "bad");
      }
    };
    const purge = () => confirmDanger({
      title: `Purge ${name}?`,
      warning: "Every message in the queue is deleted, those in flight and delayed too. This can't be undone.",
      typeName: "purge",
      action: "Purge",
      run: async () => {
        await sqs("PurgeQueue", { QueueUrl: url });
        notice(`Purged ${name}`);
        void load();
      }
    });
    const remove = () => confirmDanger({
      title: `Delete queue ${name}?`,
      warning: "The queue and every message in it are deleted. This can't be undone.",
      typeName: name,
      action: "Delete queue",
      run: async () => {
        await sqs("DeleteQueue", { QueueUrl: url });
        notice(`Deleted ${name}`);
        location.hash = "#/s/sqs";
      }
    });
    main.replaceChildren(
      crumbs(["SQS", "#/s/sqs"], ["Queues", "#/s/sqs"], [name, null]),
      heading(
        h("span", { class: "with-tile" }, serviceTile("sqs", 40), name),
        fifo ? "FIFO queue" : "Standard queue",
        iconButton("trash", "Purge", purge),
        iconButton("trash", "Delete", remove, "danger")
      ),
      h(
        "div",
        { class: "grid" },
        h("div", { class: "c7" }, container("Details", { info: "Live" }, h("div", { class: "body" }, details))),
        h(
          "div",
          { class: "c5" },
          container(
            "Send a message",
            {},
            h(
              "div",
              { class: "body form" },
              h("label", null, "Body", body),
              fifo ? h("label", null, "Group", group) : null,
              iconButton("send", "Send message", () => void send(), "primary")
            )
          )
        ),
        h(
          "div",
          { class: "c12" },
          container(
            "Messages",
            { info: "Looked at, not received", actions: [iconButton("refresh", "Refresh", () => void load())] },
            list.el
          )
        )
      )
    );
    void load();
    let seen = counts(name);
    return () => {
      const now = counts(name);
      if (now !== seen) {
        seen = now;
        void load();
      }
    };
  }
  function queueNameProblem(name, fifo) {
    const base = fifo ? name.replace(/\.fifo$/, "") : name;
    if (!base || name.length > 80) return "A queue name has 1 to 80 characters.";
    if (!/^[A-Za-z0-9_-]+$/.test(base)) return "A queue name can use only letters, numbers, hyphens (-) and underscores (_).";
    return null;
  }
  function newQueue() {
    const type = radioTiles(
      "queue-type",
      [
        { value: "standard", title: "Standard", about: "At-least-once delivery; order isn't kept." },
        { value: "fifo", title: "FIFO", about: "First in, first out, each message once." }
      ],
      "standard"
    );
    const name = textInput({ placeholder: "my-queue", mono: true });
    const suffix = h("span", { class: "suffix mono", hidden: true }, ".fifo");
    const nameField = field("Name", h("div", { class: "with-suffix" }, name, suffix), {
      constraint: "Up to 80 characters: letters, numbers, hyphens (-) and underscores (_). Case matters."
    });
    const fifo = () => type.value() === "fifo";
    const full = () => {
      const given = name.value.trim();
      return fifo() && !given.endsWith(".fifo") ? `${given}.fifo` : given;
    };
    const open = dialog({
      title: "Create queue",
      body: [h("div", { class: "field" }, h("label", null, "Type"), h("div", { class: "field-hint" }, "You can't change it once the queue is made."), type.el), nameField.el],
      action: "Create queue",
      confirm: async () => {
        const queue = full();
        const attributes = fifo() ? { FifoQueue: "true", ContentBasedDeduplication: "true" } : {};
        await sqs("CreateQueue", { QueueName: queue, Attributes: attributes });
        notice(`Created queue ${queue}`);
        location.hash = `#/s/sqs/${encodeURIComponent(queue)}`;
      }
    });
    const check = () => {
      suffix.hidden = !fifo() || name.value.trim().endsWith(".fifo");
      const problem = name.value ? queueNameProblem(full(), fifo()) : null;
      nameField.error(problem);
      open.enable(Boolean(name.value.trim()) && !problem);
    };
    open.enable(false);
    name.addEventListener("input", check);
    type.onChange(check);
  }
  var sqsBrowser = {
    noun: "queue",
    open: (item) => location.hash = `#/s/sqs/${encodeURIComponent(item.cells[0])}`,
    page: queuePage,
    create: newQueue
  };

  // main.ts
  var REFRESH_MS = 2e3;
  var REQUESTS_MS = 1e3;
  var KEEP = 1e3;
  BROWSERS.s3 = s3Browser;
  BROWSERS.sqs = sqsBrowser;
  var PAGES = {
    home,
    service: servicePage,
    requests: requestsPage
  };
  var update = null;
  function current() {
    const [, page, name, ...rest] = location.hash.split("/");
    if (page === "s" && name) return { page: "service", name: decodeURIComponent(name), path: rest.join("/") };
    if (page === "requests") return { page: "requests", name: "", path: "" };
    return { page: "home", name: "", path: "" };
  }
  function route() {
    const where = current();
    const main = $("main");
    if (where.page !== "requests") state.filter = "";
    closeDrawer();
    main.ondragenter = main.ondragleave = main.ondragover = main.ondrop = null;
    renderSide(where);
    const browser = where.page === "service" && where.path ? BROWSERS[where.name] : void 0;
    update = browser?.page ? browser.page(main, where.path) : PAGES[where.page](main, where.name);
    main.scrollTop = 0;
  }
  function refreshed() {
    renderStatus();
    renderPins();
    renderSide(current());
    if (!update && current().page === "service" && state.inventory) route();
    else update?.();
  }
  async function loadInventory() {
    try {
      const [health2, inventory] = await Promise.all([get("health"), get("inventory")]);
      const changed = JSON.stringify(inventory) !== JSON.stringify(state.inventory);
      const wasDown = state.down;
      state.health = health2;
      state.inventory = inventory;
      state.down = false;
      if (changed || wasDown) refreshed();
      else renderStatus();
    } catch {
      state.down = true;
      renderStatus();
    }
  }
  async function loadRequests() {
    try {
      const answer = await get(`requests?after=${state.last}`);
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
    }
  }
  function every(ms, load) {
    void load();
    setInterval(() => {
      if (document.visibilityState === "visible") void load();
    }, ms);
  }
  function start2() {
    setUpTopBar();
    addEventListener("hashchange", route);
    route();
    every(REFRESH_MS, loadInventory);
    every(REQUESTS_MS, loadRequests);
  }
  start2();
})();
