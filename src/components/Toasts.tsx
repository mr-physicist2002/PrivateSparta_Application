import { useAppStore } from "../state/store";

export function Toasts() {
  const toasts = useAppStore((s) => s.toasts);
  const dismiss = useAppStore((s) => s.dismissToast);
  if (toasts.length === 0) return null;

  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-4 flex flex-col items-center gap-2">
      {toasts.map((t) => (
        <button
          key={t.id}
          onClick={() => dismiss(t.id)}
          className={`pointer-events-auto rounded-card border px-4 py-2 text-sm shadow-[0_1px_0_rgba(255,255,255,0.03)_inset] ${
            t.kind === "error"
              ? "border-danger/40 bg-bg-elevated text-danger"
              : "border-border bg-bg-elevated text-text-primary"
          }`}
        >
          {t.text}
        </button>
      ))}
    </div>
  );
}
