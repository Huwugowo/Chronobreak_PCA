import { invoke, isTauri } from "@tauri-apps/api/core";
import { parseFrameBoundary, parseMediaId, parseReplayTick } from "./replayTime";
import type { ClipDraft, ClipExportPreset } from "./types";

export type Attribution = "you" | "ally" | "enemy" | "unknown";
export type Availability = "available" | "partial" | "unavailable";
export type HighlightKind = "double_kill" | "triple_kill" | "quadra_kill" | "pentakill"
  | "first_blood" | "dragon_steal" | "baron_steal" | "herald_steal";
export type Highlight = { event_index: number; kind: HighlightKind; attribution: Attribution; actor: string | null };
export type ClipGame = {
  match_group: string | null; played_at: string | null; part_recorded_at: string | null;
  champion: string | null; account: string | null; queue_id: number | null; mode: string | null;
  outcome: "win" | "loss" | "unknown" | null; final_kda: [number, number, number] | null;
};
export type ClipHeader = {
  version: 1; id: string; source_timestamp: string;
  range: { media_id: string; start_frame: string; end_frame_exclusive: string };
  game: ClipGame; exported_at: number; preset: ClipExportPreset; duration: string;
  events: Availability; highlights: Highlight[]; custom_tags: string[];
};
export type ClipCard = { header: ClipHeader; title: string; video_url: string; thumbnail_url: string };
export type GroupCursor = [number, string];
export type ClipCursor = [number, number, string];
export type ClipGroup = { key: string; clips: ClipCard[]; next: ClipCursor | null };
export type ClipPage = { groups: ClipGroup[]; next: GroupCursor | null };
export type ClipQuery = {
  search?: string; champion?: string; from?: string; to?: string; queue?: string;
  outcome?: string; highlight?: string; actor?: Attribution; match_group?: string;
  after?: GroupCursor; group_key?: string; clip_after?: ClipCursor;
};
export type ClipDetail = { header: ClipHeader; candidates: Highlight[]; events: {
  availability: Availability; events: Array<{ replay_tick: string; event: { name?: string }; attribution: Attribution; actor: string | null }>;
} };
export type DraftSummary = { token: string; title: string; availability: Availability; candidates: Highlight[] };
export type ClipEdit = { kind: "rename"; title: string } |
  { kind: "tags"; custom_tags: string[]; highlights: number[] } | { kind: "delete" } | { kind: "reveal" };

export const usesManagedClips = () => isTauri();
export const reconcileClips = (snapshotToken: string) => invoke<boolean>("reconcile_clips", { snapshotToken });
export const readClipPage = (snapshotToken: string, query: ClipQuery) =>
  invoke<ClipPage>("clip_page", { snapshotToken, query });
export const readClipDetail = (snapshotToken: string, clipId: string) =>
  invoke<ClipDetail>("clip_detail", { snapshotToken, clipId });
export const editManagedClip = (snapshotToken: string, clipId: string, edit: ClipEdit) =>
  invoke<void>("edit_managed_clip", { snapshotToken, clipId, edit });
export const readClipTagSuggestions = (snapshotToken: string, prefix: string) =>
  invoke<string[]>("clip_tag_suggestions", { snapshotToken, prefix });
export const prepareClipDraft = (snapshotToken: string, draft: ClipDraft) =>
  invoke<DraftSummary>("prepare_clip_draft", { snapshotToken, request: {
    game_timestamp: draft.gameTimestamp, media_id: draft.mediaId,
    start_frame: draft.startFrame.toString(), end_frame_exclusive: draft.endFrameExclusive.toString(),
  } });

export const clipLabels: Record<HighlightKind, string> = {
  double_kill: "Double kill", triple_kill: "Triple kill", quadra_kill: "Quadra kill", pentakill: "Pentakill",
  first_blood: "First Blood", dragon_steal: "Dragon steal", baron_steal: "Baron steal", herald_steal: "Herald steal",
};
export const actorLabels: Record<Attribution, string> = { you: "You", ally: "Ally", enemy: "Enemy", unknown: "Unattributed" };
export function clipTitleError(title: string): string | undefined {
  if (!title || title.length > 120 || /[<>:"/\\|?*\p{Cc}]|\.$|\.mp4$/iu.test(title))
    return "Enter a title of up to 120 characters without a file extension or Windows filename characters.";
  if (/^(CON|PRN|AUX|NUL|CONIN\$|CONOUT\$|(?:COM|LPT)[0-9¹²³])$/i.test(title.split(".")[0].trimEnd()))
    return "This name is reserved by Windows. Enter a different title.";
}
export const highlightLabel = (tag: Highlight) => [clipLabels[tag.kind], actorLabels[tag.attribution], tag.attribution !== "you" ? tag.actor : null]
  .filter(Boolean).join(" · ");
export const clipDurationMs = (clip: ClipCard) => Number(parseReplayTick(clip.header.duration)) / 48_000;
export const sourcePosition = (clip: ClipCard): ClipDraft => ({
  gameTimestamp: clip.header.source_timestamp,
  mediaId: parseMediaId(clip.header.range.media_id),
  startFrame: parseFrameBoundary(clip.header.range.start_frame, "clip start"),
  endFrameExclusive: parseFrameBoundary(clip.header.range.end_frame_exclusive, "clip end"),
});
