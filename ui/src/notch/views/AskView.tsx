// Ask (plan §10.4, §13.3): question → sources at once → sentences stream in, each verified
// against the snippets it cites. Verified sentences carry citation chips; unverified ones are
// dimmed and labelled. If nothing verifies: "I couldn't find that in your memory" + top sources.
import { useEffect, useRef, useState } from "react";
import { api, on } from "../../ipc/api";
import type { Answer, AnswerSentence, ApiError, AskSnippet } from "../../ipc/types";
import { ItemPane } from "./SearchView";

export function AskView() {
  const [q, setQ] = useState("");
  const [snips, setSnips] = useState<AskSnippet[] | null>(null);
  const [answering, setAnswering] = useState(false);
  const [sentences, setSentences] = useState<AnswerSentence[]>([]);
  const [done, setDone] = useState<Answer | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [open, setOpen] = useState<number | null>(null);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  useEffect(() => {
    const subs = [
      on<AnswerSentence>("ask_sentence", (s) => setSentences((v) => [...v, s])),
      on<Answer>("ask_done", (a) => (setDone(a), setSentences(a.sentences), setAnswering(false))),
      on<{ code: string }>("ask_error", () => (setAnswering(false), setErr("The model's answer couldn't be used. The sources are below."))),
    ];
    return () => subs.forEach((p) => p.then((u) => u()));
  }, []);
  if (open !== null) return <ItemPane id={open} onBack={() => setOpen(null)} />;
  const submit = () => {
    if (!q.trim()) return;
    setErr(null);
    setSentences([]);
    setDone(null);
    api.ask(q.trim()).then(
      (r) => (setSnips(r.snippets), setAnswering(r.answering), !r.answering && r.snippets.length > 0 && setErr("Set up the model to get answers. Here are the closest sources.")),
      (e: ApiError) => setErr(e.code === "invalid_input" ? "Ask a shorter question." : "Couldn't search right now."),
    );
  };
  const byN = (n: number) => snips?.find((s) => s.n === n);
  const notFound = done?.not_found;
  return (
    <div>
      <form
        role="search"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <label htmlFor="ask" className="sr-only">
          Ask your memory
        </label>
        <input
          id="ask"
          ref={input}
          value={q}
          maxLength={300}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Ask, e.g. “What did I promise Ravi last week?”"
          className="notch-input w-full rounded-lg px-3 py-1.5"
        />
      </form>
      {err && (
        <p role="status" className="notch-muted mt-2 text-[12px]">
          {err}
        </p>
      )}
      {(sentences.length > 0 || answering) && !notFound && (
        <section aria-live="polite" className="mt-2">
          {sentences.map((s, k) => (
            <p key={k} className={s.verified ? "" : "opacity-50"}>
              {s.text}{" "}
              {s.verified ? (
                s.cites.map((c) => (
                  <button
                    key={c}
                    type="button"
                    className="notch-chip align-middle"
                    onClick={() => byN(c) && setOpen(byN(c)!.item_id)}
                    title={byN(c)?.preview}
                    aria-label={`Source ${c}: ${byN(c)?.app ?? ""}`}
                  >
                    {c}
                  </button>
                ))
              ) : (
                <span className="notch-chip">not verified</span>
              )}
            </p>
          ))}
          {answering && <p className="notch-muted">Answering…</p>}
        </section>
      )}
      {notFound && <p className="mt-2 font-semibold">I couldn't find that in your memory.</p>}
      {snips && snips.length === 0 && <p className="notch-muted mt-2">Nothing in your memory matches that.</p>}
      {snips && snips.length > 0 && (
        <section className="mt-3">
          <h3 className="notch-muted mb-1 text-[11px] font-semibold uppercase tracking-wide">Sources</h3>
          <ul>
            {(notFound ? snips.filter((s) => done!.fallback.includes(s.n)) : snips).map((s) => (
              <li key={s.n}>
                <button type="button" className="notch-row w-full rounded-lg px-2 py-1 text-left" onClick={() => setOpen(s.item_id)}>
                  <span className="notch-chip mr-1">{s.n}</span>
                  <span className="notch-muted text-[11px]">
                    {new Date(s.started_at).toLocaleString([], { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })} · {s.app}
                  </span>
                  <span className="block truncate text-[12px]">{s.title ?? s.preview}</span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
