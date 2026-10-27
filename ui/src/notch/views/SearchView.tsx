// Search (plan §13.3): keyword search over memory with an app filter; results open the item.
import { useEffect, useRef, useState } from "react";
import { api } from "../../ipc/api";
import type { ItemDetail, SearchHit } from "../../ipc/types";
import { Status, time, useLoad, type Load } from "./common";

export function ItemPane({ id, onBack }: { id: number; onBack: () => void }) {
  const load = useLoad<ItemDetail>(() => api.item(id), [id]);
  const d = load.data;
  return (
    <div>
      <button type="button" className="notch-button mb-2" onClick={onBack}>
        ← Back
      </button>
      <Status load={load} empty={false}>
        {d && (
          <article>
            <h3 className="font-semibold">
              {d.card.app}
              {d.card.title ? ` — ${d.card.title}` : ""}
            </h3>
            <p className="notch-muted text-[11px]">
              {new Date(d.card.started_at).toLocaleString()} · {d.facts.length} fact{d.facts.length === 1 ? "" : "s"}
            </p>
            <pre className="mt-2 whitespace-pre-wrap break-words font-sans text-[12px]">{d.text}</pre>
          </article>
        )}
      </Status>
    </div>
  );
}

export function SearchView() {
  const [q, setQ] = useState("");
  const [submitted, setSubmitted] = useState("");
  const [open, setOpen] = useState<number | null>(null);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  const load: Load<SearchHit[]> = useLoad(
    () => (submitted ? api.search(submitted) : Promise.resolve([] as SearchHit[])),
    [submitted],
  );
  if (open !== null) return <ItemPane id={open} onBack={() => setOpen(null)} />;
  return (
    <div>
      <form
        role="search"
        onSubmit={(e) => {
          e.preventDefault();
          setSubmitted(q.trim());
        }}
      >
        <label htmlFor="q" className="sr-only">
          Search memory
        </label>
        <input
          id="q"
          ref={input}
          value={q}
          maxLength={200}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Search what you saw…"
          className="notch-input w-full rounded-lg px-3 py-1.5"
        />
      </form>
      {submitted && (
        <Status load={load as Load<unknown>} empty={(load.data ?? []).length === 0}>
          <ul className="mt-2">
            {(load.data ?? []).map((h) => (
              <li key={h.item.item_id}>
                <button type="button" className="notch-row w-full rounded-lg px-2 py-1.5 text-left" onClick={() => setOpen(h.item.item_id)}>
                  <span className="notch-muted text-[11px]">
                    {h.item.app} · {time(h.item.started_at)}
                  </span>
                  <span className="block break-words">{h.snippet}</span>
                </button>
              </li>
            ))}
          </ul>
        </Status>
      )}
    </div>
  );
}
