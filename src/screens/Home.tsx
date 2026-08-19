import { ArrowDown, ArrowUp, ClipboardPaste } from "lucide-react";
import { findNode, useAppStore } from "../state/store";
import { useT } from "../i18n";
import { ConnectControl } from "../components/ConnectControl";
import { ModeChip } from "../components/ModeChip";
import { LatencyBadge } from "../components/LatencyBadge";
import {
  formatBytes,
  formatDuration,
  formatExpiry,
  formatSpeed,
} from "../lib/format";

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col items-center gap-0.5">
      <span className="text-2xs uppercase tracking-wide text-text-muted">{label}</span>
      <span className="tabular font-mono text-sm text-text-primary">{value}</span>
    </div>
  );
}

export function Home() {
  const t = useT();
  const connection = useAppStore((s) => s.connection);
  const traffic = useAppStore((s) => s.traffic);
  const manualNodes = useAppStore((s) => s.manualNodes);
  const subscriptions = useAppStore((s) => s.subscriptions);
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const previewImport = useAppStore((s) => s.previewImport);
  const setScreen = useAppStore((s) => s.setScreen);
  const node = findNode({ manualNodes, subscriptions }, selectedNodeId);
  const parentSub = subscriptions.find((s) =>
    s.nodes.some((n) => n.id === selectedNodeId),
  );
  const hasNodes =
    manualNodes.length > 0 || subscriptions.some((s) => s.nodes.length > 0);

  return (
    <main className="flex min-h-0 flex-1 flex-col items-center gap-6 overflow-y-auto px-8 py-8">
      <ConnectControl />
      <ModeChip />

      {node ? (
        <button
          onClick={() => setScreen("servers")}
          className="flex items-center gap-3 rounded-card border border-border bg-bg-surface px-4 py-2.5 transition-colors duration-[140ms] hover:bg-bg-elevated"
        >
          <span className="max-w-52 truncate text-sm">{node.name}</span>
          <span className="rounded-input bg-bg-elevated px-1.5 py-0.5 font-mono text-2xs text-text-secondary">
            {node.protocol}
          </span>
          <LatencyBadge ms={node.latencyMs} />
        </button>
      ) : hasNodes ? (
        <button
          onClick={() => setScreen("servers")}
          className="text-sm text-text-secondary transition-colors duration-[140ms] hover:text-accent"
        >
          {t("pickServer")}
        </button>
      ) : (
        <div className="flex flex-col items-center gap-3 text-center">
          <p className="text-sm text-text-secondary">{t("emptyHint")}</p>
          <div className="flex gap-2">
            <button
              onClick={() => void previewImport()}
              className="flex items-center gap-2 rounded-input border border-border-strong px-4 py-2 text-sm font-medium transition-colors duration-[140ms] hover:border-accent hover:text-accent"
            >
              <ClipboardPaste size={15} />
              {t("importFromClipboard")}
            </button>
            <button
              onClick={() => setScreen("subscriptions")}
              className="rounded-input border border-border-strong px-4 py-2 text-sm font-medium transition-colors duration-[140ms] hover:border-accent hover:text-accent"
            >
              {t("addSubscription")}
            </button>
          </div>
        </div>
      )}

      {connection.state === "connected" ? (
        <div className="flex items-center gap-8 rounded-card border border-border bg-bg-surface px-6 py-3" dir="ltr">
          <div className="flex items-center gap-1.5">
            <ArrowUp size={13} className="text-text-muted" />
            <span className="tabular w-20 text-right font-mono text-sm">
              {formatSpeed(traffic?.upBps ?? 0)}
            </span>
          </div>
          <div className="flex items-center gap-1.5">
            <ArrowDown size={13} className="text-accent" />
            <span className="tabular w-20 text-right font-mono text-sm">
              {formatSpeed(traffic?.downBps ?? 0)}
            </span>
          </div>
          <Stat
            label={t("session")}
            value={formatBytes((traffic?.upTotal ?? 0) + (traffic?.downTotal ?? 0))}
          />
          <Stat label={t("time")} value={formatDuration(traffic?.seconds ?? 0)} />
        </div>
      ) : null}

      {parentSub?.userInfo ? (
        <div className="w-full max-w-sm">
          <div className="mb-1 flex justify-between text-2xs text-text-muted">
            <span className="tabular font-mono" dir="ltr">
              {formatBytes(parentSub.userInfo.upload + parentSub.userInfo.download)}
              {" / "}
              {formatBytes(parentSub.userInfo.total)}
            </span>
            {parentSub.userInfo.expire ? (
              <span>
                {t("expires")} {formatExpiry(parentSub.userInfo.expire)}
              </span>
            ) : null}
          </div>
          <div className="h-1 overflow-hidden rounded-full bg-bg-elevated">
            <div
              className="h-full bg-accent"
              style={{
                width: `${Math.min(
                  100,
                  ((parentSub.userInfo.upload + parentSub.userInfo.download) /
                    Math.max(1, parentSub.userInfo.total)) *
                    100,
                )}%`,
              }}
            />
          </div>
        </div>
      ) : null}
    </main>
  );
}
