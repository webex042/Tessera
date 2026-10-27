// Diagnostics (plan Stage 9): the exact content-free bundle, shown before it is saved. Tessara
// never sends it; the tester decides whether to share the saved file.
import { useState } from "react";
import { api } from "../../ipc/api";
import { Status, useLoad } from "./common";

export function DiagnosticsView({ onBack }: { onBack: () => void }) {
  const load = useLoad(() => api.diagnostics(), []);
  const [saved, setSaved] = useState<string | null>(null);
  return (
    <div>
      <div className="mb-2 flex items-center gap-2">
        <button type="button" className="notch-button" onClick={onBack}>
          ← Back
        </button>
        <h3 className="font-semibold">Diagnostics</h3>
        <button
          type="button"
          className="notch-button ml-auto"
          onClick={() => api.saveDiagnostics().then(setSaved, () => setSaved(null))}
          disabled={!load.data}
        >
          Save to file
        </button>
      </div>
      <p className="notch-muted mb-2 text-[12px]">
        Counts, states, timings and versions only — no text, titles, names or apps. Nothing is sent: if you save it,
        you decide whether to share the file.
      </p>
      {saved && (
        <p role="status" className="mb-2 text-[12px]">
          Saved as {saved} in Tessara's data folder.
        </p>
      )}
      <Status load={load} empty={false}>
        <pre className="notch-input max-h-72 overflow-auto rounded-lg p-2 text-[11px]">{JSON.stringify(load.data, null, 2)}</pre>
      </Status>
    </div>
  );
}
