// Development only: a stand-in for Tauri's IPC so the notch UI can be viewed in a normal
// browser (`pnpm dev`, then open http://localhost:5173). The real notch window is excluded from
// screen capture, so this is how the UI is looked at and screenshotted. All data is synthetic.
// Loaded from main.tsx only when `import.meta.env.DEV` and no Tauri runtime is present, so
// production builds do not contain it.
import type {
  AiSetupState,
  BlocklistRuleDto,
  Brief,
  FactDto,
  HotkeyInfo,
  HourBucket,
  ItemCard,
  LoopGroups,
  PeriodDto,
  PrivacyInfo,
  RollupView,
  Status,
  Today,
} from "../ipc/types";

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

let retentionDays = 30;
let aiProvider: "local" | "groq" = "local";
let cloudConsent = false;
let hasGroqKey = false;
let hasLocalModel = true;
let hasEmbedder = true;
let groqModel = "openai/gpt-oss-20b";
let blocklistRules: BlocklistRuleDto[] = [
  { key: "builtin.process.1password", kind: "process", scope: "all", pattern: "*1password*", is_regex: false, builtin: true, enabled: true, blocked_count: 14 },
  { key: "builtin.title.otp", kind: "title", scope: "all", pattern: "\\botp\\b|verification code", is_regex: true, builtin: true, enabled: true, blocked_count: 3 },
  { key: "builtin.url.banking", kind: "url", scope: "browser", pattern: "*bank*", is_regex: false, builtin: true, enabled: true, blocked_count: 6 },
];

const privacyInfo = (): PrivacyInfo => ({
  retention_days: retentionDays,
  ai_provider: aiProvider,
  groq_model: groqModel,
  cloud_consent: cloudConsent,
  has_groq_key: hasGroqKey,
  has_local_model: hasLocalModel,
  vector_records: items.length * 2,
  last_pruned_at: now - 3600_000,
});

const aiSetupState = (): AiSetupState => ({
  provider: aiProvider,
  runtime_state: aiProvider === "groq" ? (hasGroqKey && cloudConsent ? "ready" : "not_set_up") : hasLocalModel ? "ready" : "not_set_up",
  backend: aiProvider === "groq" ? (hasGroqKey && cloudConsent ? "groq" : null) : hasLocalModel ? "vulkan" : null,
  cloud_consent: cloudConsent,
  groq_model: groqModel,
  has_groq_key: hasGroqKey,
  groq_key_source: hasGroqKey ? "dpapi" : "none",
  has_local_model: hasLocalModel,
  has_embedder: hasEmbedder,
  groq_feature_supported: true,
  hardware: {
    verdict: "gpu",
    gpu_name: "Simulated Demo GPU",
    cpu_cores: 8,
    ram_total_mib: 16_384,
    ram_available_mib: 9_800,
    disk_free_mib: 84_000,
    local_model_bytes: 3_333_461_920,
    embedder_bytes: 487_352_503,
    warnings: [],
  },
  download: {
    active: false,
    target: null,
    phase: "idle",
    done_bytes: 0,
    total_bytes: 0,
    file: null,
    error: null,
  },
});

let paused = false;
const status = (): Status => ({
  paused,
  capturing: !paused,
  model: {
    state: aiProvider === "groq" ? (hasGroqKey && cloudConsent ? "ready" : "not_set_up") : hasLocalModel ? "ready" : "not_set_up",
    backend: aiProvider === "groq" ? (hasGroqKey && cloudConsent ? "groq" : null) : hasLocalModel ? "vulkan" : null,
  },
  queue: [0, 2, 5],
  counts: { open: facts.filter((f) => f.status === "open").length, due_today: 1, overdue: 1, needs_review: 1, items_today: items.length },
});

const today = (): Today => ({
  day_start: dayStart,
  overdue: [facts[1]!],
  due_today: [facts[0]!],
  open_commitments: [facts[0]!, facts[3]!],
  needs_review: [facts[4]!],
  timeline: buckets(),
});

const brief = (): Brief => ({
  day: new Date().toISOString().slice(0, 10),
  generated_at: now,
  opener: "Today: 1 item overdue, 1 item due today, 2 open commitments you made, 1 loop that may be done.",
  overdue: [],
  due_today: [],
  commitments_by_me: [],
  commitments_to_me: [],
  waiting_on: [],
  proposed_done: [],
  yesterday: {
    summary_id: 90,
    text: "Reviewed the Q4 budget with Ravi and drafted the offsite agenda. Lena asked about the venue.",
    highlights: [
      { text: "Budget review with Ravi", item_ids: [1] },
      { text: "Offsite venue question from Lena", item_ids: [2] },
    ],
    stale: false,
  },
});

facts.push(fact(6, { status: "proposed_done", text: "Send the signed SOW to Priya", counterparty: "Priya Rao", kind: "commitment" }));
facts.push(fact(7, { text: "Update the onboarding checklist", first_seen: now - 40 * 86_400_000, last_seen: now - 30 * 86_400_000 }));
facts.push(fact(8, { text: "Ask IT about the VPN token", first_seen: now - 35 * 86_400_000, last_seen: now - 25 * 86_400_000 }));

const loopGroups = (): LoopGroups => ({
  proposed_done: facts.filter((f) => f.status === "proposed_done"),
  open: facts.filter((f) => f.status === "open" && f.last_seen > now - 21 * 86_400_000 && f.kind !== "note"),
  still_open: facts.filter((f) => f.status === "open" && f.last_seen <= now - 21 * 86_400_000),
});

function rollups(level: string, at: number): RollupView {
  const day = 86_400_000;
  const start = new Date(new Date(at).setHours(0, 0, 0, 0));
  const monday = start.getTime() - ((start.getDay() + 6) % 7) * day;
  const summary = (id: number, text: string) => ({
    id,
    text,
    highlights: [{ text: "Budget review with Ravi", item_ids: [1, 2] }],
    stale: false,
    model: id % 2 ? "extractive" : "qwen3-vl-4b-instruct-q4_k_m",
  });
  const child = (k: number): PeriodDto =>
    level === "week"
      ? {
          level: "day",
          start: monday + k * day,
          end: monday + (k + 1) * day,
          label: new Date(monday + k * day).toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" }),
          summary: k < 5 ? summary(100 + k, `Day ${k + 1}: reviewed the budget, answered Lena, drafted slides.`) : null,
        }
      : {
          level: "week",
          start: monday - (3 - k) * 7 * day,
          end: monday - (2 - k) * 7 * day,
          label: `week of Monday ${new Date(monday - (3 - k) * 7 * day).toLocaleDateString([], { day: "numeric", month: "long" })}`,
          summary: summary(200 + k, `Week ${k + 1}: vendor contract, offsite planning and the Q4 budget.`),
        };
  return {
    period: { level: level as PeriodDto["level"], start: monday, end: monday + 7 * day, label: level, summary: summary(300, `This ${level}: mostly the Q4 budget and the offsite.`) },
    children: Array.from({ length: level === "week" ? 7 : 4 }, (_, k) => child(k)),
  };
}

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
    case "get_brief":
      return brief();
    case "get_loop_groups":
      return loopGroups();
    case "get_rollups":
      return rollups(String(args.level), Number(args.atMs));
    case "get_items":
      return items.filter((i) => (args.itemIds as number[]).includes(i.item_id));
    case "ask": {
      const snippets = items.slice(0, 2).map((it, k) => ({
        n: k + 1,
        item_id: it.item_id,
        source_id: it.item_id,
        started_at: it.started_at,
        app: it.app,
        title: it.title,
        preview: it.summary ?? "",
      }));
      const s1 = { text: "Ravi asked you for the revised budget sheet by 5pm.", cites: [1], item_ids: [1], verified: true, fabricated: 0 };
      const s2 = { text: "He also approved the travel budget.", cites: [1], item_ids: [1], verified: false, fabricated: 0 };
      setTimeout(() => emit("ask_sentence", s1), 400);
      setTimeout(() => emit("ask_sentence", s2), 800);
      setTimeout(
        () => emit("ask_done", { sentences: [s1, s2], insufficient: false, not_found: false, fallback: [], first_sentence_ms: 400, total_ms: 900 }),
        1000,
      );
      return { snippets, answering: true, vector_used: hasEmbedder };
    }
    case "delete_item": {
      const idx = items.findIndex((i) => i.item_id === args.itemId);
      if (idx >= 0) items.splice(idx, 1);
      return null;
    }
    case "propose_clear_memories":
      return "2".repeat(32);
    case "clear_memories": {
      const removed = items.length;
      items.length = 0;
      facts.length = 0;
      return { items_deleted: removed, sources_deleted: removed, facts_deleted: 8, vectors_deleted: removed * 2 };
    }
    case "get_privacy_settings":
      return privacyInfo();
    case "set_retention_days":
      retentionDays = Number(args.days ?? 0);
      return privacyInfo();
    case "list_blocklist_rules":
      return blocklistRules;
    case "set_blocklist_rule_enabled":
      blocklistRules = blocklistRules.map((r) => (r.key === args.key ? { ...r, enabled: Boolean(args.enabled) } : r));
      return blocklistRules;
    case "add_blocklist_rule": {
      const rule: BlocklistRuleDto = {
        key: `user.${blocklistRules.length + 1}`,
        kind: (args.kind as BlocklistRuleDto["kind"]) ?? "process",
        scope: (args.scope as BlocklistRuleDto["scope"]) ?? "all",
        pattern: String(args.pattern ?? ""),
        is_regex: Boolean(args.isRegex),
        builtin: false,
        enabled: true,
        blocked_count: 0,
      };
      blocklistRules = [...blocklistRules, rule];
      return blocklistRules;
    }
    case "remove_blocklist_rule":
      blocklistRules = blocklistRules.filter((r) => r.key !== args.key || r.builtin);
      return blocklistRules;
    case "get_ai_setup":
      return aiSetupState();
    case "set_ai_provider":
      if (args.cloudConsent !== null && args.cloudConsent !== undefined) cloudConsent = Boolean(args.cloudConsent);
      if (args.groqModel) groqModel = String(args.groqModel);
      aiProvider = args.provider === "groq" ? "groq" : "local";
      return aiSetupState();
    case "save_groq_config":
      if (!args.cloudConsent) throw { code: "cloud_consent_required", message: "error.cloud_consent_required" };
      if (args.apiKey) hasGroqKey = true;
      groqModel = String(args.model ?? "openai/gpt-oss-20b");
      cloudConsent = true;
      aiProvider = "groq";
      return aiSetupState();
    case "remove_groq_config":
      hasGroqKey = false;
      cloudConsent = false;
      aiProvider = "local";
      return aiSetupState();
    case "test_groq_connection":
      return { ok: true, latency_ms: 142, code: null };
    case "start_model_download":
      if (args.target === "local_llm") hasLocalModel = true;
      else hasEmbedder = true;
      return aiSetupState();
    case "cancel_model_download":
      return aiSetupState();
    case "get_diagnostics":
      return { format: "tessara-diagnostics/1", mode: "synthetic-demo", store: { items: items.length, facts: facts.length } };
    case "save_diagnostics":
      return "diagnostics-demo.json";
    case "draft_generate": {
      const full = "Hi Ravi, the revised budget sheet is attached (Q3_budget_v4.xlsx). I'll send the travel split by Friday 30 Oct.";
      for (let i = 1; i <= 4; i++) setTimeout(() => emit("draft_chunk", { text: full.slice(0, (full.length * i) / 4) }), i * 150);
      setTimeout(
        () =>
          emit("draft_ready", {
            text: full,
            flags: [
              { kind: "unsupported", claim: "file", start: 52, end: 67, text: "Q3_budget_v4.xlsx" },
              { kind: "new_commitment", start: 70, end: 112, text: "I'll send the travel split by Friday 30 Oct" },
            ],
            placeholders: [],
            used: [],
            first_text_ms: 150,
            total_ms: 700,
            token: "f".repeat(32),
          }),
        800,
      );
      return null;
    }
    case "draft_insert":
    case "draft_copy":
    case "draft_discard":
      return null;
    case "propose_bulk_status":
      return "1".repeat(32);
    case "set_bulk_status":
      for (const f of facts) if ((args.factIds as number[]).includes(f.id)) f.status = args.status as FactDto["status"];
      return (args.factIds as number[]).length;
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
