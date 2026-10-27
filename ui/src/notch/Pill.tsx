// Collapsed pill (plan §13.2): status dot, state word, count of items due today.
import type { ApiError, Status } from "../ipc/types";

function state(status: Status | null, error: ApiError | null): { label: string; dot: string } {
  if (error) return { label: "Error", dot: "bg-red-500" };
  if (!status) return { label: "Starting", dot: "bg-zinc-400" };
  if (status.paused) return { label: "Paused", dot: "bg-amber-400" };
  if (status.model.state === "restarting") return { label: "Model loading", dot: "bg-sky-400" };
  return { label: "Capturing", dot: "bg-emerald-400" };
}

export function Pill({ status, error, onOpen }: { status: Status | null; error: ApiError | null; onOpen: () => void }) {
  const s = state(status, error);
  const due = status ? status.counts.due_today + status.counts.overdue : 0;
  return (
    <div className="flex h-full items-center justify-center">
      <button
        type="button"
        onClick={onOpen}
        aria-label={`Tessara: ${s.label}${due ? `, ${due} due` : ""}. Open panel`}
        className="notch-surface flex h-[30px] w-[214px] items-center gap-2 rounded-full px-3 text-xs"
      >
        <span aria-hidden="true" className={`h-2 w-2 rounded-full ${s.dot}`} />
        <span className="font-medium tracking-wide">Tessara</span>
        <span className="notch-muted ml-auto" role="status">
          {s.label}
          {due > 0 && <span className="ml-2 rounded-full bg-red-500/80 px-1.5 text-[10px] text-white">{due}</span>}
        </span>
      </button>
    </div>
  );
}
