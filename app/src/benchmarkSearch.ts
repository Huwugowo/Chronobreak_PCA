import { replayBenchmarkObserver, type ReplayBenchmarkObserver } from "./benchmark";
import type { HistoryController, HistoryQueryObservation } from "./historyController";

// Imports publish their final progress before scheduling the last refresh query.
// Recheck after that synchronous stack so transient idle state cannot admit work.
export const waitForBenchmarkSearchReady = (history: Pick<HistoryController, "value" | "subscribe">, signal: AbortSignal) =>
  new Promise<void>((resolve, reject) => {
    let unsubscribe = () => {};
    let finished = false;
    const finish = (error?: Error) => {
      if (finished) return;
      finished = true;
      clearTimeout(timer); unsubscribe(); signal.removeEventListener("abort", abort);
      if (error) reject(error); else resolve();
    };
    const abort = () => finish(new Error("search preconditioning disposed"));
    const timer = setTimeout(() => finish(new Error("search preconditioning timed out")), 120_000);
    const check = () => queueMicrotask(() => {
      if (finished) return;
      const state = history.value;
      if (state.error || state.indexIssue) finish(new Error("search preconditioning failed"));
      else if (state.page && !state.indexing && !state.loading) finish();
    });
    unsubscribe = history.subscribe(check);
    signal.addEventListener("abort", abort, { once: true });
    if (signal.aborted) abort(); else check();
  });

const inputActions = new WeakMap<HTMLInputElement, string>();
export const waitForBenchmarkLibraryReturn = (history: Pick<HistoryController, "value">, signal: AbortSignal) =>
  new Promise<number>((resolve, reject) => {
    let frame: number | undefined, finished = false;
    const finish = (error?: Error, count = 0) => {
      if (finished) return;
      finished = true; clearTimeout(timer);
      if (frame !== undefined) cancelAnimationFrame(frame);
      signal.removeEventListener("abort", abort);
      if (error) reject(error); else resolve(count);
    };
    const abort = () => finish(new Error("library return disposed"));
    const timer = setTimeout(() => finish(new Error("library return timed out")), 10_000);
    const check = () => {
      if (finished) return;
      const state = history.value;
      const input = document.querySelector<HTMLInputElement>('input[aria-label="Search matches"]');
      if (state.error || state.indexIssue) { finish(new Error("library return failed")); return; }
      if (!state.page || state.indexing || state.loading || !input?.isConnected || input.value !== "") {
        frame = requestAnimationFrame(check); return;
      }
      const page = state.page;
      frame = requestAnimationFrame(() => {
        const current = history.value;
        if (current.page !== page || current.loading || current.indexing || !input.isConnected) {
          frame = requestAnimationFrame(check); return;
        }
        const count = page.rows.length;
        if (count !== 2) finish(new Error("library return did not restore two settled games"));
        else finish(undefined, count);
      });
    };
    signal.addEventListener("abort", abort, { once: true });
    if (signal.aborted) abort(); else frame = requestAnimationFrame(check);
  });

export const recordBenchmarkSearchInput = (input: HTMLInputElement): string | undefined => {
  const observer = replayBenchmarkObserver();
  if (!observer || !["library_search", "whole_app"].includes(observer.scenario().kind)) return;
  const actionId = observer.nextActionId("search");
  inputActions.set(input, actionId);
  observer.emit("library_search_input", { text_length: input.value.length }, { actionId, required: true });
  return actionId;
};

export const recordBenchmarkSearchSchedule = (kind: "scheduled" | "coalesced", actionId?: string, delayMs = 0) => {
  if (!actionId) return;
  replayBenchmarkObserver()?.emit(`library_search_${kind}`, { delay_ms: delayMs }, { actionId, required: true });
};

// Called after checked result admission. A paint opportunity is deliberately
// distinct from authoritative video presentation or physical screen display.
export const observeBenchmarkHistory = (event: HistoryQueryObservation, current: (generation: number) => boolean) => {
  const observer = replayBenchmarkObserver();
  if (!observer || !event.actionId) return;
  observer.emit(event.kind, { query_generation: event.generation, row_count: event.rowCount ?? null },
    { actionId: event.actionId, required: true });
  if (event.kind !== "history_query_admitted" || !event.actionId) return;
  const actionId = event.actionId;
  requestAnimationFrame(() => requestAnimationFrame(() => {
    observer.emit(current(event.generation) ? "library_search_painted" : "library_search_paint_superseded",
      { query_generation: event.generation, row_count: event.rowCount ?? null, authority: "two-animation-frame-paint-opportunity" },
      { actionId, required: true });
  }));
};

const delay = (ms: number, signal: AbortSignal) => new Promise<void>((resolve, reject) => {
  const abort = () => { clearTimeout(timer); reject(new Error("library search disposed")); };
  const timer = setTimeout(() => { signal.removeEventListener("abort", abort); resolve(); }, ms);
  if (signal.aborted) abort();
  else signal.addEventListener("abort", abort, { once: true });
});

// Equal quiet library windows; process samples are labelled by the runner when
// it observes these flushed markers, without assuming shared clock origins.
export const waitForBenchmarkMemoryWindow = async (observer: ReplayBenchmarkObserver,
  history: Pick<HistoryController, "value">, cycle: number, signal: AbortSignal) => {
  await waitForBenchmarkLibraryReturn(history, signal);
  const page = history.value.page;
  const check = () => {
    if (signal.aborted) throw new Error("library search disposed");
    const state = history.value;
    const input = document.querySelector<HTMLInputElement>('input[aria-label="Search matches"]');
    if (state.page !== page || state.loading || state.indexing || state.error || state.indexIssue ||
        !input?.isConnected || input.value !== "" || document.querySelector("video")) {
      throw new Error("memory window left its settled library");
    }
  };
  check();
  await delay(5_000, signal);
  check();
  observer.emit("memory_window_started", { cycle, settle_ms: 5_000, sample_ms: 10_000 }, { required: true });
  await observer.flush();
  for (let n = 0; n < 10; n++) { await delay(1_000, signal); check(); }
  observer.emit("memory_window_completed", { cycle }, { required: true });
  await observer.flush();
};

// This named diagnostic deliberately keeps the normal importer running. Its
// refresh queries coalesce behind active input; explicit replacements remain
// measured as superseded, never called cancelled.
export const runBenchmarkIndexSearch = async (observer: ReplayBenchmarkObserver,
  history: Pick<HistoryController, "value" | "subscribe">, term: string, catalogCount: number,
  parentSignal: AbortSignal) => {
  const control = new AbortController();
  const abort = () => control.abort();
  parentSignal.addEventListener("abort", abort, { once: true });
  if (parentSignal.aborted) abort();
  const timer = setTimeout(abort, 110_000);
  const started = performance.now();
  let indexingObserved = false, lastProgress = "";
  const progress = () => {
    if (control.signal.aborted) return;
    const state = history.value;
    indexingObserved ||= state.indexing;
    const payload = { indexing: state.indexing,
      indexed_recordings: state.page?.indexed_recordings ?? 0,
      catalog_recordings: state.page?.catalog_recordings ?? 0,
      unknown_recordings: state.page?.unknown_recordings ?? 0 };
    const key = JSON.stringify(payload);
    if (key !== lastProgress) {
      lastProgress = key;
      observer.emit("indexing_diagnostic_progress", payload, { required: true });
    }
  };
  const unsubscribe = history.subscribe(progress);
  try {
    if (control.signal.aborted) throw new Error("library search disposed");
    observer.emit("search_preconditioning_started", {}, { required: true });
    observer.emit("indexing_diagnostic_started", { indexing: history.value.indexing }, { required: true });
    progress();
    let input: HTMLInputElement | null = null;
    while (!(input = document.querySelector<HTMLInputElement>('input[aria-label="Search matches"]'))) {
      if (performance.now() - started > 10_000) throw new Error("indexing search input unavailable");
      await delay(16, control.signal);
    }
    if (control.signal.aborted) throw new Error("library search disposed");
    observer.emit("search_preconditioning_ready", { catalog_count: catalogCount, iterations: 1,
      indexing: history.value.indexing }, { required: true });
    await runBenchmarkSearchSequence(observer, input, Array.from(term, (_, i) => term.slice(0, i + 1)), "typing", control.signal);
    await runBenchmarkSearchSequence(observer, input, ["zzzzzz"], "settled", control.signal);
    await runBenchmarkSearchSequence(observer, input, [""], "clear", control.signal);
    await waitForBenchmarkSearchReady(history, control.signal);
    if (control.signal.aborted) throw new Error("library search disposed");
    progress();
    const result = { indexing_observed: indexingObserved, import_completed: !history.value.indexing,
      elapsed_ms: performance.now() - started, indexed_recordings: history.value.page?.indexed_recordings ?? 0,
      catalog_recordings: history.value.page?.catalog_recordings ?? 0, search_sequences: 3 };
    observer.emit("indexing_diagnostic_completed", result, { required: true });
    return result;
  } finally {
    clearTimeout(timer); unsubscribe(); parentSignal.removeEventListener("abort", abort);
    control.abort();
  }
};

export const runBenchmarkSearchSequence = async (observer: ReplayBenchmarkObserver, input: HTMLInputElement,
  values: readonly string[], stratum: "typing" | "settled" | "clear", signal: AbortSignal) => {
  if (!values.length || values.length > 64 || values.some(value => value.length > 64)) throw new Error("invalid search sequence");
  let finalAction: string | undefined;
  let finish!: () => void;
  let fail!: (error: Error) => void;
  const painted = new Promise<void>((resolve, reject) => { finish = resolve; fail = reject; });
  const unsubscribe = observer.subscribeEvents(event => {
    if (!finalAction || event.action_id !== finalAction) return;
    if (event.kind === "library_search_painted") finish();
    if (["history_query_error", "history_query_superseded", "library_search_paint_superseded", "history_query_replaced"].includes(event.kind)) {
      if (observer.scenario().id === "cold-index-search" && event.kind !== "history_query_error") finish();
      else fail(new Error(`search action failed: ${event.kind}`));
    }
  });
  const abort = () => fail(new Error("library search disposed"));
  const timer = setTimeout(() => fail(new Error("library search paint timed out")), 10_000);
  signal.addEventListener("abort", abort, { once: true });
  // Handle rejection immediately, including during inter-character pacing.
  const outcome = painted.then(() => null, error => error as Error);
  try {
    if (signal.aborted) throw new Error("library search disposed");
    for (const [index, value] of values.entries()) {
      if (signal.aborted) throw new Error("library search disposed");
      if (!input.isConnected) throw new Error("search input was unmounted");
      input.value = value;
      input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText" }));
      if (signal.aborted) throw new Error("library search disposed");
      const actionId = inputActions.get(input);
      if (!actionId) throw new Error("production search input did not record its action");
      if (index === values.length - 1) {
        finalAction = actionId;
        observer.emit("library_search_sequence", { stratum, input_count: values.length }, { actionId, required: true });
      } else await delay(60, signal);
    }
    const error = await outcome;
    if (signal.aborted) throw new Error("library search disposed");
    if (error) throw error;
  } finally {
    clearTimeout(timer);
    unsubscribe();
    signal.removeEventListener("abort", abort);
  }
};
