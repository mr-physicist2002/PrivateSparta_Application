import { useAppStore } from "../state/store";
import { useT } from "../i18n";
import type { ConnectionState } from "../ipc/types";
import { Shield } from "./Shield";

export function ConnectControl() {
  const t = useT();
  const connection = useAppStore((s) => s.connection);
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const traffic = useAppStore((s) => s.traffic);
  const toggle = useAppStore((s) => s.toggleConnection);

  const STATUS: Record<ConnectionState, string> = {
    disconnected: t("statusNotConnected"),
    connecting: t("statusConnecting"),
    connected: t("statusConnected"),
    disconnecting: t("statusDisconnecting"),
    error: t("statusFailed"),
  };
  const ACTION: Record<ConnectionState, string> = {
    disconnected: t("connect"),
    connecting: t("statusConnecting"),
    connected: t("disconnect"),
    disconnecting: t("statusDisconnecting"),
    error: t("retry"),
  };

  const busy =
    connection.state === "connecting" || connection.state === "disconnecting";
  const disabled = busy || (!selectedNodeId && connection.state !== "connected");

  return (
    <div className="flex flex-col items-center gap-1">
      <Shield
        state={connection.state}
        throughputBps={(traffic?.upBps ?? 0) + (traffic?.downBps ?? 0)}
        disabled={disabled}
        onClick={() => void toggle()}
        label={ACTION[connection.state]}
      />
      <span className="font-display text-xl font-semibold">
        {STATUS[connection.state]}
      </span>
      {connection.state === "error" && connection.message ? (
        <p className="max-w-sm text-center text-sm text-danger">
          {connection.message}
        </p>
      ) : null}
    </div>
  );
}
