// Timeline day view (plan §13.3): hour cards for one day; each item opens its text.
// Week and month views come with rollups (Stage 4).
import { useState } from "react";
import { api } from "../../ipc/api";
import { Status, time, useLoad } from "./common";
import { ItemPane } from "./SearchView";

const DAY = 86_400_000;

export function TimelineView() {
  const [at, setAt] = useState(() => Date.now());
  const [open, setOpen] = useState<number | null>(null);
  const load = useLoad(() => api.timelineDay(at), [at]);
  if (open !== null) return <ItemPane id={open} onBack={() => setOpen(null)} />;
  return (
    <div>
      <div className="mb-2 flex items-center gap-2">
        <button type="button" className="notch-button" onClick={() => setAt(at - DAY)} aria-label="Previous day">
          ←
        </button>
        <span className="font-semibold">{new Date(at).toLocaleDateString([], { weekday: "long", day: "numeric", month: "short" })}</span>
        <button
          type="button"
          className="notch-button"
          onClick={() => setAt(Math.min(Date.now(), at + DAY))}
          aria-label="Next day"
          disabled={at + DAY > Date.now() + DAY / 2}
        >
          →
        </button>
      </div>
      <Status load={load} empty={(load.data ?? []).length === 0}>
        <ol>
          {(load.data ?? []).map((b) => (
            <li key={b.start} className="mb-2">
              <h3 className="notch-muted text-[11px] font-semibold tabular-nums">{time(b.start)}</h3>
              <ul>
                {b.items.map((i) => (
                  <li key={i.item_id}>
                    <button type="button" className="notch-row w-full rounded-lg px-2 py-1 text-left" onClick={() => setOpen(i.item_id)}>
                      <span className="tabular-nums">{time(i.started_at)}</span> {i.app}
                      {i.title ? <span className="notch-muted"> — {i.title}</span> : null}
                      {i.summary ? <span className="block text-[12px]">{i.summary}</span> : null}
                    </button>
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ol>
      </Status>
    </div>
  );
}
