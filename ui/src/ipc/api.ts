// Typed wrappers over Tauri IPC. Errors arrive as { code, message } (content-free); anything
// else is normalised to an "ipc_error" code so the UI never shows raw text from Rust.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ApiError,
  AskStart,
  Brief,
  FactDto,
  HotkeyInfo,
  HourBucket,
  ItemCard,
  ItemDetail,
  LoopGroups,
  Panel,
  RollupView,
  SearchHit,
  Status,
  Today,
} from "./types";

function asApiError(e: unknown): ApiError {
  if (e && typeof e === "object" && "code" in e && typeof (e as ApiError).code === "string") {
    return { code: (e as ApiError).code, message: String((e as ApiError).message ?? "") };
  }
  return { code: "ipc_error", message: "error.ipc" };
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw asApiError(e);
  }
}

export const api = {
  status: () => call<Status>("get_status"),
  setPaused: (paused: boolean) => call<Status>("set_paused", { paused }),
  today: () => call<Today>("get_today"),
  timelineDay: (atMs: number) => call<HourBucket[]>("get_timeline_day", { atMs }),
  openLoops: (kind?: string) => call<FactDto[]>("list_open_loops", { kind: kind ?? null }),
  search: (query: string, appFilter?: string, limit = 30) =>
    call<SearchHit[]>("search", { query, appFilter: appFilter ?? null, limit }),
  item: (itemId: number) => call<ItemDetail>("get_item", { itemId }),
  proposeFactStatus: (factId: number, status: string) => call<string>("propose_fact_status", { factId, status }),
  setFactStatus: (factId: number, status: string, token: string) =>
    call<FactDto>("set_fact_status", { factId, status, token }),
  setPanel: (panel: Panel) => call<void>("set_panel", { panel }),
  brief: () => call<Brief>("get_brief"),
  loopGroups: () => call<LoopGroups>("get_loop_groups"),
  rollups: (level: "day" | "week" | "month", atMs: number) => call<RollupView>("get_rollups", { level, atMs }),
  items: (itemIds: number[]) => call<ItemCard[]>("get_items", { itemIds }),
  proposeBulkStatus: (factIds: number[], status: string) => call<string>("propose_bulk_status", { factIds, status }),
  setBulkStatus: (factIds: number[], status: string, token: string) =>
    call<number>("set_bulk_status", { factIds, status, token }),
  ask: (question: string) => call<AskStart>("ask", { question }),
  draftGenerate: (instruction: string) => call<void>("draft_generate", { instruction }),
  draftInsert: (token: string, edited: string | null) => call<void>("draft_insert", { token, edited }),
  draftCopy: (token: string, edited: string | null) => call<void>("draft_copy", { token, edited }),
  draftDiscard: () => call<void>("draft_discard"),
  startError: () => call<string | null>("start_error"),
  captureExcluded: () => call<boolean>("capture_excluded"),
  hotkeys: () => call<HotkeyInfo[]>("hotkeys"),
  reportFrames: (label: string, frameMs: number[]) => call<void>("report_frames", { label, frameMs }),
};

export function on<T>(event: string, f: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => f(e.payload));
}
