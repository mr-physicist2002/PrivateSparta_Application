const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 B";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value >= 100 || unit === 0 ? Math.round(value) : value.toFixed(1)} ${UNITS[unit]}`;
}

export function formatSpeed(bps: number): string {
  return `${formatBytes(bps)}/s`;
}

export function formatDuration(totalSeconds: number): string {
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = Math.floor(totalSeconds % 60);
  const mm = String(m).padStart(2, "0");
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

export function formatExpiry(unixSeconds: number): string {
  const date = new Date(unixSeconds * 1000);
  return date.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export interface RelativeStrings {
  justNow: string;
  minutesAgo: string;
  hoursAgo: string;
  daysAgo: string;
}

export function formatRelative(unixSeconds: number, strings: RelativeStrings): string {
  const delta = Math.floor(Date.now() / 1000) - unixSeconds;
  const put = (template: string, n: number) => template.replace("{n}", String(n));
  if (delta < 60) return strings.justNow;
  if (delta < 3600) return put(strings.minutesAgo, Math.floor(delta / 60));
  if (delta < 86400) return put(strings.hoursAgo, Math.floor(delta / 3600));
  return put(strings.daysAgo, Math.floor(delta / 86400));
}
