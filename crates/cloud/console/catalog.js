// How the console shows each service: its group, icon, colour and one
// line about it. The emulator says which services there are; this only
// says how they look. A service missing here still shows, under Services.

"use strict";

/// The groups, in the order the sidebar and the home page show them.
/// Basics (credentials and identity) come last: tools use them behind
/// the scenes, people rarely look at them.
const GROUPS = ["Storage", "Messaging", "Databases", "Compute", "Services", "Basics"];

const CATALOG = {
  s3: { group: "Storage", icon: "bucket", color: "#2f9e6b", about: "Buckets and objects" },
  sqs: { group: "Messaging", icon: "queue", color: "#d9822b", about: "Message queues" },
  sns: { group: "Messaging", icon: "bell", color: "#c94f7c", about: "Topics and subscriptions" },
  dynamodb: { group: "Databases", icon: "table", color: "#3d6fd8", about: "Tables of items" },
  lambda: { group: "Compute", icon: "bolt", color: "#e0662f", about: "Functions" },
  iam: { group: "Basics", icon: "key", color: "#7b61c9", about: "Roles, users and policies" },
  sts: { group: "Basics", icon: "badge", color: "#6c7a89", about: "Temporary credentials, used by tools" },
};

/// How a service looks, with defaults for one the catalog doesn't know.
function look(name) {
  return CATALOG[name] || { group: "Services", icon: "cube", color: "#8a8580", about: "" };
}

/// Icon outlines, drawn on a 24-unit square.
const ICONS = {
  home: "M4 11l8-7 8 7v8a1 1 0 0 1-1 1h-4v-6H9v6H5a1 1 0 0 1-1-1z",
  bucket: "M4 7c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zm0 0 1.6 11.2c.2 1.6 3 2.8 6.4 2.8s6.2-1.2 6.4-2.8L20 7",
  queue: "M4 6h16M4 12h16M4 18h10m3 0 3-3m-3 3 3 3",
  bell: "M6 16V11a6 6 0 1 1 12 0v5l1.5 2h-15zM10 20a2 2 0 0 0 4 0",
  table: "M4 5h16v14H4zM4 10h16M4 15h16M10 5v14",
  bolt: "M13 3 5 14h6l-1 7 8-11h-6z",
  key: "M8 16a4 4 0 1 1 3.9-5H21v3h-2v2h-3v-2h-4.1A4 4 0 0 1 8 16zm0-4h.01",
  badge: "M12 3 5 6v5c0 4.5 3 8.3 7 10 4-1.7 7-5.5 7-10V6zm-3 9 2 2 4-4",
  cube: "M12 3 4 7.5v9L12 21l8-4.5v-9zM4 7.5 12 12l8-4.5M12 12v9",
  activity: "M3 12h4l3-7 4 14 3-7h4",
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  file: "M7 3h7l5 5v13H7zM14 3v5h5",
  upload: "M12 16V4m0 0-5 5m5-5 5 5M5 20h14",
  copy: "M9 9h10v10H9zM5 15V5h10",
};

/// An icon as an inline SVG element, in the current text colour.
function icon(name, size = 18) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", size);
  svg.setAttribute("height", size);
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("class", "icon");
  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("d", ICONS[name] || ICONS.cube);
  svg.append(path);
  return svg;
}

/// A service's icon on a tile of its colour.
function serviceTile(name, size = 40) {
  const { icon: glyph, color } = look(name);
  return h(
    "span",
    { class: "tile", style: `--c:${color};width:${size}px;height:${size}px` },
    icon(glyph, Math.round(size * 0.55)),
  );
}

/// The services there are, grouped and in the catalog's order.
function grouped(services) {
  return GROUPS.map((group) => ({ group, services: services.filter((s) => look(s.name).group === group) })).filter(
    (g) => g.services.length,
  );
}
