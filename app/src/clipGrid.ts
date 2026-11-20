import { batch, createEffect, createMemo, on, onCleanup, type Accessor } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { libraryError } from "./libraryController";
import { readClipGridPage, replaceTagDefinition, type ClipFilters, type ClipGridCard, type ClipGridCursor, type ClipTagUpdate, type Tag } from "./managedClips";

type GridSource = { root: string | null; token: string; ready: boolean; query: ClipFilters };
type Operation = "refresh" | "append";

export function createClipGrid(source: Accessor<GridSource>) {
  const [state, setState] = createStore({
    clips: [] as ClipGridCard[], next: null as ClipGridCursor | null,
    token: undefined as string | undefined, busy: false,
    failure: undefined as { message: string; operation: Operation } | undefined,
  });
  let depth = 1;
  let generation = 0;
  let read = Promise.resolve();

  const current = createMemo(() => {
    const { ready, token } = source();
    return ready && state.token === token;
  });
  const load = (operation: Operation) => {
    const { ready, token, query } = source();
    const append = operation === "append";
    const start = append ? state.next : null;
    if (!ready || state.busy || (append && (!current() || !start))) return Promise.resolve(undefined);

    const request = generation;
    const pages = append ? 1 : depth;
    setState("busy", true);
    // Finish the current read before issuing one for a superseding catalog or filter.
    const result = read.then(async () => {
      if (request !== generation) return;
      try {
        const clips: ClipGridCard[] = [];
        let cursor = start;
        for (let i = 0; i < pages; i++) {
          const page = await readClipGridPage(token, query, cursor);
          if (request !== generation) return;
          clips.push(...page.clips);
          cursor = page.next;
          if (!cursor) break;
        }
        const firstNew = state.clips.length;
        batch(() => {
          if (append) {
            for (const [index, clip] of clips.entries()) setState("clips", firstNew + index, clip);
            depth++;
          } else {
            // Reconcile the full loaded range together, using fresh cursors for this catalog.
            setState("clips", reconcile(clips, { key: "id" }));
          }
          setState({ next: cursor, token, failure: undefined });
        });
        if (append && clips.length) return firstNew;
      } catch (error) {
        if (request === generation) setState("failure", { message: libraryError(error), operation });
      } finally {
        if (request === generation) setState("busy", false);
      }
    });
    read = result.then(() => {});
    return result;
  };

  createEffect(on(source, (next, previous) => {
    // A catalog token grants read access. The root and filters define the browse session.
    const newLibrary = !previous || next.root !== previous.root;
    const newSession = newLibrary || next.query !== previous?.query;
    if (!newSession && next.token === previous.token && next.ready === previous.ready) return;
    generation++;
    batch(() => {
      if (newSession) {
        depth = 1;
        setState({ next: null, token: undefined });
        if (newLibrary) setState("clips", []);
      }
      setState({ busy: false, failure: undefined });
      if (next.ready && !current()) void load("refresh");
    });
  }));
  onCleanup(() => { generation++; });

  const refreshTagResults = () => {
    if (!current()) return;
    // A tag-dependent query may lose or gain the item. Re-seek only the
    // loaded range, keeping stable card identities until the result is ready.
    generation++;
    setState({ busy: false, failure: undefined });
    void load("refresh");
  };
  const tagsEdited = ({ id, tags }: ClipTagUpdate) => {
    const index = state.clips.findIndex(clip => clip.id === id);
    const { query } = source();
    if (index >= 0) setState("clips", index, "tags", tags);
    if (query.search || query.tags?.length || state.busy) refreshTagResults();
  };
  const tagRenamed = (tag: Tag) => {
    batch(() => {
      for (const [index, clip] of state.clips.entries()) {
        const tags = replaceTagDefinition(clip.tags, tag);
        if (tags !== clip.tags) setState("clips", index, "tags", tags);
      }
    });
    if (source().query.search || state.busy) refreshTagResults();
  };
  return { state, current, tagsEdited, tagRenamed, append: () => load("append"), retry: () => load(state.failure?.operation ?? "refresh") };
}
