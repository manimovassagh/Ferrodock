// How the console shows each service: its category, icon and one line
// about it, and the services still to come. The emulator says which
// services there are; this only says how they look. A service missing
// here still shows, under Other services.

"use strict";

/// The categories, in order, each with the gradient of its tiles.
/// Security and identity comes last: tools use it behind the scenes.
const CATEGORIES = [
  { name: "Storage", tile: "linear-gradient(135deg, #2e8b57, #7ac143)" },
  { name: "Application integration", tile: "linear-gradient(135deg, #b0084d, #ff4f8b)" },
  { name: "Databases", tile: "linear-gradient(135deg, #2e27ad, #527fff)" },
  { name: "Compute", tile: "linear-gradient(135deg, #c8511b, #ff9900)" },
  { name: "Other services", tile: "linear-gradient(135deg, #545b64, #8d99a8)" },
  { name: "Security and identity", tile: "linear-gradient(135deg, #bd0816, #ff5252)" },
];

const CATALOG = {
  s3: { category: "Storage", icon: "bucket", about: "Buckets and objects", noun: "bucket" },
  sqs: { category: "Application integration", icon: "queue", about: "Message queues", noun: "queue" },
  sns: { title: "SNS", category: "Application integration", icon: "bell", about: "Topics and subscriptions", planned: "coming next" },
  dynamodb: { title: "DynamoDB", category: "Databases", icon: "table", about: "Tables of items", planned: "planned" },
  lambda: { title: "Lambda", category: "Compute", icon: "bolt", about: "Functions", planned: "planned" },
  iam: { category: "Security and identity", icon: "key", about: "Roles, users and policies", noun: "role" },
  sts: { category: "Security and identity", icon: "badge", about: "Temporary credentials, used by tools" },
};

/// Services shown in the bar under the top one, in order.
const PINNED = ["s3", "sqs", "iam"];

/// How a service looks, with defaults for one the catalog doesn't know.
function look(name) {
  return CATALOG[name] || { category: "Other services", icon: "cube", about: "" };
}

function category(name) {
  return CATEGORIES.find((c) => c.name === look(name).category) || CATEGORIES[4];
}

/// Icon outlines, drawn on a 24-unit square.
const ICONS = {
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
  copy: "M9 9h10v10H9zM5 15V5h10",
  grid: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
  search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zm5-2 4 4",
  cloud: "M7 18h10.5a4.5 4.5 0 0 0 .6-8.96A6 6 0 0 0 6.6 9.5 4.25 4.25 0 0 0 7 18Z",
  plus: "M12 5v14M5 12h14",
  refresh: "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7",
  lock: "M6 11h12v9H6zM8 11V8a4 4 0 0 1 8 0v3",
  send: "M4 12 20 4l-6 16-3-7z",
  trash: "M5 7h14M10 7V4h4v3m-7 0 1 13h8l1-13",
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

/// An icon on a tile of a category's gradient.
function tileOf(glyph, gradient, size) {
  return h(
    "span",
    { class: "tile", style: `background:${gradient};width:${size}px;height:${size}px` },
    icon(glyph, Math.round(size * 0.56)),
  );
}

/// A service's icon on its category's tile.
function serviceTile(name, size = 40) {
  return tileOf(look(name).icon, category(name).tile, size);
}

/// The services by category, in order: those the emulator has, then the
/// planned ones (`planned: true` on those).
function catalogue(services) {
  const have = new Set(services.map((s) => s.name));
  const planned = Object.entries(CATALOG)
    .filter(([name, entry]) => entry.planned && !have.has(name))
    .map(([name, entry]) => ({ name, title: entry.title, planned: entry.planned }));
  const all = [...services, ...planned];
  return CATEGORIES.map((c) => ({ category: c.name, services: all.filter((s) => look(s.name).category === c.name) })).filter(
    (g) => g.services.length,
  );
}
