// Privacy & data controls (Stage 2): retention pruning, local/cloud transparency, blocklist editor,
// and token-confirmed memory wipe that keeps settings, blocklist rules, and DPAPI keys intact.
import { useState } from "react";
import { api } from "../../ipc/api";
import type { BlocklistRuleDto, ClearReport, PrivacyInfo } from "../../ipc/types";
import { Status, useLoad } from "./common";

const RETENTION_PRESETS = [
  { label: "Forever", days: 0 },
  { label: "7 days", days: 7 },
  { label: "30 days", days: 30 },
  { label: "90 days", days: 90 },
  { label: "1 year", days: 365 },
];

export function PrivacyView({ onBack, onOpenSetup }: { onBack: () => void; onOpenSetup: () => void }) {
  const privacyLoad = useLoad<PrivacyInfo>(() => api.privacySettings(), []);
  const rulesLoad = useLoad<BlocklistRuleDto[]>(() => api.blocklistRules(), []);

  const [retMsg, setRetMsg] = useState<string | null>(null);
  const [kind, setKind] = useState<"process" | "title" | "url">("process");
  const [scope, setScope] = useState<"all" | "browser">("all");
  const [pattern, setPattern] = useState("");
  const [isRegex, setIsRegex] = useState(false);
  const [ruleErr, setRuleErr] = useState<string | null>(null);

  const [clearToken, setClearToken] = useState<string | null>(null);
  const [confirmWord, setConfirmWord] = useState("");
  const [clearReport, setClearReport] = useState<ClearReport | null>(null);
  const [clearErr, setClearErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const p = privacyLoad.data;
  const rules = rulesLoad.data ?? [];

  const updateRetention = (days: number) => {
    setRetMsg(null);
    api.setRetentionDays(days).then(
      () => {
        setRetMsg(days === 0 ? "Keeping memories indefinitely." : `Auto-pruning memories older than ${days} days.`);
        privacyLoad.reload();
      },
      () => setRetMsg("Could not update retention policy."),
    );
  };

  const toggleRule = (r: BlocklistRuleDto) => {
    setRuleErr(null);
    api.setBlocklistRuleEnabled(r.key, !r.enabled).then(rulesLoad.reload, () => setRuleErr("Could not update rule."));
  };

  const addRule = () => {
    const clean = pattern.trim();
    if (!clean) return;
    setRuleErr(null);
    api.addBlocklistRule(kind, scope, clean, isRegex).then(
      () => {
        setPattern("");
        setIsRegex(false);
        rulesLoad.reload();
      },
      () => setRuleErr("Rule rejected (check pattern or regex syntax)."),
    );
  };

  const removeRule = (key: string) => {
    setRuleErr(null);
    api.removeBlocklistRule(key).then(rulesLoad.reload, () => setRuleErr("Could not remove user rule."));
  };

  const startClear = () => {
    setClearErr(null);
    setClearReport(null);
    setConfirmWord("");
    api.proposeClearMemories().then(setClearToken, () => setClearErr("Could not start memory wipe."));
  };

  const executeClear = () => {
    if (!clearToken || confirmWord.trim().toUpperCase() !== "CLEAR") return;
    setBusy(true);
    setClearErr(null);
    api.clearMemories(clearToken).then(
      (rep) => {
        setBusy(false);
        setClearToken(null);
        setConfirmWord("");
        setClearReport(rep);
        privacyLoad.reload();
      },
      () => {
        setBusy(false);
        setClearErr("Confirmation expired or failed. Please try again.");
        setClearToken(null);
      },
    );
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <button type="button" className="notch-button" onClick={onBack}>
          ← Back
        </button>
        <h3 className="font-semibold">Privacy & Data Controls</h3>
        <button type="button" className="notch-button ml-auto" onClick={onOpenSetup}>
          AI Setup
        </button>
      </div>

      <Status load={privacyLoad} empty={false}>
        {p && (
          <>
            <section className="rounded-lg border border-[var(--notch-line)] p-2.5">
              <div className="flex items-center justify-between gap-2">
                <div>
                  <h4 className="text-[12px] font-semibold">Processing & Storage Boundary</h4>
                  <p className="notch-muted text-[11px]">
                    {p.ai_provider === "groq" && p.cloud_consent
                      ? `Cloud BYOK active (Groq: ${p.groq_model}). Full PII masking runs before outbound text calls.`
                      : "Local-only processing. Captured data stays in encrypted SQLCipher + AES-256-GCM VectorDb."}
                  </p>
                </div>
                <span className="notch-chip whitespace-nowrap">{p.vector_records} vector chunks</span>
              </div>
            </section>

            <section className="rounded-lg border border-[var(--notch-line)] p-2.5">
              <h4 className="text-[12px] font-semibold">Automatic Retention Pruning</h4>
              <p className="notch-muted mb-2 text-[11px]">
                Memories older than your retention window are purged across SQLCipher, FTS5, and encrypted VectorDb.
              </p>
              <div className="flex flex-wrap gap-1.5">
                {RETENTION_PRESETS.map((opt) => (
                  <button
                    key={opt.days}
                    type="button"
                    aria-pressed={p.retention_days === opt.days}
                    className={`notch-button ${p.retention_days === opt.days ? "font-semibold underline" : ""}`}
                    onClick={() => updateRetention(opt.days)}
                  >
                    {opt.label}
                  </button>
                ))}
              </div>
              {retMsg && <p className="notch-muted mt-1.5 text-[11px]">{retMsg}</p>}
            </section>
          </>
        )}
      </Status>

      <section className="rounded-lg border border-[var(--notch-line)] p-2.5">
        <h4 className="text-[12px] font-semibold">Capture Blocklist ({rules.filter((r) => r.enabled).length}/{rules.length} active)</h4>
        <p className="notch-muted mb-2 text-[11px]">
          Windows matching enabled rules are skipped before OCR or UIA storage. Built-in defaults can be toggled; custom rules can be deleted.
        </p>
        <div className="mb-2 flex flex-wrap items-center gap-1.5 text-[12px]">
          <select
            aria-label="Rule kind"
            value={kind}
            onChange={(e) => setKind(e.target.value as "process" | "title" | "url")}
            className="notch-input rounded px-1.5 py-0.5 text-[11px]"
          >
            <option value="process">Process</option>
            <option value="title">Window title</option>
            <option value="url">URL</option>
          </select>
          <select
            aria-label="Rule scope"
            value={scope}
            onChange={(e) => setScope(e.target.value as "all" | "browser")}
            className="notch-input rounded px-1.5 py-0.5 text-[11px]"
          >
            <option value="all">All apps</option>
            <option value="browser">Browsers</option>
          </select>
          <input
            value={pattern}
            onChange={(e) => setPattern(e.target.value)}
            placeholder="Pattern (e.g. bank.exe or invoice)"
            maxLength={120}
            className="notch-input min-w-[180px] flex-1 rounded px-2 py-0.5 text-[11px]"
          />
          {kind === "title" && (
            <label className="flex items-center gap-1 text-[11px]">
              <input type="checkbox" checked={isRegex} onChange={(e) => setIsRegex(e.target.checked)} />
              Regex
            </label>
          )}
          <button type="button" className="notch-button text-[11px]" onClick={addRule}>
            Add rule
          </button>
        </div>
        {ruleErr && <p className="mb-1 text-[11px]">{ruleErr}</p>}
        <Status load={rulesLoad} empty={rules.length === 0}>
          <ul className="max-h-36 space-y-1 overflow-y-auto pr-1 text-[11px]">
            {rules.map((r) => (
              <li key={r.key} className="notch-row flex items-center gap-2 rounded px-1.5 py-0.5">
                <input
                  type="checkbox"
                  aria-label={`Toggle ${r.pattern}`}
                  checked={r.enabled}
                  onChange={() => toggleRule(r)}
                />
                <span className="notch-chip">{r.kind}</span>
                <span className="min-w-0 flex-1 truncate">{r.pattern}</span>
                {r.blocked_count > 0 && <span className="notch-muted">{r.blocked_count} blocked</span>}
                {!r.builtin && (
                  <button type="button" className="notch-button text-[10px]" onClick={() => removeRule(r.key)}>
                    Remove
                  </button>
                )}
              </li>
            ))}
          </ul>
        </Status>
      </section>

      <section className="rounded-lg border border-[var(--notch-line)] p-2.5">
        <div className="flex items-center justify-between gap-2">
          <div>
            <h4 className="text-[12px] font-semibold">Clear All Memories</h4>
            <p className="notch-muted text-[11px]">
              Deletes captured items, facts, summaries, people, and encrypted vectors while keeping settings, blocklist, and API keys.
            </p>
          </div>
          {!clearToken && (
            <button type="button" className="notch-button whitespace-nowrap" onClick={startClear}>
              Clear all memories
            </button>
          )}
        </div>
        {clearToken && (
          <div className="mt-2 flex flex-wrap items-center gap-2 text-[11px]">
            <span>Type CLEAR to confirm permanent deletion:</span>
            <input
              value={confirmWord}
              onChange={(e) => setConfirmWord(e.target.value)}
              placeholder="CLEAR"
              className="notch-input w-24 rounded px-2 py-0.5"
            />
            <button
              type="button"
              className="notch-button"
              disabled={busy || confirmWord.trim().toUpperCase() !== "CLEAR"}
              onClick={executeClear}
            >
              {busy ? "Wiping…" : "Confirm wipe"}
            </button>
            <button type="button" className="notch-button" disabled={busy} onClick={() => setClearToken(null)}>
              Cancel
            </button>
          </div>
        )}
        {clearErr && <p className="mt-1 text-[11px]">{clearErr}</p>}
        {clearReport && (
          <p role="status" className="notch-muted mt-1 text-[11px]">
            Cleared {clearReport.items_deleted} items, {clearReport.facts_deleted} facts, and {clearReport.vectors_deleted} vector chunks.
          </p>
        )}
      </section>
    </div>
  );
}
