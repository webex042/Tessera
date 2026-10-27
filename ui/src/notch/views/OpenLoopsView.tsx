// Open loops (plan §11.3, §13.3): "May be done" (later evidence; confirm or not yet), open loops
// grouped by person, and "Still open?" (no evidence for 21 days) with dismiss-all. Nothing is
// closed without the user's confirmation: every change goes through a Rust approval token.
import { useState, type ReactNode } from "react";
import { api } from "../../ipc/api";
import type { ApiError, FactDto } from "../../ipc/types";
import { FactRow, Status, useLoad } from "./common";

const KINDS = [
  { id: "", label: "All" },
  { id: "commitment", label: "Commitments" },
  { id: "task", label: "Tasks" },
  { id: "question", label: "Questions" },
];

function byPerson(facts: FactDto[]): [string, FactDto[]][] {
  const groups = new Map<string, FactDto[]>();
  for (const f of facts) {
    // Group by the other person; facts only about the user go under "You".
    const who = f.counterparty ?? f.owner;
    const k = !who ? "No person" : who.toLowerCase() === "me" ? "You" : who;
    groups.set(k, [...(groups.get(k) ?? []), f]);
  }
  return [...groups.entries()];
}

/// "Dismiss all" for stale loops: one token for exactly these facts, confirmed by a second click.
function DismissAll({ facts, onDone }: { facts: FactDto[]; onDone: () => void }) {
  const [token, setToken] = useState<string | null>(null);
  const [err, setErr] = useState(false);
  const ids = facts.map((f) => f.id);
  if (token)
    return (
      <span className="flex gap-1">
        <button
          type="button"
          className="notch-button"
          onClick={() =>
            api.setBulkStatus(ids, "dropped", token).then(
              () => (setToken(null), onDone()),
              (_: ApiError) => (setToken(null), setErr(true)),
            )
          }
        >
          Confirm dismiss {ids.length}
        </button>
        <button type="button" className="notch-button" onClick={() => setToken(null)}>
          Cancel
        </button>
      </span>
    );
  return (
    <>
      <button
        type="button"
        className="notch-button"
        onClick={() => api.proposeBulkStatus(ids, "dropped").then(setToken, () => setErr(true))}
      >
        Dismiss all
      </button>
      {err && (
        <span role="alert" className="ml-2 text-[11px] text-red-500">
          Could not dismiss. Try again.
        </span>
      )}
    </>
  );
}

function Group({ title, facts, onChanged, extra }: { title: string; facts: FactDto[]; onChanged: () => void; extra?: ReactNode }) {
  if (facts.length === 0) return null;
  return (
    <section className="mb-3">
      <div className="mb-1 flex items-center gap-2">
        <h3 className="notch-muted text-[11px] font-semibold uppercase tracking-wide">{title}</h3>
        {extra}
      </div>
      <ul>
        {facts.map((f) => (
          <FactRow key={f.id} f={f} onChanged={onChanged} />
        ))}
      </ul>
    </section>
  );
}

export function OpenLoopsView() {
  const [kind, setKind] = useState("");
  const load = useLoad(() => api.loopGroups(), []);
  const keep = (fs: FactDto[]) => fs.filter((f) => !kind || f.kind === kind);
  const g = load.data;
  const total = g ? keep(g.proposed_done).length + keep(g.open).length + keep(g.still_open).length : 0;
  return (
    <>
      <div className="mb-2 flex gap-1" role="radiogroup" aria-label="Kind">
        {KINDS.map((k) => (
          <button
            key={k.id}
            type="button"
            role="radio"
            aria-checked={kind === k.id}
            onClick={() => setKind(k.id)}
            className="notch-tab rounded-lg px-2 py-0.5 text-xs"
          >
            {k.label}
          </button>
        ))}
      </div>
      <Status load={load} empty={total === 0}>
        {g && (
          <>
            <Group title="May be done" facts={keep(g.proposed_done)} onChanged={load.reload} />
            {byPerson(keep(g.open)).map(([person, facts]) => (
              <Group key={person} title={person} facts={facts} onChanged={load.reload} />
            ))}
            <Group
              title="Still open?"
              facts={keep(g.still_open)}
              onChanged={load.reload}
              extra={keep(g.still_open).length > 1 ? <DismissAll facts={keep(g.still_open)} onDone={load.reload} /> : undefined}
            />
          </>
        )}
      </Status>
    </>
  );
}
