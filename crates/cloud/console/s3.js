// S3's browser in the console: a bucket's folders and objects, with
// upload, download and delete. It goes through S3's own API (unsigned
// requests are S3's), so everything it does shows in the request log like
// any other client's. Pages: #/s/s3/<bucket>/<folder/>.

"use strict";

/// The most keys S3 lists at once.
const PAGE = 1000;

/// `a/b c/d.txt` as a URL path, each part encoded and the slashes kept.
function keyPath(key) {
  return key.split("/").map(encodeURIComponent).join("/");
}

/// An object's address, for links and requests.
function objectUrl(bucket, key) {
  return `/${encodeURIComponent(bucket)}/${keyPath(key)}`;
}

/// Sends a request to S3, and says what went wrong when it fails.
async function s3(method, path, body, headers) {
  const response = await fetch(path, { method, body, headers, cache: "no-store" });
  if (!response.ok) {
    const text = await response.text();
    const message = /<Message>([^<]*)<\/Message>/.exec(text);
    throw new Error(message ? message[1] : `${method} ${path}: ${response.status}`);
  }
  return response;
}

/// One page of a folder: its subfolders and objects.
async function listFolder(bucket, prefix, token) {
  const query = new URLSearchParams({ "list-type": "2", delimiter: "/", prefix, "max-keys": String(PAGE) });
  if (token) query.set("continuation-token", token);
  const response = await s3("GET", `/${encodeURIComponent(bucket)}?${query}`);
  const doc = new DOMParser().parseFromString(await response.text(), "application/xml");
  const text = (el, tag) => {
    const found = el.getElementsByTagName(tag)[0];
    return found ? found.textContent : "";
  };
  return {
    folders: [...doc.getElementsByTagName("CommonPrefixes")].map((el) => text(el, "Prefix")),
    objects: [...doc.getElementsByTagName("Contents")].map((el) => ({
      key: text(el, "Key"),
      size: Number(text(el, "Size")),
      modified: text(el, "LastModified"),
      etag: text(el, "ETag"),
    })),
    next: text(doc, "IsTruncated") === "true" ? text(doc, "NextContinuationToken") : null,
  };
}

/// The link back to S3's page, then one link per folder down to `prefix`.
function crumbs(bucket, prefix) {
  const parts = prefix.split("/").filter(Boolean);
  const links = [h("a", { href: "#/s/s3" }, "S3"), h("span", { class: "dim" }, " / ")];
  links.push(h("a", { href: `#/s/s3/${encodeURIComponent(bucket)}/` }, bucket));
  parts.forEach((part, i) => {
    const down = parts.slice(0, i + 1).join("/") + "/";
    links.push(h("span", { class: "dim" }, " / "), h("a", { href: `#/s/s3/${encodeURIComponent(bucket)}/${keyPath(down)}` }, part));
  });
  return h("div", { class: "crumbs" }, links);
}

/// Shows an object on the right: what it is, Download and Delete.
async function openObject(bucket, object, reload) {
  const url = objectUrl(bucket, object.key);
  const name = object.key.split("/").pop();
  const facts = (head) =>
    h(
      "dl",
      {},
      [
        ["Key", object.key],
        ["Size", `${bytes(object.size)} (${object.size} bytes)`],
        ["Modified", object.modified],
        ["ETag", object.etag],
        ["Type", head ? head.get("content-type") : ""],
        ["URL", location.origin + url],
      ]
        .filter(([, v]) => v)
        .flatMap(([k, v]) => [h("dt", {}, k), h("dd", { class: k === "URL" || k === "ETag" ? "mono" : null }, v)]),
    );
  const buttons = h(
    "div",
    { class: "actions" },
    h("a", { class: "button primary", href: url, download: name }, "Download"),
    h(
      "button",
      {
        class: "button danger",
        onclick: async () => {
          if (!confirm(`Delete ${object.key}? This can't be undone.`)) return;
          try {
            await s3("DELETE", url);
            closeDrawer();
            reload();
          } catch (e) {
            alert(e.message);
          }
        },
      },
      "Delete",
    ),
  );
  openDrawer(name, facts(null), buttons);
  try {
    const head = await s3("HEAD", url);
    if (!$("drawer").hidden) openDrawer(name, facts(head.headers), buttons);
  } catch {
    // The facts from the listing are enough.
  }
}

/// A bucket's folder: `path` is `<bucket>/<folder/>`, as in the address.
function bucketPage(main, path) {
  const [bucketPart, ...rest] = path.split("/");
  const bucket = decodeURIComponent(bucketPart);
  const prefix = rest.map(decodeURIComponent).join("/");
  let folders = [];
  let objects = [];
  let next = null;
  let problem = "";
  const status = h("p", { class: "dim" });

  const rows = () => [
    ...folders
      .filter((f) => matches([f]))
      .map((f) => ({
        key: f,
        cells: [h("span", { class: "folder" }, f.slice(prefix.length)), "", ""],
        open: () => (location.hash = `#/s/s3/${encodeURIComponent(bucket)}/${keyPath(f)}`),
      })),
    ...objects
      .filter((o) => matches([o.key]))
      .map((o) => ({
        key: o.key,
        cells: [o.key.slice(prefix.length), bytes(o.size), o.modified],
        mono: [2],
        open: () => openObject(bucket, o, load),
      })),
  ];
  const shown = table(["Name", "Size", "Modified"], rows, () =>
    h(
      "div",
      { class: "empty" },
      problem || (state.filter ? "Nothing matches the filter." : "This folder is empty. Upload a file, or copy one in:"),
      problem ? null : h("br"),
      problem ? null : h("code", {}, `aws s3 cp ./file.txt s3://${bucket}/${prefix}`),
    ),
  );
  const more = h("button", { class: "button", hidden: true, onclick: () => load(true) }, "Show more");

  async function load(append) {
    try {
      const page = await listFolder(bucket, prefix, append ? next : null);
      folders = append ? folders.concat(page.folders) : page.folders;
      objects = append ? objects.concat(page.objects) : page.objects;
      next = page.next;
      problem = "";
    } catch (e) {
      folders = [];
      objects = [];
      next = null;
      problem = e.message;
    }
    more.hidden = !next;
    status.textContent = problem ? "" : `${plural(folders.length, "folder")} · ${plural(objects.length, "object")}${next ? " so far" : ""}`;
    shown.update();
  }

  const picker = h("input", {
    type: "file",
    multiple: true,
    hidden: true,
    onchange: async (e) => {
      const files = [...e.target.files];
      e.target.value = "";
      for (const [i, file] of files.entries()) {
        status.textContent = `Uploading ${file.name} (${i + 1} of ${files.length})…`;
        try {
          await s3("PUT", objectUrl(bucket, prefix + file.name), file, file.type ? { "content-type": file.type } : {});
        } catch (err) {
          alert(`${file.name}: ${err.message}`);
        }
      }
      load();
    },
  });

  main.replaceChildren(
    crumbs(bucket, prefix),
    h("h1", {}, prefix ? prefix.split("/").filter(Boolean).pop() : bucket),
    status,
    h(
      "div",
      { class: "toolbar" },
      filterBox(() => shown.update()),
      h("button", { class: "button primary", onclick: () => picker.click() }, "Upload"),
      picker,
    ),
    shown.el,
    more,
  );
  load();
  // List again when the bucket's count or size changes (the CLI wrote
  // something), not on every refresh: this page's own requests are
  // logged too, and each new one refreshes the page.
  let seen = usage(bucket);
  return () => {
    const now = usage(bucket);
    if (now !== seen) {
      seen = now;
      load();
    }
  };
}

/// A bucket's object count and size from the inventory, as one string.
function usage(bucket) {
  const service = state.inventory && state.inventory.services.find((s) => s.name === "s3");
  const row = service && service.listings[0] && service.listings[0].items.find((i) => i.cells[0] === bucket);
  return row ? `${row.cells[1]}/${row.detail.size}` : "";
}

/// The New bucket button on S3's page.
function newBucket() {
  return h(
    "button",
    {
      class: "button primary",
      onclick: async () => {
        const name = prompt("Name of the new bucket (lowercase letters, digits, dots and dashes):");
        if (!name) return;
        try {
          await s3("PUT", `/${encodeURIComponent(name.trim())}`);
          location.hash = `#/s/s3/${encodeURIComponent(name.trim())}/`;
        } catch (e) {
          alert(e.message);
        }
      },
    },
    "New bucket",
  );
}

BROWSERS.s3 = {
  open: (item) => (location.hash = `#/s/s3/${encodeURIComponent(item.cells[0])}/`),
  page: bucketPage,
  actions: () => [newBucket()],
};
