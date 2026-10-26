// Collapsed pill (plan §13.2). Stage 0 shows a static "paused" state: the app starts
// paused (D12) and capture does not exist yet. Real status arrives over IPC in Stage 3.
export function Pill() {
  return (
    <div className="flex h-full items-center justify-center">
      <div
        role="status"
        aria-label="Tessara is paused"
        className="flex h-[30px] w-[214px] items-center gap-2 rounded-full bg-zinc-900/90 px-3 text-xs text-zinc-200 ring-1 ring-white/10"
      >
        <span aria-hidden="true" className="h-2 w-2 rounded-full bg-amber-400" />
        <span className="font-medium tracking-wide">Tessara</span>
        <span className="ml-auto text-zinc-400">Paused</span>
      </div>
    </div>
  );
}
