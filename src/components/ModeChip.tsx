import { useAppStore } from "../state/store";
import type { ProxyMode } from "../ipc/types";

const MODES: Array<{ value: ProxyMode; label: string }> = [
  { value: "system-proxy", label: "System proxy" },
  { value: "proxy-only", label: "Proxy only" },
];

export function ModeChip() {
  const settings = useAppStore((s) => s.settings);
  const saveSettings = useAppStore((s) => s.saveSettings);
  const state = useAppStore((s) => s.connection.state);
  const locked = state !== "disconnected" && state !== "error";

  return (
    <div className="flex rounded-input border border-border bg-bg-surface p-0.5">
      {MODES.map((m) => (
        <button
          key={m.value}
          disabled={locked}
          onClick={() => void saveSettings({ ...settings, mode: m.value })}
          title={locked ? "Disconnect to change mode" : undefined}
          className={`rounded-[5px] px-3 py-1 text-xs font-medium transition-colors duration-[140ms] disabled:cursor-not-allowed ${
            settings.mode === m.value
              ? "bg-bg-elevated text-text-primary"
              : "text-text-muted hover:text-text-secondary"
          }`}
        >
          {m.label}
        </button>
      ))}
    </div>
  );
}
