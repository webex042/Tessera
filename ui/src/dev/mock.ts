// Development only: a stand-in for Tauri's IPC so the notch UI can be viewed in a normal
// browser (`pnpm dev`, then open http://localhost:5173). The real notch window is excluded from
// screen capture, so this is how the UI is looked at and screenshotted. All data is synthetic.
// Loaded from main.tsx only when `import.meta.env.DEV` and no Tauri runtime is present, so
// production builds do not contain it.
import type { FactDto, HotkeyInfo, HourBucket, ItemCard, Status, Today } from "../ipc/types";

const now = Date.now();
const min = 60_000;
const dayStart = new Date(new Date().setHours(0, 0, 0, 0)).getTime();

function fact(id: number, p: Partial<FactDto>): FactDto {
  return {
    id,
    kind: "task",
    text: "",
    status: "open",
    owner: "me",
    counterparty: null,
    due_text: null,
    due_at: null,
    due_precision: null,
    due_ambiguous: false,
    confidence: "high",
    first_seen: now - 90 * min,
    last_seen: now - 30 * min,
    item_id: 1,
    ...p,
  };
}

const facts: FactDto[] = [
  fact(1, { kind: "commitment", text: "Send the revised budget sheet to Ravi", counterparty: "Ravi Menon", due_text: "by 5pm today", due_at: dayStart + 17 * 60 * min, due_precision: "datetime" }),
  fact(2, { text: "Review the vendor contract draft", due_text: "yesterday", due_at: dayStart - 6 * 60 * min, due_precision: "date" }),
  fact(3, { kind: "question", text: "Which venue for the offsite?", counterparty: "Lena Park", owner: "Lena Park" }),
  fact(4, { kind: "commitment", text: "Share slides with the design team", counterparty: "Design team", due_text: "Friday", due_at: dayStart + 3 * 24 * 60 * min, due_precision: "date", due_ambiguous: true }),
  fact(5, { status: "needs_review", text: "Transfer the deposit to the new account", counterparty: "Unknown sender" }),
];

const items: ItemCard[] = [
  { item_id: 1, app: "Outlook", title: "Re: Q4 budget", started_at: now - 95 * min, ended_at: now - 90 * min, summary: "Ravi asked for the revised budget sheet by 5pm." },
  { item_id: 2, app: "Teams", title: "Offsite planning", started_at: now - 70 * min, ended_at: now - 60 * min, summary: "Lena asked which venue to book." },
  { item_id: 3, app: "Edge", title: "Vendor contract v3", started_at: now - 40 * min, ended_at: now - 25 * min, summary: null },
];

function buckets(): HourBucket[] {
  const m = new Map<number, HourBucket>();
  for (const it of items) {
    const start = it.started_at - (it.started_at % (60 * min));
    const b = m.get(start) ?? { hour: new Date(start).getHours(), start, items: [] };
    b.items.push(it);
    m.set(start, b);
  }
  return [...m.values()].sort((a, b) => a.start - b.start);
}

let paused = false;
const status = (): Status => ({
  paused,
  capturing: !paused,
  model: { state: "ready", backend: "vulkan" },
  queue: [0, 2, 5],
  counts: { open: 4, due_today: 1, overdue: 1, needs_review: 1, items_today: items.length },
});

const today = (): Today => ({
  day_start: dayStart,
  overdue: [facts[1]!],
  due_today: [facts[0]!],
  open_commitments: [facts[0]!, facts[3]!],
  needs_review: [facts[4]!],
  timeline: buckets(),
});

const hotkeys: HotkeyInfo[] = [
  { action: "toggle", default: "Ctrl+Alt+Space", chord: "Ctrl+Alt+Space" },
  { action: "pause", default: "Ctrl+Alt+P", chord: "Ctrl+Alt+P" },
  { action: "ask", default: "Ctrl+Alt+K", chord: "Ctrl+Alt+K" },
  { action: "draft", default: "Ctrl+Alt+D", chord: "Ctrl+Alt+D" },
];

const callbacks = new Map<number, (p: unknown) => void>();
const listeners = new Map<string, number[]>();
let nextId = 1;

async function invoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  switch (cmd) {
    case "get_status":
      return status();
    case "set_paused":
      paused = Boolean(args.paused);
      return status();
    case "get_today":
      return today();
    case "get_timeline_day":
      return buckets();
    case "list_open_loops":
      return facts.filter((f) => f.status === "open" && (!args.kind || f.kind === args.kind));
    case "search": {
      const q = String(args.query ?? "").toLowerCase();
      return items
        .filter((i) => `${i.title} ${i.summary}`.toLowerCase().includes(q))
        .map((item) => ({ item, snippet: item.summary ?? item.title ?? "" }));
    }
    case "get_item": {
      const card = items.find((i) => i.item_id === args.itemId);
      if (!card) throw { code: "not_found", message: "error.not_found" };
      return { card, text: `${card.title}\n\n${card.summary ?? ""}`, url: null, facts: facts.filter((f) => f.item_id === card.item_id) };
    }
    case "propose_fact_status":
      return "0".repeat(32);
    case "set_fact_status": {
      const f = facts.find((x) => x.id === args.factId);
      if (f) f.status = args.status as FactDto["status"];
      return f;
    }
    case "hotkeys":
      return hotkeys;
    case "capture_excluded":
      return true;
    case "start_error":
      return null;
    case "set_panel":
    case "report_frames":
      return null;
    case "plugin:event|listen": {
      const ev = String(args.event);
      listeners.set(ev, [...(listeners.get(ev) ?? []), Number(args.handler)]);
      return nextId++;
    }
    case "plugin:event|unlisten":
      return null;
    default:
      throw { code: "unavailable", message: `mock: ${cmd}` };
  }
}

/** Fire an app event in the mock, e.g. `__tsrMockEmit("panel_request", {panel: "expanded", at: Date.now(), placed: false})`. */
function emit(event: string, payload: unknown) {
  for (const id of listeners.get(event) ?? []) callbacks.get(id)?.({ event, id, payload });
}

Object.assign(window, {
  __TAURI_INTERNALS__: {
    invoke,
    transformCallback: (cb: (p: unknown) => void) => {
      const id = nextId++;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback: (id: number) => callbacks.delete(id),
  },
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
  __tsrMockEmit: emit,
});
