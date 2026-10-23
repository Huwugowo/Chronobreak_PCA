import type { PlaybackEvent } from "./playbackController";

// Benchmark settlement is stricter than the production seek promise: native
// completion and authoritative presentation are independent observations.
export const createBenchmarkSeekEvidence = (generation: number, actionId: string) => {
  let dispatched = false;
  let seeked = false;
  let presented = false;
  let resolve!: () => void;
  let reject!: (reason: Error) => void;
  const complete = new Promise<void>((done, fail) => { resolve = done; reject = fail; });
  const observe = (event: PlaybackEvent) => {
    if (event.generation !== generation) {
      reject(new Error("benchmark seek generation changed"));
      return;
    }
    if (["viewer_disposed", "media_error", "recovery_started"].includes(event.kind)) {
      reject(new Error(`benchmark seek interrupted: ${event.kind}`));
      return;
    }
    if (event.kind === "seek_requested" && event.actionId !== actionId) {
      reject(new Error("benchmark seek replaced by a newer intent"));
      return;
    }
    if (event.actionId !== actionId) return;
    if (["action_cancelled_superseded", "action_failed", "seek_timeout"].includes(event.kind)) {
      reject(new Error(`benchmark seek did not settle: ${event.kind}`));
      return;
    }
    // A deduplicated request has its own explicit terminal outcome and does not
    // claim a new native seek or frame.
    if (event.kind === "seek_deduped") resolve();
    if (event.kind === "seek_dispatched") dispatched = true;
    if (event.kind === "seeked") seeked = true;
    if (event.kind === "seek_presented" && event.payload.authoritative === true) presented = true;
    if (dispatched && seeked && presented) resolve();
  };
  return { complete, observe };
};
