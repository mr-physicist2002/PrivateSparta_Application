import { useAppStore } from "../state/store";
import { interpolate, useT } from "../i18n";

export function ImportPreviewModal() {
  const t = useT();
  const preview = useAppStore((s) => s.importPreview);
  const cancel = useAppStore((s) => s.cancelImport);
  const commit = useAppStore((s) => s.commitImport);
  if (!preview) return null;

  return (
    <div className="absolute inset-0 z-10 flex items-center justify-center bg-black/60">
      <div className="w-96 rounded-modal border border-border-strong bg-bg-surface p-5">
        <h2 className="font-display text-lg font-semibold">
          {preview.nodes.length === 1
            ? t("foundServer")
            : interpolate(t("foundServers"), preview.nodes.length)}
        </h2>
        {preview.skipped > 0 ? (
          <p className="mt-1 text-xs text-warn">
            {preview.skipped === 1
              ? t("entrySkipped")
              : interpolate(t("entriesSkipped"), preview.skipped)}
          </p>
        ) : null}
        <ul className="mt-3 max-h-48 space-y-1 overflow-y-auto">
          {preview.nodes.map((n) => (
            <li
              key={n.id}
              className="flex items-center justify-between gap-3 rounded-input bg-bg-elevated px-3 py-2 text-sm"
            >
              <span className="truncate">{n.name}</span>
              <span className="shrink-0 font-mono text-2xs text-text-muted">
                {n.protocol}
              </span>
            </li>
          ))}
        </ul>
        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={cancel}
            className="rounded-input px-4 py-1.5 text-sm text-text-secondary transition-colors duration-[140ms] hover:text-text-primary"
          >
            {t("cancel")}
          </button>
          <button
            onClick={() => void commit()}
            className="rounded-input bg-accent px-4 py-1.5 text-sm font-medium text-bg-base transition-colors duration-[140ms] hover:bg-accent-hover"
          >
            {t("import")}
          </button>
        </div>
      </div>
    </div>
  );
}
