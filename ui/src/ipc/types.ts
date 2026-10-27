// Types mirroring the Rust command results (src-tauri/src/views.rs, services.rs).
// Hand-written (plan §14 allows this when tauri-specta lags); keep in sync with the Rust structs.

export type ApiError = { code: string; message: string };

export type FactDto = {
  id: number;
  kind: "task" | "commitment" | "decision" | "event" | "question" | "note";
  text: string;
  status: "open" | "done" | "dropped" | "proposed_done" | "needs_review";
  owner: string | null;
  counterparty: string | null;
  due_text: string | null;
  due_at: number | null;
  due_precision: "datetime" | "date" | "week" | "month" | null;
  due_ambiguous: boolean;
  confidence: "low" | "medium" | "high";
  first_seen: number;
  last_seen: number;
  item_id: number | null;
};

export type ItemCard = {
  item_id: number;
  app: string;
  title: string | null;
  started_at: number;
  ended_at: number;
  summary: string | null;
};

export type HourBucket = { hour: number; start: number; items: ItemCard[] };

export type Today = {
  day_start: number;
  overdue: FactDto[];
  due_today: FactDto[];
  open_commitments: FactDto[];
  needs_review: FactDto[];
  timeline: HourBucket[];
};

export type SearchHit = { item: ItemCard; snippet: string };

export type ItemDetail = { card: ItemCard; text: string; url: string | null; facts: FactDto[] };

export type Counts = {
  open: number;
  due_today: number;
  overdue: number;
  needs_review: number;
  items_today: number;
};

export type Status = {
  paused: boolean;
  capturing: boolean;
  model: { state: string; backend: string | null };
  queue: [number, number, number];
  counts: Counts;
};

export type Panel = "pill" | "peek" | "expanded" | "draft" | "hidden";

/** `panel_request` event: which panel to show, and when the input reached Rust (Unix ms). */
export interface PanelRequest {
  panel: "expanded" | "pill" | "search" | "draft";
  at: number;
  /** Rust sets the window region and focus for this panel itself; the UI only animates. */
  placed: boolean;
}

/** A global hotkey: `chord` is what is registered (a fallback if `default` was taken), or null. */
export interface HotkeyInfo {
  action: "toggle" | "pause" | "ask" | "draft";
  default: string;
  chord: string | null;
}
