// Today (plan §13.3): the morning brief, due/overdue, open commitments, loops that may be done,
// items to review, and today's timeline so far.
import { useState } from "react";
import { api } from "../../ipc/api";
import type { FactDto } from "../../ipc/types";
import { FactRow, Status, time, useLoad } from "./common";
import { HighlightItems, SummaryBlock } from "./Rollup";

function Section({ title, facts, onChanged }: { title: string; facts: FactDto[]; onChanged: () => void }) {
  if (facts.length === 0) return null;
  return (
    <section className="mb-3">
      <h3 className="notch-muted mb-1 text-[11px] font-semibold uppercase tracking-wide">{title}</h3>
      <ul>
        {facts.map((f) => (
          <FactRow key={f.id} f={f} onChanged={onChanged} />
        ))}
      </ul>
    </section>
  );
}

export function TodayView() {
  const [drill, setDrill] = useState<number[] | null>(null);
  const load = useLoad(() => Promise.all([api.today(), api.brief(), api.loopGroups()]), []);
  if (drill) return <HighlightItems ids={drill} onBack={() => setDrill(null)} />;
  const [t, b, g] = load.data ?? [null, null, null];
  const empty =
    !!t &&
    !t.overdue.length &&
    !t.due_today.length &&
    !t.open_commitments.length &&
    !t.needs_review.length &&
    !t.timeline.length &&
    !g?.proposed_done.length &&
    !b?.yesterday;
  return (
    <Status load={load} empty={empty}>
      {t && (
        <>
          {b && (
            <section className="mb-3" aria-label="Morning brief">
              <p className="font-semibold">{b.opener}</p>
              {b.yesterday && (
                <div className="mt-1">
                  <h3 className="notch-muted mb-0.5 text-[11px] font-semibold uppercase tracking-wide">Yesterday</h3>
                  <SummaryBlock
                    s={{
                      id: b.yesterday.summary_id,
                      text: b.yesterday.text,
                      highlights: b.yesterday.highlights,
                      stale: b.yesterday.stale,
                      model: "",
                    }}
                    onDrill={setDrill}
                  />
                </div>
              )}
            </section>
          )}
          <Section title="Overdue" facts={t.overdue} onChanged={load.reload} />
          <Section title="Due today" facts={t.due_today} onChanged={load.reload} />
          <Section title="May be done" facts={g?.proposed_done ?? []} onChanged={load.reload} />
          <Section title="Open commitments" facts={t.open_commitments} onChanged={load.reload} />
          <Section title="Needs your review" facts={t.needs_review} onChanged={load.reload} />
          {t.timeline.length > 0 && (
            <section>
              <h3 className="notch-muted mb-1 text-[11px] font-semibold uppercase tracking-wide">Today so far</h3>
              <ul>
                {t.timeline.map((h) => (
                  <li key={h.start} className="mb-1">
                    <span className="notch-muted mr-2 tabular-nums">{time(h.start)}</span>
                    {h.items.map((i) => i.app).filter((a, k, all) => all.indexOf(a) === k).join(", ")}
                    <span className="notch-muted"> · {h.items.length} session{h.items.length === 1 ? "" : "s"}</span>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </>
      )}
    </Status>
  );
}
