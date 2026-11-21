import { invoke, isTauri } from "@tauri-apps/api/core";
import { localDayBounds } from "./format";
import { parseFrameBoundary, parseMediaId, parseReplayTick } from "./replayTime";
import type { ClipDraft, ClipExportPreset } from "./types";

export type Attribution = "you" | "ally" | "enemy" | "unknown";
export type Availability = "available" | "partial" | "unavailable";
export const MAX_TAG_FILTERS = 40;
/** A stable, user-visible library tag.  IDs, rather than labels, are the query/write contract. */
export type Tag = { id: string; label: string; tier: number | null; builtin: boolean };
export type TagAssignment = Tag & { source: "detected" | "manual" };
export type ClipTagUpdate = { id: string; tags: TagAssignment[] };
export function replaceTagDefinition<T extends Tag>(tags: T[], replacement: Tag): T[] {
  if (!tags.some(tag => tag.id === replacement.id && tag.label !== replacement.label)) return tags;
  return tags.map(tag => tag.id === replacement.id ? { ...tag, label: replacement.label } : tag);
}
export const manualTagDefinitions = (tags: TagAssignment[]): Tag[] => tags
  .filter(tag => tag.source === "manual")
  .map(({ id, label, tier, builtin }) => ({ id, label, tier, builtin }));
export const detectedTagAssignments = (tags: TagAssignment[]) => tags.filter(tag => tag.source === "detected");

export function visibleTagAssignments(tags: TagAssignment[]): TagAssignment[] {
  const byId = new Map<string, TagAssignment>();
  let bestTier = 0;
  for (const tag of tags) {
    bestTier = Math.max(bestTier, tag.tier ?? 0);
    if (tag.source === "detected" || !byId.has(tag.id)) byId.set(tag.id, tag);
  }
  return [...byId.values()].filter(tag => tag.tier === null || tag.tier === bestTier);
}
export type ClipGame = {
  match_group: string | null; played_at: string | null; part_recorded_at: string | null;
  champion: string | null; account: string | null; queue_id: number | null; mode: string | null;
  outcome: "win" | "loss" | "unknown" | null;
  final_kda: [number | null, number | null, number | null] | null;
  summoner_spells?: [string | null, string | null];
  final_items?: Array<number | null>;
  duration_seconds?: number;
};
export type ClipHeader = {
  version: 3; id: string; source_timestamp: string;
  range: { media_id: string; start_frame: string; end_frame_exclusive: string };
  game: ClipGame; exported_at: number; preset: ClipExportPreset; duration: string;
  events: Availability; tags: TagAssignment[];
};
export type ClipCard = { header: ClipHeader; title: string; video_url: string; thumbnail_url: string; game_clock?: string };
export type GroupCursor = [number, string];
export type ClipCursor = [number, number, string];
export type ClipGroup = { key: string; clips: ClipCard[]; next: ClipCursor | null };
export type ClipPage = { groups: ClipGroup[]; next: GroupCursor | null };
export type ClipQueue = import("./leagueIndex").HistoryQueue;
export type ClipFilters = {
  search?: string; champions?: string[]; from?: string; to?: string; queue?: ClipQueue;
  outcome?: "win" | "loss"; tags?: string[];
};
export type ClipQuery = {
  match_group?: string;
  after?: GroupCursor; group_key?: string; clip_after?: ClipCursor; include_game_clock?: boolean;
};
export type ClipGridCard = Pick<ClipHeader, "id" | "duration" | "tags"> & {
  title: string; champion: string | null; thumbnail_url: string;
};
export type ClipGridCursor = [number, string];
export type ClipGridPage = { clips: ClipGridCard[]; next: ClipGridCursor | null };
export type ClipPlayback = { clip: ClipCard; file_size_bytes: number };
export type ClipDetail = { events: {
  availability: Availability; events: Array<{ replay_tick: string; event: { name?: string }; attribution: Attribution; actor: string | null }>;
} };
export type DraftSummary = { token: string; title: string; availability: Availability; detected_tags: TagAssignment[] };
export type ClipEdit = { kind: "rename"; title: string } |
  { kind: "delete" } | { kind: "reveal" };

export const usesManagedClips = () => isTauri();
export const reconcileClips = (snapshotToken: string) => invoke<boolean>("reconcile_clips", { snapshotToken });
export const readClipPage = (snapshotToken: string, query: ClipQuery) =>
  invoke<ClipPage>("clip_page", { snapshotToken, query });
export const readClipGridPage = (snapshotToken: string, { from, to, ...query }: ClipFilters, after: ClipGridCursor | null) => {
  const bounds = localDayBounds(from, to);
  return invoke<ClipGridPage>("clip_grid_page", {
    snapshotToken, after, query: { ...query, played_from: bounds.from, played_before: bounds.before },
  });
};
export const readClipGridChampions = (snapshotToken: string) =>
  invoke<string[]>("clip_grid_champions", { snapshotToken });
export const readClipPlayback = (snapshotToken: string, clipId: string) =>
  invoke<ClipPlayback>("clip_playback", { snapshotToken, clipId });
export const readClipDetail = (snapshotToken: string, clipId: string) =>
  invoke<ClipDetail>("clip_detail", { snapshotToken, clipId });
export const editManagedClip = (snapshotToken: string, clipId: string, edit: ClipEdit) =>
  invoke<void>("edit_managed_clip", { snapshotToken, clipId, edit });
export const editClipTags = (snapshotToken: string, clipId: string, tags: Tag[]) =>
  invoke<TagAssignment[]>("edit_clip_tags", { snapshotToken, clipId, tags });
export const tagChoices = (snapshotToken: string, prefix: string) =>
  invoke<Tag[]>("tag_choices", { snapshotToken, prefix });
export const createTag = (snapshotToken: string, label: string) =>
  invoke<Tag>("create_tag", { snapshotToken, label });
export const renameTag = (snapshotToken: string, tagId: string, label: string) =>
  invoke<Tag>("rename_tag", { snapshotToken, tagId, label });
export const prepareClipDraft = (snapshotToken: string, draft: ClipDraft) =>
  invoke<DraftSummary>("prepare_clip_draft", { snapshotToken, request: {
    game_timestamp: draft.gameTimestamp, media_id: draft.mediaId,
    start_frame: draft.startFrame.toString(), end_frame_exclusive: draft.endFrameExclusive.toString(),
  } });

export function clipTitleError(title: string): string | undefined {
  if (!title || title.length > 120 || /[<>:"/\\|?*\p{Cc}]|\.$|\.mp4$/iu.test(title))
    return "Enter a title of up to 120 characters without a file extension or Windows filename characters.";
  if (/^(CON|PRN|AUX|NUL|CONIN\$|CONOUT\$|(?:COM|LPT)[0-9¹²³])$/i.test(title.split(".")[0].trimEnd()))
    return "This name is reserved by Windows. Enter a different title.";
}
export const clipDurationMs = (duration: string) => Number(parseReplayTick(duration)) / 48_000;
export const sourcePosition = (clip: ClipCard): ClipDraft => ({
  gameTimestamp: clip.header.source_timestamp,
  mediaId: parseMediaId(clip.header.range.media_id),
  startFrame: parseFrameBoundary(clip.header.range.start_frame, "clip start"),
  endFrameExclusive: parseFrameBoundary(clip.header.range.end_frame_exclusive, "clip end"),
});
