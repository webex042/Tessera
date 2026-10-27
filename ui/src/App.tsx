// The notch (plan §13): pill → peek → expanded → draft. The Rust side owns the window size;
// this component follows the plan's resize order: grow the window, then animate content in;
// animate content out, then shrink the window.
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { api, on } from "./ipc/api";
import type { ApiError, Panel, PanelRequest, Status } from "./ipc/types";
import { Pill } from "./notch/Pill";
import { Expanded, type Tab } from "./notch/Expanded";
import { Draft } from "./notch/Draft";
import { Peek } from "./notch/Peek";
import { measureFrames } from "./perf";
import { OpenEpoch } from "./notch/views/common";

/// Logical panel sizes; must match `Panel::size` in src-tauri/src/window.rs.
const SIZES: Record<Exclude<Panel, "hidden">, [number, number]> = {
  pill: [220, 34],
  peek: [380, 92],
  expanded: [760, 520],
  draft: [640, 360],
};

type Visual = "pill" | "peek" | "expanded" | "closing" | "draft";

function Layer({ size, children }: { size: [number, number]; children: ReactNode }) {
  return (
    <div className="absolute top-0 left-1/2 -translate-x-1/2" style={{ width: size[0], height: size[1] }}>
      {children}
    </div>
  );
}

export function App() {
  const [visual, setVisual] = useState<Visual>("pill");
  const [tab, setTab] = useState<Tab>("today");
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [peekText, setPeekText] = useState<string>("");
  const [epoch, setEpoch] = useState(0);
  const peekTimer = useRef<number | undefined>(undefined);

  const refresh = useCallback(() => {
    api.status().then(setStatus, setError);
  }, []);

  // `inputAt`: Unix ms when the user's input reached Rust (panel_request.at).
  // `placed`: Rust applies the region itself (hotkey/tray opens), so skip the round trip.
  const open = useCallback(async (t?: Tab, inputAt?: number, placed = false) => {
    if (t) setTab(t);
    // Refresh the views once the open animation is under way, off the input-to-visual path.
    window.setTimeout(() => setEpoch((n) => n + 1), 200);
    if (!placed) {
      try {
        await api.setPanel("expanded"); // window first
      } catch (e) {
        setError(e as ApiError);
        return;
      }
    }
    // The panel is already mounted in its closed style, so the transition starts from this change.
    setVisual("expanded");
    requestAnimationFrame(() => {
      if (inputAt !== undefined) {
        // Second frame after the state change: the first frame of the open animation is on screen.
        requestAnimationFrame(() => api.reportFrames("frame_after_input_ms", [Date.now() - inputAt]).catch(() => {}));
      }
      measureFrames("expand", 600);
    });
  }, []);

  const close = useCallback(async () => {
    setVisual("closing"); // content out first
    measureFrames("collapse", 400);
    window.setTimeout(async () => {
      try {
        await api.setPanel("pill");
      } catch (e) {
        setError(e as ApiError);
      }
      setVisual("pill");
    }, 180);
  }, []);

  const peek = useCallback((text: string) => {
    setPeekText(text);
    setVisual((v) => {
      if (v !== "pill") return v; // never interrupts an open panel
      api.setPanel("peek").catch(() => {});
      return "peek";
    });
    window.clearTimeout(peekTimer.current);
    peekTimer.current = window.setTimeout(() => {
      setVisual((v) => {
        if (v !== "peek") return v;
        api.setPanel("pill").catch(() => {});
        return "pill";
      });
    }, 5000);
  }, []);

  // Tell the user when a default hotkey was taken by another app (fallback or none).
  const checkHotkeys = useCallback(() => {
    api.hotkeys().then((hs) => {
      const missing = hs.find((h) => h.chord === null);
      const moved = hs.find((h) => h.chord !== null && h.chord !== h.default);
      if (missing) peek(`${missing.default} is used by another app`);
      else if (moved) peek(`${moved.default} is used by another app. Use ${moved.chord}`);
    }, () => {});
  }, [peek]);

  useEffect(() => {
    refresh();
    api.startError().then((e) => e && setError({ code: "start_failed", message: "error.start" }), () => {});
    api.captureExcluded().then((ok) => ok || peek("Could not hide Tessara from screen capture"), () => {});
    checkHotkeys();
    const subs = [
      on<Status>("status_changed", (s) => {
        setStatus((prev) => {
          if (prev && prev.paused !== s.paused) peek(s.paused ? "Capture paused" : "Capture resumed");
          return s;
        });
      }),
      on<PanelRequest>("panel_request", ({ panel: p, at, placed }) => {
        // Latency breakdown (numbers only; written only when TSR_FRAME_LOG is set).
        if (p !== "pill") api.reportFrames("event_after_input_ms", [Date.now() - at]).catch(() => {});
        if (p === "expanded") open(undefined, at, placed);
        else if (p === "search") open("search", at, placed);
        else if (p === "pill") close();
        else if (p === "draft") {
          if (placed) setVisual("draft");
          else api.setPanel("draft").then(() => setVisual("draft"), (e) => setError(e as ApiError));
        }
      }),
      on<null>("hotkeys_changed", checkHotkeys),
    ];
    const timer = window.setInterval(refresh, 30_000);
    return () => {
      window.clearInterval(timer);
      subs.forEach((p) => p.then((u) => u()));
    };
  }, [refresh, open, close, peek, checkHotkeys]);

  const panel: Exclude<Panel, "hidden"> = visual === "peek" ? "peek" : visual === "draft" ? "draft" : visual === "pill" ? "pill" : "expanded";

  return (
    // The window is always 760 × 520; Rust clips it to the current panel's region, centred at
    // the top. Each panel is laid out top-centre at exactly its region's size. The expanded
    // panel stays mounted (hidden) so opening it is a style change, not a React mount.
    <div className="relative h-full w-full" data-panel={panel}>
      <Layer size={SIZES.expanded}>
        <OpenEpoch.Provider value={epoch}>
          <Expanded
            shown={visual === "expanded"}
            tab={tab}
            onTab={setTab}
            status={status}
            onClose={close}
            onTogglePause={() => status && api.setPaused(!status.paused).then(setStatus, setError)}
          />
        </OpenEpoch.Provider>
      </Layer>
      {visual === "pill" && (
        <Layer size={SIZES.pill}>
          <Pill status={status} error={error} onOpen={() => open()} />
        </Layer>
      )}
      {visual === "peek" && (
        <Layer size={SIZES.peek}>
          <Peek text={peekText} onOpen={() => open()} />
        </Layer>
      )}
      {visual === "draft" && (
        <Layer size={SIZES.draft}>
          <Draft onClose={close} />
        </Layer>
      )}
    </div>
  );
}
