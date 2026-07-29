import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, X } from "lucide-react";

export function TitleBar() {
  const win = getCurrentWindow();
  return (
    <header
      data-tauri-drag-region
      className="flex h-9 shrink-0 items-center justify-between border-b border-border bg-bg-base"
    >
      <span
        data-tauri-drag-region
        className="pl-4 font-display text-xs font-semibold tracking-wide text-text-secondary"
      >
        PRIVATESPARTA
      </span>
      <div className="flex h-full">
        <button
          aria-label="Minimize"
          onClick={() => void win.minimize()}
          className="flex h-full w-11 items-center justify-center text-text-muted transition-colors duration-[140ms] hover:bg-bg-elevated hover:text-text-primary"
        >
          <Minus size={14} />
        </button>
        <button
          aria-label="Close"
          onClick={() => void win.close()}
          className="flex h-full w-11 items-center justify-center text-text-muted transition-colors duration-[140ms] hover:bg-danger hover:text-text-primary"
        >
          <X size={14} />
        </button>
      </div>
    </header>
  );
}
