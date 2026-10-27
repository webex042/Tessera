// Shared view pieces: async loading with error/empty states, fact rows with the approval flow,
// and time formatting. Text from memory is rendered as text only (no HTML, no auto-links).
import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { api } from "../../ipc/api";
import type { ApiError, FactDto } from "../../ipc/types";

export type Load<T> = { data: T | null; error: ApiError | null; loading: boolean; reload: () => void };

/// Incremented each time the panel opens. The panel stays mounted while closed (so opening
/// is only a style change), and every view reloads in the background when this changes.
export const OpenEpoch = createContext(0);

export function useLoad<T>(f: () => Promise<T>, deps: unknown[]): Load<T> {
  const epoch = useContext(OpenEpoch);
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [loading, setLoading] = useState(true);
  const [n, setN] = useState(0);
  useEffect(() => {
    let live = true;
    setLoading(true);
    f().then(
      (d) => live && (setData(d), setError(null), setLoading(false)),
      (e: ApiError) => live && (setError(e), setLoading(false)),
    );
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, n, epoch]);
  const reload = useCallback(() => setN((x) => x + 1), []);
  return { data, error, loading, reload };
}

const ERRORS: Record<string, string> = {
  invalid_input: "That request was not valid.",
  not_found: "That item no longer exists.",
  store_error: "The memory store could not be read.",
  unavailable: "Tessara is still starting, or could not start.",
  approval_invalid: "That confirmation expired. Try again.",
  ipc_error: "Something went wrong talking to Tessara.",
};

export function Status({ load, empty, children }: { load: Load<unknown>; empty: boolean; children: ReactNode }) {
  if (load.loading && !load.data) return <p className="notch-muted py-6 text-center">Loading…</p>;
  if (load.error)
    return (
      <div role="alert" className="py-6 text-center">
        <p>{ERRORS[load.error.code] ?? ERRORS.ipc_error}</p>
        <button type="button" className="notch-button mt-2" onClick={load.reload}>
          Retry
        </button>
      </div>
    );
  if (empty) return <p className="notch-muted py-6 text-center">Nothing here yet.</p>;
  return <>{children}</>;
}

export function time(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function due(f: FactDto): string | null {
  if (f.due_at === null) return f.due_text;
  const d = new Date(f.due_at);
  const s =
    f.due_precision === "datetime"
      ? d.toLocaleString([], { weekday: "short", hour: "2-digit", minute: "2-digit" })
      : d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" });
  return f.due_ambiguous ? `${s} (unsure)` : s;
}

/// A fact with Done / Drop. Changing status is a two-step approval: Rust issues a single-use
/// token for exactly this change when the confirm buttons appear; Confirm sends it back.
export function FactRow({ f, onChanged }: { f: FactDto; onChanged: () => void }) {
  const [pending, setPending] = useState<{ status: string; token: string } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const ask = (status: string) =>
    api.proposeFactStatus(f.id, status).then(
      (token) => (setPending({ status, token }), setErr(null)),
      (e: ApiError) => setErr(ERRORS[e.code] ?? ERRORS.ipc_error ?? e.message),
    );
  const confirm = () =>
    pending &&
    api.setFactStatus(f.id, pending.status, pending.token).then(
      () => (setPending(null), onChanged()),
      (e: ApiError) => (setPending(null), setErr(ERRORS[e.code] ?? ERRORS.ipc_error ?? e.message)),
    );
  const d = due(f);
  return (
    <li className="notch-row flex items-start gap-2 rounded-lg px-2 py-1.5">
      <span className="notch-chip mt-0.5 shrink-0">{f.status === "needs_review" ? "review" : f.kind}</span>
      <div className="min-w-0 flex-1">
        <p className="break-words">{f.text}</p>
        <p className="notch-muted text-[11px]">
          {[f.owner && `owner: ${f.owner}`, f.counterparty && `with ${f.counterparty}`, d && `due ${d}`]
            .filter(Boolean)
            .join(" · ")}
        </p>
        {err && (
          <p role="alert" className="text-[11px] text-red-500">
            {err}
          </p>
        )}
      </div>
      {pending ? (
        <span className="flex shrink-0 gap-1">
          <button type="button" className="notch-button" onClick={confirm}>
            Confirm {pending.status === "done" ? "done" : pending.status}
          </button>
          <button type="button" className="notch-button" onClick={() => setPending(null)}>
            Cancel
          </button>
        </span>
      ) : (
        <span className="flex shrink-0 gap-1">
          <button type="button" className="notch-button" onClick={() => ask("done")} aria-label={`Mark done: ${f.text}`}>
            Done
          </button>
          <button type="button" className="notch-button" onClick={() => ask("dropped")} aria-label={`Drop: ${f.text}`}>
            Drop
          </button>
        </span>
      )}
    </li>
  );
}
