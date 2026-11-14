// What the console knows of the emulator, refreshed in the background,
// and how it asks for more.

import type { Health, Inventory, RequestEvent, ServiceInventory } from "./types";

export const API = "/_ferrocloud/";

export const state = {
  health: null as Health | null,
  down: false,
  inventory: null as Inventory | null,
  requests: [] as RequestEvent[],
  /** The ID of the newest request seen. */
  last: 0,
  /** The text in the page's filter box. */
  filter: "",
  /** Per service, the listing shown. */
  tab: {} as Record<string, number>,
};

export async function get<T>(path: string): Promise<T> {
  const response = await fetch(API + path, { cache: "no-store" });
  if (!response.ok) throw new Error(`${path}: ${response.status}`);
  return (await response.json()) as T;
}

export function services(): ServiceInventory[] {
  return state.inventory?.services ?? [];
}

export function service(name: string): ServiceInventory | undefined {
  return services().find((s) => s.name === name);
}

/** Whether any of `fields` holds the filter. */
export function matches(fields: unknown[]): boolean {
  const needle = state.filter.trim().toLowerCase();
  return !needle || fields.some((f) => String(f).toLowerCase().includes(needle));
}
