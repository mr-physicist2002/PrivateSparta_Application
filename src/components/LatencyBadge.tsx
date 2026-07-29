export function latencyColor(ms: number): string {
  if (ms < 150) return "text-ok";
  if (ms < 400) return "text-warn";
  return "text-danger";
}

export function LatencyBadge({ ms }: { ms: number | null }) {
  if (ms === null) {
    return <span className="font-mono text-2xs text-text-muted">—</span>;
  }
  return (
    <span className={`tabular font-mono text-2xs ${latencyColor(ms)}`}>{ms} ms</span>
  );
}
