import { useEffect, useState } from "react";
import { useAppStore } from "../state/store";
import type { Settings } from "../ipc/types";

function Toggle(props: {
  label: string;
  hint?: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <label
      className={`flex items-center justify-between gap-4 py-2 ${
        props.disabled ? "opacity-40" : "cursor-pointer"
      }`}
    >
      <div>
        <div className="text-sm">{props.label}</div>
        {props.hint ? <div className="text-2xs text-text-muted">{props.hint}</div> : null}
      </div>
      <button
        role="switch"
        aria-checked={props.checked}
        disabled={props.disabled}
        onClick={() => props.onChange(!props.checked)}
        className={`relative h-5 w-9 shrink-0 rounded-full transition-colors duration-[140ms] ${
          props.checked ? "bg-accent" : "bg-border-strong"
        }`}
      >
        <span
          className={`absolute top-0.5 size-4 rounded-full bg-bg-base transition-transform duration-[140ms] ${
            props.checked ? "translate-x-4" : "translate-x-0.5"
          }`}
        />
      </button>
    </label>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-card border border-border bg-bg-surface px-4 py-3">
      <h3 className="mb-1 text-2xs font-medium uppercase tracking-wide text-text-muted">
        {title}
      </h3>
      {children}
    </section>
  );
}

export function SettingsScreen() {
  const stored = useAppStore((s) => s.settings);
  const version = useAppStore((s) => s.version);
  const saveSettings = useAppStore((s) => s.saveSettings);
  const connection = useAppStore((s) => s.connection);
  const locked = connection.state !== "disconnected" && connection.state !== "error";

  const [draft, setDraft] = useState<Settings>(stored);
  const [port, setPort] = useState(String(stored.localPort));
  useEffect(() => {
    setDraft(stored);
    setPort(String(stored.localPort));
  }, [stored]);

  const apply = (patch: Partial<Settings>) => {
    const next = { ...draft, ...patch };
    setDraft(next);
    void saveSettings(next);
  };

  const commitPort = () => {
    const parsed = Number(port);
    if (!Number.isInteger(parsed) || parsed < 1025 || parsed > 65535) {
      setPort(String(draft.localPort));
      return;
    }
    if (parsed !== draft.localPort) apply({ localPort: parsed });
  };

  return (
    <main className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-6 py-5">
      <Section title="General">
        <Toggle
          label="Launch at login"
          checked={draft.autostart}
          onChange={(v) => apply({ autostart: v })}
        />
        <Toggle
          label="Start minimized"
          hint="Open in the tray without showing the window"
          checked={draft.startMinimized}
          onChange={(v) => apply({ startMinimized: v })}
        />
        <Toggle
          label="Connect on launch"
          hint="Reconnects to the last selected server"
          checked={draft.autoConnect}
          onChange={(v) => apply({ autoConnect: v })}
        />
      </Section>

      <Section title="Network">
        <div className="flex items-center justify-between gap-4 py-2">
          <div>
            <div className="text-sm">Local port</div>
            <div className="text-2xs text-text-muted">
              SOCKS + HTTP on 127.0.0.1{draft.allowLan ? " and LAN" : ""}
            </div>
          </div>
          <input
            value={port}
            onChange={(e) => setPort(e.target.value.replace(/\D/g, ""))}
            onBlur={commitPort}
            onKeyDown={(e) => e.key === "Enter" && commitPort()}
            disabled={locked}
            inputMode="numeric"
            className="tabular w-24 rounded-input border border-border bg-bg-base px-2 py-1 text-right font-mono text-sm outline-none focus:border-border-strong disabled:opacity-40"
          />
        </div>
        <Toggle
          label="Allow LAN connections"
          hint="Other devices on your network can use the proxy"
          checked={draft.allowLan}
          disabled={locked}
          onChange={(v) => apply({ allowLan: v })}
        />
      </Section>

      <Section title="Advanced">
        <div className="flex items-center justify-between gap-4 py-2">
          <div className="text-sm">Core log level</div>
          <select
            value={draft.logLevel}
            onChange={(e) => apply({ logLevel: e.target.value })}
            className="rounded-input border border-border bg-bg-base px-2 py-1 text-xs text-text-secondary outline-none"
          >
            {["error", "warn", "info", "debug"].map((level) => (
              <option key={level} value={level}>
                {level}
              </option>
            ))}
          </select>
        </div>
      </Section>

      <Section title="About">
        <div className="flex items-center justify-between py-2">
          <span className="text-sm">PrivateSparta</span>
          <span className="tabular font-mono text-xs text-text-muted">v{version}</span>
        </div>
        <div className="flex items-center justify-between py-2">
          <span className="text-sm">Support</span>
          <span className="font-mono text-xs text-text-secondary select-text">
            @PrivateSpartaBot
          </span>
        </div>
      </Section>
    </main>
  );
}
