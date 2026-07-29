import { useMemo, useState } from "react";
import {
  Activity,
  ClipboardPaste,
  Link,
  Search,
  Star,
  Trash2,
  X,
} from "lucide-react";
import { useAppStore } from "../state/store";
import { useT } from "../i18n";
import type { NodeView } from "../ipc/types";
import { VirtualList } from "../components/VirtualList";
import { LatencyBadge } from "../components/LatencyBadge";

type SortKey = "added" | "name" | "latency";

interface Row {
  kind: "header" | "node";
  key: string;
  title?: string;
  count?: number;
  node?: NodeView;
  deletable?: boolean;
}

export function Servers() {
  const t = useT();
  const store = useAppStore();
  const {
    manualNodes,
    subscriptions,
    selectedNodeId,
    connection,
    testing,
    testAll,
    testOne,
    cancelTest,
    selectNode,
    deleteNode,
    toggleFavorite,
    copyNodeLink,
    previewImport,
  } = store;
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<SortKey>("added");
  const [protocolFilter, setProtocolFilter] = useState<string>("all");
  const locked = connection.state !== "disconnected" && connection.state !== "error";

  const protocols = useMemo(() => {
    const set = new Set<string>();
    manualNodes.forEach((n) => set.add(n.protocol));
    subscriptions.forEach((s) => s.nodes.forEach((n) => set.add(n.protocol)));
    return Array.from(set).sort();
  }, [manualNodes, subscriptions]);

  const rows = useMemo<Row[]>(() => {
    const q = query.trim().toLowerCase();
    const matches = (n: NodeView) =>
      (protocolFilter === "all" || n.protocol === protocolFilter) &&
      (q === "" || n.name.toLowerCase().includes(q));
    const order = (nodes: NodeView[]) => {
      const filtered = nodes.filter(matches);
      // Favorites first within each group, then the chosen sort.
      const rank = (n: NodeView) => (n.favorite ? 0 : 1);
      return filtered.sort((a, b) => {
        if (rank(a) !== rank(b)) return rank(a) - rank(b);
        if (sort === "name") return a.name.localeCompare(b.name);
        if (sort === "latency") {
          const la = a.latencyMs ?? Number.MAX_SAFE_INTEGER;
          const lb = b.latencyMs ?? Number.MAX_SAFE_INTEGER;
          return la - lb;
        }
        return 0; // "added": keep stored order
      });
    };

    const out: Row[] = [];
    if (manualNodes.length > 0) {
      const nodes = order([...manualNodes]);
      if (nodes.length > 0) {
        out.push({
          kind: "header",
          key: "h-manual",
          title: t("manualGroup"),
          count: nodes.length,
        });
        nodes.forEach((n) =>
          out.push({ kind: "node", key: n.id, node: n, deletable: true }),
        );
      }
    }
    for (const sub of subscriptions) {
      const nodes = order([...sub.nodes]);
      if (nodes.length > 0) {
        out.push({
          kind: "header",
          key: `h-${sub.id}`,
          title: sub.name,
          count: nodes.length,
        });
        nodes.forEach((n) => out.push({ kind: "node", key: n.id, node: n }));
      }
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [manualNodes, subscriptions, query, sort, protocolFilter, t]);

  return (
    <main className="flex min-h-0 flex-1 flex-col gap-3 px-6 py-5">
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search
            size={14}
            className="absolute start-2.5 top-1/2 -translate-y-1/2 text-text-muted rtl:translate-x-0"
          />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("searchServers")}
            className="w-full rounded-input border border-border bg-bg-surface py-1.5 pe-3 ps-8 text-sm outline-none placeholder:text-text-muted focus:border-border-strong"
          />
        </div>
        <select
          value={protocolFilter}
          onChange={(e) => setProtocolFilter(e.target.value)}
          className="rounded-input border border-border bg-bg-surface px-2 py-1.5 text-xs text-text-secondary outline-none"
        >
          <option value="all">{t("allProtocols")}</option>
          {protocols.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </select>
        <select
          value={sort}
          onChange={(e) => setSort(e.target.value as SortKey)}
          className="rounded-input border border-border bg-bg-surface px-2 py-1.5 text-xs text-text-secondary outline-none"
        >
          <option value="added">{t("sortAdded")}</option>
          <option value="name">{t("sortName")}</option>
          <option value="latency">{t("sortLatency")}</option>
        </select>
        {testing.running ? (
          <button
            onClick={() => void cancelTest()}
            className="flex items-center gap-1.5 rounded-input border border-warn/50 px-3 py-1.5 text-xs text-warn transition-colors duration-[140ms] hover:bg-bg-elevated"
          >
            <X size={13} />
            <span className="tabular font-mono">
              {testing.done}/{testing.total}
            </span>
          </button>
        ) : (
          <button
            onClick={() => void testAll()}
            className="flex items-center gap-1.5 rounded-input border border-border-strong px-3 py-1.5 text-xs transition-colors duration-[140ms] hover:border-accent hover:text-accent"
          >
            <Activity size={13} />
            {t("testAll")}
          </button>
        )}
      </div>

      {rows.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 text-center">
          <p className="text-sm text-text-secondary">
            {query || protocolFilter !== "all" ? t("nothingMatches") : t("emptyHint")}
          </p>
          {!query && protocolFilter === "all" ? (
            <button
              onClick={() => void previewImport()}
              className="flex items-center gap-2 rounded-input border border-border-strong px-4 py-2 text-sm font-medium transition-colors duration-[140ms] hover:border-accent hover:text-accent"
            >
              <ClipboardPaste size={15} />
              {t("importFromClipboard")}
            </button>
          ) : null}
        </div>
      ) : (
        <VirtualList
          items={rows}
          itemHeight={52}
          className="min-h-0 flex-1 -mr-2 pr-2"
          renderItem={(row) =>
            row.kind === "header" ? (
              <div className="flex h-full items-end justify-between px-1 pb-1.5">
                <span className="text-2xs font-medium uppercase tracking-wide text-text-muted">
                  {row.title}
                </span>
                <span className="tabular font-mono text-2xs text-text-muted">
                  {row.count}
                </span>
              </div>
            ) : row.node ? (
              <NodeRow
                node={row.node}
                selected={row.node.id === selectedNodeId}
                locked={locked}
                deletable={row.deletable ?? false}
                onSelect={() => void selectNode(row.node!.id)}
                onTest={() => void testOne(row.node!.id)}
                onCopy={() => void copyNodeLink(row.node!.id)}
                onFavorite={() => void toggleFavorite(row.node!.id)}
                onDelete={() => void deleteNode(row.node!.id)}
              />
            ) : null
          }
        />
      )}
    </main>
  );
}

function NodeRow(props: {
  node: NodeView;
  selected: boolean;
  locked: boolean;
  deletable: boolean;
  onSelect: () => void;
  onTest: () => void;
  onCopy: () => void;
  onFavorite: () => void;
  onDelete: () => void;
}) {
  const t = useT();
  const { node, selected, locked, deletable } = props;
  return (
    <div
      className={`group mb-1 flex h-[48px] items-center gap-2 rounded-card border px-3 transition-colors duration-[140ms] ${
        selected
          ? "border-accent/50 bg-accent-wash"
          : "border-border bg-bg-surface hover:bg-bg-elevated"
      }`}
    >
      <button
        onClick={props.onSelect}
        disabled={locked}
        className="flex min-w-0 flex-1 items-center gap-2 text-start disabled:cursor-not-allowed"
      >
        <span className="truncate text-sm">{node.name}</span>
        <span className="shrink-0 rounded-input bg-bg-elevated px-1.5 py-0.5 font-mono text-2xs text-text-secondary">
          {node.protocol}
        </span>
      </button>
      <LatencyBadge ms={node.latencyMs} />
      <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-[140ms] group-hover:opacity-100">
        <button
          aria-label={t("testLatency")}
          title={t("testLatency")}
          onClick={props.onTest}
          className="rounded p-1.5 text-text-muted hover:text-accent"
        >
          <Activity size={13} />
        </button>
        <button
          aria-label={t("copyLink")}
          title={t("copyLink")}
          onClick={props.onCopy}
          className="rounded p-1.5 text-text-muted hover:text-accent"
        >
          <Link size={13} />
        </button>
        {deletable ? (
          <button
            aria-label={t("delete")}
            title={t("delete")}
            onClick={props.onDelete}
            disabled={locked}
            className="rounded p-1.5 text-text-muted hover:text-danger disabled:opacity-30"
          >
            <Trash2 size={13} />
          </button>
        ) : null}
      </div>
      <button
        aria-label={node.favorite ? t("unpin") : t("pin")}
        title={node.favorite ? t("unpin") : t("pin")}
        onClick={props.onFavorite}
        className={`shrink-0 rounded p-1.5 transition-colors duration-[140ms] ${
          node.favorite
            ? "text-accent"
            : "text-text-muted opacity-0 hover:text-accent group-hover:opacity-100"
        }`}
      >
        <Star size={13} fill={node.favorite ? "currentColor" : "none"} />
      </button>
    </div>
  );
}
