// Open loops (plan §13.3): grouped by person; nothing is closed without the user's confirmation.
import { useState } from "react";
import { api } from "../../ipc/api";
import type { FactDto } from "../../ipc/types";
import { FactRow, Status, useLoad } from "./common";

const KINDS = [
  { id: "", label: "All" },
  { id: "commitment", label: "Commitments" },
  { id: "task", label: "Tasks" },
  { id: "question", label: "Questions" },
];

export function OpenLoopsView() {
  const [kind, setKind] = useState("");
  const load = useLoad(() => api.openLoops(kind || undefined), [kind]);
  const groups = new Map<string, FactDto[]>();
  for (const f of load.data ?? []) {
    // Group by the other person; facts only about the user go under "You".
    const who = f.counterparty ?? f.owner;
    const k = !who ? "No person" : who.toLowerCase() === "me" ? "You" : who;
    groups.set(k, [...(groups.get(k) ?? []), f]);
  }
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
      <Status load={load} empty={(load.data ?? []).length === 0}>
        {[...groups.entries()].map(([person, facts]) => (
          <section key={person} className="mb-3">
            <h3 className="notch-muted mb-1 text-[11px] font-semibold uppercase tracking-wide">{person}</h3>
            <ul>
              {facts.map((f) => (
                <FactRow key={f.id} f={f} onChanged={load.reload} />
              ))}
            </ul>
          </section>
        ))}
      </Status>
    </>
  );
}
