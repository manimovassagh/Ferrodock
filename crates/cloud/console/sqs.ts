// SQS's browser: a queue's details and counts, sending a message, and
// the messages in it looked at without receiving them (so nothing is
// hidden or counted as received). Sending, purging and deleting go
// through SQS's own API, so they show in the request log.
// Pages: #/s/sqs/<queue>.

import { icon, serviceTile } from "./catalog";
import { copy, h, time } from "./dom";
import { button, container, crumbs, empty, heading, iconButton, notice, openDrawer, table } from "./frame";
import { get, service, state } from "./state";
import type { Browser, Item, Update } from "./types";

/** A message as the emulator shows it to the console. */
interface Peeked {
  id: string;
  body: string;
  attributes: Record<string, { DataType: string; StringValue?: string; BinaryValue?: string }> | null;
  group: string | null;
  sent: string;
  receives: number;
  state: "available" | "in flight" | "delayed";
}

/** Calls an SQS operation in its JSON protocol; throws what it says on failure. */
async function sqs(operation: string, input: Record<string, unknown>): Promise<Record<string, unknown>> {
  const response = await fetch("/", {
    method: "POST",
    headers: { "x-amz-target": `AmazonSQS.${operation}`, "content-type": "application/x-amz-json-1.0" },
    body: JSON.stringify(input),
  });
  const answer = (await response.json().catch(() => ({}))) as Record<string, unknown>;
  if (!response.ok) throw new Error(String(answer.message ?? `${operation}: ${response.status}`));
  return answer;
}

function queueUrl(name: string): string {
  return `${location.origin}/${state.health?.account ?? "000000000000"}/${name}`;
}

function queueItem(name: string): Item | undefined {
  return service("sqs")?.listings[0]?.items.find((i) => i.cells[0] === name);
}

/** A queue's counts, as one string, to notice when they change. */
function counts(name: string): string {
  return JSON.stringify(queueItem(name)?.detail.counts ?? null);
}

function stateLabel(s: Peeked["state"]): HTMLElement {
  return h("span", { class: `status ${s === "available" ? "ok" : s === "in flight" ? "warn" : "info"}` }, s);
}

function openMessage(m: Peeked): void {
  let body = m.body;
  try {
    body = JSON.stringify(JSON.parse(m.body), null, 2);
  } catch {
    // Not JSON: as it was.
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
      m.group ? [h("dt", null, "Group"), h("dd", null, m.group)] : null,
    ),
    h("h3", null, "Body"),
    h("pre", null, body),
    attributes.length
      ? [
          h("h3", null, "Attributes"),
          h("dl", null, attributes.map(([k, v]) => [h("dt", null, k), h("dd", null, `${v.StringValue ?? v.BinaryValue ?? ""} `, h("small", { class: "muted" }, v.DataType))])),
        ]
      : null,
  );
}

function figure(label: string, value: string, tone = ""): HTMLElement {
  return h("div", { class: `figure ${tone}` }, h("span", null, label), h("strong", null, value));
}

function queuePage(main: HTMLElement, path: string): Update {
  const name = decodeURIComponent(path.split("/")[0]);
  const url = queueUrl(name);
  let messages: Peeked[] = [];
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
    const copyUrl = button([icon("copy", 14), "Copy"], (e) => void copy(url, e.currentTarget as HTMLElement), "link");
    const redrive = settings.redrive ? `${settings.redrive.target}, after ${settings.redrive.max_receives} receives` : "None";
    details.replaceChildren(
      h(
        "div",
        { class: "figures-row" },
        figure("Available", String(d.counts?.visible ?? 0), "ok"),
        figure("In flight", String(d.counts?.in_flight ?? 0), "warn"),
        figure("Delayed", String(d.counts?.delayed ?? 0), "info"),
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
        h("dd", null, d.created ?? ""),
      ),
    );
  };

  const list = table(
    ["Sent", "Body", "Group", "Received", "State"],
    () =>
      messages.map((m) => ({
        key: m.id,
        cells: [time(Date.parse(m.sent)), m.body.length > 80 ? `${m.body.slice(0, 80)}…` : m.body, m.group ?? "", String(m.receives), stateLabel(m.state)],
        mono: [0, 1],
        open: () => openMessage(m),
      })),
    () => (problem ? empty(problem) : empty("No messages", "Send one above, or from your code:", `aws sqs send-message --queue-url ${url} --message-body hello`)),
  );

  async function load(): Promise<void> {
    try {
      messages = (await get<{ messages: Peeked[] }>(`browse/sqs/${encodeURIComponent(name)}/messages`)).messages;
      problem = "";
    } catch (e) {
      messages = [];
      problem = (e as Error).message;
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
      const input: Record<string, unknown> = { QueueUrl: url, MessageBody: body.value };
      if (fifo) {
        input.MessageGroupId = group.value || "default";
        input.MessageDeduplicationId = `${Date.now()}`;
      }
      await sqs("SendMessage", input);
      body.value = "";
      notice("Message sent");
      void load();
    } catch (e) {
      notice((e as Error).message, "bad");
    }
  };
  const purge = async () => {
    if (!confirm(`Delete every message in ${name}?`)) return;
    try {
      await sqs("PurgeQueue", { QueueUrl: url });
      notice(`Purged ${name}`);
      void load();
    } catch (e) {
      notice((e as Error).message, "bad");
    }
  };
  const remove = async () => {
    if (!confirm(`Delete the queue ${name} and its messages? This can't be undone.`)) return;
    try {
      await sqs("DeleteQueue", { QueueUrl: url });
      notice(`Deleted ${name}`);
      location.hash = "#/s/sqs";
    } catch (e) {
      notice((e as Error).message, "bad");
    }
  };

  main.replaceChildren(
    crumbs(["SQS", "#/s/sqs"], ["Queues", "#/s/sqs"], [name, null]),
    heading(
      h("span", { class: "with-tile" }, serviceTile("sqs", 40), name),
      fifo ? "FIFO queue" : "Standard queue",
      iconButton("trash", "Purge", () => void purge()),
      iconButton("trash", "Delete", () => void remove(), "danger"),
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
            iconButton("send", "Send message", () => void send(), "primary"),
          ),
        ),
      ),
      h(
        "div",
        { class: "c12" },
        container(
          "Messages",
          { info: "Looked at, not received", actions: [iconButton("refresh", "Refresh", () => void load())] },
          list.el,
        ),
      ),
    ),
  );
  void load();
  // Look again when the queue's counts change, not on every refresh:
  // this page's own requests are logged too.
  let seen = counts(name);
  return () => {
    const now = counts(name);
    if (now !== seen) {
      seen = now;
      void load();
    }
  };
}

async function newQueue(): Promise<void> {
  const name = prompt("Name of the new queue (end it in .fifo for a FIFO queue):")?.trim();
  if (!name) return;
  const attributes: Record<string, string> = name.endsWith(".fifo") ? { FifoQueue: "true", ContentBasedDeduplication: "true" } : {};
  try {
    await sqs("CreateQueue", { QueueName: name, Attributes: attributes });
    notice(`Created queue ${name}`);
    location.hash = `#/s/sqs/${encodeURIComponent(name)}`;
  } catch (e) {
    notice((e as Error).message, "bad");
  }
}

export const sqsBrowser: Browser = {
  noun: "queue",
  open: (item) => (location.hash = `#/s/sqs/${encodeURIComponent(item.cells[0])}`),
  page: queuePage,
  create: () => void newQueue(),
};
