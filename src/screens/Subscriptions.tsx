import { useState } from "react";
import { Eye, Plus, RefreshCw, Trash2 } from "lucide-react";
import { useAppStore } from "../state/store";
import { interpolate, useT } from "../i18n";
import type { SubscriptionView, UpdateInterval } from "../ipc/types";
import { revealSubscriptionUrl } from "../ipc/commands";
import { formatBytes, formatExpiry, formatRelative } from "../lib/format";

export function Subscriptions() {
  const t = useT();
  const { subscriptions } = useAppStore();
  return (
    <main className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-6 py-5">
      <AddForm />
      {subscriptions.length === 0 ? (
        <p className="mt-8 text-center text-sm text-text-secondary">
          {t("subEmptyHint")}
        </p>
      ) : (
        subscriptions.map((sub) => <SubCard key={sub.id} sub={sub} />)
      )}
    </main>
  );
}

function AddForm() {
  const t = useT();
  const addSubscription = useAppStore((s) => s.addSubscription);
  const [url, setUrl] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (!url.trim() || busy) return;
    setBusy(true);
    const ok = await addSubscription(name, url);
    setBusy(false);
    if (ok) {
      setUrl("");
      setName("");
    }
  };

  return (
    <div className="flex gap-2">
      <input
        value={url}
        onChange={(e) => setUrl(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && void submit()}
        placeholder={t("subLinkPlaceholder")}
        dir="ltr"
        className="min-w-0 flex-[2] rounded-input border border-border bg-bg-surface px-3 py-2 text-sm outline-none placeholder:text-text-muted focus:border-border-strong"
      />
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && void submit()}
        placeholder={t("subNamePlaceholder")}
        className="min-w-0 flex-1 rounded-input border border-border bg-bg-surface px-3 py-2 text-sm outline-none placeholder:text-text-muted focus:border-border-strong"
      />
      <button
        onClick={() => void submit()}
        disabled={!url.trim() || busy}
        className="flex items-center gap-1.5 rounded-input bg-accent px-4 py-2 text-sm font-medium text-bg-base transition-colors duration-[140ms] hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
      >
        {busy ? (
          <RefreshCw size={14} className="animate-spin" />
        ) : (
          <Plus size={14} />
        )}
        {t("add")}
      </button>
    </div>
  );
}

function SubCard({ sub }: { sub: SubscriptionView }) {
  const t = useT();
  const { updateSubscription, deleteSubscription, setSubAutoUpdate, connection, toast } =
    useAppStore();
  const INTERVALS: Array<{ value: UpdateInterval; label: string }> = [
    { value: "off", label: t("intervalManual") },
    { value: "6h", label: t("interval6h") },
    { value: "12h", label: t("interval12h") },
    { value: "24h", label: t("interval24h") },
  ];
  const [revealed, setRevealed] = useState<string | null>(null);
  const [updating, setUpdating] = useState(false);
  const locked = connection.state !== "disconnected" && connection.state !== "error";
  const used = sub.userInfo ? sub.userInfo.upload + sub.userInfo.download : 0;

  const update = async () => {
    setUpdating(true);
    await updateSubscription(sub.id);
    setUpdating(false);
  };

  const reveal = async () => {
    if (revealed) {
      setRevealed(null);
      return;
    }
    try {
      setRevealed(await revealSubscriptionUrl(sub.id));
    } catch {
      toast("error", t("toastLinkUnreadable"));
    }
  };

  return (
    <div className="rounded-card border border-border bg-bg-surface p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="truncate font-display text-base font-semibold">{sub.name}</h3>
          <button
            onClick={() => void reveal()}
            className="mt-0.5 flex max-w-full items-center gap-1.5 font-mono text-2xs text-text-muted transition-colors duration-[140ms] hover:text-text-secondary"
          >
            <Eye size={11} className="shrink-0" />
            <span className="truncate select-text" dir="ltr">
              {revealed ?? sub.urlMasked}
            </span>
          </button>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <select
            value={sub.autoUpdate}
            onChange={(e) =>
              void setSubAutoUpdate(sub.id, e.target.value as UpdateInterval)
            }
            className="rounded-input border border-border bg-bg-base px-2 py-1 text-2xs text-text-secondary outline-none"
          >
            {INTERVALS.map((i) => (
              <option key={i.value} value={i.value}>
                {i.label}
              </option>
            ))}
          </select>
          <button
            aria-label={t("updateNow")}
            title={t("updateNow")}
            onClick={() => void update()}
            disabled={updating}
            className="rounded p-1.5 text-text-muted transition-colors duration-[140ms] hover:text-accent disabled:opacity-40"
          >
            <RefreshCw size={14} className={updating ? "animate-spin" : undefined} />
          </button>
          <button
            aria-label={t("delete")}
            title={t("delete")}
            onClick={() => void deleteSubscription(sub.id)}
            disabled={locked}
            className="rounded p-1.5 text-text-muted transition-colors duration-[140ms] hover:text-danger disabled:opacity-30"
          >
            <Trash2 size={14} />
          </button>
        </div>
      </div>

      <div className="mt-3 flex items-center gap-4 text-2xs text-text-muted">
        <span className="tabular font-mono">
          {interpolate(t("serversCount"), sub.nodeCount)}
        </span>
        {sub.lastUpdated ? (
          <span>
            {t("updatedPrefix")}{" "}
            {formatRelative(sub.lastUpdated, {
              justNow: t("justNow"),
              minutesAgo: t("minutesAgo"),
              hoursAgo: t("hoursAgo"),
              daysAgo: t("daysAgo"),
            })}
          </span>
        ) : null}
        {sub.userInfo?.expire ? (
          <span>
            {t("expires")} {formatExpiry(sub.userInfo.expire)}
          </span>
        ) : null}
      </div>

      {sub.userInfo ? (
        <div className="mt-2">
          <div className="mb-1 flex justify-between text-2xs text-text-muted">
            <span className="tabular font-mono">
              {formatBytes(used)} of {formatBytes(sub.userInfo.total)}
            </span>
            <span className="tabular font-mono">
              {Math.min(100, Math.round((used / Math.max(1, sub.userInfo.total)) * 100))}%
            </span>
          </div>
          <div className="h-1 overflow-hidden rounded-full bg-bg-elevated">
            <div
              className={`h-full ${used / Math.max(1, sub.userInfo.total) > 0.9 ? "bg-danger" : "bg-accent"}`}
              style={{
                width: `${Math.min(100, (used / Math.max(1, sub.userInfo.total)) * 100)}%`,
              }}
            />
          </div>
        </div>
      ) : null}

      {sub.lastError ? (
        <p className="mt-2 text-xs text-danger">{sub.lastError}</p>
      ) : null}
    </div>
  );
}
