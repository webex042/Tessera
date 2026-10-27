// Expanded panel (plan §13.2–13.4): tabs Today, Open loops, Search, Timeline (day). Keyboard:
// Left/Right/Home/End move between tabs, Esc closes. Data comes from Rust; every view has
// loading, empty and error states.
import { useEffect, useRef, type KeyboardEvent, type ReactNode } from "react";
import type { Status } from "../ipc/types";
import { TodayView } from "./views/TodayView";
import { OpenLoopsView } from "./views/OpenLoopsView";
import { SearchView } from "./views/SearchView";
import { AskView } from "./views/AskView";
import { TimelineView } from "./views/TimelineView";

export type Tab = "today" | "ask" | "loops" | "search" | "timeline";

const TABS: { id: Tab; label: string }[] = [
  { id: "today", label: "Today" },
  { id: "ask", label: "Ask" },
  { id: "loops", label: "Open loops" },
  { id: "search", label: "Search" },
  { id: "timeline", label: "Timeline" },
];

export function Expanded(props: {
  shown: boolean;
  tab: Tab;
  onTab: (t: Tab) => void;
  status: Status | null;
  onClose: () => void;
  onTogglePause: () => void;
}) {
  const { shown, tab, onTab, status, onClose, onTogglePause } = props;
  const tabRefs = useRef<Record<string, HTMLButtonElement | null>>({});
  useEffect(() => {
    if (shown) tabRefs.current[tab]?.focus();
    // Focus the active tab only when the panel opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shown]);

  const onTabKey = (e: KeyboardEvent) => {
    const i = TABS.findIndex((t) => t.id === tab);
    let n = i;
    if (e.key === "ArrowRight") n = (i + 1) % TABS.length;
    else if (e.key === "ArrowLeft") n = (i - 1 + TABS.length) % TABS.length;
    else if (e.key === "Home") n = 0;
    else if (e.key === "End") n = TABS.length - 1;
    else return;
    const next = TABS[n];
    if (!next) return;
    e.preventDefault();
    onTab(next.id);
    tabRefs.current[next.id]?.focus();
  };

  let view: ReactNode;
  if (tab === "today") view = <TodayView />;
  else if (tab === "loops") view = <OpenLoopsView />;
  else if (tab === "search") view = <SearchView />;
  else if (tab === "ask") view = <AskView />;
  else view = <TimelineView />;

  return (
    <div
      role="dialog"
      aria-label="Tessara"
      onKeyDown={(e) => e.key === "Escape" && onClose()}
      className={`notch-surface notch-panel m-1 flex h-[512px] flex-col rounded-2xl text-sm ${shown ? "is-shown" : ""}`}
    >
      <div className="flex items-center gap-2 border-b border-[var(--notch-line)] px-3 py-2">
        <div role="tablist" aria-label="Views" className="flex gap-1" onKeyDown={onTabKey}>
          {TABS.map((t) => (
            <button
              key={t.id}
              ref={(el) => {
                tabRefs.current[t.id] = el;
              }}
              role="tab"
              id={`tab-${t.id}`}
              aria-selected={tab === t.id}
              aria-controls={`panel-${t.id}`}
              tabIndex={tab === t.id ? 0 : -1}
              onClick={() => onTab(t.id)}
              className="notch-tab rounded-lg px-3 py-1"
            >
              {t.label}
            </button>
          ))}
        </div>
        <button type="button" onClick={onTogglePause} className="notch-button ml-auto" aria-pressed={status?.paused ?? true}>
          {status?.paused ? "Resume capture" : "Pause capture"}
        </button>
        <button type="button" onClick={onClose} className="notch-button" aria-label="Close panel">
          ✕
        </button>
      </div>
      <div role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`} className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
        {view}
      </div>
      {status && (
        <div className="notch-muted border-t border-[var(--notch-line)] px-3 py-1 text-[11px]">
          Model: {status.model.state.replace(/_/g, " ")}
          {status.model.backend ? ` (${status.model.backend})` : ""} · queued {status.queue[1] + status.queue[2]}
        </div>
      )}
    </div>
  );
}
