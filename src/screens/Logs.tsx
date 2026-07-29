import { useEffect, useRef, useState } from "react";
import { ClipboardCopy, Trash2 } from "lucide-react";
import { useAppStore } from "../state/store";
import { useT } from "../i18n";
import type { LogLevel, LogLine } from "../ipc/types";
import { VirtualList } from "../components/VirtualList";

const LEVEL_COLOR: Record<LogLevel, string> = {
  debug: "text-text-muted",
  info: "text-text-secondary",
  warn: "text-warn",
  error: "text-danger",
};

const LEVEL_RANK: Record<LogLevel, number> = { debug: 0, info: 1, warn: 2, error: 3 };

export function Logs() {
  const t = useT();
  const logLines = useAppStore((s) => s.logLines);
  const loadLogs = useAppStore((s) => s.loadLogs);
  const clearLogs = useAppStore((s) => s.clearLogs);
  const copyLogs = useAppStore((s) => s.copyLogs);
  const [minLevel, setMinLevel] = useState<LogLevel | "all">("all");
  const [follow, setFollow] = useState(true);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void loadLogs();
  }, [loadLogs]);

  const visible: LogLine[] =
    minLevel === "all"
      ? logLines
      : logLines.filter((l) => LEVEL_RANK[l.level] >= LEVEL_RANK[minLevel]);

  // Follow tail: scroll the inner virtual container to the bottom on growth.
  useEffect(() => {
    if (follow && listRef.current) {
      const scroller = listRef.current.querySelector("[data-virtual-scroll]");
      if (scroller) scroller.scrollTop = scroller.scrollHeight;
    }
  }, [visible.length, follow]);

  return (
    <main className="flex min-h-0 flex-1 flex-col gap-3 px-6 py-5">
      <div className="flex items-center gap-2">
        <select
          value={minLevel}
          onChange={(e) => setMinLevel(e.target.value as LogLevel | "all")}
          className="rounded-input border border-border bg-bg-surface px-2 py-1.5 text-xs text-text-secondary outline-none"
        >
          <option value="all">{t("levelAll")}</option>
          <option value="info">info+</option>
          <option value="warn">warn+</option>
          <option value="error">error</option>
        </select>
        <label className="flex cursor-pointer items-center gap-1.5 text-xs text-text-secondary">
          <input
            type="checkbox"
            checked={follow}
            onChange={(e) => setFollow(e.target.checked)}
            className="accent-(--color-accent)"
          />
          {t("followTail")}
        </label>
        <div className="flex-1" />
        <button
          onClick={() => void copyLogs()}
          className="flex items-center gap-1.5 rounded-input border border-border-strong px-3 py-1.5 text-xs transition-colors duration-[140ms] hover:border-accent hover:text-accent"
        >
          <ClipboardCopy size={13} />
          {t("copy")}
        </button>
        <button
          onClick={() => void clearLogs()}
          className="flex items-center gap-1.5 rounded-input border border-border-strong px-3 py-1.5 text-xs transition-colors duration-[140ms] hover:border-danger hover:text-danger"
        >
          <Trash2 size={13} />
          {t("clear")}
        </button>
      </div>

      {visible.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-sm text-text-secondary">{t("logsEmpty")}</p>
        </div>
      ) : (
        <div ref={listRef} className="min-h-0 flex-1" dir="ltr">
          <VirtualList
            items={visible}
            itemHeight={22}
            className="h-full rounded-card border border-border bg-bg-surface px-3 py-2"
            renderItem={(line) => (
              <div
                className={`select-text truncate font-mono text-2xs leading-[22px] ${LEVEL_COLOR[line.level]}`}
                title={line.text}
              >
                {line.text}
              </div>
            )}
          />
        </div>
      )}
    </main>
  );
}
