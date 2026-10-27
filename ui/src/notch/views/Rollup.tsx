// Rollup pieces (plan §11.1, §13.3): a summary with its highlights, and drill-down from a
// highlight to the items it cites. Summaries written by code (no model) say so.
import { useState } from "react";
import { api } from "../../ipc/api";
import type { ItemCard, SummaryDto } from "../../ipc/types";
import { Status, time, useLoad } from "./common";
import { ItemPane } from "./SearchView";

/// The items behind a highlight; each opens its text.
export function HighlightItems({ ids, onBack }: { ids: number[]; onBack: () => void }) {
  const [open, setOpen] = useState<number | null>(null);
  const load = useLoad<ItemCard[]>(() => api.items(ids), [ids.join(",")]);
  if (open !== null) return <ItemPane id={open} onBack={() => setOpen(null)} />;
  return (
    <div>
      <button type="button" className="notch-button mb-2" onClick={onBack}>
        ← Back
      </button>
      <Status load={load} empty={(load.data ?? []).length === 0}>
        <ul>
          {(load.data ?? []).map((i) => (
            <li key={i.item_id}>
              <button type="button" className="notch-row w-full rounded-lg px-2 py-1 text-left" onClick={() => setOpen(i.item_id)}>
                <span className="notch-muted tabular-nums">
                  {new Date(i.started_at).toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })} {time(i.started_at)}
                </span>{" "}
                {i.app}
                {i.title ? <span className="notch-muted"> — {i.title}</span> : null}
                {i.summary ? <span className="block text-[12px]">{i.summary}</span> : null}
              </button>
            </li>
          ))}
        </ul>
      </Status>
    </div>
  );
}

/// A summary: text, then highlights that drill down to their items.
export function SummaryBlock({ s, onDrill }: { s: SummaryDto; onDrill: (ids: number[]) => void }) {
  return (
    <div>
      <p className="text-[13px] leading-snug">{s.text}</p>
      {(s.stale || s.model === "extractive") && (
        <p className="notch-muted text-[10px]">
          {s.stale ? "Updating after a deletion. " : ""}
          {s.model === "extractive" ? "Written from your activity without the model." : ""}
        </p>
      )}
      {s.highlights.length > 0 && (
        <ul className="mt-1">
          {s.highlights.map((h, k) => (
            <li key={k}>
              <button
                type="button"
                className="notch-row w-full rounded-md px-2 py-0.5 text-left text-[12px]"
                onClick={() => onDrill(h.item_ids)}
                aria-label={`${h.text} (${h.item_ids.length} source${h.item_ids.length === 1 ? "" : "s"})`}
              >
                • {h.text} <span className="notch-muted">({h.item_ids.length})</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
