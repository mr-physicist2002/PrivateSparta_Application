import { ClipboardPaste, Trash2 } from "lucide-react";
import { useAppStore } from "../state/store";
import { ConnectControl } from "../components/ConnectControl";
import { ModeChip } from "../components/ModeChip";

export function Home() {
  const nodes = useAppStore((s) => s.nodes);
  const selectedNodeId = useAppStore((s) => s.selectedNodeId);
  const selectNode = useAppStore((s) => s.selectNode);
  const deleteNode = useAppStore((s) => s.deleteNode);
  const previewImport = useAppStore((s) => s.previewImport);
  const state = useAppStore((s) => s.connection.state);
  const locked = state !== "disconnected" && state !== "error";

  return (
    <main className="flex min-h-0 flex-1 flex-col items-center gap-8 px-8 py-10">
      <ConnectControl />
      <ModeChip />

      {nodes.length === 0 ? (
        <div className="flex flex-col items-center gap-3 text-center">
          <p className="text-sm text-text-secondary">
            Copy a server link, then import it from the clipboard.
          </p>
          <button
            onClick={() => void previewImport()}
            className="flex items-center gap-2 rounded-input border border-border-strong px-4 py-2 text-sm font-medium text-text-primary transition-colors duration-[140ms] hover:border-accent hover:text-accent"
          >
            <ClipboardPaste size={15} />
            Import from clipboard
          </button>
        </div>
      ) : (
        <div className="flex min-h-0 w-full max-w-md flex-1 flex-col gap-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium tracking-wide text-text-muted">
              SERVERS
            </span>
            <button
              onClick={() => void previewImport()}
              className="flex items-center gap-1.5 text-xs text-text-secondary transition-colors duration-[140ms] hover:text-accent"
            >
              <ClipboardPaste size={13} />
              Import
            </button>
          </div>
          <ul className="min-h-0 flex-1 space-y-1 overflow-y-auto">
            {nodes.map((n) => (
              <li key={n.id} className="group relative">
                <button
                  onClick={() => void selectNode(n.id)}
                  disabled={locked}
                  className={`w-full rounded-card border px-3 py-2.5 text-left transition-colors duration-[140ms] disabled:cursor-not-allowed ${
                    n.id === selectedNodeId
                      ? "border-accent/50 bg-accent-wash"
                      : "border-border bg-bg-surface hover:bg-bg-elevated"
                  }`}
                >
                  <div className="flex items-center justify-between gap-3 pr-6">
                    <span className="truncate text-sm">{n.name}</span>
                    <span className="shrink-0 rounded-input bg-bg-elevated px-1.5 py-0.5 font-mono text-2xs text-text-secondary">
                      {n.protocol}
                    </span>
                  </div>
                  <div className="mt-0.5 pr-6 font-mono text-2xs text-text-muted">
                    {n.endpoint}
                  </div>
                </button>
                <button
                  aria-label={`Delete ${n.name}`}
                  onClick={() => void deleteNode(n.id)}
                  disabled={locked}
                  className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-text-muted opacity-0 transition-opacity duration-[140ms] hover:text-danger group-hover:opacity-100 disabled:hidden"
                >
                  <Trash2 size={13} />
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </main>
  );
}
