// Draft shell (plan §13.2, 640 × 360). The draft flow itself is Stage 5; until a draft is
// generated this shows the empty state, never sample text.
import { useEffect, useRef } from "react";

export function Draft({ onClose }: { onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => ref.current?.focus(), []);
  return (
    <div
      ref={ref}
      tabIndex={-1}
      role="dialog"
      aria-label="Draft"
      onKeyDown={(e) => e.key === "Escape" && onClose()}
      className="notch-surface notch-fade-in m-1 flex h-[352px] flex-col rounded-2xl p-4 text-sm"
    >
      <div className="flex items-center">
        <h2 className="font-semibold">Draft</h2>
        <button type="button" onClick={onClose} className="notch-button ml-auto">
          Close
        </button>
      </div>
      <p className="notch-muted mt-6">No draft yet. Put the cursor in a text box and press Ctrl+Alt+D.</p>
    </div>
  );
}
