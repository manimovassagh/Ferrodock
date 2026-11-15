// Form pieces, after the patterns of Cloudscape (AWS's open-source design
// system): a field with its label, hint, constraint and error; a text
// input; choices as radio tiles. Each piece is plain elements; the page
// reads and checks the values.

import { type Child, h } from "./dom";

/** A field: label, an optional hint above, the control, a constraint below, and room for an error. */
export interface Field {
  el: HTMLElement;
  /** Shows `message` under the control, marked red; null clears it. */
  error(message: string | null): void;
}

export function field(label: string, control: HTMLElement, options: { hint?: string; constraint?: string; optional?: boolean } = {}): Field {
  const error = h("div", { class: "field-error", hidden: true });
  const el = h(
    "div",
    { class: "field" },
    h("label", null, label, options.optional ? h("span", { class: "optional" }, " - optional") : null),
    options.hint ? h("div", { class: "field-hint" }, options.hint) : null,
    control,
    error,
    options.constraint ? h("div", { class: "field-constraint" }, options.constraint) : null,
  );
  return {
    el,
    error(message) {
      error.hidden = !message;
      error.textContent = message ? `⚠ ${message}` : "";
      control.classList.toggle("invalid", Boolean(message));
    },
  };
}

export function textInput(options: { value?: string; placeholder?: string; mono?: boolean } = {}): HTMLInputElement {
  return h("input", {
    class: `input${options.mono ? " mono" : ""}`,
    type: "text",
    value: options.value ?? "",
    placeholder: options.placeholder ?? "",
    autocomplete: "off",
    spellcheck: "false",
  });
}

/** One of a few choices, each a tile with a title and a line under it. */
export interface Tiles {
  el: HTMLElement;
  value(): string;
  onChange(listener: (value: string) => void): void;
}

export function radioTiles(name: string, choices: { value: string; title: string; about: Child }[], chosen: string): Tiles {
  let current = chosen;
  const listeners: ((value: string) => void)[] = [];
  const tiles = choices.map((choice) => {
    const input = h("input", { type: "radio", name, value: choice.value, checked: choice.value === chosen });
    const tile = h("label", { class: `tile-choice${choice.value === chosen ? " on" : ""}` }, input, h("span", null, h("b", null, choice.title), h("small", null, choice.about)));
    input.addEventListener("change", () => {
      current = choice.value;
      tiles.forEach((t) => t.classList.toggle("on", t === tile));
      listeners.forEach((listen) => listen(current));
    });
    return tile;
  });
  return {
    el: h("div", { class: "tiles" }, tiles),
    value: () => current,
    onChange: (listener) => listeners.push(listener),
  };
}
