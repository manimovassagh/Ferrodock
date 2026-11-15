// S3's browser: a bucket's folders and objects, with upload (a button or
// files dropped on the page), download and delete, and New bucket. It
// goes through S3's own API (unsigned requests are S3's), so what it does
// shows in the request log like any other tool's.
// Pages: #/s/s3/<bucket>/<folder/>.

import { icon, serviceTile } from "./catalog";
import { $, bytes, h, plural } from "./dom";
import { confirmDanger, dialog } from "./dialog";
import { button, closeDrawer, container, crumbs, empty, filterBox, heading, iconButton, notice, openDrawer, table } from "./frame";
import { field, textInput } from "./form";
import { matches, service } from "./state";
import type { Browser, Update } from "./types";

/** The most keys S3 lists at once. */
const PAGE = 1000;

interface S3Object {
  key: string;
  size: number;
  modified: string;
  etag: string;
}

/** `a/b c/d.txt` as a URL path, each part encoded and the slashes kept. */
function keyPath(key: string): string {
  return key.split("/").map(encodeURIComponent).join("/");
}

function objectUrl(bucket: string, key: string): string {
  return `/${encodeURIComponent(bucket)}/${keyPath(key)}`;
}

function folderHref(bucket: string, prefix: string): string {
  return `#/s/s3/${encodeURIComponent(bucket)}/${keyPath(prefix)}`;
}

/** Sends a request to S3, and says what went wrong when it fails. */
async function s3(method: string, path: string, body?: BodyInit, headers?: HeadersInit): Promise<Response> {
  const response = await fetch(path, { method, body, headers, cache: "no-store" });
  if (!response.ok) {
    const text = await response.text();
    const message = /<Message>([^<]*)<\/Message>/.exec(text);
    throw new Error(message ? message[1] : `${method} ${path}: ${response.status}`);
  }
  return response;
}

/** One page of a folder: its subfolders and objects. */
async function listFolder(bucket: string, prefix: string, token: string | null) {
  const query = new URLSearchParams({ "list-type": "2", delimiter: "/", prefix, "max-keys": String(PAGE) });
  if (token) query.set("continuation-token", token);
  const response = await s3("GET", `/${encodeURIComponent(bucket)}?${query}`);
  const doc = new DOMParser().parseFromString(await response.text(), "application/xml");
  const text = (el: Element | Document, tag: string) => el.getElementsByTagName(tag)[0]?.textContent ?? "";
  return {
    folders: [...doc.getElementsByTagName("CommonPrefixes")].map((el) => text(el, "Prefix")),
    objects: [...doc.getElementsByTagName("Contents")].map(
      (el): S3Object => ({
        key: text(el, "Key"),
        size: Number(text(el, "Size")),
        modified: text(el, "LastModified"),
        etag: text(el, "ETag"),
      }),
    ),
    next: text(doc, "IsTruncated") === "true" ? text(doc, "NextContinuationToken") : null,
  };
}

/** Shows an object on the right: what it is, Download and Delete. */
async function openObject(bucket: string, object: S3Object, reload: () => void): Promise<void> {
  const url = objectUrl(bucket, object.key);
  const name = object.key.split("/").pop() ?? object.key;
  const facts = (type: string) => {
    const rows: [string, string, boolean][] = [
      ["Key", object.key, false],
      ["Size", `${bytes(object.size)} (${object.size.toLocaleString()} bytes)`, false],
      ["Modified", object.modified, false],
      ["ETag", object.etag, true],
      ["Type", type, false],
      ["URL", location.origin + url, true],
    ];
    return h("dl", null, rows.filter(([, v]) => v).map(([k, v, mono]) => [h("dt", null, k), h("dd", { class: mono ? "mono" : null }, v)]));
  };
  const remove = () =>
    confirmDanger({
      title: `Delete ${name}?`,
      warning: `The object ${object.key} (${bytes(object.size)}) is deleted. This can't be undone.`,
      action: "Delete object",
      run: async () => {
        await s3("DELETE", url);
        closeDrawer();
        notice(`Deleted ${object.key}`);
        reload();
      },
    });
  const actions = h(
    "div",
    { class: "actions" },
    h("a", { class: "button primary", href: url, download: name }, icon("download", 16), "Download"),
    button([icon("trash", 16), "Delete"], remove, "danger"),
  );
  openDrawer(name, facts(""), actions);
  try {
    const head = await s3("HEAD", url);
    if (!$("drawer").hidden) openDrawer(name, facts(head.headers.get("content-type") ?? ""), actions);
  } catch {
    // The facts from the listing are enough.
  }
}

/** Files dragged onto the page go to `upload`, with an overlay meanwhile. The router clears these listeners. */
function dropZone(main: HTMLElement, upload: (files: File[]) => void): HTMLElement {
  const overlay = h("div", { class: "drop", hidden: true }, h("div", null, icon("upload", 34), "Drop to upload here"));
  let depth = 0;
  const hasFiles = (e: DragEvent) => Boolean(e.dataTransfer && [...e.dataTransfer.types].includes("Files"));
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

/** A bucket's object count and size from the inventory, as one string. */
function usage(bucket: string): string {
  const row = service("s3")?.listings[0]?.items.find((i) => i.cells[0] === bucket);
  return row ? `${row.cells[1]}/${row.detail.size}` : "";
}

/** A bucket's folder: `path` is `<bucket>/<folder/>`, as in the address. */
function bucketPage(main: HTMLElement, path: string): Update {
  const [bucketPart, ...rest] = path.split("/");
  const bucket = decodeURIComponent(bucketPart);
  const prefix = rest.map(decodeURIComponent).join("/");
  let folders: string[] = [];
  let objects: S3Object[] = [];
  let next: string | null = null;
  let problem = "";
  const status = h("span");

  const shown = table(
    ["Name", "Size", "Last modified"],
    () => [
      ...folders
        .filter((f) => matches([f]))
        .map((f) => ({
          key: f,
          cells: [h("span", { class: "name-cell folder" }, icon("folder", 17), f.slice(prefix.length)), "—", ""],
          open: () => (location.hash = folderHref(bucket, f)),
        })),
      ...objects
        .filter((o) => matches([o.key]))
        .map((o) => ({
          key: o.key,
          cells: [h("span", { class: "name-cell" }, icon("file", 17), o.key.slice(prefix.length)), bytes(o.size), o.modified],
          mono: [2],
          open: () => void openObject(bucket, o, () => void load(false)),
        })),
    ],
    () =>
      problem
        ? empty(problem)
        : empty("This folder is empty", "Drop files here, click Upload, or copy one in:", `aws s3 cp ./file.txt s3://${bucket}/${prefix}`),
  );
  const more = button("Show more", () => void load(true));
  more.hidden = true;

  async function load(append: boolean): Promise<void> {
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
      problem = (e as Error).message;
    }
    more.hidden = !next;
    const parts = [folders.length && plural(folders.length, "folder"), plural(objects.length, "object")].filter(Boolean);
    status.textContent = problem ? "" : `${parts.join(" · ")}${next ? " so far" : ""}`;
    shown.update();
  }

  async function upload(files: File[]): Promise<void> {
    for (const [i, file] of files.entries()) {
      status.textContent = `Uploading ${file.name} (${i + 1} of ${files.length})…`;
      try {
        await s3("PUT", objectUrl(bucket, prefix + file.name), file, file.type ? { "content-type": file.type } : {});
      } catch (e) {
        notice(`${file.name}: ${(e as Error).message}`, "bad");
      }
    }
    notice(`Uploaded ${plural(files.length, "file")}`);
    void load(false);
  }

  const picker = h("input", {
    type: "file",
    multiple: true,
    hidden: true,
    onchange: (e: Event) => {
      const input = e.target as HTMLInputElement;
      const files = [...(input.files ?? [])];
      input.value = "";
      void upload(files);
    },
  });

  const parts = prefix.split("/").filter(Boolean);
  main.replaceChildren(
    dropZone(main, (files) => void upload(files)),
    crumbs(
      ["S3", "#/s/s3"],
      ["Buckets", "#/s/s3"],
      [bucket, parts.length ? folderHref(bucket, "") : null],
      ...parts.map((part, i): [string, string | null] => [part, i < parts.length - 1 ? folderHref(bucket, parts.slice(0, i + 1).join("/") + "/") : null]),
    ),
    heading(h("span", { class: "with-tile" }, serviceTile("s3", 40), parts.length ? parts[parts.length - 1] : bucket), status),
    container(
      "Objects",
      {
        info: "Drop files anywhere to upload",
        actions: [filterBox(() => shown.update(), "Find objects by prefix"), iconButton("upload", "Upload", () => picker.click(), "primary"), picker],
      },
      shown.el,
      more,
    ),
  );
  void load(false);
  // List again when the bucket's count or size changes (the CLI wrote
  // something), not on every refresh: this page's own requests are
  // logged too, and each new one refreshes the page.
  let seen = usage(bucket);
  return () => {
    const now = usage(bucket);
    if (now !== seen) {
      seen = now;
      void load(false);
    }
  };
}

/** What's wrong with a bucket name, by S3's rules, or null. */
export function bucketNameProblem(name: string): string | null {
  if (name.length < 3 || name.length > 63) return "Bucket names are 3 to 63 characters long.";
  if (!/^[a-z0-9.-]+$/.test(name)) return "Bucket names can use only lowercase letters, numbers, dots (.) and hyphens (-).";
  if (!/^[a-z0-9].*[a-z0-9]$/.test(name)) return "Bucket names start and end with a letter or a number.";
  if (name.includes("..")) return "Bucket names can't have two dots in a row.";
  if (/^\d+\.\d+\.\d+\.\d+$/.test(name)) return "Bucket names can't look like an IP address.";
  return null;
}

/** Asks for a new bucket's name, checked as it's typed, and makes it. */
function newBucket(): void {
  const name = textInput({ placeholder: "my-bucket", mono: true });
  const nameField = field("Bucket name", name, {
    constraint: "3 to 63 characters: lowercase letters, numbers, dots (.) and hyphens (-), starting and ending with a letter or number.",
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
    },
  });
  open.enable(false);
  name.addEventListener("input", () => {
    const problem = name.value ? bucketNameProblem(name.value.trim()) : null;
    nameField.error(problem);
    open.enable(Boolean(name.value) && !problem);
  });
}

export const s3Browser: Browser = {
  noun: "bucket",
  open: (item) => (location.hash = `#/s/s3/${encodeURIComponent(item.cells[0])}/`),
  page: bucketPage,
  create: newBucket,
};
