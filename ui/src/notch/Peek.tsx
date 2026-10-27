// Peek (plan §13.2): one short notification, auto-hides after 5 s. It never takes focus: it is
// announced politely and can be opened with a click.
export function Peek({ text, onOpen }: { text: string; onOpen: () => void }) {
  return (
    <div className="flex h-full items-start justify-center pt-1">
      <button
        type="button"
        onClick={onOpen}
        className="notch-surface notch-fade-in flex h-[84px] w-[372px] items-center rounded-2xl px-4 text-left text-sm"
      >
        <span role="status" aria-live="polite">
          {text}
        </span>
      </button>
    </div>
  );
}
