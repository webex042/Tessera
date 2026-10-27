// Timeline (plan §13.3): Day (hours and items), Week (days with summaries) and Month (weeks).
// Drill-down: month → week → day → items, and any highlight → the items it cites.
import { useState } from "react";
import { api } from "../../ipc/api";
import type { PeriodDto } from "../../ipc/types";
import { Status, time, useLoad } from "./common";
import { HighlightItems, SummaryBlock } from "./Rollup";
import { ItemPane } from "./SearchView";

type Mode = "day" | "week" | "month";
const DAY = 86_400_000;
const STEP: Record<Mode, number> = { day: DAY, week: 7 * DAY, month: 31 * DAY };

/// Local Monday 00:00 of the week containing `ms` (weeks start on Monday, as in Rust).
function monday(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  return d.getTime();
}

function DayList({ at, onOpen }: { at: number; onOpen: (id: number) => void }) {
  const load = useLoad(() => api.timelineDay(at), [at]);
  return (
    <Status load={load} empty={(load.data ?? []).length === 0}>
      <ol>
        {(load.data ?? []).map((b) => (
          <li key={b.start} className="mb-2">
            <h3 className="notch-muted text-[11px] font-semibold tabular-nums">{time(b.start)}</h3>
            <ul>
              {b.items.map((i) => (
                <li key={i.item_id}>
                  <button type="button" className="notch-row w-full rounded-lg px-2 py-1 text-left" onClick={() => onOpen(i.item_id)}>
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
  );
}

function PeriodList({ level, at, onDrill, onChild }: { level: "week" | "month"; at: number; onDrill: (ids: number[]) => void; onChild: (p: PeriodDto) => void }) {
  const load = useLoad(() => api.rollups(level, at), [level, at]);
  const v = load.data;
  const active = (v?.children ?? []).filter((c) => c.summary);
  return (
    <Status load={load} empty={!v?.period.summary && active.length === 0}>
      {v && (
        <>
          {v.period.summary && (
            <section className="mb-3">
              <h3 className="notch-muted mb-1 text-[11px] font-semibold uppercase tracking-wide">This {level}</h3>
              <SummaryBlock s={v.period.summary} onDrill={onDrill} />
            </section>
          )}
          <ul>
            {active.map((c) => (
              <li key={c.start} className="mb-2">
                <button type="button" className="notch-row w-full rounded-lg px-2 py-0.5 text-left font-semibold" onClick={() => onChild(c)}>
                  {c.label}
                </button>
                {c.summary && (
                  <div className="pl-2">
                    <SummaryBlock s={c.summary} onDrill={onDrill} />
                  </div>
                )}
              </li>
            ))}
          </ul>
        </>
      )}
    </Status>
  );
}

export function TimelineView() {
  const [mode, setMode] = useState<Mode>("day");
  const [at, setAt] = useState(() => Date.now());
  const [open, setOpen] = useState<number | null>(null);
  const [drill, setDrill] = useState<number[] | null>(null);
  if (open !== null) return <ItemPane id={open} onBack={() => setOpen(null)} />;
  if (drill) return <HighlightItems ids={drill} onBack={() => setDrill(null)} />;
  const label =
    mode === "day"
      ? new Date(at).toLocaleDateString([], { weekday: "long", day: "numeric", month: "short" })
      : mode === "week"
        ? `Week of ${new Date(monday(at)).toLocaleDateString([], { day: "numeric", month: "short" })}`
        : new Date(at).toLocaleDateString([], { month: "long", year: "numeric" });
  return (
    <div>
      <div className="mb-2 flex items-center gap-2">
        <div role="radiogroup" aria-label="Range" className="flex gap-1">
          {(["day", "week", "month"] as Mode[]).map((m) => (
            <button key={m} type="button" role="radio" aria-checked={mode === m} className="notch-tab rounded-lg px-2 py-0.5 text-xs" onClick={() => setMode(m)}>
              {m[0]!.toUpperCase() + m.slice(1)}
            </button>
          ))}
        </div>
        <button type="button" className="notch-button" onClick={() => setAt(at - STEP[mode])} aria-label={`Previous ${mode}`}>
          ←
        </button>
        <span className="font-semibold">{label}</span>
        <button
          type="button"
          className="notch-button"
          onClick={() => setAt(Math.min(Date.now(), at + STEP[mode]))}
          aria-label={`Next ${mode}`}
          disabled={at + DAY / 2 > Date.now()}
        >
          →
        </button>
      </div>
      {mode === "day" ? (
        <DayList at={at} onOpen={setOpen} />
      ) : (
        <PeriodList
          level={mode}
          at={at}
          onDrill={setDrill}
          onChild={(c) => {
            setAt(c.start + 1);
            setMode(c.level === "week" ? "week" : "day");
          }}
        />
      )}
    </div>
  );
}
