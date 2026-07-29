import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { useAppStore } from "../state/store";
import { LANGUAGES, useT } from "../i18n";
import { checkForUpdate, installUpdate } from "../ipc/commands";
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
            props.checked
              ? "ltr:translate-x-4 rtl:-translate-x-4"
              : "ltr:translate-x-0.5 rtl:-translate-x-0.5"
          }`}
        />
      </button>
    </label>
  );
}

/** Update checks are user-initiated only — nothing polls in the background. */
function UpdateRow() {
  const t = useT();
  const toast = useAppStore((s) => s.toast);
  const [busy, setBusy] = useState(false);
  const [found, setFound] = useState<string | null>(null);

  const check = async () => {
    setBusy(true);
    try {
      const info = await checkForUpdate();
      if (info.available && info.version) {
        setFound(info.version);
      } else {
        setFound(null);
        toast("info", t("upToDate"));
      }
    } catch (e) {
      toast("error", typeof e === "string" ? e : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex items-center justify-between gap-4 py-2">
      <div className="min-w-0">
        {found ? (
          <div className="text-sm text-accent">
            {t("updateAvailable").replace("{n}", found)}
          </div>
        ) : (
          <div className="text-sm">{t("checkForUpdates")}</div>
        )}
      </div>
      <button
        onClick={() => (found ? void installUpdate() : void check())}
        disabled={busy}
        className="flex shrink-0 items-center gap-1.5 rounded-input border border-border-strong px-3 py-1 text-xs transition-colors duration-[140ms] hover:border-accent hover:text-accent disabled:opacity-40"
      >
        {busy ? <RefreshCw size={12} className="animate-spin" /> : null}
        {busy ? t("checking") : found ? t("installUpdate") : t("checkForUpdates")}
      </button>
    </div>
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
  const t = useT();
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
      <Section title={t("sectionGeneral")}>
        <Toggle
          label={t("launchAtLogin")}
          checked={draft.autostart}
          onChange={(v) => apply({ autostart: v })}
        />
        <Toggle
          label={t("startMinimized")}
          hint={t("startMinimizedHint")}
          checked={draft.startMinimized}
          onChange={(v) => apply({ startMinimized: v })}
        />
        <Toggle
          label={t("connectOnLaunch")}
          hint={t("connectOnLaunchHint")}
          checked={draft.autoConnect}
          onChange={(v) => apply({ autoConnect: v })}
        />
        <div className="flex items-center justify-between gap-4 py-2">
          <div className="text-sm">{t("language")}</div>
          <select
            value={draft.language}
            onChange={(e) => apply({ language: e.target.value })}
            className="rounded-input border border-border bg-bg-base px-2 py-1 text-xs text-text-secondary outline-none"
          >
            {LANGUAGES.map((l) => (
              <option key={l.value} value={l.value}>
                {l.label}
              </option>
            ))}
          </select>
        </div>
      </Section>

      <Section title={t("sectionNetwork")}>
        <div className="flex items-center justify-between gap-4 py-2">
          <div>
            <div className="text-sm">{t("localPort")}</div>
            <div className="text-2xs text-text-muted" dir="ltr">
              {draft.allowLan ? t("localPortHintLan") : t("localPortHintLoopback")}
            </div>
          </div>
          <input
            value={port}
            onChange={(e) => setPort(e.target.value.replace(/\D/g, ""))}
            onBlur={commitPort}
            onKeyDown={(e) => e.key === "Enter" && commitPort()}
            disabled={locked}
            inputMode="numeric"
            dir="ltr"
            className="tabular w-24 rounded-input border border-border bg-bg-base px-2 py-1 text-right font-mono text-sm outline-none focus:border-border-strong disabled:opacity-40"
          />
        </div>
        <Toggle
          label={t("allowLan")}
          hint={t("allowLanHint")}
          checked={draft.allowLan}
          disabled={locked}
          onChange={(v) => apply({ allowLan: v })}
        />
        <Toggle
          label={t("splitRouting")}
          hint={t("splitRoutingHint")}
          checked={draft.rulesEnabled}
          disabled={locked}
          onChange={(v) => apply({ rulesEnabled: v })}
        />
        <Toggle
          label={t("adBlocking")}
          hint={t("adBlockingHint")}
          checked={draft.adBlock}
          disabled={locked || !draft.rulesEnabled}
          onChange={(v) => apply({ adBlock: v })}
        />
      </Section>

      <Section title={t("sectionAdvanced")}>
        <Toggle
          label={t("rulesetAutoUpdate")}
          hint={t("rulesetAutoUpdateHint")}
          checked={draft.rulesetAutoUpdate}
          onChange={(v) => apply({ rulesetAutoUpdate: v })}
        />
        <div className="flex items-center justify-between gap-4 py-2">
          <div className="text-sm">{t("coreLogLevel")}</div>
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

      <Section title={t("sectionAbout")}>
        <div className="flex items-center justify-between py-2">
          <span className="text-sm">PrivateSparta</span>
          <span className="tabular font-mono text-xs text-text-muted">v{version}</span>
        </div>
        <div className="flex items-center justify-between py-2">
          <span className="text-sm">{t("author")}</span>
          <span className="select-text text-xs text-text-secondary" dir="ltr">
            H. Talebi · github.com/mr-physicist2002
          </span>
        </div>
        <div className="flex items-center justify-between py-2">
          <span className="text-sm">{t("support")}</span>
          <span className="select-text font-mono text-xs text-text-secondary" dir="ltr">
            @PrivateSpartaBot
          </span>
        </div>
        <UpdateRow />
      </Section>
    </main>
  );
}
