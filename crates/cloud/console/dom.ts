// Building the page, and small helpers for showing values. Everything is
// built with textContent, never as HTML: names come from whoever called
// the APIs.

import type { Captured } from "./types";

export type Child = Node | string | number | null | undefined | false | Child[];

type Attributes = Record<string, string | number | boolean | null | undefined | EventListener>;

/** An element with attributes (`on…` ones as listeners) and children. */
export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attributes?: Attributes | null,
  ...children: Child[]
): HTMLElementTagNameMap[K] {
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

/** Appends children, arrays flattened and empty ones skipped. */
export function append(el: Element, children: Child[]): void {
  for (const child of children) {
    if (child == null || child === false) continue;
    if (Array.isArray(child)) append(el, child);
    else el.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
}

/** Replaces an element's children, arrays flattened. */
export function fill(el: Element, ...children: Child[]): void {
  el.replaceChildren();
  append(el, children);
}

export function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`no #${id} on the page`);
  return el;
}

export function plural(n: number, word: string): string {
  return `${n.toLocaleString()} ${word}${n === 1 ? "" : "s"}`;
}

/** A size for people: `512 B`, `1.4 MB`. */
export function bytes(n: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${n} B` : `${value.toFixed(1)} ${units[unit]}`;
}

export function time(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour12: false });
}

/** How long, for people: `2 h 14 min`, `40 s`. */
export function duration(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min`;
  const d = Math.floor(m / 1440);
  if (d > 0) return `${d} d ${Math.floor((m % 1440) / 60)} h`;
  return `${Math.floor(m / 60)} h ${m % 60} min`;
}

/** Copies `text`, and says so on `button` for a moment. */
export async function copy(text: string, button: HTMLElement): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // Without the clipboard API (a page not on localhost, say): the old way.
    const area = h("textarea", { style: "position:fixed;opacity:0" }, text);
    document.body.append(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
  button.classList.add("done");
  setTimeout(() => button.classList.remove("done"), 1400);
}

/** A record with the JSON kept in its text fields opened up. */
export function expanded(value: unknown): unknown {
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

/** XML one element to a line, indented; text-only elements on one line. */
export function prettyXml(text: string): string {
  const tokens = text.replace(/>\s+</g, "><").match(/<[^>]+>|[^<]+/g) ?? [];
  const lines: string[] = [];
  let depth = 0;
  let open = false;
  for (const token of tokens) {
    const tag = token.startsWith("<");
    const closing = token.startsWith("</");
    const single = token.startsWith("<?") || token.startsWith("<!") || token.endsWith("/>");
    if (!tag || (closing && open)) {
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

/** A body as text, laid out when it's JSON or XML. */
export function readable(body: Captured | undefined): string {
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
      // Neither: as it was.
    }
  }
  return body.truncated ? `${text}\n… (${body.size} bytes in all)` : text;
}

/** Which tool sent a request, from its user agent: `AWS CLI 2.31`. */
export function sender(agent: string): string {
  const known: [RegExp, string][] = [
    [/aws-cli\/(\d+\.\d+)/, "AWS CLI"],
    [/Boto3\/(\d+\.\d+)/i, "boto3"],
    [/aws-sdk-js\/(\d+\.\d+)/, "JavaScript SDK"],
    [/aws-sdk-java\/(\d+\.\d+)/, "Java SDK"],
    [/aws-sdk-go-v2\/(\d+\.\d+)/, "Go SDK"],
    [/aws-sdk-rust\/(\d+\.\d+)/, "Rust SDK"],
    [/Terraform\/(\d+\.\d+)/, "Terraform"],
    [/Mozilla/, "Browser"],
  ];
  for (const [pattern, name] of known) {
    const found = pattern.exec(agent);
    if (found) return found[1] ? `${name} ${found[1]}` : name;
  }
  return agent.split(" ")[0] ?? "";
}
