import type { HistoryQueue } from "./leagueIndex";

export const queueLabels: Record<HistoryQueue, string> = {
  ranked_solo_duo: "Ranked Solo/Duo", ranked_flex: "Ranked Flex", ranked_5s: "Ranked 5s",
  aram_mayhem: "ARAM: Mayhem", aram: "ARAM", aram_mayhem_classic: "ARAM: Mayhem Classic-ish",
  arena: "Arena", other: "All other queues", normal_draft: "Normal Draft", swiftplay: "Swiftplay", urf: "URF",
};

// The same queue IDs as HistoryQueue::ids in league_index/query.rs.
const historyQueueById: Partial<Record<number, HistoryQueue>> = {
  4: "ranked_solo_duo", 420: "ranked_solo_duo", 440: "ranked_flex", 42: "ranked_5s", 710: "ranked_5s",
  2400: "aram_mayhem", 65: "aram", 100: "aram", 450: "aram", 2450: "aram_mayhem_classic", 3280: "aram_mayhem_classic",
  1700: "arena", 1710: "arena", 1740: "arena", 1750: "arena",
  14: "normal_draft", 400: "normal_draft", 480: "swiftplay", 76: "urf", 1900: "urf",
};

export function matchQueueLabel(queueId: number | null, mode: string | null): string | null {
  const queue = queueId === null ? undefined : historyQueueById[queueId];
  if (queue) return queueLabels[queue];
  const recordedMode = mode?.trim();
  if (recordedMode === "CLASSIC") return "Classic";
  if (recordedMode === "PRACTICETOOL") return "Practice Tool";
  return recordedMode || (queueId === null ? null : `Queue ${queueId}`);
}
