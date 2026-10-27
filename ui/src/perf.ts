// Frame-timing harness (Stage 3 exit criteria: 60 fps expand/collapse). Records the time between
// animation frames for `durationMs` and sends the numbers to Rust, which writes them to a file
// only when TSR_FRAME_LOG is set. No content leaves this function.
import { api } from "./ipc/api";

export function measureFrames(label: string, durationMs: number): void {
  const deltas: number[] = [];
  const start = performance.now();
  let last = start;
  const tick = (t: number) => {
    deltas.push(t - last);
    last = t;
    if (t - start < durationMs) {
      requestAnimationFrame(tick);
    } else {
      api.reportFrames(label, deltas.slice(1)).catch(() => {});
    }
  };
  requestAnimationFrame(tick);
}
