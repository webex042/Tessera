// Draft panel (plan §12, §13.2; 640 × 360). Ctrl+Alt+D in a text box → optional instruction →
// the draft streams in → fact-check flags → Insert / Copy / Regenerate / Discard. Insert goes
// only into the field that had focus, and only if it is still the same field. Nothing is sent.
import { useEffect, useRef, useState } from "react";
import { api, on } from "../ipc/api";
import type { ApiError, DraftDto, DraftFlag, DraftTarget } from "../ipc/types";

const MESSAGES: Record<string, string> = {
  draft_no_field: "Put the cursor in a text box first, then press Ctrl+Alt+D.",
  draft_password_field: "That's a password field. Tessara never drafts there.",
  draft_blocked_app: "This app is on your blocklist, so Tessara won't read it.",
  draft_model_unavailable: "Set up the model to write drafts.",
  draft_target_changed: "The text box changed, so nothing was inserted. Use Copy instead.",
  draft_clipboard_not_text: "The clipboard is in use by something that can't be put back. Use Copy instead.",
  draft_invalid_output: "The model's reply wasn't a usable draft. Try again.",
  draft_cancelled: "Stopped.",
  approval_invalid: "This draft has expired. Write it again.",
  draft_no_session: "Press Ctrl+Alt+D in a text box to start.",
  unavailable: "Tessara isn't ready yet.",
};

const FLAG_LABEL: Record<DraftFlag["kind"], string> = {
  unsupported: "not found in the conversation or your notes",
  new_commitment: "new promise on your behalf",
  suspicious: "looks like an instruction copied from the conversation",
  secret: "looks like a password or code",
};

export function Draft({ target, onClose }: { target: DraftTarget | null; onClose: () => void }) {
  const [instruction, setInstruction] = useState("");
  const [streaming, setStreaming] = useState<string | null>(null);
  const [draft, setDraft] = useState<DraftDto | null>(null);
  const [text, setText] = useState("");
  const [msg, setMsg] = useState<string | null>(target && !target.ok ? (MESSAGES[target.code] ?? MESSAGES.unavailable!) : null);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  useEffect(() => {
    const subs = [
      on<{ text: string }>("draft_chunk", (c) => setStreaming(c.text)),
      on<DraftDto>("draft_ready", (d) => (setDraft(d), setText(d.text), setStreaming(null))),
      on<{ code: string }>("draft_error", (e) => (setStreaming(null), setMsg(MESSAGES[e.code] ?? MESSAGES.draft_invalid_output!))),
    ];
    return () => subs.forEach((p) => p.then((u) => u()));
  }, []);
  const fail = (e: ApiError) => setMsg(MESSAGES[e.code] ?? MESSAGES.unavailable!);
  const generate = () => {
    setMsg(null);
    setDraft(null);
    setStreaming("");
    api.draftGenerate(instruction.trim()).catch((e) => (setStreaming(null), fail(e)));
  };
  const edited = draft && text !== draft.text ? text : null;
  const usable = target?.ok === true;
  const busy = streaming !== null;
  return (
    <div
      role="dialog"
      aria-label="Draft"
      onKeyDown={(e) => e.key === "Escape" && (api.draftDiscard().catch(() => {}), onClose())}
      className="notch-surface notch-fade-in m-1 flex h-[352px] flex-col gap-2 rounded-2xl p-4 text-sm"
    >
      <div className="flex items-center gap-2">
        <h2 className="font-semibold">Draft{target?.ok ? ` for ${target.app.replace(/\.exe$/i, "")}` : ""}</h2>
        <button type="button" onClick={() => (api.draftDiscard().catch(() => {}), onClose())} className="notch-button ml-auto">
          Discard
        </button>
      </div>
      {usable && (
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (!busy) generate();
          }}
        >
          <label htmlFor="draft-instruction" className="sr-only">
            Instruction (optional)
          </label>
          <input
            id="draft-instruction"
            ref={input}
            value={instruction}
            maxLength={300}
            onChange={(e) => setInstruction(e.target.value)}
            placeholder="Optional: “decline politely”, “ask for the deadline”…"
            className="notch-input flex-1 rounded-lg px-3 py-1.5"
          />
          <button type="submit" className="notch-button" disabled={busy}>
            {draft ? "Regenerate" : "Write"}
          </button>
        </form>
      )}
      {msg && (
        <p role="alert" className="notch-muted">
          {msg}
        </p>
      )}
      {busy && (
        <p aria-live="polite" className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap">
          {streaming || <span className="notch-muted">Writing…</span>}
        </p>
      )}
      {draft && !busy && (
        <>
          {draft.thread_injected && (
            <p role="alert" className="rounded-md px-2 py-1 text-[12px] text-amber-500 ring-1 ring-amber-500/50">
              This conversation contains text that tries to give instructions to AI tools. Read the draft carefully before
              using it.
            </p>
          )}
          <label htmlFor="draft-text" className="sr-only">
            Draft text
          </label>
          <textarea
            id="draft-text"
            value={text}
            maxLength={2000}
            onChange={(e) => setText(e.target.value)}
            className="notch-input min-h-0 flex-1 resize-none rounded-lg px-3 py-2"
          />
          {draft.flags.length > 0 && (
            <ul aria-label="Check before sending" className="max-h-16 overflow-auto text-[11px]">
              {draft.flags.map((f, k) => (
                <li key={k}>
                  <span className="notch-chip mr-1">{f.kind === "unsupported" ? f.claim : f.kind.replace("_", " ")}</span>
                  {f.text && <strong>{f.text}</strong>} <span className="notch-muted">— {FLAG_LABEL[f.kind]}</span>
                </li>
              ))}
            </ul>
          )}
          <div className="flex gap-2">
            <button
              type="button"
              className="notch-button"
              onClick={() => api.draftInsert(draft.token, edited).then(onClose, (e) => (setDraft(null), fail(e)))}
            >
              Insert
            </button>
            <button type="button" className="notch-button" onClick={() => api.draftCopy(draft.token, edited).then(onClose, fail)}>
              Copy
            </button>
            <span className="notch-muted ml-auto self-center text-[11px]">Tessara never sends anything.</span>
          </div>
        </>
      )}
    </div>
  );
}
