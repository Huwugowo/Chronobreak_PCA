export const formatBytes = (bytes: number): string => {
  if (bytes < 1_000) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1_000;
  let unit = units[0];
  for (let index = 1; index < units.length && value >= 1_000; index += 1) {
    value /= 1_000;
    unit = units[index];
  }
  return `${value >= 10 ? value.toFixed(1) : value.toFixed(2)} ${unit}`;
};

export const formatDuration = (milliseconds: number): string => {
  const totalSeconds = Math.max(0, Math.floor(milliseconds / 1_000));
  const hours = Math.floor(totalSeconds / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  return hours > 0
    ? `${hours}:${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`
    : `${minutes}:${seconds.toString().padStart(2, "0")}`;
};

export const formatMatchDuration = (durationMs: number): string => {
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}m ${seconds}s`;
};

export const formatKdaRatio = (kills: number | null, deaths: number | null, assists: number | null): string | null => {
  if (kills === null || deaths === null || assists === null) return null;
  if (deaths === 0) return "PERFECT KDA";
  return `${((kills + assists) / deaths).toFixed(2)}:1 KDA`;
};

export const formatDate = (value: string, compact = false): string => {
  if (!value) return "Date unavailable";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Date unavailable";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    ...(compact ? {} : { hour: "2-digit", minute: "2-digit" }),
  }).format(date);
};

export const formatTimestampDate = (timestamp: string): string => {
  const seconds = Number(timestamp);
  return Number.isFinite(seconds) ? formatDate(new Date(seconds * 1_000).toISOString()) : "Unknown";
};

export function localDayBounds(from?: string, to?: string) {
  const midnight = (date: string, offset: number) => {
    const [year, month, day] = date.split("-").map(Number);
    return new Date(year, month - 1, day + offset).getTime() / 1000;
  };
  return { from: from ? midnight(from, 0) : undefined, before: to ? midnight(to, 1) : undefined };
}
