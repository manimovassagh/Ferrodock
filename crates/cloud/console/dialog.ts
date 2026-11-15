// Dialogs in the page, instead of the browser's own prompt and confirm:
// a title, a body, Cancel and one main button. Escape, the backdrop and ×
// cancel; focus stays inside while it's open.

import { type Child, h } from "./dom";
import { field, textInput } from "./form";

interface Options {
  title: string;
  body: Child[];
  /** The main button's label: "Delete", "Create bucket". */
  action: string;
  danger?: boolean;
  /** Called on the main button; resolve to close, throw to stay open with the error shown. */
  confirm(): Promise<void> | void;
}

/** Opens a dialog; gives back a function to turn its main button on or off. */
export function dialog(options: Options): { enable(on: boolean): void } {
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
      h("footer", null, h("button", { class: "button link", onclick: () => close() }, "Cancel"), main),
    ),
  );
  const before = document.activeElement as HTMLElement | null;
  const close = () => {
    backdrop.remove();
    removeEventListener("keydown", keys, true);
    before?.focus();
  };
  const focusable = () => [...backdrop.querySelectorAll<HTMLElement>("input, button:not([disabled]), select, textarea")];
  const keys = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
    if (e.key === "Enter" && !(e.target instanceof HTMLTextAreaElement) && !main.disabled) {
      e.preventDefault();
      main.click();
    }
    // Tab goes round inside the dialog.
    if (e.key === "Tab") {
      const all = focusable();
      const at = all.indexOf(document.activeElement as HTMLElement);
      const next = e.shiftKey ? (at <= 0 ? all.length - 1 : at - 1) : (at + 1) % all.length;
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
      problem.textContent = (e as Error).message;
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
    },
  };
}

/**
 * Asks before something that can't be undone. With `typeName`, the button
 * waits until that name is typed, as cloud consoles ask for a delete.
 */
export function confirmDanger(options: { title: string; warning: Child; typeName?: string; action: string; run(): Promise<void> }): void {
  const body: Child[] = [h("div", { class: "alert warn" }, h("span", null, options.warning))];
  let typed: HTMLInputElement | null = null;
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
