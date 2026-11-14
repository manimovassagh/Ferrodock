// What the emulator's own API (/_ferrocloud/) answers, and the shapes
// the console's pages share.

export interface Health {
  status: string;
  version: string;
  services: string[];
  account: string;
  region: string;
  console: string;
  /** When the emulator started, in milliseconds since 1970. */
  started: number;
}

/** One row of a listing: its cells, and the whole record behind it. */
export interface Item {
  cells: string[];
  // A record's fields differ per service; pages read the ones they know.
  detail: Record<string, any>;
}

/** One kind of thing a service holds, e.g. S3's buckets. */
export interface Listing {
  title: string;
  columns: string[];
  items: Item[];
}

export interface ServiceInventory {
  name: string;
  title: string;
  operations: number;
  listings: Listing[];
  error: string | null;
}

export interface Inventory {
  services: ServiceInventory[];
}

/** A request the emulator answered, as the log lists it. */
export interface RequestEvent {
  id: number;
  time: number;
  service: string;
  operation: string;
  resource: string;
  method: string;
  path: string;
  status: number;
  error: string;
  message: string;
  duration_ms: number;
  region: string;
  user_agent: string;
  request_id: string;
}

/** A body as the log keeps it. */
export interface Captured {
  text: string;
  size: number;
  truncated: boolean;
  binary: boolean;
  streamed: boolean;
}

export interface Detail {
  request_headers: [string, string][];
  request_body: Captured;
  response_headers: [string, string][];
  response_body: Captured;
}

/** Called when new data arrives, to bring a page up to date. */
export type Update = (() => void) | null;

/** A page: draws itself in `main`, and says how to update. */
export type Page = (main: HTMLElement, arg: string) => Update;

/**
 * What a service with more to show than its listings adds: `open`
 * replaces showing a row's record, `page` draws #/s/<service>/<path>,
 * `create` makes a new one (a bucket, a queue).
 */
export interface Browser {
  open?(item: Item): void;
  page?(main: HTMLElement, path: string): Update;
  create?(): void;
  /** What `create` makes, for its button: "bucket". */
  noun?: string;
}
