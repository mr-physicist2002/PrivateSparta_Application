import { useAppStore } from "../state/store";
import type { ConnectionState } from "../ipc/types";

const LABEL: Record<ConnectionState, string> = {
  disconnected: "Connect",
  connecting: "Connecting…",
  connected: "Disconnect",
  disconnecting: "Disconnecting…",
  error: "Retry",
};

const STATUS: Record<ConnectionState, string> = {
  disconnected: "Not connected",
  connecting: "Connecting",
  connected: "Connected",
  disconnecting: "Disconnecting",
  error: "Connection failed",
};

/** Phase 1 stand-in for the lambda shield (Phase 3). Plain, fast, correct. */
export function ConnectControl() {
  const connection = useAppStore((s) => s.connection);
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const toggle = useAppStore((s) => s.toggleConnection);

  const busy =
    connection.state === "connecting" || connection.state === "disconnecting";
  const disabled = busy || (!selectedNodeId && connection.state !== "connected");

  return (
    <div className="flex flex-col items-center gap-3">
      <div
        className={`size-2.5 rounded-full transition-colors duration-[140ms] ${
          connection.state === "connected"
            ? "bg-ok"
            : connection.state === "error"
              ? "bg-danger"
              : busy
                ? "bg-warn"
                : "bg-border-strong"
        }`}
      />
      <span className="font-display text-xl font-semibold">
        {STATUS[connection.state]}
      </span>
      <button
        onClick={() => void toggle()}
        disabled={disabled}
        className={`rounded-input border px-10 py-2.5 font-display text-base font-semibold transition-colors duration-[140ms] disabled:cursor-not-allowed disabled:opacity-40 ${
          connection.state === "connected"
            ? "border-border-strong text-text-secondary hover:border-danger hover:text-danger"
            : "border-accent bg-accent-wash text-accent hover:bg-accent hover:text-bg-base"
        }`}
      >
        {LABEL[connection.state]}
      </button>
      {connection.state === "error" && connection.message ? (
        <p className="max-w-sm text-center text-sm text-danger">
          {connection.message}
        </p>
      ) : null}
    </div>
  );
}
