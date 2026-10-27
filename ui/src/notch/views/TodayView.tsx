import { api } from "../../ipc/api";
import type { FactDto } from "../../ipc/types";
import { FactRow, Status, time, useLoad } from "./common";

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
  const load = useLoad(() => api.today(), []);
  const t = load.data;
  const empty = !!t && !t.overdue.length && !t.due_today.length && !t.open_commitments.length && !t.needs_review.length && !t.timeline.length;
  return (
    <Status load={load} empty={empty}>
      {t && (
        <>
          <Section title="Overdue" facts={t.overdue} onChanged={load.reload} />
          <Section title="Due today" facts={t.due_today} onChanged={load.reload} />
          <Section title="Open commitments" facts={t.open_commitments} onChanged={load.reload} />
          <Section title="Needs your review" facts={t.needs_review} onChanged={load.reload} />
          {t.timeline.length > 0 && (
            <section>
              <h3 className="notch-muted mb-1 text-[11px] font-semibold uppercase tracking-wide">Today so far</h3>
              <ul>
                {t.timeline.map((b) => (
                  <li key={b.start} className="mb-1">
                    <span className="notch-muted mr-2 tabular-nums">{time(b.start)}</span>
                    {b.items.map((i) => i.app).filter((a, k, all) => all.indexOf(a) === k).join(", ")}
                    <span className="notch-muted"> · {b.items.length} session{b.items.length === 1 ? "" : "s"}</span>
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
