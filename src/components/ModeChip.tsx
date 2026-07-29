import { useState } from "react";
import { ShieldAlert } from "lucide-react";
import { useAppStore } from "../state/store";
import { useT } from "../i18n";
import type { ProxyMode } from "../ipc/types";

export function ModeChip() {
  const t = useT();
  const settings = useAppStore((s) => s.settings);
  const saveSettings = useAppStore((s) => s.saveSettings);
  const elevated = useAppStore((s) => s.elevated);
  const relaunchElevated = useAppStore((s) => s.relaunchElevated);
  const state = useAppStore((s) => s.connection.state);
  const locked = state !== "disconnected" && state !== "error";
  const [tunPrompt, setTunPrompt] = useState(false);

  const MODES: Array<{ value: ProxyMode; label: string }> = [
    { value: "system-proxy", label: t("modeSystemProxy") },
    { value: "proxy-only", label: t("modeProxyOnly") },
    { value: "tun", label: t("modeTun") },
  ];

  const pick = (mode: ProxyMode) => {
    if (mode === "tun" && !elevated) {
      setTunPrompt(true);
      return;
    }
    void saveSettings({ ...settings, mode });
  };

  return (
    <>
      <div className="flex rounded-input border border-border bg-bg-surface p-0.5">
        {MODES.map((m) => (
          <button
            key={m.value}
            disabled={locked}
            onClick={() => pick(m.value)}
            title={locked ? t("disconnectToChangeMode") : undefined}
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

      {tunPrompt ? (
        <div className="absolute inset-0 z-10 flex items-center justify-center bg-black/60">
          <div className="w-[26rem] rounded-modal border border-border-strong bg-bg-surface p-5">
            <div className="flex items-center gap-2.5">
              <ShieldAlert size={18} className="shrink-0 text-warn" />
              <h2 className="font-display text-base font-semibold">
                {t("tunNeedsAdminTitle")}
              </h2>
            </div>
            <p className="mt-2 text-sm leading-relaxed text-text-secondary">
              {t("tunNeedsAdminBody")}
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <button
                onClick={() => setTunPrompt(false)}
                className="rounded-input px-4 py-1.5 text-sm text-text-secondary transition-colors duration-[140ms] hover:text-text-primary"
              >
                {t("cancel")}
              </button>
              <button
                onClick={() => {
                  void saveSettings({ ...settings, mode: "tun" }).then(() =>
                    relaunchElevated(),
                  );
                }}
                className="rounded-input bg-accent px-4 py-1.5 text-sm font-medium text-bg-base transition-colors duration-[140ms] hover:bg-accent-hover"
              >
                {t("restartAsAdmin")}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
}
