/** 41 → "0:41", 3725 → "1:02:05". */
export function clock(totalSecs: number): string {
  const s = Math.max(0, Math.round(totalSecs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

/** "just now" / "42s ago" / "12m ago" / "3h ago" / a date. */
export function ago(ms: number, now = Date.now()): string {
  const secs = Math.floor((now - ms) / 1000);
  if (secs < 10) return "just now";
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** 15248112640 → "14.2 GB". */
export function bytes(n: number): string {
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

/** Last path segment, for either slash style. */
/** yt-dlp's extractor names, as people say them: "YoutubeTab" → "YouTube". */
export function siteName(site: string): string {
  if (site.startsWith("Youtube")) return "YouTube";
  if (site.startsWith("Twitch")) return "Twitch";
  if (site.startsWith("Soundcloud")) return "SoundCloud";
  return site;
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}
