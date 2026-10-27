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

/** Morning brief (Rust: tsr_pipeline::brief::Brief). Lists are assembled by code. */
export type BriefFact = {
  id: number;
  kind: string;
  text: string;
  owner: string | null;
  counterparty: string | null;
  due_at: number | null;
  due_text: string | null;
  status: string;
};

export type Highlight = { text: string; item_ids: number[] };

export type Brief = {
  day: string;
  generated_at: number;
  opener: string;
  overdue: BriefFact[];
  due_today: BriefFact[];
  commitments_by_me: BriefFact[];
  commitments_to_me: BriefFact[];
  waiting_on: BriefFact[];
  proposed_done: BriefFact[];
  yesterday: { summary_id: number; text: string; highlights: (Highlight & { child_ids?: number[] })[]; stale: boolean } | null;
};

export type LoopGroups = { proposed_done: FactDto[]; open: FactDto[]; still_open: FactDto[] };

/** A rollup summary; `model` is "extractive" when code wrote it (no model). */
export type SummaryDto = { id: number; text: string; highlights: Highlight[]; stale: boolean; model: string };

export type PeriodDto = {
  level: "hour" | "day" | "week" | "month";
  start: number;
  end: number;
  label: string;
  summary: SummaryDto | null;
};

export type RollupView = { period: PeriodDto; children: PeriodDto[] };

/** `draft_target` event: the focused field when Ctrl+Alt+D was pressed. */
export type DraftTarget = { ok: true; app: string; thread_chars: number } | { ok: false; code: string };

/** A fact-check flag on a draft (Rust: tsr_draft::factcheck::Flag); offsets are characters. */
export type DraftFlag = {
  kind: "unsupported" | "new_commitment" | "suspicious" | "secret";
  claim?: string;
  start: number;
  end: number;
  text: string;
};

export type DraftDto = {
  text: string;
  flags: DraftFlag[];
  placeholders: string[];
  used: number[];
  first_text_ms: number | null;
  total_ms: number;
  token: string;
};

/** Ask (Rust: tsr_retrieval::ask). */
export type AskSnippet = {
  n: number;
  item_id: number;
  source_id: number;
  started_at: number;
  app: string;
  title: string | null;
  preview: string;
};
export type AnswerSentence = { text: string; cites: number[]; item_ids: number[]; verified: boolean; fabricated: number };
export type Answer = {
  sentences: AnswerSentence[];
  insufficient: boolean;
  not_found: boolean;
  fallback: number[];
  first_sentence_ms: number | null;
  total_ms: number;
};
export type AskStart = { snippets: AskSnippet[]; answering: boolean };
